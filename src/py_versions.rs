//! Manages Python installations

use commands;
use dep_types::{Version};

/// Only versions we've built and hosted
#[derive(Clone, Copy, Debug)]
enum PyVers{
    V3_7_4,
    V3_6_9,
    V3_5_6,
    V3_4_10,
}

/// Only Oses we've built and hosted
/// todo: How cross-compat are these? Eg work across diff versions of Ubuntu?
/// todo Ubuntu/Debian? Ubuntu/all linux??
/// todo: 32-bit
#[derive(Clone, Copy, Debug)]
enum Os {  // Don't confuse with crate::Os
Ubuntu,
    Windows,
    Mac,
}

#[derive(Debug)]
struct Variant {
    version: PyVers,
    os: Os,
}

impl ToString for Variant {
    fn to_string(&self) -> String {

    }
}

fn download(python_install_path: &PathBuf, variant: Variant) {
    let archive_path = lib_path.join(variant.to_string());

    // If the archive is already in the lib folder, don't re-download it. Note that this
    // isn't the usual flow, but may have some uses.
    if !archive_path.exists() {
        // Save the file
        let mut resp = reqwest::get(url)?; // Download the file
        let mut out =
            fs::File::create(&archive_path).expect("Failed to save downloaded package file");
        io::copy(&mut resp, &mut out).expect("failed to copy content");
    }
}

#[derive(Debug)]
pub struct AliasError {
    details: String,
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
fn prompt_alias(aliases: &[(String, Version)]) -> (String, Version) {
    // Todo: Overall, the API here is inelegant.
    println!("Found multiple Python aliases. Please enter the number associated with the one you'd like to use for this project:");
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
pub fn find_py_alias() -> Result<(String, Version), AliasError> {
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

    let mut found_aliases = Vec::new();

    for alias in possible_aliases {
        // We use the --version command as a quick+effective way to determine if
        // this command is associated with Python.
        if let Some(v) = commands::find_py_version(alias) {
            found_aliases.push((alias.to_string(), v));
        }
    }

    match found_aliases.len() {
        0 => Err(AliasError {
            details: "Can't find Python on the path.".into(),
        }),
        1 => Ok(found_aliases[0].clone()),
        _ => Ok(prompt_alias(&found_aliases)),
    }
}

// Find versions installed with this tool.
fn find_installed_versions() -> Vec<Version> {
    let python_installs_dir = env::home_dir()
        .expect("Problem finding home directory")
        .join(".python-installs");

    let mut result = Vec![];
    for entry in python_installs_dir.read_dir().expect("Can't open python installs path") {
        if let Ok(entry) = entry {
            if !entry
                .path()
                .is_dir()
            {
                continue
            }
            if let Some(v) = commands::find_py_version(entry.path().join("python3")) {
                resultpush(v);
            }

        }
    }
    result

}

//enum PyInstall {
//    External(String), // ie one installed through something other than this tool. PATH Alias
//    Internal(PathBuf),  // Path to bin dir.
//}

/// Create a new virtual environment, and install Wheel.
//fn create_venv(cfg_v: &Version, py_install: PyInstall, pyypackages_dir: &PathBuf) -> Version {
pub fn create_venv(cfg_v: &Version, pyypackages_dir: &PathBuf) -> Version {

    // One's this tool installed
    let installed_versions = find_installed_versions();

    // todo perhaps move alias finding back into create_venv, or make a
    // todo create_venv_if_doesnt_exist fn.
    let (alias, py_ver_from_alias) = match find_py_alias() {
        Ok(a) => a,
        Err(_) => {
            abort("Unable to find a Python version on the path");
            unreachable!()
        }
    };

    let vers_path = pyypackages_dir.join(format!(
        "{}.{}",
        py_ver_from_alias.major, py_ver_from_alias.minor
    ));

    let lib_path = vers_path.join("lib");

    if !lib_path.exists() {
        fs::create_dir_all(&lib_path).expect("Problem creating __pypackages__ directory");
    }

    if let Some(c_v) = cfg_v {
        // We don't expect the config version to specify a patch, but if it does, take it
        // into account.
        if !c_v.is_compatible(&py_ver_from_alias) {
            abort(&format!("The Python version you selected ({}) doesn't match the one specified in `pyprojecttoml` ({})",
                           py_ver_from_alias.to_string(), c_v.to_string(false, false))
            );
        }
    }

    println!("Setting up Python environment...");

    if commands::create_venv(&alias, &lib_path, ".venv").is_err() {
        util::abort("Problem creating virtual environment");
    }

    let python_name;
    let pip_name;
    #[cfg(target_os = "windows")]
        {
            python_name = "python.exe";
            pip_name = "pip.exe";
        }
    #[cfg(target_os = "linux")]
        {
            python_name = "python";
            pip_name = "pip";
        }
    #[cfg(target_os = "macos")]
        {
            python_name = "python";
            pip_name = "pip";
        }

    let bin_path = util::find_bin_path(&vers_path);

    util::wait_for_dirs(&[bin_path.join(python_name), bin_path.join(pip_name)])
        .expect("Timed out waiting for venv to be created.");

    // We need `wheel` installed to build wheels from source.
    // Note: This installs to the venv's site-packages, not __pypackages__/3.x/lib.
    Command::new(bin_path.join("python"))
        .args(&["-m", "pip", "install", "--quiet", "wheel"])
        .status()
        .expect("Problem installing `wheel`");

    py_ver_from_alias
}