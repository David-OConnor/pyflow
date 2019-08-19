use crate::util;
use std::collections::HashMap;
use std::{env, fs, path::PathBuf, process::Command};

// https://packaging.python.org/tutorials/packaging-projects/

/// Serialize to a python list of strings.
fn serialize_py_list(items: &Vec<String>) -> String {
    let mut result = "[\n".to_string();
    for item in items.iter() {
        result.push_str(&format!("    \"{}\",\n", item));
    }
    result.push(']');
    result
}

/// Serialize to a Python dics of lists of strings.
fn serialize_py_dict(hm: &HashMap<String, Vec<String>>) -> String {
    let mut result = "{\n".to_string();
    for (key, val) in hm.iter() {
        result.push_str(&format!("    \"{}\": {}\n", key, serialize_py_list(val)));
    }
    result.push('}');
    result
}

/// Creates a temporary file which imitates setup.py
fn create_dummy_setup(cfg: &crate::Config, filename: &str) {
    let version = match cfg.version {
        Some(v) => v.to_string(),
        None => "".into(),
    };
    let cfg = cfg.clone();

    let data = format!(
        r#"import setuptools
 
with open("{}", "r") as fh:
    long_description = fh.read()

setuptools.setup(
    name="{}",
    version="{}",
    author="{}",
    author_email="{}",
    license="{}"
    description="{}",
    long_description=long_description,
    long_description_content_type="text/markdown",
    url="{}",
    packages=setuptools.find_packages(),
    classifiers={},
    entry_points={},
    extras_require={},
)
"#,
        cfg.readme_filename.unwrap_or_else(|| "README.md".into()),
        cfg.name.unwrap_or_else(|| "".into()),
        version,
        cfg.author.unwrap_or_else(|| "".into()),
        cfg.author_email.unwrap_or_else(|| "".into()),
        cfg.license.unwrap_or_else(|| "".into()),
        cfg.description.unwrap_or_else(|| "".into()),
        cfg.repo_url.unwrap_or_else(|| "".into()),
        serialize_py_list(&cfg.classifiers),
        serialize_py_dict(&cfg.entry_points),
        match cfg.extras {
            Some(e) => serialize_py_dict(&e),
            None => "".into(),
        }
    );

    fs::write(filename, data).expect("Problem writing dummy setup.py");
    if util::wait_for_dirs(&[env::current_dir()
        .expect("Problem finding current dir")
        .join(filename)])
    .is_err()
    {
        util::abort("Problem waiting for setup.py to be created.")
    };
}

pub(crate) fn build(
    bin_path: &PathBuf,
    lib_path: &PathBuf,
    cfg: &crate::Config,
    extras: Vec<String>,
) {
    // todo: Check if they exist; only install if they don't.
    let dummy_setup_fname = "setup_temp_pypackage.py";

    Command::new("./python")
        .current_dir(bin_path)
        .args(&[
            "-m", "pip", "install", //            "--upgrade",
            "twine", "wheel",
        ])
        .status()
        .expect("Problem installing Twine");

    create_dummy_setup(cfg, dummy_setup_fname);

    util::set_pythonpath(lib_path);
    println!("Building the package...");
    Command::new(format!("{}/{}", bin_path.to_str().unwrap(), "python"))
        .args(&[dummy_setup_fname, "sdist", "bdist_wheel"])
        .status()
        .expect("Problem building");
    println!("Build complete.");

    if fs::remove_file(dummy_setup_fname).is_err() {
        println!("Problem removing temporary setup file while building ")
    };
}

pub(crate) fn publish(bin_path: &PathBuf, cfg: &crate::Config) {
    let repo_url = cfg
        .package_url
        .clone()
        .unwrap_or_else(|| "https://test.pypi.org/legacy".to_string());

    println!("Uploading to {}", repo_url);
    Command::new(format!("{}/{}", bin_path.to_str().unwrap(), "twine"))
        .args(&[
            //            "-m",
            //            "twine upload",
            "upload",
            // todo - test repo / setting repos not working.
            //            &format!("--repository-url {}/", repo_url),
            "dist/*",
        ])
        .status()
        .expect("Problem publishing");
}

#[cfg(test)]
pub mod test {
    use super::*;

    #[test]
    fn py_list() {
        let expected = r#"[
    "Programming Language :: Python :: 3",
    "License :: OSI Approved :: MIT License",
    "Operating System :: OS Independent",
]"#;

        let actual = serialize_py_list(&vec![
            "Programming Language :: Python :: 3".into(),
            "License :: OSI Approved :: MIT License".into(),
            "Operating System :: OS Independent".into(),
        ]);

        assert_eq!(expected, actual);
    }

    #[test]
    fn py_dict() {
        let expected = r#"{
    "PDF": [
        "ReportLab>=1.2",
        "RXP"
    ],
    "reST": [
        "docutils>=0.3"
    ],
    }"#;

        let mut data = HashMap::new();
        data.insert("PDF".into(), vec!["ReportLab>=1.2".into(), "RXP".into()]);
        data.insert("reST".into(), vec!["docutils>=0.3".into()]);

        assert_eq!(expected, serialize_py_dict(&data));
    }
}
