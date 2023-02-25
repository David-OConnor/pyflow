use clap::{Parser, Subcommand};
use std::str::FromStr;

#[derive(Debug, Parser)]
#[command(
    name = "pyflow",
    version,
    author,
    about = "Python packaging and publishing"
)]
pub struct Opt {
    #[command(subcommand)]
    pub subcmds: SubCommand,

    /// Force a color option: auto (default), always, ansi, never
    #[arg(short, long)]
    pub color: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum SubCommand {
    /// Create a project folder with the basics
    #[command(name = "new")]
    New {
        #[arg(name = "name")]
        name: String, // holds the project name.
    },

    /// Add packages to `pyproject.toml` and sync an environment
    #[command(name = "add")]
    Add {
        #[arg(name = "packages")]
        packages: Vec<String>, // holds the packages names.
        /// Save package to your dev-dependencies section
        #[arg(short, long)]
        dev: bool,
    },

    /** Install packages from `pyproject.toml`, `pyflow.lock`, or specified ones. Example:

    `pyflow install`: sync your installation with `pyproject.toml`, or `pyflow.lock` if it exists.
    `pyflow install numpy scipy`: install `numpy` and `scipy`.*/
    #[command(name = "install")]
    Install {
        #[arg(name = "packages")]
        packages: Vec<String>,
        /// Save package to your dev-dependencies section
        #[arg(short, long)]
        dev: bool,
    },
    /// Uninstall all packages, or ones specified
    #[command(name = "uninstall")]
    Uninstall {
        #[arg(name = "packages")]
        packages: Vec<String>,
    },
    /// Display all installed packages and console scripts
    #[command(name = "list")]
    List,
    /// Build the package - source and wheel
    #[command(name = "package")]
    Package {
        #[arg(name = "extras")]
        extras: Vec<String>,
    },
    /// Publish to `pypi`
    #[command(name = "publish")]
    Publish,
    /// Create a `pyproject.toml` from requirements.txt, pipfile etc, setup.py etc
    #[command(name = "init")]
    Init,
    /// Remove the environment, and uninstall all packages
    #[command(name = "reset")]
    Reset,
    /// Remove cached packages, Python installs, or script-environments. Eg to free up hard drive space.
    #[command(name = "clear")]
    Clear,
    /// Run a CLI script like `ipython` or `black`. Note that you can simply run `pyflow black`
    /// as a shortcut.
    // Dummy option with space at the end for documentation
    #[command(name = "run ")] // We don't need to invoke this directly, but the option exists
    Run,

    /// Run the project python or script with the project python environment.
    /// As a shortcut you can simply specify a script name ending in `.py`
    // Dummy option with space at the end for documentation
    #[command(name = "python ")]
    Python,

    /// Run a standalone script not associated with a project
    // Dummy option with space at the end for documentation
    #[command(name = "script ")]
    Script,
    //    /// Run a package globally; used for CLI tools like `ipython` and `black`. Doesn't
    //    /// interfere Python installations. Must have been installed with `pyflow install -g black` etc
    //    #[command(name = "global")]
    //    Global {
    //        #[arg(name = "name")]
    //        name: String,
    //    },
    /// Change the Python version for this project. eg `pyflow switch 3.8`. Equivalent to setting
    /// `py_version` in `pyproject.toml`.
    #[command(name = "switch")]
    Switch {
        #[arg(name = "version")]
        version: String,
    },
    // Documentation for supported external subcommands can be documented by
    // adding a `dummy` subcommand with the name having a trailing space.
    // #[command(name = "external ")]
    #[command(external_subcommand, name = "external")]
    External(Vec<String>),
}

#[derive(Clone, Debug)]
pub enum ExternalSubcommands {
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
pub struct ExternalCommand {
    pub cmd: ExternalSubcommands,
    pub args: Vec<String>,
}

impl ExternalCommand {
    pub fn from_opt(args: Vec<String>) -> Self {
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
