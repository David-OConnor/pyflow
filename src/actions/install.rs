use std::path::Path;

use termcolor::Color;

use crate::{
    dep_types::{LockPackage, Version},
    util::{self, process_reqs, Os, Paths},
    Config,
};

use util::deps::sync;

// TODO: Refactor this function
#[allow(clippy::too_many_arguments)]
pub fn install(
    cfg_path: &Path,
    cfg: &Config,
    git_path: &Path,
    paths: &Paths,
    found_lock: bool,
    packages: &[String],
    dev: bool,
    lockpacks: &[LockPackage],
    os: &Os,
    py_vers: &Version,
    lock_path: &Path,
) {
    if !cfg_path.exists() {
        cfg.write_file(cfg_path);
    }

    if found_lock {
        util::print_color("Found lockfile", Color::Green);
    }

    // Merge reqs added via cli with those in `pyproject.toml`.
    let (updated_reqs, up_dev_reqs) = util::merge_reqs(packages, dev, cfg, cfg_path);

    let dont_uninstall = util::find_dont_uninstall(&updated_reqs, &up_dev_reqs);

    let updated_reqs = process_reqs(updated_reqs, git_path, paths);
    let up_dev_reqs = process_reqs(up_dev_reqs, git_path, paths);

    sync(
        paths,
        lockpacks,
        &updated_reqs,
        &up_dev_reqs,
        &dont_uninstall,
        *os,
        py_vers,
        lock_path,
    );
    util::print_color("Installation complete", Color::Green);
}
