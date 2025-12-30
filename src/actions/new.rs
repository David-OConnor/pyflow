use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use termcolor::Color;

use crate::{
    commands,
    util::{self, abort, success},
    Config,
};

const GITIGNORE_INIT: &str = r##"
# General Python ignores
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

pub const NEW_ERROR_MESSAGE: &str = r#"
Problem creating the project. This may be due to a permissions problem.
If on linux, please try again with `sudo`.
"#;

pub fn new(name: &str) {
    if new_internal(name).is_err() {
        abort(NEW_ERROR_MESSAGE);
    }
    success(&format!("Created a new Python project named {}", name))
}

// TODO: Join this function after refactoring
/// Create a template directory for a python project.
fn new_internal(name: &str) -> Result<(), Box<dyn Error>> {
    if !PathBuf::from(name).exists() {
        fs::create_dir_all(&format!("{}/{}", name, name.replace("-", "_")))?;
        fs::File::create(&format!("{}/{}/__init__.py", name, name.replace("-", "_")))?;
        fs::File::create(&format!("{}/README.md", name))?;
        fs::File::create(&format!("{}/.gitignore", name))?;
    }

    let readme_init = &format!("# {}\n\n{}", name, "(A description)");

    fs::write(&format!("{}/.gitignore", name), GITIGNORE_INIT)?;
    fs::write(&format!("{}/README.md", name), readme_init)?;

    let cfg = Config {
        name: Some(name.to_string()),
        authors: util::get_git_author(),
        py_version: Some(util::prompts::py_vers()),
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
