pub mod deps;
pub mod paths;
pub mod prompts;

mod os;
pub use os::{get_os, Os};

#[mockall_double::double]
use crate::dep_resolution::res;

use crate::dep_resolution::WarehouseRelease;
use crate::dep_types::Extras;
use crate::{
    commands,
    dep_types::{Constraint, DependencyError, Lock, Req, ReqType, Version},
    files,
    install::{self, PackageType},
    py_versions, util, CliConfig,
};
use ini::Ini;
use regex::Regex;

use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Component;
use std::str::FromStr;
use std::{
    env,
    error::Error,
    path::{Path, PathBuf},
    process, thread, time,
};
use tar::Archive;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
use xz2::read::XzDecoder;

#[derive(Debug)]
pub struct Paths {
    pub bin: PathBuf,
    pub lib: PathBuf,
    pub entry_pt: PathBuf,
    pub cache: PathBuf,
}

/// Used to store a Wheel's metadata, from dist-info/METADATA
#[derive(Debug, Default)]
pub struct Metadata {
    pub name: String,
    pub summary: Option<String>,
    pub version: Version,
    pub author: Option<String>,
    pub author_email: Option<String>,
    pub license: Option<String>,
    pub keywords: Vec<String>,
    pub platform: Option<String>,
    pub requires_dist: Vec<Req>,
}

/// Print line in a color, then reset formatting.
pub fn print_color(message: &str, color: Color) {
    if let Err(_e) = print_color_res(message, color) {
        panic!("Error printing in color")
    }
}

fn print_color_res(message: &str, color: Color) -> io::Result<()> {
    let mut stdout = StandardStream::stdout(CliConfig::current().color_choice);
    stdout.set_color(ColorSpec::new().set_fg(Some(color)))?;
    writeln!(&mut stdout, "{}", message)?;
    stdout.reset()?;
    Ok(())
}

/// Print in a color, then reset formatting. (no newline)
pub fn print_color_(message: &str, color: Color) {
    if let Err(_e) = print_color_res_(message, color) {
        panic!("Error printing in color")
    }
}

fn print_color_res_(message: &str, color: Color) -> io::Result<()> {
    let mut stdout = StandardStream::stdout(CliConfig::current().color_choice);
    stdout.set_color(ColorSpec::new().set_fg(Some(color)))?;
    write!(&mut stdout, "{}", message)?;
    stdout.reset()?;
    Ok(())
}

/// Used when the program should exit from a condition that may arise normally from program use,
/// like incorrect info in config files, problems with dependencies, or internet connection problems.
/// We use `expect`, `panic!` etc for problems that indicate a bug in this program.
pub fn abort(message: &str) -> ! {
    print_color(message, Color::Red);
    process::exit(1)
}

/// Find which virtual environments exist.
pub fn find_venvs(pypackages_dir: &Path) -> Vec<(u32, u32)> {
    let py_versions: &[(u32, u32)] = &[
        (2, 6),
        (2, 7),
        (2, 8),
        (2, 9),
        (3, 0),
        (3, 1),
        (3, 2),
        (3, 3),
        (3, 4),
        (3, 5),
        (3, 6),
        (3, 7),
        (3, 8),
        (3, 9),
        (3, 10),
        (3, 11),
        (3, 12),
    ];

    let mut result = vec![];
    for (maj, mi) in py_versions.iter() {
        let venv_path = pypackages_dir.join(&format!("{}.{}/.venv", maj, mi));

        if venv_path.join("bin/python").exists() || venv_path.join("Scripts/python.exe").exists() {
            result.push((*maj, *mi))
        }
    }

    result
}

/// Checks whether the path is under `/bin` (Linux generally) or `/Scripts` (Windows generally)
/// Returns the bin path (ie under the venv)
pub fn find_bin_path(vers_path: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    return vers_path.join(".venv/Scripts");
    #[cfg(target_os = "linux")]
    return vers_path.join(".venv/bin");
    #[cfg(target_os = "macos")]
    return vers_path.join(".venv/bin");
}

/// Wait for directories to be created; required between modifying the filesystem,
/// and running code that depends on the new files.
pub fn wait_for_dirs(dirs: &[PathBuf]) -> Result<(), crate::py_versions::AliasError> {
    // todo: AliasError is a quick fix to avoid creating new error type.
    let timeout = 1000; // ms
    for _ in 0..timeout {
        let mut all_created = true;
        for dir in dirs {
            if !dir.exists() {
                all_created = false;
            }
        }
        if all_created {
            return Ok(());
        }
        thread::sleep(time::Duration::from_millis(10));
    }
    Err(crate::py_versions::AliasError {
        details: "Timed out attempting to create a directory".to_string(),
    })
}

/// Sets the `PYTHONPATH` environment variable, causing Python to look for
/// dependencies in `__pypackages__`,
pub fn set_pythonpath(paths: &[PathBuf]) {
    let formatted_paths = paths
        .iter()
        .map(|p| p.to_str().unwrap())
        .collect::<Vec<&str>>()
        .join(":");
    env::set_var("PYTHONPATH", formatted_paths);
}

/// Find the packages installed, by browsing the lib folder for metadata.
/// Returns package-name, version, folder names
pub fn find_installed(lib_path: &Path) -> Vec<(String, Version, Vec<String>)> {
    if !lib_path.exists() {
        return vec![];
    }

    let mut result = vec![];

    for folder_name in &find_folders(lib_path) {
        let re_dist = Regex::new(r"^(.*?)-(.*?)\.dist-info$").unwrap();

        if let Some(caps) = re_dist.captures(folder_name) {
            let name = caps.get(1).unwrap().as_str();
            let vers = Version::from_str(
                caps.get(2)
                    .expect("Problem parsing version in folder name")
                    .as_str(),
            )
            .expect("Problem parsing version in package folder");

            let top_level = lib_path.join(folder_name).join("top_level.txt");

            let mut tops = vec![];
            match fs::File::open(top_level) {
                Ok(f) => {
                    for line in BufReader::new(f).lines().flatten() {
                        tops.push(line);
                    }
                }
                Err(_) => tops.push(folder_name.to_owned()),
            }

            result.push((name.to_owned(), vers, tops));
        }
    }
    result
}
/// Handle reqs added via the CLI. Result is (normal reqs, dev reqs)
pub fn merge_reqs(
    added: &[String],
    dev: bool,
    cfg: &crate::Config,
    cfg_path: &Path,
) -> (Vec<Req>, Vec<Req>) {
    let mut added_reqs = vec![];
    for p in added.iter() {
        let trimmed = p.replace(',', "");
        match Req::from_str(&trimmed, false) {
            Ok(r) => added_reqs.push(r),
            Err(_) => abort(&format!("Unable to parse this package: {}. \
                    Note that installing a specific version via the CLI is currently unsupported. If you need to specify a version,\
                     edit `pyproject.toml`", &p)),
        }
    }

    let existing = if dev { &cfg.dev_reqs } else { &cfg.reqs };

    // Reqs to add to `pyproject.toml`
    let mut added_reqs_unique: Vec<Req> = added_reqs
        .into_iter()
        .filter(|ar| {
            // return true if the added req's not in the cfg reqs, or if it is
            // and the version's different.
            let mut add = true;

            for cr in existing.iter() {
                if cr == ar
                    || (cr.name.to_lowercase() == ar.name.to_lowercase()
                        && ar.constraints.is_empty())
                {
                    // Same req/version exists
                    add = false;
                    break;
                }
            }
            add
        })
        .collect();

    // If no constraints are specified, use a caret constraint with the latest
    // version.
    for added_req in &mut added_reqs_unique {
        if added_req.constraints.is_empty() {
            let (_, vers, _) = if let Ok(r) = res::get_version_info(
                &added_req.name,
                Some(Req::new_with_extras(
                    added_req.name.clone(),
                    vec![Constraint::new_any()],
                    Extras::new_py(Constraint::new(
                        ReqType::Exact,
                        cfg.py_version.clone().unwrap_or_else(Version::new_any),
                    )),
                )),
            ) {
                r
            } else {
                abort("Problem getting latest version of the package you added. Is it spelled correctly? Is the internet OK?")
            };

            added_req.constraints.push(Constraint::new(
                ReqType::Caret,
                //                Version::new(vers.major, vers.minor, vers.patch),
                vers,
            ));
        }
    }

    let mut result = vec![]; // Reqs to sync

    // Merge reqs from the config and added via CLI. If there's a conflict in version,
    // use the added req.
    for cr in existing.iter() {
        let mut replaced = false;
        for added_req in &added_reqs_unique {
            if compare_names(&added_req.name, &cr.name) && added_req.constraints != cr.constraints {
                result.push(added_req.clone());
                replaced = true;
                break;
            }
        }
        if !replaced {
            result.push(cr.clone());
        }
    }

    result.append(&mut added_reqs_unique.clone());

    if dev {
        if !added_reqs_unique.is_empty() {
            files::add_reqs_to_cfg(cfg_path, &[], &added_reqs_unique);
        }
        (cfg.reqs.clone(), result)
    } else {
        if !added_reqs_unique.is_empty() {
            files::add_reqs_to_cfg(cfg_path, &added_reqs_unique, &[]);
        }
        (result, cfg.dev_reqs.clone())
    }
}

pub fn standardize_name(name: &str) -> String {
    name.to_lowercase().replace('-', "_").replace('.', "_")
}

// PyPi naming isn't consistent; it capitalization and _ vs -
pub fn compare_names(name1: &str, name2: &str) -> bool {
    standardize_name(name1) == standardize_name(name2)
}

/// Extract the wheel or zip.
/// From [this example](https://github.com/mvdnes/zip-rs/blob/master/examples/extract.rs#L32)
pub fn extract_zip(
    file: &fs::File,
    out_path: &Path,
    rename: &Option<(String, String)>,
    package_names: &Option<(&str, &str)>,
) {
    // Separate function, since we use it twice.
    let mut archive = if let Ok(a) = zip::ZipArchive::new(file) {
        a
    } else {
        abort(&format!(
            "Problem reading the wheel archive: {:?}. Is it corrupted?",
            &file
        ))
    };

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        // Change name here instead of after in case we've already installed a non-renamed version.
        // (which would be overwritten by this one.)
        let entry_path = file.enclosed_name().unwrap();
        let mut final_entry_path = PathBuf::with_capacity(entry_path.as_os_str().len());
        // The `hexdump` Python package intentionally strips its own root folder from its zip source
        // distribution, which breaks wheel building. As a workaround, add the package name and version
        // as a prefix to the path when extracting if the package name isn't in the first folder's
        // name already.
        if let Some((name, filename)) = package_names {
            let stem = Path::new(filename).file_stem().unwrap();
            let components: Vec<Component> = entry_path.components().collect();
            if components.len() == 1
                || !components[0]
                    .as_os_str()
                    .to_string_lossy()
                    .starts_with(name)
            {
                final_entry_path.push(stem);
            }
        }
        final_entry_path.push(entry_path);
        let entry_path_str = final_entry_path
            .to_str()
            .expect("Problem converting path to str");

        let extracted_file =
            if !entry_path_str.contains("dist-info") && !entry_path_str.contains("egg-info") {
                match rename {
                    Some((old, new)) => {
                        PathBuf::from_str(entry_path_str.to_owned().replace(old, new).as_str())
                    }
                    None => Ok(final_entry_path),
                }
            } else {
                Ok(final_entry_path)
            };

        let outpath = out_path.join(extracted_file.unwrap());

        if (&*file.name()).ends_with('/') {
            fs::create_dir_all(&outpath).unwrap();
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(&p).unwrap();
                }
            }
            let mut outfile = fs::File::create(&outpath).unwrap();
            io::copy(&mut file, &mut outfile).unwrap();
        }

        // Get and Set permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            if let Some(mode) = file.unix_mode() {
                fs::set_permissions(&outpath, fs::Permissions::from_mode(mode)).unwrap();
            }
        }
    }
}

pub fn unpack_tar_xz(archive_path: &Path, dest: &Path) {
    let archive_bytes = fs::read(archive_path).expect("Problem reading archive as bytes");

    let mut tar: Vec<u8> = Vec::new();
    let mut decompressor = XzDecoder::new(&archive_bytes[..]);
    if decompressor.read_to_end(&mut tar).is_err() {
        abort(&format!(
            "Problem decompressing the archive: {:?}. This may be due to a failed downoad. \
        Try deleting it, then trying again. Note that Pyflow will only install officially-released \
        Python versions. If you'd like to use a pre-release, you must install it manually.",
            archive_path
        ))
    }

    // We've decompressed the .xz; now unpack the tar.
    let mut archive = Archive::new(&tar[..]);
    if archive.unpack(dest).is_err() {
        abort(&format!(
            "Problem unpacking tar: {}",
            archive_path.to_str().unwrap()
        ))
    }
}

/// Find venv info, creating a venv as required.
pub fn find_or_create_venv(
    cfg_vers: &Version,
    pypackages_dir: &Path,
    pyflow_dir: &Path,
    dep_cache_path: &Path,
) -> (PathBuf, Version) {
    let venvs = find_venvs(pypackages_dir);
    // The version's explicitly specified; check if an environment for that version
    let compatible_venvs: Vec<&(u32, u32)> = venvs
        .iter()
        .filter(|(ma, mi)| cfg_vers.major == Some(*ma) && cfg_vers.minor == Some(*mi))
        .collect();

    let vers_path;
    let py_vers;
    match compatible_venvs.len() {
        0 => {
            let vers =
                py_versions::create_venv(cfg_vers, pypackages_dir, pyflow_dir, dep_cache_path);
            vers_path = pypackages_dir.join(vers.to_string_med());
            py_vers = Version::new_opt(vers.major, vers.minor, None); // Don't include patch.
        }
        1 => {
            vers_path = pypackages_dir.join(&format!(
                "{}.{}",
                compatible_venvs[0].0, compatible_venvs[0].1
            ));
            py_vers = Version::new_short(compatible_venvs[0].0, compatible_venvs[0].1);
        }
        _ => {
            abort(
                // todo: Handle this, eg by letting the user pick the one to use?
                "Multiple compatible Python environments found
                for this project.",
            )
        }
    }

    #[cfg(target_os = "windows")]
    {
        (vers_path, py_vers)
    }

    #[cfg(target_os = "linux")]
    {
        let vers_path = fs::canonicalize(vers_path);
        let vers_path = match vers_path {
            Ok(path) => path,
            Err(error) => {
                abort(&format!(
                    "Problem converting path to absolute path: {:?}",
                    error
                ));
                unreachable!()
            }
        };
        (vers_path, py_vers)
    }

    #[cfg(target_os = "macos")]
    {
        let vers_path = fs::canonicalize(vers_path);
        let vers_path = match vers_path {
            Ok(path) => path,
            Err(error) => abort(&format!(
                "Problem converting path to absolute path: {:?}",
                error
            )),
        };
        (vers_path, py_vers)
    }
}

///// Remove all files (but not folders) in a path.
//pub fn wipe_dir(path: &Path) {
//    if !path.exists() {
//        fs::create_dir(&path).expect("Problem creating directory");
//    }
//    for entry in fs::read_dir(&path).expect("Problem reading path") {
//        if let Ok(entry) = entry {
//            let path2 = entry.path();
//
//            if path2.is_file() {
//                fs::remove_file(path2).expect("Problem removing a file");
//            }
//        };
//    }
//}

/// Used when the version might be an error, eg user input
pub fn fallible_v_parse(vers: &str) -> Version {
    let vers = vers.replace(" ", "").replace("\n", "").replace("\r", "");
    if let Ok(v) = Version::from_str(&vers) {
        v
    } else {
        abort("Problem parsing the Python version you entered. It should look like this: 3.7 or 3.7.1")
    }
}

/// Find the operating system from a wheel filename. This doesn't appear to be available
/// anywhere else on the Pypi Warehouse.
fn os_from_wheel_fname(filename: &str) -> Result<Os, DependencyError> {
    // Format is "name-version-pythonversion-mobileversion?-os.whl"
    // Also works with formats like this:
    // `PyQt5-5.13.0-5.13.0-cp35.cp36.cp37.cp38-none-win32.whl` too.
    // The point is, pull the last part before ".whl".
    let re = Regex::new(r"^(?:.*?-)+(.*).whl$").unwrap();
    if let Some(caps) = re.captures(filename) {
        let parsed = caps.get(1).unwrap().as_str();
        return Ok(
            Os::from_str(parsed).unwrap_or_else(|_| panic!("Problem parsing Os: {}", parsed))
        );
    }

    Err(DependencyError::new("Problem parsing os from wheel name"))
}

/// Find the most appropriate release to download. Ie Windows vs Linux, wheel vs source.
pub fn find_best_release(
    data: &[WarehouseRelease],
    name: &str,
    version: &Version,
    os: Os,
    python_vers: &Version,
) -> (WarehouseRelease, PackageType) {
    // Find which release we should download. Preferably wheels, and if so, for the right OS and
    // Python version.
    let mut compatible_releases = vec![];
    // Store source releases as a fallback, for if no wheels are found.
    let mut source_releases = vec![];

    for rel in data.iter() {
        let mut compatible = true;
        match rel.packagetype.as_ref() {
            "bdist_wheel" => {
                // Now determine if this wheel is appropriate for the Os and Python version.
                if let Some(py_ver) = &rel.requires_python {
                    // If a version constraint exists, make sure it's compatible.
                    let py_constrs = Constraint::from_str_multiple(py_ver)
                        .expect("Problem parsing constraint from requires_python");

                    for constr in &py_constrs {
                        if !constr.is_compatible(python_vers) {
                            compatible = false;
                        }
                    }
                }

                let wheel_os =
                    os_from_wheel_fname(&rel.filename).expect("Problem getting os from wheel name");
                if wheel_os != os && wheel_os != Os::Any {
                    compatible = false;
                }

                // Packages that use C code(eg numpy) may fail to load C extensions if installing
                // for the wrong version of python (eg  cp35 when python 3.7 is installed), even
                // if `requires_python` doesn't indicate an incompatibility. Check `python_version`
                // instead of `requires_python`.
                // Note that the result of this parse is an any match.
                if let Ok(constrs) = Constraint::from_wh_py_vers(&rel.python_version) {
                    let mut compat_py_v = false;
                    for constr in &constrs {
                        if constr.is_compatible(python_vers) {
                            compat_py_v = true;
                        }
                    }
                    if !compat_py_v {
                        compatible = false;
                    }
                } else {
                    println!(
                        "Unable to match python version from python_version: {}",
                        &rel.python_version
                    )
                };

                if compatible {
                    compatible_releases.push(rel.clone());
                }
            }
            "sdist" => source_releases.push(rel.clone()),
            "bdist_wininst" | "bdist_msi" | "bdist_egg" => (), // Don't execute Windows installers
            _ => {
                println!("Found surprising package type: {}", rel.packagetype);
                continue;
            }
        }
    }

    let best_release;
    let package_type;
    // todo: Sort further / try to match exact python_version if able.
    if compatible_releases.is_empty() {
        if source_releases.is_empty() {
            abort(&format!(
                "Unable to find a compatible release for {}: {}",
                name,
                version.to_string_color()
            ))
        } else {
            best_release = source_releases[0].clone();
            package_type = install::PackageType::Source;
        }
    } else {
        best_release = compatible_releases[0].clone();
        package_type = install::PackageType::Wheel;
    }

    (best_release, package_type)
}

/// Find the global git config's user and email, and format it to go in the config's `authors` field.
pub fn get_git_author() -> Vec<String> {
    let gitcfg = directories::BaseDirs::new()
        .unwrap()
        .home_dir()
        .join(".gitconfig");

    if !gitcfg.exists() {
        return vec![];
    }

    // Load the gitconfig file and read the [user] values.
    let conf = Ini::load_from_file(gitcfg).expect("Could not read ~/.gitconfig");
    let user = conf.section(Some("user".to_owned()));
    if let Some(user) = user {
        let name: String = user.get("name").unwrap_or(&String::from("")).to_string();
        let email: String = user.get("email").unwrap_or(&String::from("")).to_string();
        vec![format!("{} <{}>", name, email)]
    } else {
        vec![]
    }
}

pub fn find_first_file(path: &Path) -> PathBuf {
    // todo: Propagate errors rather than abort here?
    {
        // There should only be one file in this dist folder: The wheel we're looking for.
        for entry in path
            .read_dir()
            .expect("Trouble reading the directory when finding the first file.")
            .flatten()
        {
            if entry.file_type().unwrap().is_file() {
                return entry.path();
            }
        }
        abort(&format!(
            "Problem the first file in the directory: {:?}",
            path
        ))
    };
}

/// Mainly to avoid repeating error-handling code.
pub fn open_archive(path: &Path) -> fs::File {
    // We must re-open the file after computing the hash.
    if let Ok(f) = fs::File::open(&path) {
        f
    } else {
        abort(&format!(
            "Problem opening the archive file: {:?}. Was there a problem while
        downloading it?",
            &path
        ))
    }
}

/// Parse a wheel's `METADATA` file.
pub fn parse_metadata(path: &Path) -> Metadata {
    let re = |key: &str| Regex::new(&format!(r"^{}:\s*(.*)$", key)).unwrap();

    let mut result = Metadata::default();

    let data = fs::read_to_string(path).expect("Problem reading METADATA");
    for line in data.lines() {
        if let Some(caps) = re("Version").captures(line) {
            let val = caps.get(1).unwrap().as_str();
            result.version =
                Version::from_str(val).expect("Problem parsing version from `METADATA`");
        }
        if let Some(caps) = re("Requires-Dist").captures(line) {
            let val = caps.get(1).unwrap().as_str();
            let req =
                Req::from_str(val, true).expect("Problem parsing requirement from `METADATA`");
            result.requires_dist.push(req);
        }
    }
    // todo: For now, just pull version and requires_dist. Add more as-required.
    result
}

pub fn find_folders(path: &Path) -> Vec<String> {
    let mut result = vec![];
    for entry in path.read_dir().expect("Can't open lib path").flatten() {
        if entry
            .file_type()
            .expect("Problem reading lib path file type")
            .is_dir()
        {
            result.push(
                entry
                    .file_name()
                    .to_str()
                    .expect("Problem converting folder name to string")
                    .to_owned(),
            );
        }
    }
    result
}

fn default_python() -> Version {
    match commands::find_py_version("python") {
        Some(x) => x,
        None => Version::new_short(3, 9),
    }
}

/// We've removed the git repos from packages to install form pypi, but make
/// sure we flag them as not-to-uninstall.
pub fn find_dont_uninstall(reqs: &[Req], dev_reqs: &[Req]) -> Vec<String> {
    let mut result: Vec<String> = reqs
        .iter()
        .filter_map(|r| {
            if r.git.is_some() || r.path.is_some() {
                Some(r.name.to_owned())
            } else {
                None
            }
        })
        .collect();

    for r in dev_reqs {
        if r.git.is_some() || r.path.is_some() {
            result.push(r.name.to_owned());
        }
    }

    result
}

// Internal function to handle error reporting for commands.
//
// Panics on subprocess failure printing error message
pub(crate) fn check_command_output(output: &process::Output, msg: &str) {
    check_command_output_with(output, |s| panic!("{}: {}", msg, s));
}

// Internal function to handle error reporting for commands.
//
// Panics on subprocess failure printing error message
pub(crate) fn check_command_output_with(output: &process::Output, f: impl Fn(&str)) {
    if !output.status.success() {
        let stderr =
            std::str::from_utf8(&output.stderr).expect("building string from command output");
        f(stderr)
    }
}

/// Take the canonicalized `path` and join `extend` onto it
pub fn canon_join(path: &Path, extend: &str) -> PathBuf {
    let ex_path = Path::new(extend);
    let canon = match ex_path.canonicalize() {
        Ok(c) => c,
        Err(e) => abort(&format!("{}\n\"{}\"", e, extend)),
    };
    let mut new_path = path.to_path_buf();

    for comp in canon.components() {
        new_path = match comp {
            std::path::Component::Normal(c) => new_path.join(c),
            _ => new_path.join(""),
        }
    }
    new_path
}

/// Install git requirements and collect their downstream dependencies.
///
/// The git requirements are removed from the `reqs` vector, and are replaced
/// by all their downstream requirements.
pub fn process_reqs(reqs: Vec<Req>, git_path: &Path, paths: &util::Paths) -> Vec<Req> {
    // git_reqs is used to store requirements from packages installed via git.
    let mut git_reqs = vec![]; // For path reqs too.
    for req in reqs.iter().filter(|r| r.git.is_some()) {
        // todo: as_ref() would be better than clone, if we can get it working.
        let mut metadata = install::download_and_install_git(
            &req.name,
            //  util::GitPath::Git(req.git.clone().unwrap()),
            &req.git.clone().unwrap(),
            git_path,
            paths,
        );
        git_reqs.append(&mut metadata.requires_dist);
    }
    // We don't pass the git requirement itself, since we've directly installed it,
    // but we do pass its requirements.
    let mut updated_reqs: Vec<Req> = reqs
        .into_iter()
        .filter(|r| r.git.is_none() && r.path.is_none())
        .collect();
    for r in git_reqs {
        updated_reqs.push(r);
    }
    updated_reqs
}

/// Read dependency data from a lock file.
pub fn read_lock(path: &Path) -> Result<Lock, Box<dyn Error>> {
    let data = fs::read_to_string(path)?;
    Ok(toml::from_str(&data)?)
}

/// Write dependency data to a lock file.
pub fn write_lock(path: &Path, data: &Lock) -> Result<(), Box<dyn Error>> {
    let data = toml::to_string(data)?;
    fs::write(path, data)?;
    Ok(())
}

pub fn handle_color_option(s: &str) -> ColorChoice {
    match s {
        "always" => ColorChoice::Always,
        "ansi" => ColorChoice::AlwaysAnsi,
        "auto" => {
            if atty::is(atty::Stream::Stdout) {
                ColorChoice::Auto
            } else {
                ColorChoice::Never
            }
        }
        _ => ColorChoice::Never,
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::dep_types;

    #[test]
    fn dummy_test() {}

    #[rstest(
        input,
        expected,
        case("manylinux1_i686", Ok(Os::Linux32)),
        case("manylinux2010_i686", Ok(Os::Linux32)),
        case("manylinux2014_i686", Ok(Os::Linux32)),
        case("cygwin", Ok(Os::Linux)),
        case("linux", Ok(Os::Linux)),
        case("linux2", Ok(Os::Linux)),
        case("manylinux1_x86_64", Ok(Os::Linux)),
        case("manylinux2010_x86_64", Ok(Os::Linux)),
        case("manylinux2014_aarch64", Ok(Os::Linux)),
        case("manylinux2014_ppc64le", Ok(Os::Linux)),
        case("manylinux2014_x86_64", Ok(Os::Linux)),
        case("win32", Ok(Os::Windows32)),
        case("windows", Ok(Os::Windows)),
        case("win", Ok(Os::Windows)),
        case("win_amd64", Ok(Os::Windows)),
        case("macosx_10_6_intel", Ok(Os::Mac)),
        case("darwin", Ok(Os::Mac)),
        case("openbsd6", Ok(Os::Linux)),
        case("any", Ok(Os::Any)),
        case("some other bsd name", Ok(Os::Linux)),
        case("some other mac name", Ok(Os::Mac))
    )]
    fn test_os_from_str(input: &str, expected: Result<Os, dep_types::DependencyError>) {
        assert_eq!(Os::from_str(input), expected);
    }
}
