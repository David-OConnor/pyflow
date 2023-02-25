use crate::{
    dep_resolution::res,
    dep_types::{Constraint, Lock, LockPackage, Package, Rename, Req, ReqType, Version},
    install,
    util::{self, abort},
    PackToInstall,
};
use regex::Regex;
use std::{collections::HashMap, path::Path, str::FromStr};
use termcolor::Color;

/// Function used by `Install` and `Uninstall` subcommands to syn dependencies with
/// the config and lock files.
#[allow(clippy::too_many_arguments)]
pub fn sync(
    paths: &util::Paths,
    lockpacks: &[LockPackage],
    reqs: &[Req],
    dev_reqs: &[Req],
    dont_uninstall: &[String],
    os: util::Os,
    py_vers: &Version,
    lock_path: &Path,
) {
    let installed = util::find_installed(&paths.lib);
    // We control the lock format, so this regex will always match
    let dep_re = Regex::new(r"^(.*?)\s(.*)\s.*$").unwrap();

    // We don't need to resolve reqs that are already locked.
    let locked: Vec<Package> = lockpacks
        .iter()
        .map(|lp| {
            let mut deps = vec![];
            for dep in lp.dependencies.as_ref().unwrap_or(&vec![]) {
                let caps = dep_re
                    .captures(dep)
                    .expect("Problem reading lock file dependencies");
                let name = caps.get(1).unwrap().as_str().to_owned();
                let vers = Version::from_str(caps.get(2).unwrap().as_str())
                    .expect("Problem parsing version from lock");
                deps.push((999, name, vers)); // dummy id
            }

            Package {
                id: lp.id, // todo
                parent: 0, // todo
                name: lp.name.clone(),
                version: Version::from_str(&lp.version).expect("Problem parsing lock version"),
                deps,
                rename: Rename::No, // todo
            }
        })
        .collect();

    // todo: Only show this when needed.
    // todo: Temporarily? Removed.
    // Powershell  doesn't like emojis
    //    #[cfg(target_os = "windows")]
    //    println!("Resolving dependencies...");
    //    #[cfg(target_os = "linux")]
    //    println!("🔍 Resolving dependencies...");
    //    #[cfg(target_os = "macos")]
    //    println!("🔍 Resolving dependencies...");

    // Dev reqs and normal reqs are both installed here; we only commit dev reqs
    // when packaging.
    let mut combined_reqs = reqs.to_vec();
    for dev_req in dev_reqs.to_vec() {
        combined_reqs.push(dev_req);
    }

    let resolved = if let Ok(r) = res::resolve(&combined_reqs, &locked, os, py_vers) {
        r
    } else {
        abort("Problem resolving dependencies")
    };

    // Now merge the existing lock packages with new ones from resolved packages.
    // We have a collection of requirements; attempt to merge them with the already-locked ones.
    let mut updated_lock_packs = vec![];

    for package in &resolved {
        let dummy_constraints = vec![Constraint::new(ReqType::Exact, package.version.clone())];
        if already_locked(&locked, &package.name, &dummy_constraints) {
            let existing: Vec<&LockPackage> = lockpacks
                .iter()
                .filter(|lp| util::compare_names(&lp.name, &package.name))
                .collect();
            let existing2 = existing[0];

            updated_lock_packs.push(existing2.clone());
            continue;
        }

        let deps = package
            .deps
            .iter()
            .map(|(_, name, version)| {
                format!(
                    "{} {} pypi+https://pypi.org/pypi/{}/{}/json",
                    name, version, name, version,
                )
            })
            .collect();

        updated_lock_packs.push(LockPackage {
            id: package.id,
            name: package.name.clone(),
            version: package.version.to_string(),
            source: Some(format!(
                "pypi+https://pypi.org/pypi/{}/{}/json",
                package.name,
                package.version.to_string()
            )),
            dependencies: Some(deps),
            rename: match &package.rename {
                Rename::Yes(parent_id, _, name) => Some(format!("{} {}", parent_id, name)),
                Rename::No => None,
            },
        });
    }

    let updated_lock = Lock {
        //        metadata: Some(lock_metadata),
        metadata: HashMap::new(), // todo: Problem with toml conversion.
        package: Some(updated_lock_packs.clone()),
    };
    if util::write_lock(lock_path, &updated_lock).is_err() {
        abort("Problem writing lock file");
    }

    // Now that we've confirmed or modified the lock file, we're ready to sync installed
    // dependencies with it.
    sync_deps(
        paths,
        &updated_lock_packs,
        dont_uninstall,
        &installed,
        os,
        py_vers,
    );
}
/// Install/uninstall deps as required from the passed list, and re-write the lock file.
fn sync_deps(
    paths: &util::Paths,
    lock_packs: &[LockPackage],
    dont_uninstall: &[String],
    installed: &[(String, Version, Vec<String>)],
    os: util::Os,
    python_vers: &Version,
) {
    let packages: Vec<PackToInstall> = lock_packs
        .iter()
        .map(|lp| {
            (
                (
                    util::standardize_name(&lp.name),
                    Version::from_str(&lp.version).expect("Problem parsing lock version"),
                ),
                lp.rename.as_ref().map(|rn| parse_lockpack_rename(rn)),
            )
        })
        .collect();

    // todo shim. Use top-level A/R. We discard it temporarily while working other issues.
    let installed: Vec<(String, Version)> = installed
        .iter()
        // Don't standardize name here; see note below in to_uninstall.
        .map(|t| (t.0.clone(), t.1.clone()))
        .collect();

    // Filter by not-already-installed.
    let to_install: Vec<&PackToInstall> = packages
        .iter()
        .filter(|(pack, _)| {
            let mut contains = false;
            for inst in &installed {
                if util::compare_names(&pack.0, &inst.0) && pack.1 == inst.1 {
                    contains = true;
                    break;
                }
            }

            // The typing module is sometimes downloaded, causing a conflict/improper
            // behavior compared to the built in module.
            !contains && pack.0 != "typing"
        })
        .collect();

    // todo: Once you include rename info in installed, you won't need to use the map logic here.
    let packages_only: Vec<&(String, Version)> = packages.iter().map(|(p, _)| p).collect();
    let to_uninstall: Vec<&(String, Version)> = installed
        .iter()
        .filter(|inst| {
            // Don't standardize the name here; we need original capitalization to uninstall
            // metadata etc.
            let inst = (inst.0.clone(), inst.1.clone());
            let mut contains = false;
            // We can't just use the contains method, due to needing compare_names().
            for pack in &packages_only {
                if util::compare_names(&pack.0, &inst.0) && pack.1 == inst.1 {
                    contains = true;
                    break;
                }
            }

            for name in dont_uninstall {
                if util::compare_names(name, &inst.0) {
                    contains = true;
                    break;
                }
            }

            !contains
        })
        .collect();

    for (name, version) in &to_uninstall {
        // todo: Deal with renamed. Currently won't work correctly with them.
        install::uninstall(name, version, &paths.lib)
    }

    for ((name, version), rename) in &to_install {
        let data =
            res::get_warehouse_release(name, version).expect("Problem getting warehouse data");

        let (best_release, package_type) =
            util::find_best_release(&data, name, version, os, python_vers);

        // Powershell  doesn't like emojis
        // todo format literal issues, so repeating this whole statement.
        #[cfg(target_os = "windows")]
        util::print_color_(&format!("Installing {}", &name), Color::Cyan);
        #[cfg(target_os = "linux")]
        util::print_color_(&format!("⬇ Installing {}", &name), Color::Cyan);
        #[cfg(target_os = "macos")]
        util::print_color_(&format!("⬇ Installing {}", &name), Color::Cyan);
        println!(" {} ...", &version.to_string_color());

        if install::download_and_install_package(
            name,
            version,
            &best_release.url,
            &best_release.filename,
            &best_release.digests.sha256,
            paths,
            package_type,
            rename,
        )
        .is_err()
        {
            abort("Problem downloading packages");
        }
    }
    // Perform renames after all packages are installed, or we may attempt to rename a package
    // we haven't yet installed.
    for ((name, version), rename) in &to_install {
        if let Some((id, new)) = rename {
            // Rename in the renamed package

            let renamed_path = &paths.lib.join(util::standardize_name(new));

            util::wait_for_dirs(&[renamed_path.clone()]).expect("Problem creating renamed path");
            install::rename_package_files(renamed_path, name, new);

            // Rename in the parent calling the renamed package. // todo: Multiple parents?
            let parent = lock_packs
                .iter()
                .find(|lp| lp.id == *id)
                .expect("Can't find parent calling renamed package");
            install::rename_package_files(
                &paths.lib.join(util::standardize_name(&parent.name)),
                name,
                new,
            );

            // todo: Handle this more generally, in case we don't have proper semver dist-info paths.
            install::rename_metadata(
                &paths
                    .lib
                    .join(&format!("{}-{}.dist-info", name, version.to_string())),
                name,
                new,
            );
        }
    }
}

fn already_locked(locked: &[Package], name: &str, constraints: &[Constraint]) -> bool {
    let mut result = true;
    for constr in constraints.iter() {
        if !locked
            .iter()
            .any(|p| util::compare_names(&p.name, name) && constr.is_compatible(&p.version))
        {
            result = false;
            break;
        }
    }
    result
}

fn parse_lockpack_rename(rename: &str) -> (u32, String) {
    let re = Regex::new(r"^(\d+)\s(.*)$").unwrap();
    let caps = re
        .captures(rename)
        .expect("Problem reading lock file rename");

    let id = caps.get(1).unwrap().as_str().parse::<u32>().unwrap();
    let name = caps.get(2).unwrap().as_str().to_owned();

    (id, name)
}
