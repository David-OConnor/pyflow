use crate::util;
use crate::{dep_types::Version, PackageType};
use crossterm::{Color, Colored};
use flate2::read::GzDecoder;
use regex::Regex;
use ring::digest;
use std::{fs, io, path::PathBuf, process::Command};
use tar::Archive;

/// Extract the wheel. (It's like a zip)
fn install_wheel(file: &fs::File, lib_path: &PathBuf) {
    // Separate function, since we use it twice.
    let mut archive = zip::ZipArchive::new(file).unwrap();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let outpath = lib_path.join(file.sanitized_name());

        if (&*file.name()).ends_with('/') {
            fs::create_dir_all(&outpath).unwrap();
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(&p).unwrap();
                }
            }
            let mut outfile = fs::File::create(&outpath).unwrap();
            io::copy(&mut file, &mut outfile).unwrap();
        }
    }
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
fn replace_distutils(setup_path: &PathBuf) {
    let setup_text =
        fs::read_to_string(setup_path).expect("Can't find setup.py on a source distribution.");

    let re = Regex::new(r"distutils.core").unwrap();
    let new_text = re.replace_all(&setup_text, "setuptools");

    if new_text != setup_text {
        fs::write(setup_path, new_text.to_string())
            .expect("Problem replacing `distutils.core` with `setuptools` in `setup.py`");
    }
}

/// Download and install a package. For wheels, we can just extract the contents into
/// the lib folder.  For source dists, make a wheel first.
pub fn download_and_install_package(
    url: &str,
    filename: &str,
    expected_digest: &str,
    lib_path: &PathBuf,
    bin_path: &PathBuf,
    bin: bool, // todo what is this for?
    package_type: crate::PackageType,
) -> Result<(), reqwest::Error> {
    let mut resp = reqwest::get(url)?; // Download the file
    let archive_path = lib_path.join(filename);

    // Save the file
    let mut out = fs::File::create(&archive_path).expect("Failed to save downloaded package file");
    io::copy(&mut resp, &mut out).expect("failed to copy content");
    let file = fs::File::open(&archive_path).unwrap();

    // https://rust-lang-nursery.github.io/rust-cookbook/cryptography/hashing.html
    let reader = io::BufReader::new(&file);
    let file_digest =
        sha256_digest(reader).expect(&format!("Problem reading hash for {}", filename));

    let file_digest_str = data_encoding::HEXUPPER.encode(file_digest.as_ref());
    if file_digest_str.to_lowercase() != expected_digest.to_lowercase() {
        util::abort(&format!("Hash failed for {}", filename))
    }

    // We must re-open the file after computing the hash.
    let archive_file = fs::File::open(&archive_path).unwrap();

    // todo: Setup executable scripts.

    match package_type {
        PackageType::Wheel => {
            install_wheel(&archive_file, lib_path);
        }
        PackageType::Source => {
            // Extract the tar.gz source code.
            let tar = GzDecoder::new(archive_file);
            let mut archive = Archive::new(tar);
            archive
                .unpack(lib_path)
                .expect("Problem unpacking tar archive");

            // The archive is now unpacked into a parent folder from the `tar.gz`. Place
            // its sub-folders directly in the lib folder, and delete the parent.
            let re = Regex::new(r"^(.*?)\.tar\.gz$").unwrap();
            let folder_name = re
                .captures(&filename)
                .expect("Problem matching extracted folder name")
                .get(1)
                .expect(&format!(
                    "Unable to find extracted folder name: {}",
                    filename
                ))
                .as_str();

            // todo: This fs_extras move does a full copy. Normal fs lib doesn't include
            // todo moves, only copies. Figure out how to do a normal move,
            // todo, to speed this up.

            let extracted_parent = lib_path.join(folder_name);

            replace_distutils(&extracted_parent.join("setup.py"));

            // Build a wheel from source.
            Command::new(format!("{}/python", bin_path.to_str().unwrap()))
                .current_dir(&extracted_parent)
                .args(&["setup.py", "bdist_wheel"])
                .output()
                .expect("Problem running setup.py bdist_wheel");

            let mut built_wheel_filename = String::new();
            for entry in
                fs::read_dir(extracted_parent.join("dist")).expect("Problem reading dist directory")
            {
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
            println!("ex par: {:?}", extracted_parent);
            let options = fs_extra::file::CopyOptions::new();
            fs_extra::file::move_file(
                extracted_parent.join("dist").join(built_wheel_filename),
                lib_path.join(built_wheel_filename),
                &options,
            )
            .expect("Problem copying wheel built from source");

            let file_created = fs::File::open(&lib_path.join(built_wheel_filename))
                .expect("Can't find created wheel.");
            install_wheel(&file_created, lib_path);

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

    // Remove the archive
    if fs::remove_file(&archive_path).is_err() {
        util::abort(&format!(
            "Problem removing this downloaded package: {:?}",
            &archive_path
        ));
    }

    Ok(())
}

pub fn uninstall(name_ins: &str, vers_ins: &Version, lib_path: &PathBuf) {
    println!("Uninstalling {}: {}", name_ins, vers_ins.to_string());
    // Uninstall the package
    // package folders appear to be lowercase, while metadata keeps the package title's casing.
    if fs::remove_dir_all(lib_path.join(name_ins.to_lowercase())).is_err() {
        println!(
            "{}Problem uninstalling {} {}",
            Colored::Fg(Color::DarkRed),
            name_ins,
            vers_ins.to_string(),
        )
    }

    // Only report error if both dist-info and egg-info removal fail.
    let mut meta_folder_removed = false;
    if fs::remove_dir_all(lib_path.join(format!("{}-{}.dist-info", name_ins, vers_ins.to_string())))
        .is_ok()
    {
        meta_folder_removed = true;
    }
    if fs::remove_dir_all(lib_path.join(format!("{}-{}.egg-info", name_ins, vers_ins.to_string())))
        .is_ok()
    {
        meta_folder_removed = true;
    }
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
}
