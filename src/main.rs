use regex::Regex;
use serde::Deserialize;
use std::path::Prefix::Verbatim;
use std::{
    env,
    error::Error,
    fmt, fs,
    io::{self, BufRead, BufReader},
    num, path,
    process::Command,
    str::FromStr,
    string::ParseError,
};
use structopt::StructOpt;
//use textio;
use std::collections::HashMap;
use toml;

mod build;
mod commands;

/// Categorize arguments parsed from the command line.
#[derive(Debug)]
enum Arg {
    Install,
    Uninstall,
    Python,
    IPython,
    Pip,
    List,
    Package,
    Publish,
    Other(String), // eg a script file, or package name to install.
}

impl FromStr for Arg {
    type Err = ParseError;

    fn from_str(arg: &str) -> Result<Self, Self::Err> {
        let result = match arg.to_string().to_lowercase().as_ref() {
            "install" => Arg::Install,
            "uninstall" => Arg::Uninstall,
            "python" => Arg::Python,
            "python3" => Arg::Python,
            "ipython" => Arg::IPython,
            "ipython3" => Arg::IPython,
            "pip" => Arg::Pip,
            "pip3" => Arg::Pip,
            "list" => Arg::List,
            "package" => Arg::Package,
            "publish" => Arg::Publish,
            _ => Arg::Other(arg.into()),
        };

        Ok(result)
    }
}

/// Similar to Arg, but grouped.
#[derive(Debug, PartialEq)]
enum Task {
    InstallAll,
    UninstallAll,
    Install(Vec<Package>),
    Uninstall(Vec<Package>),
    Python(Vec<String>),
    IPython(Vec<String>),
    Pip(Vec<String>), // If if we want pip list etc
    General(Vec<String>),
    Package,
    Publish,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
enum VersionType {
    Exact,
    Carot,
    Tilde,
}

impl ToString for VersionType {
    fn to_string(&self) -> String {
        match self {
            VersionType::Exact => "==".into(),
            // todo this isn't quite a valid mapping.
            VersionType::Carot => ">=".into(),
            VersionType::Tilde => ">=".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
struct Version {
    // todo wildcard
    major: u32,
    minor: u32,
    patch: Option<u32>,
}

impl Version {
    fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch: Some(patch),
        }
    }

    /// No patch specified.
    fn new_short(major: u32, minor: u32) -> Self {
        Self {
            major,
            minor,
            patch: None,
        }
    }
}

impl FromStr for Version {
    type Err = num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"^(\d{1,4})\.(\d{1,4})(?:\.(\d{1,4}))?$").unwrap();
        let caps = re
            .captures(s)
            .expect(&format!("Problem parsing version: {}", s));

        let major = caps.get(1).unwrap().as_str().parse::<u32>().unwrap();
        let minor = caps.get(2).unwrap().as_str().parse::<u32>().unwrap();

        let patch = match caps.get(3) {
            Some(p) => Some(p.as_str().parse::<u32>().unwrap()),
            None => None,
        };

        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

impl ToString for Version {
    fn to_string(&self) -> String {
        match self.patch {
            Some(patch) => format!("{}.{}.{}", self.major, self.minor, patch),
            None => format!("{}.{}", self.major, self.minor),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Package {
    name: String,
    version_type: VersionType, // Not used if version not specified.
    // None on version means not specified
    version: Option<Version>, // https://semver.org
}

impl Package {
    pub fn name_with_version(&self) -> String {
        match self.version {
            Some(version) => {
                self.name.clone() + &self.version_type.to_string() + &version.to_string()
            }
            None => self.name.clone(),
        }
    }
}

impl FromStr for Package {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // todo: Wildcard
        let re = Regex::new(
            r#"^(.+?)(?:\s*=\s*"([\^\~]?)(\d{1,4})(?:\.(\d{1,4}?))?(?:\.(\d{1,4})")?)?$"#,
        )
        .unwrap();

        let caps = re
            .captures(s)
            .expect(&format!("Problem parsing dependency: {}. Skipping", s));

        let name = caps.get(1).unwrap().as_str();

        let prefix = match caps.get(2) {
            Some(p) => Some(p.as_str()),
            None => None,
        };

        let major = match caps.get(3) {
            Some(p) => Some(p.as_str().parse::<u32>().unwrap()),
            None => None,
        };

        let mut minor = match caps.get(4) {
            Some(p) => Some(p.as_str().parse::<u32>().unwrap()),
            None => None,
        };

        let mut patch = match caps.get(5) {
            Some(p) => Some(p.as_str().parse::<u32>().unwrap()),
            None => None,
        };

        // If the version has 2 numbers, eg 4.3, the regex is picking up the second
        // as patch and None for minor.
        // todo: Ideally, fix the regex instead of using this workaround.
        if let Some(p) = patch {
            if minor.is_none() {
                minor = Some(p);
                patch = None;
            }
        }

        // If no major, Version is None
        let version = match major {
            Some(ma) => Some(Version {
                major: ma,
                minor: minor.unwrap_or(0),
                patch,
            }),
            None => None,
        };

        Ok(Self {
            name: name.to_string(),
            version,
            version_type: match prefix {
                Some(t) => {
                    if t.is_empty() {
                        VersionType::Exact
                    } else if t == "^" {
                        VersionType::Carot
                    } else {
                        VersionType::Tilde
                    }
                }
                None => VersionType::Exact,
            },
        })
    }
}

// todo: Another string parser to package, from pip fmt ie == / >=

#[derive(StructOpt, Debug)]
#[structopt(name = "basic")]
struct Opt {
    #[structopt(name = "args")]
    args: Vec<Arg>, // ie "install", "python" etc.
}

/// A config, parsed from pyproject.toml
#[derive(Debug, Default, Deserialize)]
struct Config {
    py_version: Option<Version>,
    dependencies: Vec<Package>,
    name: Option<String>,
    version: Option<Version>,
    author: Option<String>,
    author_email: Option<String>,
    description: Option<String>,
    classifiers: Vec<String>,
    homepage: Option<String>,
    repo_url: Option<String>,
    readme_filename: Option<String>,
}

fn key_re(key: &str) -> Regex {
    Regex::new(&format!(r#"^{}\s*=\s*"(.*)"$"#, key)).unwrap()
}

impl Config {
    pub fn from_file(file_name: &str) -> Self {
        let mut result = Config::default();
        let file = fs::File::open(file_name).expect("cannot open pyproject.toml");

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
                //                    if let Some(n2) = key_re("version").captures(&l) {
                //                        if let Some(n) = n2.get(1) {
                //                            result.version = Some(Version::from_str(n.as_str()).unwrap());
                //                        }
                //                    }
                } else if in_dep {
                    if !l.is_empty() {
                        result.dependencies.push(Package::from_str(&l).unwrap());
                    }
                }
            }
        }

        //        println!("cfg: {:?}", &result);
        result
    }
}

/// Prompt which Python alias to use, if multiple are found.
fn prompt_alias(aliases: &[(String, Version)]) -> (String, Version) {
    // Todo: Overall, the API here is inelegant.
    println!("Found multiple Python aliases. Please enter the number associated with the one you'd like to use for this project:");
    for (i, (alias, version)) in aliases.iter().enumerate() {
        println!("{}: {} version: {}", i, alias, version.to_string())
    }

    let mut mapping = HashMap::new();
    for (i, alias) in aliases.iter().enumerate() {
        mapping.insert(i, alias);
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
struct AliasError {
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
fn find_py_alias(config_ver: Option<Version>) -> Result<(String, Version), AliasError> {
    let mut guess = "python3";
    //    let version_guess = commands::find_version(guess);

    // todo expand, and iterate over versions.
    let possible_aliases = &[
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

fn venv_exists(venv_path: &path::PathBuf) -> bool {
    venv_path.exists()
}

fn find_sub_dependencies(package: Package) -> Vec<Package> {
    // todo: This will be useful for dependency resolution, and removing packages
    // todo no longer needed when running install.
    vec![]
}

fn find_tasks(args: &[Arg]) -> Vec<Task> {
    // We want to match args as appropriate. Ie, `python main.py`, and
    // `pip install django requests` are parsed as separate args,
    //but should be treated as single items.
    let mut result = vec![];
    // let mut current_task = vec![];

    for (i, arg) in args.iter().enumerate() {
        match arg {
            // Non-custom args are things like Python, Install etc;
            // start a new group.
            Arg::Install => {
                let mut packages: Vec<Package> = Vec::new();
                for i2 in i + 1..args.len() {
                    let arg2 = &args[i2];
                    match arg2 {
                        Arg::Other(name) => packages.push(Package::from_str(name).unwrap()),
                        _ => break,
                    }
                }
                if packages.is_empty() {
                    result.push(Task::InstallAll);
                } else {
                    result.push(Task::Install(packages))
                }
            }
            Arg::Uninstall => {
                // todo DRY
                let mut packages: Vec<Package> = Vec::new();
                for i2 in i + 1..args.len() {
                    let arg2 = &args[i2];
                    match arg2 {
                        Arg::Other(name) => packages.push(Package::from_str(name).unwrap()),
                        _ => break,
                    }
                }
                if packages.is_empty() {
                    result.push(Task::UninstallAll);
                } else {
                    result.push(Task::Uninstall(packages))
                }
            }
            Arg::Python => {
                let mut args_ = vec![];
                for i2 in i + 1..args.len() {
                    let arg2 = &args[i2];
                    if let Arg::Other(a) = arg2 {
                        args_.push(a.to_string());
                    }
                }
                result.push(Task::Python(args_));
            }
            Arg::IPython => {
                // todo DRY
                let mut args_ = vec![];
                for i2 in i + 1..args.len() {
                    let arg2 = &args[i2];
                    if let Arg::Other(a) = arg2 {
                        args_.push(a.to_string());
                    }
                    break; // todo: Consider how to handle more than one arg following `python`.
                }
                result.push(Task::IPython(args_));
            }

            Arg::Pip => {
                let mut args_ = vec![];
                for i2 in i + 1..args.len() {
                    let arg2 = &args[i2];
                    if let Arg::Other(arg) = arg2 {
                        args_.push(arg.to_string());
                    }
                    // List can be used directly as an arg, or passed to pip normally; handle
                    // the latter case here.
                    if let Arg::List = arg2 {
                        args_.push("list".to_string());
                    }
                }
                result.push(Task::Pip(args_));
            }

            Arg::List => {
                // todo

            }
            Arg::Package => result.push(Task::Package),
            Arg::Publish => result.push(Task::Publish),
            Arg::Other(_) => (),
        }
    }
    result
}

/// Write dependencies to pyproject.toml
fn add_dependencies(dependencies: &[Package]) {
    let data = fs::read_to_string("pyproject.toml")
        .expect("Unable to read pyproject.toml while attempting to add a dependency");

    // todo
    let new_data = data;

    fs::write("pyproject.toml", new_data)
        .expect("Unable to read pyproject.toml while attempting to add a dependency");
}

/// Remove dependencies from pyproject.toml
fn remove_dependencies(dependencies: &[Package]) {
    let data = fs::read_to_string("pyproject.toml")
        .expect("Unable to read pyproject.toml while attempting to add a dependency");

    // todo
    let new_data = data;

    fs::write("pyproject.toml", new_data)
        .expect("Unable to read pyproject.toml while attempting to add a dependency");
}

fn main() {
    let opt = Opt::from_args();
    let cfg = Config::from_file("pyproject.toml");

    let project_dir = env::current_dir().expect("Can't find current path");

    let py_version = cfg.py_version.unwrap_or(Version::new_short(3, 7)); // todo better default
    let venv_name = &format!(
        "__pypackages__/{}.{}/.venv",
        py_version.major, py_version.minor
    );
    let venv_path = project_dir.join(venv_name);

    if !venv_exists(&venv_path) {
        // We only use the alias for creating the virtual environment. After that,
        // we call our venv's executable directly.
        let alias = find_py_alias(cfg.py_version);
        // todo version QC
        match alias {
            Ok((alias, py_version)) => {
                commands::create_venv(&alias, "__pypackages__/3.7", ".venv", py_version);
            }
            Err(e) => panic!(e),
        }
    }

    for task in find_tasks(&opt.args).iter() {
        match task {
            Task::Install(packages) => {
                commands::install(&venv_name, packages, false);
                add_dependencies(packages);
            }
            Task::InstallAll => {
                commands::install(&venv_name, &cfg.dependencies, false);
            }
            Task::Uninstall(packages) => {
                commands::install(&venv_name, packages, true);
                remove_dependencies(packages);
            }
            Task::UninstallAll => {
                commands::install(&venv_name, &cfg.dependencies, true);
            }
            Task::Python(script) => commands::run_python(&venv_name, script, false),
            Task::IPython(script) => {
                let mut ipython_installed = false;
                for package in &cfg.dependencies {
                    if &package.name == "ipython" {
                        ipython_installed = true;
                    }
                }

                if !ipython_installed {
                    commands::install(
                        &venv_name,
                        &[Package {
                            name: "ipython".to_string(),
                            version: None,
                            version_type: VersionType::Exact,
                        }],
                        false,
                    );
                }
                commands::run_python(&venv_name, script, true)
            }
            Task::Pip(args) => commands::run_pip(&venv_name, args),
            Task::Package => build::build(&venv_name, &cfg),
            Task::Publish => build::publish(&venv_name, &cfg),

            Task::General(args) => (),
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn tasks_python() {
        let args = vec![Arg::Python];
        assert_eq!(vec![Task::Python(None)], find_tasks(&args));
    }

    #[test]
    fn tasks_python_with_script() {
        let script = "main.py".to_string();
        let args = vec![Arg::Python, Arg::Other(script.clone())];
        assert_eq!(vec![Task::Python(Some(script))], find_tasks(&args));
    }

    #[test]
    fn tasks_ipython() {
        let args = vec![Arg::IPython];
        assert_eq!(vec![Task::IPython(None)], find_tasks(&args));
    }

    #[test]
    fn tasks_install_one() {
        let name = "requests".to_string();
        let args = vec![Arg::Install, Arg::Other(name.clone())];

        assert_eq!(
            vec![Task::Install(vec![Package {
                name,
                version_type: VersionType::Exact,
                version: None,
            }])],
            find_tasks(&args)
        );
    }

    #[test]
    fn tasks_install_several() {
        let name1 = "numpy".to_string();
        let name2 = "scipy".to_string();
        let args = vec![
            Arg::Install,
            Arg::Other(name1.clone()),
            Arg::Other(name2.clone()),
        ];

        assert_eq!(
            vec![Task::Install(vec![
                Package {
                    name: name1,
                    version_type: VersionType::Exact,
                    version: None,
                },
                Package {
                    name: name2,
                    version_type: VersionType::Exact,
                    version: None,
                }
            ])],
            find_tasks(&args)
        );
    }

    #[test]
    fn tasks_install_all() {
        let args = vec![Arg::Install];
        assert_eq!(vec![Task::InstallAll], find_tasks(&args));
    }

    #[test]
    fn tasks_uninstall_one() {
        let name = "requests".to_string();
        let args = vec![Arg::Uninstall, Arg::Other(name.clone())];

        assert_eq!(
            vec![Task::Uninstall(vec![Package {
                name,
                version_type: VersionType::Exact,
                version: None,
            }])],
            find_tasks(&args)
        );
    }

    #[test]
    fn tasks_uninstall_several() {
        let name1 = "numpy".to_string();
        let name2 = "scipy".to_string();
        let args = vec![
            Arg::Uninstall,
            Arg::Other(name1.clone()),
            Arg::Other(name2.clone()),
        ];

        assert_eq!(
            vec![Task::Uninstall(vec![
                Package {
                    name: name1,
                    version_type: VersionType::Exact,
                    version: None,
                },
                Package {
                    name: name2,
                    version_type: VersionType::Exact,
                    version: None,
                }
            ])],
            find_tasks(&args)
        );
    }

    #[test]
    fn tasks_uninstall_all() {
        let args = vec![Arg::Uninstall];
        assert_eq!(vec![Task::UninstallAll], find_tasks(&args));
    }

    #[test]
    fn tasks_pip() {
        let args = vec![Arg::Pip, Arg::Other("list".into())];
        assert_eq!(vec![Task::Pip(vec!["list".into()])], find_tasks(&args));
    }

    #[test]
    fn tasks_general() {
        let name1 = "pip".to_string();
        let name2 = "list".to_string();
        let args = vec![Arg::Other(name1.clone()), Arg::Other(name2.clone())];
        assert_eq!(vec![Task::General(vec![name1, name2])], find_tasks(&args));
    }

    // todo: Invalid or non-standard task arg combos for tasks
    // todo: Versioned tasks.

    #[test]
    fn valid_version() {
        assert_eq!(
            Version::from_str("3.7").unwrap(),
            Version {
                major: 3,
                minor: 7,
                patch: None
            }
        );
        assert_eq!(Version::from_str("3.12.5").unwrap(), Version::new(3, 12, 5));
        assert_eq!(Version::from_str("0.1.0").unwrap(), Version::new(0, 1, 0));
    }

    #[test]
    #[should_panic(expected = "Problem parsing version: 3-7")]
    fn bad_version() {
        Version::from_str("3-7").unwrap();
    }

    #[test]
    fn parse_package_novers() {
        let p = Package::from_str("saturn").unwrap();
        assert_eq!(
            p,
            Package {
                name: "saturn".into(),
                version: None,
                version_type: VersionType::Exact,
            }
        )
    }

    #[test]
    fn parse_package_withvers() {
        let p = Package::from_str("bolt = \"3.1.4\"").unwrap();
        assert_eq!(
            p,
            Package {
                name: "bolt".into(),
                version: Some(Version::new(3, 1, 4)),
                version_type: VersionType::Exact,
            }
        )
    }

    #[test]
    fn parse_package_carot() {
        let p = Package::from_str("chord = \"^2.7.18\"").unwrap();
        assert_eq!(
            p,
            Package {
                name: "chord".into(),
                version: Some(Version::new(2, 7, 18)),
                version_type: VersionType::Carot,
            }
        )
    }

    #[test]
    fn parse_package_tilde_short() {
        let p = Package::from_str("sphere = \"~6.7\"").unwrap();
        assert_eq!(
            p,
            Package {
                name: "sphere".into(),
                version: Some(Version::new_short(6, 7)),
                version_type: VersionType::Tilde,
            }
        )
    }
}
