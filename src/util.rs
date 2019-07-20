use std::{path::PathBuf, process};

pub fn get_pypi_metadata(name: &str) {
    // todo this may not have entry pts...
    let url = format!("https://pypi.org/pypi/{}/json", name);
}

/// A convenience function
pub fn exit_early(message: &str) {
    {
        println!("{}", message);
        process::exit(1)
    }
}

pub fn venv_exists(bin_path: &PathBuf) -> bool {
    bin_path.join("python").exists() && bin_path.join("pip").exists()
}
