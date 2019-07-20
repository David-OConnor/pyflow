use clap;
use crate::package_types::{Package, Version, VersionType};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    error::Error,
    fmt, fs,
    io::{self, BufRead, BufReader},
    path::PathBuf,
    process,
    string::ParseError,
    str::FromStr,
    thread, time,
};
use structopt::StructOpt;
//use textio;
use toml;

mod build;
mod commands;
mod package_types;
mod util;

/// Categorize arguments parsed from the command line.
#[derive(Debug)]
enum Arg {
    Install,
    InstallBin, // todo temp perhaps
    Uninstall,
    Python,
    List,
    Package,
    Publish,
    New,
    Help,
    Version,
    Other(String), // eg a script file, or package name to install.
}

impl FromStr for Arg {
    type Err = ParseError;

    fn from_str(arg: &str) -> Result<Self, Self::Err> {
        let result = match arg.to_string().to_lowercase().as_ref() {
            "install" => Arg::Install,
            "installbin" => Arg::InstallBin,
            "uninstall" => Arg::Uninstall,
            "python" => Arg::Python,
            "python3" => Arg::Python,
            "list" => Arg::List,
            "package" => Arg::Package,
            "publish" => Arg::Publish,
            "new" => Arg::New,
            "help" => Arg::Help,
            "version" => Arg::Version,
            _ => Arg::Other(arg.into()),
        };

        Ok(result)
    }
}

/// Similar to Arg, but grouped.
#[derive(Debug, PartialEq)]
enum Task {
    InstallAll,
    UninstallAll,
    Install(Vec<Package>),
    InstallBin(Vec<Package>), // todo temp perhaps
    Uninstall(Vec<Package>),
    Python(Vec<String>),
    CustomBin(String, Vec<String>), // bin name, args

    // todo: Instead of special ipython here, should have a type for running any
    // todo custom executable for Python that ends up in the bin/Scripts folder.
    // todo eg Black.

    //    IPython(Vec<String>),
    //    Pip(Vec<String>), // If if we want pip list etc
    //    General(Vec<String>),
    New(String), // holds the project name.
    Package,
    Publish,
    Help,
    Version,
}



// todo: Another string parser to package, from pip fmt ie == / >=

#[derive(StructOpt, Debug)]
#[structopt(name = "basic")]
struct Opt {
    #[structopt(name = "args")]
    args: Vec<Arg>, // ie "install", "python" etc.
}

/// A config, parsed from pyproject.toml
#[derive(Debug, Default, Deserialize)]
struct Config {
    py_version: Option<Version>,
    dependencies: Vec<Package>,
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
                        result.dependencies.push(Package::from_str(&l).unwrap());
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
fn find_py_alias(config_ver: Option<Version>) -> Result<(String, Version), AliasError> {
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

fn find_sub_dependencies(package: Package) -> Vec<Package> {
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
    fn add_packages(&mut self, packages: &[Package]) {
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

/// Categorize CLI arguments.
fn find_tasks(args: &[Arg]) -> Vec<Task> {
    // We want to match args as appropriate. Ie, `python main.py`, and
    // `pip install django requests` are parsed as separate args,
    //but should be treated as single items.
    let mut result = vec![];

    // todo: Figure out a better way than the messy, repetative sub-iteting,
    // todo for finding grouped args.

    let mut i = 0;
//    for (i, arg) in args.iter().enumerate() {
//    for for i in 0..args.len() {
    while i < args.len() {
        match args.get([i]).expect("Can't find arg by index") {
            // Non-custom args are things like Python, Install etc;
            // start a new group.
            Arg::Install => {
                let mut packages: Vec<Package> = Vec::new();
                for i2 in i + 1..args.len() {
                    let arg2 = &args[i2];
                    match arg2 {
                        Arg::Other(name) => packages.push(Package::from_str(name).unwrap()),
                        _ => {
                            i = i2;  // Next loop, skip over the package-args we added.
                            break
                        },
                    }
                }
                if packages.is_empty() {
                    result.push(Task::InstallAll);
                } else {
                    result.push(Task::Install(packages))
                }
            }
            Arg::InstallBin => {
                // todo DRY!
                let mut packages: Vec<Package> = Vec::new();
                for i2 in i + 1..args.len() {
                    let arg2 = &args[i2];
                    match arg2 {
                        Arg::Other(name) => packages.push(Package::from_str(name).unwrap()),
                        // Ipython as an arg could mean run ipython, or install it, if post the `install` arg.
                        //                        Arg::IPython => packages.push(Package::from_str("ipython").unwrap()),
                        _ => {
                            i = i2;
                            break
                        },
                    }
                }
                result.push(Task::InstallBin(packages))
            }
            Arg::Uninstall => {
                // todo DRY
                let mut packages: Vec<Package> = Vec::new();
                for i2 in i + 1..args.len() {
                    let arg2 = &args[i2];
                    match arg2 {
                        Arg::Other(name) => packages.push(Package::from_str(name).unwrap()),
                        _ => {
                            i = i2;
                            break
                        },
                    }
                }
                if packages.is_empty() {
                    result.push(Task::UninstallAll);
                } else {
                    result.push(Task::Uninstall(packages))
                }
            }
            Arg::Python => {
                let mut args_ = vec![];
                for i2 in i + 1..args.len() {
                    let arg2 = &args[i2];
                    if let Arg::Other(a) = arg2 {
                        args_.push(a.to_string());
                    }
                }
                result.push(Task::Python(args_));
            }

            Arg::List => {
                // todo

            }
            Arg::New => {
                // Exactly one arg is allowed following New.
                let name = match args.get(i + 1) {
                    Some(a) => {
                        match a {
                            Arg::Other(name) => {
                                result.push(Task::New(name.into()));
                                break
                            },
                            // todo: Allow other arg types
                            _ => util::exit_early("Please pick a different name")
                        }
                    },
                    None => util::exit_early("Please specify a name for the projct, and try again. Eg: pyproject new myproj")
                };

            }
            Arg::Package => result.push(Task::Package),
            Arg::Publish => result.push(Task::Publish),
            Arg::Help => result.push(Task::Help),
            Arg::Version => result.push(Task::Version),
            // todo pop args for custom!
            Arg::Other(name) => (result.push(Task::CustomBin(name.to_string(), vec![]))),
        }
        i += 1;
    }
    result
}

/// Write dependencies to pyproject.toml
fn add_dependencies(filename: &str, dependencies: &[Package]) {
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
fn remove_dependencies(filename: &str, dependencies: &[Package]) {
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

fn create_venv(py_v: Option<Version>, lib_path: &PathBuf) {
    // We only use the alias for creating the virtual environment. After that,
    // we call our venv's executable directly.
    let mut alias = String::new();
    let mut version = Version::new(0, 0, 0);
    match find_py_alias(py_v) {
        Ok(a) => {
            alias = a.0;
            version = a.1;
        }
        Err(_) => util::exit_early("Unable to find a Python version on the path"),
    };

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

fn main() {
    let package_dir = "__pypackages__";
    let cfg_filename = "pyproject.toml";
    let lock_filename = "pypackage.lock";

    let project_dir = env::current_dir().expect("Can't find current path");
    let py_version = cfg.py_version.unwrap_or(Version::new_short(3, 7)); // todo better default

    let cfg = Config::from_file(cfg_filename);

    // Don't include version patch in the directory name, per PEP 582.
    let venv_path = project_dir.join(&format!(
        "{}/{}.{}/.venv",
        package_dir, py_version.major, py_version.minor
    ));
    let lib_path = project_dir.join(&format!(
        "{}/{}.{}/lib",
        package_dir, py_version.major, py_version.minor
    ));

    let mut lock = match read_lock(lock_filename) {
        Ok(l) => {
            println!("Found lockfile");
            l
        }
        Err(_) => Lock::default(),
    };

    // todo: Doesn't work with Scripts
    let bin_path_temp = venv_path.join("bin");
    if !util::venv_exists(&bin_path_temp) {
        // todo fix this check for the venv existing.
        create_venv(cfg.py_version, &venv_path);
    }

    // The bin name should be `bin` on Linux, and `Scripts` on Windows. Check both.
    // Locate bin name after ensuring we have a virtual environment.
    let mut bin_path = venv_path.clone();
    // It appears that 'binary' scripts are installed in the `lib` directory's bin folder when
    // using the --target arg, instead of the one directly in the env.
    let mut custom_bin_path = venv_path.clone();
    if venv_path.join("bin").exists() {
        bin_path = venv_path.join("bin");
        // We assume that the name applies to both the directory in the virtual env, and in `lib`.
        custom_bin_path = lib_path.join("bin");
    } else if venv_path.join("Scripts").exists() {
        bin_path = venv_path.join("Scripts");
        custom_bin_path = lib_path.join("bin");
    } else {
        util::exit_early("Can't find the new binary directory. (ie `bin` or `Scripts` in the virtual environment's folder)")
    }

    let opt = Opt::from_args();

    for task in find_tasks(&opt.args).iter() {
        match task {
            Task::Install(packages) => {
                if let Err(_) = commands::install(&bin_path, packages, false, false) {
                    util::exit_early("Problem installing packages");
                }
                add_dependencies(cfg_filename, packages);

                lock.add_packages(packages);

                //                write_lock(lock_filename, &lock).expect("Problem writing lock.");
                if let Err(_) = write_lock(lock_filename, &lock) {
                    util::exit_early("Problem writing the lock file");
                }
            }
            Task::InstallBin(packages) => {
                // todo DRY
                if let Err(_) = commands::install(&bin_path, packages, false, true) {
                    util::exit_early("Problem installing packages");
                }
                add_dependencies(cfg_filename, packages);

                lock.add_packages(packages);

                //                write_lock(lock_filename, &lock).expect("Problem writing lock.");
                if let Err(_) = write_lock(lock_filename, &lock) {
                    util::exit_early("Problem writing the lock file");
                }
            }
            Task::InstallAll => {
                if let Err(_) = commands::install(&bin_path, &cfg.dependencies, false, false) {
                    util::exit_early("Problem installing packages");
                }
                //                write_lock(lock_filename, &lock).expect("Problem writing lock.");
                if let Err(_) = write_lock(lock_filename, &lock) {
                    util::exit_early("Problem writing the lock file");
                }
            }
            Task::Uninstall(packages) => {
                // todo: Display which packages?
                match commands::install(&bin_path, packages, true, false) {
                    Ok(_) => (),
                    Err(_) => util::exit_early("Problem uninstalling packages"),
                }
                remove_dependencies(cfg_filename, packages);
                //                write_lock(lock_filename, &lock).expect("Problem writing lock.");
                match write_lock(lock_filename, &lock) {
                    Ok(_) => (),
                    Err(_) => util::exit_early("Problem writing the lock file"),
                }
            }
            Task::UninstallAll => {
                commands::install(&bin_path, &cfg.dependencies, true, false)
                    .expect("Problem uninstalling packages");
                //                write_lock(lock_filename, &Lock::default()).expect("Problem writing lock.");
                match write_lock(lock_filename, &Lock::default()) {
                    Ok(_) => (),
                    Err(e) => util::exit_early("Problem writing the lock file"),
                }
            }
            Task::Python(args) => commands::run_python(&bin_path, &lib_path, args),
            Task::CustomBin(name, args) => {
                // todo put this back.
                //
                //                let mut bin_package_installed = false;
                //                for package in &cfg.dependencies {
                //                    if &package.name == name {
                //                        bin_package_installed = true;
                //                    }
                //                }
                //
                //                if !bin_package_installed {
                //                    if let Err(_) = commands::install(
                //                        &bin_path,
                //                        &[Package {
                //                            name: name.to_string(),
                //                            version: None,
                //                            version_type: VersionType::Exact,
                //                        }],
                //                        false,
                //                        true,
                //                    ) {
                //                        util::exit_early(&format!("Problem installing {}", name));
                //                    }
                //                }
                //                wait_for_dirs(&vec![bin_path.join(name)]).unwrap();
                //                commands::run_bin(&custom_bin_path, &lib_path, name, args);
                commands::run_bin(&bin_path, &lib_path, name, args);
            }
            Task::New(name) => {
                match new(name) {
                    Ok(_) => (),
                    Err(_) => util::exit_early("Problem creating project"),
                }
                //                new(name).expect("Problem creating project");
                // todo: Maybe show the path created at.
                println!("Created a new Python project named {}", name)
            }
            Task::Package => build::build(&bin_path, &cfg),
            Task::Publish => build::publish(&bin_path, &cfg),
            Task::Help => help(),
            Task::Version => version(),
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::package_types::{Package, Version, VersionType};

    #[test]
    fn tasks_python() {
        let args = vec![Arg::Python];
        assert_eq!(vec![Task::Python(vec![])], find_tasks(&args));
    }

    #[test]
    fn tasks_python_with_script() {
        let script = "main.py".to_string();
        let args = vec![Arg::Python, Arg::Other(script.clone())];
        assert_eq!(vec![Task::Python(vec![script])], find_tasks(&args));
    }

    //    #[test]
    //    fn tasks_ipython() {
    //        let args = vec![Arg::IPython];
    //        assert_eq!(vec![Task::IPython(vec![])], find_tasks(&args));
    //    }

    #[test]
    fn tasks_install_one() {
        let name = "requests".to_string();
        let args = vec![Arg::Install, Arg::Other(name.clone())];

        assert_eq!(
            vec![Task::Install(vec![Package {
                name,
                version_type: VersionType::Exact,
                version: None,
                bin: false,
            }])],
            find_tasks(&args)
        );
    }

    #[test]
    fn tasks_install_several() {
        let name1 = "numpy".to_string();
        let name2 = "scipy".to_string();
        let args = vec![
            Arg::Install,
            Arg::Other(name1.clone()),
            Arg::Other(name2.clone()),
        ];

        assert_eq!(
            vec![Task::Install(vec![
                Package {
                    name: name1,
                    version_type: VersionType::Exact,
                    version: None,
                    bin: false,
                },
                Package {
                    name: name2,
                    version_type: VersionType::Exact,
                    version: None,
                    bin: false,
                }
            ])],
            find_tasks(&args)
        );
    }

    #[test]
    fn tasks_install_all() {
        let args = vec![Arg::Install];
        assert_eq!(vec![Task::InstallAll], find_tasks(&args));
    }

    //    #[test]
    //    fn tasks_install_ipython() {
    //        // Needed to make sure this is attempting to install ipython, not install all and run ipython.
    //        let args = vec![Arg::Install, Arg::IPython];
    //        assert_eq!(
    //            vec![Task::Install(vec![Package {
    //                name: "ipython".to_string(),
    //                version_type: VersionType::Exact,
    //                version: None,
    //            }])],
    //            find_tasks(&args)
    //        );
    //    }

    #[test]
    fn tasks_uninstall_one() {
        let name = "requests".to_string();
        let args = vec![Arg::Uninstall, Arg::Other(name.clone())];

        assert_eq!(
            vec![Task::Uninstall(vec![Package {
                name,
                version_type: VersionType::Exact,
                version: None,
                bin: false,
            }])],
            find_tasks(&args)
        );
    }

    #[test]
    fn tasks_uninstall_several() {
        let name1 = "numpy".to_string();
        let name2 = "scipy".to_string();
        let args = vec![
            Arg::Uninstall,
            Arg::Other(name1.clone()),
            Arg::Other(name2.clone()),
        ];

        assert_eq!(
            vec![Task::Uninstall(vec![
                Package {
                    name: name1,
                    version_type: VersionType::Exact,
                    version: None,
                    bin: false,
                },
                Package {
                    name: name2,
                    version_type: VersionType::Exact,
                    version: None,
                    bin: false,
                }
            ])],
            find_tasks(&args)
        );
    }

    #[test]
    fn tasks_uninstall_all() {
        let args = vec![Arg::Uninstall];
        assert_eq!(vec![Task::UninstallAll], find_tasks(&args));
    }

    //    #[test]
    //    fn tasks_pip() {
    //        let args = vec![Arg::Pip, Arg::Other("list".into())];
    //        assert_eq!(vec![Task::Pip(vec!["list".into()])], find_tasks(&args));
    //    }

    //    #[test]
    //    fn tasks_general() {
    //        let name1 = "pip".to_string();
    //        let name2 = "list".to_string();
    //        let args = vec![Arg::Other(name1.clone()), Arg::Other(name2.clone())];
    //        assert_eq!(vec![Task::General(vec![name1, name2])], find_tasks(&args));
    //    }

    // todo: Invalid or non-standard task arg combos for tasks
    // todo: Versioned tasks.

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
        let p = Package::from_str("saturn").unwrap();
        assert_eq!(
            p,
            Package {
                name: "saturn".into(),
                version: None,
                version_type: VersionType::Exact,
                bin: false,
            }
        )
    }

    #[test]
    fn parse_package_withvers() {
        let p = Package::from_str("bolt = \"3.1.4\"").unwrap();
        assert_eq!(
            p,
            Package {
                name: "bolt".into(),
                version: Some(Version::new(3, 1, 4)),
                version_type: VersionType::Exact,
                bin: false,
            }
        )
    }

    #[test]
    fn parse_package_carot() {
        let p = Package::from_str("chord = \"^2.7.18\"").unwrap();
        assert_eq!(
            p,
            Package {
                name: "chord".into(),
                version: Some(Version::new(2, 7, 18)),
                version_type: VersionType::Carot,
                bin: false,
            }
        )
    }

    #[test]
    fn parse_package_tilde_short() {
        let p = Package::from_str("sphere = \"~6.7\"").unwrap();
        assert_eq!(
            p,
            Package {
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
