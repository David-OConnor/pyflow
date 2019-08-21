use crate::{
    dep_resolution,
    dep_types::{Constraint, Req, ReqType, Version},
    edit_files,
};
use crossterm::{Color, Colored};
use regex::Regex;
use std::str::FromStr;
use std::{env, path::PathBuf, process, thread, time};

/// Print in a color, then reset formatting.
pub fn print_color(message: &str, color: Color) {
    println!(
        "{}{}{}",
        Colored::Fg(color),
        message,
        Colored::Fg(Color::Reset)
    );
}

/// A convenience function
pub fn abort(message: &str) {
    println!(
        "{}{}{}",
        Colored::Fg(Color::Red),
        message,
        Colored::Fg(Color::Reset)
    );
    process::exit(1)
}

pub fn possible_py_versions() -> Vec<Version> {
    vec![
        "2.0", "2.1", "2.2", "2.3", "2.4", "2.5", "2.6", "2.7", "3.3", "3.4", "3.5", "3.6", "3.7",
        "3.8", "3.9", "3.10", "3.11", "3.12",
    ]
    .into_iter()
    .map(|v| Version::from_str(v).unwrap())
    .collect()
}

pub fn venv_exists(venv_path: &PathBuf) -> bool {
    (venv_path.join("bin/python").exists() && venv_path.join("bin/pip").exists())
        || (venv_path.join("Scripts/python.exe").exists()
            && venv_path.join("Scripts/pip.exe").exists())
}

/// Checks whether the path is under `/bin` (Linux generally) or `/Scripts` (Windows generally)
/// Returns the bin path (ie under the venv)
pub fn find_bin_path(vers_path: &PathBuf) -> PathBuf {
    // The bin name should be `bin` on Linux, and `Scripts` on Windows. Check both.
    // Locate bin name after ensuring we have a virtual environment.
    // It appears that 'binary' scripts are installed in the `lib` directory's bin folder when
    // using the --target arg, instead of the one directly in the env.

    //    if vers_path.join(".venv/bin").exists() {
    //        (vers_path.join(".venv/bin"), vers_path.join("lib/bin"))
    //    } else if vers_path.join(".venv/Scripts").exists() {
    //        // todo: Perhaps the lib path may not be the same.
    //        (
    //            vers_path.join(".venv/Scripts"),
    //            vers_path.join("lib/Scripts"),
    //        )
    //    } else {
    //        // todo: This logic is perhaps sufficient for all cases.
    //        #[cfg(target_os = "windows")]
    //        return (
    //            vers_path.join(".venv/Scripts"),
    //            vers_path.join("lib/Scripts"),
    //        );
    //        #[cfg(target_os = "linux")]
    //        return (vers_path.join(".venv/bin"), vers_path.join("lib/bin"));
    //        #[cfg(target_os = "macos")]
    //        return (vers_path.join(".venv/bin"), vers_path.join("lib/bin"));
    //    }

    #[cfg(target_os = "windows")]
    return vers_path.join(".venv/Scripts");
    #[cfg(target_os = "linux")]
    return vers_path.join(".venv/bin");
    #[cfg(target_os = "macos")]
    return vers_path.join(".venv/bin");
}

/// Wait for directories to be created; required between modifying the filesystem,
/// and running code that depends on the new files.
pub fn wait_for_dirs(dirs: &[PathBuf]) -> Result<(), crate::AliasError> {
    // todo: AliasError is a quick fix to avoid creating new error type.
    let timeout = 1000; // ms
    for _ in 0..timeout {
        let mut all_created = true;
        for dir in dirs {
            if !dir.exists() {
                all_created = false;
            }
        }
        if all_created {
            return Ok(());
        }
        thread::sleep(time::Duration::from_millis(10));
    }
    Err(crate::AliasError {
        details: "Timed out attempting to create a directory".to_string(),
    })
}

/// Sets the `PYTHONPATH` environment variable, causing Python to look for
/// dependencies in `__pypackages__`,
pub fn set_pythonpath(lib_path: &PathBuf) {
    env::set_var(
        "PYTHONPATH",
        lib_path
            .to_str()
            .expect("Problem converting current path to string"),
    );
}

/// List all installed dependencies and console scripts, by examining the `libs` and `bin` folders.
pub fn show_installed(lib_path: &PathBuf) {
    let installed = find_installed(lib_path);
    let scripts = find_console_scripts(&lib_path.join("../bin"));

    print_color("The following packages are installed:", Color::DarkBlue);
    for (name, version) in installed {
        //        print_color(&format!("{} == \"{}\"", name, version.to_string()), Color::Magenta);
        println!(
            "{}{}{} == {}",
            Colored::Fg(Color::Cyan),
            name,
            Colored::Fg(Color::Reset),
            version
        );
    }

    print_color(
        "\nThe following console scripts are installed:",
        Color::DarkBlue,
    );
    for script in scripts {
        print_color(&script, Color::Cyan);
    }
}

/// Find the packages installed, by browsing the lib folder.
pub fn find_installed(lib_path: &PathBuf) -> Vec<(String, Version)> {
    let mut package_folders = vec![];

    if !lib_path.exists() {
        return vec![];
    }
    for entry in lib_path.read_dir().unwrap() {
        if let Ok(entry) = entry {
            if entry.file_type().unwrap().is_dir() {
                package_folders.push(entry.file_name())
            }
        }
    }

    let mut result = vec![];

    for folder in package_folders.iter() {
        let folder_name = folder.to_str().unwrap();
        let re = Regex::new(r"^(.*?)-(.*?)\.dist-info$").unwrap();
        let re_egg = Regex::new(r"^(.*?)-(.*?)\.egg-info$").unwrap();

        if let Some(caps) = re.captures(&folder_name) {
            let name = caps.get(1).unwrap().as_str();
            let vers = Version::from_str(caps.get(2).unwrap().as_str()).unwrap();
            result.push((name.to_owned(), vers));

        // todo dry
        } else if let Some(caps) = re_egg.captures(&folder_name) {
            let name = caps.get(1).unwrap().as_str();
            let vers = Version::from_str(caps.get(2).unwrap().as_str()).unwrap();
            result.push((name.to_owned(), vers));
        }
    }
    result
}

/// Find console scripts installed, by browsing the (custom) bin folder
pub fn find_console_scripts(bin_path: &PathBuf) -> Vec<String> {
    let mut result = vec![];
    if !bin_path.exists() {
        return vec![];
    }

    for entry in bin_path.read_dir().unwrap() {
        if let Ok(entry) = entry {
            if entry.file_type().unwrap().is_file() {
                result.push(entry.file_name().to_str().unwrap().to_owned())
            }
        }
    }
    result
}

/// Handle reqs added via the CLI
pub fn merge_reqs(added: &Vec<String>, cfg: &crate::Config, cfg_filename: &str) -> Vec<Req> {
    let mut added_reqs = vec![];
    for p in added.into_iter() {
        match Req::from_str(&p, false) {
            Ok(r) => added_reqs.push(r),
            Err(_) => abort(&format!("Unable to parse this package: {}. \
                    Note that installing a specific version via the CLI is currently unsupported. If you need to specify a version,\
                     edit `pyproject.toml`", &p)),
        }
    }

    // Reqs to add to `pyproject.toml`
    let mut added_reqs_unique: Vec<Req> = added_reqs
        .into_iter()
        .filter(|ar| {
            // return true if the added req's not in the cfg reqs, or if it is
            // and the version's different.
            let mut add = true;
            for cr in cfg.reqs.iter() {
                if cr == ar
                    || (cr.name.to_lowercase() == ar.name.to_lowercase()
                        && ar.constraints.is_empty())
                {
                    // Same req/version exists
                    add = false;
                    break;
                }
            }
            add
        })
        .collect();

    // If no constraints are specified, use a caret constraint with the latest
    // version.
    for added_req in added_reqs_unique.iter_mut() {
        if added_req.constraints.is_empty() {
            let (formatted_name, vers, _) = dep_resolution::get_version_info(&added_req.name)
                .expect("Problem getting latest version of the package you added.");
            added_req.constraints.push(Constraint::new(
                ReqType::Caret,
                Version::new(vers.major, vers.minor, vers.patch),
            ));
        }
    }

    let mut result = vec![]; // Reqs to sync

    // Merge reqs from the config and added via CLI. If there's a conflict in version,
    // use the added req.
    for cr in cfg.reqs.iter() {
        let mut replaced = false;
        for added_req in added_reqs_unique.iter() {
            if added_req.name == cr.name && added_req.constraints != cr.constraints {
                result.push(added_req.clone());
                replaced = true;
                break;
            }
        }
        if !replaced {
            result.push(cr.clone());
        }
    }

    if !added_reqs_unique.is_empty() {
        edit_files::add_reqs_to_cfg(cfg_filename, &added_reqs_unique);
    }

    result.append(&mut added_reqs_unique);
    result
}
