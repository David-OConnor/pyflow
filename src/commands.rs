use regex::Regex;
use std::{env, process::Command};

/// Find the py_version from the `python --py_version` command. Eg: "Python 3.7".
pub(crate) fn find_version(alias: &str) -> crate::Version {
    Command::new(alias)
        .args(&["--version"])
        .output()
        .expect("Problem finding python py_version");

    // todo fix with regex
    crate::Version::new(3, 7, 1) // todo
}

/// Create the virtual env. Assume we're running Python 3.3+, where venv is included.
pub(crate) fn create_venv(py_alias: &str, directory: &str, name: &str) {
    Command::new(py_alias)
        .args(&["-m", "venv", name])
        // todo fix this!
        .current_dir(directory)
        .spawn()
        .expect("Problem creating the virtual environment");
}

pub(crate) fn install(venv_name: &str, packages: &[crate::Package], uninstall: bool) {
    // We don't need an alias from the venv's bin directory; should
    // always be `python` or `pip`.
    let install = if uninstall { "uninstall" } else { "install" };

    // todo: this path setup may be linux specific. Make it more generic.
    for package in packages {
        // Even though `bin` contains `pip`, it doesn't appear to work directly.
        Command::new("./python")
            .current_dir(&format!("{}/bin", venv_name))
            .args(&[
                "-m",
                "pip",
                install,
                &package.name_with_version(),
                "--target",
                "../../lib",
            ])
            .status()
            .unwrap_or_else(|_| panic!("Problem {}ing these packages: {:#?}", install, packages));
    }
}

pub(crate) fn run_python(venv_name: &str, script: &Option<String>, ipython: bool) {
    // todo: this path setup may be linux specific. Make it more generic.
    let name = if ipython { "ipython" } else { "python" };
    let venv = format!("{}/bin", venv_name);

    env::set_var(
        "PYTHONPATH",
        env::current_dir()
            .expect("Problem finding current directory")
            .join("__pypackages__/3.7/lib") // todo version hack
            .to_str()
            .expect("Problem converting current path to string"),
    );

    match script {
        Some(filename) => {
            Command::new("./".to_string() + name)
                .current_dir(venv)
                .arg(filename)
                .status()
                .unwrap_or_else(|_| panic!("Problem running Python with {}", filename));
        }
        None => {
            Command::new("./".to_string() + name)
                .current_dir(venv)
                .status()
                .expect("Problem running Python");
        }
    }
}

//// todo consolidate this (and others) with run python or run_general?
pub(crate) fn run_pip(venv_name: &str, args: &[String]) {
    // todo: this path setup may be linux specific. Make it more generic.
    Command::new("./pip")
        .current_dir(&format!("{}/bin", venv_name))
        .arg("--target")
        .arg("../lib")
        .args(args)
        .status()
        .expect("Problem running Pip");
}

// Run a general task not specialized to this package.  First, attempt to run a command by
// that name in the bin directory. Useful for pip, ipython, and other environment-specific
// Python tools.
//pub(crate) fn run_general(venv_name: &str, args: &Vec<String>) {
//    // todo: this path setup may be linux specific. Make it more generic.
//
//    // See if the first arg is something we can run
//    let first = args.get(0).expect("args is empty");
//    let env_specific = Command::new(("./".to_string() + first)
//        .current_dir(&format!("{}/bin", venv_name))
//        .args(args)
//        .status();
//
//    match env_specific {
//        Ok(_) => (),
//        // Just run a normal command.
//        Err(error) => {
//            Command::new("bash")
//                .current_dir(&format!("{}/bin", venv_name))
//                .arg("-c")
//                .args(args)
//                .status()
//                .expect("Problem running Python");
//        }
//    }
//}
