use crate::{
    pyproject,
    util::{abort, success},
};
use std::{fs, process};

pub fn reset() {
    let pcfg = pyproject::current::get_config().unwrap_or_else(|| process::exit(1));
    if (&pcfg.pypackages_path).exists() && fs::remove_dir_all(&pcfg.pypackages_path).is_err() {
        abort("Problem removing `__pypackages__` directory")
    }
    if (&pcfg.lock_path).exists() && fs::remove_file(&pcfg.lock_path).is_err() {
        abort("Problem removing `pyflow.lock`")
    }
    success("`__pypackages__` folder and `pyflow.lock` removed")
}
