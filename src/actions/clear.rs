use crate::util::{self, abort, paths::PyflowDirs, success};
use std::fs;

#[derive(Clone)]
enum ClearChoice {
    Dependencies,
    ScriptEnvs,
    PyInstalls,
    //    Global,
    All,
}

impl ToString for ClearChoice {
    fn to_string(&self) -> String {
        "".into()
    }
}

/// Clear `Pyflow`'s cache. Allow the user to select which parts to clear based on a prompt.
pub fn clear(pyflow_dirs: &PyflowDirs) {
    let result = util::prompts::list(
        "Which cached items would you like to clear?",
        "choice",
        &[
            ("Downloaded dependencies".into(), ClearChoice::Dependencies),
            (
                "Standalone-script environments".into(),
                ClearChoice::ScriptEnvs,
            ),
            ("Python installations".into(), ClearChoice::PyInstalls),
            ("All of the above".into(), ClearChoice::All),
        ],
        false,
    );

    // todo: DRY
    match result.1 {
        ClearChoice::Dependencies => {
            let dep_cache_path = pyflow_dirs.dep_cache_path();
            if fs::remove_dir_all(dep_cache_path).is_err() {
                abort(&format!(
                    "Problem removing the dependency-cache path: {:?}",
                    dep_cache_path
                ));
            }
        }
        ClearChoice::ScriptEnvs => {
            let script_envs_dir = pyflow_dirs.script_envs_dir();
            if fs::remove_dir_all(&script_envs_dir).is_err() {
                abort(&format!(
                    "Problem removing the script env path: {:?}",
                    script_envs_dir
                ));
            }
        }
        ClearChoice::PyInstalls => {}
        ClearChoice::All => {
            let data_dir = pyflow_dirs.data_dir();
            if fs::remove_dir_all(&data_dir).is_err() {
                abort(&format!(
                    "Problem removing the Pyflow data directory: {:?}",
                    data_dir
                ));
            }
            let cache_dir = pyflow_dirs.cache_dir();
            if data_dir != cache_dir {
                if fs::remove_dir_all(&cache_dir).is_err() {
                    abort(&format!(
                        "Problem removing the Pyflow cache directory: {:?}",
                        cache_dir
                    ));
                }
            }
            let state_dir = pyflow_dirs.state_dir();
            if data_dir != state_dir && cache_dir != state_dir {
                if fs::remove_dir_all(&state_dir).is_err() {
                    abort(&format!(
                        "Problem removing the Pyflow state directory: {:?}",
                        state_dir
                    ));
                }
            }
        }
    }
    success("Cache is cleared")
}
