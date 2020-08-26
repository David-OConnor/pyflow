use crate::{dep_types::Req, util};
use crossterm::Color;
use regex::Regex;
use std::collections::HashMap;
use std::{env, fs, path::PathBuf, process::Command};

// https://packaging.python.org/tutorials/packaging-projects/

/// Serialize to a python list of strings.
fn serialize_py_list(items: &[String], indent_level: u8) -> String {
    let mut pad = "".to_string();
    for _ in 0..indent_level {
        pad.push_str("    ");
    }

    let mut result = "[\n".to_string();
    for item in items.iter() {
        result.push_str(&format!("{}    \"{}\",\n", &pad, item));
    }
    result.push_str(&pad);
    result.push(']');
    result
}

/// Serialize to a Python dict of lists of strings.
fn _serialize_py_dict(hm: &HashMap<String, Vec<String>>) -> String {
    let mut result = "{\n".to_string();
    for (key, val) in hm.iter() {
        result.push_str(&format!("    \"{}\": {}\n", key, serialize_py_list(val, 0)));
    }
    result.push('}');
    result
}

/// Serialize to a Python dict of strings.
//fn serialize_scripts(hm: &HashMap<String, String>) -> String {
//    let mut result = "{\n".to_string();
//
//    for (key, val) in hm.iter() {
//        result.push_str(&format!("    \"{}\": {}\n", key, serialize_py_list(val)));
//    }
//    result.push('}');
//    result
//}

///// A different format, as used in console_scripts
//fn serialize_py_dict2(hm: &HashMap<String, String>) -> String {
//    let mut result = "{\n".to_string();
//    for (key, val) in hm.iter() {
//        result.push_str(&format!("    \"{}\": {}\n", key, serialize_py_list(val)));
//    }
//    result.push('}');
//    result
//}

fn cfg_to_setup(cfg: &crate::Config) -> String {
    let cfg = cfg.clone();

    let version = match cfg.version {
        Some(v) => v.to_string(),
        None => "".into(),
    };

    let mut keywords = String::new();
    for (i, kw) in cfg.keywords.iter().enumerate() {
        if i != 0 {
            keywords.push_str(" ");
        }
        keywords.push_str(kw);
    }

    let author_re = Regex::new(r"^(.*?)\s*(?:<(.*?)>)?\s*$").unwrap();

    let mut author = "".to_string();
    let mut author_email = "".to_string();
    if let Some(first) = cfg.authors.get(0) {
        let caps = if let Some(c) = author_re.captures(first) {
            c
        } else {
            util::abort(&format!(
                "Problem parsing the `authors` field in `pyproject.toml`: {:?}",
                &cfg.authors
            ));
            unreachable!()
        };
        author = caps.get(1).unwrap().as_str().to_owned();
        author_email = caps.get(2).unwrap().as_str().to_owned();
    }

    let deps: Vec<String> = cfg.reqs.iter().map(Req::to_setup_py_string).collect();

    // todo: Entry pts!
    format!(
        r#"import setuptools

with open("{}", "r") as fh:
    long_description = fh.read()

setuptools.setup(
    name="{}",
    version="{}",
    author="{}",
    author_email="{}",
    license="{}",
    description="{}",
    long_description=long_description,
    long_description_content_type="text/markdown",
    url="{}",
    packages=setuptools.find_packages(),
    keywords="{}",
    classifiers={},
    python_requires="{}",
    install_requires={},
)
"#,
        //            entry_points={{
        //        "console_scripts": ,
        //    }},
        cfg.readme.unwrap_or_else(|| "README.md".into()),
        cfg.name.unwrap_or_else(|| "".into()),
        version,
        author,
        author_email,
        cfg.license.unwrap_or_else(|| "".into()),
        cfg.description.unwrap_or_else(|| "".into()),
        cfg.homepage.unwrap_or_else(|| "".into()),
        keywords,
        serialize_py_list(&cfg.classifiers, 1),
        //        serialize_py_list(&cfg.console_scripts),
        cfg.python_requires.unwrap_or_else(|| "".into()),
        serialize_py_list(&deps, 1),
        // todo:
        //            extras_require="{}",
        //        match cfg.extras {
        //            Some(e) => serialize_py_dict(&e),
        //            None => "".into(),
        //        }
    )
}

/// Creates a temporary file which imitates setup.py
fn create_dummy_setup(cfg: &crate::Config, filename: &str) {
    fs::write(filename, cfg_to_setup(cfg)).expect("Problem writing dummy setup.py");
    if util::wait_for_dirs(&[env::current_dir()
        .expect("Problem finding current dir")
        .join(filename)])
    .is_err()
    {
        util::abort("Problem waiting for setup.py to be created.")
    };
}

pub fn build(
    lockpacks: &[crate::dep_types::LockPackage],
    paths: &util::Paths,
    cfg: &crate::Config,
    _extras: &[String],
) {
    for lp in lockpacks.iter() {
        if lp.rename.is_some() {
            //    if lockpacks.iter().any(|lp| lp.rename.is_some()) {
            util::abort(&format!(
                "{} is installed with multiple versions. We can't create a package that \
                 relies on multiple versions of a dependency - \
                 this would cause this package not work work correctly if not used with pyflow.",
                lp.name
            ))
        }
    }

    let dummy_setup_fname = "setup_temp_pyflow.py";

    // Twine has too many dependencies to install when the environment, like we do with `wheel`, and
    // for now, it's easier to install using pip
    // todo: Install using own tools instead of pip; this is the last dependence on pip.
    let output = Command::new(paths.bin.join("python"))
        .args(&["-m", "pip", "install", "twine"])
        .output()
        .expect("Problem installing Twine");
    util::check_command_output(&output, "failed to install twine");

    //    let twine_url = "https://files.pythonhosted.org/packages/c4/43/b9c56d378f5d0b9bee7be564b5c5fb65c65e5da6e82a97b6f50c2769249a/twine-2.0.0-py3-none-any.whl";
    //    install::download_and_install_package(
    //        "twine",
    //        &Version::new(2, 0, 0),
    //        twine_url,
    //        "twine-2.0.0-py3-none-any.whl",
    //        "5319dd3e02ac73fcddcd94f0…1f4699d57365199d85261e1",
    //        &paths,
    //        install::PackageType::Wheel,
    //        &None,
    //    )
    //    .expect("Problem installing `twine`");

    create_dummy_setup(cfg, dummy_setup_fname);

    util::set_pythonpath(&[paths.lib.to_owned()]);
    println!("🛠️️ Building the package...");
    // todo: Run build script first, right?
    if let Some(build_file) = &cfg.build {
        let output = Command::new(paths.bin.join("python"))
            .arg(&build_file)
            .output()
            .unwrap_or_else(|_| panic!("Problem building using {}", build_file));
        util::check_command_output(&output, "failed to run build script");
    }

    //    Command::new(paths.bin.join("python"))
    //        .args(&[dummy_setup_fname, "sdist", "bdist_wheel"])
    //        .status()
    //        .expect("Problem building");

    util::print_color("Build complete.", Color::Green);

    if fs::remove_file(dummy_setup_fname).is_err() {
        println!("Problem removing temporary setup file while building ")
    };
}

pub(crate) fn publish(bin_path: &PathBuf, cfg: &crate::Config) {
    let repo_url = match cfg.package_url.clone() {
        Some(pu) => {
            let mut r = pu;
            if !r.ends_with('/') {
                r.push('/');
            }
            r
        }
        None => "https://test.pypi.org/legacy/".to_string(),
    };

    println!("Uploading to {}", repo_url);
    let output = Command::new(bin_path.join("twine"))
        .args(&["upload", "--repository-url", &repo_url, "dist/*"])
        .output()
        .expect("Problem publishing");
    util::check_command_output(&output, "publishing");
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::dep_types::{
        Constraint, Req,
        ReqType::{Caret, Exact},
        Version,
    };

    #[test]
    fn setup_creation() {
        let mut scripts = HashMap::new();
        scripts.insert("activate".into(), "jeejah:activate".into());

        let cfg = crate::Config {
            name: Some("everythingkiller".into()),
            py_version: Some(Version::new_short(3, 6)),
            version: Some(Version::new_short(0, 1)),
            authors: vec!["Fraa Erasmas <raz@edhar.math>".into()],
            homepage: Some("https://everything.math".into()),
            description: Some("Small, but packs a punch!".into()),
            repository: Some("https://github.com/raz/everythingkiller".into()),
            license: Some("MIT".into()),
            keywords: vec!["nanotech".into(), "weapons".into()],
            classifiers: vec![
                "Topic :: System :: Hardware".into(),
                "Topic :: Scientific/Engineering :: Human Machine Interfaces".into(),
            ],
            python_requires: Some(">=3.6".into()),
            package_url: Some("https://upload.pypi.org/legacy/".into()),
            scripts,
            readme: Some("README.md".into()),
            reqs: vec![
                Req::new(
                    "numpy".into(),
                    vec![Constraint::new(Caret, Version::new(1, 16, 4))],
                ),
                Req::new(
                    "manimlib".into(),
                    vec![Constraint::new(Exact, Version::new(0, 1, 8))],
                ),
                Req::new(
                    "ipython".into(),
                    vec![Constraint::new(Caret, Version::new(7, 7, 0))],
                ),
            ],
            dev_reqs: vec![Req::new(
                "black".into(),
                vec![Constraint::new(Caret, Version::new(18, 0, 0))],
            )],
            extras: HashMap::new(),
            repo_url: None,
            build: None,
        };

        let expected = r#"import setuptools

with open("README.md", "r") as fh:
    long_description = fh.read()

setuptools.setup(
    name="everythingkiller",
    version="0.1.0",
    author="Fraa Erasmas",
    author_email="raz@edhar.math",
    license="MIT",
    description="Small, but packs a punch!",
    long_description=long_description,
    long_description_content_type="text/markdown",
    url="https://everything.math",
    packages=setuptools.find_packages(),
    keywords="nanotech weapons",
    classifiers=[
        "Topic :: System :: Hardware",
        "Topic :: Scientific/Engineering :: Human Machine Interfaces",
    ],
    python_requires=">=3.6",
    install_requires=[
        "numpy>=1.16.4",
        "manimlib==0.1.8",
        "ipython>=7.7.0",
    ],
)
"#;

        assert_eq!(expected, &cfg_to_setup(&cfg));
    }

    #[test]
    fn py_list() {
        let expected = r#"[
    "Programming Language :: Python :: 3",
    "License :: OSI Approved :: MIT License",
    "Operating System :: OS Independent",
]"#;

        let actual = serialize_py_list(
            &vec![
                "Programming Language :: Python :: 3".into(),
                "License :: OSI Approved :: MIT License".into(),
                "Operating System :: OS Independent".into(),
            ],
            0,
        );

        assert_eq!(expected, actual);
    }

    // todo: Re-impl if you end up using this
    //    #[test]
    //    fn py_dict() {
    //        let expected = r#"{
    //    "PDF": [
    //        "ReportLab>=1.2",
    //        "RXP"
    //    ],
    //    "reST": [
    //        "docutils>=0.3"
    //    ],
    //    }"#;
    //
    //        let mut data = HashMap::new();
    //        data.insert("PDF".into(), vec!["ReportLab>=1.2".into(), "RXP".into()]);
    //        data.insert("reST".into(), vec!["docutils>=0.3".into()]);
    //
    //        assert_eq!(expected, serialize_py_dict(&data));
    //    }
}
