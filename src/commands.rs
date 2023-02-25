use crate::util;
use regex::Regex;
use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

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

/// Todo: Dry from `find_py_version`
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

/// Find the Python version from the `python --py_version` command. Eg: "Python 3.7".
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
pub fn create_venv(py_alias: &str, lib_path: &Path, name: &str) -> Result<(), Box<dyn Error>> {
    // While creating the lib path, we're creating the __pypackages__ structure.
    let output = Command::new(py_alias)
        .args(&["-m", "venv", name])
        .current_dir(lib_path.join("../"))
        .output()?;
    util::check_command_output(&output, "creating virtual environment");

    Ok(())
}

// todo: DRY for using a path instead of str. use impl Into<PathBuf> ?
pub fn create_venv2(py_alias: &Path, lib_path: &Path, name: &str) -> Result<(), Box<dyn Error>> {
    // While creating the lib path, we're creating the __pypackages__ structure.
    let output = Command::new(py_alias)
        .args(&["-m", "venv", name])
        .current_dir(lib_path.join("../"))
        .output()?;
    util::check_command_output(&output, "creating virtual environment");

    Ok(())
}

pub fn run_python(
    bin_path: &Path,
    lib_paths: &[PathBuf],
    args: &[String],
) -> Result<(), Box<dyn Error>> {
    util::set_pythonpath(lib_paths);
    Command::new(bin_path.join("python"))
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()?;
    Ok(())
}

pub fn download_git_repo(repo: &str, dest_path: &Path) -> Result<(), Box<dyn Error>> {
    // todo: Download directly instead of using git clone?
    // todo: Suppress this output.
    if Command::new("git").arg("--version").status().is_err() {
        util::abort("Can't find Git on the PATH. Is it installed?");
    }

    let output = Command::new("git")
        .current_dir(dest_path)
        .args(&["clone", repo])
        .output()?;
    util::check_command_output(&output, "cloning repo");
    Ok(())
}

/// Initialize a new git repo.
pub fn git_init(dir: &Path) -> Result<(), Box<dyn Error>> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(&["init", "--quiet"])
        .output()?;
    util::check_command_output(&output, "initializing git repository");
    Ok(())
}
