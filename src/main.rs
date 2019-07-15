// use dirs;
use regex;
use std::{env, fs, num, path, process::Command, str};
use structopt::StructOpt;
use toml;

#[derive(Debug)]
enum Arg {
    Install,
    Uninstall,
    Custom(String), // eg python
}

impl str::FromStr for Arg {
    type Err = num::ParseIntError; // todo not sure what to put here.

    fn from_str(arg: &str) -> Result<Self, Self::Err> {
        let result = match arg {
            "install" => Arg::Install,
            "uninstall" => Arg::Uninstall,
            _ => Arg::Custom(arg.into()),
        };

        Ok(result)
    }
}

#[derive(Clone, Copy, Debug)]
enum VersionType {
    Exact,
    OrHigher,
}

impl ToString for VersionType {
    fn to_string(&self) -> String {
        match self {
            VersionType::Exact => "==".into(),
            VersionType::OrHigher => ">=".into(),
        }
    }
}

#[derive(Clone, Debug)]
struct Package {
    name: String,
    version: (u32, u32, u32), // https://semver.org
    version_type: VersionType,
}

impl Package {
    pub fn name_with_version(self) -> String {
        self.name
            + &self.version_type.to_string()
            + &format!("{}.{}.{}", self.version.0, self.version.1, self.version.2)
    }
}

#[derive(StructOpt, Debug)]
#[structopt(name = "basic")]
struct Opt {
    #[structopt(name = "args")]
    args: Vec<Arg>, // ie "install", "python" etc.
}

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
                dependencies.push(Package {
                    name: "saturn".into(),
                    version: (0, 3, 5),
                    version_type: VersionType::OrHigher,
                });
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
fn create_venv(py_alias: &str, venv_path: &path::PathBuf) {
    // let home = dirs::home_dir().expect("Can't find home");

    // let venvs_path = project_dir.join(".virtualenvs");
    // let venv_path = home.join(format!(".virtualenvs/{}", env_name));

    // fs::create_dir(venv_path).expect("Problem creating virtual env folder");
    Command::new(py_alias)
        // .current_dir(venv_path)
        .args(&["-m", "venv", ".venv"])
        .spawn()
        .expect("Problem creating the virtual environment");
}

fn venv_exists(venv_path: &path::PathBuf) -> bool {
    venv_path.exists()
}

fn activate_venv(venv_path: &path::PathBuf) {
    Command::new("source")
        // todo: path to activate may be diff on differnet OSes
        .current_dir(venv_path)
        .args(&["bin/activate"])
        .spawn()
        .expect("Problem activating the virtual environment");
}

fn deactivate_venv(venv_path: &path::PathBuf) {
    // todo: Make this and other errors caused by user input neater; Ie
    // don't show anything Rust specific.
    Command::new("deactivate")
        .spawn()
        .expect("Problem deactivating the virtual environment");
}

fn install_all(py_alias: &str, venv_path: &path::PathBuf, packages: &Vec<Package>) {
    for package in packages {
        let name = package.clone().name_with_version();
        let args = vec!["-m", "pip", "install", &name];
        
        Command::new("python")
            .args(&args)
            .spawn()
            .expect("Problem installing packages from Python.toml");
    }
}

fn main() {
    let opt = Opt::from_args();
    let config = Config::from_file("Python.toml");
    let py_alias = find_py_alias(config.py_version);
    let project_dir = env::current_dir().expect("Can't find current path");
    let venv_path = project_dir.join(".venv");

    for arg in opt.args.iter() {
        match arg {
            Arg::Install => {
                if venv_exists(&venv_path) {
                    activate_venv(&venv_path);

                    // todo: Only if no specific thign specified.
                    install_all(&py_alias, &venv_path, &config.dependencies);

                    deactivate_venv(&venv_path);
                } else {
                    create_venv(&py_alias, &venv_path);
                }
            }
            _ => (),
        }
    }
}
