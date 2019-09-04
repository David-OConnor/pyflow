use crate::dep_types::{
    Constraint, DependencyError, Lock, LockPackage, Package, Rename, Req, ReqType, Version,
};
use crate::util::abort;
use crossterm::{Color, Colored};
use install::PackageType::{Source, Wheel};
use regex::Regex;
use serde::Deserialize;
use std::{
    collections::HashMap, env, error::Error, fmt, fs, io, path::PathBuf, process::Command,
    str::FromStr, thread, time,
};

use crate::dep_resolution::WarehouseRelease;
use crate::install::PackageType;
use structopt::StructOpt;

mod build;
mod commands;
mod dep_resolution;
mod dep_types;
mod files;
mod install;
mod util;

type PackToInstall = ((String, Version), Option<(u32, String)>); // ((Name, Version), (parent id, rename name))

#[derive(Copy, Clone, Debug, Deserialize, PartialEq)]
/// Used to determine which version of a binary package to download. Assume 64-bit.
pub enum Os {
    Linux32,
    Linux,
    Windows32,
    Windows,
    //    Mac32,
    Mac,
    Any,
}

impl FromStr for Os {
    type Err = dep_types::DependencyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "manylinux1_i686" => Os::Linux32,
            "manylinux1_x86_64" => Os::Linux,
            "linux" => Os::Linux,
            "linux2" => Os::Linux,
            "windows" => Os::Windows,
            "win" => Os::Windows,
            "win32" => Os::Windows32,
            "win_amd64" => Os::Windows,
            "darwin" => Os::Mac,
            "any" => Os::Any,
            _ => {
                if s.contains("mac") {
                    Os::Mac
                } else {
                    return Err(DependencyError::new("Problem parsing Os"));
                }
            }
        })
    }
}

#[derive(StructOpt, Debug)]
//#[structopt(raw(setting = "structopt::clap::AppSettings::suggestions"))]
//#[structopt(name = "Pypackage", about = "Python packaging and publishing", structopt::clap::AppSettings::suggestions = "false")]
#[structopt(name = "Pypackage", about = "Python packaging and publishing")]
struct Opt {
    #[structopt(subcommand)]
    subcmds: Option<SubCommand>,
    #[structopt(name = "script")]
    //    #[structopt(raw(setting = "structopt::clap::AppSettings::TrailingVarArg"))]
    script: Vec<String>,
}

#[derive(StructOpt, Debug)]
enum SubCommand {
    /// Create a project folder with the basics
    #[structopt(name = "new")]
    New {
        #[structopt(name = "name")]
        name: String, // holds the project name.
    },

    /// Install packages from `pyproject.toml`, or ones specified
    #[structopt(
    name = "install",
    help = "
Install packages from `pyproject.toml`, `pypackage.lock`, or speficied ones. Example:

`pypackage install`: sync your installation with `pyproject.toml`, or `pypackage.lock` if it exists.
`pypackage install numpy scipy`: install `numpy` and `scipy`.
"
    )]
    Install {
        #[structopt(name = "packages")]
        packages: Vec<String>,
    },
    /// Uninstall all packages, or ones specified
    #[structopt(name = "uninstall")]
    Uninstall {
        #[structopt(name = "packages")]
        packages: Vec<String>,
    },
    /// Run python
    #[structopt(name = "python")]
    Python {
        #[structopt(name = "args")]
        args: Vec<String>,
    },
    /// Display all installed packages and console scripts
    #[structopt(name = "list")]
    List,
    /// Build the package - source and wheel
    #[structopt(name = "package")]
    Package {
        #[structopt(name = "extras")]
        extras: Vec<String>, // todo: rename features?
    },
    /// Publish to `pypi`
    #[structopt(name = "publish")]
    Publish,
    /// Create a `pyproject.toml` from requirements.txt, pipfile etc, setup.py etc
    #[structopt(name = "init")]
    Init,
    /// Remove the environment, and uninstall all packages
    #[structopt(name = "reset")]
    Reset,
    /// Run a CLI script like `ipython` or `black`. Note that you can simply run `pypackage black`
    /// as a shortcut.
    #[structopt(name = "run")] // We don't need to invoke this directly, but the option exists
    Run {
        #[structopt(name = "args")]
        args: Vec<String>,
    },
}

/// A config, parsed from pyproject.toml
#[derive(Clone, Debug, Default, Deserialize)]
// todo: Auto-desr some of these
pub struct Config {
    py_version: Option<Constraint>,
    reqs: Vec<Req>, // name, requirements.
    name: Option<String>,
    version: Option<Version>,
    author: Option<String>,
    author_email: Option<String>,
    license: Option<String>,
    extras: HashMap<String, String>,
    description: Option<String>,
    classifiers: Vec<String>, // https://pypi.org/classifiers/
    keywords: Vec<String>,    // todo: Options for classifiers and keywords?
    homepage: Option<String>,
    repository: Option<String>,
    repo_url: Option<String>,
    package_url: Option<String>,
    readme_filename: Option<String>,
    entry_points: HashMap<String, Vec<String>>, // todo option?
    //    console_scripts: HashMap<String, String>,   // todo option?
    console_scripts: Vec<String>, // We don't parse these; pass them to `setup.py` as-entered.
}

impl Config {
    /// Pull config data from `pyproject.toml`. We use this to deserialize things like Versions
    /// and requirements.
    fn from_file(filename: &str) -> Option<Self> {
        // todo: Lots of tweaks and QC could be done re what fields to parse, and how best to
        // todo parse and store them.
        let toml_str = match fs::read_to_string(filename) {
            Ok(d) => d,
            Err(_) => return None,
        };

        let decoded: files::Pyproject = match toml::from_str(&toml_str) {
            Ok(d) => d,
            Err(_) => {
                abort("Problem parsing `pyproject.toml`");
                unreachable!()
            }
        };
        let mut result = Self::default();

        // Parse Poetry first, since we'll use PyPackage if there's a conflict.
        if let Some(po) = decoded.tool.poetry {
            if let Some(v) = po.name {
                result.name = Some(v);
            }
            if let Some(v) = po.authors {
                result.author = Some(v.join(", "));
            }
            if let Some(v) = po.license {
                result.license = Some(v);
            }

            if let Some(v) = po.homepage {
                result.homepage = Some(v);
            }
            if let Some(v) = po.description {
                result.description = Some(v);
            }
            if let Some(v) = po.repository {
                result.repository = Some(v);
            }

            // todo: Process entry pts, classifiers etc?
            if let Some(v) = po.classifiers {
                result.classifiers = v;
            }
            if let Some(v) = po.keywords {
                result.keywords = v;
            }

            //                        if let Some(v) = po.source {
            //                result.source = v;
            //            }
            //            if let Some(v) = po.scripts {
            //                result.console_scripts = v;
            //            }
            if let Some(v) = po.extras {
                result.extras = v;
            }

            if let Some(v) = po.version {
                result.version = Some(
                    Version::from_str(&v).expect("Problem parsing version in `pyproject.toml`"),
                )
            }

            // todo: DRY (c+p) from pypackage dependency parsing, other than parsing python version here,
            // todo which only poetry does.
            if let Some(deps) = po.dependencies {
                for (name, data) in deps {
                    let constraints;
                    let mut extras = None;
                    let mut python_version = None;
                    match data {
                        files::DepComponentWrapperPoetry::A(constrs) => {
                            constraints = Constraint::from_str_multiple(&constrs)
                                .expect("Problem parsing constraints in `pyproject.toml`.");
                        }
                        files::DepComponentWrapperPoetry::B(subdata) => {
                            constraints = Constraint::from_str_multiple(&subdata.constrs)
                                .expect("Problem parsing constraints in `pyproject.toml`.");
                            if let Some(ex) = subdata.extras {
                                extras = Some(ex);
                            }
                            if let Some(v) = subdata.python {
                                python_version = Some(Constraint::from_str(&v)
                                    .expect("Problem parsing python version in dependency"));
                            }
                            // todo repository etc
                        }
                    }
                    if name.to_lowercase() == "python" {
                        result.py_version = Some(
                            constraints
                                .get(0).clone()
                                .unwrap_or(
                                    &Constraint::new(ReqType::Tilde, Version::new(3, 4, 0))
                                ).clone(),

                        );
                    } else {
                        result.reqs.push(Req {
                            name,
                            constraints,
                            extra: None,
                            sys_platform: None,
                            python_version,
                            install_with_extras: extras,
                        });
                    }
                }
            }
        }

        if let Some(pp) = decoded.tool.pypackage {
            if let Some(v) = pp.name {
                result.name = Some(v);
            }
            if let Some(v) = pp.author {
                result.author = Some(v);
            }
            if let Some(v) = pp.author_email {
                result.author_email = Some(v);
            }
            if let Some(v) = pp.license {
                result.license = Some(v);
            }
            if let Some(v) = pp.homepage {
                result.homepage = Some(v);
            }
            if let Some(v) = pp.description {
                result.description = Some(v);
            }
            if let Some(v) = pp.repository {
                result.repository = Some(v);
            }

            // todo: Process entry pts, classifiers etc?
            if let Some(v) = pp.classifiers {
                result.classifiers = v;
            }
            if let Some(v) = pp.keywords {
                result.keywords = v;
            }
            if let Some(v) = pp.entry_points {
                result.entry_points = v;
            }
            if let Some(v) = pp.console_scripts {
                result.console_scripts = v;
            }

            if let Some(v) = pp.version {
                result.version = Some(
                    Version::from_str(&v).expect("Problem parsing version in `pyproject.toml`"),
                )
            }

            if let Some(v) = pp.py_version {
                result.py_version = Some(
                    Constraint::from_str(&v)
                        .expect("Problem parsing python version in `pyproject.toml`"),
                )
            }

            if let Some(deps) = pp.dependencies {
                for (name, data) in deps {
                    let constraints;
                    let mut extras = None;
                    let mut python_version = None;
                    match data {
                        files::DepComponentWrapper::A(constrs) => {
                            constraints = Constraint::from_str_multiple(&constrs)
                                .expect("Problem parsing constraints in `pyproject.toml`.");
                        }
                        files::DepComponentWrapper::B(subdata) => {
                            constraints = Constraint::from_str_multiple(&subdata.constrs)
                                .expect("Problem parsing constraints in `pyproject.toml`.");
                            if let Some(ex) = subdata.extras {
                                extras = Some(ex);
                            }
                            if let Some(v) = subdata.python {
                                python_version = Some(Constraint::from_str(&v)
                                    .expect("Problem parsing python version in dependency"));
                            }
                            // todo repository etc
                        }
                    }
                    //                    let

                    result.reqs.push(Req {
                        name,
                        constraints,
                        extra: None,
                        sys_platform: None,
                        python_version,
                        install_with_extras: extras,
                    });
                }
            }
        }

        Some(result)
    }

    /// Create a new `pyproject.toml` file.
    fn write_file(&self, filename: &str) {
        let file = PathBuf::from(filename);
        if file.exists() {
            abort("`pyproject.toml` already exists")
        }

        let mut result =
            "# See PEP 518: https://www.python.org/dev/peps/pep-0518/ for info on this \
             file's structure.\n"
                .to_string();

        result.push_str("\n[tool.pypackage]\n");
        if let Some(name) = &self.name {
            result.push_str(&("name = \"".to_owned() + name + "\"\n"));
        } else {
            // Give name, and a few other fields default values.
            result.push_str(&("name = \"\"".to_owned() + "\n"));
        }
        if let Some(py_v) = &self.py_version {
            result
                .push_str(&("py_version = \"".to_owned() + &py_v.to_string(false, false) + "\"\n"));
        } else {
            result.push_str(&("py_version = \"^3.4\"".to_owned() + "\n"));
        }
        if let Some(vers) = self.version {
            result.push_str(&(format!("version = \"{}\"", vers.to_string2()) + "\n"));
        }
        if let Some(author) = &self.author {
            result.push_str(&(format!("author = \"{}\"", author) + "\n"));
        }
        if let Some(v) = &self.author_email {
            result.push_str(&(format!("author_email = \"{}\"", v) + "\n"));
        }
        if let Some(v) = &self.description {
            result.push_str(&(format!("description = \"{}\"", v) + "\n"));
        }
        if let Some(v) = &self.homepage {
            result.push_str(&(format!("homepage = \"{}\"", v) + "\n"));
        }
        // todo: more fields.

        result.push_str("\n\n");
        result.push_str("[tool.pypackage.dependencies]\n\n");
        for dep in self.reqs.iter() {
            result.push_str(&(dep.to_cfg_string() + "\n"));
        }

        match fs::write(file, result) {
            Ok(_) => util::print_color("Created `pyproject.toml`", Color::Green),
            Err(_) => abort("Problem writing `pyproject.toml`"),
        }
    }
}

/// Create a template directory for a python project.
pub fn new(name: &str) -> Result<(), Box<dyn Error>> {
    if !PathBuf::from(name).exists() {
        fs::create_dir_all(&format!("{}/{}", name, name))?;
        fs::File::create(&format!("{}/{}/main.py", name, name))?;
        fs::File::create(&format!("{}/README.md", name))?;
        fs::File::create(&format!("{}/LICENSE", name))?;
        fs::File::create(&format!("{}/pyproject.toml", name))?;
        fs::File::create(&format!("{}/.gitignore", name))?;
    }

    let gitignore_init = r##"# General Python ignores
build/
dist/
__pycache__/
__pypackages__/
.ipynb_checkpoints/
*.pyc
*~
*/.mypy_cache/


# Project ignores
"##;

    let pyproject_init = &format!(
        r##"#See PEP 518: https://www.python.org/dev/peps/pep-0518/ for info on this file's structure.

[tool.pypackage]
name = "{}"
py_version = "^3.7"
version = "0.1.0"
description = ""
author = ""

pyackage_url = "https://test.pypi.org"
# pyackage_url = "https://pypi.org"


[tool.pypackage.dependencies]

"##,
        name
    );

    // todo: flesh readme out
    let readme_init = &format!("# {}", name);

    fs::write(&format!("{}/.gitignore", name), gitignore_init)?;
    fs::write(&format!("{}/pyproject.toml", name), pyproject_init)?;
    fs::write(&format!("{}/README.md", name), readme_init)?;

    Ok(())
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

/// Make an educated guess at the command needed to execute python the
/// current system.  An alternative approach is trying to find python
/// installations.
fn find_py_alias() -> Result<(String, Version), AliasError> {
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

/// Read dependency data from a lock file.
fn read_lock(filename: &str) -> Result<(Lock), Box<dyn Error>> {
    let data = fs::read_to_string(filename)?;
    Ok(toml::from_str(&data)?)
}

/// Write dependency data to a lock file.
fn write_lock(filename: &str, data: &Lock) -> Result<(), Box<dyn Error>> {
    let data = toml::to_string(data)?;
    fs::write(filename, data)?;
    Ok(())
}

/// Find the operating system from a wheel filename. This doesn't appear to be available
/// anywhere else on the Pypi Warehouse.
fn os_from_wheel_fname(filename: &str) -> Result<(Os), dep_types::DependencyError> {
    // Format is "name-version-pythonversion-mobileversion?-os.whl"
    // Also works with formats like this:
    // `PyQt5-5.13.0-5.13.0-cp35.cp36.cp37.cp38-none-win32.whl` too.
    // The point is, pull the last part before ".whl".
    let re = Regex::new(r"^(?:.*?-)+(.*).whl$").unwrap();
    if let Some(caps) = re.captures(filename) {
        let parsed = caps.get(1).unwrap().as_str();
        return Ok(
            Os::from_str(parsed).unwrap_or_else(|_| panic!("Problem parsing Os: {}", parsed))
        );
    }

    Err(dep_types::DependencyError::new(
        "Problem parsing os from wheel name",
    ))
}

/// Create a new virtual environment, and install Wheel.
fn create_venv(cfg_v: Option<&Constraint>, pyypackages_dir: &PathBuf) -> Version {
    // We only use the alias for creating the virtual environment. After that,
    // we call our venv's executable directly.

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

    // todo: Chicken-egg scenario where we need to wait for the venv to complete before
    // todo installing `wheel` and returning, but don't know what folder
    // todo to look for in wait_for_dirs. Blanket sleep for now.
    thread::sleep(time::Duration::from_millis(2000));
    let bin_path = util::find_bin_path(&vers_path);

    // We need `wheel` installed to build wheels from source.
    // Note: This installs to the venv's site-packages, not __pypackages__/3.x/lib.
    Command::new("./python")
        .current_dir(bin_path)
        .args(&["-m", "pip", "install", "--quiet", "wheel"])
        .spawn()
        .expect("Problem installing `wheel`");

    py_ver_from_alias
}

fn parse_lockpack_rename(rename: &str) -> (u32, String) {
    let re = Regex::new(r"^(\d+)\s(.*)$").unwrap();
    let caps = re
        .captures(&rename)
        .expect("Problem reading lock file rename");

    let id = caps.get(1).unwrap().as_str().parse::<u32>().unwrap();
    let name = caps.get(2).unwrap().as_str().to_owned();

    (id, name)
}

/// Find the most appropriate release to download. Ie Windows vs Linux, wheel vs source.
fn find_best_release(
    data: &[WarehouseRelease],
    name: &str,
    version: &Version,
    os: Os,
    python_vers: &Version,
) -> (WarehouseRelease, PackageType) {
    // Find which release we should download. Preferably wheels, and if so, for the right OS and
    // Python version.
    let mut compatible_releases = vec![];
    // Store source releases as a fallback, for if no wheels are found.
    let mut source_releases = vec![];

    for rel in data.iter() {
        let mut compatible = true;
        match rel.packagetype.as_ref() {
            "bdist_wheel" => {
                if let Some(py_ver) = &rel.requires_python {
                    // If a version constraint exists, make sure it's compatible.
                    let py_constrs = Constraint::from_str_multiple(&py_ver)
                        .expect("Problem parsing constraint from requires_python");

                    for constr in py_constrs.iter() {
                        if !constr.is_compatible(&python_vers) {
                            compatible = false;
                        }
                    }
                }

                let wheel_os =
                    os_from_wheel_fname(&rel.filename).expect("Problem getting os from wheel name");
                if wheel_os != os && wheel_os != Os::Any {
                    compatible = false;
                }

                // Packages that use C code(eg numpy) may fail to load C extensions if installing
                // for the wrong version of python (eg  cp35 when python 3.7 is installed), even
                // if `requires_python` doesn't indicate an incompatibility. Check `python_version`
                // instead of `requires_python`.
                // Note that the result of this parse is an any match.
                match Constraint::from_wh_py_vers(&rel.python_version) {
                    Ok(constrs) => {
                        let mut compat_py_v = false;
                        for constr in constrs.iter() {
                            if constr.is_compatible(python_vers) {
                                compat_py_v = true;
                            }
                        }
                        if !compat_py_v {
                            compatible = false;
                        }
                    }
                    Err(_) => {
                        (println!(
                            "Unable to match python version from python_version: {}",
                            &rel.python_version
                        ))
                    }
                }

                if compatible {
                    compatible_releases.push(rel.clone());
                }
            }
            "sdist" => source_releases.push(rel.clone()),
            // todo: handle dist_egg and bdist_wininst?
            "bdist_egg" => println!("Found bdist_egg... skipping"),
            "bdist_wininst" => (), // Don't execute Windows installers
            "bdist_msi" => (),     // Don't execute Windows installers
            _ => {
                println!("Found surprising package type: {}", rel.packagetype);
                continue;
            }
        }
    }

    let best_release;
    let package_type;
    // todo: Sort further / try to match exact python_version if able.
    if compatible_releases.is_empty() {
        if source_releases.is_empty() {
            abort(&format!(
                "Unable to find a compatible release for {}: {}",
                name,
                version.to_string()
            ));
            unreachable!()
        } else {
            best_release = source_releases[0].clone();
            package_type = Source;
        }
    } else {
        best_release = compatible_releases[0].clone();
        package_type = Wheel;
    }

    (best_release, package_type)
}

/// Install/uninstall deps as required from the passed list, and re-write the lock file.
fn sync_deps(
    bin_path: &PathBuf,
    lib_path: &PathBuf,
    lock_packs: &[LockPackage],
    installed: &[(String, Version, Vec<String>)],
    os: Os,
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
                match &lp.rename {
                    // todo back to our custom type?
                    Some(rn) => Some(parse_lockpack_rename(&rn)),
                    None => None,
                },
            )
        })
        .collect();

    // todo shim. Use top-level A/R. We discard it temporarily while working other issues.
    let installed: Vec<(String, Version)> = installed
        .iter()
        .map(|t| (util::standardize_name(&t.0), t.1))
        .collect();

    // Filter by not-already-installed.
    let to_install: Vec<&PackToInstall> = packages
        .iter()
        .filter(|(pack, _)| !installed.contains(pack))
        .collect();

    // todo: Once you include rename info in installed, you won't need to use the map logic here.
    let packages_only: Vec<&(String, Version)> = packages.iter().map(|(p, _)| p).collect();
    let to_uninstall: Vec<&(String, Version)> = installed
        .iter()
        .filter(|inst| {
            let inst = (util::standardize_name(&inst.0), inst.1);
            !packages_only.contains(&&inst)
        })
        .collect();

    for (name, version) in to_uninstall.iter() {
        // todo: Deal with renamed. Currently won't work correctly with them.
        install::uninstall(name, version, lib_path)
    }

    for ((name, version), rename) in to_install.iter() {
        let data = dep_resolution::get_warehouse_release(&name, &version)
            .expect("Problem getting warehouse data");

        let (best_release, package_type) =
            find_best_release(&data, &name, &version, os, python_vers);

        println!(
            "⬇️ Installing {}{}{} {} ...",
            Colored::Fg(Color::Cyan),
            &name,
            Colored::Fg(Color::Reset),
            &version
        );
        if install::download_and_install_package(
            &name,
            &version,
            &best_release.url,
            &best_release.filename,
            &best_release.digests.sha256,
            lib_path,
            bin_path,
            package_type,
            rename,
        )
            .is_err()
        {
            abort("Problem downloading packages");
        }

        if let Some((id, new)) = rename {
            // Rename in the renamed package
            install::rename_package_files(&lib_path.join(new), name, &new);

            // Rename in the parent calling the renamed package. // todo: Multiple parents?
            let parent = lock_packs
                .iter()
                .find(|lp| lp.id == *id)
                .expect("Can't find parent calling renamed package");
            install::rename_package_files(&lib_path.join(&parent.name), name, &new);

            // todo: Handle this more generally, in case we don't have proper semvar dist-info paths.
            install::rename_metadata(
                &lib_path.join(&format!("{}-{}.dist-info", name, version.to_string2())),
                name,
                &new,
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

/// Function used by `Install` and `Uninstall` subcommands to syn dependencies with
/// the config and lock files.
fn sync(
    bin_path: &PathBuf,
    lib_path: &PathBuf,
    lockpacks: &[LockPackage],
    reqs: &[Req],
    os: Os,
    py_vers: &Version,
    lock_filename: &str,
) {
    let installed = util::find_installed(&lib_path);
    // We control the lock format, so this regex will always match
    let dep_re = Regex::new(r"^(.*?)\s(.*)\s.*$").unwrap();

    // We don't need to resolve reqs that are already locked.
    let locked: Vec<Package> = lockpacks
        .iter()
        .map(|lp| {
            let mut deps = vec![];
            for dep in lp.dependencies.as_ref().unwrap_or(&vec![]) {
                let caps = dep_re
                    .captures(&dep)
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

    println!("🔍 Resolving dependencies...");

    let resolved = match dep_resolution::resolve(&reqs, &locked, os, &py_vers) {
        Ok(r) => r,
        Err(_) => {
            abort("Problem resolving dependencies");
            unreachable!()
        }
    };

    //    println!("RESOLVED: {:#?}", &resolved);

    // Now merge the existing lock packages with new ones from resolved packages.
    // We have a collection of requirements; attempt to merge them with the already-locked ones.
    let mut updated_lock_packs = vec![];

    for package in resolved.iter() {
        let dummy_constraints = vec![Constraint::new(ReqType::Exact, package.version)];
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
                    name,
                    version.to_string2(),
                    name,
                    version.to_string2(),
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
    if write_lock(lock_filename, &updated_lock).is_err() {
        abort("Problem writing lock file");
    }

    // Now that we've confirmed or modified the lock file, we're ready to sync installed
    // depenencies with it.
    sync_deps(
        //                &bin_path, &lib_path, &packages, &installed, &os, &py_vers, &resolved,
        &bin_path,
        &lib_path,
        &updated_lock_packs,
        &installed,
        os,
        &py_vers,
    );
}

fn main() {
    // todo perhaps much of this setup code should only be in certain match branches.
    let cfg_filename = "pyproject.toml";
    let lock_filename = "pypackage.lock";

    let mut cfg = Config::from_file(cfg_filename).unwrap_or_default();

    let opt = Opt::from_args();
    let subcmd = match opt.subcmds {
        Some(sc) => sc,
        None => SubCommand::Run { args: opt.script },
    };

    // New doesn't execute any other logic. Init must execute befor the rest of the logic,
    // since it sets up a new (or modified) `pyproject.toml`. The rest of the commands rely
    // on the virtualenv and `pyproject.toml`, so make sure those are set up before processing them.
    match subcmd {
        SubCommand::New { name } => {
            new(&name).expect("Problem creating project");
            util::print_color(
                &format!("Created a new Python project named {}", name),
                Color::Green,
            );
            return;
        }
        SubCommand::Init {} => {
            files::parse_req_dot_text(&mut cfg);
            files::parse_pipfile(&mut cfg);

            if PathBuf::from(cfg_filename).exists() {
                abort("pyproject.toml already exists - not overwriting.")
            }
            cfg.write_file(cfg_filename);
        }
        _ => (),
    }

    let pypackages_dir = env::current_dir()
        .expect("Can't find current path")
        .join("__pypackages__");

    // Check for environments. Create one if none exist. Set `vers_path`.
    let mut vers_path = PathBuf::new();
    let mut py_vers = Version::new(0, 0, 0);
    let venvs = util::find_venvs(&pypackages_dir);

    match &cfg.py_version {
        // The version's explicitly specified; check if an environment for that versionc
        // exists. If not, create one, and make sure it's the right version.
        Some(cfg_constr) => {
            // The version's explicitly specified; check if an environment for that version
            let compatible_venvs: Vec<&(u32, u32)> = venvs
                .iter()
                .filter(|(ma, mi)| cfg_constr.is_compatible(&Version::new_short(*ma, *mi)))
                .collect();

            match compatible_venvs.len() {
                0 => {
                    let vers = create_venv(Some(cfg_constr), &pypackages_dir);
                    vers_path = pypackages_dir.join(&format!("{}.{}", vers.major, vers.minor));
                    py_vers = Version::new_short(vers.major, vers.minor);  // Don't include patch.
                }
                1 => {
                    vers_path = pypackages_dir.join(&format!(
                        "{}.{}",
                        compatible_venvs[0].0, compatible_venvs[0].1
                    ));
                    py_vers = Version::new_short(compatible_venvs[0].0, compatible_venvs[0].1);
                }
                _ => abort(
                    "Multiple compatible Python environments found
                for this project;  please tighten the constraints, or remove unecessary ones.",
                ),
            }
        }
        // The version's not specified in the config; Search for existing environments, and create
        // one if we can't find any.
        None => match venvs.len() {
            0 => {
                let vers = create_venv(None, &pypackages_dir);
                vers_path = pypackages_dir.join(&format!("{}.{}", vers.major, vers.minor));
                py_vers = vers;
            }
            1 => {
                vers_path = pypackages_dir.join(&format!("{}.{}", venvs[0].0, venvs[0].1));
                py_vers = Version::new_short(venvs[0].0, venvs[0].1);
            }
            _ => abort(
                "Multiple Python environments found
                for this project; specify the desired one in `pyproject.toml`. Example:
[tool.pypackage]
py_version = \"^3.7\"",
            ),
        },
    };

    let lib_path = vers_path.join("lib");
    let bin_path = util::find_bin_path(&vers_path);

    let mut found_lock = false;
    let lock = match read_lock(lock_filename) {
        Ok(l) => {
            found_lock = true;
            l
        }
        Err(_) => Lock::default(),
    };

    #[cfg(target_os = "windows")]
        let os = Os::Windows;
    #[cfg(target_os = "linux")]
        let os = Os::Linux;
    #[cfg(target_os = "macos")]
        let os = Os::Mac;

    let lockpacks = lock.package.unwrap_or_else(|| vec![]);
    //    let extras = cfg.extras;

    match subcmd {
        // Add pacakge names to `pyproject.toml` if needed. Then sync installed packages
        // and `pyproject.lock` with the `pyproject.toml`.
        // We use data from three sources: `pyproject.toml`, `pypackage.lock`, and
        // the currently-installed packages, found by crawling metadata in the `lib` path.
        // See the readme section `How installation and locking work` for details.
        SubCommand::Install { packages } => {
            if !PathBuf::from(cfg_filename).exists() {
                cfg.write_file(cfg_filename);
            }

            if found_lock {
                println!("Found lockfile");
            }
            // Merge reqs added via cli with those in `pyproject.toml`.
            let updated_reqs = util::merge_reqs(&packages, &cfg, cfg_filename);

            sync(
                &bin_path,
                &lib_path,
                &lockpacks,
                &updated_reqs,
                os,
                &py_vers,
                &lock_filename,
            );
            util::print_color("Installation complete", Color::Green);
        }

        SubCommand::Uninstall { packages } => {
            // Remove dependencies specified in the CLI from the config, then lock and sync.

            let removed_reqs: Vec<String> = packages
                .into_iter()
                .map(|p| {
                    Req::from_str(&p, false)
                        .expect("Problem parsing req while uninstalling")
                        .name
                })
                .collect();
            println!("(dbg) to remove {:#?}", &removed_reqs);

            files::remove_reqs_from_cfg(cfg_filename, &removed_reqs);

            // Filter reqs here instead of re-reading the config from file.
            let updated_reqs: Vec<Req> = cfg
                .reqs
                .into_iter()
                .filter(|req| !removed_reqs.contains(&req.name))
                .collect();

            sync(
                &bin_path,
                &lib_path,
                &lockpacks,
                &updated_reqs,
                os,
                &py_vers,
                &lock_filename,
            );
            util::print_color("Uninstall complete", Color::Green);
        }

        SubCommand::Python { args } => {
            if commands::run_python(&bin_path, &lib_path, &args).is_err() {
                abort("Problem running Python");
            }
        }
        SubCommand::Package { extras } => build::build(&bin_path, &lib_path, &cfg, extras),
        SubCommand::Publish {} => build::publish(&bin_path, &cfg),
        SubCommand::Reset {} => {
            if fs::remove_dir_all(&pypackages_dir).is_err() {
                abort("Problem removing `__pypackages__` directory")
            }
            util::print_color("Reset complete", Color::Green);
        }

        SubCommand::Run { args } => {
            // Allow both `pypackage run ipython` (args), and `pypackage ipython` (opt.script)
            if !args.is_empty() {
                // todo better handling, eg abort
                let name = args.get(0).expect("Missing first arg").clone();
                let mut args: Vec<String> = args.into_iter().skip(1).collect();

                //                let scripts = vec![];
                let script_path = vers_path.join(format!("bin/{}", name));

                let abort_msg = &format!(
                    "Problem running the script {}. Is it installed? \
                     Try running `pypackage install {}`",
                    name, name
                );
                // Handle the error here, instead of letting Python handle it, so we can
                // display a more nicer message.
                if !script_path.exists() {
                    abort(abort_msg);
                }

                let mut args2 = vec![script_path
                    .to_str()
                    .expect("Can't find script path")
                    .to_owned()];

                args2.append(&mut args);

                if commands::run_python(&bin_path, &lib_path, &args2).is_err() {
                    abort(abort_msg);
                }

                return;
            }
        }
        SubCommand::List {} => util::show_installed(&lib_path),
        // We already handled init and new
        SubCommand::Init {} => (),
        SubCommand::New { .. } => (),
    }
}

#[cfg(test)]
pub mod tests {}
