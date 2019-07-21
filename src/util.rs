use crate::package_types::Version;
use std::{path::PathBuf, process};

pub fn get_pypi_metadata(name: &str) {
    // todo this may not have entry pts...
    let url = format!("https://pypi.org/pypi/{}/json", name);
}

/// A convenience function
pub fn abort(message: &str) {
    {
        println!("{}", message);
        process::exit(1)
    }
}

pub fn possible_py_versions() -> Vec<Version> {
    vec![
        "2.0", "2.1", "2.2", "2.3", "2.4", "2.5", "2.6", "2.7", "3.3", "3.4", "3.5", "3.6", "3.7",
        "3.8", "3.9", "3.10", "3.11", "3.12",
    ]
    .into_iter()
    .map(|v| Version::from_str2(v))
    .collect()
}

pub fn venv_exists(bin_path: &PathBuf) -> bool {
    bin_path.join("python").exists() && bin_path.join("pip").exists()
}

/// Checks whether the path is under `/bin` (Linux generally) or `/Scripts` (Windows generally)
/// Returns the primary bin path (ie under the venv), and the custom one (under `Lib`) as a Tuple.
pub fn find_bin_path(vers_path: &PathBuf) -> (PathBuf, PathBuf) {
    // The bin name should be `bin` on Linux, and `Scripts` on Windows. Check both.
    // Locate bin name after ensuring we have a virtual environment.
    // It appears that 'binary' scripts are installed in the `lib` directory's bin folder when
    // using the --target arg, instead of the one directly in the env.

    if vers_path.join(".venv/bin").exists() {
        (vers_path.join(".venv/bin"), vers_path.join("lib/bin"))
    } else if vers_path.join(".venv/Scripts").exists() {
        // todo: Perhasp the lib path may not be the same.
        (
            vers_path.join(".venv/Scripts"),
            vers_path.join("lib/Scripts"),
        )
    } else {
        // todo: YOu sould probably propogate this as an Error instead of handlign here.
        abort("Can't find the new binary directory. (ie `bin` or `Scripts` in the virtual environment's folder)");
        (vers_path.clone(), vers_path.clone()) // Never executed; used to prevent compile errors.
    }
}
