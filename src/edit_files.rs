use crate::{
    dep_types::{Dependency, VersionReq},
    util, Config,
};
use regex::Regex;
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::str::FromStr;
use std::{fs, io};
use termion::{color, style};

/// Write dependencies to pyproject.toml. If an entry for that package already exists, ask if
/// we should update the version.
pub fn add_dependencies(filename: &str, added: &[Dependency]) {
    if !added.is_empty() {
        println!("{}Adding dependencies via the CLI is not yet supported. Please specify dependencies in `pyproject.toml`.{}", color::Fg(color::Yellow), style::Reset);
        return;
    }

    //        let data = fs::read_to_string("pyproject.toml")
    //            .expect("Unable to read pyproject.toml while attempting to add a dependency");
    let file = fs::File::open(filename).expect("cannot open pyproject.toml");

    let mut in_dep = false;
    let sect_re = Regex::new(r"\[.*\]").unwrap(); // todo: Will this catch double-bracket sections?

    // todo: use this? https://doc.rust-lang.org/std/macro.writeln.html

    // todo: Handle Vec<VersionReq> vs VersionReq.
    let mut already_installed = vec![];

    let mut result = String::new();

    for line in BufReader::new(&file).lines() {
        //    for line in data.lines() {
        if let Ok(l) = line {
            result.push_str(&l);
            result.push_str("\n");
            // todo replace this with something that clips off
            // todo post-# part of strings; not just ignores ones starting with #
            if l.starts_with('#') {
                continue;
            }

            if &l == "[tool.pypackage.dependencies]" {
                in_dep = true;
                continue;
            } else if sect_re.is_match(&l) {
                in_dep = false;
                continue;
            }

            if in_dep {
                if let Ok(req) = Dependency::from_str(&l, false) {
                    already_installed.push(req);
                } else {
                    util::abort(&format!(
                        "Problem reading dependency {} in `pyproject.toml`",
                        &l
                    ));
                }
            }
        }
    }

    // Determine how to handle duplicates
    for added in added {
        for installed in already_installed.iter() {
            if installed.name.to_lowercase() == added.name.to_lowercase() {
                // todo ugly output due to Vec<VersionReq>
                println!(
                    "{} is already included in `pyproject.toml`. Do you want to update its \
                     version requirement from {:?} to {:?}?",
                    added.name, installed.version_reqs, added.version_reqs
                );

                let mut input = String::new();
                io::stdin()
                    .read_line(&mut input)
                    .expect("Unable to read user input in overwrite prompt");

                let input = input
                    .chars()
                    .next()
                    .expect("Problem reading input")
                    .to_string()
                    .to_lowercase();

                if input == "yes" || input == "y" {
                    println!("Not yet implemented");
                } else {

                }
                println!("Not yet implemented");
            }
        }
    }

    // todo: DRY: Clean this up.
    // Now that we've determined which dependencies are already installed, add new ones.
    for line in BufReader::new(file).lines() {
        //    for line in data.lines() {
        if let Ok(l) = line {
            result.push_str(&l);
            result.push_str("\n");
            // todo replace this with something that clips off
            // todo post-# part of strings; not just ignores ones starting with #
            if l.starts_with('#') {
                continue;
            }

            if &l == "[tool.pypackage.dependencies]" {
                in_dep = true;
                continue;
            } else if sect_re.is_match(&l) {
                in_dep = false;
                continue;
            }

            if in_dep {
                // There should be no more parsing errors here, since this is our second pass.
                let req = VersionReq::from_str(&l).unwrap();
            }
        }
    }

    fs::write("pyproject.toml", result)
        .expect("Unable to read pyproject.toml while attempting to add a dependency");
}

pub fn parse_req_dot_text(cfg: &mut Config) {
    let file = match fs::File::open("requirements.txt") {
        Ok(f) => f,
        Err(_) => return,
    };

    for line in BufReader::new(file).lines() {
        if let Ok(l) = line {
            match Dependency::from_pip_str(&l) {
                Some(d) => {
                    cfg.dependencies.push(d.clone());
                    println!("Added {} from requirements.txt", d.to_cfg_string())
                }
                None => println!("Problem parsing {} from requirements.txt", &l),
            };
        }
    }
}

#[derive(Debug, Deserialize)]
struct PipfileSource {
    url: Option<String>,
    //    verify_ssl: Option<bool>,
    name: Option<String>,
    // todo: Populate rest
}

#[derive(Debug, Deserialize)]
struct PipfileRequires {
    python_version: String,
}

/// https://github.com/pypa/pipfile
#[derive(Debug, Deserialize)]
struct Pipfile {
    source: Vec<PipfileSource>, //    source: Vec<Option<PipfileSource>>,
                                //    requires: Option<PipfileRequires>,
                                //    requires: Vec<String>,
                                //    packages: Option<Vec<String>>, //    dev_packages: Option<Vec<String>>  // todo currently unimplemented
}

#[derive(Debug, Deserialize)]
struct Poetry {
    // etc
    name: String,
    version: Option<String>,
    description: Option<String>,
    license: Option<String>,
    authors: Option<String>,
    readme: Option<String>,
    homepage: Option<String>,
    repository: Option<String>,
    documentation: Option<String>,
    keywords: Option<Vec<String>>,
    classifiers: Option<Vec<String>>,
    packages: Option<Vec<String>>,
    include: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
}

/// https://poetry.eustace.io/docs/pyproject/
#[derive(Debug, Deserialize)]
struct PoetryPyproject {
    #[serde(alias = "tool.poetry")]
    poetry: Poetry,
    #[serde(alias = "tool.poetry.dependencies")]
    dependencies: Option<Vec<String>>,
    #[serde(alias = "tool.poetry.source")]
    source: Option<Vec<String>>,
    #[serde(alias = "tool.poetry.scripts")]
    scripts: Option<Vec<String>>,
    #[serde(alias = "tool.poetry.extras")]
    extras: Option<Vec<String>>,
}

pub fn parse_pipfile(cfg: &mut Config) {
    let data = match fs::read_to_string("Pipfile") {
        Ok(d) => d,
        Err(_) => return,
    };

    //    let t: Config = toml::from_str(&data).unwrap();
    let pipfile: Pipfile = match toml::from_str(&data) {
        Ok(p) => p,
        Err(_) => {
            println!("Problem parsing Pipfile - skipping");
            return;
        }
    };
    //    if let Some(deps) = pipfile.packages {
    //        for dep in deps.into_iter() {
    //            match Dependency::from_str(&dep, false) {
    //                Ok(parsed) => {
    //                    cfg.dependencies.push(parsed.clone());
    //                    println!("Added {} from requirements.txt", parsed.to_cfg_string());
    //                }
    //                Err(_) => {
    //                    println!("Problem parsing {} from Pipfile - skipping", dep);
    //                }
    //            }
    //        }
    //    }

    // Pipfile deliberately only includes minimal metadata.
    //    if let Some(metadata) = pipfile.source {
    //        if let Some(name) = metadata.name {
    //            if cfg.name.is_none() {
    //                cfg.name = Some(name)
    //            }
    //        }
    //        if let Some(url) = metadata.url {
    //            if cfg.homepage.is_none() {
    //                cfg.homepage = Some(url)
    //            }
    //        }
    //    }

    //    if let Some(requires) = pipfile.requires {
    //        if cfg.py_version.is_none() {
    //            if let Some(py_v) = Version::from_str2(&requires.python_version) {
    //                if cfg.py_version.is_none() {
    //                    cfg.py_version = Some(py_v)
    //                }
    //            }
    //        }
    //    }
}

pub fn parse_poetry(cfg: &mut Config) {}

/// Create or update a `pyproject.toml` file.
pub fn update_pyproject(cfg: &Config) {}

/// Remove dependencies from pyproject.toml
pub fn remove_dependencies(filename: &str, dependencies: &[Dependency]) {
    let data = fs::read_to_string("pyproject.toml")
        .expect("Unable to read pyproject.toml while attempting to add a dependency");

    // todo
    let new_data = data;

    fs::write(filename, new_data)
        .expect("Unable to read pyproject.toml while attempting to add a dependency");
}
