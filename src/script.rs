use std::{fs, path::Path, str::FromStr};

use regex::Regex;

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

    // todo: Consider a metadata file, but for now, we'll use folders
    //    let scripts_data_path = script_env_path.join("scripts.toml");

    let env_path = util::canon_join(script_env_path, filename);
    if !env_path.exists() {
        fs::create_dir_all(&env_path).expect("Problem creating environment for the script");
    }

    // Write the version we found to a file.
    let cfg_vers;
    let py_vers_path = env_path.join("py_vers.txt");

    let script = fs::read_to_string(filename).expect("Problem opening the Python script file.");
    let dunder_python_vers = check_for_specified_py_vers(&script);

    if let Some(dpv) = dunder_python_vers {
        cfg_vers = dpv;
        create_or_update_version_file(&py_vers_path, &cfg_vers);
    } else if py_vers_path.exists() {
        cfg_vers = Version::from_str(
            &fs::read_to_string(py_vers_path)
                .expect("Problem reading Python version for this script")
                .replace("\n", ""),
        )
        .expect("Problem parsing version from file");
    } else {
        cfg_vers = util::prompts::py_vers();
        create_or_update_version_file(&py_vers_path, &cfg_vers);
    }

    // todo DRY
    let pypackages_dir = env_path.join("__pypackages__");
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
        fs::File::create(&py_vers_path)
            .expect("Problem creating a file to store the Python version for this script");
    }
    fs::write(py_vers_path, &cfg_vers.to_string()).expect("Problem writing Python version file.");
}

/// Find a script's Python version specificion by looking for the `__python__` variable.
///
/// If a `__python__` variable is identified, the version must have major, minor, and
/// patch components to be considered valid. Otherwise, there is still some ambiguity in
/// which version to use and an error is thrown.
fn check_for_specified_py_vers(script: &str) -> Option<Version> {
    let re = Regex::new(r#"^__python__\s*=\s*"(.*?)"$"#).unwrap();

    for line in script.lines() {
        if let Some(capture) = re.captures(line) {
            let specification = capture.get(1).unwrap().as_str();
            let (_, version) = parse_version(specification).unwrap();
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
                        "Problem parsing `__python__` variable. Make sure you've included \
                        major, minor, and patch specifications (eg `__python__ = X.Y.Z`)",
                    );
                }
            }
        }
    }
    None
}

/// Find a script's dependencies from a variable: `__requires__ = [dep1, dep2]`
fn find_deps_from_script(script: &str) -> Vec<String> {
    // todo: Helper for this type of logic? We use it several times in the program.
    let re = Regex::new(r"(?ms)^__requires__\s*=\s*\[(.*?)\]$").unwrap();

    let mut result = vec![];

    if let Some(c) = re.captures(script) {
        let deps_list = c.get(1).unwrap().as_str().to_owned();
        result = deps_list
            .split(',')
            .map(|d| {
                d.to_owned()
                    .replace(" ", "")
                    .replace("'", "")
                    .replace("\"", "")
                    .replace("\n", "")
            })
            .filter(|d| !d.is_empty())
            .collect();
    }
    result
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;
    use crate::dep_types::Version;

    #[test]
    fn parse_python_version_with_no_dunder_specified() {
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
    fn parse_python_version_with_valid_dunder_specified() {
        let script = indoc! { r#"
            __python__ = "3.9.1"

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
    fn parse_no_dependencies_with_no_requires() {
        let script = indoc! { r#"
            if __name__ == "__main__":
                print("Hello, world")
        "# };

        let expected: Vec<&str> = vec![];
        let actual = find_deps_from_script(script);

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_no_dependencies_with_single_line_requires() {
        let script = indoc! { r#"
            __requires__ = []

            if __name__ == "__main__":
                print("Hello, world")
        "# };

        let expected: Vec<&str> = vec![];
        let actual = find_deps_from_script(script);

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_no_dependencies_with_multi_line_requires() {
        let script = indoc! { r#"
            __requires__ = [
            ]

            if __name__ == "__main__":
                print("Hello, world")
        "# };

        let expected: Vec<&str> = vec![];
        let actual = find_deps_from_script(script);

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_one_dependency_with_single_line_requires() {
        let script = indoc! { r#"
            __requires__ = ["requests"]

            if __name__ == "__main__":
                print("Hello, world")
        "# };

        let expected: Vec<&str> = vec!["requests"];
        let actual = find_deps_from_script(script);

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_one_dependency_with_multi_line_requires() {
        let script = indoc! { r#"
            __requires__ = [
                "requests"
            ]

            if __name__ == "__main__":
                print("Hello, world")
        "# };

        let expected: Vec<&str> = vec!["requests"];
        let actual = find_deps_from_script(script);

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_multiple_dependencies_with_single_line_requires() {
        let script = indoc! { r#"
            __requires__ = ["python-dateutil", "requests"]

            if __name__ == "__main__":
                print("Hello, world")
        "# };

        let expected: Vec<&str> = vec!["python-dateutil", "requests"];
        let actual = find_deps_from_script(script);

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_multiple_dependencies_with_multi_line_requires() {
        let script = indoc! { r#"
            __requires__ = [
                "python-dateutil",
                "requests"
            ]

            if __name__ == "__main__":
                print("Hello, world")
        "# };

        let expected: Vec<&str> = vec!["python-dateutil", "requests"];
        let actual = find_deps_from_script(script);

        assert_eq!(expected, actual);
    }
}
