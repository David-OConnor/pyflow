use super::{Config, PresentConfig, CFG_FILENAME, LOCK_FILENAME};
use crate::util;
use std::{env, path::PathBuf};
use termcolor::Color;

const NOT_FOUND_ERROR_MESSAGE: &str = indoc::indoc! {r#"
To get started, run `pyflow new projname` to create a project folder, or
`pyflow init` to start a project in this folder. For a list of what you can do, run
`pyflow help`.
"#};

pub fn get_config() -> Option<PresentConfig> {
    let mut config_path = PathBuf::from(CFG_FILENAME);
    if !&config_path.exists() {
        // Try looking recursively in parent directories for a config file.
        let recursion_limit = 8; // How my levels to look up
        let mut current_level = env::current_dir().expect("Can't access current directory");
        for _ in 0..recursion_limit {
            if let Some(parent) = current_level.parent() {
                let parent_cfg_path = parent.join(CFG_FILENAME);
                if parent_cfg_path.exists() {
                    config_path = parent_cfg_path;
                    break;
                }
                current_level = parent.to_owned();
            }
        }

        if !&config_path.exists() {
            // we still can't find it after searching parents.
            util::print_color(NOT_FOUND_ERROR_MESSAGE, Color::Cyan); // Dark Cyan
            return None;
        }
    }

    // Base pypackages_path and lock_path on the `pyproject.toml` folder.
    let project_path = config_path
        .parent()
        .expect("Can't find project path via parent")
        .to_path_buf();
    let pypackages_path = project_path.join("__pypackages__");
    let lock_path = project_path.join(LOCK_FILENAME);

    let mut config = Config::from_file(&config_path).unwrap_or_default();
    config.populate_path_subreqs();
    Some(PresentConfig {
        config,
        config_path,
        project_path,
        pypackages_path,
        lock_path,
    })
}
