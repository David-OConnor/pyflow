use std::path::{Path, PathBuf};

pub fn pyflow_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .expect("Problem finding base directory");
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join("Library").join("Application Support"))
        .expect("Problem finding base directory");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .expect("Problem finding base directory");
    base.join("pyflow")
}

pub fn dep_cache_path(pyflow_path: &Path) -> PathBuf {
    pyflow_path.join("dependency_cache")
}

pub fn script_env_path(pyflow_path: &Path) -> PathBuf {
    pyflow_path.join("script_envs")
}

pub fn git_path(pyflow_path: &Path) -> PathBuf {
    pyflow_path.join("git")
}

pub fn get_paths() -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let pyflow_path = pyflow_path();
    let dep_cache_path = dep_cache_path(&pyflow_path);
    let script_env_path = script_env_path(&pyflow_path);
    let git_path = git_path(&pyflow_path);
    (pyflow_path, dep_cache_path, script_env_path, git_path)
}
