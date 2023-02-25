use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct PyflowDirs {
    cache_dir: PathBuf,
    data_dir: PathBuf,
    dep_cache_path: PathBuf,
    git_dir: PathBuf,
    py_installs_dir: PathBuf,
    script_envs_dir: PathBuf,
    state_dir: PathBuf,
}

impl PyflowDirs {
    pub fn new() -> Option<Self> {
        let base_dirs = directories::BaseDirs::new()?;
        let data_dir = base_dirs.data_dir().join("pyflow");
        let cache_dir = data_dir.clone();
        let state_dir = data_dir.clone();

        let dep_cache_path = cache_dir.join("dependency_cache");
        let git_dir = state_dir.join("git");
        let py_installs_dir = state_dir.clone();
        let script_envs_dir = state_dir.join("script_envs");

        Some(Self {
            cache_dir,
            data_dir,
            dep_cache_path,
            git_dir,
            py_installs_dir,
            script_envs_dir,
            state_dir,
        })
    }

    pub fn cache_dir(&self) -> &Path {
        self.cache_dir.as_path()
    }
    pub fn data_dir(&self) -> &Path {
        self.data_dir.as_path()
    }
    pub fn dep_cache_path(&self) -> &Path {
        self.dep_cache_path.as_path()
    }
    pub fn git_dir(&self) -> &Path {
        self.git_dir.as_path()
    }
    pub fn py_installs_dir(&self) -> &Path {
        self.py_installs_dir.as_path()
    }
    pub fn script_envs_dir(&self) -> &Path {
        self.script_envs_dir.as_path()
    }
    pub fn state_dir(&self) -> &Path {
        self.state_dir.as_path()
    }
}
