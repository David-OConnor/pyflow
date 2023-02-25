use crate::util::{self, abort, success};
use std::{fs, path::Path};

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
pub fn clear(pyflow_path: &Path, cache_path: &Path, script_env_path: &Path) {
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
            if fs::remove_dir_all(&cache_path).is_err() {
                abort(&format!(
                    "Problem removing the dependency-cache path: {:?}",
                    cache_path
                ));
            }
        }
        ClearChoice::ScriptEnvs => {
            if fs::remove_dir_all(&script_env_path).is_err() {
                abort(&format!(
                    "Problem removing the script env path: {:?}",
                    script_env_path
                ));
            }
        }
        ClearChoice::PyInstalls => {}
        ClearChoice::All => {
            if fs::remove_dir_all(&pyflow_path).is_err() {
                abort(&format!(
                    "Problem removing the Pyflow path: {:?}",
                    pyflow_path
                ));
            }
        }
    }
    success("Cache is cleared")
}
