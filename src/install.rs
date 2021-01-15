use crate::util::print_color;
use crate::{commands, dep_types::Version, util};
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

/// [Cookbook](https://rust-lang-nursery.github.io/rust-cookbook/cryptography/hashing.html)
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
    let setup_text = if let Ok(t) = fs::read_to_string(setup_path) {
        t
    } else {
        util::abort(&format!(
            "Can't find setup.py in this source distribution\
             path: {:?}. This could mean there are no suitable wheels for this package,\
             and there's a problem with its setup.py.",
            setup_path
        ));
        unreachable!()
    };

    let re = Regex::new(r"distutils.core").unwrap();
    let new_text = re.replace_all(&setup_text, "setuptools");

    if new_text != setup_text {
        fs::write(setup_path, new_text.to_string())
            .expect("Problem replacing `distutils.core` with `setuptools` in `setup.py`");
    }
}

/// Remove scripts. Used when uninstalling.
fn remove_scripts(scripts: &[String], scripts_path: &Path) {
    // todo: Likely not a great approach. QC.
    for entry in
        fs::read_dir(scripts_path).expect("Problem reading dist directory when removing scripts")
    {
        let entry = entry.unwrap();
        if !entry.file_type().unwrap().is_file() {
            continue;
        }
        let data = fs::read_to_string(entry.path()).unwrap();
        for script in scripts {
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
        .unwrap_or_else(|_| util::abort(&format!("Problem creating script file for {}", name)));
}

/// Set up entry points (ie scripts like `ipython`, `black` etc) in a single file.
/// Alternatively, we could just parse all `dist-info` folders every run; this should
/// be faster.
pub fn setup_scripts(name: &str, version: &Version, lib_path: &Path, entry_pt_path: &Path) {
    let mut scripts = vec![];
    // todo: Sep fn for dist_info path, to avoid repetition between here and uninstall?
    let mut dist_info_path = lib_path.join(format!("{}-{}.dist-info", name, version.to_string()));
    // If we can't find the dist_info path, it may be due to it not using a full 3-digit semver format.
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

    if !entry_pt_path.exists() && fs::create_dir(&entry_pt_path).is_err() {
        util::abort("Problem creating script path")
    }

    for new_script in scripts {
        let re = Regex::new(r"^(.*?)\s*=\s*(.*?):(.*)$").unwrap();
        if let Some(caps) = re.captures(&new_script) {
            let name = caps.get(1).unwrap().as_str();
            let module = caps.get(2).unwrap().as_str();
            let func = caps.get(3).unwrap().as_str();
            let path = entry_pt_path.join(name);
            make_script(&path, name, module, func);
            // `wheel` is a dependency required internally, but the user doesn't care.
            if name != "wheel" {
                util::print_color(&format!("Added a console script: {}", name), Color::Green);
            }
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
    paths: &util::Paths,
    package_type: PackageType,
    rename: &Option<(u32, String)>,
) -> Result<(), reqwest::Error> {
    if !paths.lib.exists() {
        fs::create_dir(&paths.lib).expect("Problem creating lib directory");
    }
    if !paths.cache.exists() {
        fs::create_dir(&paths.cache).expect("Problem creating cache directory");
    }
    let archive_path = paths.cache.join(filename);

    // If the archive is already in the lib folder, don't re-download it. Note that this
    // isn't the usual flow, but may have some uses.
    if !archive_path.exists() {
        // Save the file
        let mut resp = reqwest::get(url)?; // Download the file
        let mut out =
            fs::File::create(&archive_path).expect("Failed to save downloaded package file");
        // todo: DRY between here and py_versions.
        if let Err(e) = io::copy(&mut resp, &mut out) {
            // Clean up the downloaded file, or we'll get an error next time.
            fs::remove_file(&archive_path).expect("Problem removing the broken file");
            util::abort(&format!("Problem downloading the package archive: {:?}", e));
        }
    }

    let file = util::open_archive(&archive_path);

    // https://rust-lang-nursery.github.io/rust-cookbook/cryptography/hashing.html
    let reader = io::BufReader::new(&file);
    let file_digest = sha256_digest(reader).unwrap_or_else(|_| {
        util::abort(&format!("Problem reading hash for {}", filename));
        unreachable!()
    });

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
        } else {
            util::abort("Exiting due to failed hash");
        }
    }

    // We must re-open the file after computing the hash.
    let archive_file = util::open_archive(&archive_path);

    let rename = match rename.as_ref() {
        Some((_, new)) => Some((name.to_owned(), new.to_owned())),
        None => None,
    };

    match package_type {
        PackageType::Wheel => {
            util::extract_zip(&archive_file, &paths.lib, &rename);
        }
        PackageType::Source => {
            // todo: Support .tar.bz2
            if archive_path.extension().unwrap() == "bz2" {
                util::abort(&format!(
                    "Extracting source packages in the `.bz2` format isn't supported \
                     at this time: {:?}",
                    &archive_path
                ));
            }

            // Extract the tar.gz source code.
            let tar = GzDecoder::new(&archive_file);
            let mut archive = Archive::new(tar);

            // We iterate over and copy entries instead of running `Archive.unpack`, since
            // symlinks in the archive may cause the unpack to break. If this happens, we want
            // to continue unpacking the other files.
            // Overall, this is a pretty verbose workaround!
            match archive.entries() {
                Ok(entries) => {
                    for file in entries {
                        match file {
                            Ok(mut f) => {
                                match f.unpack_in(&paths.lib) {
                                    Ok(_) => (),
                                    Err(e) => {
                                        print_color(
                                            &format!(
                                                "Problem unpacking file {:?}: {:?}",
                                                f.path(),
                                                e
                                            ),
                                            Color::DarkYellow,
                                        );
                                        let f_path =
                                            f.path().expect("Problem getting path from archive");

                                        let filename =
                                            f_path.file_name().expect("Problem getting file name");

                                        // In the `pandocfilters` Python package, the readme file specified in
                                        // `setup.py` is a symlink, which we can't unwrap, and is requried to exist,
                                        // or the wheel build fails. Workaround here; may apply to other packages as well.
                                        if filename
                                            .to_str()
                                            .unwrap()
                                            .to_lowercase()
                                            .contains("readme")
                                            && fs::File::create(&paths.lib.join(f.path().unwrap()))
                                                .is_err()
                                        {
                                            print_color(
                                                "Problem creating dummy readme",
                                                Color::DarkYellow,
                                            );
                                        }
                                    }
                                };
                            }
                            Err(e) => {
                                // todo: dRY while troubleshooting
                                println!(
                                    "Problem opening the tar.gz archive: {:?}: {:?},  checking if it's a zip...",
                                    &archive_file, e
                                );
                                // The extract_wheel function just extracts a zip file, so it's appropriate here.
                                // We'll then continue with this leg, and build/move/cleanup.

                                // Check if we have a zip file instead.
                                util::extract_zip(&archive_file, &paths.lib, &None);
                            }
                        }
                    }
                }
                Err(e) => {
                    println!(
                        "Problem opening the tar.gz archive: {:?}: {:?},  checking if it's a zip...",
                        &archive_file, e
                    );
                    // The extract_wheel function just extracts a zip file, so it's appropriate here.
                    // We'll then continue with this leg, and build/move/cleanup.

                    // Check if we have a zip file instead.
                    util::extract_zip(&archive_file, &paths.lib, &None);
                }
            }

            // The archive is now unpacked into a parent folder from the `tar.gz`. Place
            // its sub-folders directly in the lib folder, and deleten the parent.
            let re = Regex::new(r"^(.*?)(?:\.tar\.gz|\.zip)$").unwrap();
            let folder_name = re
                .captures(filename)
                .expect("Problem matching extracted folder name")
                .get(1)
                .unwrap_or_else(|| {
                    util::abort(&format!(
                        "Unable to find extracted folder name: {}",
                        filename
                    ));
                    unreachable!()
                })
                .as_str();

            // todo: This fs_extras move does a full copy. Normal fs lib doesn't include
            // todo moves, only copies. Figure out how to do a normal move,
            // todo, to speed this up.

            let extracted_parent = paths.lib.join(folder_name);

            replace_distutils(&extracted_parent.join("setup.py"));

            #[cfg(target_os = "windows")]
            {
                let output = Command::new(paths.bin.join("python"))
                    .current_dir(&extracted_parent)
                    .args(&["setup.py", "bdist_wheel"])
                    .output()
                    .unwrap_or_else(|_| {
                        panic!(
                            "Problem running setup.py bdist_wheel in folder: {:?}. Py path: {:?}",
                            &extracted_parent,
                            paths.bin.join("python")
                        )
                    });
                util::check_command_output_with(&output, |s| {
                    panic!(
                        "running setup.py bdist_wheel in folder {:?}. Py path: {:?}: {}",
                        &extracted_parent,
                        paths.bin.join("python"),
                        s
                    );
                });
            }
            // The Linux and Mac builds appear to be unable to build wheels due to
            // missing the ctypes library; revert to system python.
            #[cfg(target_os = "linux")]
            {
                let output = Command::new("python3")
                    .current_dir(&extracted_parent)
                    .args(&["setup.py", "bdist_wheel"])
                    .output()
                    .unwrap_or_else(|_| {
                        panic!(
                            "Problem running setup.py bdist_wheel in folder: {:?}. Py path: {:?}",
                            &extracted_parent,
                            paths.bin.join("python")
                        )
                    });
                util::check_command_output_with(&output, |s| {
                    panic!(
                        "running setup.py bdist_wheel in folder {:?}. Py path: {:?}: {}",
                        &extracted_parent,
                        paths.bin.join("python"),
                        s
                    );
                });
            }
            #[cfg(target_os = "macos")]
            {
                let output = Command::new("python3")
                    .current_dir(&extracted_parent)
                    .args(&["setup.py", "bdist_wheel"])
                    .output()
                    .unwrap_or_else(|_| {
                        panic!(
                            "Problem running setup.py bdist_wheel in folder: {:?}. Py path: {:?}",
                            &extracted_parent,
                            paths.bin.join("python")
                        )
                    });
                util::check_command_output_with(&output, |s| {
                    panic!(
                        "running setup.py bdist_wheel in folder {:?}. Py path: {:?}: {}",
                        &extracted_parent,
                        paths.bin.join("python"),
                        s
                    );
                });
            }

            let dist_path = &extracted_parent.join("dist");
            if !dist_path.exists() {
                #[cfg(target_os = "windows")]
                let error = &format!(
                    "Problem building {} from source. \
                 This may occur if a package that requires compiling has no wheels available \
                 for Windows, and the system is missing dependencies required to compile it, \
                 or if on WSL and installing to a mounted directory.",
                    name
                );

                #[cfg(target_os = "linux")]
                let error = format!(
                    "Problem building {} from source. \
                 This may occur if a package that requires compiling has no wheels available \
                 for this OS and this system is missing dependencies required to compile it.\
                 Try running `pip install --upgrade wheel`, then try again",
                    name
                );
                #[cfg(target_os = "macos")]
                let error = format!(
                    "Problem building {} from source. \
                 This may occur if a package that requires compiling has no wheels available \
                 for this OS and this system is missing dependencies required to compile it.
                 Try running `pip install --upgrade wheel`, then try again",
                    name
                );

                util::abort(&error);
            }

            let built_wheel_filename = util::find_first_file(dist_path)
                .file_name()
                .expect("Unable to find built wheel filename")
                .to_str()
                .unwrap()
                .to_owned();

            let moved_path = paths.lib.join(&built_wheel_filename);

            // todo: Again, try to move vice copy.
            let options = fs_extra::file::CopyOptions::new();
            fs_extra::file::move_file(dist_path.join(&built_wheel_filename), &moved_path, &options)
                .expect("Problem copying wheel built from source");

            let file_created = fs::File::open(&moved_path).expect("Can't find created wheel.");
            util::extract_zip(&file_created, &paths.lib, &rename);

            // Remove the created and moved wheel
            if fs::remove_file(moved_path).is_err() {
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
    setup_scripts(name, version, &paths.lib, &paths.entry_pt);

    Ok(())
}

pub fn uninstall(name_ins: &str, vers_ins: &Version, lib_path: &Path) {
    #[cfg(target_os = "windows")]
    println!("Uninstalling {}: {}...", name_ins, vers_ins.to_string());
    #[cfg(target_os = "linux")]
    println!("🗑 Uninstalling {}: {}...", name_ins, vers_ins.to_string());
    #[cfg(target_os = "macos")]
    println!("🗑 Uninstalling {}: {}...", name_ins, vers_ins.to_string());

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
        if fs::remove_dir_all(lib_path.join(&folder_name)).is_err() {
            // Some packages include a .py file directly in the lib directory instead of a folder.
            // Check that if removing the folder fails.
            if fs::remove_file(lib_path.join(&format!("{}.py", folder_name))).is_err() {
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
        .unwrap_or(());

    // Remove console scripts.
    remove_scripts(&[name_ins.into()], &lib_path.join("../bin"));
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

/// Clone a git repo of a Python package, and build/install a wheel from it.
/// Or do the same, but with a path instead of git.
pub fn download_and_install_git(
    name: &str,
    url: &str,
    git_path: &Path,
    paths: &util::Paths,
) -> util::Metadata {
    if !git_path.exists() {
        fs::create_dir_all(git_path).expect("Problem creating git path");
    }

    let folder_name = util::standardize_name(name); // todo: Will this always work?
                                                    //    match url {
                                                    //        GitPath::Git(url) => {
                                                    // Download the repo into the pyflow folder.
                                                    // todo: Handle checking if it's current and correct; not just a matching folder
                                                    // todo name.
    if !&git_path.join(&folder_name).exists() && commands::download_git_repo(url, git_path).is_err()
    {
        util::abort(&format!("Problem cloning this repo: {}", url));
    } // todo to keep dl small while troubleshooting.
      //        }
      //        GitPath::Path(path) => {
      //            let f = &git_path.join(&folder_name);
      //            if !&f.exists() {
      //                fs::create_dir(f).expect("Problem creating dir for a path dependency");
      //                let options = fs_extra::dir::CopyOptions::new();
      //                fs_extra::dir::copy(PathBuf::from(path), &git_path, &options)
      //                    .expect("Problem copying path requirement to lib folder");
      //            }
      //        }
      //}

    // Build a wheel from the repo
    let output = Command::new(paths.bin.join("python"))
        // We assume that the module code is in the repo's immediate subfolder that has
        // the package's name.
        .current_dir(&git_path.join(&folder_name))
        .args(&["setup.py", "bdist_wheel"])
        .output()
        .expect("Problem running setup.py bdist_wheel");
    util::check_command_output(&output, "running setup.py bdist_wheel");

    let archive_path = util::find_first_file(&git_path.join(folder_name).join("dist"));
    let filename = archive_path
        .file_name()
        .expect("Problem pulling filename from archive path");

    // We've built the wheel; now move it into the lib path, as we would for a wheel download
    // from Pypi.
    let options = fs_extra::file::CopyOptions::new();
    fs_extra::file::move_file(&archive_path, paths.lib.join(&filename), &options)
        .expect("Problem moving the wheel.");

    let archive_path = &paths.lib.join(&filename);
    let archive_file = util::open_archive(archive_path);

    util::extract_zip(&archive_file, &paths.lib, &None);

    // Use the wheel's name to find the dist-info path, to avoid the chicken-egg scenario
    // of need the dist-info path to find the version.
    let re = Regex::new(r"^(.*?)-(.*?)-.*$").unwrap();
    let dist_info = if let Some(caps) = re.captures(filename.to_str().unwrap()) {
        format!(
            "{}-{}.dist-info",
            caps.get(1).unwrap().as_str(),
            caps.get(2).unwrap().as_str()
        )
    } else {
        util::abort("Unable to find the dist info path from wheel filename");
        unreachable!();
    };

    let metadata = util::parse_metadata(&paths.lib.join(dist_info).join("METADATA")); // todo temp!

    setup_scripts(name, &metadata.version, &paths.lib, &paths.entry_pt);

    // Remove the created and moved wheel
    if fs::remove_file(&archive_path).is_err() {
        util::abort(&format!(
            "Problem removing this wheel built from a git repo: {:?}",
            archive_path
        ));
    }
    metadata
}
