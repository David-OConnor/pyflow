use std::path::PathBuf;

pub fn pyflow_path() -> PathBuf {
    directories::BaseDirs::new()
        .expect("Problem finding base directory")
        .data_dir()
        .to_owned()
        .join("pyflow")
}

pub fn dep_cache_path(pyflow_path: &PathBuf) -> PathBuf {
    pyflow_path.join("dependency_cache")
}

pub fn script_env_path(pyflow_path: &PathBuf) -> PathBuf {
    pyflow_path.join("script_envs")
}

pub fn git_path(pyflow_path: &PathBuf) -> PathBuf {
    pyflow_path.join("git")
}

pub fn get_paths() -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let pyflow_path = pyflow_path();
    let dep_cache_path = dep_cache_path(&pyflow_path);
    let script_env_path = script_env_path(&pyflow_path);
    let git_path = git_path(&pyflow_path);
    (pyflow_path, dep_cache_path, script_env_path, git_path)
}
