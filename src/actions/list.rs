use crate::{
    dep_types::Req,
    pyproject,
    util::{self, abort, print_color, print_color_},
};
use std::{path::Path, process};
use termcolor::Color;

/// List all installed dependencies and console scripts, by examining the `libs` and `bin` folders.
/// Also include path requirements, which won't appear in the `lib` folder.
pub fn list(lib_path: &Path, path_reqs: &[Req]) {
    // This part check that project and venvs exists
    let pcfg = pyproject::current::get_config().unwrap_or_else(|| process::exit(1));
    let num_venvs = util::find_venvs(&pcfg.pypackages_path).len();

    if !pcfg.config_path.exists() && num_venvs == 0 {
        abort("Can't find a project in this directory")
    } else if num_venvs == 0 {
        abort("There's no python environment set up for this project")
    }

    let installed = util::find_installed(lib_path);
    let scripts = find_console_scripts(&lib_path.join("../bin"));

    if installed.is_empty() {
        print_color("No packages are installed.", Color::Blue); // Dark
    } else {
        print_color("These packages are installed:", Color::Blue); // Dark
        for (name, version, _tops) in installed {
            print_color_(&name, Color::Cyan);
            print_color(&format!("=={}", version.to_string_color()), Color::White);
        }
        for req in path_reqs {
            print_color_(&req.name, Color::Cyan);
            print_color(
                &format!(", at path: {}", req.path.as_ref().unwrap()),
                Color::White,
            );
        }
    }

    if scripts.is_empty() {
        print_color("\nNo console scripts are installed.", Color::Blue); // Dark
    } else {
        print_color("\nThese console scripts are installed:", Color::Blue); // Dark
        for script in scripts {
            print_color(&script, Color::Cyan); // Dark
        }
    }
}

/// Find console scripts installed, by browsing the (custom) bin folder
pub fn find_console_scripts(bin_path: &Path) -> Vec<String> {
    let mut result = vec![];
    if !bin_path.exists() {
        return vec![];
    }

    for entry in bin_path
        .read_dir()
        .expect("Trouble opening bin path")
        .flatten()
    {
        if entry.file_type().unwrap().is_file() {
            result.push(entry.file_name().to_str().unwrap().to_owned())
        }
    }
    result
}
