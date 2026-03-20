//! This is for running standalone script files, not associated with a project directory.
//! It uses [PEP 723: Inline script metadata](https://peps.python.org/pep-0723/)
//! to manage dependencies. This allows scripts to be run standalone, for convenience.

use std::{fs, path::Path, str::FromStr};

use toml::Value;
use crate::{
    commands,
    dep_parser::parse_version,
    dep_resolution::res,
    dep_types::{Constraint, Extras, Lock, Req, ReqType, Version},
    util,
};

/// Run a standalone script file, with package management
/// todo: We're using script name as unique identifier; address this in the future,
/// todo perhaps with an id in a comment at the top of a file
pub fn run_script(
    script_env_path: &Path,
    dep_cache_path: &Path,
    os: util::Os,
    args: &[String],
    pyflow_dir: &Path,
) {
    #[cfg(debug_assertions)]
    eprintln!("Run script args: {:?}", args);

    // todo: DRY with run_cli_tool and subcommand::Install
    let filename = if let Some(arg) = args.get(0) {
        arg
    } else {
        util::abort(
            "`script` must be followed by the script to run, eg `pyflow script myscript.py`",
        );
    };

    let env_path = util::canon_join(script_env_path, filename);
    if !env_path.exists() {
        fs::create_dir_all(&env_path).expect("Problem creating environment for the script");
    }

    // Write the version we found to a file.
    let cfg_vers;
    let py_vers_path = env_path.join("py_vers.txt");

    let script = fs::read_to_string(filename).expect("Problem opening the Python script file.");
    let specified_py_vers = check_for_specified_py_vers(&script);

    if let Some(dpv) = specified_py_vers {
        cfg_vers = dpv;
        create_or_update_version_file(&py_vers_path, &cfg_vers);
    } else if py_vers_path.exists() {
        cfg_vers = Version::from_str(
            &fs::read_to_string(&py_vers_path)
                .expect("Problem reading Python version for this script")
                .replace("\n", ""),
        )
            .expect("Problem parsing version from file");
    } else {
        cfg_vers = util::prompts::py_vers();
        create_or_update_version_file(&py_vers_path, &cfg_vers);
    }

    // todo DRY
    let pypackages_dir = env_path.join(".venv");
    let (vers_path, py_vers) =
        util::find_or_create_venv(&cfg_vers, &pypackages_dir, pyflow_dir, dep_cache_path);

    let bin_path = util::find_bin_path(&vers_path);
    let lib_path = vers_path.join("lib");
    let script_path = vers_path.join("bin");
    let lock_path = env_path.join("pyproject.lock");

    let paths = util::Paths {
        bin: bin_path,
        lib: lib_path,
        entry_pt: script_path,
        cache: dep_cache_path.to_owned(),
    };

    let deps = find_deps_from_script(&script);

    let lock = match util::read_lock(&lock_path) {
        Ok(l) => l,
        Err(_) => Lock::default(),
    };

    let lockpacks = lock.package.unwrap_or_else(Vec::new);

    let reqs: Vec<Req> = deps
        .iter()
        .map(|name| {
            let (fmtd_name, version) = if let Some(lp) = lockpacks
                .iter()
                .find(|lp| util::compare_names(&lp.name, name))
            {
                (
                    lp.name.clone(),
                    Version::from_str(&lp.version).expect("Problem getting version"),
                )
            } else {
                let vinfo = res::get_version_info(
                    name,
                    Some(Req::new_with_extras(
                        name.to_string(),
                        vec![Constraint::new_any()],
                        Extras::new_py(Constraint::new(ReqType::Exact, py_vers.clone())),
                    )),
                )
                    .unwrap_or_else(|_| panic!("Problem getting version info for {}", &name));
                (vinfo.0, vinfo.1)
            };

            Req::new(fmtd_name, vec![Constraint::new(ReqType::Caret, version)])
        })
        .collect();

    util::deps::sync(
        &paths,
        &lockpacks,
        &reqs,
        &[],
        &[],
        os,
        &py_vers,
        &lock_path,
    );

    if commands::run_python(&paths.bin, &[paths.lib], args).is_err() {
        util::abort("Problem running this script")
    };
}

/// Create the `py_vers.txt` if it doesn't exist, and then store `cfg_vers` within.
fn create_or_update_version_file(py_vers_path: &Path, cfg_vers: &Version) {
    if !py_vers_path.exists() {
        fs::File::create(py_vers_path)
            .expect("Problem creating a file to store the Python version for this script");
    }
    fs::write(py_vers_path, cfg_vers.to_string()).expect("Problem writing Python version file.");
}

/// Extracts the PEP 723 metadata TOML block from a script.
fn extract_script_metadata(script: &str) -> Option<String> {
    let mut in_block = false;
    let mut toml_content = String::new();

    for line in script.lines() {
        let trimmed = line.trim();
        if !in_block {
            if trimmed == "# /// script" {
                in_block = true;
            }
        } else {
            if trimmed == "# ///" {
                return Some(toml_content);
            }
            if let Some(stripped) = line.strip_prefix("# ") {
                toml_content.push_str(stripped);
                toml_content.push('\n');
            } else if let Some(stripped) = line.strip_prefix('#') {
                toml_content.push_str(stripped);
                toml_content.push('\n');
            }
        }
    }
    None
}

/// Find a script's Python version specification by looking for `requires-python`
/// in the PEP 723 metadata block.
fn check_for_specified_py_vers(script: &str) -> Option<Version> {
    let toml_block = extract_script_metadata(script)?;

    // Parse the extracted block into a TOML Value
    let parsed_toml = toml_block.parse::<Value>().ok()?;

    if let Some(req_py) = parsed_toml.get("requires-python").and_then(|v| v.as_str()) {
        // PEP 723 specifiers usually include operators (e.g., ">=3.9.1").
        // We strip non-numeric characters to get the raw semver for parsing.
        let cleaned_spec = req_py.trim_start_matches(|c: char| !c.is_numeric());

        if let Ok((_, version)) = parse_version(cleaned_spec) {
            match version {
                Version {
                    major: Some(_),
                    minor: Some(_),
                    patch: Some(_),
                    extra_num: None,
                    modifier: None,
                    ..
                } => return Some(version),
                _ => {
                    util::abort(
                        "Problem parsing `requires-python`. Make sure you've included \
                        major, minor, and patch specifications (eg `requires-python = \">=X.Y.Z\"`)",
                    );
                }
            }
        }
    }
    None
}

/// Find a script's dependencies by parsing the `dependencies` array in the PEP 723 block.
fn find_deps_from_script(script: &str) -> Vec<String> {
    let toml_block = match extract_script_metadata(script) {
        Some(b) => b,
        None => return vec![],
    };

    // Parse the TOML block and safely navigate to the dependencies array
    if let Ok(parsed_toml) = toml_block.parse::<Value>() {
        if let Some(deps_array) = parsed_toml.get("dependencies").and_then(|v| v.as_array()) {
            return deps_array
                .iter()
                .filter_map(|val| val.as_str()) // Only keep valid strings
                .map(|s| s.to_string())
                .collect();
        }
    }

    vec![] // Return an empty vector if parsing fails or 'dependencies' is missing
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;
    use crate::dep_types::Version;

    #[test]
    fn parse_python_version_with_no_metadata() {
        let script = indoc! { r#"
            if __name__ == "__main__":
                print("Hello, world")
        "# };

        let version: Option<Version> = None;
        let expected = version;
        let actual = check_for_specified_py_vers(script);

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_python_version_with_valid_metadata() {
        let script = indoc! { r#"
            # /// script
            # requires-python = ">=3.9.1"
            # ///

            if __name__ == "__main__":
                print("Hello, world")
        "# };

        let version: Option<Version> = Some(Version {
            major: Some(3),
            minor: Some(9),
            patch: Some(1),
            extra_num: None,
            modifier: None,
            star: false,
        });

        let expected = version;
        let actual = check_for_specified_py_vers(script);

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_no_dependencies_with_no_metadata() {
        let script = indoc! { r#"
            if __name__ == "__main__":
                print("Hello, world")
        "# };

        let expected: Vec<&str> = vec![];
        let actual = find_deps_from_script(script);

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_no_dependencies_with_single_line() {
        let script = indoc! { r#"
            # /// script
            # dependencies = []
            # ///

            if __name__ == "__main__":
                print("Hello, world")
        "# };

        let expected: Vec<&str> = vec![];
        let actual = find_deps_from_script(script);

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_no_dependencies_with_multi_line() {
        let script = indoc! { r#"
            # /// script
            # dependencies = [
            # ]
            # ///

            if __name__ == "__main__":
                print("Hello, world")
        "# };

        let expected: Vec<&str> = vec![];
        let actual = find_deps_from_script(script);

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_one_dependency_with_single_line() {
        let script = indoc! { r#"
            # /// script
            # dependencies = ["requests"]
            # ///

            if __name__ == "__main__":
                print("Hello, world")
        "# };

        let expected: Vec<&str> = vec!["requests"];
        let actual = find_deps_from_script(script);

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_one_dependency_with_multi_line() {
        let script = indoc! { r#"
            # /// script
            # dependencies = [
            #     "requests"
            # ]
            # ///

            if __name__ == "__main__":
                print("Hello, world")
        "# };

        let expected: Vec<&str> = vec!["requests"];
        let actual = find_deps_from_script(script);

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_multiple_dependencies_with_single_line() {
        let script = indoc! { r#"
            # /// script
            # dependencies = ["python-dateutil", "requests"]
            # ///

            if __name__ == "__main__":
                print("Hello, world")
        "# };

        let expected: Vec<&str> = vec!["python-dateutil", "requests"];
        let actual = find_deps_from_script(script);

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_multiple_dependencies_with_multi_line() {
        let script = indoc! { r#"
            # /// script
            # dependencies = [
            #     "python-dateutil",
            #     "requests"
            # ]
            # ///

            if __name__ == "__main__":
                print("Hello, world")
        "# };

        let expected: Vec<&str> = vec!["python-dateutil", "requests"];
        let actual = find_deps_from_script(script);

        assert_eq!(expected, actual);
    }
}