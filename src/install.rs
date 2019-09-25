use crate::dep_types::Version;
use crate::util;
use crossterm::{Color, Colored};
use flate2::read::GzDecoder;
use regex::Regex;
use ring::digest;
use std::{fs, io, io::BufRead, path::Path, process::Command};
use tar::Archive;

#[derive(Copy, Clone, Debug)]
pub enum PackageType {
    Wheel,
    Source,
}

/// https://rust-lang-nursery.github.io/rust-cookbook/cryptography/hashing.html
fn sha256_digest<R: io::Read>(mut reader: R) -> Result<digest::Digest, std::io::Error> {
    let mut context = digest::Context::new(&digest::SHA256);
    let mut buffer = [0; 1024];

    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        context.update(&buffer[..count]);
    }

    Ok(context.finish())
}

/// If the setup.py file uses `distutils.core`, replace with `setuptools`. This is required to build
/// a wheel. Eg, replace `from distutils.core import setup` with `from setuptools import setup`.
fn replace_distutils(setup_path: &Path) {
    let setup_text =
        fs::read_to_string(setup_path).expect("Can't find setup.py on a source distribution.");

    let re = Regex::new(r"distutils.core").unwrap();
    let new_text = re.replace_all(&setup_text, "setuptools");

    if new_text != setup_text {
        fs::write(setup_path, new_text.to_string())
            .expect("Problem replacing `distutils.core` with `setuptools` in `setup.py`");
    }
}

/// Remove scripts. Used when uninstalling.
fn remove_scripts(scripts: Vec<String>, scripts_path: &Path) {
    // todo: Likely not a great approach. QC.
    for entry in
        fs::read_dir(scripts_path).expect("Problem reading dist directory when removing scripts")
    {
        let entry = entry.unwrap();
        if !entry.file_type().unwrap().is_file() {
            continue;
        }
        let data = fs::read_to_string(entry.path()).unwrap();
        for script in scripts.iter() {
            if data.contains(&format!("from {}", script)) {
                fs::remove_file(entry.path()).expect("Problem removing console script");
                util::print_color(&format!("Removed console script {}:", script), Color::Green);
            }
        }
    }
}

pub fn make_script(path: &Path, name: &str, module: &str, func: &str) {
    let contents = format!(
        r"import re
import sys

from {} import {}

if __name__ == '__main__':
    sys.argv[0] = re.sub(r'(-script\.pyw?|\.exe)?$', '', sys.argv[0])
    sys.exit({}())",
        module, func, func
    );

    fs::write(path, contents)
        .unwrap_or_else(|_| panic!("Problem creating script file for {}", name));
}

/// Set up entry points (ie scripts like `ipython`, `black` etc) in a single file.
/// Alternatively, we could just parse all `dist-info` folders every run; this should
/// be faster.
fn setup_scripts(name: &str, version: &Version, lib_path: &Path) {
    let mut scripts = vec![];
    // todo: Sep fn for dist_info path, to avoid repetition between here and uninstall?
    let mut dist_info_path = lib_path.join(format!("{}-{}.dist-info", name, version.to_string()));
    // If we can't find the dist_info path, it may be due to it not using a full 3-digit semvar format.
    // todo: Dry from dep_resolution, release check.
    if !dist_info_path.exists() && version.patch == 0 {
        dist_info_path = lib_path.join(format!("{}-{}.dist-info", name, version.to_string_med()));
        if !dist_info_path.exists() && version.minor == 0 {
            dist_info_path =
                lib_path.join(format!("{}-{}.dist-info", name, version.to_string_short()));
        }
    }

    if let Ok(ep_file) = fs::File::open(&dist_info_path.join("entry_points.txt")) {
        let mut in_scripts_section = false;
        for line in io::BufReader::new(ep_file).lines() {
            if let Ok(l) = line {
                if l.contains("[console_scripts]") {
                    in_scripts_section = true;
                    continue;
                }
                if l.starts_with('[') {
                    // no longer in scripts section.
                    break;
                }
                if in_scripts_section && !l.is_empty() {
                    // Remove potential leading spaces; have seen indents included.
                    scripts.push(l.clone().replace(" ", ""));
                }
            }
        }
    } // else: Probably no scripts.

    // Now that we've found scripts, add them to our unified file.
    // Note that normally, python uses a bin directory.
    //    // todo: Currently we're setting up the unified file, and the bin/script file.
    //    let scripts_file = &lib_path.join("../console_scripts.txt");
    //    if !scripts_file.exists() {
    //        fs::File::create(scripts_file).expect("Problem creating console_scripts.txt");
    //    }
    //
    //    let mut existing_scripts =
    //        fs::read_to_string(scripts_file).expect("Can't find console_scripts.txt");

    let script_path = lib_path.join("../bin");
    if !script_path.exists() && fs::create_dir(&script_path).is_err() {
        util::abort("Problem creating script path")
    }

    for new_script in scripts {
        let re = Regex::new(r"^(.*?)\s*=\s*(.*?):(.*)$").unwrap();
        if let Some(caps) = re.captures(&new_script) {
            let name = caps.get(1).unwrap().as_str();
            let module = caps.get(2).unwrap().as_str();
            let func = caps.get(3).unwrap().as_str();
            let path = script_path.join(name);
            make_script(&path, name, module, func);
            util::print_color(&format!("Added a console script: {}", name), Color::Green);
        }
    }

    //    fs::write(scripts_file, existing_scripts).expect("Unable to write to the console_scripts file");
}

/// Download and install a package. For wheels, we can just extract the contents into
/// the lib folder.  For source dists, make a wheel first.
pub fn download_and_install_package(
    name: &str,
    version: &Version,
    url: &str,
    filename: &str,
    expected_digest: &str,
    lib_path: &Path,
    bin_path: &Path,
    cache_path: &Path,
    package_type: PackageType,
    rename: &Option<(u32, String)>,
) -> Result<(), reqwest::Error> {
    if !lib_path.exists() {
        fs::create_dir(lib_path).expect("Problem creating lib directory");
    }
    if !cache_path.exists() {
        fs::create_dir(cache_path).expect("Problem creating cache directory");
    }
    let archive_path = cache_path.join(filename);

    // If the archive is already in the lib folder, don't re-download it. Note that this
    // isn't the usual flow, but may have some uses.
    if !archive_path.exists() {
        // Save the file
        let mut resp = reqwest::get(url)?; // Download the file
        let mut out =
            fs::File::create(&archive_path).expect("Failed to save downloaded package file");
        io::copy(&mut resp, &mut out).expect("failed to copy content");
    }

    let file = fs::File::open(&archive_path).unwrap();

    // https://rust-lang-nursery.github.io/rust-cookbook/cryptography/hashing.html
    let reader = io::BufReader::new(&file);
    let file_digest =
        sha256_digest(reader).unwrap_or_else(|_| panic!("Problem reading hash for {}", filename));

    let file_digest_str = data_encoding::HEXUPPER.encode(file_digest.as_ref());
    if file_digest_str.to_lowercase() != expected_digest.to_lowercase() {
        util::print_color(&format!("Hash failed for {}. Expected: {}, Actual: {}. Continue with installation anyway? (yes / no)", filename, expected_digest.to_lowercase(), file_digest_str.to_lowercase()), Color::Red);

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("Unable to read user input Hash fail decision");

        let input = input
            .chars()
            .next()
            .expect("Problem reading input")
            .to_string();

        if input.to_lowercase().contains('y') {
            // todo: Anything?
        } else {
            util::abort("Exiting due to failed hash");
        }
    }

    // We must re-open the file after computing the hash.
    let archive_file = fs::File::open(&archive_path).unwrap();

    // todo: Setup executable scripts.

    let rename = match rename.as_ref() {
        Some((_, new)) => Some((name.to_owned(), new.to_owned())),
        None => None,
    };

    match package_type {
        PackageType::Wheel => {
            util::extract_zip(&archive_file, lib_path, &rename);
        }
        PackageType::Source => {
            // Extract the tar.gz source code.
            let tar = GzDecoder::new(&archive_file);
            let mut archive = Archive::new(tar);

            if archive.unpack(lib_path).is_err() {
                // The extract_wheel function just extracts a zip file, so it's appropriate here.
                // We'll then continue with this leg, and build/move/cleanup.
                util::extract_zip(&archive_file, lib_path, &None);
                // Check if we have a zip file instead.
            }

            // The archive is now unpacked into a parent folder from the `tar.gz`. Place
            // its sub-folders directly in the lib folder, and delete the parent.
            let re = Regex::new(r"^(.*?)(?:\.tar\.gz|\.zip)$").unwrap();
            let folder_name = re
                .captures(&filename)
                .expect("Problem matching extracted folder name")
                .get(1)
                .unwrap_or_else(|| panic!("Unable to find extracted folder name: {}", filename))
                .as_str();

            // todo: This fs_extras move does a full copy. Normal fs lib doesn't include
            // todo moves, only copies. Figure out how to do a normal move,
            // todo, to speed this up.

            let extracted_parent = lib_path.join(folder_name);

            replace_distutils(&extracted_parent.join("setup.py"));

            // Build a wheel from source.
            Command::new(bin_path.join("python"))
                .current_dir(&extracted_parent)
                .args(&["setup.py", "bdist_wheel"])
                .output()
                .expect("Problem running setup.py bdist_wheel");

            // todo: Clippy flags this for not iterating, but I can't get a better way working, ie
            //              let built_wheel_filename = &dist_files.get(0)
            //                .expect("Dist file directory is empty")
            //                .unwrap()
            //                .path()
            //                .file_name()
            //                .expect("Unable to find built wheel filename")
            //                .to_str()
            //                .unwrap()
            //                .to_owned();
            let mut built_wheel_filename = String::new();
            for entry in fs::read_dir(extracted_parent.join("dist")).expect(
                "Problem reading the dist directory of a package built from source. \
                 The `wheel` package have not have been installed in this environment.",
            ) {
                let entry = entry.unwrap();
                built_wheel_filename = entry
                    .path()
                    .file_name()
                    .expect("Unable to find built wheel filename")
                    .to_str()
                    .unwrap()
                    .to_owned();
                break;
            }

            let built_wheel_filename = &built_wheel_filename;
            if built_wheel_filename.is_empty() {
                util::abort("Problem finding built wheel")
            }

            // todo: Again, try to move vice copy.
            let options = fs_extra::file::CopyOptions::new();
            fs_extra::file::move_file(
                extracted_parent.join("dist").join(built_wheel_filename),
                lib_path.join(built_wheel_filename),
                &options,
            )
            .expect("Problem copying wheel built from source");

            let file_created = fs::File::open(&lib_path.join(built_wheel_filename))
                .expect("Can't find created wheel.");
            util::extract_zip(&file_created, lib_path, &rename);

            // Remove the created and moved wheel
            if fs::remove_file(&lib_path.join(built_wheel_filename)).is_err() {
                util::abort(&format!(
                    "Problem removing this downloaded package: {:?}",
                    &built_wheel_filename
                ));
            }
            // Remove the source directeory extracted from the tar.gz file.
            if fs::remove_dir_all(&extracted_parent).is_err() {
                util::abort(&format!(
                    "Problem removing parent folder of this downloaded package: {:?}",
                    &extracted_parent
                ));
            }
        }
    }
    setup_scripts(name, version, lib_path);

    Ok(())
}

pub fn uninstall(name_ins: &str, vers_ins: &Version, lib_path: &Path) {
    #[cfg(target_os = "windows")]
    println!("Uninstalling {}: {}...", name_ins, vers_ins.to_string());
    #[cfg(target_os = "linux")]
    println!(
        "🗑 Uninstalling {}: {}...",
        name_ins,
        vers_ins.to_string()
    );
    #[cfg(target_os = "macos")]
    println!(
        "🗑 Uninstalling {}: {}...",
        name_ins,
        vers_ins.to_string()
    );

    // Uninstall the package
    // package folders appear to be lowercase, while metadata keeps the package title's casing.

    let mut dist_info_path =
        lib_path.join(format!("{}-{}.dist-info", name_ins, vers_ins.to_string()));
    // todo: DRY
    if !dist_info_path.exists() && vers_ins.patch == 0 {
        dist_info_path = lib_path.join(format!(
            "{}-{}.dist-info",
            name_ins,
            vers_ins.to_string_med()
        ));
        if !dist_info_path.exists() && vers_ins.minor == 0 {
            dist_info_path = lib_path.join(format!(
                "{}-{}.dist-info",
                name_ins,
                vers_ins.to_string_short()
            ));
        }
    }

    let egg_info_path = lib_path.join(format!("{}-{}.egg-info", name_ins, vers_ins.to_string()));

    // todo: could top_level.txt be in egg-info too?
    // Sometimes the folder unpacked to isn't the same name as on pypi. Check for `top_level.txt`.
    let folder_names = match fs::File::open(dist_info_path.join("top_level.txt")) {
        Ok(f) => {
            let mut names = vec![];
            for line in io::BufReader::new(f).lines() {
                if let Ok(l) = line {
                    names.push(l);
                }
            }
            names
        }
        Err(_) => vec![name_ins.to_lowercase()],
    };

    for folder_name in folder_names {
        if fs::remove_dir_all(lib_path.join(folder_name)).is_err() {
            // Some packages include a .py file directly in the lib directory instead of a folder.
            // Check that if removing the folder fails.
            if fs::remove_file(lib_path.join(&format!("{}.py", name_ins))).is_err() {
                println!(
                    "{}Problem uninstalling {} {}",
                    Colored::Fg(Color::DarkRed),
                    name_ins,
                    vers_ins.to_string(),
                )
            }
        }
    }

    // Only report error if both dist-info and egg-info removal fail.

    let meta_folder_removed = if fs::remove_dir_all(egg_info_path).is_ok() {
        true
    } else {
        fs::remove_dir_all(dist_info_path).is_ok()
    };

    if !meta_folder_removed {
        println!(
            "{}Problem uninstalling metadata for {}: {}",
            Colored::Fg(Color::DarkRed),
            name_ins,
            vers_ins.to_string(),
        )
    }

    // Remove the data directory, if it exists.
    fs::remove_dir_all(lib_path.join(format!("{}-{}.data", name_ins, vers_ins.to_string())))
        .unwrap_or_else(|_| ());

    // Remove console scripts.
    remove_scripts(vec![name_ins.into()], &lib_path.join("../bin"));
}

/// Rename files in a package. Assume we already renamed the folder, ie during installation.
pub fn rename_package_files(top_path: &Path, old: &str, new: &str) {
    for entry in fs::read_dir(top_path).expect("Problem reading renamed package path") {
        let entry = entry.expect("Problem reading file while renaming");
        let path = entry.path();

        if path.is_dir() {
            rename_package_files(&path, old, new);
            continue;
        }

        if !path.is_file() {
            continue;
        }
        if path.extension().is_none() || path.extension().unwrap() != "py" {
            continue;
        }

        let mut data = fs::read_to_string(&path).expect("Problem reading file while renaming");

        // todo: More flexible with regex?
        data = data.replace(
            &format!("from {} import", old),
            &format!("from {} import", new),
        );
        data = data.replace(&format!("from {}.", old), &format!("from {}.", new));
        data = data.replace(&format!("import {}", old), &format!("import {}", new));
        // Todo: Is this one too general? Supercedes the first. Needed for things like `add_newdoc('numpy.core.multiarray...`
        data = data.replace(&format!("{}.", old), &format!("{}.", new));

        fs::write(path, data).expect("Problem writing file while renaming");
    }

    //     if let Ok(entry) = entry {
    //            if entry.file_type().unwrap().is_dir() {
    //                package_folders.push(entry.file_name())
    //            }
}

/// Rename metadata files.
pub fn rename_metadata(path: &Path, _old: &str, new: &str) {
    // todo: Handle multiple items in top_level. Figure out how to handle that.
    let top_file = path.join("top_level.txt");
    //    let mut top_data = fs::read_to_string(&top_file).expect("Problem opening top_level.txt");

    let top_data = new.to_owned(); // todo fragile.

    fs::write(top_file, top_data).expect("Problem writing file while renaming");

    // todo: Modify other files like entry_points.txt, perhaps.
}
