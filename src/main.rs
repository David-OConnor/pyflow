#![allow(clippy::non_ascii_literal)]

use crate::actions::new;
use crate::cli_options::{ExternalCommand, ExternalSubcommands, Opt, SubCommand};
use crate::dep_types::{Constraint, Lock, Package, Req, Version};
use crate::pyproject::Config;
use crate::util::abort;
use crate::util::deps::sync;

use regex::Regex;
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use termcolor::{Color, ColorChoice};

mod actions;
mod build;
mod cli_options;
mod commands;
mod dep_parser;
mod dep_resolution;
mod dep_types;
mod files;
mod install;
mod py_versions;
mod pyproject;
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

fn parse_lockpack_rename(rename: &str) -> (u32, String) {
    let re = Regex::new(r"^(\d+)\s(.*)$").unwrap();
    let caps = re
        .captures(rename)
        .expect("Problem reading lock file rename");

    let id = caps.get(1).unwrap().as_str().parse::<u32>().unwrap();
    let name = caps.get(2).unwrap().as_str().to_owned();

    (id, name)
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
    let result = util::prompts::list(
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

const CFG_FILENAME: &str = "pyproject.toml";
const LOCK_FILENAME: &str = "pyflow.lock";

/// We process input commands in a deliberate order, to ensure the required, and only the required
/// setup steps are accomplished before each.
fn main() {
    let (pyflow_path, dep_cache_path, script_env_path, git_path) = util::paths::get_paths();
    let os = util::get_os();

    let opt = <Opt as structopt::StructOpt>::from_args();
    #[cfg(debug_assertions)]
    eprintln!("opts {:?}", opt);

    CliConfig {
        color_choice: util::handle_color_option(
            opt.color.unwrap_or_else(|| String::from("auto")).as_str(),
        ),
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

    match &subcmd {
        SubCommand::External(ref x) => match ExternalCommand::from_opt(x.to_owned()) {
            ExternalCommand { cmd, args } => match cmd {
                ExternalSubcommands::Script => {
                    script::run_script(&script_env_path, &dep_cache_path, os, &args, &pyflow_path);
                }
                // TODO: Move branches to omitted match
                _ => (),
            },
        },
        SubCommand::New { name } => {
            if new(&name).is_err() {
                abort(actions::NEW_ERROR_MESSAGE);
            }

            util::print_color(
                &format!("Created a new Python project named {}", name),
                Color::Green,
            );
            return;
        }
        SubCommand::Init => {
            let cfg_path = PathBuf::from(CFG_FILENAME);
            if cfg_path.exists() {
                abort("pyproject.toml already exists - not overwriting.")
            }

            let mut cfg = match PathBuf::from("Pipfile").exists() {
                true => Config::from_pipfile(&PathBuf::from("Pipfile")).unwrap_or_default(),
                false => Config::default(),
            };

            cfg.py_version = Some(util::prompts::py_vers());

            files::parse_req_dot_text(&mut cfg, &PathBuf::from("requirements.txt"));

            cfg.write_file(&cfg_path);
            util::print_color("Created `pyproject.toml`", Color::Green);
            // Don't return here; let the normal logic create the venv now.
        }
        // TODO: Move branches to omitted match
        _ => {}
    }

    // We need access to the config from here on; throw an error if we can't find it.
    let mut cfg_path = PathBuf::from(CFG_FILENAME);
    if !&cfg_path.exists() {
        // Try looking recursively in parent directories for a config file.
        let recursion_limit = 8; // How my levels to look up
        let mut current_level = env::current_dir().expect("Can't access current directory");
        for _ in 0..recursion_limit {
            if let Some(parent) = current_level.parent() {
                let parent_cfg_path = parent.join(CFG_FILENAME);
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
    let lock_path = &proj_path.join(LOCK_FILENAME);

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
        let specified = util::prompts::py_vers();

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
        // Add package names to `pyproject.toml` if needed. Then sync installed packages
        // and `pyflow.lock` with the `pyproject.toml`.
        // We use data from three sources: `pyproject.toml`, `pyflow.lock`, and
        // the currently-installed packages, found by crawling metadata in the `lib` path.
        // See the readme section `How installation and locking work` for details.
        SubCommand::Install { packages, dev } => actions::install(
            &cfg_path, &cfg, &git_path, &paths, found_lock, &packages, dev, &lockpacks, &os,
            &py_vers, lock_path,
        ),

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
