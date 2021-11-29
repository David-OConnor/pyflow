use std::path::PathBuf;

use termcolor::Color;

use crate::{
    files,
    pyproject::Config,
    util::{self, abort},
};

pub fn init(cfg_filename: &str) {
    let cfg_path = PathBuf::from(cfg_filename);
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
}
