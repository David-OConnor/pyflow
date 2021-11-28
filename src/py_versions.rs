//! Manages Python installations

use crate::commands;
use crate::dep_types::Version;
use crate::{install, util};
use std::error::Error;
#[allow(unused_imports)]
use std::{fmt, fs, io, path::Path, path::PathBuf};
use termcolor::Color;

/// Only versions we've built and hosted
#[derive(Clone, Copy, Debug)]
enum PyVers {
    V3_12_0, // unreleased
    V3_11_0, // unreleased
    V3_10_0, // unreleased
    V3_9_0,  // either Os
    V3_8_0,  // either Os
    V3_7_4,  // Either Os
    V3_6_9,  // Linux
    V3_6_8,  // Win
    V3_5_7,  // Linux
    V3_5_4,  // Win
    V3_4_10, // Linux
}

/// Reduces code repetition for error messages related to Python binaries we don't support.
fn abort_helper(version: &str, os: &str) {
    util::abort(&format!(
        "Automatic installation of Python {} on {} is currently unsupported. If you'd like \
         to use this version of Python, please install it.",
        version, os
    ))
}

impl From<(Version, Os)> for PyVers {
    fn from(v_o: (Version, Os)) -> Self {
        let unsupported = "Unsupported python version requested; only Python ≥ 3.4 is supported. \
        to fix this, edit the `py_version` line of `pyproject.toml`, or run `pyflow switch 3.7`";
        if v_o.0.major != Some(3) {
            util::abort(unsupported)
        }
        match v_o.0.minor.unwrap_or(0) {
            4 => match v_o.1 {
                Os::Windows => {
                    abort_helper("3.4", "Windows");
                    unreachable!()
                }
                Os::Ubuntu | Os::Centos => Self::V3_4_10,
                _ => {
                    abort_helper("3.4", "Mac");
                    unreachable!()
                }
            },
            5 => match v_o.1 {
                Os::Windows => Self::V3_5_4,
                Os::Ubuntu | Os::Centos => Self::V3_5_7,
                _ => {
                    abort_helper("3.5", "Mac");
                    unreachable!()
                }
            },
            6 => match v_o.1 {
                Os::Windows => Self::V3_6_8,
                Os::Ubuntu | Os::Centos => Self::V3_6_9,
                _ => {
                    abort_helper("3.6", "Mac");
                    unreachable!()
                }
            },
            7 => match v_o.1 {
                Os::Windows | Os::Ubuntu | Os::Centos => Self::V3_7_4,
                _ => {
                    abort_helper("3.7", "Mac");
                    unreachable!()
                }
            },
            8 => match v_o.1 {
                Os::Windows | Os::Ubuntu | Os::Centos => Self::V3_8_0,
                _ => {
                    abort_helper("3.8", "Mac");
                    unreachable!()
                }
            },
            9 => match v_o.1 {
                Os::Windows | Os::Ubuntu | Os::Centos => Self::V3_9_0,
                _ => {
                    abort_helper("3.9", "Mac");
                    unreachable!()
                }
            },
            10 => match v_o.1 {
                Os::Windows | Os::Ubuntu | Os::Centos => Self::V3_10_0,
                _ => {
                    abort_helper("3.10", "Mac");
                    unreachable!()
                }
            },
            11 => match v_o.1 {
                Os::Windows | Os::Ubuntu | Os::Centos => Self::V3_11_0,
                _ => {
                    abort_helper("3.11", "Mac");
                    unreachable!()
                }
            },
            12 => match v_o.1 {
                Os::Windows | Os::Ubuntu | Os::Centos => Self::V3_12_0,
                _ => {
                    abort_helper("3.12", "Mac");
                    unreachable!()
                }
            },
            _ => {
                util::abort(unsupported)
            }
        }
    }
}

impl ToString for PyVers {
    fn to_string(&self) -> String {
        match self {
            Self::V3_12_0 => "3.12.0".into(),
            Self::V3_11_0 => "3.11.0".into(),
            Self::V3_10_0 => "3.10.0".into(),
            Self::V3_9_0 => "3.9.0".into(),
            Self::V3_8_0 => "3.8.0".into(),
            Self::V3_7_4 => "3.7.4".into(),
            Self::V3_6_9 => "3.6.9".into(),
            Self::V3_6_8 => "3.6.8".into(),
            Self::V3_5_7 => "3.5.7".into(),
            Self::V3_5_4 => "3.5.4".into(),
            Self::V3_4_10 => "3.4.10".into(),
        }
    }
}

impl PyVers {
    fn to_vers(self) -> Version {
        match self {
            Self::V3_12_0 => Version::new(3, 12, 0),
            Self::V3_11_0 => Version::new(3, 11, 0),
            Self::V3_10_0 => Version::new(3, 10, 0),
            Self::V3_9_0 => Version::new(3, 9, 0),
            Self::V3_8_0 => Version::new(3, 8, 0),
            Self::V3_7_4 => Version::new(3, 7, 4),
            Self::V3_6_9 => Version::new(3, 6, 9),
            Self::V3_6_8 => Version::new(3, 6, 8),
            Self::V3_5_7 => Version::new(3, 5, 7),
            Self::V3_5_4 => Version::new(3, 5, 4),
            Self::V3_4_10 => Version::new(3, 4, 10),
        }
    }
}

/// Only Oses we've built and hosted
/// todo: How cross-compat are these? Eg work across diff versions of Ubuntu?
/// todo: 32-bit
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
enum Os {
    // Don't confuse with crate::Os
    Ubuntu, // Builds on Ubuntu 18.04 work on Ubuntu 19.04, Debian, Arch, and Kali
    Centos, // Will this work on Red Hat and Fedora as well?
    Windows,
    Mac,
}

/// For use in the Linux distro prompt
impl fmt::Display for Os {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Ubuntu => "Ubuntu",
                Self::Centos => "Centos",
                Self::Windows => "Windows",
                Self::Mac => "Mac",
            }
        )
    }
}

fn download(py_install_path: &Path, version: &Version) {
    // We use the `.xz` format due to its small size compared to `.zip`. On order half the size.
    let os;
    let os_str;
    #[cfg(target_os = "windows")]
    {
        os = Os::Windows;
        os_str = "windows";
    }
    #[cfg(target_os = "linux")]
    {
        let result = util::prompts::list(
            "Please enter the number corresponding to your Linux distro:",
            "Linux distro",
            &[
                (
                    "2016 or newer (Ubuntu≥16.04, Debian≥9, SUSE≥15, Arch, Kali, etc)".to_owned(),
                    Os::Ubuntu,
                ),
                (
                    "Older (Centos, Redhat, Fedora, older versions of distros listed in option 1)"
                        .to_owned(),
                    Os::Centos,
                ),
            ],
            false,
        );
        os = result.1;
        os_str = match os {
            Os::Ubuntu => "ubuntu",
            Os::Centos => "centos",
            _ => {
                util::abort(
                    "Unfortunately, we don't yet support other Operating systems.\
                     It's worth trying the other options, to see if one works anyway.",
                );
                unreachable!()
            } //            _ => panic!("If you're seeing this, the code is in what I thought was an unreachable\
              //            state. I could give you advice for what to do. But honestly, why should you trust me?\
              //            I clearly screwed this up. I'm writing a message that should never appear, yet\
              //            I know it will probably appear someday. On a deep level, I know I'm not up to this tak.\
              //            I'm so sorry.")
        };
    }
    #[cfg(target_os = "macos")]
    {
        os = Os::Mac;
        os_str = "mac";
    }

    // Match up our version to the closest match (major+minor will match) we've built.
    let vers_to_dl2: PyVers = (version.clone(), os).into();
    let vers_to_dl = vers_to_dl2.to_string();

    let url = format!(
        "https://github.com/David-OConnor/pybin/releases/\
         download/{}/python-{}-{}.tar.xz",
        vers_to_dl, vers_to_dl, os_str
    );

    // eg `python-3.7.4-ubuntu.tar.xz`
    let archive_path = py_install_path.join(&format!("python-{}-{}.tar.xz", vers_to_dl, os_str));
    if !archive_path.exists() {
        // Save the file
        util::print_color(
            &format!("Downloading Python {}...", vers_to_dl),
            Color::Cyan,
        );
        let mut resp = reqwest::get(&url).expect("Problem downloading Python"); // Download the file
        let mut out =
            fs::File::create(&archive_path).expect("Failed to save downloaded Python archive");
        if let Err(e) = io::copy(&mut resp, &mut out) {
            // Clean up the downloaded file, or we'll get an error next time.
            fs::remove_file(&archive_path).expect("Problem removing the broken file");
            util::abort(&format!("Problem downloading the Python archive: {:?}", e));
        }
    }
    util::print_color(&format!("Installing Python {}...", vers_to_dl), Color::Cyan);

    util::unpack_tar_xz(&archive_path, py_install_path);

    // Strip the OS tag from the extracted Python folder name
    let extracted_path = py_install_path.join(&format!("python-{}", vers_to_dl));

    fs::rename(
        py_install_path.join(&format!("python-{}-{}", vers_to_dl, os_str)),
        &extracted_path,
    )
    .expect("Problem renaming extracted Python folder");
}

#[derive(Debug)]
pub struct AliasError {
    pub details: String,
}

impl Error for AliasError {
    fn description(&self) -> &str {
        &self.details
    }
}

impl fmt::Display for AliasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

/// Make an educated guess at the command needed to execute python the
/// current system.  An alternative approach is trying to find python
/// installations.
pub fn find_py_aliases(version: &Version) -> Vec<(String, Version)> {
    let possible_aliases = &[
        "python3.19",
        "python3.18",
        "python3.17",
        "python3.16",
        "python3.15",
        "python3.14",
        "python3.13",
        "python3.12",
        "python3.11",
        "python3.10",
        "python3.9",
        "python3.8",
        "python3.7",
        "python3.6",
        "python3.5",
        "python3.4",
        "python3.3",
        "python3.2",
        "python3.1",
        "python3",
        "python",
        "python2",
    ];

    let mut result = Vec::new();
    let mut found_dets = Vec::new();

    for alias in possible_aliases {
        // We use the --version command as a quick+effective way to determine if
        // this command is associated with Python.
        let dets = commands::find_py_dets(alias);
        if let Some(v) = commands::find_py_version(alias) {
            if v.major == version.major && v.minor == version.minor && !found_dets.contains(&dets) {
                result.push((alias.to_string(), v));
                found_dets.push(dets);
            }
        }
    }
    result
}

// Find versions installed with this tool.
fn find_installed_versions(pyflow_dir: &Path) -> Vec<Version> {
    #[cfg(target_os = "windows")]
    let py_name = "python";
    #[cfg(target_os = "linux")]
    let py_name = "bin/python3";
    #[cfg(target_os = "macos")]
    let py_name = "bin/python3";

    if !&pyflow_dir.exists() && fs::create_dir_all(&pyflow_dir).is_err() {
        util::abort("Problem creating the Pyflow directory")
    }

    let mut result = vec![];
    for entry in pyflow_dir
        .read_dir()
        .expect("Can't open python installs path")
        .flatten()
    {
        if !entry.path().is_dir() {
            continue;
        }

        if let Some(v) = commands::find_py_version(entry.path().join(py_name).to_str().unwrap()) {
            result.push(v);
        }
    }
    result
}

/// Create a new virtual environment, and install `wheel`.
pub fn create_venv(
    cfg_v: &Version,
    pypackages_dir: &Path,
    pyflow_dir: &Path,
    dep_cache_path: &Path,
) -> Version {
    let os;
    let python_name;
    #[allow(unused_mut)]
    let mut py_name;
    #[cfg(target_os = "windows")]
    {
        py_name = "python".to_string();
        os = Os::Windows;
        python_name = "python.exe";
    }
    #[cfg(target_os = "linux")]
    {
        py_name = "bin/python3".to_string();
        os = Os::Ubuntu;
        python_name = "python";
    }
    #[cfg(target_os = "macos")]
    {
        py_name = "bin/python3".to_string();
        os = Os::Mac;
        python_name = "python";
    }

    let mut alias = None;
    let mut alias_path = None;
    let mut py_ver = None;

    // If we find both a system alias, and internal version installed, go with the internal.
    // One's this tool installed
    let installed_versions = find_installed_versions(pyflow_dir);
    for iv in &installed_versions {
        if iv.major == cfg_v.major && iv.minor == cfg_v.minor {
            let folder_name = format!("python-{}", iv.to_string());
            alias_path = Some(pyflow_dir.join(folder_name).join(&py_name));
            py_ver = Some(iv.clone());
            break;
        }
    }

    // todo perhaps move alias finding back into create_venv, or make a
    // todo create_venv_if_doesnt_exist fn.
    // Only search for a system Python if we don't have an internal one.
    // todo: Why did we choose to prioritize portable over system? Perhaps do the
    // todo other way around.
    if py_ver.is_none() {
        let aliases = find_py_aliases(cfg_v);
        match aliases.len() {
            0 => (),
            1 => {
                let r = aliases[0].clone();
                alias = Some(r.0);
                py_ver = Some(r.1);
            }
            _ => {
                //                let r = prompt_alias(&aliases);
                let r = util::prompts::list(
                    "Found multiple compatible Python versions. Please enter the number associated with the one you'd like to use:",
                    "Python alias",
                    &aliases,
                    true,
                );
                alias = Some(r.0);
                py_ver = Some(r.1);
            }
        };
    }

    if py_ver.is_none() {
        // Download and install the appropriate Python binary, if we can't find either a
        // custom install, or on the Path.
        download(pyflow_dir, cfg_v);
        let py_ver2: PyVers = (cfg_v.clone(), os).into();
        py_ver = Some(py_ver2.to_vers());

        let folder_name = format!("python-{}", py_ver2.to_string());

        // We appear to have symlink issues on some builds, where `python3` won't work, but
        // `python3.7` (etc) will. Note that this is no longer applicable once the venv is built,
        // and we're using its `python`.
        #[cfg(target_os = "linux")]
        {
            match py_ver.clone().unwrap().minor.unwrap_or(0) {
                12 => py_name += ".12",
                11 => py_name += ".11",
                10 => py_name += ".10",
                9 => py_name += ".9",
                8 => py_name += ".8",
                7 => py_name += ".7",
                6 => py_name += ".6",
                5 => py_name += ".5",
                4 => py_name += ".4",
                _ => panic!("Invalid python minor version"),
            }
        }

        alias_path = Some(pyflow_dir.join(folder_name).join(py_name));
    }

    let py_ver = py_ver.expect("missing Python version");

    let vers_path = pypackages_dir.join(py_ver.to_string_med());

    let lib_path = vers_path.join("lib");

    if !lib_path.exists() {
        fs::create_dir_all(&lib_path).expect("Problem creating __pypackages__ directory");
    }

    #[cfg(target_os = "windows")]
    println!("Setting up Python...");
    #[cfg(target_os = "linux")]
    println!("🐍 Setting up Python..."); // Beware! Snake may be invisible.
    #[cfg(target_os = "macos")]
    println!("🐍 Setting up Python...");

    // For an alias on the PATH
    if let Some(alias) = alias {
        if commands::create_venv(&alias, &lib_path, ".venv").is_err() {
            util::abort("Problem creating virtual environment");
        }
    // For a Python one we've installed.
    } else if let Some(alias_path) = alias_path {
        if commands::create_venv2(&alias_path, &lib_path, ".venv").is_err() {
            util::abort("Problem creating virtual environment");
        }
    }

    let bin_path = util::find_bin_path(&vers_path);

    util::wait_for_dirs(&[bin_path.join(python_name)])
        .expect("Timed out waiting for venv to be created.");

    // Try 64 first; if not, use 32.
    #[allow(unused_variables)]
    let lib = if vers_path.join(".venv").join("lib64").exists() {
        "lib64"
    } else {
        "lib"
    };

    #[cfg(target_os = "windows")]
    let venv_lib_path = "Lib";
    #[cfg(target_os = "linux")]
    let venv_lib_path = PathBuf::from(lib).join(&format!("python{}", py_ver.to_string_med()));
    #[cfg(target_os = "macos")]
    let venv_lib_path = PathBuf::from(lib).join(&format!("python{}", py_ver.to_string_med()));

    let paths = util::Paths {
        bin: bin_path.clone(),
        lib: vers_path
            .join(".venv")
            .join(venv_lib_path)
            .join("site-packages"),
        entry_pt: bin_path,
        cache: dep_cache_path.to_owned(),
    };

    // We need `wheel` installed to build wheels from source.
    // We use `twine` to upload packages to pypi.
    // Note: This installs to the venv's site-packages, not __pypackages__/3.x/lib.
    let wheel_url = "https://files.pythonhosted.org/packages/00/83/b4a77d044e78ad1a45610eb88f745be2fd2c6d658f9798a15e384b7d57c9/wheel-0.33.6-py2.py3-none-any.whl";

    install::download_and_install_package(
        "wheel",
        &Version::new(0, 33, 6),
        wheel_url,
        "wheel-0.33.6-py2.py3-none-any.whl",
        "f4da1763d3becf2e2cd92a14a7c920f0f00eca30fdde9ea992c836685b9faf28",
        &paths,
        install::PackageType::Wheel,
        &None,
    )
    .expect("Problem installing `wheel`");

    py_ver
}
