use crate::{
    dep_types::{Req, Version},
    util, Config,
};
use crossterm::Color;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// This nested structure is required based on how the `toml` crate handles dots.
#[derive(Debug, Deserialize)]
pub struct Pyproject {
    pub tool: Tool,
}

#[derive(Debug, Deserialize)]
pub struct Tool {
    pub pyflow: Option<Pyflow>,
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
    pub constrs: Option<String>,
    pub extras: Option<Vec<String>>,
    pub git: Option<String>,
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
pub struct Pyflow {
    pub py_version: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub authors: Option<Vec<String>>,
    //    pub author_email: Option<String>,
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
    pub scripts: Option<HashMap<String, String>>,
    pub python_requires: Option<String>,
    pub dependencies: Option<HashMap<String, DepComponentWrapper>>,
    #[serde(rename = "dev-dependencies")]
    pub dev_dependencies: Option<HashMap<String, DepComponentWrapper>>,
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
    pub dev_dependencies: Option<HashMap<String, DepComponentWrapperPoetry>>,
    // todo: Include these
    //    pub source: Option<HashMap<String, String>>,
    pub scripts: Option<HashMap<String, String>>,
    //    pub extras: Option<HashMap<String, String>>,
}

/// Split from `add_reqs_to_cfg` to accomodate testing
fn update_cfg(cfg_data: &str, added: &[Req], added_dev: &[Req]) -> String {
    let mut result = String::new();
    let mut in_dep = false;
    let mut in_dev_dep = false;
    let sect_re = Regex::new(r"^\[.*\]$").unwrap();

    // We collect lines here so we can start the index at a non-0 point.
    let lines_vec: Vec<&str> = cfg_data.lines().collect();

    // todo: Lots of DRY between dep and dev dep
    let mut dep_start = 0;
    let mut dev_dep_start = 0;
    let mut dep_end = 0;
    let mut dev_dep_end = 0;

    for (i, line) in cfg_data.lines().enumerate() {
        if &line.replace(" ", "") == "[tool.pyflow.dependencies]" {
            dep_start = i + 1;
            if in_dev_dep {
                dev_dep_end = i - 1;
            }
            in_dep = true;
            in_dev_dep = false;
            continue; // Continue so this line doesn't trigger the section's end.
        }

        if &line.replace(" ", "") == "[tool.pyflow.dev-dependencies]" {
            dev_dep_start = i + 1;
            if in_dep {
                dep_end = i - 1;
            }
            in_dep = false;
            in_dev_dep = true;
            continue;
        }

        // We've found the end of the dependencies section.
        if in_dep && (sect_re.is_match(line) || i == lines_vec.len() - 1) {
            in_dep = false;
            dep_end = i - 1;
        }

        if in_dev_dep && (sect_re.is_match(line) || i == lines_vec.len() - 1) {
            in_dev_dep = false;
            dev_dep_end = i - 1;
        }
    }

    let mut insertion_pt = dep_start;
    if dep_start != 0 {
        for i in dep_start..=dep_end {
            let line = lines_vec[i];
            if !line.is_empty() {
                insertion_pt = i + 1
            }
        }
    }

    let mut dev_insertion_pt = dev_dep_start;
    if dev_dep_start != 0 {
        for i in dev_dep_start..=dev_dep_end {
            let line = lines_vec[i];
            if !line.is_empty() {
                dev_insertion_pt = i + 1
            }
        }
    }

    for (i, line) in cfg_data.lines().enumerate() {
        if i == insertion_pt && dep_start != 0 {
            for req in added {
                result.push_str(&req.to_cfg_string());
                result.push_str("\n");
            }
        }
        if i == dev_insertion_pt && dev_dep_start != 0 {
            for req in added_dev {
                result.push_str(&req.to_cfg_string());
                result.push_str("\n");
            }
        }
        result.push_str(line);
        result.push_str("\n");
    }

    // If the sections don't exist, create them.
    // todo: Adjust start pad as needed so there's exactly two blank lines before adding the section.
    if dep_start == 0 {
        // todo: Should add dependencies section before dev deps section.
        result.push_str("\n[tool.pyflow.dependencies]\n");
        for req in added {
            result.push_str(&req.to_cfg_string());
            result.push_str("\n");
        }
        if dev_dep_start != 0 {
            // We only need to add one end-of-file pad.
            result.push_str("\n");
        }
    }

    if dev_dep_start == 0 {
        result.push_str("\n[tool.pyflow.dev-dependencies]\n");
        for req in added_dev {
            result.push_str(&req.to_cfg_string());
            result.push_str("\n");
        }
        result.push_str("\n");
    }

    result
}

/// Write dependencies to pyproject.toml. If an entry for that package already exists, ask if
/// we should update the version. Assume we've already parsed the config, and are only
/// adding new reqs, or ones with a changed version.
pub fn add_reqs_to_cfg(filename: &str, added: &[Req], added_dev: &[Req]) {
    let data = fs::read_to_string(filename)
        .expect("Unable to read pyproject.toml while attempting to add a dependency");

    let updated = update_cfg(&data, added, added_dev);
    fs::write(filename, updated)
        .expect("Unable to write pyproject.toml while attempting to add a dependency");
}

/// Remove dependencies from pyproject.toml.
pub fn remove_reqs_from_cfg(filename: &str, reqs: &[String]) {
    // todo: Handle removing dev deps.
    // todo: DRY from parsing the config.
    let mut result = String::new();
    let data = fs::read_to_string(filename)
        .expect("Unable to read pyproject.toml while attempting to add a dependency");

    let mut in_dep = false;
    let mut _in_dev_dep = false;
    let sect_re = Regex::new(r"^\[.*\]$").unwrap();

    for line in data.lines() {
        if line.starts_with('#') || line.is_empty() {
            // todo handle mid-line comements
            result.push_str(line);
            result.push_str("\n");
            continue;
        }

        if line == "[tool.pyflow.dependencies]" {
            in_dep = true;
            _in_dev_dep = false;
            result.push_str(line);
            result.push_str("\n");
            continue;
        }

        if line == "[tool.pyflow.dev-dependencies]" {
            in_dep = true;
            _in_dev_dep = false;
            result.push_str(line);
            result.push_str("\n");
            continue;
        }

        if in_dep {
            if sect_re.is_match(line) {
                in_dep = false;
            }
            // todo: handle comments
            let req_line = if let Ok(r) = Req::from_str(line, false) {
                r
            } else {
                util::abort(&format!(
                    "Can't parse this line in `pyproject.toml`: {}",
                    line
                ));
                unreachable!()
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

/// Update the config file with a new version.
pub fn change_py_vers(cfg_path: &Path, specified: &Version) {
    let f = fs::File::open(&cfg_path)
        .expect("Unable to read pyproject.toml while adding Python version");
    let mut new_data = String::new();
    for line in BufReader::new(f).lines() {
        if let Ok(l) = line {
            if l.starts_with("py_version") {
                new_data.push_str(&format!(
                    "py_version = \"{}.{}\"\n",
                    specified.major, specified.minor
                ));
            } else {
                new_data.push_str(&l);
                new_data.push_str("\n");
            }
        }
    }

    fs::write(cfg_path, new_data)
        .expect("Unable to write pyproject.toml while adding Python version");
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::dep_types::{Constraint, ReqType::Caret};

    // We're not concerned with testing formatting in this func.
    fn base_constrs() -> Vec<Constraint> {
        vec![Constraint::new(Caret, Version::new(0, 0, 1))]
    }

    const BASELINE: &str = r#"
[tool.pyflow]
name = ""


[tool.pyflow.dependencies]
a = "^0.3.5"


[tool.pyflow.dev-dependencies]
dev_a = "^1.17.2"

"#;

    const _BASELINE_NO_DEPS: &str = r#"
[tool.pyflow]
name = ""


[tool.pyflow.dev-dependencies]
dev_a = "^1.17.2"

"#;

    const BASELINE_NO_DEV_DEPS: &str = r#"
[tool.pyflow]
name = ""


[tool.pyflow.dependencies]
a = "^0.3.5"

"#;

    const BASELINE_EMPTY_DEPS: &str = r#"
[tool.pyflow]
name = ""


[tool.pyflow.dependencies]


[tool.pyflow.dev-dependencies]
dev_a = "^1.17.2"

"#;

    #[test]
    fn add_deps_baseline() {
        let actual = update_cfg(
            BASELINE.into(),
            &[
                Req::new("b".into(), base_constrs()),
                Req::new("c".into(), base_constrs()),
            ],
            &[Req::new("dev_b".into(), base_constrs())],
        );

        let expected = r#"
[tool.pyflow]
name = ""


[tool.pyflow.dependencies]
a = "^0.3.5"
b = "^0.0.1"
c = "^0.0.1"


[tool.pyflow.dev-dependencies]
dev_a = "^1.17.2"
dev_b = "^0.0.1"

"#;

        assert_eq!(expected, &actual);
    }

    #[test]
    fn add_deps_no_dev_deps_sect() {
        let actual = update_cfg(
            BASELINE_NO_DEV_DEPS.into(),
            &[
                Req::new("b".into(), base_constrs()),
                Req::new("c".into(), base_constrs()),
            ],
            &[Req::new("dev_b".into(), base_constrs())],
        );

        let expected = r#"
[tool.pyflow]
name = ""


[tool.pyflow.dependencies]
a = "^0.3.5"
b = "^0.0.1"
c = "^0.0.1"


[tool.pyflow.dev-dependencies]
dev_b = "^0.0.1"

"#;

        assert_eq!(expected, &actual);
    }

    #[test]
    fn add_deps_baseline_empty_deps() {
        let actual = update_cfg(
            BASELINE_EMPTY_DEPS.into(),
            &[
                Req::new("b".into(), base_constrs()),
                Req::new("c".into(), base_constrs()),
            ],
            &[Req::new("dev_b".into(), base_constrs())],
        );

        let expected = r#"
[tool.pyflow]
name = ""


[tool.pyflow.dependencies]
b = "^0.0.1"
c = "^0.0.1"


[tool.pyflow.dev-dependencies]
dev_a = "^1.17.2"
dev_b = "^0.0.1"

"#;

        assert_eq!(expected, &actual);
    }
}
