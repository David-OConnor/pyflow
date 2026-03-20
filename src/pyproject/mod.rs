pub mod current;

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use regex::Regex;
use serde::Deserialize;

use crate::{
    dep_types::{Constraint, Req, ReqType, Version},
    files,
    util::{self, abort},
};

pub const CFG_FILENAME: &str = "pyproject.toml";
pub const LOCK_FILENAME: &str = "pyflow.lock";

#[derive(Clone, Debug, Default)]
pub struct PresentConfig {
    pub project_path: PathBuf,
    pub config_path: PathBuf,
    pub pypackages_path: PathBuf,
    pub lock_path: PathBuf,
    pub config: Config,
}

/// A config, parsed from pyproject.toml
#[derive(Clone, Debug, Default, Deserialize)]
// todo: Auto-desr some of these
pub struct Config {
    pub name: Option<String>,
    pub py_version: Option<Version>,
    pub reqs: Vec<Req>,
    pub dev_reqs: Vec<Req>,
    pub version: Option<Version>,
    pub authors: Vec<String>,
    pub license: Option<String>,
    pub extras: HashMap<String, String>,
    pub description: Option<String>,
    pub classifiers: Vec<String>, // https://pypi.org/classifiers/
    pub keywords: Vec<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub repo_url: Option<String>,
    pub package_url: Option<String>,
    pub readme: Option<String>,
    pub build: Option<String>, // A python file used to build non-python extensions
    //    entry_points: HashMap<String, Vec<String>>, // todo option?
    pub scripts: HashMap<String, String>, //todo: put under [tool.pyflow.scripts] ?
    //    console_scripts: Vec<String>, // We don't parse these; pass them to `setup.py` as-entered.
    pub python_requires: Option<String>,
}

impl Config {
    /// Helper fn to prevent repetition
    pub fn parse_deps(deps: HashMap<String, files::DepComponentWrapper>) -> Vec<Req> {
        let mut result = Vec::new();
        for (name, data) in deps {
            let constraints;
            let mut extras = None;
            let mut git = None;
            let mut path = None;
            let mut python_version = None;
            match data {
                files::DepComponentWrapper::A(constrs) => {
                    constraints = if let Ok(c) = Constraint::from_str_multiple(&constrs) {
                        c
                    } else {
                        abort(&format!(
                            "Problem parsing constraints in `pyproject.toml`: {}",
                            &constrs
                        ))
                    };
                }
                files::DepComponentWrapper::B(subdata) => {
                    constraints = match subdata.constrs {
                        Some(constrs) => {
                            if let Ok(c) = Constraint::from_str_multiple(&constrs) {
                                c
                            } else {
                                abort(&format!(
                                    "Problem parsing constraints in `pyproject.toml`: {}",
                                    &constrs
                                ))
                            }
                        }
                        None => vec![],
                    };

                    if let Some(ex) = subdata.extras {
                        extras = Some(ex);
                    }
                    if let Some(p) = subdata.path {
                        path = Some(p);
                    }
                    if let Some(repo) = subdata.git {
                        git = Some(repo);
                    }
                    if let Some(v) = subdata.python {
                        let pv = Constraint::from_str(&v)
                            .expect("Problem parsing python version in dependency");
                        python_version = Some(vec![pv]);
                    }
                }
            }

            result.push(Req {
                name,
                constraints,
                extra: None,
                sys_platform: None,
                python_version,
                install_with_extras: extras,
                path,
                git,
            });
        }
        result
    }

    // todo: DRY at the top from `from_file`.
    pub fn from_pipfile(path: &Path) -> Option<Self> {
        // todo: Lots of tweaks and QC could be done re what fields to parse, and how best to
        // todo parse and store them.
        let toml_str = match fs::read_to_string(path).ok() {
            Some(d) => d,
            None => return None,
        };

        let decoded: files::Pipfile = if let Ok(d) = toml::from_str(&toml_str) {
            d
        } else {
            abort("Problem parsing `Pipfile`")
        };
        let mut result = Self::default();

        if let Some(pipfile_deps) = decoded.packages {
            result.reqs = Self::parse_deps(pipfile_deps);
        }
        if let Some(pipfile_dev_deps) = decoded.dev_packages {
            result.dev_reqs = Self::parse_deps(pipfile_dev_deps);
        }

        Some(result)
    }

    /// Pull config data from `pyproject.toml`. We use this to deserialize things like Versions
    /// and requirements.
    pub fn from_file(path: &Path) -> Option<Self> {
        // todo: Lots of tweaks and QC could be done re what fields to parse, and how best to
        // todo parse and store them.
        let toml_str = match fs::read_to_string(path) {
            Ok(d) => d,
            Err(_) => return None,
        };

        let decoded: files::Pyproject = if let Ok(d) = toml::from_str(&toml_str) {
            d
        } else {
            abort("Problem parsing `pyproject.toml`");
        };
        let mut result = Self::default();

        // Parse Poetry first, since we'll use pyflow if there's a conflict.
        if let Some(po) = decoded.tool.poetry {
            if let Some(v) = po.name {
                result.name = Some(v);
            }
            if let Some(v) = po.authors {
                result.authors = v;
            }
            if let Some(v) = po.license {
                result.license = Some(v);
            }

            if let Some(v) = po.homepage {
                result.homepage = Some(v);
            }
            if let Some(v) = po.description {
                result.description = Some(v);
            }
            if let Some(v) = po.repository {
                result.repository = Some(v);
            }
            if let Some(v) = po.readme {
                result.readme = Some(v);
            }
            if let Some(v) = po.build {
                result.build = Some(v);
            }
            // todo: Process entry pts, classifiers etc?
            if let Some(v) = po.classifiers {
                result.classifiers = v;
            }
            if let Some(v) = po.keywords {
                result.keywords = v;
            }

            //                        if let Some(v) = po.source {
            //                result.source = v;
            //            }
            //            if let Some(v) = po.scripts {
            //                result.console_scripts = v;
            //            }
            if let Some(v) = po.extras {
                result.extras = v;
            }

            if let Some(v) = po.version {
                result.version = Some(
                    Version::from_str(&v).expect("Problem parsing version in `pyproject.toml`"),
                )
            }

            // todo: DRY (c+p) from pyflow dependency parsing, other than parsing python version here,
            // todo which only poetry does.
            // todo: Parse poetry dev deps
            if let Some(deps) = po.dependencies {
                for (name, data) in deps {
                    let constraints;
                    let mut extras = None;
                    let mut python_version = None;
                    match data {
                        files::DepComponentWrapperPoetry::A(constrs) => {
                            constraints = Constraint::from_str_multiple(&constrs)
                                .expect("Problem parsing constraints in `pyproject.toml`.");
                        }
                        files::DepComponentWrapperPoetry::B(subdata) => {
                            constraints = Constraint::from_str_multiple(&subdata.constrs)
                                .expect("Problem parsing constraints in `pyproject.toml`.");
                            if let Some(ex) = subdata.extras {
                                extras = Some(ex);
                            }
                            if let Some(v) = subdata.python {
                                let pv = Constraint::from_str(&v)
                                    .expect("Problem parsing python version in dependency");
                                python_version = Some(vec![pv]);
                            }
                            // todo repository etc
                        }
                    }
                    if &name.to_lowercase() == "python" {
                        if let Some(constr) = constraints.get(0) {
                            result.py_version = Some(constr.version.clone())
                        }
                    } else {
                        result.reqs.push(Req {
                            name,
                            constraints,
                            extra: None,
                            sys_platform: None,
                            python_version,
                            install_with_extras: extras,
                            path: None,
                            git: None,
                        });
                    }
                }
            }
        }

        // Parse PEP 621 `[project]` table (uv, flit, etc.). Pyflow takes priority below.
        if let Some(proj) = decoded.project {
            if result.name.is_none() {
                result.name = proj.name;
            }
            if result.version.is_none() {
                if let Some(v) = proj.version {
                    result.version = Version::from_str(&v).ok();
                }
            }
            if result.description.is_none() {
                result.description = proj.description;
            }
            // Parse `requires-python = ">=3.11"` → py_version.
            // Take the version from the first >= / == / > constraint found.
            if result.py_version.is_none() {
                if let Some(rp) = proj.requires_python {
                    if let Ok(constraints) = Constraint::from_str_multiple(&rp) {
                        for c in &constraints {
                            match c.type_ {
                                ReqType::Gte | ReqType::Gt | ReqType::Exact => {
                                    result.py_version = Some(c.version.clone());
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            // Only overwrite deps if the poetry block didn't set them.
            if result.reqs.is_empty() {
                if let Some(deps) = proj.dependencies {
                    result.reqs = deps
                        .iter()
                        .filter_map(|s| Req::from_pep508_str(s.trim()))
                        .collect();
                }
            }
        }

        if let Some(pf) = decoded.tool.pyflow {
            if let Some(v) = pf.name {
                result.name = Some(v);
            }

            if let Some(v) = pf.authors {
                result.authors = if v.is_empty() {
                    util::get_git_author()
                } else {
                    v
                };
            }
            if let Some(v) = pf.license {
                result.license = Some(v);
            }
            if let Some(v) = pf.homepage {
                result.homepage = Some(v);
            }
            if let Some(v) = pf.description {
                result.description = Some(v);
            }
            if let Some(v) = pf.repository {
                result.repository = Some(v);
            }

            // todo: Process entry pts, classifiers etc?
            if let Some(v) = pf.classifiers {
                result.classifiers = v;
            }
            if let Some(v) = pf.keywords {
                result.keywords = v;
            }
            if let Some(v) = pf.readme {
                result.readme = Some(v);
            }
            if let Some(v) = pf.build {
                result.build = Some(v);
            }
            //            if let Some(v) = pf.entry_points {
            //                result.entry_points = v;
            //            } // todo
            if let Some(v) = pf.scripts {
                result.scripts = v;
            }

            if let Some(v) = pf.python_requires {
                result.python_requires = Some(v);
            }

            if let Some(v) = pf.package_url {
                result.package_url = Some(v);
            }

            if let Some(v) = pf.version {
                result.version = Some(
                    Version::from_str(&v).expect("Problem parsing version in `pyproject.toml`"),
                )
            }

            if let Some(v) = pf.py_version {
                result.py_version = Some(
                    Version::from_str(&v)
                        .expect("Problem parsing python version in `pyproject.toml`"),
                );
            }

            if let Some(deps) = pf.dependencies {
                result.reqs = Self::parse_deps(deps);
            }
            if let Some(deps) = pf.dev_dependencies {
                result.dev_reqs = Self::parse_deps(deps);
            }
        }

        Some(result)
    }

    /// For reqs of `path` type, add their sub-reqs by parsing `setup.py` or `pyproject.toml`.
    pub fn populate_path_subreqs(&mut self) {
        self.reqs.append(&mut pop_reqs_helper(&self.reqs, false));
        self.dev_reqs
            .append(&mut pop_reqs_helper(&self.dev_reqs, true));
    }

    /// Create a new `pyproject.toml` file in PEP 621 format.
    pub fn write_file(&self, path: &Path) {
        if path.exists() {
            abort("`pyproject.toml` already exists")
        }

        let mut result = String::new();

        // ── [project] ────────────────────────────────────────────────────────
        result.push_str("[project]\n");

        match &self.name {
            Some(name) => result.push_str(&format!("name = \"{}\"\n", name)),
            None => result.push_str("name = \"\"\n"),
        }

        match &self.version {
            Some(vers) => result.push_str(&format!("version = \"{}\"\n", vers)),
            None => result.push_str("version = \"0.1.0\"\n"),
        }

        match &self.py_version {
            Some(py_v) => result.push_str(&format!(
                "requires-python = \">={}\"\n",
                py_v.to_string_no_patch()
            )),
            None => result.push_str("requires-python = \">=3.8\"\n"),
        }

        if !self.authors.is_empty() {
            result.push_str("authors = [\n");
            for author in &self.authors {
                // Parse "Name <email>" into separate fields; fall back to name-only.
                if let (Some(lt), Some(gt)) = (author.find('<'), author.rfind('>')) {
                    let name = author[..lt].trim();
                    let email = &author[lt + 1..gt];
                    result.push_str(&format!(
                        "    {{name = \"{}\", email = \"{}\"}},\n",
                        name, email
                    ));
                } else {
                    result.push_str(&format!("    {{name = \"{}\"}},\n", author.trim()));
                }
            }
            result.push_str("]\n");
        }

        if let Some(v) = &self.description {
            result.push_str(&format!("description = \"{}\"\n", v));
        }

        // dependencies array (PEP 508 strings)
        result.push_str("dependencies = [\n");
        for dep in &self.reqs {
            result.push_str(&format!("    \"{}\",\n", dep.to_pep508_string()));
        }
        result.push_str("]\n");

        // ── [project.urls] ───────────────────────────────────────────────────
        if self.homepage.is_some() || self.repository.is_some() {
            result.push_str("\n[project.urls]\n");
            if let Some(v) = &self.homepage {
                result.push_str(&format!("Homepage = \"{}\"\n", v));
            }
            if let Some(v) = &self.repository {
                result.push_str(&format!("Repository = \"{}\"\n", v));
            }
        }

        // ── [project.scripts] ────────────────────────────────────────────────
        if !self.scripts.is_empty() {
            result.push_str("\n[project.scripts]\n");
            for (name, mod_fn) in &self.scripts {
                result.push_str(&format!("{} = \"{}\"\n", name, mod_fn));
            }
        }

        // ── [tool.uv] dev-dependencies ───────────────────────────────────────
        if !self.dev_reqs.is_empty() {
            result.push_str("\n[tool.uv]\ndev-dependencies = [\n");
            for dep in &self.dev_reqs {
                result.push_str(&format!("    \"{}\",\n", dep.to_pep508_string()));
            }
            result.push_str("]\n");
        }

        result.push('\n'); // trailing newline

        if fs::write(path, result).is_err() {
            abort("Problem writing `pyproject.toml`")
        }
    }
}

/// Reduce repetition between reqs and dev reqs when populating reqs of path reqs.
fn pop_reqs_helper(reqs: &[Req], dev: bool) -> Vec<Req> {
    let mut result = vec![];
    for req in reqs.iter().filter(|r| r.path.is_some()) {
        let req_path = PathBuf::from(req.path.clone().unwrap());
        let pyproj = req_path.join("pyproject.toml");
        let req_txt = req_path.join("requirements.txt");
        //        let pipfile = req_path.join("Pipfile");

        let mut dummy_cfg = Config::default();

        if req_txt.exists() {
            files::parse_req_dot_text(&mut dummy_cfg, &req_txt);
        }

        //        if pipfile.exists() {
        //            files::parse_pipfile(&mut dummy_cfg, &pipfile);
        //        }

        if dev {
            result.append(&mut dummy_cfg.dev_reqs);
        } else {
            result.append(&mut dummy_cfg.reqs);
        }

        // We don't parse `setup.py`, since it involves running arbitrary Python code.

        if pyproj.exists() {
            let mut req_cfg = Config::from_file(&PathBuf::from(&pyproj))
                .unwrap_or_else(|| panic!("Problem parsing`pyproject.toml`: {:?}", &pyproj));
            result.append(&mut req_cfg.reqs)
        }

        // Check for metadata of a built wheel
        for folder_name in util::find_folders(&req_path) {
            // todo: Dry from `util` and `install`.
            let re_dist = Regex::new(r"^(.*?)-(.*?)\.dist-info$").unwrap();
            if re_dist.captures(&folder_name).is_some() {
                let metadata_path = req_path.join(folder_name).join("METADATA");
                let mut metadata = util::parse_metadata(&metadata_path);

                result.append(&mut metadata.requires_dist);
            }
        }
    }
    result
}
