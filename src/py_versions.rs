//! Manages Python installations

use crate::commands;
use crate::dep_types::Version;
use crate::util;
use crossterm::Color;
use std::error::Error;
use std::{collections::HashMap, fmt, fs, io, path::Path, process};

/// Only versions we've built and hosted
#[derive(Clone, Copy, Debug)]
enum PyVers {
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
        to fix this, edit the `python_version` line of `pyproject.toml`, or run `pyflow switch 3.7`";
        if v_o.0.major != 3 {
            util::abort(unsupported);
            unreachable!()
        }
        // todo: Handle non Ubuntu/Debian
        match v_o.0.minor {
            4 => match v_o.1 {
                Os::Windows => {
                    abort_helper("3.4", "Windows");
                    unreachable!()
                }
                Os::Ubuntu => Self::V3_4_10,
                _ => {
                    abort_helper("3.4", "Mac");
                    unreachable!()
                }
            },
            5 => match v_o.1 {
                Os::Windows => Self::V3_5_4,
                Os::Ubuntu => Self::V3_5_7,
                _ => {
                    abort_helper("3.5", "Mac");
                    unreachable!()
                }
            },
            6 => match v_o.1 {
                Os::Windows => Self::V3_6_8,
                Os::Ubuntu => Self::V3_6_9,
                _ => {
                    abort_helper("3.6", "Mac");
                    unreachable!()
                }
            },
            7 => match v_o.1 {
                Os::Windows => Self::V3_7_4,
                Os::Ubuntu => Self::V3_7_4,
                _ => {
                    abort_helper("3.7", "Mac");
                    unreachable!()
                }
            },
            _ => {
                util::abort(unsupported);
                unreachable!()
            }
        }
    }
}

impl ToString for PyVers {
    fn to_string(&self) -> String {
        match self {
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
/// todo Ubuntu/Debian? Ubuntu/all linux??
/// todo: 32-bit
#[derive(Clone, Copy, Debug)]
enum Os {
    // Don't confuse with crate::Os
    Ubuntu,
    Windows,
    Mac,
}

//impl FromStr for Os {
//    type Err = crate::dep_types::DependencyError;
//
//    fn from_str(s: &str) -> Result<Self, Self::Err> {
//        Ok(match s {
//            "windows" => Os::Any,
//            "linux" => Os::Any,
//            "mac" => Os::Any,
//            _ => {
//                     return Err(crate::DependencyError::new(&format!("Problem parsing Os: {}", s)));
//                }
//        })
//    }
//}

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
        // todo: Support different distros.
        os = Os::Ubuntu;
        os_str = "ubuntu";
    }
    #[cfg(target_os = "macos")]
    {
        os = Os::Mac;
        os_str = "mac";
    }

    // Match up our version to the closest match (major+minor will match) we've built.
    let vers_to_dl2: PyVers = (*version, os).into();
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
            fs::File::create(&archive_path).expect("Failed to save downloaded package file");
        io::copy(&mut resp, &mut out).expect("failed to copy content");
    }
    util::print_color(&format!("Installing Python {}...", vers_to_dl), Color::Cyan);

    util::unpack_tar_xz(&archive_path, &py_install_path);

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

/// Prompt which Python alias to use, if multiple are found.
pub fn prompt_alias(aliases: &[(String, Version)]) -> (String, Version) {
    // Todo: Overall, the API here is inelegant.
    util::print_color("Found multiple compatible Python aliases. Please enter the number associated with the one you'd like to use for this project:", Color::Magenta);
    for (i, (alias, version)) in aliases.iter().enumerate() {
        println!("{}: {} version: {}", i + 1, alias, version.to_string())
    }

    let mut mapping = HashMap::new();
    for (i, alias) in aliases.iter().enumerate() {
        mapping.insert(i + 1, alias);
    }

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Unable to read user input for version");

    let input = input
        .chars()
        .next()
        .expect("Problem reading input")
        .to_string();

    let (alias, version) = mapping
        .get(
            &input
                .parse::<usize>()
                .expect("Enter the number associated with the Python alias."),
        )
        .expect(
            "Can't find the Python alias associated with that number. Is it in the list above?",
        );
    (alias.to_string(), *version)
}

/// Make an educated guess at the command needed to execute python the
/// current system.  An alternative approach is trying to find python
/// installations.
pub fn find_py_aliases(version: &Version) -> Vec<(String, Version)> {
    let possible_aliases = &[
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

    for alias in possible_aliases {
        // We use the --version command as a quick+effective way to determine if
        // this command is associated with Python.
        if let Some(v) = commands::find_py_version(alias) {
            if v.major == version.major && v.minor == version.minor {
                result.push((alias.to_string(), v));
            }
        }
    }
    result
}

// Find versions installed with this tool.
fn find_installed_versions() -> Vec<Version> {
    #[cfg(target_os = "windows")]
    let py_name = "python";
    #[cfg(target_os = "linux")]
    let py_name = "bin/python3";
    #[cfg(target_os = "macos")]
    let py_name = "bin/python3";

    let python_installs_dir = dirs::home_dir()
        .expect("Problem finding home directory")
        .join(".python-installs");

    if !&python_installs_dir.exists() && fs::create_dir(&python_installs_dir).is_err() {
        util::abort("Problem creating ~/python-installs directory")
    }

    let mut result = vec![];
    for entry in python_installs_dir
        .read_dir()
        .expect("Can't open python installs path")
    {
        if let Ok(entry) = entry {
            if !entry.path().is_dir() {
                continue;
            }

            if let Some(v) = commands::find_py_version(entry.path().join(py_name).to_str().unwrap())
            {
                result.push(v);
            }
        }
    }
    result
}

/// Create a new virtual environment, and install Wheel.
//fn create_venv(cfg_v: &Version, py_install: PyInstall, pyypackages_dir: &PathBuf) -> Version {
pub fn create_venv(cfg_v: &Version, pyypackages_dir: &Path) -> Version {
    let python_installs_dir = dirs::home_dir()
        .expect("Problem finding home directory")
        .join(".python-installs"); // todo dry

    let py_name;
    let os;
    let python_name;
    let pip_name;
    #[cfg(target_os = "windows")]
    {
        py_name = "python";
        os = Os::Windows;
        python_name = "python.exe";
        pip_name = "pip.exe";
    }
    #[cfg(target_os = "linux")]
    {
        py_name = "bin/python3";
        os = Os::Ubuntu;
        python_name = "python";
        pip_name = "pip";
    }
    #[cfg(target_os = "macos")]
    {
        py_name = "bin/python3";
        os = Os::Mac;
        python_name = "python";
        pip_name = "pip";
    }

    let mut alias = None;
    let mut alias_path = None;
    let mut py_ver = None;

    // If we find both a system alias, and internal version installed, go with the internal.
    // One's this tool installed
    let installed_versions = find_installed_versions();
    for iv in installed_versions.iter() {
        if iv.major == cfg_v.major && iv.minor == cfg_v.minor {
            let folder_name = format!("python-{}", iv.to_string2());
            alias_path = Some(python_installs_dir.join(folder_name).join(py_name));
            py_ver = Some(*iv);
            break;
        }
    }

    // todo perhaps move alias finding back into create_venv, or make a
    // todo create_venv_if_doesnt_exist fn.
    // Only search for a system Python if we don't have an internal one.
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
                let r = prompt_alias(&aliases);
                alias = Some(r.0);
                py_ver = Some(r.1);
            }
        };
    }

    if py_ver.is_none() {
        // Download and install the appropriate Python binary, if we can't find either a
        // custom install, or on the Path.
        download(&python_installs_dir, cfg_v);
        let py_ver2: PyVers = (*cfg_v, os).into();
        py_ver = Some(py_ver2.to_vers());

        let folder_name = format!("python-{}", py_ver2.to_string());
        alias_path = Some(python_installs_dir.join(folder_name).join(py_name));
    }

    let py_ver = py_ver.expect("missing Python version");

    let vers_path = pyypackages_dir.join(format!("{}.{}", py_ver.major, py_ver.minor));

    let lib_path = vers_path.join("lib");

    if !lib_path.exists() {
        fs::create_dir_all(&lib_path).expect("Problem creating __pypackages__ directory");
    }

    println!("Setting up Python environment...");

    if let Some(alias) = alias {
        if commands::create_venv(&alias, &lib_path, ".venv").is_err() {
            util::abort("Problem creating virtual environment");
        }
    } else if let Some(alias_path) = alias_path {
        if commands::create_venv2(&alias_path, &lib_path, ".venv").is_err() {
            util::abort("Problem creating virtual environment");
        }
    }

    let bin_path = util::find_bin_path(&vers_path);

    util::wait_for_dirs(&[bin_path.join(python_name), bin_path.join(pip_name)])
        .expect("Timed out waiting for venv to be created.");

    // We need `wheel` installed to build wheels from source.
    // Note: This installs to the venv's site-packages, not __pypackages__/3.x/lib.
    process::Command::new(bin_path.join("python"))
        .args(&["-m", "pip", "install", "--quiet", "wheel"])
        .status()
        .expect("Problem installing `wheel`");

    py_ver
}
