use std::process::Command;

// https://packaging.python.org/tutorials/packaging-projects/

/// Creates a temporary file which imitates setup.py
fn create_dummy_setup(cfg: crate::Config) {
    let classifiers = ""; // todo temp
                          // todo add to this
    let version = match cfg.version {
        Some(v) => v.to_string(),
        None => "".into(),
    };

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
}

fn cleanup_dummy_setup(filename: &str) {}

pub(crate) fn build(venv_name: &str, cfg: &crate::Config) {
    Command::new("./python")
        .current_dir(&format!("{}/bin", venv_name))
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

    Command::new("./python")
        .current_dir(&format!("{}/bin", venv_name))
        .args(&["setup.py", "sdist", "bdist_wheel"])
        .status()
        .expect("Problem building");
}

pub(crate) fn publish(venv_name: &str, cfg: &crate::Config) {
    let repo_url = cfg.repo_url.clone();

    Command::new("./python")
        .current_dir(&format!("{}/bin", venv_name))
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
