use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader},
    path::Path,
};

use regex::Regex;
use serde::Deserialize;
use termcolor::Color;

use crate::{
    Config,
    dep_types::{Req, Version},
    util,
};

#[derive(Debug, Deserialize)]
pub struct Pipfile {
    // Pipfile doesn't use a prefix; assume `[packages]` and [`dev-packages`] sections
    // are from it, and use the same format as this tool and `Poetry`.
    pub packages: Option<HashMap<String, DepComponentWrapper>>,
    #[serde(rename = "dev-packages")]
    pub dev_packages: Option<HashMap<String, DepComponentWrapper>>,
}

/// Represents the PEP 621 `[project]` table used by uv and other modern tools.
#[derive(Debug, Deserialize)]
pub struct Project {
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    /// PEP 440 Python version constraint, eg `">=3.11"`.
    #[serde(rename = "requires-python")]
    pub requires_python: Option<String>,
    /// PEP 508 dependency strings, eg `["requests>=2.0", "flask"]`.
    pub dependencies: Option<Vec<String>>,
    #[serde(rename = "optional-dependencies")]
    pub optional_dependencies: Option<HashMap<String, Vec<String>>>,
}

/// This nested structure is required based on how the `toml` crate handles dots.
#[derive(Debug, Deserialize)]
pub struct Pyproject {
    /// PEP 621 `[project]` table (used by uv and other standards-compliant tools).
    pub project: Option<Project>,
    /// `[tool]` table (used by pyflow, poetry, etc.).
    #[serde(default)]
    pub tool: Tool,
}

#[derive(Debug, Deserialize, Default)]
pub struct Tool {
    pub pyflow: Option<Pyflow>,
    pub poetry: Option<Poetry>,
    pub uv: Option<Poetry>, // todo: A/R
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

/// Which pyproject.toml format is in use.
pub enum CfgFormat {
    /// Pyflow's own `[tool.pyflow.dependencies]` map format.
    Pyflow,
    /// PEP 621 `[project]` table with a `dependencies` array of PEP 508 strings (uv, flit, etc.).
    Pep621,
}

/// Detect whether a pyproject.toml uses pyflow's custom format or PEP 621 (uv-style).
/// Pyflow format takes precedence when both sections are present.
pub fn detect_cfg_format(cfg_data: &str) -> CfgFormat {
    for line in cfg_data.lines() {
        let trimmed = line.trim();
        if trimmed == "[tool.pyflow]" || trimmed == "[tool.pyflow.dependencies]" {
            return CfgFormat::Pyflow;
        }
    }
    for line in cfg_data.lines() {
        if line.trim() == "[project]" {
            return CfgFormat::Pep621;
        }
    }
    CfgFormat::Pyflow
}

/// Insert PEP 508 requirement entries into a TOML array identified by `section_header` and `key`.
/// Handles both multi-line and inline arrays, and creates the section/key if absent.
fn insert_into_pep621_array(
    mut lines: Vec<String>,
    section_header: &str,
    key: &str,
    reqs: &[Req],
) -> Vec<String> {
    let section_re = Regex::new(r"^\s*\[.*\]\s*$").unwrap();
    let key_no_space = format!("{}=[", key);
    let key_with_space = format!("{} = [", key);

    let mut in_section = false;
    let mut section_idx: Option<usize> = None;
    let mut array_open_idx: Option<usize> = None;
    let mut array_close_idx: Option<usize> = None;
    let mut bracket_depth: i32 = 0;

    'outer: for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if section_re.is_match(trimmed) {
            if trimmed == section_header {
                in_section = true;
                section_idx = Some(i);
            } else if in_section {
                in_section = false;
            }
            continue;
        }

        if in_section && array_open_idx.is_none() {
            let t = trimmed.replace(' ', "");
            if t.starts_with(&key_no_space) || t.starts_with(&key_with_space.replace(' ', "")) {
                array_open_idx = Some(i);
                for ch in line.chars() {
                    match ch {
                        '[' => bracket_depth += 1,
                        ']' => {
                            bracket_depth -= 1;
                            if bracket_depth == 0 {
                                array_close_idx = Some(i);
                                break 'outer;
                            }
                        }
                        _ => {}
                    }
                }
            }
        } else if array_open_idx.is_some() && bracket_depth > 0 {
            for ch in line.chars() {
                match ch {
                    '[' => bracket_depth += 1,
                    ']' => {
                        bracket_depth -= 1;
                        if bracket_depth == 0 {
                            array_close_idx = Some(i);
                            break 'outer;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let new_entries: Vec<String> = reqs
        .iter()
        .map(|r| format!("    \"{}\",", r.to_pep508_string()))
        .collect();

    match (array_open_idx, array_close_idx) {
        (Some(open), Some(close)) if open == close => {
            // Inline array on one line: convert to multi-line
            let line = lines[open].clone();
            if let Some(bracket_pos) = line.find('[') {
                let prefix = &line[..=bracket_pos];
                let rest = &line[bracket_pos + 1..];
                let end_bracket = rest.rfind(']').unwrap_or(rest.len());
                let existing_content = &rest[..end_bracket];
                let mut new_line_vec: Vec<String> = vec![prefix.to_string()];
                for item in existing_content.split(',') {
                    let item = item.trim();
                    if !item.is_empty() {
                        new_line_vec.push(format!("    {},", item.trim_end_matches(',')));
                    }
                }
                new_line_vec.extend(new_entries);
                new_line_vec.push("]".into());
                lines.splice(open..=open, new_line_vec);
            }
        }
        (Some(_open), Some(close)) => {
            // Multi-line array: insert before the closing `]`
            lines.splice(close..close, new_entries);
        }
        _ => {
            // Key not found: add it to the section or create the section
            match section_idx {
                Some(si) => {
                    let mut new_lines: Vec<String> = vec![format!("{} = [", key)];
                    new_lines.extend(new_entries);
                    new_lines.push("]".into());
                    new_lines.push("".into());
                    lines.splice(si + 1..si + 1, new_lines);
                }
                None => {
                    if lines.last().map(|l| !l.is_empty()).unwrap_or(false) {
                        lines.push("".into());
                    }
                    lines.push(section_header.to_string());
                    lines.push(format!("{} = [", key));
                    lines.extend(new_entries);
                    lines.push("]".into());
                    lines.push("".into());
                }
            }
        }
    }
    lines
}

/// Update a PEP 621 pyproject.toml with new regular and dev dependencies.
fn update_pep621_cfg(cfg_data: &str, added: &[Req], added_dev: &[Req]) -> String {
    let mut lines: Vec<String> = cfg_data.lines().map(str::to_string).collect();
    if !added.is_empty() {
        lines = insert_into_pep621_array(lines, "[project]", "dependencies", added);
    }
    if !added_dev.is_empty() {
        lines = insert_into_pep621_array(lines, "[tool.uv]", "dev-dependencies", added_dev);
    }
    lines.join("\n")
}

/// Remove deps from a PEP 621 `[project].dependencies` array by package name.
fn remove_reqs_pep621(data: &str, reqs: &[String]) -> String {
    let mut result = String::new();
    let mut in_project = false;
    let mut in_deps_array = false;
    let mut bracket_depth: i32 = 0;
    let section_re = Regex::new(r"^\s*\[.*\]\s*$").unwrap();

    for line in data.lines() {
        let trimmed = line.trim();

        if section_re.is_match(trimmed) {
            in_project = trimmed == "[project]";
            if !in_project {
                in_deps_array = false;
                bracket_depth = 0;
            }
            result.push_str(line);
            result.push('\n');
            continue;
        }

        if in_project && !in_deps_array {
            let t = trimmed.replace(' ', "");
            if t.starts_with("dependencies=[") || t == "dependencies=[" {
                in_deps_array = true;
                for ch in line.chars() {
                    match ch {
                        '[' => bracket_depth += 1,
                        ']' => bracket_depth -= 1,
                        _ => {}
                    }
                }
                if bracket_depth <= 0 {
                    in_deps_array = false;
                }
                result.push_str(line);
                result.push('\n');
                continue;
            }
        }

        if in_deps_array {
            let mut depth_delta: i32 = 0;
            for ch in line.chars() {
                match ch {
                    '[' => depth_delta += 1,
                    ']' => depth_delta -= 1,
                    _ => {}
                }
            }
            bracket_depth += depth_delta;
            if bracket_depth <= 0 {
                in_deps_array = false;
                result.push_str(line);
                result.push('\n');
                continue;
            }
            // Try to extract the dep name and skip if it matches a req to remove
            if let Some(req) = Req::from_pep508_str(trimmed) {
                if reqs.iter().any(|r| util::compare_names(r, &req.name)) {
                    continue;
                }
            }
        }

        result.push_str(line);
        result.push('\n');
    }
    result
}

/// Remove deps from a pyflow-format `[tool.pyflow.dependencies]` section by package name.
fn remove_reqs_pyflow(data: &str, reqs: &[String]) -> String {
    let mut result = String::new();
    let mut in_dep = false;
    let mut _in_dev_dep = false;
    let sect_re = Regex::new(r"^\[.*\]$").unwrap();

    for line in data.lines() {
        if line.starts_with('#') || line.is_empty() {
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
            let req_line = if let Ok(r) = Req::from_str(line, false) {
                r
            } else {
                result.push_str(line);
                result.push('\n');
                continue;
            };

            if reqs
                .iter()
                .map(|r| r.to_lowercase())
                .any(|x| x == req_line.name.to_lowercase())
            {
                continue;
            }
        }
        result.push_str(line);
        result.push('\n');
    }
    result
}

/// Write dependencies to pyproject.toml. If an entry for that package already exists, ask if
/// we should update the version. Assume we've already parsed the config, and are only
/// adding new reqs, or ones with a changed version.
pub fn add_reqs_to_cfg(cfg_path: &Path, added: &[Req], added_dev: &[Req]) {
    let data = fs::read_to_string(cfg_path)
        .expect("Unable to read pyproject.toml while attempting to add a dependency");

    let updated = match detect_cfg_format(&data) {
        CfgFormat::Pep621 => update_pep621_cfg(&data, added, added_dev),
        CfgFormat::Pyflow => update_cfg(&data, added, added_dev),
    };
    fs::write(cfg_path, updated)
        .expect("Unable to write pyproject.toml while attempting to add a dependency");
}

/// Remove dependencies from pyproject.toml.
pub fn remove_reqs_from_cfg(cfg_path: &Path, reqs: &[String]) {
    let data = fs::read_to_string(cfg_path)
        .expect("Unable to read pyproject.toml while attempting to remove a dependency");

    let updated = match detect_cfg_format(&data) {
        CfgFormat::Pep621 => remove_reqs_pep621(&data, reqs),
        CfgFormat::Pyflow => remove_reqs_pyflow(&data, reqs),
    };

    fs::write(cfg_path, updated)
        .expect("Unable to write to pyproject.toml while attempting to remove a dependency");
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

/// Update the config file with a new Python version.
/// Handles both pyflow format (`py_version = "3.x"`) and PEP 621 (`requires-python = ">=3.x"`).
pub fn change_py_vers(cfg_path: &Path, specified: &Version) {
    let f = fs::File::open(cfg_path)
        .expect("Unable to read pyproject.toml while adding Python version");
    let ver_str = specified.to_string_no_patch();
    let mut new_data = String::new();
    let mut updated = false;

    for line in BufReader::new(f).lines().flatten() {
        if line.starts_with("py_version") {
            new_data.push_str(&format!("py_version = \"{}\"\n", ver_str));
            updated = true;
        } else if line.starts_with("requires-python") {
            new_data.push_str(&format!("requires-python = \">={}\"\n", ver_str));
            updated = true;
        } else {
            new_data.push_str(&line);
            new_data.push('\n');
        }
    }

    // If neither field existed (fresh file), append requires-python.
    if !updated {
        new_data.push_str(&format!("requires-python = \">={}\"\n", ver_str));
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
