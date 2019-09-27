use crate::util;
use regex::Regex;
//use std::process::Stdio;
use std::{error::Error, fmt};
use std::{path::Path, process::Command};

#[derive(Debug)]
struct _ExecutionError {
    details: String,
}

impl Error for _ExecutionError {
    fn description(&self) -> &str {
        &self.details
    }
}

impl fmt::Display for _ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

/// Todo: Dry from find_py_version
pub fn find_py_dets(alias: &str) -> Option<String> {
    let output = Command::new(alias).args(&["--version, --version"]).output();

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

    match std::str::from_utf8(&output_bytes) {
        Ok(r) => Some(r.to_owned()),
        Err(_) => None,
    }
}

/// Find the py_version from the `python --py_version` command. Eg: "Python 3.7".
pub fn find_py_version(alias: &str) -> Option<crate::Version> {
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
pub(crate) fn create_venv(
    py_alias: &str,
    lib_path: &Path,
    name: &str,
) -> Result<(), Box<dyn Error>> {
    // While creating the lib path, we're creating the __pypackages__ structure.
    Command::new(py_alias)
        .args(&["-m", "venv", name])
        .current_dir(lib_path.join("../"))
        .output()?;

    Ok(())
}

// todo: DRY for using a path instead of str. use impl Into<PathBuf> ?
pub(crate) fn create_venv2(
    py_alias: &Path,
    lib_path: &Path,
    name: &str,
) -> Result<(), Box<dyn Error>> {
    // While creating the lib path, we're creating the __pypackages__ structure.
    Command::new(py_alias)
        .args(&["-m", "venv", name])
        .current_dir(lib_path.join("../"))
        .output()?;

    Ok(())
}

pub(crate) fn run_python(
    bin_path: &Path,
    lib_path: &Path,
    args: &[String],
) -> Result<(), Box<dyn Error>> {
    util::set_pythonpath(lib_path);
    Command::new(bin_path.join("python")).args(args).status()?;
    Ok(())
}
