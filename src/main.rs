#![allow(clippy::non_ascii_literal)]

#[mockall_double::double]
use crate::dep_resolution::res;
use crate::dep_types::{Constraint, Lock, LockPackage, Package, Rename, Req, ReqType, Version};
use crate::util::{abort, process_reqs, Os};

use regex::Regex;
use serde::Deserialize;
use std::{collections::HashMap, env, error::Error, fs, path::PathBuf, str::FromStr};

use std::path::Path;
use std::sync::{Arc, RwLock};
use structopt::StructOpt;
use termcolor::{Color, ColorChoice};

mod build;
mod commands;
mod dep_parser;
mod dep_resolution;
mod dep_types;
mod files;
mod install;
mod py_versions;
mod script;
mod util;

// todo:
// Custom build system
// Fix pydeps caching timeout
// Make binaries work on any linux distro
// Mac binaries for pyflow and python
// "fatal: destination path exists" when using git deps
// add hash and git/path info to locks
// clear download git source as an option. In general, git install is a mess

type PackToInstall = ((String, Version), Option<(u32, String)>); // ((Name, Version), (parent id, rename name))

#[derive(StructOpt, Debug)]
#[structopt(name = "pyflow", about = "Python packaging and publishing")]
struct Opt {
    #[structopt(subcommand)]
    subcmds: SubCommand,

    /// Force a color option: auto (default), always, ansi, never
    #[structopt(short, long)]
    color: Option<String>,
}

#[derive(StructOpt, Debug)]
enum SubCommand {
    /// Create a project folder with the basics
    #[structopt(name = "new")]
    New {
        #[structopt(name = "name")]
        name: String, // holds the project name.
    },

    /** Install packages from `pyproject.toml`, `pyflow.lock`, or speficied ones. Example:

    `pyflow install`: sync your installation with `pyproject.toml`, or `pyflow.lock` if it exists.
    `pyflow install numpy scipy`: install `numpy` and `scipy`.*/
    #[structopt(name = "install")]
    Install {
        #[structopt(name = "packages")]
        packages: Vec<String>,
        /// Save package to your dev-dependencies section
        #[structopt(short, long)]
        dev: bool,
    },
    /// Uninstall all packages, or ones specified
    #[structopt(name = "uninstall")]
    Uninstall {
        #[structopt(name = "packages")]
        packages: Vec<String>,
    },
    /// Display all installed packages and console scripts
    #[structopt(name = "list")]
    List,
    /// Build the package - source and wheel
    #[structopt(name = "package")]
    Package {
        #[structopt(name = "extras")]
        extras: Vec<String>,
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
    /// Remove cached packages, Python installs, or script-environments. Eg to free up hard drive space.
    #[structopt(name = "clear")]
    Clear,
    /// Run a CLI script like `ipython` or `black`. Note that you can simply run `pyflow black`
    /// as a shortcut.
    // Dummy option with space at the end for documentation
    #[structopt(name = "run ")] // We don't need to invoke this directly, but the option exists
    Run,

    /// Run the project python or script with the project python environment.
    /// As a shortcut you can simply specify a script name ending in `.py`
    // Dummy option with space at the end for documentation
    #[structopt(name = "python ")]
    Python,

    /// Run a standalone script not associated with a project
    // Dummy option with space at the end for documentation
    #[structopt(name = "script ")]
    Script,
    //    /// Run a package globally; used for CLI tools like `ipython` and `black`. Doesn't
    //    /// interfere Python installations. Must have been installed with `pyflow install -g black` etc
    //    #[structopt(name = "global")]
    //    Global {
    //        #[structopt(name = "name")]
    //        name: String,
    //    },
    /// Change the Python version for this project. eg `pyflow switch 3.8`. Equivalent to setting
    /// `py_version` in `pyproject.toml`.
    #[structopt(name = "switch")]
    Switch {
        #[structopt(name = "version")]
        version: String,
    },
    // Documentation for supported external subcommands can be documented by
    // adding a `dummy` subcommand with the name having a trailing space.
    // #[structopt(name = "external ")]
    #[structopt(external_subcommand, name = "external")]
    External(Vec<String>),
}

#[derive(Clone, Debug)]
enum ExternalSubcommands {
    Run,
    Script,
    Python,
    ImpliedRun(String),
    ImpliedPython(String),
}

impl ToString for ExternalSubcommands {
    fn to_string(&self) -> String {
        match self {
            Self::Run => "run".into(),
            Self::Script => "script".into(),
            Self::Python => "python".into(),
            Self::ImpliedRun(x) => x.into(),
            Self::ImpliedPython(x) => x.into(),
        }
    }
}

impl FromStr for ExternalSubcommands {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Self> {
        let result = match s {
            "run" => Self::Run,
            "script" => Self::Script,
            "python" => Self::Python,
            x if x.ends_with(".py") => Self::ImpliedPython(x.to_string()),
            x => Self::ImpliedRun(x.to_string()),
        };
        Ok(result)
    }
}

#[derive(Clone, Debug)]
struct ExternalCommand {
    cmd: ExternalSubcommands,
    args: Vec<String>,
}

impl ExternalCommand {
    fn from_opt(args: Vec<String>) -> Self {
        let cmd = ExternalSubcommands::from_str(&args[0]).unwrap();
        let cmd_args = match cmd {
            ExternalSubcommands::Run
            | ExternalSubcommands::Script
            | ExternalSubcommands::Python => &args[1..],
            ExternalSubcommands::ImpliedRun(_) | ExternalSubcommands::ImpliedPython(_) => &args,
        };
        let cmd = match cmd {
            ExternalSubcommands::ImpliedRun(_) => ExternalSubcommands::Run,
            ExternalSubcommands::ImpliedPython(_) => ExternalSubcommands::Python,
            x => x,
        };
        Self {
            cmd,
            args: cmd_args.to_vec(),
        }
    }
}
/// A config, parsed from pyproject.toml
#[derive(Clone, Debug, Default, Deserialize)]
// todo: Auto-desr some of these
pub struct Config {
    name: Option<String>,
    py_version: Option<Version>,
    reqs: Vec<Req>,
    dev_reqs: Vec<Req>,
    version: Option<Version>,
    authors: Vec<String>,
    license: Option<String>,
    extras: HashMap<String, String>,
    description: Option<String>,
    classifiers: Vec<String>, // https://pypi.org/classifiers/
    keywords: Vec<String>,
    homepage: Option<String>,
    repository: Option<String>,
    repo_url: Option<String>,
    package_url: Option<String>,
    readme: Option<String>,
    build: Option<String>, // A python file used to build non-python extensions
    //    entry_points: HashMap<String, Vec<String>>, // todo option?
    scripts: HashMap<String, String>, //todo: put under [tool.pyflow.scripts] ?
    //    console_scripts: Vec<String>, // We don't parse these; pass them to `setup.py` as-entered.
    python_requires: Option<String>,
}

/// Reduce repetition between reqs and dev reqs when populating reqs of path reqs.
fn pop_reqs_helper(reqs: &[Req], dev: bool) -> Vec<Req> {
    let mut result = vec![];
    for req in reqs.iter().filter(|r| r.path.is_some()) {
        let req_path = PathBuf::from(req.path.clone().unwrap());
        let pyproj = req_path.join("pyproject.toml");
        let req_txt = req_path.join("requirements.txt");
        //        let pipfile = req_path.join("Pipfile");

        let mut dummy_cfg = Config::default();

        if req_txt.exists() {
            files::parse_req_dot_text(&mut dummy_cfg, &req_txt);
        }

        //        if pipfile.exists() {
        //            files::parse_pipfile(&mut dummy_cfg, &pipfile);
        //        }

        if dev {
            result.append(&mut dummy_cfg.dev_reqs);
        } else {
            result.append(&mut dummy_cfg.reqs);
        }

        // We don't parse `setup.py`, since it involves running arbitrary Python code.

        if pyproj.exists() {
            let mut req_cfg = Config::from_file(&PathBuf::from(&pyproj))
                .unwrap_or_else(|| panic!("Problem parsing`pyproject.toml`: {:?}", &pyproj));
            result.append(&mut req_cfg.reqs)
        }

        // Check for metadata of a built wheel
        for folder_name in util::find_folders(&req_path) {
            // todo: Dry from `util` and `install`.
            let re_dist = Regex::new(r"^(.*?)-(.*?)\.dist-info$").unwrap();
            if re_dist.captures(&folder_name).is_some() {
                let metadata_path = req_path.join(folder_name).join("METADATA");
                let mut metadata = util::parse_metadata(&metadata_path);

                result.append(&mut metadata.requires_dist);
            }
        }
    }
    result
}

impl Config {
    /// Helper fn to prevent repetition
    fn parse_deps(deps: HashMap<String, files::DepComponentWrapper>) -> Vec<Req> {
        let mut result = Vec::new();
        for (name, data) in deps {
            let constraints;
            let mut extras = None;
            let mut git = None;
            let mut path = None;
            let mut python_version = None;
            match data {
                files::DepComponentWrapper::A(constrs) => {
                    constraints = if let Ok(c) = Constraint::from_str_multiple(&constrs) {
                        c
                    } else {
                        abort(&format!(
                            "Problem parsing constraints in `pyproject.toml`: {}",
                            &constrs
                        ));
                        unreachable!()
                    };
                }
                files::DepComponentWrapper::B(subdata) => {
                    constraints = match subdata.constrs {
                        Some(constrs) => {
                            if let Ok(c) = Constraint::from_str_multiple(&constrs) {
                                c
                            } else {
                                abort(&format!(
                                    "Problem parsing constraints in `pyproject.toml`: {}",
                                    &constrs
                                ));
                                unreachable!()
                            }
                        }
                        None => vec![],
                    };

                    if let Some(ex) = subdata.extras {
                        extras = Some(ex);
                    }
                    if let Some(p) = subdata.path {
                        path = Some(p);
                    }
                    if let Some(repo) = subdata.git {
                        git = Some(repo);
                    }
                    if let Some(v) = subdata.python {
                        let pv = Constraint::from_str(&v)
                            .expect("Problem parsing python version in dependency");
                        python_version = Some(vec![pv]);
                    }
                }
            }

            result.push(Req {
                name,
                constraints,
                extra: None,
                sys_platform: None,
                python_version,
                install_with_extras: extras,
                path,
                git,
            });
        }
        result
    }

    // todo: DRY at the top from `from_file`.
    fn from_pipfile(path: &Path) -> Option<Self> {
        // todo: Lots of tweaks and QC could be done re what fields to parse, and how best to
        // todo parse and store them.
        let toml_str = match fs::read_to_string(path).ok() {
            Some(d) => d,
            None => return None,
        };

        let decoded: files::Pipfile = if let Ok(d) = toml::from_str(&toml_str) {
            d
        } else {
            abort("Problem parsing `Pipfile`");
            unreachable!()
        };
        let mut result = Self::default();

        if let Some(pipfile_deps) = decoded.packages {
            result.reqs = Self::parse_deps(pipfile_deps);
        }
        if let Some(pipfile_dev_deps) = decoded.dev_packages {
            result.dev_reqs = Self::parse_deps(pipfile_dev_deps);
        }

        Some(result)
    }

    /// Pull config data from `pyproject.toml`. We use this to deserialize things like Versions
    /// and requirements.
    fn from_file(path: &Path) -> Option<Self> {
        // todo: Lots of tweaks and QC could be done re what fields to parse, and how best to
        // todo parse and store them.
        let toml_str = match fs::read_to_string(path) {
            Ok(d) => d,
            Err(_) => return None,
        };

        let decoded: files::Pyproject = if let Ok(d) = toml::from_str(&toml_str) {
            d
        } else {
            abort("Problem parsing `pyproject.toml`");
            unreachable!()
        };
        let mut result = Self::default();

        // Parse Poetry first, since we'll use pyflow if there's a conflict.
        if let Some(po) = decoded.tool.poetry {
            if let Some(v) = po.name {
                result.name = Some(v);
            }
            if let Some(v) = po.authors {
                result.authors = v;
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
            if let Some(v) = po.readme {
                result.readme = Some(v);
            }
            if let Some(v) = po.build {
                result.build = Some(v);
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

            // todo: DRY (c+p) from pyflow dependency parsing, other than parsing python version here,
            // todo which only poetry does.
            // todo: Parse poetry dev deps
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
                                let pv = Constraint::from_str(&v)
                                    .expect("Problem parsing python version in dependency");
                                python_version = Some(vec![pv]);
                            }
                            // todo repository etc
                        }
                    }
                    if &name.to_lowercase() == "python" {
                        if let Some(constr) = constraints.get(0) {
                            result.py_version = Some(constr.version.clone())
                        }
                    } else {
                        result.reqs.push(Req {
                            name,
                            constraints,
                            extra: None,
                            sys_platform: None,
                            python_version,
                            install_with_extras: extras,
                            path: None,
                            git: None,
                        });
                    }
                }
            }
        }

        if let Some(pf) = decoded.tool.pyflow {
            if let Some(v) = pf.name {
                result.name = Some(v);
            }

            if let Some(v) = pf.authors {
                result.authors = if v.is_empty() {
                    util::get_git_author()
                } else {
                    v
                };
            }
            if let Some(v) = pf.license {
                result.license = Some(v);
            }
            if let Some(v) = pf.homepage {
                result.homepage = Some(v);
            }
            if let Some(v) = pf.description {
                result.description = Some(v);
            }
            if let Some(v) = pf.repository {
                result.repository = Some(v);
            }

            // todo: Process entry pts, classifiers etc?
            if let Some(v) = pf.classifiers {
                result.classifiers = v;
            }
            if let Some(v) = pf.keywords {
                result.keywords = v;
            }
            if let Some(v) = pf.readme {
                result.readme = Some(v);
            }
            if let Some(v) = pf.build {
                result.build = Some(v);
            }
            //            if let Some(v) = pf.entry_points {
            //                result.entry_points = v;
            //            } // todo
            if let Some(v) = pf.scripts {
                result.scripts = v;
            }

            if let Some(v) = pf.python_requires {
                result.python_requires = Some(v);
            }

            if let Some(v) = pf.package_url {
                result.package_url = Some(v);
            }

            if let Some(v) = pf.version {
                result.version = Some(
                    Version::from_str(&v).expect("Problem parsing version in `pyproject.toml`"),
                )
            }

            if let Some(v) = pf.py_version {
                result.py_version = Some(
                    Version::from_str(&v)
                        .expect("Problem parsing python version in `pyproject.toml`"),
                );
            }

            if let Some(deps) = pf.dependencies {
                result.reqs = Self::parse_deps(deps);
            }
            if let Some(deps) = pf.dev_dependencies {
                result.dev_reqs = Self::parse_deps(deps);
            }
        }

        Some(result)
    }

    /// For reqs of `path` type, add their sub-reqs by parsing `setup.py` or `pyproject.toml`.
    fn populate_path_subreqs(&mut self) {
        self.reqs.append(&mut pop_reqs_helper(&self.reqs, false));
        self.dev_reqs
            .append(&mut pop_reqs_helper(&self.dev_reqs, true));
    }

    /// Create a new `pyproject.toml` file.
    fn write_file(&self, path: &Path) {
        let file = path;
        if file.exists() {
            abort("`pyproject.toml` already exists")
        }

        let mut result = String::new();

        result.push_str("\n[tool.pyflow]\n");
        if let Some(name) = &self.name {
            result.push_str(&("name = \"".to_owned() + name + "\"\n"));
        } else {
            // Give name, and a few other fields default values.
            result.push_str(&("name = \"\"".to_owned() + "\n"));
        }
        if let Some(py_v) = &self.py_version {
            result.push_str(&("py_version = \"".to_owned() + &py_v.to_string_no_patch() + "\"\n"));
        } else {
            result.push_str(&("py_version = \"3.8\"".to_owned() + "\n"));
        }
        if let Some(vers) = self.version.clone() {
            result.push_str(&(format!("version = \"{}\"", vers.to_string() + "\n")));
        } else {
            result.push_str("version = \"0.1.0\"");
            result.push('\n');
        }
        if !self.authors.is_empty() {
            result.push_str("authors = [\"");
            for (i, author) in self.authors.iter().enumerate() {
                if i != 0 {
                    result.push_str(", ");
                }
                result.push_str(author);
            }
            result.push_str("\"]\n");
        }

        if let Some(v) = &self.description {
            result.push_str(&(format!("description = \"{}\"", v) + "\n"));
        }
        if let Some(v) = &self.homepage {
            result.push_str(&(format!("homepage = \"{}\"", v) + "\n"));
        }

        // todo: More fields

        result.push('\n');
        result.push_str("[tool.pyflow.scripts]\n");
        for (name, mod_fn) in &self.scripts {
            result.push_str(&(format!("{} = \"{}\"", name, mod_fn) + "\n"));
        }

        result.push('\n');
        result.push_str("[tool.pyflow.dependencies]\n");
        for dep in &self.reqs {
            result.push_str(&(dep.to_cfg_string() + "\n"));
        }

        result.push('\n');
        result.push_str("[tool.pyflow.dev-dependencies]\n");
        for dep in &self.dev_reqs {
            result.push_str(&(dep.to_cfg_string() + "\n"));
        }

        result.push('\n'); // trailing newline

        if fs::write(file, result).is_err() {
            abort("Problem writing `pyproject.toml`")
        }
    }
}

/// Cli Config to hold command line options
struct CliConfig {
    pub color_choice: ColorChoice,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            color_choice: ColorChoice::Auto,
        }
    }
}

impl CliConfig {
    pub fn current() -> Arc<CliConfig> {
        CLI_CONFIG.with(|c| c.read().unwrap().clone())
    }
    pub fn make_current(self) {
        CLI_CONFIG.with(|c| *c.write().unwrap() = Arc::new(self))
    }
}

thread_local! {
    static CLI_CONFIG: RwLock<Arc<CliConfig>> = RwLock::new(Default::default());
}

/// Create a template directory for a python project.
pub fn new(name: &str) -> Result<(), Box<dyn Error>> {
    if !PathBuf::from(name).exists() {
        fs::create_dir_all(&format!("{}/{}", name, name.replace("-", "_")))?;
        fs::File::create(&format!("{}/{}/__init__.py", name, name.replace("-", "_")))?;
        fs::File::create(&format!("{}/README.md", name))?;
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

    let readme_init = &format!("# {}\n\n{}", name, "(A description)");

    fs::write(&format!("{}/.gitignore", name), gitignore_init)?;
    fs::write(&format!("{}/README.md", name), readme_init)?;

    let cfg = Config {
        name: Some(name.to_string()),
        authors: util::get_git_author(),
        py_version: Some(util::prompt_py_vers()),
        ..Default::default()
    };

    cfg.write_file(&PathBuf::from(format!("{}/pyproject.toml", name)));

    if commands::git_init(Path::new(name)).is_err() {
        util::print_color(
            "Unable to initialize a git repo for your project",
            Color::Yellow, // Dark
        );
    };

    Ok(())
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

/// Execute a python CLI tool, either specified in `pyproject.toml`, or in a dependency.
fn run_cli_tool(
    lib_path: &Path,
    bin_path: &Path,
    vers_path: &Path,
    cfg: &Config,
    args: Vec<String>,
) {
    // Allow both `pyflow run ipython` (args), and `pyflow ipython` (opt.script)
    if args.is_empty() {
        return;
    }

    let name = if let Some(a) = args.get(0) {
        a.clone()
    } else {
        abort("`run` must be followed by the script to run, eg `pyflow run black`");
        unreachable!()
    };

    // If the script we're calling is specified in `pyproject.toml`, ensure it exists.

    // todo: Delete these scripts as required to sync with pyproject.toml.
    let re = Regex::new(r"(.*?):(.*)").unwrap();

    let mut specified_args: Vec<String> = args.into_iter().skip(1).collect();

    // If a script name is specified by by this project and a dependency, favor
    // this project.
    if let Some(s) = cfg.scripts.get(&name) {
        let abort_msg = format!(
            "Problem running the function {}, specified in `pyproject.toml`",
            name,
        );

        if let Some(caps) = re.captures(s) {
            let module = caps.get(1).unwrap().as_str();
            let function = caps.get(2).unwrap().as_str();
            let mut args_to_pass = vec![
                "-c".to_owned(),
                format!(r#"import {}; {}.{}()"#, module, module, function),
            ];

            args_to_pass.append(&mut specified_args);
            if commands::run_python(bin_path, &[lib_path.to_owned()], &args_to_pass).is_err() {
                abort(&abort_msg);
            }
        } else {
            abort(&format!("Problem parsing the following script: {:#?}. Must be in the format module:function_name", s));
            unreachable!()
        }
        return;
    }
    //            None => {
    let abort_msg = format!(
        "Problem running the CLI tool {}. Is it installed? \
         Try running `pyflow install {}`",
        name, name
    );
    let script_path = vers_path.join("bin").join(name);
    if !script_path.exists() {
        abort(&abort_msg);
    }

    let mut args_to_pass = vec![script_path
        .to_str()
        .expect("Can't find script path")
        .to_owned()];

    args_to_pass.append(&mut specified_args);
    if commands::run_python(bin_path, &[lib_path.to_owned()], &args_to_pass).is_err() {
        abort(&abort_msg);
    }
}

/// Function used by `Install` and `Uninstall` subcommands to syn dependencies with
/// the config and lock files.
#[allow(clippy::too_many_arguments)]
fn sync(
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

    // Dev reqs and normal reqs are both installed here; we only ommit dev reqs
    // when packaging.
    let mut combined_reqs = reqs.to_vec();
    for dev_req in dev_reqs.to_vec() {
        combined_reqs.push(dev_req);
    }

    let resolved = if let Ok(r) = res::resolve(&combined_reqs, &locked, os, py_vers) {
        r
    } else {
        abort("Problem resolving dependencies");
        unreachable!()
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
    // depenencies with it.
    sync_deps(
        paths,
        &updated_lock_packs,
        dont_uninstall,
        &installed,
        os,
        py_vers,
    );
}

#[derive(Clone)]
enum ClearChoice {
    Dependencies,
    ScriptEnvs,
    PyInstalls,
    //    Global,
    All,
}

impl ToString for ClearChoice {
    fn to_string(&self) -> String {
        "".into()
    }
}

/// Clear `Pyflow`'s cache. Allow the user to select which parts to clear based on a prompt.
fn clear(pyflow_path: &Path, cache_path: &Path, script_env_path: &Path) {
    let result = util::prompt_list(
        "Which cached items would you like to clear?",
        "choice",
        &[
            ("Downloaded dependencies".into(), ClearChoice::Dependencies),
            (
                "Standalone-script environments".into(),
                ClearChoice::ScriptEnvs,
            ),
            ("Python installations".into(), ClearChoice::PyInstalls),
            ("All of the above".into(), ClearChoice::All),
        ],
        false,
    );

    // todo: DRY
    match result.1 {
        ClearChoice::Dependencies => {
            if fs::remove_dir_all(&cache_path).is_err() {
                abort(&format!(
                    "Problem removing the dependency-cache path: {:?}",
                    cache_path
                ));
            }
        }
        ClearChoice::ScriptEnvs => {
            if fs::remove_dir_all(&script_env_path).is_err() {
                abort(&format!(
                    "Problem removing the script env path: {:?}",
                    script_env_path
                ));
            }
        }
        ClearChoice::PyInstalls => {}
        ClearChoice::All => {
            if fs::remove_dir_all(&pyflow_path).is_err() {
                abort(&format!(
                    "Problem removing the Pyflow path: {:?}",
                    pyflow_path
                ));
            }
        }
    }
}

/// We process input commands in a deliberate order, to ensure the required, and only the required
/// setup steps are accomplished before each.
fn main() {
    let cfg_filename = "pyproject.toml";
    let lock_filename = "pyflow.lock";

    let base_dir = directories::BaseDirs::new();
    let pyflow_path = base_dir
        .expect("Problem finding base directory")
        .data_dir()
        .to_owned()
        .join("pyflow");

    let dep_cache_path = pyflow_path.join("dependency-cache");
    let script_env_path = pyflow_path.join("script-envs");
    let git_path = pyflow_path.join("git");

    #[cfg(target_os = "windows")]
    let os = Os::Windows;
    #[cfg(target_os = "linux")]
    let os = Os::Linux;
    #[cfg(target_os = "macos")]
    let os = Os::Mac;

    let opt = Opt::from_args();
    #[cfg(debug_assertions)]
    eprintln!("opts {:?}", opt);
    // Handle color option
    let choice = match opt.color.unwrap_or_else(|| String::from("auto")).as_str() {
        "always" => ColorChoice::Always,
        "ansi" => ColorChoice::AlwaysAnsi,
        "auto" => {
            if atty::is(atty::Stream::Stdout) {
                ColorChoice::Auto
            } else {
                ColorChoice::Never
            }
        }
        _ => ColorChoice::Never,
    };

    CliConfig {
        color_choice: choice,
    }
    .make_current();

    // Handle commands that don't involve operating out of a project before one that do, with setup
    // code in-between.
    let subcmd = opt.subcmds;

    let extcmd = if let SubCommand::External(ref x) = subcmd {
        Some(ExternalCommand::from_opt(x.to_owned()))
    } else {
        None
    };

    // Run this before parsing the config.
    if let Some(x) = extcmd.clone() {
        if let ExternalSubcommands::Script = x.cmd {
            script::run_script(&script_env_path, &dep_cache_path, os, &x.args, &pyflow_path);
            return;
        }
    }

    if let SubCommand::New { name } = subcmd {
        if new(&name).is_err() {
            abort(
                "Problem creating the project. This may be due to a permissions problem. \
                 If on linux, please try again with `sudo`.",
            );
        }
        util::print_color(
            &format!("Created a new Python project named {}", name),
            Color::Green,
        );
        return;
    }

    if let SubCommand::Init {} = subcmd {
        let cfg_path = PathBuf::from(cfg_filename);
        if cfg_path.exists() {
            abort("pyproject.toml already exists - not overwriting.")
        }

        let mut cfg = match PathBuf::from("Pipfile").exists() {
            true => Config::from_pipfile(&PathBuf::from("Pipfile")).unwrap_or_default(),
            false => Config::default(),
        };

        cfg.py_version = Some(util::prompt_py_vers());

        files::parse_req_dot_text(&mut cfg, &PathBuf::from("requirements.txt"));

        cfg.write_file(&cfg_path);
        util::print_color("Created `pyproject.toml`", Color::Green);
        // Don't return here; let the normal logic create the venv now.
    }

    // We need access to the config from here on; throw an error if we can't find it.
    let mut cfg_path = PathBuf::from(cfg_filename);
    if !&cfg_path.exists() {
        //        if let SubCommand::Python { args: _ } = subcmd {
        // Try looking recursively in parent directories for a config file.
        let recursion_limit = 8; // How my levels to look up
        let mut current_level = env::current_dir().expect("Can't access current directory");
        for _ in 0..recursion_limit {
            if let Some(parent) = current_level.parent() {
                let parent_cfg_path = parent.join(cfg_filename);
                if parent_cfg_path.exists() {
                    cfg_path = parent_cfg_path;
                    break;
                }
                current_level = parent.to_owned();
            }
        }

        if !&cfg_path.exists() {
            // ie still can't find it after searching parents.
            util::print_color(
                "To get started, run `pyflow new projname` to create a project folder, or \
            `pyflow init` to start a project in this folder. For a list of what you can do, run \
            `pyflow help`.",
                Color::Cyan, // Dark
            );
            return;
        }
        //        }
    }

    // Base pypackages_path and lock_path on the `pyproject.toml` folder.
    let proj_path = cfg_path.parent().expect("Can't find proj pathw via parent");
    let pypackages_path = proj_path.join("__pypackages__");
    let lock_path = &proj_path.join(lock_filename);

    let mut cfg = Config::from_file(&cfg_path).unwrap_or_default();
    cfg.populate_path_subreqs();

    // Run subcommands that don't require info about the environment.
    match &subcmd {
        SubCommand::Reset {} => {
            if pypackages_path.exists() && fs::remove_dir_all(&pypackages_path).is_err() {
                abort("Problem removing `__pypackages__` directory")
            }
            if lock_path.exists() && fs::remove_file(&lock_path).is_err() {
                abort("Problem removing `pyflow.lock`")
            }
            util::print_color(
                "`__pypackages__` folder and `pyflow.lock` removed",
                Color::Green,
            );
            return;
        }
        SubCommand::Switch { version } => {
            // Updates `pyproject.toml` with a new python version
            let specified = util::fallible_v_parse(&version.clone());
            cfg.py_version = Some(specified.clone());
            files::change_py_vers(&PathBuf::from(&cfg_path), &specified);
            util::print_color(
                &format!("Switched to Python version {}", specified.to_string()),
                Color::Green,
            );
            // Don't return; now that we've changed the cfg version, let's run the normal flow.
        }
        SubCommand::Clear {} => {
            clear(&pyflow_path, &dep_cache_path, &script_env_path);
            return;
        }
        SubCommand::List => {
            let num_venvs = util::find_venvs(&pypackages_path).len();
            if !cfg_path.exists() && num_venvs == 0 {
                abort("Can't find a project in this directory")
            } else if num_venvs == 0 {
                util::print_color(
                    "There's no python environment set up for this project",
                    Color::Green,
                );
                return;
            }
        }
        _ => (),
    }

    let cfg_vers = if let Some(v) = cfg.py_version.clone() {
        v
    } else {
        let specified = util::prompt_py_vers();

        if !cfg_path.exists() {
            cfg.write_file(&cfg_path);
        }
        files::change_py_vers(&cfg_path, &specified);

        specified
    };

    // Check for environments. Create one if none exist. Set `vers_path`.
    let (vers_path, py_vers) =
        util::find_or_create_venv(&cfg_vers, &pypackages_path, &pyflow_path, &dep_cache_path);

    let paths = util::Paths {
        bin: util::find_bin_path(&vers_path),
        lib: vers_path.join("lib"),
        entry_pt: vers_path.join("bin"),
        cache: dep_cache_path,
    };

    // Add all path reqs to the PYTHONPATH; this is the way we make these packages accessible when
    // running `pyflow`.
    let mut pythonpath = vec![paths.lib.clone()];
    for r in cfg.reqs.iter().filter(|r| r.path.is_some()) {
        pythonpath.push(PathBuf::from(r.path.clone().unwrap()));
    }
    for r in cfg.dev_reqs.iter().filter(|r| r.path.is_some()) {
        pythonpath.push(PathBuf::from(r.path.clone().unwrap()));
    }

    let mut found_lock = false;
    let lock = match util::read_lock(&lock_path) {
        Ok(l) => {
            found_lock = true;
            l
        }
        Err(_) => Lock::default(),
    };

    let lockpacks = lock.package.unwrap_or_else(Vec::new);

    sync(
        &paths,
        &lockpacks,
        &cfg.reqs,
        &cfg.dev_reqs,
        &util::find_dont_uninstall(&cfg.reqs, &cfg.dev_reqs),
        os,
        &py_vers,
        &lock_path,
    );

    // Now handle subcommands that require info about the environment
    match subcmd {
        // Add pacakge names to `pyproject.toml` if needed. Then sync installed packages
        // and `pyproject.lock` with the `pyproject.toml`.
        // We use data from three sources: `pyproject.toml`, `pyflow.lock`, and
        // the currently-installed packages, found by crawling metadata in the `lib` path.
        // See the readme section `How installation and locking work` for details.
        SubCommand::Install { packages, dev } => {
            if !cfg_path.exists() {
                cfg.write_file(&cfg_path);
            }

            if found_lock {
                util::print_color("Found lockfile", Color::Green);
            }

            // Merge reqs added via cli with those in `pyproject.toml`.
            let (updated_reqs, up_dev_reqs) = util::merge_reqs(&packages, dev, &cfg, &cfg_path);

            let dont_uninstall = util::find_dont_uninstall(&updated_reqs, &up_dev_reqs);

            let updated_reqs = process_reqs(updated_reqs, &git_path, &paths);
            let up_dev_reqs = process_reqs(up_dev_reqs, &git_path, &paths);

            sync(
                &paths,
                &lockpacks,
                &updated_reqs,
                &up_dev_reqs,
                &dont_uninstall,
                os,
                &py_vers,
                &lock_path,
            );
            util::print_color("Installation complete", Color::Green);
        }

        SubCommand::Uninstall { packages } => {
            // todo: uninstall dev?
            // Remove dependencies specified in the CLI from the config, then lock and sync.

            let removed_reqs: Vec<String> = packages
                .into_iter()
                .map(|p| {
                    Req::from_str(&p, false)
                        .expect("Problem parsing req while uninstalling")
                        .name
                })
                .collect();

            files::remove_reqs_from_cfg(&cfg_path, &removed_reqs);

            // Filter reqs here instead of re-reading the config from file.
            let updated_reqs: Vec<Req> = cfg
                .clone()
                .reqs
                .into_iter()
                .filter(|req| !removed_reqs.contains(&req.name))
                .collect();

            sync(
                &paths,
                &lockpacks,
                &updated_reqs,
                &cfg.dev_reqs,
                &[],
                os,
                &py_vers,
                &lock_path,
            );
            util::print_color("Uninstall complete", Color::Green);
        }

        SubCommand::Package { extras } => {
            sync(
                &paths,
                &lockpacks,
                &cfg.reqs,
                &cfg.dev_reqs,
                &util::find_dont_uninstall(&cfg.reqs, &cfg.dev_reqs),
                os,
                &py_vers,
                &lock_path,
            );

            build::build(&lockpacks, &paths, &cfg, &extras)
        }
        SubCommand::Publish {} => build::publish(&paths.bin, &cfg),

        //        SubCommand::M { args } => {
        //            run_cli_tool(&paths.lib, &paths.bin, &vers_path, &cfg, args);
        //        }
        SubCommand::List {} => util::show_installed(
            &paths.lib,
            &[cfg.reqs.as_slice(), cfg.dev_reqs.as_slice()]
                .concat()
                .into_iter()
                .filter(|r| r.path.is_some())
                .collect::<Vec<Req>>(),
        ),
        _ => (),
    }

    if let Some(x) = extcmd {
        match x.cmd {
            ExternalSubcommands::Python => {
                if commands::run_python(&paths.bin, &pythonpath, &x.args).is_err() {
                    abort("Problem running Python");
                }
            }
            ExternalSubcommands::Run => {
                run_cli_tool(&paths.lib, &paths.bin, &vers_path, &cfg, x.args);
            }
            x => {
                abort(&format!(
                    "Sub command {:?} should have been handled already",
                    x
                ));
            }
        }
    }
}

#[cfg(test)]
pub mod tests {}
