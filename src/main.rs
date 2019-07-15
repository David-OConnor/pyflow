// use dirs;
use regex;
use std::{env, fs, process::Command};
use structopt::StructOpt;
use toml;

#[derive(StructOpt, Debug)]
#[structopt(name = "basic")]
struct Opt {
    #[structopt(short = "d", long = "debug")]
    debug: bool,
}

/// Parse the version from a config file, eg "3.7".
fn version_from_str(version: &str) -> (u32, u32) {
    (3, 7)
}

/// Find the version from the `python --version` command. Eg: "Python 3.7".
fn find_version(alias: &str) -> (u32, u32) {
    Command::new(alias)
        .args(&["--version"])
        .output()
        .expect("Problem finding python version");

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
fn create_venv(py_alias: String) {
    // let home = dirs::home_dir().expect("Can't find home");
    let project_dir = env::current_dir().expect("Can't find current path");
    // let venvs_path = project_dir.join(".virtualenvs");
    // let venv_path = home.join(format!(".virtualenvs/{}", env_name));
    let venv_path = project_dir.join(".venv");

    if !venv_path.exists() {
        // fs::create_dir(venv_path).expect("Problem creating virtual env folder");
        Command::new(&py_alias)
        // .current_dir(venv_path)
        .args(&["-m", "venv", ".venv"])
        .spawn()
        .expect("Problem creating virtual environment");
    }
}

fn main() {
    // let opt = Opt::from_args();
    // println!("{:?}", opt);

    let mut config_ver = (0, 0);
    let config = fs::read_to_string("Python.toml");
    match config {
        Ok(data) => {
            let config_data = data
                .parse::<toml::Value>()
                .expect("Problem parsing Python.toml");
            // todo error handle not finding version in toml.
            let config_version_str = &config_data["Python"]["version"]
                .as_str()
                .expect("Problem finding version in Python.toml");

            config_ver = version_from_str(config_version_str);
        }
        Err(_) => panic!("Can't find Python.toml in this directory . Does it exist?"),
    };
  

    let py_alias = find_py_alias(config_ver);
    create_venv(py_alias);

    
}
