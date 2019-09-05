use crate::{dep_types::Req, util, Config};
use crossterm::Color;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};

/// This nested structure is required based on how the `toml` crate handles dots.
#[derive(Debug, Deserialize)]
pub struct Pyproject {
    pub tool: Tool,
}

#[derive(Debug, Deserialize)]
pub struct Tool {
    pub pypackage: Option<Pypackage>,
    pub poetry: Option<Poetry>,
}

#[serde(untagged)]
#[derive(Debug, Deserialize)]
/// Allows use of both Strings, ie "ipython = "^7.7.0", and maps: "ipython = {version = "^7.7.0", extras=["qtconsole"]}"
pub enum DepComponentWrapper {
    A(String),
    B(DepComponent),
}

#[serde(untagged)]
#[derive(Debug, Deserialize)]
pub enum DepComponentWrapperPoetry {
    A(String),
    B(DepComponentPoetry),
}

#[derive(Debug, Deserialize)]
pub struct DepComponent {
    #[serde(rename = "version")]
    pub constrs: String,
    pub extras: Option<Vec<String>>,
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub service: Option<String>,
    pub python: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DepComponentPoetry {
    #[serde(rename = "version")]
    pub constrs: String,
    pub python: Option<String>,
    pub extras: Option<Vec<String>>,
    pub optional: Option<bool>,
    // todo: more fields
    //    pub repository: Option<String>,
    //    pub branch: Option<String>,
    //    pub service: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Pypackage {
    pub py_version: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub author_email: Option<String>,
    pub license: Option<String>,
    pub description: Option<String>,
    pub classifiers: Option<Vec<String>>, // https://pypi.org/classifiers/
    pub keywords: Option<Vec<String>>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub repo_url: Option<String>,
    pub package_url: Option<String>,
    pub readme_filename: Option<String>,
    //    pub entry_points: Option<HashMap<String, Vec<String>>>,
    pub scripts: Option<HashMap<String, String>>, // todo. Maybe [tool.pypackage.scripts] , ie a standalone table?

    pub dependencies: Option<HashMap<String, DepComponentWrapper>>,

    pub dev_dependencies: Option<HashMap<String, String>>,
    pub extras: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub struct Poetry {
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    pub authors: Option<Vec<String>>,
    pub readme: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub documentation: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub classifiers: Option<Vec<String>>,
    pub packages: Option<Vec<String>>,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub extras: Option<HashMap<String, String>>,

    pub dependencies: Option<HashMap<String, DepComponentWrapperPoetry>>,
    // todo: Can't find dets on poetry docs, but apparently exists
    //    dev_dependencies: Option<HashMap<String, String>>,
    // todo: Include these
    //    pub source: Option<HashMap<String, String>>,
    pub scripts: Option<HashMap<String, String>>,
    //    pub extras: Option<HashMap<String, String>>,
}

/// Write dependencies to pyproject.toml. If an entry for tha = true;t package already exists, ask if
/// we should update the version. Assume we've already parsed the config, and are only
/// adding new reqs, or ones with a changed version.
pub fn add_reqs_to_cfg(filename: &str, added: &[Req]) {
    let mut result = String::new();
    let data = fs::read_to_string(filename)
        .expect("Unable to read pyproject.toml while attempting to add a dependency");

    let mut in_dep = false;
    let sect_re = Regex::new(r"^\[.*\]$").unwrap();

    // We collect lines here so we can start the index at a non-0 point.
    let lines_vec: Vec<&str> = data.lines().collect();

    for (i, line) in data.lines().enumerate() {
        result.push_str(line);
        result.push_str("\n");
        if line == "[tool.pypackage.dependencies]" {
            in_dep = true;
            continue;
        }

        if in_dep {
            let mut ready_to_insert = true;
            // Check if this is the last non-blank line in the dependencies section.
            for i2 in i..lines_vec.len() {
                let line2 = lines_vec[i2];
                // We've hit the end of the section or file without encountering a non-empty line.
                if sect_re.is_match(line2) || i2 == lines_vec.len() - 1 {
                    break;
                }
                if !line2.is_empty() {
                    // We haven't hit the end of the section yet; don't add the new reqs here.
                    ready_to_insert = false;
                    break;
                }
            }
            if ready_to_insert {
                for req in added {
                    result.push_str(&req.to_cfg_string());
                    result.push_str("\n");
                }
            }
        }
    }

    fs::write(filename, result)
        .expect("Unable to read pyproject.toml while attempting to add a dependency");
}

/// Remove dependencies from pyproject.toml.
pub fn remove_reqs_from_cfg(filename: &str, reqs: &[String]) {
    let mut result = String::new();
    let data = fs::read_to_string(filename)
        .expect("Unable to read pyproject.toml while attempting to add a dependency");

    let mut in_dep = false;
    let sect_re = Regex::new(r"^\[.*\]$").unwrap();

    for line in data.lines() {
        if line.starts_with('#') || line.is_empty() {
            // todo handle mid-line comements
            result.push_str(line);
            result.push_str("\n");
            continue;
        }

        if line == "[tool.pypackage.dependencies]" {
            in_dep = true;
            result.push_str(line);
            result.push_str("\n");
            continue;
        }

        if in_dep {
            if sect_re.is_match(line) {
                in_dep = false;
            }
            // todo: handle comments
            let req_line = match Req::from_str(line, false) {
                Ok(r) => r,
                Err(_) => {
                    util::abort(&format!(
                        "Can't parse this line in `pyproject.toml`: {}",
                        line
                    ));
                    Req::new(String::new(), vec![]) // todo temp to allow compiling
                }
            };

            if reqs
                .iter()
                .map(|r| r.to_lowercase())
                .any(|x| x == req_line.name.to_lowercase())
            {
                continue; // ie don't append this line to result.
            }
        }
        result.push_str(line);
        result.push_str("\n");
    }

    fs::write(filename, result)
        .expect("Unable to write to pyproject.toml while attempting to add a dependency");
}

pub fn parse_req_dot_text(cfg: &mut Config) {
    let file = match fs::File::open("requirements.txt") {
        Ok(f) => f,
        Err(_) => return,
    };

    for line in BufReader::new(file).lines() {
        if let Ok(l) = line {
            match Req::from_pip_str(&l) {
                Some(r) => {
                    cfg.reqs.push(r.clone());
                    util::print_color(
                        &format!("Added {} from requirements.txt", r.name),
                        Color::Green,
                    )
                }
                None => util::print_color(
                    &format!("Problem parsing {} from requirements.txt", l),
                    Color::Red,
                ),
            };
        }
    }
}

fn key_re(key: &str) -> Regex {
    // todo DRY from main
    Regex::new(&format!(r#"^{}\s*=\s*"(.*)"$"#, key)).unwrap()
}

// todo: Dry from config parsing!!
pub fn parse_pipfile(cfg: &mut Config) {
    let file = match fs::File::open("Pipfile") {
        Ok(f) => f,
        Err(_) => return,
    };

    let mut in_metadata = false;
    let mut in_dep = false;
    let mut _in_extras = false;

    let sect_re = Regex::new(r"\[.*\]").unwrap();

    for line in BufReader::new(file).lines() {
        if let Ok(l) = line {
            // todo replace this with something that clips off
            // todo post-# part of strings; not just ignores ones starting with #
            if l.starts_with('#') {
                continue;
            }

            if &l == "[[source]]" {
                in_metadata = true;
                in_dep = false;
                _in_extras = false;
                continue;
            } else if &l == "[packages]" {
                in_metadata = false;
                in_dep = true;
                _in_extras = false;
                continue;
            } else if &l == "[dev-packages]" {
                in_metadata = false;
                in_dep = false;
                // todo
                continue;
            } else if sect_re.is_match(&l) {
                in_metadata = false;
                in_dep = false;
                _in_extras = false;
                continue;
            }

            if in_metadata {
                // todo DRY
                // Pipfile deliberately only includes minimal metadata.
                if let Some(n2) = key_re("name").captures(&l) {
                    if let Some(n) = n2.get(1) {
                        cfg.name = Some(n.as_str().to_string());
                    }
                }
                if let Some(n2) = key_re("url").captures(&l) {
                    if let Some(n) = n2.get(1) {
                        cfg.homepage = Some(n.as_str().to_string());
                    }
                }
            } else if in_dep && !l.is_empty() {
                match Req::from_str(&l, false) {
                    Ok(r) => {
                        cfg.reqs.push(r.clone());
                        util::print_color(&format!("Added {} from Pipfile", r.name), Color::Green)
                    }
                    Err(_) => util::print_color(
                        &format!("Problem parsing {} from Pipfile", l),
                        Color::Red,
                    ),
                }
            }

            // todo: [requires] section has python_version.
        }
    }
}
