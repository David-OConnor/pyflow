// use dirs;
use regex;
use std::{env, fs, num, path, process::Command, str};
use structopt::StructOpt;
use toml;

/// Categorize arguments parsed from the command line.
#[derive(Debug)]
enum Arg {
    Install,
    Uninstall,
    Python,
    // todo ipython ?
    Other(String), // eg a script file, or package name to install.
}

impl str::FromStr for Arg {
    type Err = num::ParseIntError; // todo not sure what to put here.

    fn from_str(arg: &str) -> Result<Self, Self::Err> {
        let result = match arg {
            "install" => Arg::Install,
            "uninstall" => Arg::Uninstall,
            "python" => Arg::Python,
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
    Uninstall(Vec<Package>),
    Run(Option<String>), // ie run python, or a script
    // todo ipython?
    Pip(Vec<String>), // If if we want pip list etc
    General(Vec<String>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum VersionType {
    Exact,
    OrHigher,
    OrLower,
}

impl ToString for VersionType {
    fn to_string(&self) -> String {
        match self {
            VersionType::Exact => "==".into(),
            VersionType::OrHigher => ">=".into(),
            VersionType::OrLower => "<=".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Package {
    name: String,
    version_type: VersionType, // Not used if version not specified.
    // None on version means not specified
    version: Option<(u32, u32, u32)>, // https://semver.org
}

impl Package {
    pub fn name_with_version(&self) -> String {
        match self.version {
            Some(version) => {
                self.name.clone()
                    + &self.version_type.to_string()
                    + &format!("{}.{}.{}", version.0, version.1, version.2)
            }
            None => self.name.clone(),
        }
    }
}

impl From<String> for Package {
    fn from(arg: String) -> Self {
        // todo impl with regex for versions
        Self {
            name: arg,
            version_type: VersionType::Exact,
            version: None,
        }
    }
}

// impl str::FromStr for Package {
//     type Err = num::ParseIntError; // todo not sure what to put here.

//     fn from_str(arg: &str) -> Result<Self, Self::Err> {
//         // todo impl with regex for versions
//         let result = Self {
//             name: arg,
//             version_type: VersionType::Exact,
//             version: None,
//         }

//         Ok(result)
//     }
// }

#[derive(StructOpt, Debug)]
#[structopt(name = "basic")]
struct Opt {
    #[structopt(name = "args")]
    args: Vec<Arg>, // ie "install", "python" etc.
}

/// A config, parsed from Python.toml
struct Config {
    py_version: (u32, u32),
    dependencies: Vec<Package>,
}

impl Config {
    pub fn from_file(file_name: &str) -> Self {
        match fs::read_to_string(file_name) {
            Ok(data) => {
                let data = data
                    .parse::<toml::Value>()
                    .expect("Problem parsing Python.toml");

                let version_str = &data["Python"]["version"]
                    .as_str()
                    .expect("Problem finding py_version in Python.toml");
                let py_version = py_version_from_str(version_str);

                let mut dependencies = Vec::new();
                dependencies.push("saturn".to_string().into());
                // todo fill dependencies

                Self {
                    py_version,
                    dependencies,
                }
            }
            Err(_) => panic!("Can't find Python.toml in this directory . Does it exist?"),
        }
    }
}

/// Parse the py_version from a config file, eg "3.7".
fn py_version_from_str(py_version: &str) -> (u32, u32) {
    (3, 7)
}

/// Find the py_version from the `python --py_version` command. Eg: "Python 3.7".
fn find_version(alias: &str) -> (u32, u32) {
    Command::new(alias)
        .args(&["--py_version"])
        .output()
        .expect("Problem finding python py_version");

    // todo fix with regex
    (3, 7)
}

/// Make an educated guess at the command needed to execute python the
/// current system.
fn find_py_alias(config_ver: (u32, u32)) -> String {
    let mut guess = "python3";
    let version_guess = find_version(guess);

    guess.to_string()
}

/// Create the virtual env
fn create_venv(py_alias: &str, venv_name: &str) {
    Command::new(py_alias)
        .args(&["-m", "venv", venv_name])
        .spawn()
        .expect("Problem creating the virtual environment");
}

fn venv_exists(venv_path: &path::PathBuf) -> bool {
    venv_path.exists()
}

fn install(venv_name: &str, packages: &Vec<Package>, uninstall: bool) {
    // We don't need an alias from the venv's bin directory; should
    // always be `python` or `pip`.
    let install = if uninstall { "uninstall" } else { "install" };

    // todo: this path setup may be linux specific. Make it more generic.
    for package in packages {
        // Even though `bin` contains `pip`, it doesn't appear to work directly.
        Command::new("./python")
            .current_dir(&format!("{}/bin", venv_name))
            .args(&["-m", "pip", install, &package.name_with_version()])
            .status()
            .expect(&format!(
                "Problem {}ing these packages: {:#?}",
                install, packages
            ));
    }
}

fn run_python(venv_name: &str, script: &Option<String>) {
    // todo: this path setup may be linux specific. Make it more generic.
    match script {
        Some(filename) => {
            Command::new("./python")
                .current_dir(&format!("{}/bin", venv_name))
                .arg(filename)
                .status()
                .expect(&format!("Problem running Python with {}", filename));
        }
        None => {
            Command::new("./python")
                .current_dir(&format!("{}/bin", venv_name))
                .status()
                .expect("Problem running Python");
        }
    }
}

// todo consolidate this (and others) with run python or run_general?
fn run_pip(venv_name: &str, args: &Vec<String>) {
    // todo: this path setup may be linux specific. Make it more generic.
    Command::new("./pip")
        .current_dir(&format!("{}/bin", venv_name))
        .args(args)
        .status()
        .expect("Problem running Pip");
}

/// Run a general task not specialized to this package.
fn run_general(venv_name: &str, args: &Vec<String>) {
    // todo: this path setup may be linux specific. Make it more generic.
    Command::new("bash")
        .current_dir(&format!("{}/bin", venv_name))
        .arg("-c")
        .args(args)
        .status()
        .expect("Problem running Python");
}

fn find_tasks(args: &Vec<Arg>) -> Vec<Task> {
    // We want to match args as appropriate. Ie, `python main.py`, and
    // `pip install django requests` are parsed as separate args,
    //but should be treated as single items.
    let mut result = vec![];
    // let mut current_task = vec![];

    for (i, arg) in args.iter().enumerate() {
        match arg {
            // Non-custom args are things like Python, Install etc;
            // start a new group.
            Arg::Install => {
                let mut packages: Vec<Package> = Vec::new();
                for i2 in i + 1..args.len() {
                    let arg2 = &args[i2];
                    match arg2 {
                        Arg::Other(name) => packages.push(name.to_string().into()),
                        _ => break,
                    }
                }
                if packages.is_empty() {
                    result.push(Task::InstallAll);
                } else {
                    result.push(Task::Install(packages))
                }
            }
            Arg::Uninstall => {
                // todo DRY
                let mut packages: Vec<Package> = Vec::new();
                for i2 in i + 1..args.len() {
                    let arg2 = &args[i2];
                    match arg2 {
                        Arg::Other(name) => packages.push(name.to_string().into()),
                        _ => break,
                    }
                }
                if packages.is_empty() {
                    result.push(Task::UninstallAll);
                } else {
                    result.push(Task::Uninstall(packages))
                }
            }
            Arg::Python => {
                let mut script = None;
                for i2 in i + 1..args.len() {
                    let arg2 = &args[i2];
                    match arg2 {
                        Arg::Other(filename) => script = Some(filename.to_string()),
                        _ => (),
                    }
                    break; // todo: Consider how to handle more than one arg following `python`.
                }
                result.push(Task::Run(script));
            }
            // Custom args can't start tasks; we handle them in the recursive
            // arms above.
            Arg::Other(_) => (),
        }
    }
    result
}

/// Write dependencies to Python.toml
fn add_dependencies(dependencies: &Vec<Package>) {
    let data = fs::read_to_string("Python.toml")
        .expect("Unable to read Python.toml while attempting to add a dependency");

    // todo
    let new_data = data;

    fs::write("Python.toml", new_data)
        .expect("Unable to read Python.toml while attempting to add a dependency");
}

/// Remove dependencies from Python.toml
fn remove_dependencies(dependencies: &Vec<Package>) {
    let data = fs::read_to_string("Python.toml")
        .expect("Unable to read Python.toml while attempting to add a dependency");

    // todo
    let new_data = data;

    fs::write("Python.toml", new_data)
        .expect("Unable to read Python.toml while attempting to add a dependency");
}

fn main() {
    let opt = Opt::from_args();
    let config = Config::from_file("Python.toml");
    let py_alias = find_py_alias(config.py_version);
    let project_dir = env::current_dir().expect("Can't find current path");

    let venv_name = ".venv";
    let venv_path = project_dir.join(venv_name);

    if !venv_exists(&venv_path) {
        create_venv(&py_alias, venv_name);
    }

    for task in find_tasks(&opt.args).iter() {
        match task {
            // todo DRY
            Task::Install(packages) => {
                install(&venv_name, packages, false);
                add_dependencies(packages);
            }
            Task::InstallAll => {
                install(&venv_name, &config.dependencies, false);
            }
            Task::Uninstall(packages) => {
                install(&venv_name, packages, true);
                remove_dependencies(packages);
            }
            Task::UninstallAll => {
                install(&venv_name, &config.dependencies, true);
            }
            Task::Run(script) => run_python(&venv_name, script),
            Task::Pip(args) => run_pip(&venv_name, args),
            Task::General(args) => run_general(&venv_name, args),
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn tasks_python() {
        let args = vec![Arg::Python];
        assert_eq!(vec![Task::Run(None)], find_tasks(&args));
    }

    #[test]
    fn tasks_python_with_script() {
        let script = "main.py".to_string();
        let args = vec![Arg::Python, Arg::Other(script.clone())];
        assert_eq!(vec![Task::Run(Some(script))], find_tasks(&args));
    }

    #[test]
    fn tasks_ipython() {
        //        let args = vec![Arg::Python];
        //        assert_eq!(Task::Run(None), find_tasks(&args));
    }

    #[test]
    fn tasks_install_one() {
        let name = "requests".to_string();
        let args = vec![Arg::Install, Arg::Other(name.clone())];

        assert_eq!(
            vec![Task::Install(vec![Package {
                name,
                version_type: VersionType::Exact,
                version: None,
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
                },
                Package {
                    name: name2,
                    version_type: VersionType::Exact,
                    version: None,
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

    #[test]
    fn tasks_uninstall_one() {
        let name = "requests".to_string();
        let args = vec![Arg::Uninstall, Arg::Other(name.clone())];

        assert_eq!(
            vec![Task::Uninstall(vec![Package {
                name,
                version_type: VersionType::Exact,
                version: None,
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
                },
                Package {
                    name: name2,
                    version_type: VersionType::Exact,
                    version: None,
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

    fn tasks_general() {
        let name1 = "pip".to_string();
        let name2 = "list".to_string();
        let args = vec![Arg::Other(name1.clone()), Arg::Other(name2.clone())];
        assert_eq!(vec![Task::General(vec![name1, name2])], find_tasks(&args));
    }

    // todo: Invalid or non-standard task arg combos for tasks
    // todo: Versioned tasks.
}
