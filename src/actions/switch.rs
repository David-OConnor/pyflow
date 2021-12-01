use std::{path::PathBuf, process};

use termcolor::Color;

use crate::{files, pyproject, util};

/// Updates `pyproject.toml` with a new python version
pub fn switch(version: &str) {
    let mut pcfg = pyproject::current::get_config().unwrap_or_else(|| process::exit(1));

    let specified = util::fallible_v_parse(version);
    pcfg.config.py_version = Some(specified.clone());
    files::change_py_vers(&PathBuf::from(&pcfg.config_path), &specified);
    util::print_color(
        &format!("Switched to Python version {}", specified.to_string()),
        Color::Green,
    );
    // Don't exit program here; now that we've changed the cfg version, let's run the normal flow.
}
