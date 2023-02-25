use crate::{
    build,
    dep_types::{LockPackage, Version},
    util::{self, deps::sync},
};
use std::path::Path;

pub fn package(
    paths: &util::Paths,
    lockpacks: &[LockPackage],
    os: util::Os,
    py_vers: &Version,
    lock_path: &Path,
    cfg: &crate::Config,
    extras: &[String],
) {
    sync(
        paths,
        lockpacks,
        &cfg.reqs,
        &cfg.dev_reqs,
        &util::find_dont_uninstall(&cfg.reqs, &cfg.dev_reqs),
        os,
        py_vers,
        lock_path,
    );

    build::build(lockpacks, paths, cfg, extras)
}
