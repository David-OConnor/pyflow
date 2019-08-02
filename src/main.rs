use crate::dep_types::{Constraint, DepNode, Lock, LockPackage, Package, Req, Version};
use crate::util::abort;
use regex::Regex;
use serde::Deserialize;
use std::{
    collections::HashMap,
    env,
    error::Error,
    fmt, fs,
    io::{self, BufRead, BufReader},
    path::PathBuf,
    str::FromStr,
};
use structopt::StructOpt;
use termion::{color, style};

mod build;
mod commands;
mod dep_resolution;
mod dep_types;
mod edit_files;
mod util;

//type CleanedDeps = HashMap<String, (u32, Vec<(Version, Version)>)>;

#[derive(StructOpt, Debug)]
#[structopt(name = "Pypackage", about = "Python packaging and publishing")]
struct Opt {
    #[structopt(subcommand)]
    subcmds: Option<SubCommand>,
    #[structopt(name = "custom_bin")]
    //    custom_bin: Vec<String>,
    custom_bin: Vec<String>,
}

///// eg `ipython`, `black` etc.
//#[derive(StructOpt, Debug)]
//struct CustomBin {
//    test: bool,
////    #[structopt(name = "name")]
////    name: String,
////    #[structopt(name = "args")]
////    args: Vec<String>,
//}

#[derive(StructOpt, Debug)]
enum SubCommand {
    /// Create a project folder with the basics
    #[structopt(name = "new")]
    New {
        #[structopt(name = "name")]
        name: String, // holds the project name.
    },

    /// Install packages from `pyproject.toml`, or ones specified
    #[structopt(
        name = "install",
        help = "
Install packages from `pyproject.toml`, `pypackage.lock`, or speficied ones. Example:

`pypackage install`: sync your installation with `pyproject.toml`, or `pypackage.lock` if it exists.
`pypackage install numpy scipy`: install `numpy` and `scipy`.
"
    )]
    Install {
        #[structopt(name = "packages")]
        packages: Vec<String>,
        #[structopt(short = "b", long = "binary")]
        bin: bool,
    },
    /// Uninstall all packages, or ones specified
    #[structopt(name = "uninstall")]
    Uninstall {
        #[structopt(name = "packages")]
        packages: Vec<String>,
    },
    /// Run python
    #[structopt(name = "python")]
    Python {
        #[structopt(name = "args")]
        args: Vec<String>,
    },
    /// Build the package, wrapping `setuptools`
    #[structopt(name = "package")]
    Package,
    /// Publish to `pypi`
    #[structopt(name = "publish")]
    Publish,
    /// Create a `pyproject.toml` from requirements.txt, pipfile etc, setup.py etc
    #[structopt(name = "init")]
    Init,
}

/// A config, parsed from pyproject.toml
#[derive(Clone, Debug, Default, Deserialize)]
// todo: Auto-desr some of these!
pub struct Config {
    py_version: Option<Version>,
    dependencies: Vec<Req>, // name, requirements.
    name: Option<String>,
    version: Option<Version>,
    author: Option<String>,
    author_email: Option<String>,
    description: Option<String>,
    classifiers: Vec<String>, // https://pypi.org/classifiers/
    keywords: Vec<String>,
    homepage: Option<String>,
    repo_url: Option<String>,
    package_url: Option<String>,
    readme_filename: Option<String>,
    license: Option<String>,
}

fn key_re(key: &str) -> Regex {
    Regex::new(&format!(r#"^{}\s*=\s*"(.*)"$"#, key)).unwrap()
}

impl Config {
    /// Pull config data from `pyproject.toml`
    fn from_file(filename: &str) -> Option<Self> {
        // We don't use the `toml` crate here because it doesn't appear flexible enough.
        let mut result = Config::default();
        let file = match fs::File::open(filename) {
            Ok(f) => f,
            Err(_) => return None,
        };

        let mut in_sect = false;
        let mut in_dep = false;

        let sect_re = Regex::new(r"\[.*\]").unwrap();

        for line in BufReader::new(file).lines() {
            if let Ok(l) = line {
                // todo replace this with something that clips off
                // todo post-# part of strings; not just ignores ones starting with #
                if l.starts_with('#') {
                    continue;
                }

                if &l == "[tool.pypackage]" {
                    in_sect = true;
                    in_dep = false;
                    continue;
                } else if &l == "[tool.pypackage.dependencies]" {
                    in_sect = false;
                    in_dep = true;
                    continue;
                } else if sect_re.is_match(&l) {
                    in_sect = false;
                    in_dep = false;
                    continue;
                }

                if in_sect {
                    // todo DRY
                    if let Some(n2) = key_re("name").captures(&l) {
                        if let Some(n) = n2.get(1) {
                            result.name = Some(n.as_str().to_string());
                        }
                    }
                    if let Some(n2) = key_re("description").captures(&l) {
                        if let Some(n) = n2.get(1) {
                            result.description = Some(n.as_str().to_string());
                        }
                    }
                    if let Some(n2) = key_re("version").captures(&l) {
                        if let Some(n) = n2.get(1) {
                            let n3 = n.as_str();
                            if !n3.is_empty() {
                                result.version = Some(Version::from_str(n3).unwrap());
                            }
                        }
                    }
                    if let Some(n2) = key_re("py_version").captures(&l) {
                        if let Some(n) = n2.get(1) {
                            let n3 = n.as_str();
                            if !n3.is_empty() {
                                result.py_version = Some(Version::from_str(n.as_str()).unwrap());
                            }
                        }
                    }
                } else if in_dep {
                    if !l.is_empty() {
                        result.dependencies.push(Req::from_str(&l, false).unwrap());
                    }
                }
            }
        }

        Some(result)
    }

    /// Create a new `pyproject.toml` file.
    fn write_file(&self, filename: &str) {
        let file = PathBuf::from(filename);
        if file.exists() {
            abort("`pyproject.toml` already exists")
        }

        // todo: Use a bufer instead of String?
        let mut result = String::new();

        result.push_str("[tool.pypackage]\n");
        if let Some(name) = &self.name {
            result.push_str(&("name = \"".to_owned() + name + "\"\n"));
        } else {
            // Give name, and a few other fields default values.
            result.push_str(&("name = \"\"".to_owned() + "\n"));
        }
        if let Some(py_v) = self.py_version {
            result.push_str(&("version = \"".to_owned() + &py_v.to_string() + "\"\n"));
        } else {
            result.push_str(&("version = \"\"".to_owned() + "\n"));
        }
        if let Some(vers) = self.version {
            result.push_str(&(vers.to_string() + "\n"));
        }
        if let Some(author) = &self.author {
            result.push_str(&(author.to_owned() + "\n"));
        }

        result.push_str("\n\n");
        result.push_str("[tool.pypackage.dependencies]\n");
        for dep in self.dependencies.iter() {
            result.push_str(&(dep.to_cfg_string() + "\n"));
        }

        println!("FILE: {:?}", file);
        match fs::write(file, result) {
            Ok(_) => println!("Created `pyproject.toml`"),
            Err(_) => abort("Problem writing `pyproject.toml`"),
        }
    }
}

/// Create a template directory for a python project.
pub(crate) fn new(name: &str) -> Result<(), Box<Error>> {
    if !PathBuf::from(name).exists() {
        fs::create_dir_all(&format!("{}/{}", name, name))?;
        fs::File::create(&format!("{}/{}/main.py", name, name))?;
        fs::File::create(&format!("{}/README.md", name))?;
        fs::File::create(&format!("{}/LICENSE", name))?;
        fs::File::create(&format!("{}/pyproject.toml", name))?;
        fs::File::create(&format!("{}/.gitignore", name))?;
    }

    let gitignore_init = r##"# General Python ignores

build/
dist/
__pycache__/
.ipynb_checkpoints/
*.pyc
*~
*/.mypy_cache/


# Project ignores
"##;

    let pyproject_init = &format!(
        r##"[tool.pypackage]
name = "{}"
py_version = "3.7"
version = "0.1.0"
description = ""
author = ""

pyackage_url = "https://test.pypi.org"
# pyackage_url = "https://pypi.org"

[tool.pypackage.dependencies]
"##,
        name
    );

    // todo: flesh readme out
    let readme_init = &format!("# {}", name);

    fs::write(&format!("{}/.gitignore", name), gitignore_init)?;
    fs::write(&format!("{}/pyproject.toml", name), pyproject_init)?;
    fs::write(&format!("{}/README.md", name), readme_init)?;

    Ok(())
}

/// Prompt which Python alias to use, if multiple are found.
fn prompt_alias(aliases: &[(String, Version)]) -> (String, Version) {
    // Todo: Overall, the API here is inelegant.
    println!("Found multiple Python aliases. Please enter the number associated with the one you'd like to use for this project:");
    for (i, (alias, version)) in aliases.iter().enumerate() {
        println!("{}: {} version: {}", i + 1, alias, version.to_string())
    }

    let mut mapping = HashMap::new();
    for (i, alias) in aliases.iter().enumerate() {
        mapping.insert(i + 1, alias);
    }

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Unable to read user input for version");

    let input = input
        .chars()
        .next()
        .expect("Problem reading input")
        .to_string();

    let (alias, version) = mapping
        .get(
            &input
                .parse::<usize>()
                .expect("Enter the number associated with the Python alias."),
        )
        .expect(
            "Can't find the Python alias associated with that number. Is it in the list above?",
        );
    (alias.to_string(), *version)
}

#[derive(Debug)]
pub struct AliasError {
    details: String,
}

impl Error for AliasError {
    fn description(&self) -> &str {
        &self.details
    }
}

impl fmt::Display for AliasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

/// Make an educated guess at the command needed to execute python the
/// current system.  An alternative approach is trying to find python
/// installations.
fn find_py_alias() -> Result<(String, Version), AliasError> {
    let possible_aliases = &[
        "python3.10",
        "python3.9",
        "python3.8",
        "python3.7",
        "python3.6",
        "python3.5",
        "python3.4",
        "python3.3",
        "python3.2",
        "python3.1",
        "python3",
        "python",
        "python2",
    ];

    let mut found_aliases = Vec::new();

    for alias in possible_aliases {
        // We use the --version command as a quick+effective way to determine if
        // this command is associated with Python.
        if let Some(v) = commands::find_py_version(alias) {
            found_aliases.push((alias.to_string(), v));
        }
    }

    match possible_aliases.len() {
        0 => Err(AliasError {
            details: "Can't find Python on the path.".into(),
        }),
        1 => Ok(found_aliases[0].clone()),
        _ => Ok(prompt_alias(&found_aliases)),
    }
}

/// Read dependency data from a lock file.
fn read_lock(filename: &str) -> Result<(Lock), Box<Error>> {
    let data = fs::read_to_string(filename)?;
    //    let t: Lock = toml::from_str(&data).unwrap();
    Ok(toml::from_str(&data)?)
}

/// Write dependency data to a lock file.
fn write_lock(filename: &str, data: &Lock) -> Result<(), Box<Error>> {
    let data = toml::to_string(data)?;
    //    let f = fs::File::open(filename).expect("cannot open pypackage.lock");

    // Wipe the existing data
    //    f.set_len(0).unwrap();
    //    fs::remove_file(filename).unwrap();

    fs::write(filename, data)?;
    Ok(())
}

fn create_venv(cfg_v: Option<&Version>, pyypackage_dir: &PathBuf) -> Version {
    // We only use the alias for creating the virtual environment. After that,
    // we call our venv's executable directly.

    // todo perhaps move alias finding back into create_venv, or make a
    // todo create_venv_if_doesnt_exist fn.
    let (alias, py_ver_from_alias) = match find_py_alias() {
        Ok(a) => a,
        Err(_) => {
            abort("Unable to find a Python version on the path");
            ("".to_string(), Version::new_short(0, 0)) // Required for compiler
        }
    };

    let lib_path = pyypackage_dir.join(format!(
        "{}.{}/lib",
        py_ver_from_alias.major, py_ver_from_alias.minor
    ));
    if !lib_path.exists() {
        fs::create_dir_all(&lib_path).expect("Problem creating __pypackages__ directory");
    }

    if let Some(c_v) = cfg_v {
        // We don't expect the config version to specify a patch, but if it does, take it
        // into account.
        if c_v != &py_ver_from_alias {
            println!("{:?}, {:?}", c_v, &py_ver_from_alias);
            abort(&format!("The Python version you selected ({}) doesn't match the one specified in `pyprojecttoml` ({})",
                           py_ver_from_alias.to_string(), c_v.to_string())
            );
        }
    }

    println!("Setting up Python environment...");

    if commands::create_venv(&alias, &lib_path, ".venv").is_err() {
        util::abort("Problem creating virtual environment");
    }

    // Wait until the venv's created before continuing, or we'll get errors
    // when attempting to use it
    // todo: These won't work with Scripts ! - pass venv_path et cinstead
    let py_venv = lib_path.join("../.venv/bin/python");
    let pip_venv = lib_path.join("../.venv/bin/pip");
    util::wait_for_dirs(&[py_venv, pip_venv]).unwrap();

    py_ver_from_alias
}

/// Remove duplicates. If the requirements are different but compatible, pick the more-restrictive
/// one. If incompatible, return Err.
/// todo: We'll have to think about how to resolve the errors, likely elsewhere.
//fn clean_flattened_deps(deps: &[(u32, DepNode)]) -> Result<CleanedDeps, DependencyError> {
//    // result is a (min, max) tuple
//    let mut result: HashMap<String, (u32, Vec<(Version, Version)>)> = HashMap::new(); // todo remove annotation
//
//    for (level, dep) in deps.iter() {
//        match result.get(&dep.name) {
//            Some(reqs) => {
//                result.insert(
//                    dep.name.to_owned(),
//                    (
//                        cmp::max(*level, reqs.0),
//                        dep_types::intersection_convert_one(&dep.version_reqs, &reqs.1),
//                    ),
//                );
//            }
//            None => {
//                // Not already present; without any checks.
//                result.insert(
//                    dep.name.to_owned(),
//                    (*level, dep_types::to_ranges(&dep.version_reqs.reqs)),
//                );
//            }
//        }
//    }
//
//    Ok(result)
//}

/// Extract dependencies from a nested hierarchy. Ultimately, we can only (practially) have
/// one copy per dep. Record what level they came from.
/// // todo: Is there a clever way to have multiple versions of a dep installed???
fn flatten_deps(result: &mut Vec<(u32, DepNode)>, level: u32, tree: &DepNode) {
    for node in tree.dependencies.iter() {
        // We don't need sub-deps in the result; they're extraneous info. We only really care about
        // the name and version requirements.
        let mut result_dep = node.clone();
        result_dep.dependencies = vec![];
        result.push((level, result_dep));
        flatten_deps(result, level + 1, &node);
    }
}

/// Find teh packages installed, by browsing the lib folder.
fn find_installed(lib_path: &PathBuf) -> Vec<(String, Version)> {
    // todo: More functional?
    let mut package_folders = vec![];
    for entry in lib_path.read_dir().unwrap() {
        if let Ok(entry) = entry {
            if entry.file_type().unwrap().is_dir() {
                package_folders.push(entry.file_name())
            }
        }
    }

    let mut result = vec![];

    for folder in package_folders.iter() {
        let folder_name = folder.to_str().unwrap();
        let re = Regex::new(r"^(.*?)-(.*?)\.dist-info$").unwrap();
        let re_egg = Regex::new(r"^(.*?)-(.*?)\.egg-info$").unwrap();

        if let Some(caps) = re.captures(&folder_name) {
            let name = caps.get(1).unwrap().as_str();
            let vers = Version::from_str(caps.get(2).unwrap().as_str()).unwrap();
            result.push((name.to_owned(), vers));

        // todo dry
        } else if let Some(caps) = re_egg.captures(&folder_name) {
            let name = caps.get(1).unwrap().as_str();
            let vers = Version::from_str(caps.get(2).unwrap().as_str()).unwrap();
            result.push((name.to_owned(), vers));
        }
    }
    result
}

fn download_and_install_package(
    url: &str,
    filename: &str,
    hash: &str,
    lib_path: &PathBuf,
    bin: bool,
) -> Result<(), reqwest::Error> {
    // todo: Md5 isn't secure! sha256 instead?
    let mut resp = reqwest::get(url)?;
    let mut out =
        fs::File::create(lib_path.join(filename)).expect("Failed to save downloaded package file");

    io::copy(&mut resp, &mut out).expect("failed to copy content");

    // todo: Impl hash.

    Ok(())
}

/// Uninstall and install packages to be in accordance with the lock.
fn sync_packages_with_lock(
    bin_path: &PathBuf,
    lib_path: &PathBuf,
    lock_packs: &Vec<LockPackage>,
    installed: &Vec<(String, Version)>,
) {
    // Uninstall packages no longer needed.
    for (name_ins, vers_ins) in installed.iter() {
        if !lock_packs
            .iter()
            .map(|lp| {
                (
                    lp.name.to_owned().to_lowercase(),
                    Version::from_str(&lp.version).unwrap(),
                )
            })
            .collect::<Vec<(String, Version)>>()
            .contains(&(name_ins.to_owned().to_lowercase(), *vers_ins))
            || name_ins.to_lowercase() == "twine"
            || name_ins.to_lowercase() == "setuptools"
            || name_ins.to_lowercase() == "setuptools"
        {
            println!("Uninstalling {}: {}", name_ins, vers_ins.to_string());
            // Uninstall the package
            // package folders appear to be lowercase, while metadata keeps the package title's casing.
            if fs::remove_dir_all(lib_path.join(name_ins.to_lowercase())).is_err() {
                println!(
                    "{}Problem uninstalling {} {}{}",
                    color::Fg(color::LightRed),
                    name_ins,
                    vers_ins.to_string(),
                    style::Reset
                )
            }

            // Only report error if both dist-info and egg-info removal fail.
            let mut meta_folder_removed = false;
            if fs::remove_dir_all(lib_path.join(format!(
                "{}-{}.dist-info",
                name_ins,
                vers_ins.to_string()
            )))
            .is_ok()
            {
                meta_folder_removed = true;
            }
            if fs::remove_dir_all(lib_path.join(format!(
                "{}-{}.egg-info",
                name_ins,
                vers_ins.to_string()
            )))
            .is_ok()
            {
                meta_folder_removed = true;
            }
            if !meta_folder_removed {
                println!(
                    "{}Problem uninstalling metadata for {}: {}{}",
                    color::Fg(color::LightRed),
                    name_ins,
                    vers_ins.to_string(),
                    style::Reset,
                )
            }
        }
    }

    for lock_pack in lock_packs {
        let p = Package::from_lock_pack(lock_pack);
        if installed
            .iter()
            // Set both names to lowercase to ensure case doesn't preclude a match.
            .map(|(p_name, p_vers)| (p_name.clone().to_lowercase(), *p_vers))
            .collect::<Vec<(String, Version)>>()
            .contains(&(p.name.clone().to_lowercase(), p.version))
        {
            continue; // Already installed.
        }

        // path_to_info is the path to the metadatafolder, ie dist-info (or egg-info for older packages).
        // todo: egg-info
        // when making the path, use the LockPackage vice p, since its version's already serialized.
        //        let path_to_dep = lib_path.join(&lock_pack.name);
        //        let path_to_info = lib_path.join(format!(
        //            "{}-{}.dist-info",
        //            lock_pack.name, lock_pack.version
        //        ));

        //        if commands::install(&bin_path, &[p], false, false).is_err() {
        //            abort("Problem installing packages");
        //        }
        //        download_and_install_package(p.file_url, p.filename, p.hash_, lib_path, false);
    }
}

/// Install/uninstall deps as required from the passed list, and re-write the lock file.
fn sync_deps(
    lock_filename: &str,
    bin_path: &PathBuf,
    lib_path: &PathBuf,
    reqs: &mut Vec<Req>,
    installed: &Vec<(String, Version)>,
) {
    println!("Resolving dependencies...");
    // Recursively add sub-dependencies.
    let mut tree = DepNode {
        // dummy parent
        name: String::from("root"),
        version: Version::new(0, 0, 0),
        reqs: reqs.clone(), // todo clone?
        dependencies: vec![],
        constraints_for_this: vec![],
        //        filename: String::new(),
        //        hash: String::new(),
        //        file_url: String::new(),
    };

    let resolved = match dep_resolution::resolve(&mut tree) {
        Ok(r) => r,
        Err(_) => {
            abort("Problem resolving dependencies");
            vec![] // todo find proper way to equlaize mathc arms.
        }
    };

    //    println!("RESOLVED: {:#?}", &resolved);
    //    let mut to_install = vec![];
    for dep in resolved {
        let data = dep_resolution::get_warehouse_release(&dep.name, &dep.version)
            .expect("Problem getting warehouse data");

        // todo: Pick the correct release.
        let release = &data[0];

        //        let packge = Package {
        //            name: dep.name.clone(),
        //            version: dep.version.clone(),
        //            deps: vec![], // todo: I think we may have purged these.fix
        //            source: None,  // todo
        //            filename: release.filename,
        //            file_url: release.url,
        //            hash: release.md5_digest,
        //        };
        println!(
            "Downloading {} = \"{}\"",
            &dep.name,
            &dep.version.to_string()
        );
        // todo: Make download-and_install accept a package instead of sep args?
        if download_and_install_package(
            &release.url,
            &release.filename,
            &release.md5_digest,
            lib_path,
            false,
        )
        .is_err()
        {
            abort("Problem downloading packages");
        }
    }

    // todo big DRY from dep_resolution
    // todo: And you're making redundant warehouse calls to populate versions/find the best.. Fix this by caching.
    //    for (name, (level, req)) in cleaned {
    //        let versions = dep_resolution::get_warehouse_versions(&name).unwrap();
    //        let compatible_versions = dep_resolution::filter_compatible2(&req, versions);
    //
    //        let newest_compat = compatible_versions.into_iter().max().unwrap();
    //
    //        lock_packs.push(LockPackage {
    //            name: name.to_owned(),
    //            version: newest_compat.to_string(),
    //            source: None,       // todo
    //            dependencies: None, // todo??
    //        });
    //    }

    // todo: Sort by level (deeper gets installed first) before discarding level info.

    let lock_packs = vec![];
    let lock = Lock {
        metadata: None, // todo
        package: Some(lock_packs),
    };

    // Now that the deps are resolved, flattened, cleaned, and only have one per package name, we can
    // pick the best match of each, download, and lock.

    if let Some(lock_packs) = &lock.package {
        sync_packages_with_lock(bin_path, lib_path, &lock_packs, installed)
    } else {
        println!("Found no dependencies in `pyproject.toml` to install")
    }

    if write_lock(lock_filename, &lock).is_err() {
        abort("Problem writing lock file");
    }
}

fn main() {
    // todo perhaps much of this setup code should only be in certain match branches.
    let cfg_filename = "pyproject.toml";
    let lock_filename = "pypackage.lock";

    let mut cfg = Config::from_file(cfg_filename).unwrap_or_default();

    let opt = Opt::from_args();
    let subcmd = match opt.subcmds {
        Some(sc) => sc,
        None => {
            abort("No command entered. For a list of what you can do, run `pyproject --help`.");
            SubCommand::Init {} // Dummy to satisfy the compiler.
        }
    };

    // New doesn't execute any other logic. Init must execute befor the rest of the logic,
    // since it sets up a new (or modified) `pyproject.toml`. The rest of the commands rely
    // on the virtualenv and `pyproject.toml`, so make sure those are set up before processing them.
    match subcmd {
        SubCommand::New { name } => {
            new(&name).expect("Problem creating project");
            println!("Created a new Python project named {}", name);
            return;
        }
        SubCommand::Init {} => {
            edit_files::parse_req_dot_text(&mut cfg);
            edit_files::parse_pipfile(&mut cfg);
            edit_files::parse_poetry(&mut cfg);
            edit_files::update_pyproject(&cfg);

            cfg.write_file(cfg_filename);
        }
        _ => (),
    }

    let pypackage_dir = env::current_dir()
        .expect("Can't find current path")
        .join("__pypackages__");

    let py_version_cfg = cfg.py_version;

    // Check for environments. Create one if none exist. Set `vers_path`.
    let mut vers_path = PathBuf::new();
    match py_version_cfg {
        // The version's explicitly specified; check if an environment for that version
        // exists. If not, create one, and make sure it's the right version.
        Some(cfg_v) => {
            // The version's specified in the config. Ensure a virtualenv for this
            // is setup.  // todo: Confirm using --version on the python bin, instead of relying on folder name.

            // Don't include version patch in the directory name, per PEP 582.
            vers_path = pypackage_dir.join(&format!("{}.{}", cfg_v.major, cfg_v.minor));

            if !util::venv_exists(&vers_path.join(".venv")) {
                let created_vers = create_venv(Some(&cfg_v), &pypackage_dir);
            }
        }
        // The version's not specified in the config; Search for existing environments, and create
        // one if we can't find any.
        None => {
            // Note that we rely on the proper folder name, vice inspecting the binary.
            // ie: could also check `bin/python --version`.
            let venv_versions_found: Vec<Version> = util::possible_py_versions()
                .into_iter()
                .filter(|v| {
                    util::venv_exists(
                        &pypackage_dir.join(&format!("{}.{}/.venv", v.major, v.minor)),
                    )
                })
                .collect();

            match venv_versions_found.len() {
                0 => {
                    let created_vers = create_venv(None, &pypackage_dir);
                    vers_path = pypackage_dir
                        .join(&format!("{}.{}", created_vers.major, created_vers.minor));
                }
                1 => {
                    vers_path = pypackage_dir.join(&format!(
                        "{}.{}",
                        venv_versions_found[0].major, venv_versions_found[0].minor
                    ));
                }
                _ => abort(
                    "Multiple Python environments found
                for this project; specify the desired one in `pyproject.toml`. Example:
[tool.pyproject]
py_version = \"3.7\"",
                ),
            }
        }
    };

    let lib_path = vers_path.join("lib");
    let (bin_path, lib_bin_path) = util::find_bin_path(&vers_path);

    let lock = match read_lock(lock_filename) {
        Ok(l) => {
            println!("Found lockfile");
            l
        }
        Err(_) => Lock::default(),
    };

    let args = opt.custom_bin;
    if !args.is_empty() {
        // todo better handling, eg abort
        let name = args.get(0).expect("Missing first arg").clone();
        let args: Vec<String> = args.into_iter().skip(1).collect();
        if commands::run_bin(&bin_path, &lib_path, &name, &args).is_err() {
            abort(&format!(
                "Problem running the binary script {}. Is it installed? \
                 Try running `pypackage install {} -b`",
                name, name
            ));
        }

        return;
    }

    match subcmd {
        // Add pacakge names to `pyproject.toml` if needed. Then sync installed packages
        // and `pyproject.lock` with the `pyproject.toml`.
        SubCommand::Install { packages, bin } => {
            let mut added_deps: Vec<Req> = packages
                .into_iter()
                .map(|p| Req::from_str(&p, false).unwrap())
                .collect();

            let installed = find_installed(&lib_path);

            // todo: Compare to existing listed lock_packs and merge appropriately.
            edit_files::add_dependencies(cfg_filename, &added_deps);

            let mut deps = cfg.dependencies.clone();
            deps.append(&mut added_deps);

            sync_deps(lock_filename, &bin_path, &lib_path, &mut deps, &installed);
            println!("Installation complete")
        }
        SubCommand::Uninstall { packages } => {
            // todo: DRY with ::Install
            let removed_deps: Vec<Req> = packages
                .into_iter()
                .map(|p| Req::from_str(&p, false).unwrap())
                .collect();

            edit_files::remove_dependencies(cfg_filename, &removed_deps);

            let installed = find_installed(&lib_path);
            sync_deps(
                lock_filename,
                &bin_path,
                &lib_path,
                &mut cfg.dependencies,
                &installed,
            )
        }

        SubCommand::Python { args } => {
            if commands::run_python(&bin_path, &lib_path, &args).is_err() {
                abort("Problem running Python");
            }
        }
        SubCommand::Package {} => build::build(&bin_path, &lib_path, &cfg),
        SubCommand::Publish {} => build::publish(&bin_path, &cfg),

        // We already handled init
        SubCommand::Init {} => (),
        SubCommand::New { name } => (),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

}
