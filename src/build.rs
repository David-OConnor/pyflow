use crate::util;
use std::{env, fs, path::PathBuf, process::Command};

// https://packaging.python.org/tutorials/packaging-projects/

/// Creates a temporary file which imitates setup.py
fn create_dummy_setup(cfg: &crate::Config) {
    let classifiers = ""; // todo temp
                          // todo add to this
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
    description="{}",
    long_description=long_description,
    long_description_content_type="text/markdown",
    url="{}",
    packages=setuptools.find_packages(),
    classifiers=[{}],
)
"#,
        cfg.readme_filename.unwrap_or_else(|| "README.md".into()),
        cfg.name.unwrap_or_else(|| "".into()),
        version,
        cfg.author.unwrap_or_else(|| "".into()),
        cfg.author_email.unwrap_or_else(|| "".into()),
        cfg.description.unwrap_or_else(|| "".into()),
        cfg.repo_url.unwrap_or_else(|| "".into()),
        classifiers,
    );

    fs::write("setup.py", data).expect("Problem writing dummy setup.py");
    if util::wait_for_dirs(&[env::current_dir()
        .expect("Problem finding current dir")
        .join("setup.py")])
    .is_err()
    {
        util::abort("Problem waiting for setup.py to be created.")
    };
}

fn cleanup_dummy_setup(filename: &str) {}

pub(crate) fn build(bin_path: &PathBuf, lib_path: &PathBuf, cfg: &crate::Config) {
    // todo: Check if they exist; only install if they don't.
    println!("Installing build tools...");
    Command::new("./python")
        .current_dir(bin_path)
        .args(&[
            "-m",
            "pip",
            "install",
            "--upgrade",
            "setuptools",
            "twine",
            "wheel",
        ])
        .status()
        .expect("Problem building");

    create_dummy_setup(cfg);

    util::set_pythonpath(lib_path);
    println!("Building the package...");
    //    Command::new("./python")
    //        .current_dir(bin_path)
    Command::new("./__pypackages__/3.7/venv/bin/python")
        .args(&["setup.py", "sdist", "bdist_wheel"])
        .status()
        .expect("Problem building");
    println!("Build complete.");
}

pub(crate) fn publish(bin_path: &PathBuf, cfg: &crate::Config) {
    let repo_url = cfg.repo_url.clone();

    Command::new("./python")
        .current_dir(bin_path)
        .args(&[
            "-m",
            "twine_upload",
            &format!(
                "--{}",
                repo_url.expect("Can't find repo url when publishing")
            ),
        ])
        .status()
        .expect("Problem publishing");
}
