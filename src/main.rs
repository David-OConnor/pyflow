use crate::package_types::{Dependency, Version, VersionType};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    error::Error,
    fmt, fs,
    io::{self, BufRead, BufReader},
    path::PathBuf,
    str::FromStr,
    thread, time,
};
use structopt::StructOpt;
//use textio;
use crate::util::exit_early;
use toml;

mod build;
mod commands;
mod package_types;
mod util;

#[derive(StructOpt, Debug)]
#[structopt(name = "Pypackage", about = "Python packaging and publishing")]
struct Opt {
    #[structopt(subcommand)]
    subcmds: SubCommand,

    //    custom_bin: Option<Vec<String>>
    //    #[structopt(name = "custom_bin")]
    //    #[structopt(subcommand)]
    //    custom_bin: CustomBin,
    #[structopt(name = "custom_bin")]
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
    #[structopt(name = "new")]
    New {
        #[structopt(name = "name")]
        name: String, // holds the project name.
    },

    #[structopt(name = "install")]
    Install {
        #[structopt(name = "packages")]
        packages: Vec<String>,
        #[structopt(short = "b", long = "binary")]
        bin: bool,
    },
    #[structopt(name = "uninstall")]
    Uninstall {
        #[structopt(name = "packages")]
        packages: Vec<String>,
    },
    #[structopt(name = "python")]
    Python {
        #[structopt(name = "args")]
        args: Vec<String>,
    },
    #[structopt(name = "package")]
    Package,
    #[structopt(name = "publish")]
    Publish,
    /// Designed to be run inside a project directory; creates a `pyproject.toml`
    /// from requirements.txt, pipfile etc, setup.py etc.
    #[structopt(name = "init")]
    Init,
}

/// A config, parsed from pyproject.toml
#[derive(Debug, Default, Deserialize)]
struct Config {
    py_version: Option<Version>,
    dependencies: Vec<Dependency>,
    name: Option<String>,
    version: Option<Version>,
    author: Option<String>,
    author_email: Option<String>,
    description: Option<String>,
    classifiers: Vec<String>,
    keywords: Vec<String>, // todo: Classifiers vs keywords?
    homepage: Option<String>,
    repo_url: Option<String>,
    readme_filename: Option<String>,
    license: Option<String>,
}

fn key_re(key: &str) -> Regex {
    Regex::new(&format!(r#"^{}\s*=\s*"(.*)"$"#, key)).unwrap()
}

impl Config {
    /// Pull config data from Cargo.toml
    pub fn from_file(filename: &str) -> Self {
        // We don't use the `toml` crate here because it doesn't appear flexible enough.
        let mut result = Config::default();
        let file = fs::File::open(filename).expect("cannot open pyproject.toml");

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
                    //                    if let Some(n2) = key_re("version").captures(&l) {
                    //                        if let Some(n) = n2.get(1) {
                    //                            result.version = Some(Version::from_str(n.as_str()).unwrap());
                    //                        }
                    //                    }
                } else if in_dep {
                    if !l.is_empty() {
                        result.dependencies.push(Dependency::from_str(&l).unwrap());
                    }
                }
            }
        }

        result
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
    (alias.to_string(), version.clone())
}

#[derive(Debug)]
struct AliasError {
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

/// Help for this tool
fn help() {
    // todo: Use a pre-built help from a CLI crate?
    // todo
}

/// Version info about this tool
fn version() {
    // todo
}

/// Make an educated guess at the command needed to execute python the
/// current system.  An alternative approach is trying to find python
/// installations.
fn find_py_alias() -> Result<(String, Version), AliasError> {
    // todo expand, and iterate over versions.
    let possible_aliases = &[
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
        match commands::find_py_version(alias) {
            Some(v) => found_aliases.push((alias.to_string(), v)),
            None => (),
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

fn find_sub_dependencies(package: Dependency) -> Vec<Dependency> {
    // todo: This will be useful for dependency resolution, and removing packages
    // todo no longer needed when running install.
    vec![]
}

/// Similar to that used by Cargo.lock
#[derive(Debug, Deserialize, Serialize)]
struct LockPackage {
    name: String,
    // We use a tuple for version instead of Version, since the lock
    // uses exact, 3-number versions only.
    //    version: Option<LockVersion>,  // todo not sure how to implement
    version: String,
    source: Option<String>,
    dependencies: Option<Vec<String>>, // todo option self
}

/// Modelled after [Cargo.lock](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html)
#[derive(Debug, Default, Deserialize, Serialize)]
struct Lock {
    package: Option<Vec<LockPackage>>,
    metadata: Option<String>, // todo unimplemented
}

impl Lock {
    fn add_packages(&mut self, packages: &[Dependency]) {
        // todo: Write tests for this.

        for package in packages {
            // Use the actual version installed, not the requirement!
            // todo: reconsider your package etc structs
            let lock_package = LockPackage {
                name: package.name.clone(),
                version: package.version.unwrap_or(Version::new(0, 0, 0)).to_string(), // todo ensure 3-digit.
                source: None,                                                          // todo
                dependencies: None,                                                    // todo
            };

            match &mut self.package {
                Some(p) => p.push(lock_package),
                None => self.package = Some(vec![lock_package]),
            }
        }
    }
}

/// Read dependency data froma lock file.
fn read_lock(filename: &str) -> Result<(Lock), Box<Error>> {
    let data = fs::read_to_string(filename)?;
    let t: Lock = toml::from_str(&data).unwrap();
    Ok(toml::from_str(&data)?)
}

/// Write dependency data to a lock file.
fn write_lock(filename: &str, data: &Lock) -> Result<(), Box<Error>> {
    let data = toml::to_string(data)?;
    fs::write(filename, data)?;
    Ok(())
}

/// Write dependencies to pyproject.toml
fn add_dependencies(filename: &str, dependencies: &[Dependency]) {
    //        let data = fs::read_to_string("pyproject.toml")
    //            .expect("Unable to read pyproject.toml while attempting to add a dependency");
    let file = fs::File::open(filename).expect("cannot open pyproject.toml");

    let mut in_dep = false;

    let sect_re = Regex::new(r"\[.*\]").unwrap();

    let result = String::new();

    for line in BufReader::new(file).lines() {
        //    for line in data.lines() {
        if let Ok(l) = line {
            // todo replace this with something that clips off
            // todo post-# part of strings; not just ignores ones starting with #
            if l.starts_with('#') {
                continue;
            }

            if &l == "[tool.pypackage.dependencies]" {
                in_dep = true;
                continue;
            } else if sect_re.is_match(&l) {
                in_dep = false;
                continue;
            }

            if in_dep {}
        }
    }

    //    let new_data = data;

    //    fs::write("pyproject.toml", new_data)
    //        .expect("Unable to read pyproject.toml while attempting to add a dependency");
}

/// Remove dependencies from pyproject.toml
fn remove_dependencies(filename: &str, dependencies: &[Dependency]) {
    let data = fs::read_to_string("pyproject.toml")
        .expect("Unable to read pyproject.toml while attempting to add a dependency");

    // todo
    let new_data = data;

    fs::write(filename, new_data)
        .expect("Unable to read pyproject.toml while attempting to add a dependency");
}

/// Wait for directories to be created; required between modifying the filesystem,
/// and running code that depends on the new files.
fn wait_for_dirs(dirs: &Vec<PathBuf>) -> Result<(), AliasError> {
    // todo: AliasError is a quick fix to avoid creating new error type.
    let timeout = 1000; // ms
    for i in 0..timeout {
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
    Err(AliasError {
        details: "Timed out attempting to create a directory".to_string(),
    })
}

fn create_venv(alias: &str, lib_path: &PathBuf) {
    // We only use the alias for creating the virtual environment. After that,
    // we call our venv's executable directly.

    println!("Setting up Python environment...");

    // If the Python version's below 3.3, we must download and install the
    // `virtualenv` package, since `venv` isn't included.
    if version < Version::new_short(3, 3) {
        if let Err(_) = commands::install_virtualenv_global(&alias) {
            util::exit_early("Problem installing the virtualenv package, required by Python versions older than 3.3)");
        }
        if let Err(_) = commands::create_legacy_virtualenv(&alias, lib_path, ".venv") {
            util::exit_early("Problem creating virtual environment");
        }
    } else {
        if let Err(_) = commands::create_venv(&alias, lib_path, ".venv") {
            util::exit_early("Problem creating virtual environment");
        }
    }

    // Wait until the venv's created before continuing, or we'll get errors
    // when attempting to use it
    // todo: These won't work with Scripts ! - pass venv_path et cinstead
    let py_venv = lib_path.join("../.venv/bin/python");
    let pip_venv = lib_path.join("../.venv/bin/pip");
    wait_for_dirs(&vec![py_venv, pip_venv]).unwrap();
}

enum InstallType {
    Install,
    Uninstall,
}

/// Helper function to reduce repetition between installing and uninstalling
fn install(
    packages: &[Dependency],
    installed_packages: &[Dependency],
    lock: &mut Lock,
    lock_filename: &str,
    cfg_filename: &str,
    bin_path: &PathBuf,
    type_: InstallType,
    bin: bool,
) {
    if packages.is_empty() {
        // Install all from `pyproject.toml`.
        if let Err(_) = commands::install(&bin_path, installed_packages, false, false) {
            util::exit_early("Problem installing packages");
        }
        //                write_lock(lock_filename, &lock).expect("Problem writing lock.");
        if let Err(_) = write_lock(lock_filename, lock) {
            util::exit_early("Problem writing the lock file");
        }
    } else {
        if let Err(_) = commands::install(&bin_path, &packages, false, bin) {
            util::exit_early("Problem installing packages");
        }
        add_dependencies(cfg_filename, &packages);

        lock.add_packages(&packages);

        //                write_lock(lock_filename, &lock).expect("Problem writing lock.");
        if let Err(_) = write_lock(lock_filename, lock) {
            util::exit_early("Problem writing the lock file");
        }
    }
}

fn main() {
    let cfg_filename = "pyproject.toml";
    let lock_filename = "pypackage.lock";

    let pypackage_dir = env::current_dir()
        .expect("Can't find current path")
        .join("__pypackages__");
    let cfg = Config::from_file(cfg_filename);
    let py_version_cfg = cfg.py_version;

    // Note that we rely on the proper folder name, vice inspecting the binary.
    // ie: could also check `bin/python --version`.
    let venv_versions_found: Vec<Version> = util::possible_py_versions()
        .into_iter()
        .filter(|v| {
            let bin_path = pypackage_dir.join(&format!("{}.{}/.venv/bin", v.major, v.minor));
            util::venv_exists(&bin_path)
        })
        .collect();

    let py_version = match py_version_cfg {
        // The version's explicitly specified; use that.
        Some(v) => v,
        // If not, we'll have to guess.
        None => match venv_versions_found.len() {
            0 => None,
            1 => Some(venv_versions_found[0]),
            _ => {
                exit_early(
                    "Multiple Python environments found
                for this project; specify the desired one in `pyproject.toml`. Example:
[tool.pyproject]
py_version = \"3.7\"");
                None //  Required by compiler
            }
        },
    };

    // Don't include version patch in the directory name, per PEP 582.

    let vers_path = pypackage_dir.join(&format!("{}.{}", py_version.major, py_version.minor));
    let venv_path = vers_path.join(".venv");
    let lib_path = vers_path.join("lib");

    if !util::venv_exists(&vers_path) {
        //    let mut alias = String::new();
//    let mut version = Version::new(0, 0, 0);
        let (alias, py_version_from_alias) = match find_py_alias() {
            Ok(a) => a,
            Err(_) => util::exit_early("Unable to find a Python version on the path"),
        };

        if let Some(py_v) = py_version {
            if py_v != py_version_from_alias {
                util::exit_early("The python version specified doesn't match the alias you selected.");
            }
        }

        create_venv(&alias, &venv_path);
    }

    let (bin_path, lib_path) = util::find_bin_path(&venv_path, &lib_path);

    let mut lock = match read_lock(lock_filename) {
        Ok(l) => {
            println!("Found lockfile");
            l
        }
        Err(_) => Lock::default(),
    };

    let opt = Opt::from_args();

    //    if let Some(args) = opt.custom_bin {
    //        let mut bin_package_installed = false;
    //        for package in &cfg.dependencies {
    //            if &package.name == name {
    //                bin_package_installed = true;
    //            }
    //        }
    //
    //        if !bin_package_installed {
    //            if let Err(_) = commands::install(
    //                &bin_path,
    //                &[Package {
    //                    name: name.to_string(),
    //                    version: None,
    //                    version_type: VersionType::Exact,
    //                }],
    //                false,
    //                true,
    //            ) {
    //                util::exit_early(&format!("Problem installing {}", name));
    //            }
    //        }
    //        wait_for_dirs(&vec![bin_path.join(name)]).unwrap();

    //        // todo better handling, eg exit early.
    //        let name = args.get(1).expect("Missing first arg");
    //        let args: Vec<String> = args.iter().skip(1).collect();
    //
    //        commands::run_bin(&custom_bin_path, &lib_path, name, &args);
    //        commands::run_bin(&bin_path, &lib_path, name, &args);
    //    }
    match opt.subcmds {
        SubCommand::New { name } => {
            new(&name).expect("Problem creating project");
            //                // todo: Maybe show the path created at.
            println!("Created a new Python project named {}", name)
        }

        SubCommand::Install { packages, bin } => {
            let packages: Vec<Dependency> = packages
                .into_iter()
                .map(|p| Dependency::from_str(&p).unwrap())
                .collect();

            install(
                &packages,
                &[],
                &mut lock,
                lock_filename,
                cfg_filename,
                &bin_path,
                InstallType::Install,
                bin,
            );
        }
        SubCommand::Uninstall { packages } => {
            let packages: Vec<Dependency> = packages
                .into_iter()
                .map(|p| Dependency::from_str(&p).unwrap())
                .collect();

            install(
                &packages,
                &cfg.dependencies,
                &mut lock,
                lock_filename,
                cfg_filename,
                &bin_path,
                InstallType::Uninstall,
                false,
            );
        }

        SubCommand::Python { args } => commands::run_python(&bin_path, &lib_path, &args),
        SubCommand::Package {} => build::build(&bin_path, &cfg),
        SubCommand::Publish {} => build::publish(&bin_path, &cfg),
        SubCommand::Init {} => exit_early("Init not yet implemented"),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::package_types::{Dependency, Version, VersionType};

    #[test]
    fn valid_version() {
        assert_eq!(
            Version::from_str("3.7").unwrap(),
            Version {
                major: 3,
                minor: 7,
                patch: None
            }
        );
        assert_eq!(Version::from_str("3.12.5").unwrap(), Version::new(3, 12, 5));
        assert_eq!(Version::from_str("0.1.0").unwrap(), Version::new(0, 1, 0));
    }

    #[test]
    #[should_panic(expected = "Problem parsing version: 3-7")]
    fn bad_version() {
        Version::from_str("3-7").unwrap();
    }

    #[test]
    fn parse_package_novers() {
        let p = Dependency::from_str("saturn").unwrap();
        assert_eq!(
            p,
            Dependency {
                name: "saturn".into(),
                version: None,
                version_type: VersionType::Exact,
                bin: false,
            }
        )
    }

    #[test]
    fn parse_package_withvers() {
        let p = Dependency::from_str("bolt = \"3.1.4\"").unwrap();
        assert_eq!(
            p,
            Dependency {
                name: "bolt".into(),
                version: Some(Version::new(3, 1, 4)),
                version_type: VersionType::Exact,
                bin: false,
            }
        )
    }

    #[test]
    fn parse_package_carot() {
        let p = Dependency::from_str("chord = \"^2.7.18\"").unwrap();
        assert_eq!(
            p,
            Dependency {
                name: "chord".into(),
                version: Some(Version::new(2, 7, 18)),
                version_type: VersionType::Carot,
                bin: false,
            }
        )
    }

    #[test]
    fn parse_package_tilde_short() {
        let p = Dependency::from_str("sphere = \"~6.7\"").unwrap();
        assert_eq!(
            p,
            Dependency {
                name: "sphere".into(),
                version: Some(Version::new_short(6, 7)),
                version_type: VersionType::Tilde,
                bin: false,
            }
        )
    }

    #[test]
    fn version_ordering() {
        let a = Version::new(4, 9, 4);
        let b = Version::new(4, 8, 0);
        let c = Version::new(3, 3, 6);
        let d = Version::new(3, 3, 5);
        let e = Version::new(3, 3, 0);

        assert!(a > b && b > c && c > d && d > e);
    }
}
