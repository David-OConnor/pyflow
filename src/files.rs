use crate::{
    dep_types::{Req, Version},
    util, Config,
};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use termcolor::Color;

#[derive(Debug, Deserialize)]
pub struct Pipfile {
    // Pipfile doesn't use a prefix; assume `[packages]` and [`dev-packages`] sections
    // are from it, and use the same format as this tool and `Poetry`.
    pub packages: Option<HashMap<String, DepComponentWrapper>>,
    #[serde(rename = "dev-packages")]
    pub dev_packages: Option<HashMap<String, DepComponentWrapper>>,
}

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

#[derive(Debug, Deserialize)]
#[serde(untagged)]
/// Allows use of both Strings, ie "ipython = "^7.7.0", and maps: "ipython = {version = "^7.7.0", extras=["qtconsole"]}"
pub enum DepComponentWrapper {
    A(String),
    B(DepComponent),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum DepComponentWrapperPoetry {
    A(String),
    B(DepComponentPoetry),
}

#[derive(Debug, Deserialize)]
pub struct DepComponent {
    #[serde(rename = "version")]
    pub constrs: Option<String>,
    pub extras: Option<Vec<String>>,
    pub path: Option<String>,
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
    pub license: Option<String>,
    pub description: Option<String>,
    pub classifiers: Option<Vec<String>>, // https://pypi.org/classifiers/
    pub keywords: Option<Vec<String>>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub repo_url: Option<String>,
    pub package_url: Option<String>,
    pub readme: Option<String>,
    pub build: Option<String>,
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
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub documentation: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub readme: Option<String>,
    pub build: Option<String>,
    pub classifiers: Option<Vec<String>>,
    pub packages: Option<Vec<HashMap<String, String>>>,
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

/// Encapsulate one section of the `pyproject.toml`.
///
/// # Attributes:
/// * lines: A vector containing each line of the section
/// * i_start: Zero-indexed indicating the line of the header.
/// * i_end: Zero-indexed indicating the line number of the next section header,
///     or the last line of the file.
struct Section {
    lines: Vec<String>,
    i_start: usize,
    i_end: usize,
}

/// Identify the start index, end index, and lines of a particular section.
fn collect_section(cfg_lines: &[String], title: &str) -> Option<Section> {
    // This will tell us when we've reached a new section
    let section_re = Regex::new(r"^\[.*\]$").unwrap();

    let mut existing_entries = Vec::new();
    let mut in_section = false;
    let mut i_start = 0usize;

    for (i, line) in cfg_lines.iter().enumerate() {
        if in_section && section_re.is_match(line) {
            return Some(Section {
                lines: existing_entries,
                i_start,
                i_end: i,
            });
        }

        if in_section {
            existing_entries.push(line.parse().unwrap())
        }

        // This must be the last step of the loop to work properly
        if line.replace(" ", "") == title {
            existing_entries.push(title.into());
            i_start = i;
            in_section = true;
        }
    }
    // We've reached the end of the file without detecting a new section
    if in_section {
        Some(Section {
            lines: existing_entries,
            i_start,
            i_end: cfg_lines.len(),
        })
    } else {
        None
    }
}

/// Main logic for adding dependencies to a particular section.
///
/// If the section is detected, then the dependencies are appended to that section. Otherwise,
/// a new section is appended to the end of the file.
fn extend_or_insert(mut cfg_lines: Vec<String>, section_header: &str, reqs: &[Req]) -> Vec<String> {
    let collected = collect_section(&cfg_lines, section_header);

    match collected {
        // The section already exists, so we can just add the new reqs
        Some(section) => {
            // To enforce proper spacing we first remove any empty lines,
            // and later we append a trailing empty line
            let mut all_deps: Vec<String> = section
                .lines
                .to_owned()
                .into_iter()
                .filter(|x| !x.is_empty())
                .collect();

            for req in reqs {
                all_deps.push(req.to_cfg_string())
            }
            all_deps.push("".into());

            // Replace the original lines with our new updated lines
            cfg_lines.splice(section.i_start..section.i_end, all_deps);
            cfg_lines
        }
        // The section did not already exist, so we must create it
        None => {
            // A section is composed of its header, followed by all the requirements
            // and then an empty line
            let mut section = vec![section_header.to_string()];
            section.extend(reqs.iter().map(|r| r.to_cfg_string()));
            section.push("".into());

            // We want an empty line before adding the new section
            if let Some(last) = cfg_lines.last() {
                if !last.is_empty() {
                    cfg_lines.push("".into())
                }
            }
            cfg_lines.extend(section);
            cfg_lines
        }
    }
}

/// Add dependencies and dev-dependencies to `cfg-data`, creating the sections if necessary.
///
/// The added sections are appended to the end of the file. Split from `add_reqs_to_cfg`
/// to accommodate testing.
fn update_cfg(cfg_data: &str, added: &[Req], added_dev: &[Req]) -> String {
    let cfg_lines: Vec<String> = cfg_data.lines().map(str::to_string).collect();

    // First we update the dependencies section
    let cfg_lines_with_reqs = if !added.is_empty() {
        extend_or_insert(cfg_lines, "[tool.pyflow.dependencies]", added)
    } else {
        cfg_lines
    };

    // Then we move onto the dev-dependencies
    let cfg_lines_with_all_reqs = if !added_dev.is_empty() {
        extend_or_insert(
            cfg_lines_with_reqs,
            "[tool.pyflow.dev-dependencies]",
            added_dev,
        )
    } else {
        cfg_lines_with_reqs
    };

    cfg_lines_with_all_reqs.join("\n")
}

/// Write dependencies to pyproject.toml. If an entry for that package already exists, ask if
/// we should update the version. Assume we've already parsed the config, and are only
/// adding new reqs, or ones with a changed version.
pub fn add_reqs_to_cfg(cfg_path: &Path, added: &[Req], added_dev: &[Req]) {
    let data = fs::read_to_string(cfg_path)
        .expect("Unable to read pyproject.toml while attempting to add a dependency");

    let updated = update_cfg(&data, added, added_dev);
    fs::write(cfg_path, updated)
        .expect("Unable to write pyproject.toml while attempting to add a dependency");
}

/// Remove dependencies from pyproject.toml.
pub fn remove_reqs_from_cfg(cfg_path: &Path, reqs: &[String]) {
    // todo: Handle removing dev deps.
    // todo: DRY from parsing the config.
    let mut result = String::new();
    let data = fs::read_to_string(cfg_path)
        .expect("Unable to read pyproject.toml while attempting to add a dependency");

    let mut in_dep = false;
    let mut _in_dev_dep = false;
    let sect_re = Regex::new(r"^\[.*\]$").unwrap();

    for line in data.lines() {
        if line.starts_with('#') || line.is_empty() {
            // todo handle mid-line comements
            result.push_str(line);
            result.push('\n');
            continue;
        }

        if line == "[tool.pyflow.dependencies]" {
            in_dep = true;
            _in_dev_dep = false;
            result.push_str(line);
            result.push('\n');
            continue;
        }

        if line == "[tool.pyflow.dev-dependencies]" {
            in_dep = true;
            _in_dev_dep = false;
            result.push_str(line);
            result.push('\n');
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
                result.push_str(line);
                result.push('\n');
                continue; // Could be caused by a git etc req.
                          //                util::abort(&format!(
                          //                    "Can't parse this line in `pyproject.toml`: {}",
                          //                    line
                          //                ));
                          //                unreachable!()
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
        result.push('\n');
    }

    fs::write(cfg_path, result)
        .expect("Unable to write to pyproject.toml while attempting to add a dependency");
}

pub fn parse_req_dot_text(cfg: &mut Config, path: &Path) {
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return,
    };

    for line in BufReader::new(file).lines().flatten() {
        match Req::from_pip_str(&line) {
            Some(r) => {
                cfg.reqs.push(r.clone());
            }
            None => util::print_color(
                &format!("Problem parsing {} from requirements.txt", line),
                Color::Red,
            ),
        };
    }
}

/// Update the config file with a new version.
pub fn change_py_vers(cfg_path: &Path, specified: &Version) {
    let f = fs::File::open(&cfg_path)
        .expect("Unable to read pyproject.toml while adding Python version");
    let mut new_data = String::new();
    for line in BufReader::new(f).lines().flatten() {
        if line.starts_with("py_version") {
            new_data.push_str(&format!("py_version = \"{}\"\n", specified.to_string()));
        } else {
            new_data.push_str(&line);
            new_data.push('\n');
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

    const BASELINE_NO_DEPS_NO_DEV_DEPS: &str = r#"
[tool.pyflow]
name = ""
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
            BASELINE,
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
            BASELINE_NO_DEV_DEPS,
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
            BASELINE_EMPTY_DEPS,
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

    #[test]
    fn add_deps_dev_deps_baseline_no_deps_dev_deps() {
        let actual = update_cfg(
            BASELINE_NO_DEPS_NO_DEV_DEPS,
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
dev_b = "^0.0.1"
"#;
        assert_eq!(expected, &actual);
    }
}
