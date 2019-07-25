use crate::dep_types::{Dependency, Package, Version};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    error::Error,
    fmt, fs,
    io::{self, BufRead, BufReader},
    path::PathBuf,
    str::FromStr,
};
use structopt::StructOpt;
//use textio;
use crate::util::abort;
mod build;
mod commands;
mod dep_resolution;
mod dep_types;
mod edit_files;
mod util;

#[derive(StructOpt, Debug)]
#[structopt(name = "Pypackage", about = "Python packaging and publishing")]
struct Opt {
    #[structopt(subcommand)]
    subcmds: Option<SubCommand>,
    #[structopt(name = "custom_bin")]
    //    custom_bin: Vec<String>,
    custom_bin: Vec<String>,
}

///// eg `ipython`, `black` etc.
//#[derive(StructOpt, Debug)]
//struct CustomBin {
//    test: bool,
////    #[structopt(name = "name")]
////    name: String,
////    #[structopt(name = "args")]
////    args: Vec<String>,
//}

#[derive(StructOpt, Debug)]
enum SubCommand {
    /// Create a project folder with the basics
    #[structopt(name = "new")]
    New {
        #[structopt(name = "name")]
        name: String, // holds the project name.
    },

    /// Install packages from `pyproject.toml`, or ones specified
    #[structopt(
        name = "install",
        help = "
Install packages from `pyproject.toml`, `pypackage.lock`, or speficied ones. Example:

`pypackage install`: sync your installation with `pyproject.toml`, or `pypackage.lock` if it exists.
`pypackage install numpy scipy`: install `numpy` and `scipy`.
"
    )]
    Install {
        #[structopt(name = "packages")]
        packages: Vec<String>,
        #[structopt(short = "b", long = "binary")]
        bin: bool,
    },
    /// Uninstall all packages, or ones specified
    #[structopt(name = "uninstall")]
    Uninstall {
        #[structopt(name = "packages")]
        packages: Vec<String>,
    },
    /// Run python
    #[structopt(name = "python")]
    Python {
        #[structopt(name = "args")]
        args: Vec<String>,
    },
    /// Build the package, wrapping `setuptools`
    #[structopt(name = "package")]
    Package,
    /// Publish to `pypi`
    #[structopt(name = "publish")]
    Publish,
    /// Create a `pyproject.toml` from requirements.txt, pipfile etc, setup.py etc
    #[structopt(name = "init")]
    Init,
}

/// A config, parsed from pyproject.toml
#[derive(Clone, Debug, Default, Deserialize)]
// todo: Auto-desr some of these!
pub struct Config {
    py_version: Option<Version>,
    dependencies: Vec<Dependency>,
    name: Option<String>,
    version: Option<Version>,
    author: Option<String>,
    author_email: Option<String>,
    description: Option<String>,
    // https://pypi.org/classifiers/
    classifiers: Vec<String>,
    keywords: Vec<String>,
    homepage: Option<String>,
    repo_url: Option<String>,
    readme_filename: Option<String>,
    license: Option<String>,
}

fn key_re(key: &str) -> Regex {
    Regex::new(&format!(r#"^{}\s*=\s*"(.*)"$"#, key)).unwrap()
}

impl Config {
    /// Pull config data from Cargo.toml
    fn from_file(filename: &str) -> Option<Self> {
        // We don't use the `toml` crate here because it doesn't appear flexible enough.
        let mut result = Config::default();
        let file = match fs::File::open(filename) {
            Ok(f) => f,
            Err(_) => return None,
        };

        let mut in_sect = false;
        let mut in_dep = false;

        let sect_re = Regex::new(r"\[.*\]").unwrap();

        for line in BufReader::new(file).lines() {
            if let Ok(l) = line {
                // todo replace this with something that clips off
                // todo post-# part of strings; not just ignores ones starting with #
                if l.starts_with('#') {
                    continue;
                }

                if &l == "[tool.pypackage]" {
                    in_sect = true;
                    in_dep = false;
                    continue;
                } else if &l == "[tool.pypackage.dependencies]" {
                    in_sect = false;
                    in_dep = true;
                    continue;
                } else if sect_re.is_match(&l) {
                    in_sect = false;
                    in_dep = false;
                    continue;
                }

                if in_sect {
                    // todo DRY
                    if let Some(n2) = key_re("name").captures(&l) {
                        if let Some(n) = n2.get(1) {
                            result.name = Some(n.as_str().to_string());
                        }
                    }
                    if let Some(n2) = key_re("description").captures(&l) {
                        if let Some(n) = n2.get(1) {
                            result.description = Some(n.as_str().to_string());
                        }
                    }
                    if let Some(n2) = key_re("version").captures(&l) {
                        if let Some(n) = n2.get(1) {
                            result.version = Some(Version::from_str(n.as_str()).unwrap());
                        }
                    }
                    if let Some(n2) = key_re("py_version").captures(&l) {
                        if let Some(n) = n2.get(1) {
                            result.py_version = Some(Version::from_str(n.as_str()).unwrap());
                        }
                    }
                } else if in_dep {
                    if !l.is_empty() {
                        result
                            .dependencies
                            .push(Dependency::from_str(&l, false).unwrap());
                    }
                }
            }
        }

        Some(result)
    }

    /// Create a new `pyproject.toml` file.
    fn write_file(&self, filename: &str) {
        let file = PathBuf::from(filename);
        if file.exists() {
            abort("`pyproject.toml` already exists")
        }

        // todo: Use a bufer instead of String?
        let mut result = String::new();

        result.push_str("[tool.pypackage]\n");
        if let Some(name) = &self.name {
            result.push_str(&("name = \"".to_owned() + name + "\"\n"));
        } else {
            // Give name, and a few other fields default values.
            result.push_str(&("name = \"\"".to_owned() + "\n"));
        }
        if let Some(py_v) = self.py_version {
            result.push_str(&("version = \"".to_owned() + &py_v.to_string() + "\"\n"));
        } else {
            result.push_str(&("version = \"\"".to_owned() + "\n"));
        }
        if let Some(vers) = self.version {
            result.push_str(&(vers.to_string() + "\n"));
        }
        if let Some(author) = &self.author {
            result.push_str(&(author.to_owned() + "\n"));
        }

        result.push_str("\n\n");
        result.push_str("[tool.pypackage.dependencies]\n");
        for dep in self.dependencies.iter() {
            result.push_str(&(dep.to_cfg_string() + "\n"));
        }

        println!("FILE: {:?}", file);
        match fs::write(file, result) {
            Ok(_) => println!("Created `pyproject.toml`"),
            Err(_) => abort("Problem writing `pyproject.toml`"),
        }
    }
}

/// Create a template directory for a python project.
pub(crate) fn new(name: &str) -> Result<(), Box<Error>> {
    if !PathBuf::from(name).exists() {
        fs::create_dir_all(&format!("{}/{}", name, name))?;
        fs::File::create(&format!("{}/{}/main.py", name, name))?;
        fs::File::create(&format!("{}/README.md", name))?;
        fs::File::create(&format!("{}/LICENSE", name))?;
        fs::File::create(&format!("{}/pyproject.toml", name))?;
        fs::File::create(&format!("{}/.gitignore", name))?;
    }

    let gitignore_init = r##"# General Python ignores

build/
dist/
__pycache__/
.ipynb_checkpoints/
*.pyc
*~
*/.mypy_cache/


# Project ignores
"##;

    let pyproject_init = &format!(
        r##"[tool.pypackage]
name = "{}"
py_version = "3.7"
version = "0.1.0"
description = ""
author = ""

[tool.pypackage.dependencies]
"##,
        name
    );

    // todo: flesh readme out
    let readme_init = &format!("# {}", name);

    fs::write(&format!("{}/.gitignore", name), gitignore_init)?;
    fs::write(&format!("{}/pyproject.toml", name), pyproject_init)?;
    fs::write(&format!("{}/README.md", name), readme_init)?;

    Ok(())
}

/// Prompt which Python alias to use, if multiple are found.
fn prompt_alias(aliases: &[(String, Version)]) -> (String, Version) {
    // Todo: Overall, the API here is inelegant.
    println!("Found multiple Python aliases. Please enter the number associated with the one you'd like to use for this project:");
    for (i, (alias, version)) in aliases.iter().enumerate() {
        println!("{}: {} version: {}", i + 1, alias, version.to_string())
    }

    let mut mapping = HashMap::new();
    for (i, alias) in aliases.iter().enumerate() {
        mapping.insert(i + 1, alias);
    }

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Unable to read user input for version");

    let input = input
        .chars()
        .next()
        .expect("Problem reading input")
        .to_string();

    let (alias, version) = mapping
        .get(
            &input
                .parse::<usize>()
                .expect("Enter the number associated with the Python alias."),
        )
        .expect(
            "Can't find the Python alias associated with that number. Is it in the list above?",
        );
    (alias.to_string(), version.clone())
}

#[derive(Debug)]
pub struct AliasError {
    details: String,
}

impl Error for AliasError {
    fn description(&self) -> &str {
        &self.details
    }
}

impl fmt::Display for AliasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

/// Make an educated guess at the command needed to execute python the
/// current system.  An alternative approach is trying to find python
/// installations.
fn find_py_alias() -> Result<(String, Version), AliasError> {
    // todo expand, and iterate over versions.
    let possible_aliases = &[
        "python3.9",
        "python3.8",
        "python3.7",
        "python3.6",
        "python3.5",
        "python3.4",
        "python3.3",
        "python3.2",
        "python3.1",
        "python3",
        "python",
        "python2",
    ];

    let mut found_aliases = Vec::new();

    for alias in possible_aliases {
        // We use the --version command as a quick+effective way to determine if
        // this command is associated with Python.
        match commands::find_py_version(alias) {
            Some(v) => found_aliases.push((alias.to_string(), v)),
            None => (),
        }
    }

    match possible_aliases.len() {
        0 => Err(AliasError {
            details: "Can't find Python on the path.".into(),
        }),
        1 => Ok(found_aliases[0].clone()),
        _ => Ok(prompt_alias(&found_aliases)),
    }
}

fn find_sub_dependencies(package: Dependency) -> Vec<Dependency> {
    // todo: This will be useful for dependency resolution, and removing packages
    // todo no longer needed when running install.
    vec![]
}

/// Similar to that used by Cargo.lock
#[derive(Debug, Deserialize, Serialize)]
struct LockPackage {
    // We use Strings here instead of types like Version to make it easier to
    // serialize and deserialize
    // todo: We have an analog Package type; perhaps just figure out how to serialize that.
    name: String,
    version: Version,
    source: Option<String>,
    dependencies: Option<Vec<String>>,
}

//impl LockPackage {
//    pub fn to_pip_string(&self) -> String {
//        format!("{}={}", self.name, self.version)
//    }
//}

/// Modelled after [Cargo.lock](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html)
#[derive(Debug, Default, Deserialize, Serialize)]
struct Lock {
    package: Option<Vec<LockPackage>>,
    metadata: Option<String>, // ie checksums
}

impl Lock {
    fn add_packages(&mut self, packages: &[Package]) {
        // todo: Write tests for this.

        for package in packages {
            // Use the actual version installed, not the requirement!
            // todo: reconsider your package etc structs
            // todo: Perhaps impl to_lockpack etc from Package.
            let lock_package = LockPackage {
                name: package.name.clone(),
                version: package.version,
                source: package.source.clone(),
                dependencies: None,
            };

            match &mut self.package {
                Some(p) => p.push(lock_package),
                None => self.package = Some(vec![lock_package]),
            }
        }
    }
    // todo perhaps obsolete
    // Create a lock from dependencies; resolve them and their sub-dependencies. Find conflicts.
    //    fn from_dependencies(dependencies: &[Dependency]) -> Self {
    //        for dep in dependencies {
    //            match dep_resolution::get_warehouse_data(&dep.name) {
    //                Ok(data) => {
    //                    let warehouse_versions: Vec<Version> = data
    //                        .releases
    //                        .keys()
    //                        // Don't include release candidate and beta; they'll be None when parsing the string.
    //                        .filter(|v| Version::from_str2(&v).is_some())
    //                        .map(|v| Version::from_str2(&v).unwrap())
    //                        .collect();
    //
    //                    //                    match dep.best_match(&warehouse_versions) {
    //                    //                        Some(best) => {
    //                    //                            lock_packs.push(
    //                    //                                LockPackage {
    //                    //                                    name: dep.name.clone(),
    //                    //                                    version: best.to_string(),
    //                    //                                    source: None,  // todo
    //                    //                                    dependencies: None // todo
    //                    //                                }
    //                    //                            )
    //                    //                        }
    //                    //                        None => abort(&format!("Unable to find a matching dependency for {}", dep.to_toml_string())),
    //                    //                    }
    //
    //                    //                    for (v, release) in data.releases {
    //                    //                        let vers = Version::from_str2(&v);
    //                    //                        if
    //                    //                    }
    //                }
    //                Err(_) => abort(&format!("Problem getting warehouse data for {}", dep.name)),
    //            }
    //        }
    //        let lock_packs = vec![];
    //
    //        Self {
    //            metadata: None,
    //            package: Some(lock_packs),
    //        }
    //    }
}

/// Read dependency data from a lock file.
fn read_lock(filename: &str) -> Result<(Lock), Box<Error>> {
    let data = fs::read_to_string(filename)?;
    //    let t: Lock = toml::from_str(&data).unwrap();
    Ok(toml::from_str(&data)?)
}

/// Write dependency data to a lock file.
fn write_lock(filename: &str, data: &Lock) -> Result<(), Box<Error>> {
    let data = toml::to_string(data)?;
    //    let f = fs::File::open(filename).expect("cannot open pypackage.lock");

    // Wipe the existing data
    //    f.set_len(0).unwrap();
    //    fs::remove_file(filename).unwrap();

    fs::write(filename, data)?;
    Ok(())
}

fn create_venv(cfg_v: Option<&Version>, pyypackage_dir: &PathBuf) -> Version {
    // We only use the alias for creating the virtual environment. After that,
    // we call our venv's executable directly.

    // todo perhaps move alias finding back into create_venv, or make a
    // todo create_venv_if_doesnt_exist fn.
    let (alias, py_ver_from_alias) = match find_py_alias() {
        Ok(a) => a,
        Err(_) => {
            abort("Unable to find a Python version on the path");
            ("".to_string(), Version::new_short(0, 0)) // Required for compiler
        }
    };

    let lib_path = pyypackage_dir.join(format!(
        "{}.{}/lib",
        py_ver_from_alias.major, py_ver_from_alias.minor
    ));
    if !lib_path.exists() {
        fs::create_dir_all(&lib_path).expect("Problem creating __pypackages__ directory");
    }

    if let Some(c_v) = cfg_v {
        // We don't expect the config version to specify a patch, but if it does, take it
        // into account.
        if c_v != &py_ver_from_alias {
            println!("{:?}, {:?}", c_v, &py_ver_from_alias);
            abort(&format!("The Python version you selected ({}) doesn't match the one specified in `pyprojecttoml` ({})",
                           py_ver_from_alias.to_string(), c_v.to_string())
            );
        }
    }

    println!("Setting up Python environment...");

    // If the Python version's below 3.3, we must download and install the
    // `virtualenv` package, since `venv` isn't included.
    if py_ver_from_alias < Version::new_short(3, 3) {
        if let Err(_) = commands::install_virtualenv_global(&alias) {
            util::abort("Problem installing the virtualenv package, required by Python versions older than 3.3)");
        }
        if let Err(_) = commands::create_legacy_virtualenv(&alias, &lib_path, ".venv") {
            util::abort("Problem creating virtual environment");
        }
    } else {
        if let Err(_) = commands::create_venv(&alias, &lib_path, ".venv") {
            util::abort("Problem creating virtual environment");
        }
    }

    // Wait until the venv's created before continuing, or we'll get errors
    // when attempting to use it
    // todo: These won't work with Scripts ! - pass venv_path et cinstead
    let py_venv = lib_path.join("../.venv/bin/python");
    let pip_venv = lib_path.join("../.venv/bin/pip");
    util::wait_for_dirs(&vec![py_venv, pip_venv]).unwrap();

    py_ver_from_alias
}

/// Recursively add nodes.
//fn add_nodes(graph: &mut petgraph::Graph<Dependency, &str>, node: Dependency, parent_i: u32) {
//    let n_i = graph.add_node(node);
//    graph.add_edge(parent_i, n, "");
//
//    for dep in node.dependencies {
//        add_nodes(graph, dep, n_i)
//    }
//}
//
//fn make_dependency_graph(deps: &Vec<Dependency>) -> petgraph::Graph<Dependency, &str> {
//    let deps = deps.clone(); // todo do we want this?
//
//    let mut graph = petgraph::Graph::<Dependency, &str>::new();
//
//    let top_dep = Dependency {
//        // Dummy to hold the others
//        name: "".into(),
//        version_reqs: vec![],
//        dependencies: deps,
//    };
//
//    let top_i = graph.add_node(top_dep);
//    let t = graph.add_node(top_dep.clone());
//
//    for mut dep in deps {
//        add_nodes(&mut graph, dep, top_i);
//    }
//
//    graph
//}

fn main() {
    // todo perhaps much of this setup code should only be in certain match branches.
    let cfg_filename = "pyproject.toml";
    let lock_filename = "pypackage.lock";

    let mut cfg = Config::from_file(cfg_filename).unwrap_or_default();

    let opt = Opt::from_args();
    let subcmd = match opt.subcmds {
        Some(sc) => sc,
        None => {
            abort("No command entered. For a list of what you can do, run `pyproject --help`.");
            SubCommand::Init {} // Dummy to satisfy the compiler.
        }
    };

    // New doesn't execute any other logic. Init must execute befor the rest of the logic,
    // since it sets up a new (or modified) `pyproject.toml`. The rest of the commands rely
    // on the virtualenv and `pyproject.toml`, so make sure those are set up before processing them.
    match subcmd {
        SubCommand::New { name } => {
            new(&name).expect("Problem creating project");
            println!("Created a new Python project named {}", name);
            return;
        }
        SubCommand::Init {} => {
            edit_files::parse_req_dot_text(&mut cfg);
            edit_files::parse_pipfile(&mut cfg);
            edit_files::parse_poetry(&mut cfg);
            edit_files::update_pyproject(&cfg);

            cfg.write_file(cfg_filename);
        }
        _ => (),
    }

    let pypackage_dir = env::current_dir()
        .expect("Can't find current path")
        .join("__pypackages__");

    let py_version_cfg = cfg.py_version;

    // Check for environments. Create one if none exist. Set `vers_path`.
    let mut vers_path = PathBuf::new();
    match py_version_cfg {
        // The version's explicitly specified; check if an environment for that version
        // exists. If not, create one, and make sure it's the right version.
        Some(cfg_v) => {
            // The version's specified in the config. Ensure a virtualenv for this
            // is setup.  // todo: Confirm using --version on the python bin, instead of relying on folder name.

            // Don't include version patch in the directory name, per PEP 582.
            vers_path = pypackage_dir.join(&format!("{}.{}", cfg_v.major, cfg_v.minor));

            if !util::venv_exists(&vers_path.join(".venv")) {
                let created_vers = create_venv(Some(&cfg_v), &pypackage_dir);
            }
        }
        // The version's not specified in the config; Search for existing environments, and create
        // one if we can't find any.
        None => {
            // Note that we rely on the proper folder name, vice inspecting the binary.
            // ie: could also check `bin/python --version`.
            let venv_versions_found: Vec<Version> = util::possible_py_versions()
                .into_iter()
                .filter(|v| {
                    // todo: Missses `Scripts`!
                    let bin_path =
                        pypackage_dir.join(&format!("{}.{}/.venv/bin", v.major, v.minor));
                    util::venv_exists(&bin_path)
                })
                .collect();

            match venv_versions_found.len() {
                0 => {
                    let created_vers = create_venv(None, &pypackage_dir);
                    vers_path = pypackage_dir
                        .join(&format!("{}.{}", created_vers.major, created_vers.minor));
                }
                1 => {
                    vers_path = pypackage_dir.join(&format!(
                        "{}.{}",
                        venv_versions_found[0].major, venv_versions_found[0].minor
                    ));
                }
                _ => abort(
                    "Multiple Python environments found
                for this project; specify the desired one in `pyproject.toml`. Example:
[tool.pyproject]
py_version = \"3.7\"",
                ),
            }
        }
    };

    let lib_path = vers_path.join("lib");
    let (bin_path, lib_bin_path) = util::find_bin_path(&vers_path);

    let mut lock = match read_lock(lock_filename) {
        Ok(l) => {
            println!("Found lockfile");
            l
        }
        Err(_) => Lock::default(),
    };

    let args = opt.custom_bin;
    if !args.is_empty() {
        // todo better handling, eg abort
        let name = args.get(0).expect("Missing first arg").clone();
        let args: Vec<String> = args.into_iter().skip(1).collect();
        if let Err(_) = commands::run_bin(&bin_path, &lib_path, &name, &args) {
            abort(&format!(
                "Problem running the binary script {}. Is it installed? \
                 Try running `pypackage install {} -b`",
                name, name
            ));
        }

        return;
    }

    match subcmd {
        // Add pacakge names to `pyproject.toml` if needed. Then sync installed packages
        // and `pyproject.lock` with the `pyproject.toml`.
        SubCommand::Install { packages, bin } => {
            let mut new_deps: Vec<Dependency> = packages
                .into_iter()
                .map(|p| Dependency::from_str(&p, false).unwrap())
                .collect();

            // todo: Compare to existing listed lock_packs and merge appropriately.
            edit_files::add_dependencies(cfg_filename, &new_deps);

            cfg.dependencies.append(&mut new_deps);

            // Recursively add sub-dependencies.
            for mut dep in cfg.dependencies.iter_mut() {
                dep_resolution::populate_subdeps(&mut dep);
            }

            //            let lock = Lock::from_dependencies(&cfg.dependencies);
            let lock = Lock {
                metadata: None,
                package: None,
            }; // todo temp so it'll compile while we work around things.

            if let Some(lock_packs) = &lock.package {
                for lock_pack in lock_packs {
                    // todo: methods to convert between LockPack and Package, since they're analogous
                    let p = Package {
                        name: lock_pack.name.clone(),
                        //                        version: Version::from_str(&lock_pack.version).unwrap(),
                        version: lock_pack.version,
                        deps: vec![],
                        source: None,
                    };
                    if let Err(_) = commands::install(&bin_path, &vec![p], false, false) {
                        abort("Problem installing packages");
                    }
                }
            } else {
                println!("Found no dependencies in `pyproject.toml` to install")
            }

            if let Err(_) = write_lock(lock_filename, &lock) {
                abort("Problem writing lock file");
            }
        }
        SubCommand::Uninstall { packages } => {}

        SubCommand::Python { args } => {
            if let Err(_) = commands::run_python(&bin_path, &lib_path, &args) {
                abort("Problem running Python");
            }
        }
        SubCommand::Package {} => build::build(&bin_path, &lib_path, &cfg),
        SubCommand::Publish {} => build::publish(&bin_path, &cfg),

        // We already handled init
        SubCommand::Init {} => (),
        SubCommand::New { name } => (),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

}
