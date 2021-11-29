use std::path::Path;

use termcolor::Color;

use crate::{
    dep_types::Req,
    util::{self, print_color, print_color_},
};

/// List all installed dependencies and console scripts, by examining the `libs` and `bin` folders.
/// Also include path requirements, which won't appear in the `lib` folder.
pub fn list(lib_path: &Path, path_reqs: &[Req]) {
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
