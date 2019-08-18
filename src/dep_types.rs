use std::error::Error;
use std::{cmp, fmt, num, str::FromStr};

use regex::{Captures, Match, Regex};
use serde::{Deserialize, Serialize};

pub const MAX_VER: u32 = 999_999; // Represents the highest major version we can have

#[derive(Debug, PartialEq)]
pub struct DependencyError {
    pub details: String,
}

impl DependencyError {
    pub fn new(details: &str) -> Self {
        Self {
            details: details.to_owned(),
        }
    }
}

impl Error for DependencyError {
    fn description(&self) -> &str {
        &self.details
    }
}

impl fmt::Display for DependencyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl From<num::ParseIntError> for DependencyError {
    fn from(_: num::ParseIntError) -> Self {
        Self {
            details: "Pare int error".into(),
        }
    }
}

impl From<reqwest::Error> for DependencyError {
    fn from(_: reqwest::Error) -> Self {
        Self {
            details: "Network error".into(),
        }
    }
}

/// An exact, 3-number Semver version.
#[derive(Clone, Copy, Default, Deserialize, Eq, Hash, PartialEq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// No patch specified.
    pub fn new_short(major: u32, minor: u32) -> Self {
        Self {
            major,
            minor,
            patch: 0,
        }
    }

    pub fn _max() -> Self {
        Self::new(MAX_VER, 0, 0)
    }

    pub fn to_string_med(&self) -> String {
        format!("{}.{}", self.major, self.minor)
    }
    pub fn to_string_short(&self) -> String {
        format!("{}", self.major,)
    }

    /// ie cp37, a version from Pypi.
    pub fn from_cp_str(s: &str) -> Result<Self, DependencyError> {
        if s == "py2.py3" {
            return Ok(Self::new(3, 3, 0));
        }

        let re = Regex::new(r"^(?:(?:cp)|(?:py))?(\d)(\d)?$").unwrap();

        if let Some(caps) = re.captures(s) {
            return Ok(Self {
                major: caps.get(1).unwrap().as_str().parse::<u32>()?,
                minor: match caps.get(2) {
                    Some(m) => m.as_str().parse::<u32>()?,
                    None => 0,
                },
                patch: 0,
            });
        }

        Err(DependencyError::new(&format!(
            "Problem parsing Python version from {}",
            s
        )))
    }
}

impl FromStr for Version {
    type Err = DependencyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // We could try to handle this as a single regex, but splitting it up may be easier here.

        let re_maj_only = Regex::new(r"^(\d{1,9})\.?$").unwrap();
        let re_maj_minor = Regex::new(r"^(\d{1,9})\.(\d{1,9})\.?$").unwrap();
        let re_maj_minor_patch = Regex::new(r"^(\d{1,9})\.(\d{1,9})\.(\d{1,9})\.?$").unwrap();

        if let Some(caps) = re_maj_only.captures(s) {
            return Ok(Self {
                major: caps.get(1).unwrap().as_str().parse::<u32>()?,
                minor: 0,
                patch: 0,
            });
        }

        if let Some(caps) = re_maj_minor.captures(s) {
            return Ok(Self {
                major: caps.get(1).unwrap().as_str().parse::<u32>()?,
                minor: caps.get(2).unwrap().as_str().parse::<u32>()?,
                patch: 0,
            });
        }

        if let Some(caps) = re_maj_minor_patch.captures(s) {
            return Ok(Self {
                major: caps.get(1).unwrap().as_str().parse::<u32>()?,
                minor: caps.get(2).unwrap().as_str().parse::<u32>()?,
                patch: caps.get(3).unwrap().as_str().parse::<u32>()?,
            });
        }

        Err(DependencyError::new(&format!(
            "Problem parsing version: {}",
            s
        )))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        if self.major != other.major {
            self.major.cmp(&other.major)
        } else if self.minor != other.minor {
            self.minor.cmp(&other.minor)
        } else {
            self.patch.cmp(&other.patch)
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl ToString for Version {
    fn to_string(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl fmt::Debug for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.to_string())
    }
}

/// Specify the type of version requirement
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum ReqType {
    Exact,
    Gte,
    Lte,
    Ne,
    Gt,
    Lt,
    Caret,
    Tilde,
    // todo wildcard
}

impl ToString for ReqType {
    /// These show immediately before the version numbers
    fn to_string(&self) -> String {
        match self {
            ReqType::Exact => "==".into(),
            ReqType::Gte => ">=".into(),
            ReqType::Lte => "<=".into(),
            ReqType::Gt => ">".into(),
            ReqType::Lt => "<".into(),
            ReqType::Ne => "!=".into(),
            ReqType::Caret => "^".into(),
            ReqType::Tilde => "~".into(),
        }
    }
}

impl FromStr for ReqType {
    type Err = DependencyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "==" => Ok(ReqType::Exact),
            ">=" => Ok(ReqType::Gte),
            "<=" => Ok(ReqType::Lte),
            ">" => Ok(ReqType::Gt),
            "<" => Ok(ReqType::Lt),
            "!=" => Ok(ReqType::Ne),
            "^" => Ok(ReqType::Caret),
            "~" => Ok(ReqType::Tilde),
            _ => Err(DependencyError::new("Problem parsing ReqType")),
        }
    }
}

/// For holding semvar-style version requirements with Caret, tilde etc
/// [Ref](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Constraint {
    pub type_: ReqType,
    pub major: u32,
    pub minor: Option<u32>,
    pub patch: Option<u32>,
}

impl Constraint {
    pub fn version(&self) -> Version {
        Version::new(self.major, self.minor.unwrap_or(0), self.patch.unwrap_or(0))
    }
}

impl FromStr for Constraint {
    type Err = DependencyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // todo: You could delegate part of this out, or at least share the regex with Version::from_string
        if s == "*" {
            return Ok(Self::new(ReqType::Gte, 0, 0, 0));
        }

        let re = Regex::new(
            r"^(\^|~|==|<=|>=|<|>|!=)?(\d{1,9})\.?(?:(?:(\d{1,9})\.?)?\.?(\d{1,9})?)?(\.?.*)$",
        )
        .unwrap();

        let caps = match re.captures(s) {
            Some(c) => c,
            None => return Err(DependencyError::new("Problem parsing constraint")),
        };

        // Only major is required.
        let type_ = match caps.get(1) {
            Some(t) => ReqType::from_str(t.as_str())?,
            None => ReqType::Exact,
        };

        let major = match caps.get(2) {
            Some(p) => p.as_str().parse::<u32>()?,
            None => panic!("Problem parsing version"),
        };

        let minor = match caps.get(3) {
            Some(p) => Some(p.as_str().parse::<u32>()?),
            None => None,
        };

        let patch = match caps.get(4) {
            Some(p) => Some(p.as_str().parse::<u32>()?),
            None => None,
        };

        Ok(Self {
            major,
            minor,
            patch,
            type_,
        })
    }
}

/// A single version req. Can be chained together.
impl Constraint {
    pub fn new(type_: ReqType, major: u32, minor: u32, patch: u32) -> Self {
        Self {
            type_,
            major,
            minor: Some(minor),
            patch: Some(patch),
            //            suffix: None,
        }
    }

    /// From a comma-separated list
    pub fn from_str_multiple(vers: &str) -> Result<Vec<Self>, DependencyError> {
        let mut result = vec![];
        for req in vers.split(',') {
            match Self::from_str(req) {
                Ok(r) => result.push(r),
                Err(e) => return Err(e),
            }
        }
        Ok(result)
    }

    pub fn to_string(&self, ommit_equals: bool, pip_style: bool) -> String {
        // ommit_equals indicates we dont' want to add any type if it's exact. Eg in config files.
        // pip_style means that ^ is transformed to ^=, and ~ to ~=
        let mut type_str = if ommit_equals && self.type_ == ReqType::Exact {
            "".to_string()
        } else {
            self.type_.to_string()
        };
        if pip_style {
            match self.type_ {
                ReqType::Caret => type_str.push_str("="),
                ReqType::Tilde => type_str.push_str("="),
                _ => (),
            }
        }

        if let Some(mi) = self.minor {
            if let Some(p) = self.patch {
                format!("{}{}.{}.{}", type_str, self.major, mi, p)
            } else {
                format!("{}{}.{}", type_str, self.major, mi)
            }
        } else {
            format!("{}{}", type_str, self.major)
        }
    }

    /// Find the lowest and highest compatible versions. Return a vec, since the != requirement type
    /// has two ranges.
    pub fn compatible_range(&self) -> Vec<(Version, Version)> {
        // Version being unspecified means any version's acceptable; setting to 0s ensure that.
        // Treat missing minor and patch values as 0
        let minor = self.minor.unwrap_or(0);
        let patch = self.patch.unwrap_or(0);

        let self_version = Version::new(self.major, minor, patch);

        let highest = Version::new(MAX_VER, 0, 0);
        let lowest = Version::new(0, 0, 0);
        let max;

        let safely_subtract = || {
            // Don't try to make a negative version component.
            let mut major = self_version.major;
            let mut minor = self_version.minor;
            let mut patch = self_version.patch;
            // ie 0.0.0. Return max of 0.0.0
            if self_version.major == 0 && self_version.minor == 0 && self_version.patch == 0 {}
            // ie 3.0.0. Return max of 2.999999.999999
            if self_version.minor == 0 && self_version.patch == 0 {
                major -= 1;
                minor = MAX_VER;
                patch = MAX_VER;
            // ie 2.9.0. Return max of 2.8.999999
            } else if self_version.patch == 0 {
                minor -= 1;
                patch = MAX_VER
            } else {
                patch -= 1;
            }
            (major, minor, patch)
        };

        // Note that other than for not-equals, the the resulting Vec has len 1.
        match self.type_ {
            ReqType::Exact => vec![(self_version, self_version)],
            ReqType::Gte => vec![(self_version, highest)],
            ReqType::Lte => vec![(lowest, self_version)],
            ReqType::Gt => vec![(
                Version::new(self.major, self_version.minor, self_version.patch + 1),
                highest,
            )],
            ReqType::Lt => {
                let (major, minor, patch) = safely_subtract();
                vec![(lowest, Version::new(major, minor, patch))]
            }
            ReqType::Ne => {
                let (major, minor, patch) = safely_subtract();
                vec![
                    (lowest, Version::new(major, minor, patch)),
                    (
                        Version::new(self.major, self_version.minor, self_version.patch + 1),
                        highest,
                    ),
                ]
            }
            // This section DRY from `compatible`.
            ReqType::Caret => {
                if self.major > 0 {
                    max = Version::new(self.major + 1, 0, 0);
                } else if self_version.minor > 0 {
                    max = Version::new(0, self_version.minor + 1, 0);
                } else {
                    max = Version::new(0, 0, self_version.patch + 2);
                }
                vec![(self_version, max)]
            }
            // For tilde, if minor's specified, can only increment patch.
            // If not, can increment minor or patch.
            ReqType::Tilde => {
                if self.minor.is_some() {
                    max = Version::new(self.major, self_version.minor + 1, 0);
                } else {
                    max = Version::new(self.major + 1, 0, 0);
                }
                vec![(self_version, max)]
            }
        }
    }

    pub fn is_compatible(&self, version: &Version) -> bool {
        // Version being unspecified means any version's acceptable; setting to 0s ensure that.
        // Treat missing minor and patch values as 0
        let minor = self.minor.unwrap_or(0);
        let patch = self.patch.unwrap_or(0);

        let self_version = Version::new(self.major, minor, patch);

        let min = self_version;
        let max;

        match self.type_ {
            ReqType::Exact => self_version == *version,
            ReqType::Gte => self_version <= *version,
            ReqType::Lte => self_version >= *version,
            ReqType::Gt => self_version < *version,
            ReqType::Lt => self_version > *version,
            ReqType::Ne => self_version != *version,
            ReqType::Caret => {
                if self.major > 0 {
                    max = Version::new(self.major + 1, 0, 0);
                } else if self_version.minor > 0 {
                    max = Version::new(0, self_version.minor + 1, 0);
                } else {
                    max = Version::new(0, 0, self_version.patch + 2);
                }
                //                println!(
                //                    "MIN: {}, vers: {} Max: {}",
                //                    &min.to_string(),
                //                    &version.to_string(),
                //                    &max.to_string()
                //                );
                min <= *version && *version < max
            }
            // For tilde, if minor's specified, can only increment patch.
            // If not, can increment minor or patch.
            ReqType::Tilde => {
                if self.minor.is_some() {
                    max = Version::new(self.major, self_version.minor + 1, 0);
                } else {
                    max = Version::new(self.major + 1, 0, 0);
                }
                min < *version && *version < max
            }
        }
    }
}

//pub fn to_ranges(reqs: &[Constraint]) -> Vec<(Version, Version)> {
//    // If no requirement specified, return the full range.
//    if reqs.is_empty() {
//        vec![(Version::new(0, 0, 0), Version::new(MAX_VER, 0, 0))]
//    } else {
//        reqs.iter()
//            .map(|r| r.compatible_range())
//            .flatten()
//            .collect()
//    }
//}

//fn intersection_convert_one(
//    constrs1: &[Constraint],
//    ranges2: &[(Version, Version)],
//) -> Vec<(Version, Version)> {
//    let mut ranges1 = vec![];
//
//    for constr in constrs1 {
//        for range in constr.compatible_range() {
//            ranges1.push(range);
//        }
//    }
//
//    intersection(&ranges1, ranges2)
//}

pub fn intersection_many(reqs: &[Vec<Constraint>]) -> Vec<(Version, Version)> {
    // todo: Broken for notequals, which involves joining two ranges with OR logic.
    let mut flattened = vec![];
    for req in reqs {
        for constr in req.iter() {
            flattened.push(constr);
        }
    }

    let mut tuples = vec![];
    for r in flattened.into_iter() {
        tuples.append(&mut r.compatible_range());
    }

    //    let tuples: Vec<(Version, Version)> = flattened.iter().map(|r| r.compatible_range()).collect();
    intersection_many2(&tuples)
}

/// Interface an arbitrary number of constraint sets into the intersection fn(s), which
/// handle 2 at a time.
fn intersection_many2(reqs: &[(Version, Version)]) -> Vec<(Version, Version)> {
    // todo: Broken for notequals, which involves joining two ranges with OR logic.
    let init = vec![(Version::new(0, 0, 0), Version::new(MAX_VER, 0, 0))];
    reqs.iter().fold(init, |acc, constraint_set| {
        intersection(&[*constraint_set], &acc)
    })
}

///// Find the intersection of two sets of version requirements. Result is a Vec of (min, max) tuples.
//pub fn intersection_convert_both(
//    reqs1: &[Constraint],
//    reqs2: &[Constraint],
//) -> Vec<(Version, Version)> {
//    let mut ranges1 = vec![];
//    for req in reqs1 {
//        for range in req.compatible_range() {
//            ranges1.push(range);
//        }
//    }
//
//    let mut ranges2 = vec![];
//    for req in reqs2 {
//        for range in req.compatible_range() {
//            ranges2.push(range);
//        }
//    }
//
//    intersection(&ranges1, &ranges2)
//}

/// Find the intersection of two sets of version requirements. Result is a Vec of (min, max) tuples.
pub fn intersection(
    ranges1: &[(Version, Version)],
    ranges2: &[(Version, Version)],
) -> Vec<(Version, Version)> {
    // todo: Should we use and all the way, and pass a net iterator?
    let mut result = vec![];
    // Each range imposes an additonal constraint.
    for rng1 in ranges1 {
        for rng2 in ranges2 {
            // 0 is min, 1 is max.
            if rng2.1 >= rng1.0 && rng1.1 >= rng2.0 {
                result.push((cmp::max(rng2.0, rng1.0), cmp::min(rng1.1, rng2.1)));
            }
        }
    }
    result
}

/// For parsing Req from string.
fn parse_extras(
    m: Option<Match>,
) -> (
    Option<String>,
    Option<(ReqType, crate::Os)>,
    Option<Constraint>,
) {
    let mut extra = None;
    let mut sys_platform = None;
    let mut python_version = None;

    match m {
        Some(s) => {
            let extras = s.as_str();
            // Now that we've extracted extras etc, parse them with a new re.
            let ex_re = Regex::new(
                r#"(extra|sys_platform|python_version)\s*(\^|~|==|<=|>=|<|>|!=)\s*['"](.*?)['"]"#,
            )
            .unwrap();

            for caps in ex_re.captures_iter(extras) {
                let type_ = caps.get(1).unwrap().as_str();
                let req_type = caps.get(2).unwrap().as_str();
                let val = caps.get(3).unwrap().as_str();

                match type_ {
                    "extra" => extra = Some(val.to_owned()),
                    "sys_platform" => {
                        sys_platform = Some((
                            ReqType::from_str(req_type)
                                .expect(&format!("Problem parsing reqtype: {}", req_type)),
                            crate::Os::from_str(val)
                                .expect(&format!("Problem parsing Os: {}", val)),
                        ))
                    }
                    "python_version" => {
                        // If we parse reqtype and version separately, version will be forced
                        // to take 3 digits, even if not all 3 are specified.
                        python_version =
                            Some(Constraint::from_str(&(req_type.to_owned() + val)).expect(
                                &format!("Problem parsing constraint: {} {}", req_type, val),
                            ));
                    }
                    _ => println!("Found unexpected extra: {}", type_),
                }
            }
        }
        None => {
            extra = None;
            sys_platform = None;
            python_version = None;
        }
    };
    (extra, sys_platform, python_version)
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Req {
    pub name: String,
    pub constraints: Vec<Constraint>,
    pub extra: Option<String>, // todo:
    pub sys_platform: Option<(ReqType, crate::Os)>,
    pub python_version: Option<Constraint>,
}

impl Req {
    pub fn new(name: String, constraints: Vec<Constraint>) -> Self {
        Self {
            name,
            constraints,
            extra: None,
            sys_platform: None,
            python_version: None,
        }
    }

    pub fn from_str(s: &str, pypi_fmt: bool) -> Result<Self, DependencyError> {
        // eg some-crate = { version = "1.0", registry = "my-registry" }
        // todo
        let re_detailed = Regex::new(r#"^(.*?)\s*=\s*\{(.*)\}"#).unwrap();
        if let Some(caps) = re_detailed.captures(s) {
            let name = caps.get(1).unwrap().as_str().to_owned();

            let re_dets = Regex::new(r#"\s*(.*?)/s*=\s*?(.*)?}"#).unwrap();
            let dets = caps.get(2).unwrap().as_str();
            //            let features: Vec<(String, String)> = re_dets
            let features: Vec<String> = re_dets
                .find_iter(dets)
                .map(|caps| {
                    //                    (
                    caps.as_str().to_owned()
                    //                        caps.get(2).unwrap().as_str().to_owned(),
                    //                    )
                })
                .collect();
        }

        let re = if pypi_fmt {
            // eg saturn (>=0.3.4) or argon2-cffi (>=16.1.0) ; extra == 'argon2'
            //            Regex::new(r"^(.*?)\s+\((.*)\)(?:\s*;\s*extra == '(.*)')?$").unwrap()
            // todo deal with extra etc
            // Note: We specify what chars are acceptable in a name instead of using
            // wildcard, so we don't accidentally match a semicolon here if a
            // set of parens appears later. The non-greedy ? in the version-matching
            // expression's important as well, in some cases of extras.
            //            Regex::new(r"^([a-zA-Z\-0-9._]+)\s+\((.*?)\)(?:\s*;\s*(.*))?$").unwrap();
            // Whoah!
            Regex::new(r#"^([a-zA-Z\-0-9._]+)\s+\((.*?)\)(?:(?:\s*;\s*)(.*))?$"#).unwrap()
        } else {
            // eg saturn = ">=0.3.4", as in pyproject.toml
            // todo extras in this format?
            Regex::new(r#"^(.*?)\s*=\s*["'](.*)["']$"#).unwrap()
        };

        // todo: Excessive nesting
        if let Some(caps) = re.captures(s) {
            let name = caps.get(1).unwrap().as_str().to_owned();
            let reqs_m = caps.get(2).unwrap();
            let constraints = Constraint::from_str_multiple(reqs_m.as_str())?;

            let (extra, sys_platform, python_version) = parse_extras(caps.get(3));

            return Ok(Self {
                name,
                constraints,
                extra,
                sys_platform,
                python_version,
            });
        };

        // Check if no version is specified.
        let novers_re = if pypi_fmt {
            Regex::new(r"^([a-zA-Z\-0-9._]+)(?:(?:\s*;\s*)(.*))?$").unwrap()
        } else {
            // todo extras
            Regex::new(r"^([a-zA-Z\-0-9._]+)$").unwrap()
        };

        if let Some(caps) = novers_re.captures(s) {
            let (extra, sys_platform, python_version) = parse_extras(caps.get(2));

            return Ok(Self {
                name: caps.get(1).unwrap().as_str().to_string(),
                constraints: vec![],
                extra,
                sys_platform,
                python_version,
            });
        }
        Err(DependencyError::new(&format!(
            "Problem parsing version requirement: {}",
            s
        )))
    }

    /// We use this for parsing requirements.txt.
    pub fn from_pip_str(s: &str) -> Option<Self> {
        // todo multiple ie single quotes support?
        // Check if no version is specified.
        if Regex::new(r"^([a-zA-Z\-0-9]+)$")
            .unwrap()
            .captures(s)
            .is_some()
        {
            return Some(Self::new(s.to_string(), vec![]));
        }

        let re = Regex::new(r"^(.*?)((?:\^|~|==|<=|>=|<|>|!=).*)$").unwrap();

        let caps = match re.captures(s) {
            Some(c) => c,
            // todo: Figure out how to return an error
            None => return None,
        };

        let name = caps.get(1).unwrap().as_str().to_string();
        let req = Constraint::from_str(caps.get(2).unwrap().as_str())
            .expect("Problem parsing requirement");

        Some(Self::new(name, vec![req]))
    }

    /// eg `saturn = "^0.3.1"` or `matplotlib = "3.1.1"`
    pub fn to_cfg_string(&self) -> String {
        // todo suffix?
        //                let suffix_text = if let Some(suffix) = self.suffix.clone() {
        //            suffix
        //        } else {
        //            "".to_owned()
        //        };

        match self.constraints.len() {
            0 => self.name.to_owned(),
            _ => format!(
                r#"{} = "{}""#,
                self.name,
                self.constraints
                    .iter()
                    .map(|r| r.to_string(true, false))
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
        }
    }

    //    /// Return true if other is a subset of self.
    //    fn _fully_contains(&self, other: &Self) -> bool {
    //
    //    }
}

/// Includes information for describing a `Python` dependency.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Dependency {
    pub name: String,
    pub version: Version,

    //    pub filename: String,
    //    pub hash: String, // todo do you want hash, url, filename here?
    //    pub file_url: String,
    pub reqs: Vec<Req>,

    pub constraints_for_this: Vec<Constraint>, // Ie what constraints drove this node's version?

    //    pub dependencies: Vec<DepNode>,
    pub extras: Vec<String>,
}

/// Similar to that used by Cargo.lock. Represents an exact package to download. // todo(Although
/// todo the dependencies field isn't part of that/?)
#[derive(Debug, Deserialize, Serialize)]
pub struct LockPackage {
    // We use Strings here instead of types like Version to make it easier to
    // serialize and deserialize
    // todo: We have an analog Package type; perhaps just figure out how to serialize that.
    pub name: String,
    //    version: Version,  todo
    pub version: String,
    pub source: Option<String>,
    pub dependencies: Option<Vec<String>>,
}

/// Modelled after [Cargo.lock](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html)
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Lock {
    pub package: Option<Vec<LockPackage>>,
    pub metadata: Option<Vec<String>>, // ie checksums
}

//impl Lock {
//    // todo delete this?
//    fn add_packages(&mut self, packages: &[Package]) {
//        // todo: Write tests for this.
//
//        for package in packages {
//            // Use the actual version installed, not the requirement!
//            // todo: reconsider your package etc structs
//            // todo: Perhaps impl to_lockpack etc from Package.
//            let lock_package = LockPackage {
//                name: package.name.to_owned(),
//                version: package.version.to_string(),
//                source: package.source.clone(),
//                dependencies: None,
//            };
//
//            match &mut self.package {
//                Some(p) => p.push(lock_package),
//                None => self.package = Some(vec![lock_package]),
//            }
//        }
//    }
//}

#[cfg(test)]
pub mod tests {
    use ReqType::{Caret, Exact, Gt, Gte, Lt, Lte, Ne, Tilde};

    use super::*;

    #[test]
    fn compat_caret() {
        let req1 = Constraint::new(Caret, 1, 2, 3);
        let req2 = Constraint::new(Caret, 0, 2, 3);
        let req3 = Constraint::new(Caret, 0, 0, 3);
        let req4 = Constraint::new(Caret, 0, 0, 3);

        assert!(req1.is_compatible(&Version::new(1, 9, 9)));
        assert!(!req1.is_compatible(&Version::new(2, 0, 0)));
        assert!(req2.is_compatible(&Version::new(0, 2, 9)));
        assert!(!req2.is_compatible(&Version::new(0, 3, 0)));
        assert!(req3.is_compatible(&Version::new(0, 0, 3)));
        assert!(!req3.is_compatible(&Version::new(0, 0, 5)));
        // Caret requirements below major and minor v 0 must be exact.
        assert!(req4.is_compatible(&Version::new(0, 0, 3)));
        //        assert!(!req4.is_compatible(&Version::new(0, 0, 4)));
        assert!(!req4.is_compatible(&Version::new(0, 0, 5)));
    }

    #[test]
    fn compat_gt_eq() {
        let req1 = Constraint::new(Gte, 1, 2, 3);
        let req2 = Constraint::new(Gt, 0, 2, 3);
        let req3 = Constraint::new(Exact, 0, 0, 3);

        assert!(req1.is_compatible(&Version::new(1, 2, 3)));
        assert!(req1.is_compatible(&Version::new_short(4, 2)));
        assert!(!req1.is_compatible(&Version::new(1, 2, 2)));
        assert!(req2.is_compatible(&Version::new(0, 2, 9)));
        assert!(!req2.is_compatible(&Version::new(0, 1, 0)));
        assert!(!req2.is_compatible(&Version::new(0, 2, 3)));
        assert!(req3.is_compatible(&Version::new(0, 0, 3)));
        assert!(!req3.is_compatible(&Version::new(0, 0, 5)));
    }

    #[test]
    fn valid_version() {
        assert_eq!(
            Version::from_str("3.7").unwrap(),
            Version {
                major: 3,
                minor: 7,
                patch: 0
            }
        );
        assert_eq!(Version::from_str("3.12.5").unwrap(), Version::new(3, 12, 5));
        assert_eq!(Version::from_str("0.1.0").unwrap(), Version::new(0, 1, 0));
        assert_eq!(Version::from_str("1").unwrap(), Version::new(1, 0, 0));
        assert_eq!(Version::from_str("2.3").unwrap(), Version::new(2, 3, 0));
    }

    #[test]
    fn bad_version() {
        assert_eq!(
            Version::from_str("3-7"),
            Err(DependencyError {
                details: "Problem parsing version: 3-7".to_owned()
            })
        );

        // for now at least, we don't support alpha, beta in version, as how to handle them
        // isn't well-defined.
        assert_eq!(
            Version::from_str("1.2a2"),
            Err(DependencyError {
                details: "Problem parsing version: 1.2a2".to_owned()
            })
        );
    }

    #[test]
    fn version_req_with_suffix() {
        let a = "!=2.3b3";
        let b = "^1.3.32rc1";
        let c = "^1.3.32.dep1";

        let req_a = Constraint {
            major: 2,
            minor: Some(3),
            patch: None,
            type_: Ne,
        };
        let req_b = Constraint {
            major: 1,
            minor: Some(3),
            patch: Some(32),
            type_: Caret,
        };
        let req_c = Constraint {
            major: 1,
            minor: Some(3),
            patch: Some(32),
            type_: Caret,
        };

        assert_eq!(Constraint::from_str(a).unwrap(), req_a);
        assert_eq!(Constraint::from_str(b).unwrap(), req_b);
        assert_eq!(Constraint::from_str(c).unwrap(), req_c);
    }

    #[test]
    fn version_req_tostring() {
        let a = "!=2.3";
        let b = "^1.3.32";
        let c = "~2.3";
        let d = "==5";
        let e = "<=11.2.3";
        let f = ">=0.0.1";
        let f = ">=0.0.1";

        let req_a = Constraint {
            major: 2,
            minor: Some(3),
            patch: None,
            type_: Ne,
        };
        let req_b = Constraint::new(Caret, 1, 3, 32);
        let req_c = Constraint {
            major: 2,
            minor: Some(3),
            patch: None,
            type_: Tilde,
        };
        let req_d = Constraint {
            major: 5,
            minor: None,
            patch: None,
            type_: Exact,
        };
        let req_e = Constraint::new(Lte, 11, 2, 3);
        let req_f = Constraint::new(Gte, 0, 0, 1);

        assert_eq!(Constraint::from_str(a).unwrap(), req_a);
        assert_eq!(Constraint::from_str(b).unwrap(), req_b);
        assert_eq!(Constraint::from_str(c).unwrap(), req_c);
        assert_eq!(Constraint::from_str(d).unwrap(), req_d);
        assert_eq!(Constraint::from_str(e).unwrap(), req_e);
        assert_eq!(Constraint::from_str(f).unwrap(), req_f);
    }

    #[test]
    fn parse_req_novers() {
        let actual1 = Req::from_str("saturn", false).unwrap();
        let actual2 = Req::from_str("saturn", true).unwrap();
        let expected = Req::new("saturn".into(), vec![]);
        assert_eq!(actual1, expected);
        assert_eq!(actual2, expected);
    }

    #[test]
    fn parse_req_pypi_w_extras() {
        // tod: Make this handle extras.
        let actual = Req::from_str("pyOpenSSL (>=0.14) ; extra == 'security'", true).unwrap();
        let expected = Req {
            name: "pyOpenSSL".into(),
            constraints: vec![Constraint {
                type_: Gte,
                major: 0,
                minor: Some(14),
                patch: None,
            }],
            extra: Some("security".into()),
            sys_platform: None,
            python_version: None,
        };

        let actual2 = Req::from_str(
            "pathlib2; extra == \"test\" and ( python_version == \"2.7\")",
            true,
        )
        .unwrap();

        let expected2 = Req {
            name: "pathlib2".into(),
            constraints: vec![],
            extra: Some("test".into()),
            sys_platform: None,
            python_version: Some(Constraint {
                type_: Exact,
                major: 2,
                minor: Some(7),
                patch: None,
            }),
        };

        let actual3 = Req::from_str(
            "win-unicode-console (>=0.5) ; sys_platform == \"win32\" and python_version < \"3.6\"",
            true,
        )
        .unwrap();

        let expected3 = Req {
            name: "win-unicode-console".into(),
            constraints: vec![Constraint {
                type_: Gte,
                major: 0,
                minor: Some(5),
                patch: None,
            }],
            extra: None,
            sys_platform: Some((Exact, crate::Os::Windows32)),
            python_version: Some(Constraint {
                type_: Lt,
                major: 3,
                minor: Some(6),
                patch: None,
            }),
        };

        assert_eq!(actual, expected);
        assert_eq!(actual2, expected2);
        assert_eq!(actual3, expected3);
    }

    #[test]
    fn parse_req_withvers() {
        let p = Req::from_str("bolt = \"3.1.4\"", false).unwrap();
        assert_eq!(
            p,
            Req::new(
                "bolt".into(),
                vec![Constraint {
                    major: 3,
                    minor: Some(1),
                    patch: Some(4),
                    type_: Exact,
                }]
            )
        )
    }

    #[test]
    fn parse_req_caret() {
        let p = Req::from_str("chord = \"^2.7.18\"", false).unwrap();
        assert_eq!(
            p,
            Req::new(
                "chord".into(),
                vec![Constraint {
                    major: 2,
                    minor: Some(7),
                    patch: Some(18),
                    type_: Caret,
                }]
            )
        )
    }

    #[test]
    fn parse_req_tilde_short() {
        let p = Req::from_str("sphere = \"~6.7\"", false).unwrap();
        assert_eq!(
            p,
            Req::new(
                "sphere".into(),
                vec![Constraint {
                    major: 6,
                    minor: Some(7),
                    patch: None,
                    type_: Tilde,
                }]
            )
        )
    }

    #[test]
    fn parse_req_pip() {
        let p = Req::from_pip_str("Django>=2.22").unwrap();
        assert_eq!(
            p,
            Req::new(
                "Django".into(),
                vec![Constraint {
                    major: 2,
                    minor: Some(22),
                    patch: None,
                    type_: Gte,
                }]
            )
        )
    }

    #[test]
    fn parse_req_pypi() {
        let p = Req::from_str("pytz (>=2016.3)", true).unwrap();
        assert_eq!(
            p,
            Req::new(
                "pytz".into(),
                vec![Constraint {
                    type_: Gte,
                    major: 2016,
                    minor: Some(3),
                    patch: None,
                }]
            )
        )
    }

    #[test]
    fn parse_req_pypi_dot() {
        let a = Req::from_str("zc.lockfile (>=0.2.3)", true).unwrap();
        let b = Req::from_str("zc.lockfile", true).unwrap();

        assert_eq!(
            a,
            Req::new(
                "zc.lockfile".into(),
                vec![Constraint {
                    type_: Gte,
                    major: 0,
                    minor: Some(2),
                    patch: Some(3),
                }]
            )
        );
        assert_eq!(b, Req::new("zc.lockfile".into(), vec![]));
    }

    #[test]
    fn parse_req_pypi_cplx() {
        let p = Req::from_str("urllib3 (!=1.25.0,!=1.25.1,<=1.26)", true).unwrap();
        assert_eq!(
            p,
            Req::new(
                "urllib3".into(),
                vec![
                    Constraint::new(Ne, 1, 25, 0),
                    Constraint::new(Ne, 1, 25, 1),
                    Constraint {
                        major: 1,
                        minor: Some(26),
                        patch: None,
                        type_: Lte,
                    }
                ]
            )
        )
    }

    #[test]
    fn req_tostring_single_reqs() {
        // todo: Expand this with more cases

        let a = Req::new("package".to_string(), vec![Constraint::new(Exact, 3, 3, 6)]);

        //        assert_eq!(a._to_pip_string(), "package==3.3.6".to_string());
        assert_eq!(a.to_cfg_string(), r#"package = "3.3.6""#.to_string());
    }

    #[test]
    fn req_tostring_multiple_reqs() {
        // todo: Expand this with more cases

        let a = Req::new(
            "package".to_string(),
            vec![
                Constraint::new(Ne, 2, 7, 4),
                Constraint {
                    major: 3,
                    minor: Some(7),
                    patch: None,
                    type_: Gte,
                },
            ],
        );

        //        assert_eq!(a._to_pip_string(), "'package!=2.7.4,>=3.7'".to_string());
        assert_eq!(
            a.to_cfg_string(),
            r#"package = "!=2.7.4, >=3.7""#.to_string()
        );
    }

    #[test]
    fn version_ordering() {
        let a = Version::new(4, 9, 4);
        let b = Version::new(4, 8, 0);
        let c = Version::new(3, 3, 6);
        let d = Version::new(3, 3, 5);
        let e = Version::new(3, 3, 0);
        let f = Version::new(1, 9, 9);
        let g = Version::new(2, 0, 0);

        assert!(a > b && b > c && c > d && d > e);
        assert!(f < g);
    }

    #[test]
    fn compat_rng() {
        let actual1 = Constraint::new(Gte, 5, 1, 0).compatible_range();
        let expected1 = vec![(Version::new(5, 1, 0), Version::_max())];

        let actual2 = Constraint::new(Ne, 5, 1, 3).compatible_range();
        let expected2 = vec![
            (Version::new(0, 0, 0), Version::new(5, 1, 2)),
            (Version::new(5, 1, 4), Version::_max()),
        ];
        assert_eq!(expected1, actual1);
        assert_eq!(expected2, actual2);
    }

    #[test]
    fn intersections_empty() {
        let reqs1 = vec![Constraint::new(Exact, 4, 9, 4)];
        let reqs2 = vec![Constraint::new(Gte, 4, 9, 7)];

        let reqs3 = vec![Constraint::new(Lte, 4, 9, 6)];
        let reqs4 = vec![Constraint::new(Gte, 4, 9, 7)];

        assert!(intersection_many(&[reqs1, reqs2]).is_empty());
        assert!(intersection_many(&[reqs3, reqs4]).is_empty());
    }

    #[test]
    fn intersections_simple() {
        let reqs1 = (Version::new(4, 9, 4), Version::new(MAX_VER, 0, 0));
        let reqs2 = (Version::new(4, 3, 1), Version::new(MAX_VER, 0, 0));

        let reqs3 = (Version::new(3, 0, 0), Version::new(3, 9, 0));
        let reqs4 = (Version::new(3, 3, 6), Version::new(3, 3, 6));

        assert_eq!(
            intersection(&[reqs1], &[reqs2]),
            vec![(Version::new(4, 9, 4), Version::_max())]
        );
        assert_eq!(
            intersection(&[reqs3], &[reqs4]),
            vec![(Version::new(3, 3, 6), Version::new(3, 3, 6))]
        );
    }

    #[test]
    // todo: Test many with more than 2 sets.
    fn intersections_simple_many() {
        let reqs1 = vec![Constraint::new(Gte, 4, 9, 4)];
        let reqs2 = vec![Constraint::new(Gte, 4, 3, 1)];

        let reqs3 = vec![Constraint::new(Caret, 3, 0, 0)];
        let reqs4 = vec![Constraint::new(Exact, 3, 3, 6)];

        assert_eq!(
            intersection_many(&[reqs1, reqs2]),
            vec![(Version::new(4, 9, 4), Version::_max())]
        );
        assert_eq!(
            intersection_many(&[reqs3, reqs4]),
            vec![(Version::new(3, 3, 6), Version::new(3, 3, 6))]
        );
    }

    #[test]
    fn intersection_contained() {
        let rng1 = (Version::new(4, 9, 2), Version::_max());
        let rng2 = (Version::new(4, 9, 4), Version::new(5, 5, 4));

        assert_eq!(
            intersection(&[rng1], &[rng2]),
            vec![(Version::new(4, 9, 4), Version::new(5, 5, 4))]
        );
    }

    #[test]
    fn intersection_contained_many() {
        let reqs1 = vec![Constraint::new(Gte, 4, 9, 2)];
        let reqs2 = vec![Constraint::new(Gte, 4, 9, 4), Constraint::new(Lt, 5, 5, 5)];

        assert_eq!(
            intersection_many(&[reqs1, reqs2]),
            vec![(Version::new(4, 9, 4), Version::new(5, 5, 4))]
        );
    }
    //    #[test]
    //    fn intersection_contained_many_more() {
    //        let reqs1 = vec![Constraint::new(Gte, 4, 9, 2)];
    //        let reqs2 = vec![Constraint::new(Gte, 4, 9, 4), Constraint::new(Lt, 5, 5, 5)];
    //        let reqs3 = vec![Constraint::new(Gte, 4, 9, 4), Constraint::new(Lt, 5, 5, 5)];
    //
    //        assert_eq!(
    //            intersection_many(&[reqs1, reqs2, reqs3]),
    //            vec![(Version::new(4, 9, 4), Version::new(5, 5, 4))]
    //        );
    //    }
}
