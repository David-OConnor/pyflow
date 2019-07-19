use regex::Regex;
use std::{env, fs, path, process::Command};
use std::{error::Error, fmt};

#[derive(Debug)]
struct ExecutionError {
    details: String,
}

impl Error for ExecutionError {
    fn description(&self) -> &str {
        &self.details
    }
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

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
    bin_path: &path::PathBuf,
    packages: &[crate::Package],
    uninstall: bool,
) -> Result<(), Box<Error>> {
    // We don't need an alias from the venv's bin directory; we call the
    // executble directly.
    let install = if uninstall { "uninstall" } else { "install" };

    for package in packages {
        // Even though `bin` contains `pip`, it doesn't appear to work directly.
        Command::new("./python")
            .current_dir(bin_path)
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

// todo have these propogate errors.

pub(crate) fn run_python(bin_path: &path::PathBuf, args: &[String]) {
    set_pythonpath();

    Command::new("./python")
        .current_dir(bin_path)
        .args(args)
        .status()
        .expect("Problem running Python");
}

/// Run a binary installed in the virtual environment, such as `ipython` or `black`.
pub(crate) fn run_bin(bin_path: &path::PathBuf, name: &str, args: &[String]) {
    set_pythonpath();

    Command::new("./".to_string())
        .current_dir(bin_path)
        .args(&["-m", name])
        .args(args)
        .status()
        .expect(&format!("Problem running {}", name));
}
