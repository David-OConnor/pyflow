use regex::Regex;
use std::error::Error;
use std::{env, fs, path, process::Command};

/// Sets the `PYTHONPATH` environment variable, causing Python to look for
/// dependencies in `__pypackages__`,
fn set_pythonpath() {
    env::set_var(
        "PYTHONPATH",
        env::current_dir()
            .expect("Problem finding current directory")
            .join("__pypackages__/3.7/lib") // todo version hack
            .to_str()
            .expect("Problem converting current path to string"),
    );
}

/// Find the py_version from the `python --py_version` command. Eg: "Python 3.7".
pub(crate) fn find_py_version(alias: &str) -> Option<crate::Version> {
    let output = Command::new(alias).arg("--version").output();
    let output_bytes = match output {
        Ok(ob) => {
            // Old versions of python output `--version` to `stderr`; newer ones to `stdout`,
            // so check both.
            if ob.stdout.is_empty() {
                ob.stderr
            } else {
                ob.stdout
            }
        }
        Err(_) => return None,
    };

    if let Ok(version) = std::str::from_utf8(&output_bytes) {
        let re = Regex::new(r"Python\s+(\d{1,4})\.(\d{1,4})\.(\d{1,4})").unwrap();
        match re.captures(version) {
            Some(caps) => {
                let major = caps.get(1).unwrap().as_str().parse::<u32>().unwrap();
                let minor = caps.get(2).unwrap().as_str().parse::<u32>().unwrap();
                let patch = caps.get(3).unwrap().as_str().parse::<u32>().unwrap();
                Some(crate::Version::new(major, minor, patch))
            }
            None => None,
        }
    } else {
        None
    }
}

/// Create the virtual env. Assume we're running Python 3.3+, where `venv` is included.
/// Additionally, create the __pypackages__ directory if not already created.
pub(crate) fn create_venv(py_alias: &str, directory: &str, name: &str) -> Result<(), Box<Error>> {
    let lib_path = &format!("{}/lib", directory);
    if !path::PathBuf::from(lib_path).exists() {
        fs::create_dir_all(lib_path).expect("Problem creating __pypackages__ directory");
    }

    Command::new(py_alias)
        .args(&["-m", "venv", name])
        .current_dir(directory)
        .spawn()?;

    Ok(())
}

pub(crate) fn install(
    venv_name: &str,
    packages: &[crate::Package],
    uninstall: bool,
) -> Result<(), Box<Error>> {
    // We don't need an alias from the venv's bin directory; we call the
    // executble directly.
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
                &package.to_pip_string(),
                "--target",
                "../../lib",
            ])
            .status()?;
    }

    Ok(())
}

pub(crate) fn run_python(venv_name: &str, args: &[String], ipython: bool) {
    // todo: this path setup may be linux specific. Make it more generic.
    let name = if ipython { "ipython" } else { "python" };
    let venv = format!("{}/bin", venv_name);
    set_pythonpath();

    Command::new("./".to_string() + name)
        .current_dir(venv)
        .args(args)
        .status()
        .expect("Problem running Python");
}

////// todo consolidate this (and others) with run python or run_general?
//pub(crate) fn run_pip(venv_name: &str, args: &[String]) {
//    // todo: this path setup may be linux specific. Make it more generic.
//    set_pythonpath();
//
//    Command::new("./python")
//        .current_dir(&format!("{}/bin", venv_name))
//        .args(&["-m", "pip"])
//        .args(args)
//        .status()
//        .expect("Problem running Pip");
//}

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
