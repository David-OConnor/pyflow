use crate::{
    dep_resolution,
    dep_types::{Constraint, Req, ReqType, Version},
    files, py_versions,
};
use crossterm::{Color, Colored};
use regex::Regex;
use std::io::{self, BufRead, BufReader, Read};
use std::str::FromStr;
use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    process, thread, time,
};
use tar::Archive;
use xz2::read::XzDecoder;

/// Print in a color, then reset formatting.
pub fn print_color(message: &str, color: Color) {
    println!(
        "{}{}{}",
        Colored::Fg(color),
        message,
        Colored::Fg(Color::Reset)
    );
}

/// Used when the program should exit from a condition that may arise normally from program use,
/// like incorrect info in config files, problems with dependencies, or internet connection problems.
/// We use `expect`, `panic!` etc for problems that indicate a bug in this program.
pub fn abort(message: &str) {
    println!(
        "{}{}{}",
        Colored::Fg(Color::Red),
        message,
        Colored::Fg(Color::Reset)
    );
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
pub fn set_pythonpath(lib_path: &Path) {
    env::set_var(
        "PYTHONPATH",
        lib_path
            .to_str()
            .expect("Problem converting current path to string"),
    );
}

/// List all installed dependencies and console scripts, by examining the `libs` and `bin` folders.
pub fn show_installed(lib_path: &Path) {
    let installed = find_installed(lib_path);
    let scripts = find_console_scripts(&lib_path.join("../bin"));

    print_color("These packages are installed:", Color::DarkBlue);
    for (name, version, _tops) in installed {
        //        print_color(&format!("{} == \"{}\"", name, version.to_string()), Color::Magenta);
        println!(
            "{}{}{} == {}",
            Colored::Fg(Color::Cyan),
            name,
            Colored::Fg(Color::Reset),
            version
        );
    }

    print_color("\nThese console scripts are installed:", Color::DarkBlue);
    for script in scripts {
        print_color(&script, Color::DarkCyan);
    }
}

/// Find the packages installed, by browsing the lib folder for metadata.
/// Returns package-name, version, folder names
pub fn find_installed(lib_path: &Path) -> Vec<(String, Version, Vec<String>)> {
    let mut package_folders = vec![];

    if !lib_path.exists() {
        return vec![];
    }
    for entry in lib_path.read_dir().expect("Can't open lib path") {
        if let Ok(entry) = entry {
            if entry
                .file_type()
                .expect("Problem reading lib path file type")
                .is_dir()
            {
                package_folders.push(entry.file_name())
            }
        }
    }

    let mut result = vec![];

    for folder in package_folders.iter() {
        let folder_name = folder
            .to_str()
            .expect("Problem converting folder name to string");
        let re_dist = Regex::new(r"^(.*?)-(.*?)\.dist-info$").unwrap();

        if let Some(caps) = re_dist.captures(&folder_name) {
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
                    for line in BufReader::new(f).lines() {
                        if let Ok(l) = line {
                            tops.push(l);
                        }
                    }
                }
                Err(_) => tops.push(folder_name.to_owned()),
            }

            result.push((name.to_owned(), vers, tops));
        }
    }
    result
}

/// Find console scripts installed, by browsing the (custom) bin folder
pub fn find_console_scripts(bin_path: &Path) -> Vec<String> {
    let mut result = vec![];
    if !bin_path.exists() {
        return vec![];
    }

    for entry in bin_path.read_dir().expect("Trouble opening bin path") {
        if let Ok(entry) = entry {
            if entry.file_type().unwrap().is_file() {
                result.push(entry.file_name().to_str().unwrap().to_owned())
            }
        }
    }
    result
}

/// Handle reqs added via the CLI. Result is (normal reqs, dev reqs)
pub fn merge_reqs(
    added: &[String],
    dev: bool,
    cfg: &crate::Config,
    cfg_filename: &str,
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
    for added_req in added_reqs_unique.iter_mut() {
        if added_req.constraints.is_empty() {
            let (_, vers, _) = dep_resolution::get_version_info(&added_req.name)
                .expect("Problem getting latest version of the package you added.");
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
        for added_req in added_reqs_unique.iter() {
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
            files::add_reqs_to_cfg(cfg_filename, &[], &added_reqs_unique);
        }
        (cfg.reqs.clone(), result)
    } else {
        if !added_reqs_unique.is_empty() {
            files::add_reqs_to_cfg(cfg_filename, &added_reqs_unique, &[]);
        }
        (result, cfg.dev_reqs.clone())
    }
}

pub fn standardize_name(name: &str) -> String {
    name.to_lowercase().replace('-', "_")
}

// PyPi naming isn't consistent; it capitalization and _ vs -
pub fn compare_names(name1: &str, name2: &str) -> bool {
    standardize_name(name1) == standardize_name(name2)
}

/// Extract the wheel or zip.
/// From this example: https://github.com/mvdnes/zip-rs/blob/master/examples/extract.rs#L32
pub fn extract_zip(file: &fs::File, out_path: &Path, rename: &Option<(String, String)>) {
    // Separate function, since we use it twice.
    let mut archive = zip::ZipArchive::new(file).unwrap();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        // Change name here instead of after in case we've already installed a non-renamed version.
        // (which would be overwritten by this one.)
        let file_str2 = file.sanitized_name();
        let file_str = file_str2.to_str().expect("Problem converting path to str");

        let extracted_file = if !file_str.contains("dist-info") && !file_str.contains("egg-info") {
            match rename {
                Some((old, new)) => file
                    .sanitized_name()
                    .to_str()
                    .unwrap()
                    .to_owned()
                    .replace(old, new)
                    .into(),
                None => file.sanitized_name(),
            }
        } else {
            file.sanitized_name()
        };

        let outpath = out_path.join(extracted_file);

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
    decompressor
        .read_to_end(&mut tar)
        .expect("Problem decompressing archive");

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
pub fn find_venv_info(
    cfg_vers: &Version,
    pypackages_dir: &Path,
    pyflow_dir: &Path,
    cache_dir: &Path,
) -> (PathBuf, Version) {
    let venvs = find_venvs(&pypackages_dir);
    // The version's explicitly specified; check if an environment for that version
    let compatible_venvs: Vec<&(u32, u32)> = venvs
        .iter()
        .filter(|(ma, mi)| cfg_vers.major == *ma && cfg_vers.minor == *mi)
        .collect();

    let vers_path;
    let py_vers;
    match compatible_venvs.len() {
        0 => {
            let vers = py_versions::create_venv(&cfg_vers, pypackages_dir, pyflow_dir, cache_dir);
            vers_path = pypackages_dir.join(&format!("{}.{}", vers.major, vers.minor));
            py_vers = Version::new_short(vers.major, vers.minor); // Don't include patch.
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
            );
            unreachable!()
        }
    }

    (vers_path, py_vers)
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
    match Version::from_str(&vers) {
        Ok(v) => v,
        Err(_) => {
            abort("Problem parsing the Python version you entered. It should look like this: 3.7 or 3.7.1");
            unreachable!()
        }
    }
}

/// A generic prompt function, where the user selects from a list
pub fn prompt_list<T: Clone + ToString>(
    init_msg: &str,
    type_: &str,
    items: &[(String, T)],
    show_item: bool,
) -> (String, T) {
    print_color(init_msg, Color::Magenta);
    for (i, (name, content)) in items.iter().enumerate() {
        if show_item {
            println!("{}: {}: {}", i + 1, name, content.to_string())
        } else {
            println!("{}: {}", i + 1, name)
        }
    }

    let mut mapping = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        mapping.insert(i + 1, item);
    }

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Problem reading input");

    let input = input
        .chars()
        .next()
        .expect("Problem parsing input")
        .to_string()
        .parse::<usize>();

    let input = match input {
        Ok(ip) => ip,
        Err(_) => {
            abort("Please try again; enter a number like 1 or 2 .");
            unreachable!()
        }
    };

    let (name, content) = match mapping.get(&input) {
        Some(r) => r,
        None => {
            abort(&format!(
                "Can't find the {} associated with that number. Is it in the list above?",
                type_
            ));
            unreachable!()
        }
    };

    (name.to_string(), content.clone())
}
