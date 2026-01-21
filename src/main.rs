use std::{
    collections::HashMap,
    env,
    error::Error,
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process,
    str::FromStr,
    sync::{Arc, RwLock},
};

use regex::Regex;
use serde::Deserialize;
use structopt::StructOpt;
use termcolor::{Color, ColorChoice};

use crate::{
    actions::run,
    cli_options::{ExternalCommand, ExternalSubcommands, Opt, SubCommand},
    dep_types::{Constraint, Extras, Lock, LockPackage, Package, Rename, Req, ReqType, Version},
    pyproject::{Config, CFG_FILENAME},
    util::{abort, deps::sync, Os},
};

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

type PackToInstall = ((String, Version), Option<(u32, String)>); // ((Name, Version), (parent id, rename name))

///////////////////////////////////////////////////////////////////////////////
/// Global multithreaded variables part
///////////////////////////////////////////////////////////////////////////////

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

///////////////////////////////////////////////////////////////////////////////
/// \ Global multithreaded variables part
///////////////////////////////////////////////////////////////////////////////

/// We process input commands in a deliberate order, to ensure the required, and only the required
/// setup steps are accomplished before each.
#[allow(clippy::match_single_binding)]
#[allow(clippy::single_match)]
// TODO: Remove clippy::match_single_binding and clippy::single_match after full function refactoring
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
        // Actions requires nothing to know about the project
        SubCommand::New { path, name } => {
            // if name is not provided, use the directory name
            match name {
                Some(name) => actions::new(path, name),
                None => actions::new(path, path)
            }
        },
        SubCommand::Init => actions::init(CFG_FILENAME),
        SubCommand::Reset {} => actions::reset(),
        SubCommand::Clear {} => actions::clear(&pyflow_path, &dep_cache_path, &script_env_path),
        SubCommand::Switch { version } => actions::switch(version),
        SubCommand::External(x) => match ExternalCommand::from_opt(x.to_owned()) {
            ExternalCommand { cmd, args } => match cmd {
                ExternalSubcommands::Script => {
                    script::run_script(&script_env_path, &dep_cache_path, os, &args, &pyflow_path);
                }
                // TODO: Move branches to omitted match
                _ => (),
            },
        },

        // TODO: Move branches to omitted match
        _ => {}
    }

    let pcfg = pyproject::current::get_config().unwrap_or_else(|| process::exit(1));
    let cfg_vers = if let Some(v) = pcfg.config.py_version.clone() {
        v
    } else {
        let specified = util::prompts::py_vers();

        if !pcfg.config_path.exists() {
            pcfg.config.write_file(&pcfg.config_path);
        }
        files::change_py_vers(&pcfg.config_path, &specified);

        specified
    };

    // Check for environments. Create one if none exist. Set `vers_path`.
    let (vers_path, py_vers) = util::find_or_create_venv(
        &cfg_vers,
        &pcfg.pypackages_path,
        &pyflow_path,
        &dep_cache_path,
    );

    let paths = util::Paths {
        bin: util::find_bin_path(&vers_path),
        lib: vers_path.join("lib"),
        entry_pt: vers_path.join("bin"),
        cache: dep_cache_path,
    };

    // Add all path reqs to the PYTHONPATH; this is the way we make these packages accessible when
    // running `pyflow`.
    let mut pythonpath = vec![paths.lib.clone()];
    for r in pcfg.config.reqs.iter().filter(|r| r.path.is_some()) {
        pythonpath.push(PathBuf::from(r.path.clone().unwrap()));
    }
    for r in pcfg.config.dev_reqs.iter().filter(|r| r.path.is_some()) {
        pythonpath.push(PathBuf::from(r.path.clone().unwrap()));
    }

    let mut found_lock = false;
    let lock = match util::read_lock(&pcfg.lock_path) {
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
        &pcfg.config.reqs,
        &pcfg.config.dev_reqs,
        &util::find_dont_uninstall(&pcfg.config.reqs, &pcfg.config.dev_reqs),
        os,
        &py_vers,
        &pcfg.lock_path,
    );

    // Now handle subcommands that require info about the environment
    match subcmd {
        // Add package names to `pyproject.toml` if needed. Then sync installed packages
        // and `pyflow.lock` with the `pyproject.toml`.
        // We use data from three sources: `pyproject.toml`, `pyflow.lock`, and
        // the currently-installed packages, found by crawling metadata in the `lib` path.
        // See the readme section `How installation and locking work` for details.
        SubCommand::Install { packages, dev } | SubCommand::Add { packages, dev } => {
            actions::install(
                &pcfg.config_path,
                &pcfg.config,
                &git_path,
                &paths,
                found_lock,
                &packages,
                dev,
                &lockpacks,
                &os,
                &py_vers,
                &pcfg.lock_path,
            )
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

            files::remove_reqs_from_cfg(&pcfg.config_path, &removed_reqs);

            // Filter reqs here instead of re-reading the config from file.
            let updated_reqs: Vec<Req> = pcfg
                .config
                .clone()
                .reqs
                .into_iter()
                .filter(|req| !removed_reqs.contains(&req.name))
                .collect();

            sync(
                &paths,
                &lockpacks,
                &updated_reqs,
                &pcfg.config.dev_reqs,
                &[],
                os,
                &py_vers,
                &pcfg.lock_path,
            );
            util::print_color("Uninstall complete", Color::Green);
        }

        SubCommand::Package { extras } => actions::package(
            &paths,
            &lockpacks,
            os,
            &py_vers,
            &pcfg.lock_path,
            &pcfg.config,
            &extras,
        ),
        SubCommand::Publish {} => build::publish(&paths.bin, &pcfg.config),
        SubCommand::List {} => actions::list(
            &paths.lib,
            &[pcfg.config.reqs.as_slice(), pcfg.config.dev_reqs.as_slice()]
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
                run(&paths.lib, &paths.bin, &vers_path, &pcfg.config, x.args);
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
