use std::{
    cmp,
    collections::HashMap,
    error::Error,
    fmt,
    hash::{Hash, Hasher},
    io::Write,
    num,
    str::FromStr,
};

use nom::combinator::all_consuming;
use serde::{Deserialize, Serialize};
use termcolor::{Buffer, BufferWriter, Color, ColorSpec, WriteColor};

// #[mockall_double::double]
use crate::dep_resolution::res;
use crate::{
    dep_parser::{
        parse_constraint, parse_pip_str, parse_req, parse_req_pypi_fmt, parse_version,
        parse_wh_py_vers,
    },
    dep_resolution::WarehouseRelease,
    util, CliConfig,
};

pub const MAX_VER: u32 = 999_999; // Represents the highest major version we can have

#[derive(Clone, Debug, PartialEq)]
pub struct Dependency {
    pub id: u32,
    pub name: String,
    pub version: Version,
    pub reqs: Vec<Req>,
    // Identify what constraints drove this, and by what package name/version.
    // The latter is so we know which package to mangle the inputs for, if
    // we need to rename this one.
    pub parent: u32, // id
}

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
            details: "Parse int error".into(),
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

/// Ideally we won't deal with these much, but include for compatibility.
#[derive(Debug, Clone, Deserialize, Eq, Hash, PartialEq)]
pub enum VersionModifier {
    Alpha,
    Beta,
    ReleaseCandidate,
    Dep, // todo: Not sure what this is, but have found it.
    // Used to allow comparisons between versions that have and don't have modifiers.
    Null,
    Other(String),
}

impl FromStr for VersionModifier {
    type Err = DependencyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let result = match s {
            "a" => Self::Alpha,
            "b" => Self::Beta,
            "rc" => Self::ReleaseCandidate,
            "dep" => Self::Dep,
            //_ => return Err(DependencyError::new("Problem parsing version modifier")),
            _x => Self::Other(s.to_string()),
        };
        Ok(result)
    }
}

impl ToString for VersionModifier {
    fn to_string(&self) -> String {
        match self {
            Self::Alpha => "a".into(),
            Self::Beta => "b".into(),
            Self::ReleaseCandidate => "rc".into(),
            Self::Dep => "dep".into(),
            Self::Null => panic!("Can't convert Null to string; misused"),
            Self::Other(x) => x.into(),
        }
    }
}

impl VersionModifier {
    fn orderval(self) -> u8 {
        match self {
            Self::Null => 5,
            Self::ReleaseCandidate => 4,
            Self::Beta => 3,
            Self::Alpha => 2,
            Self::Dep => 1,
            Self::Other(_x) => 0,
        }
    }
}

impl Ord for VersionModifier {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.clone().orderval().cmp(&other.clone().orderval())
    }
}

impl PartialOrd for VersionModifier {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// An exact, 3-number Semver version. With some possible extras.
#[derive(Clone, Default, Deserialize, Eq)]
pub struct Version {
    pub major: Option<u32>,
    pub minor: Option<u32>,
    pub patch: Option<u32>,
    pub extra_num: Option<u32>,                   // eg 4.2.3.1
    pub modifier: Option<(VersionModifier, u32)>, // eg a1
    /// if `true` the star goes in the first `None` slot. Remaining slots should be `None`
    pub star: bool,
}

impl Version {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major: Some(major),
            minor: Some(minor),
            patch: Some(patch),
            extra_num: None,
            modifier: None,
            star: false,
        }
    }

    pub const fn new_any() -> Self {
        Self {
            major: None,
            minor: None,
            patch: None,
            extra_num: None,
            modifier: None,
            star: true,
        }
    }

    /// No patch specified.
    pub const fn new_short(major: u32, minor: u32) -> Self {
        Self {
            major: Some(major),
            minor: Some(minor),
            patch: None,
            extra_num: None,
            modifier: None,
            star: false,
        }
    }

    pub const fn new_opt(major: Option<u32>, minor: Option<u32>, patch: Option<u32>) -> Self {
        Self {
            major,
            minor,
            patch,
            extra_num: None,
            modifier: None,
            star: false,
        }
    }

    /// create new version with star option
    pub const fn new_star(
        major: Option<u32>,
        minor: Option<u32>,
        patch: Option<u32>,
        star: bool,
    ) -> Self {
        Self {
            major,
            minor,
            patch,
            extra_num: None,
            modifier: None,
            star,
        }
    }

    /// Create new Version of self with `star` set to false
    pub fn new_unstar(&self) -> Self {
        Self {
            major: Some(self.major.unwrap_or(0)),
            minor: Some(self.minor.unwrap_or(0)),
            patch: Some(self.patch.unwrap_or(0)),
            extra_num: self.extra_num,
            modifier: self.modifier.clone(),
            star: false,
        }
    }

    pub const fn _max() -> Self {
        Self::new_opt(Some(MAX_VER), None, None)
    }

    /// Prevents repetition.
    fn add_str_mod(&self, s: &mut String) {
        if let Some(extra_num) = self.extra_num {
            s.push_str(&format!(".{}", extra_num.to_string()));
        }
        if let Some((modifier, num)) = self.modifier.clone() {
            s.push_str(&format!("{}{}", modifier.to_string(), num.to_string()));
        }
    }

    pub fn to_string_med(&self) -> String {
        let mut result = format!("{}.{}", self.major.unwrap_or(0), self.minor.unwrap_or(0));
        self.add_str_mod(&mut result);
        result
    }
    pub fn to_string_short(&self) -> String {
        let mut result = format!("{}", self.major.unwrap_or(0));
        self.add_str_mod(&mut result);
        result
    }

    /// unlike Display, which overwrites to_string, don't add colors.
    pub fn to_string_no_patch(&self) -> String {
        let mut result = format!("{}.{}", self.major.unwrap_or(0), self.minor.unwrap_or(0));
        self.add_str_mod(&mut result);
        result
    }

    pub fn to_string_color(&self) -> String {
        self.colorize().unwrap_or_else(|_| self.to_string())
    }

    fn colorize(&self) -> anyhow::Result<String> {
        let bufwtr = BufferWriter::stdout(CliConfig::current().color_choice);
        let mut buf: Buffer = bufwtr.buffer();
        let num_c = Some(Color::Blue);
        let dot_c = Some(Color::Yellow); // Dark

        let mut suffix = "".to_string();
        if let Some(num) = self.extra_num {
            suffix.push('.');
            suffix.push_str(&num.to_string());
        }
        if let Some((modifier, num)) = self.modifier.clone() {
            suffix.push_str(&modifier.to_string());
            suffix.push_str(&num.to_string());
        }
        buf.set_color(ColorSpec::new().set_fg(num_c))?;
        write!(buf, "{}", self.major.unwrap_or(0))?;
        if let Some(x) = self.minor {
            buf.set_color(ColorSpec::new().set_fg(dot_c))?;
            write!(buf, ".")?;
            buf.set_color(ColorSpec::new().set_fg(num_c))?;
            write!(buf, "{}", x)?;
        }
        if let Some(x) = self.patch {
            buf.set_color(ColorSpec::new().set_fg(dot_c))?;
            write!(buf, ".")?;
            buf.set_color(ColorSpec::new().set_fg(num_c))?;
            write!(buf, "{}", x)?;
        }
        write!(buf, "{}", suffix)?;
        buf.reset()?;

        Ok(String::from_utf8_lossy(buf.as_slice()).to_string())
    }
}

impl FromStr for Version {
    type Err = DependencyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        all_consuming(parse_version)(s)
            .map_err(|_| DependencyError::new(&format!("Problem parsing version: {}", s)))
            .map(|(_, v)| v)
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        // None version modifiers should rank highest. Ie 17.0 > 17.0rc1
        let self_mod = self.modifier.clone().unwrap_or((VersionModifier::Null, 0));
        let other_mod = other.modifier.clone().unwrap_or((VersionModifier::Null, 0));
        // Mirror the set field if we have a star present
        let cmp_star = |obj: Option<u32>, oth: Option<u32>, star: bool| -> cmp::Ordering {
            let none_val = if star {
                if obj.is_none() && oth.is_none() {
                    0
                } else if let Some(x) = obj {
                    x
                } else {
                    oth.unwrap_or(0)
                }
            } else {
                0
            };
            obj.unwrap_or(none_val).cmp(&oth.unwrap_or(none_val))
        };
        let star = self.star || other.star;
        let maj = cmp_star(self.major, other.major, star);
        let min = cmp_star(self.minor, other.minor, star);
        let pat = cmp_star(self.patch, other.patch, star);
        let ext = cmp_star(self.extra_num, other.extra_num, star);
        if !matches!(maj, cmp::Ordering::Equal) {
            maj
        } else if !matches!(min, cmp::Ordering::Equal) {
            min
        } else if !matches!(pat, cmp::Ordering::Equal) {
            pat
        } else if !matches!(ext, cmp::Ordering::Equal) {
            ext
        } else if !star {
            if self_mod.0 == other_mod.0 {
                self_mod.1.cmp(&other_mod.1)
            } else {
                self_mod.0.cmp(&other_mod.0)
            }
        } else {
            cmp::Ordering::Equal
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        matches!(self.cmp(other), cmp::Ordering::Equal)
    }
}

impl Hash for Version {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.major.hash(state);
        self.minor.unwrap_or(0).hash(state);
        self.patch.unwrap_or(0).hash(state);
        self.extra_num.unwrap_or(0).hash(state);
        self.modifier
            .clone()
            .unwrap_or((VersionModifier::Null, 0))
            .hash(state);
        self.star.hash(state);
    }
}

//impl ToString for Version {
//    fn to_string(&self) -> String {
//        format!("{}.{}.{}", self.major, self.minor, self.patch)
//    }
//}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut version = if let Some(x) = self.major {
            x.to_string()
        } else {
            "*".to_string()
        };
        if self.major.is_some() {
            let mut star_handled = false;
            let parts = vec![self.minor, self.patch, self.extra_num];
            for part in parts.iter() {
                if let Some(p) = part {
                    version.push('.');
                    version.push_str(&p.to_string());
                } else if self.star && !star_handled {
                    version.push_str(".*");
                    star_handled = true;
                    break;
                }
            }
            if !self.star || !star_handled {
                if let Some((modifier, num)) = self.modifier.clone() {
                    version.push_str(&modifier.to_string());
                    version.push_str(&num.to_string());
                }
            }
            if self.star && !star_handled {
                version.push('*');
            }
        }
        write!(f, "{}", version)
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
    TildeEq, // PEP440 ~= is different from ~
             // todo wildcard
}

impl ToString for ReqType {
    /// These show immediately before the version numbers
    fn to_string(&self) -> String {
        match self {
            Self::Exact => "==".into(),
            Self::Gte => ">=".into(),
            Self::Lte => "<=".into(),
            Self::Gt => ">".into(),
            Self::Lt => "<".into(),
            Self::Ne => "!=".into(),
            Self::Caret => "^".into(),
            Self::Tilde => "~".into(),
            Self::TildeEq => "~=".into(),
        }
    }
}

impl FromStr for ReqType {
    type Err = DependencyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "==" => Ok(Self::Exact),
            ">=" => Ok(Self::Gte),
            "<=" => Ok(Self::Lte),
            ">" => Ok(Self::Gt),
            "<" => Ok(Self::Lt),
            "!=" => Ok(Self::Ne),
            "^" => Ok(Self::Caret),
            "~" => Ok(Self::Tilde),
            "~=" => Ok(Self::TildeEq),
            _ => Err(DependencyError::new("Problem parsing ReqType")),
        }
    }
}

/// For holding semver-style version requirements with Caret, tilde etc
/// [Ref](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Constraint {
    pub type_: ReqType,
    pub version: Version,
}

impl FromStr for Constraint {
    type Err = DependencyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        all_consuming(parse_constraint)(s)
            .map_err(|_| DependencyError::new(&format!("Problem parsing constraint: {}", s)))
            .map(|(_, c)| c)
    }
}

/// A single version req. Can be chained together.
impl Constraint {
    pub const fn new(type_: ReqType, version: Version) -> Self {
        Self { type_, version }
    }

    pub const fn new_any() -> Self {
        Self {
            type_: ReqType::Exact,
            version: Version::new_any(),
        }
    }

    /// From a comma-separated list
    pub fn from_str_multiple(vers: &str) -> Result<Vec<Self>, DependencyError> {
        let mut result = vec![];
        let vers = vers.replace(" ", "");
        let vers = if vers.is_empty() {
            ">=2.0".to_string()
        } else {
            vers
        };

        for req in vers.split(',') {
            match Self::from_str(req) {
                Ok(r) => result.push(r),
                Err(e) => return Err(e),
            }
        }
        Ok(result)
    }

    /// ie cp37, a version from Pypi. Eg: "py3", "cp35.cp36.cp37.cp38", "cp26", "py2.py3",
    /// "pp36", "any", "2.7",
    /// Note that we're parsing the `python_version` field, not `requires_python`, since the latter
    /// May throw false-positives for compatibility.
    /// Important: The result is intended to be used in an "any" way. Ie "cp35.36" should match
    /// either Python 3.5 or 3.6.
    pub fn from_wh_py_vers(s: &str) -> Result<Vec<Self>, DependencyError> {
        all_consuming(parse_wh_py_vers)(s)
            .map_err(|_| DependencyError::new(&format!("Problem parsing wh_py_vers: {}", s)))
            .map(|(_, vs)| vs)
    }

    /// Called `to_string2` to avoid shadowing `Display`
    pub fn to_string2(&self, ommit_equals: bool, pip_style: bool) -> String {
        // ommit_equals indicates we don't want to add any type if it's exact. Eg in config files.
        // pip_style means that ^ is transformed to ^=, and ~ to ~=
        let mut type_str = if ommit_equals && self.type_ == ReqType::Exact {
            "".to_string()
        } else {
            self.type_.to_string()
        };
        if pip_style {
            match self.type_ {
                ReqType::Caret | ReqType::Tilde => type_str.push('='),
                _ => (),
            }
        }
        format!("{}{}", type_str, self.version.to_string())
    }

    /// Find the lowest and highest compatible versions. Return a vec, since the != requirement type
    /// has two ranges.
    pub fn compatible_range(&self) -> Vec<(Version, Version)> {
        let highest = Version::_max();
        let lowest = Version::new(0, 0, 0);
        let max;

        let safely_subtract = |major: Option<u32>, minor: Option<u32>, patch: Option<u32>| {
            let mut major = major.unwrap_or(0);
            let mut minor = minor.unwrap_or(0);
            let mut patch = patch.unwrap_or(0);
            // Don't try to make a negative version component.
            // ie 0.0.0. Return max of 0.0.0
            if major == 0 && minor == 0 && patch == 0 {
            }
            // ie 3.0.0. Return max of 2.999999.999999
            else if minor == 0 && patch == 0 {
                major -= 1;
                minor = MAX_VER;
                patch = MAX_VER;
            // ie 2.9.0. Return max of 2.8.999999
            } else if patch == 0 {
                minor -= 1;
                patch = MAX_VER
            } else {
                patch -= 1;
            }
            (major, minor, patch)
        };

        // Note that other than for not-equals, the the resulting Vec has len 1.
        match self.type_ {
            ReqType::Exact => vec![(self.version.new_unstar(), self.get_max_version())],
            ReqType::Gte => vec![(self.version.new_unstar(), highest)],
            ReqType::Lte => vec![(lowest, self.version.new_unstar())],
            ReqType::Gt => vec![(
                Version::new(
                    self.version.major.unwrap_or(0),
                    self.version.minor.unwrap_or(0),
                    self.version.patch.unwrap_or(0) + 1,
                ),
                highest,
            )],
            ReqType::Lt => {
                let (major, minor, patch) =
                    safely_subtract(self.version.major, self.version.minor, self.version.patch);
                vec![(lowest, Version::new(major, minor, patch))]
            }
            ReqType::Ne => {
                let (major, minor, patch) =
                    safely_subtract(self.version.major, self.version.minor, self.version.patch);
                vec![
                    (lowest, Version::new(major, minor, patch)),
                    (
                        Version::new(
                            self.version.major.unwrap_or(0),
                            self.version.minor.unwrap_or(0),
                            self.version.patch.unwrap_or(0) + 1,
                        ),
                        highest,
                    ),
                ]
            }
            ReqType::Caret => {
                max = self.get_max_version();
                // We need to use Lt logic for ^ and ~.
                let (major, minor, patch) = safely_subtract(max.major, max.minor, max.patch);
                vec![(self.version.clone(), Version::new(major, minor, patch))]
            }
            ReqType::Tilde => {
                max = self.get_max_version();
                let (major, minor, patch) = safely_subtract(max.major, max.minor, max.patch);
                vec![(self.version.clone(), Version::new(major, minor, patch))]
            }
            ReqType::TildeEq => {
                max = self.get_max_version();
                let (major, minor, patch) = safely_subtract(max.major, max.minor, max.patch);
                vec![(self.version.clone(), Version::new(major, minor, patch))]
            }
        }
    }

    pub fn is_compatible(&self, version: &Version) -> bool {
        let min = self.version.clone();
        let max;

        match self.type_ {
            ReqType::Exact => {
                if !self.version.star && !version.star {
                    self.version == *version
                } else {
                    max = self.get_max_version();
                    min <= *version && *version <= max
                }
            }
            ReqType::Gte => self.version <= *version,
            ReqType::Lte => self.version >= *version,
            ReqType::Gt => self.version < *version,
            ReqType::Lt => self.version > *version,
            ReqType::Ne => self.version != *version,
            ReqType::Caret => {
                max = self.get_max_version();
                min <= *version && *version < max
            }
            // For tilde, if minor's specified, can only increment patch.
            // If not, can increment minor or patch.
            ReqType::Tilde => {
                max = self.get_max_version();
                min <= *version && *version < max
            }

            ReqType::TildeEq => {
                max = self.get_max_version();
                min <= *version && *version < max
            }
        }
    }

    /// This internal function is to DRY Caret and Tilde max versions
    fn get_max_version(&self) -> Version {
        match self.type_ {
            ReqType::Exact => {
                if self.version.star {
                    if self.version.major.is_none() {
                        Version::new_star(Some(MAX_VER), Some(MAX_VER), Some(MAX_VER), true)
                    } else if self.version.minor.is_none() {
                        Version::new_star(self.version.major, Some(MAX_VER), Some(MAX_VER), true)
                    } else if self.version.patch.is_none() {
                        Version::new_star(
                            self.version.major,
                            self.version.minor,
                            Some(MAX_VER),
                            true,
                        )
                    } else {
                        self.version.clone()
                    }
                } else {
                    self.version.clone()
                }
            }
            ReqType::Caret => {
                if self.version.major.unwrap_or(0) > 0 {
                    Version::new(self.version.major.unwrap_or(0) + 1, 0, 0)
                } else if self.version.minor > Some(0) {
                    Version::new(0, self.version.minor.unwrap() + 1, 0)
                } else {
                    Version::new(0, 0, self.version.patch.unwrap_or(0) + 1)
                }
            }
            // For tilde, if minor's specified, can only increment patch.
            // If not, can increment minor or patch.
            ReqType::Tilde => {
                if let Some(x) = self.version.minor {
                    Version::new(self.version.major.unwrap_or(0), x + 1, 0)
                } else {
                    Version::new(self.version.major.unwrap_or(0) + 1, 0, 0)
                }
            }
            /*
            https://www.python.org/dev/peps/pep-0440/#compatible-release
            This operator MUST NOT be used with a single segment version number such as ~=1.
            For example, the following groups of version clauses are equivalent:

            ~= 2.2
            >= 2.2, == 2.*

            ~= 1.4.5
            >= 1.4.5, == 1.4.*

             */
            ReqType::TildeEq => {
                if self.version.patch.is_some() {
                    Version::new(
                        self.version.major.unwrap_or(0),
                        self.version.minor.unwrap_or(0) + 1,
                        0,
                    )
                } else if self.version.minor.is_some() {
                    Version::new(self.version.major.unwrap_or(0) + 1, 0, 0)
                } else {
                    panic!("Invalid `~=` constraint for {:?}", self.version);
                }
            }
            // Not sure we would ever actually use this with other types. So
            // just return a clone
            _ => self.version.clone(),
        }
    }
}

impl fmt::Display for Constraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.type_.to_string(), self.version)
    }
}

pub fn intersection_many(constrs: &[Constraint]) -> Vec<(Version, Version)> {
    // And logic between constraints. We use a range to account for Ne logic, which
    // may result in more than one compatible range.
    // Result is or logic.

    // We must take into account that a `Ne` constraint has two ranges joined with `or` logic.
    let mut nes = vec![];

    let mut ranges = vec![];
    for constr in constrs.iter() {
        // rngs will be len 2 for Ne, else 1. Or logic within rngs.
        let rng = &constr.compatible_range();
        let rng2;
        if let ReqType::Ne = constr.type_ {
            rng2 = (Version::new(0, 0, 0), Version::_max());
            // We'll remove nes at the end.
            nes.push(constr.version.clone());
        } else {
            rng2 = rng[0].clone(); // If not Ne, there will be exactly 1.
        }
        ranges.push(rng2);
    }
    // todo: We haven't included nes!
    intersection_many2(&ranges)
}

/// Interface an arbitrary number of constraint sets into the intersection fn(s), which
/// handle 2 at a time.
fn intersection_many2(reqs: &[(Version, Version)]) -> Vec<(Version, Version)> {
    // todo: Broken for notequals, which involves joining two ranges with OR logic.
    let init = vec![(Version::new(0, 0, 0), Version::new(MAX_VER, 0, 0))];
    reqs.iter().fold(init, |acc, constraint_set| {
        intersection(&[constraint_set.clone()], &acc)
    })
}

/// Find the intersection of two sets of version requirements. Uses `and` logic for everything.
/// Result is a Vec of (min, max) tuples.
pub fn intersection(
    ranges1: &[(Version, Version)],
    ranges2: &[(Version, Version)],
) -> Vec<(Version, Version)> {
    let mut result = vec![];
    // Each range imposes an additional constraint.
    for rng1 in ranges1 {
        for rng2 in ranges2 {
            // 0 is min, 1 is max.
            if rng2.1 >= rng1.0 && rng1.1 >= rng2.0 {
                result.push((
                    cmp::max(rng2.0.clone(), rng1.0.clone()),
                    cmp::min(rng1.1.clone(), rng2.1.clone()),
                ));
            }
        }
    }
    result
}

#[derive(Clone, Debug, PartialEq)]
pub struct Extras {
    pub extra: Option<String>,
    pub sys_platform: Option<(ReqType, util::Os)>,
    pub python_version: Option<Constraint>,
}

impl Extras {
    pub const fn new_py(python_version: Constraint) -> Self {
        Self {
            extra: None,
            sys_platform: None,
            python_version: Some(python_version),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Req {
    pub name: String,
    pub constraints: Vec<Constraint>,
    pub extra: Option<String>,
    pub sys_platform: Option<(ReqType, util::Os)>,
    pub python_version: Option<Vec<Constraint>>,
    pub install_with_extras: Option<Vec<String>>,
    pub path: Option<String>,
    pub git: Option<String>, // String is the git repo. // todo: Branch
}

impl Req {
    pub const fn new(name: String, constraints: Vec<Constraint>) -> Self {
        Self {
            name,
            constraints,
            extra: None,
            sys_platform: None,
            python_version: None,
            install_with_extras: None,
            path: None,
            git: None,
        }
    }

    pub fn new_with_extras(name: String, constraints: Vec<Constraint>, extras: Extras) -> Self {
        Self {
            name,
            constraints,
            extra: extras.extra,
            sys_platform: extras.sys_platform,
            python_version: extras.python_version.map(|x| vec![x]),
            install_with_extras: None,
            path: None,
            git: None,
        }
    }

    pub fn from_str(s: &str, pypi_fmt: bool) -> Result<Self, DependencyError> {
        (if pypi_fmt {
            all_consuming(parse_req_pypi_fmt)(s)
        } else {
            all_consuming(parse_req)(s)
        })
        .map_err(|_| DependencyError::new(&format!("Problem parsing version requirement: {}", s)))
        .map(|x| x.1)
    }

    /// We use this for parsing requirements.txt.
    pub fn from_pip_str(s: &str) -> Option<Self> {
        // todo multiple ie single quotes support?
        all_consuming(parse_pip_str)(s).ok().map(|x| x.1)
    }

    pub fn from_warehouse_release(
        name: String,
        version: String,
        release: WarehouseRelease,
    ) -> Self {
        let ver = Version::from_str(&version)
            .ok()
            .unwrap_or_else(|| Version::new_star(None, None, None, true));
        let constraint = Constraint::new(ReqType::Exact, ver);
        let py_ver = Constraint::from_wh_py_vers(&release.python_version);
        let requires = if let Some(x) = release.requires_python {
            if x.as_str() > "" {
                if let Ok(c) = Constraint::from_str_multiple(&x) {
                    Some(c)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        let py_req = requires.unwrap_or_else(|| py_ver.unwrap());

        Self {
            name,
            constraints: vec![constraint],
            extra: None,
            sys_platform: None,
            python_version: Some(py_req),
            install_with_extras: None,
            path: None,
            git: None,
        }
    }

    /// Clone the Req but set a python requirement if python_version.is_none()
    pub fn clone_or_default_py(&self, python_version: &Version) -> Self {
        Self {
            name: self.name.clone(),
            constraints: self.constraints.clone(),
            extra: self.extra.clone(),
            sys_platform: self.sys_platform,
            python_version: if let Some(ref pv) = self.python_version {
                Some(pv.clone())
            } else {
                Some(vec![Constraint::new(ReqType::Gte, python_version.clone())])
            },
            install_with_extras: self.install_with_extras.clone(),
            path: self.path.clone(),
            git: self.path.clone(),
        }
    }

    /// eg `saturn = "^0.3.1"` or `matplotlib = "3.1.1"`
    pub fn to_cfg_string(&self) -> String {
        match self.constraints.len() {
            0 => {
                let (name, latest_version) = if let Ok((fmtd_name, version, _)) =
                    res::get_version_info(
                        &self.name,
                        Some(Req::new_with_extras(
                            self.name.clone(),
                            vec![Constraint::new_any()],
                            Extras::new_py(Constraint::new(
                                ReqType::Exact,
                                self.py_ver_or_default(),
                            )),
                        )),
                    ) {
                    (fmtd_name, version)
                } else {
                    util::abort(&format!("Unable to find version info for {:?}", &self.name));
                };
                format!(
                    r#"{} = "{}""#,
                    name,
                    Constraint::new(ReqType::Caret, latest_version).to_string2(true, false)
                )
            }
            _ => format!(
                r#"{} = "{}""#,
                self.name,
                self.constraints
                    .iter()
                    .map(|r| r.to_string2(true, false))
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
        }
    }

    pub fn py_ver_or_default(&self) -> Version {
        let default = vec![Constraint::from_str("==*").ok().unwrap()];
        self.python_version
            .as_ref()
            .unwrap_or(&default)
            .first()
            .unwrap()
            .version
            .clone()
    }

    /// Format for setup.py
    pub fn to_setup_py_string(&self) -> String {
        format!(
            "{}{}",
            self.name,
            self.constraints
                .iter()
                .map(|c| c.to_string2(false, true))
                .collect::<Vec<String>>()
                .join(",")
        )
        .replace("^", ">")
        .replace("~", ">") // todo: Sloppy, but perhaps the best way.
    }
}

impl fmt::Display for Req {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut constraints = "".to_string();
        for constr in &self.constraints {
            constraints.push_str(&format!("{}", constr));
        }
        let bufwtr = BufferWriter::stdout(CliConfig::current().color_choice);
        let mut buf = bufwtr.buffer();
        if let Err(_e) = buf.set_color(ColorSpec::new().set_fg(Some(Color::Cyan))) {
            // Dark
            panic!("An Error occurred formatting Req")
        }
        if let Err(_e) = write!(buf, "{} {}", self.name, constraints) {
            panic!("An Error occurred formatting Req")
        }
        if let Err(_e) = buf.reset() {
            panic!("An Error occurred formatting Req")
        }
        write!(f, "{}", String::from_utf8_lossy(buf.as_slice()))
    }
}

#[derive(Clone, Debug)]
pub enum Rename {
    No,
    // todo: May not need to store self id.
    Yes(u32, u32, String), // parent id, self id, name
}

#[derive(Clone, Debug)]
pub struct Package {
    pub id: u32,
    pub parent: u32,
    pub name: String,
    pub version: Version,
    pub deps: Vec<(u32, String, Version)>,
    pub rename: Rename,
}

/// Similar to that used by Cargo.lock. Represents an exact package to download. // todo(Although
/// todo the dependencies field isn't part of that/?)
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LockPackage {
    // We use Strings here instead of types like Version to make it easier to
    // serialize and deserialize
    // todo: We have an analog Package type; perhaps just figure out how to serialize that.
    pub id: u32, // used for tracking renames
    pub name: String,
    pub version: String,
    pub source: Option<String>,
    pub dependencies: Option<Vec<String>>,
    pub rename: Option<String>,
}

/// Modelled after [Cargo.lock](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html)
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Lock {
    pub package: Option<Vec<LockPackage>>,
    //    pub metadata: Option<Vec<String>>, // ie checksums
    pub metadata: HashMap<String, String>, // ie checksums
}

#[cfg(test)]
pub mod tests {
    use rstest::rstest;
    use ReqType::*;
    use VersionModifier::*;

    use super::*;

    #[rstest(
        req,
        max_compat,
        not_compat,
        case::caret_full_version(
            Constraint::new(Caret, Version::new(1, 2, 3)),
            Version::new(1, 9, MAX_VER),
            Version::new(2, 0, 0)
        ),
        case::caret_minor_version(
            Constraint::new(Caret, Version::new(0, 2, 3)),
            Version::new(0, 2, MAX_VER),
            Version::new(0, 3, 0)
        ),
        case::caret_patch_version_is_exact_over(
            Constraint::new(Caret, Version::new(0, 0, 3)),
            Version::new(0, 0, 3),
            Version::new(0, 0, 4)
        ),
        case::caret_patch_version_is_exact_under(
            Constraint::new(Caret, Version::new(0, 0, 3)),
            Version::new(0, 0, 3),
            Version::new(0, 0, 2)
        ),
        case::gte_max(
            Constraint::new(Gte, Version::new(1, 2, 3)),
            Version::_max(),
            Version::new(1, 2, 2)
        ),
        case::gte_eq(
            Constraint::new(Gte, Version::new(1, 2, 3)),
            Version::new(1, 2, 3),
            Version::new(1, 2, 2)
        ),
        case::gte_misc(
            Constraint::new(Gte, Version::new(1, 2, 3)),
            Version::new(2, 1, 2),
            Version::new(1, 2, 2)
        ),
        case::gt_max(
            Constraint::new(Gt, Version::new(0, 2, 3)),
            Version::_max(),
            Version::new(0, 2, 3)
        ),
        case::gt_first(
            Constraint::new(Gt, Version::new(0, 2, 3)),
            Version::new(0, 2, 4),
            Version::new(0, 2, 3)
        ),
        case::exact_over(
            Constraint::new(Exact, Version::new(1, 2, 3)),
            Version::new(1, 2, 3),
            Version::new(1, 2, 4)
        ),
        case::exact_under(
            Constraint::new(Exact, Version::new(1, 2, 3)),
            Version::new(1, 2, 3),
            Version::new(1, 2, 2)
        ),
        case::ne_over(
            Constraint::new(Ne, Version::new(1, 2, 3)),
            Version::new(1, 2, 4),
            Version::new(1, 2, 3)
        ),
        case::ne_under(
            Constraint::new(Ne, Version::new(1, 2, 3)),
            Version::new(1, 2, 2),
            Version::new(1, 2, 3)
        ),
        case::lt_max(
            Constraint::new(Lt, Version::new(1, 2, 3)),
            Version::new(1, 2, 2),
            Version::new(1, 2, 3)
        ),
        case::lt_min(
            Constraint::new(Lt, Version::new(1, 2, 3)),
            Version::new(0, 0, 0),
            Version::new(1, 2, 3)
        ),
        case::lte_max(
            Constraint::new(Lte, Version::new(1, 2, 3)),
            Version::new(1, 2, 3),
            Version::new(1, 2, 4)
        ),
        case::lte_min(
            Constraint::new(Lte, Version::new(1, 2, 3)),
            Version::new(0, 0, 0),
            Version::new(1, 2, 4)
        ),
        case::tilde_full_version_max(
            Constraint::new(Tilde, Version::new(1, 2, 3)),
            Version::new(1, 2, MAX_VER),
            Version::new(1, 3, 0)
        ),
        case::tilde_full_version_min(
            Constraint::new(Tilde, Version::new(1, 2, 3)),
            Version::new(1, 2, MAX_VER),
            Version::new(1, 2, 2)
        ),
        case::tilde_minor_version_max(
            Constraint::new(Tilde, Version::new_short(1, 2)),
            Version::new(1, 2, MAX_VER),
            Version::new(1, 3, 0)
        ),
        case::tilde_minor_version_min(
            Constraint::new(Tilde, Version::new_short(1, 2)),
            Version::new(1, 2, MAX_VER),
            Version::new(1, 1, MAX_VER)
        ),
        case::tilde_major_version_max(
            Constraint::new(Tilde, Version::new_opt(Some(1), None, None)),
            Version::new(1, MAX_VER, MAX_VER),
            Version::new(2, 0, 0)
        ),
        case::tilde_major_version_under(
            Constraint::new(Tilde, Version::new_opt(Some(1), None, None)),
            Version::new(1, MAX_VER, MAX_VER),
            Version::new(0, MAX_VER, MAX_VER)
        ),
        case::tilde_eq_full_version_max(
            Constraint::new(TildeEq, Version::new(1, 2, 3)),
            Version::new(1, 2, MAX_VER),
            Version::new(1, 3, 0)
        ),
        case::tilde_eq_full_version_min(
            Constraint::new(TildeEq, Version::new(1, 2, 3)),
            Version::new(1, 2, MAX_VER),
            Version::new(1, 2, 2)
        ),
        case::tilde_eq_minor_version_max(
            Constraint::new(TildeEq, Version::new_short(1, 2)),
            Version::new(1, MAX_VER, MAX_VER),
            Version::new(2, 0, 0)
        ),
        case::tilde_eq_minor_version_min(
            Constraint::new(TildeEq, Version::new_short(1, 2)),
            Version::new(1, MAX_VER, MAX_VER),
            Version::new(1, 1, MAX_VER)
        ),
        #[should_panic]
        case::tilde_eq_major_version_max(
            Constraint::new(TildeEq, Version::new_opt(Some(1), None, None)),
            Version::new(1, MAX_VER, MAX_VER),
            Version::new(2, 0, 0)
        ),
        #[should_panic]
        case::tilde_eq_major_version_under(
            Constraint::new(TildeEq, Version::new_opt(Some(1), None, None)),
            Version::new(1, MAX_VER, MAX_VER),
            Version::new(0, MAX_VER, MAX_VER)
        ),
        #[should_panic(expected="assertion failed: !req.is_compatible(&not_compat)")]
        case::star_major_version(
            Constraint::new(Exact, Version::new_star(None, None, None, true)),
            Version{
                major: Some(1),
                minor: Some(2),
                patch: Some(3),
                extra_num: Some(MAX_VER),
                modifier: Some((VersionModifier::Beta, 1)),
                star: false,
            },
            Version::new_star(None, None, None, false)
        ),
        case::star_minor_version_min(
            Constraint::new(Exact, Version::new_star(Some(1), None, None, true)),
            Version::new(1, MAX_VER, MAX_VER),
            Version::new(0, MAX_VER, MAX_VER)
        ),
        case::star_minor_version_max(
            Constraint::new(Exact, Version::new_star(Some(1), None, None, true)),
            Version::new(1, MAX_VER, MAX_VER),
            Version::new(2, 0, 0)
        ),
        case::star_patch_version_min(
            Constraint::new(Exact, Version::new_star(Some(1), Some(2), None, true)),
            Version::new(1, 2, MAX_VER),
            Version::new(0, 1, MAX_VER)
        ),
        case::star_minor_version_max(
            Constraint::new(Exact, Version::new_star(Some(1), Some(2), None, true)),
            Version::new(1, 2, MAX_VER),
            Version::new(1, 3, 0)
        ), // TODO: Test below patch. Right now we don't check compatible below patch level
        case::star_extra_num_version_max(
            Constraint::new(Exact, Version{
                major: Some(1),
                minor: Some(2),
                patch: Some(3),
                extra_num: None,
                modifier: None,
                star:true}),
            Version{
                major: Some(1),
                minor: Some(2),
                patch: Some(3),
                extra_num: Some(MAX_VER),
                modifier: Some((VersionModifier::Beta, 1)),
                star: false,
            },
            Version::new(1, 3, 0)
        ),
    )]
    fn is_compatible(req: Constraint, max_compat: Version, not_compat: Version) {
        if !req.is_compatible(&max_compat) {
            eprintln!(
                "req: {:?}\nmax_compat: {:?}\ncompat range: {:?}",
                req,
                max_compat,
                req.compatible_range()
            );
        }
        if req.is_compatible(&not_compat) {
            eprintln!(
                "req: {:?}\nnot_compat: {:?}\ncompat range: {:?}",
                req,
                not_compat,
                req.compatible_range()
            );
        }
        assert!(req.is_compatible(&max_compat));
        assert!(!req.is_compatible(&not_compat));
    }

    #[rstest(
        ver_str,
        is_compat,
        case::exact("==1.1", true),
        case::zero_padded("==1.1.0", true),
        case::dev_release("==1.1.dev1", false),
        case::pre_release("==1.1a1", false),
        case::post_release("==1.1.post1", false),
        case::star("==1.1.*", true)
    )]
    fn pep440_bare(ver_str: &str, is_compat: bool) {
        let ver_match = Version::new_short(1, 1);
        let constraint = Constraint::from_str(ver_str).unwrap();
        assert_eq!(constraint.is_compatible(&ver_match), is_compat);
    }

    #[rstest(
        ver_str,
        is_compat,
        case::exact("==1.1.post1", true),
        case::bare("==1.1", false),
        case::star("==1.1.*", true)
    )]
    fn pep440_post(ver_str: &str, is_compat: bool) {
        let ver_match = Version::from_str("1.1.post1").unwrap();
        let constraint = Constraint::from_str(ver_str).unwrap();
        if constraint.is_compatible(&ver_match) != is_compat {
            eprintln!(
                "ver_match: {:?}\nconstraint: {:?}\nshould be compat: {:?}",
                ver_match, constraint, is_compat
            );
        }

        assert_eq!(constraint.is_compatible(&ver_match), is_compat);
    }

    #[test]
    fn version_parse() {
        assert_eq!(Version::from_str("3.12.5").unwrap(), Version::new(3, 12, 5));
        assert_eq!(Version::from_str("0.1.0").unwrap(), Version::new(0, 1, 0));
        assert_eq!(Version::from_str("3.7").unwrap(), Version::new(3, 7, 0));
        assert_eq!(Version::from_str("1").unwrap(), Version::new(1, 0, 0));
    }

    #[test]
    fn version_parse_w_wildcard() {
        assert_eq!(
            Version::from_str("3.2.*").unwrap(),
            Version::new_star(Some(3), Some(2), None, true)
        );
        assert_eq!(
            Version::from_str("1.*").unwrap(),
            Version::new_star(Some(1), None, None, true)
        );
        assert_eq!(
            Version::from_str("1.*.*").unwrap(),
            Version::new_star(Some(1), None, None, true)
        );
    }

    #[test]
    fn version_parse_w_modifiers() {
        assert_eq!(
            Version::from_str("19.3b0").unwrap(),
            Version {
                major: Some(19),
                minor: Some(3),
                patch: Some(0),
                extra_num: None,
                modifier: Some((Beta, 0)),
                star: false,
            }
        );

        assert_eq!(
            Version::from_str("1.3.5rc0").unwrap(),
            Version {
                major: Some(1),
                minor: Some(3),
                patch: Some(5),
                extra_num: None,
                modifier: Some((ReleaseCandidate, 0)),
                star: false,
            }
        );

        assert_eq!(
            Version::from_str("1.3.5.11").unwrap(),
            Version {
                major: Some(1),
                minor: Some(3),
                patch: Some(5),
                extra_num: Some(11),
                modifier: None,
                star: false,
            }
        );

        assert_eq!(
            Version::from_str("5.2.5.11b3").unwrap(),
            Version {
                major: Some(5),
                minor: Some(2),
                patch: Some(5),
                extra_num: Some(11),
                modifier: Some((Beta, 3)),
                star: false,
            }
        );
    }

    #[test]
    fn bad_version() {
        assert_eq!(
            Version::from_str("3-7"),
            Err(DependencyError {
                details: "Problem parsing version: 3-7".to_owned()
            })
        );
    }

    #[test]
    fn constraint_w_modifier() {
        let a = "!=2.3b3";
        let b = "^1.3.32rc1";
        let c = "^1.3.32.dep1";

        let req_a = Constraint::new(
            Ne,
            Version {
                major: Some(2),
                minor: Some(3),
                patch: Some(0),
                extra_num: None,
                modifier: Some((Beta, 3)),
                star: false,
            },
        );
        let req_b = Constraint::new(
            Caret,
            Version {
                major: Some(1),
                minor: Some(3),
                patch: Some(32),
                extra_num: None,
                modifier: Some((ReleaseCandidate, 1)),
                star: false,
            },
        );
        let req_c = Constraint::new(
            Caret,
            Version {
                major: Some(1),
                minor: Some(3),
                patch: Some(32),
                extra_num: None,
                modifier: Some((Dep, 1)),
                star: false,
            },
        );

        assert_eq!(Constraint::from_str(a).unwrap(), req_a);
        assert_eq!(Constraint::from_str(b).unwrap(), req_b);
        assert_eq!(Constraint::from_str(c).unwrap(), req_c);
    }

    #[test]
    fn constraint_multiple() {
        let expected = vec![
            Constraint::new(Gte, Version::new(2, 7, 0)),
            Constraint::new(Ne, Version::new(3, 0, 0)),
            Constraint::new(Ne, Version::new(3, 1, 0)),
            Constraint::new(Ne, Version::new(3, 2, 0)),
            Constraint::new(Lte, Version::new(3, 5, 0)),
        ];

        let actual =
            Constraint::from_str_multiple(">=2.7, !=3.0.0, !=3.1.0, !=3.2.0, <=3.5.0").unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn constraint_to_string() {
        let a = "!=2.3";
        let b = "^1.3.32";
        let c = "~2.3";
        let d = "==5";
        let e = "<=11.2.3";
        let f = ">=0.0.1";

        let req_a = Constraint::new(Ne, Version::new(2, 3, 0));
        let req_b = Constraint::new(Caret, Version::new(1, 3, 32));
        let req_c = Constraint::new(Tilde, Version::new(2, 3, 0));
        let req_d = Constraint::new(Exact, Version::new(5, 0, 0));
        let req_e = Constraint::new(Lte, Version::new(11, 2, 3));
        let req_f = Constraint::new(Gte, Version::new(0, 0, 1));

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
            constraints: vec![Constraint::new(Gte, Version::new(0, 14, 0))],
            extra: Some("security".into()),
            sys_platform: None,
            python_version: None,
            install_with_extras: None,
            path: None,
            git: None,
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
            python_version: Some(vec![Constraint::new(Exact, Version::new(2, 7, 0))]),
            install_with_extras: None,
            path: None,
            git: None,
        };

        let actual3 = Req::from_str(
            "win-unicode-console (>=0.5) ; sys_platform == \"win32\" and python_version < \"3.6\"",
            true,
        )
        .unwrap();

        let expected3 = Req {
            name: "win-unicode-console".into(),
            constraints: vec![Constraint::new(Gte, Version::new(0, 5, 0))],
            extra: None,
            sys_platform: Some((Exact, util::Os::Windows32)),
            python_version: Some(vec![Constraint::new(Lt, Version::new(3, 6, 0))]),
            install_with_extras: None,
            path: None,
            git: None,
        };

        let actual4 = Req::from_str("envisage ; extra == 'app'", true).unwrap();

        // Test with extras, but no version
        let expected4 = Req {
            name: "envisage".into(),
            constraints: vec![],
            extra: Some("app".into()),
            sys_platform: None,
            python_version: None,
            install_with_extras: None,
            path: None,
            git: None,
        };

        assert_eq!(actual, expected);
        assert_eq!(actual2, expected2);
        assert_eq!(actual3, expected3);
        assert_eq!(actual4, expected4);
    }

    // Non-standard format I've come across; more like the non-pypi fmt.
    #[test]
    fn parse_req_pypi_no_parens() {
        let actual1 = Req::from_str("pydantic >=0.32.2,<=0.32.2", true).unwrap();
        let actual2 = Req::from_str("starlette >=0.11.1,<=0.12.8", true).unwrap();

        let expected1 = Req {
            name: "pydantic".into(),
            constraints: vec![
                Constraint::new(Gte, Version::new(0, 32, 2)),
                Constraint::new(Lte, Version::new(0, 32, 2)),
            ],
            extra: None,
            sys_platform: None,
            python_version: None,
            install_with_extras: None,
            path: None,
            git: None,
        };

        let expected2 = Req {
            name: "starlette".into(),
            constraints: vec![
                Constraint::new(Gte, Version::new(0, 11, 1)),
                Constraint::new(Lte, Version::new(0, 12, 8)),
            ],
            extra: None,
            sys_platform: None,
            python_version: None,
            install_with_extras: None,
            path: None,
            git: None,
        };

        assert_eq!(actual1, expected1);
        assert_eq!(actual2, expected2);
    }

    #[test]
    fn parse_req_withvers() {
        let p = Req::from_str("bolt = \"3.1.4\"", false).unwrap();
        assert_eq!(
            p,
            Req::new(
                "bolt".into(),
                vec![Constraint::new(Exact, Version::new(3, 1, 4))]
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
                vec![Constraint::new(Caret, Version::new(2, 7, 18))]
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
                vec![Constraint::new(Tilde, Version::new(6, 7, 0))]
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
                vec![Constraint::new(Gte, Version::new(2, 22, 0))]
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
                vec![Constraint::new(Gte, Version::new(2016, 3, 0))]
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
                vec![Constraint::new(Gte, Version::new(0, 2, 3))]
            )
        );
        assert_eq!(b, Req::new("zc.lockfile".into(), vec![]));
    }

    #[rstest(
        input,
        expected,
        case("asgiref (~=3.2)", Constraint::new(TildeEq, Version::new(3, 2, 0))),
        case("asgiref (~3.2)", Constraint::new(Tilde, Version::new(3, 2, 0)))
    )]
    fn parse_req_pypi_tilde(input: &str, expected: Constraint) {
        let a = Req::from_str(input, true).unwrap();

        let rexpected = Req::new("asgiref".into(), vec![expected]);

        assert_eq!(a, rexpected);
    }

    #[test]
    fn parse_req_pypi_bracket() {
        // Note that [ufo] doesn't refer to an extra required to install this input; it's
        // an extra that may trigger additional installs from fonttools.
        let actual = Req::from_str("fonttools[ufo] (>=3.34.0)", true).unwrap();
        let mut expected = Req::new(
            "fonttools".into(),
            vec![Constraint::new(Gte, Version::new(3, 34, 0))],
        );
        expected.install_with_extras = Some(vec!["ufo".to_string()]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn parse_req_pypi_cplx() {
        let p = Req::from_str("urllib3 (!=1.25.0,!=1.25.1,<=1.26)", true).unwrap();
        assert_eq!(
            p,
            Req::new(
                "urllib3".into(),
                vec![
                    Constraint::new(Ne, Version::new(1, 25, 0)),
                    Constraint::new(Ne, Version::new(1, 25, 1)),
                    Constraint::new(Lte, Version::new(1, 26, 0))
                ]
            )
        )
    }

    #[rstest(
        req,
        expected,
        case::exact(Req::new("package".to_string(),
                             vec![
                                 Constraint::new(Exact, Version::new(1,2,3))
                             ]),
                    r#"package = "1.2.3""#),
        case::gte(Req::new("package".to_string(),
                           vec![
                               Constraint::new(Gte, Version::new(1,2,3))
                           ]),
                  r#"package = ">=1.2.3""#),
        case::lte(Req::new("package".to_string(),
                           vec![
                               Constraint::new(Lte, Version::new(1,2,3))
                           ]),
                  r#"package = "<=1.2.3""#),
        case::ne(Req::new("package".to_string(),
                          vec![
                              Constraint::new(Ne, Version::new(1,2,3))
                          ]),
                 r#"package = "!=1.2.3""#),
        case::gt(Req::new("package".to_string(),
                          vec![
                              Constraint::new(Gt, Version::new(1,2,3))
                          ]),
                 r#"package = ">1.2.3""#),
        case::lt(Req::new("package".to_string(),
                          vec![
                              Constraint::new(Lt, Version::new(1,2,3))
                          ]),
                 r#"package = "<1.2.3""#),
        case::caret(Req::new("package".to_string(),
                             vec![
                                 Constraint::new(Caret, Version::new(1,2,3))
                             ]),
                    r#"package = "^1.2.3""#),
        case::tilde(Req::new("package".to_string(),
                             vec![
                                 Constraint::new(Tilde, Version::new(1,2,3))
                             ]),
                    r#"package = "~1.2.3""#),
        case::multi_ne_gte(Req::new("package".to_string(),
                                    vec![
                                        Constraint::new(Ne, Version::new(1,2,3)),
                                        Constraint::new(Gte, Version::new(1,2,0))
                                    ]),
                           r#"package = "!=1.2.3, >=1.2.0""#)
    )]
    fn req_to_cfg_string(req: Req, expected: &str) {
        assert_eq!(req.to_cfg_string(), expected.to_string());
    }

    #[test]
    fn req_to_cfg_string_empty_constraints() {
        let ctx = res::get_version_info_context();
        ctx.expect().returning(|name, _py_ver| {
            Ok((
                name.to_string(),
                Version::new(1, 2, 3),
                vec![Version::new(1, 2, 3), Version::new(1, 1, 2)],
            ))
        });
        let req = Req::new("package".to_string(), vec![]);
        let expected = r#"package = "^1.2.3""#;
        assert_eq!(req.to_cfg_string(), expected.to_string());
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
    fn version_ordering_modded() {
        let a = Version {
            major: Some(4),
            minor: Some(9),
            patch: Some(4),
            extra_num: Some(2),
            modifier: None,
            star: false,
        };
        let b = Version::new(4, 9, 4);

        let c = Version {
            major: Some(4),
            minor: Some(9),
            patch: Some(4),
            extra_num: None,
            modifier: Some((VersionModifier::ReleaseCandidate, 2)),
            star: false,
        };
        let d = Version {
            major: Some(4),
            minor: Some(9),
            patch: Some(4),
            extra_num: None,
            modifier: Some((VersionModifier::ReleaseCandidate, 1)),
            star: false,
        };
        let e = Version {
            major: Some(4),
            minor: Some(9),
            patch: Some(4),
            extra_num: None,
            modifier: Some((VersionModifier::Beta, 6)),
            star: false,
        };
        let f = Version {
            major: Some(4),
            minor: Some(9),
            patch: Some(4),
            extra_num: None,
            modifier: Some((VersionModifier::Alpha, 7)),
            star: false,
        };
        let g = Version::new(4, 9, 2);

        assert!(a > b && b > c && c > d && d > e && e > f && f > g);
    }

    #[rstest(actual,
             expected,
             case::gt(Constraint::new(Gt, Version::new(5, 1, 3)),
                      vec![(Version::new(5, 1, 4), Version::_max())]),
             case::gte(Constraint::new(Gte, Version::new(5, 1, 0)),
                       vec![(Version::new(5, 1, 0), Version::_max())] ),
             case::ne(Constraint::new(Ne, Version::new(5, 1, 3)),
                      vec![(Version::new(0, 0, 0), Version::new(5, 1, 2)),
                           (Version::new(5, 1, 4), Version::_max()),]),
             case::lt(Constraint::new(Lt, Version::new(5, 1, 3)),
                      vec![(Version::new(0, 0, 0), Version::new(5, 1, 2))]),
             case::lte(Constraint::new(Lte, Version::new(5, 1, 3)),
                       vec![(Version::new(0, 0, 0), Version::new(5, 1, 3))]),
             case::caret(Constraint::new(Caret, Version::new(1,2,3)),
                         vec![(Version::new(1,2,3), Version::new(1,MAX_VER,MAX_VER))]
             ),
             case::tilde(Constraint::new(Tilde, Version::new(1,2,3)),
                         vec![(Version::new(1,2,3), Version::new(1,2,MAX_VER))]
             )
    )]
    fn compat_rng(actual: Constraint, expected: Vec<(Version, Version)>) {
        assert_eq!(actual.compatible_range(), expected);
    }

    #[test]
    fn intersections_empty() {
        let reqs1 = vec![
            Constraint::new(Exact, Version::new(4, 9, 4)),
            Constraint::new(Gte, Version::new(4, 9, 7)),
        ];

        let reqs2 = vec![
            Constraint::new(Lte, Version::new(4, 9, 6)),
            Constraint::new(Gte, Version::new(4, 9, 7)),
        ];

        assert!(intersection_many(&reqs1).is_empty());
        assert!(intersection_many(&reqs2).is_empty());
    }

    #[test]
    fn intersections_simple() {
        let reqs1 = (Version::new(4, 9, 4), Version::_max());
        let reqs2 = (Version::new(4, 3, 1), Version::_max());

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
        let reqs1 = vec![
            Constraint::new(Gte, Version::new(4, 9, 4)),
            Constraint::new(Gte, Version::new(4, 3, 1)),
        ];
        let reqs2 = vec![
            Constraint::new(Caret, Version::new(3, 0, 0)),
            Constraint::new(Exact, Version::new(3, 3, 6)),
        ];

        assert_eq!(
            intersection_many(&reqs1),
            vec![(Version::new(4, 9, 4), Version::_max())]
        );
        assert_eq!(
            intersection_many(&reqs2),
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
        let reqs = vec![
            Constraint::new(Gte, Version::new(4, 9, 2)),
            Constraint::new(Gte, Version::new(4, 9, 4)),
            Constraint::new(Lt, Version::new(5, 5, 5)),
        ];

        assert_eq!(
            intersection_many(&reqs),
            vec![(Version::new(4, 9, 4), Version::new(5, 5, 4))]
        );
    }

    #[test]
    fn intersection_contained_many_w_ne() {
        let reqs1 = vec![
            Constraint::new(Ne, Version::new(2, 0, 4)),
            Constraint::new(Ne, Version::new(2, 1, 2)),
            Constraint::new(Ne, Version::new(2, 1, 6)),
            Constraint::new(Gte, Version::new(2, 0, 1)),
            Constraint::new(Gte, Version::new(2, 0, 2)),
        ];

        assert_eq!(
            intersection_many(&reqs1),
            vec![(Version::new(2, 0, 2), Version::_max())]
        );
    }

    #[rstest(input, expected,
             case::py3("py3", vec![Constraint::new(Gte, Version::new(3, 0, 0))]),
             case::cp_chain("cp35.cp36.cp37.cp38",
                            vec![
                                Constraint::new(Exact, Version::new(3, 5, 0)),
                                Constraint::new(Exact, Version::new(3, 6, 0)),
                                Constraint::new(Exact, Version::new(3, 7, 0)),
                                Constraint::new(Exact, Version::new(3, 8, 0)),
                            ]),
             case::cp26("cp26", vec![Constraint::new(Exact, Version::new(2, 6, 0))]),
             case::py_chain("py2.py3",
                            vec![
                                Constraint::new(Lte, Version::new(2, 10, 0)),
                                Constraint::new(Gte, Version::new(3, 0, 0)),
                            ]),
             case::pp36("pp36", vec![Constraint::new(Exact, Version::new(3, 6, 0))]),
             case::any("any", vec![Constraint::new(Gte, Version::new(2, 0, 0))]),
             case::semver("2.7", vec![Constraint::new(Caret, Version::new(2, 7, 0))]),
             case::pp257("pp257", vec![Constraint::new(Exact, Version::new(2, 5, 7))])
    )]
    fn python_version_from_warehouse(input: &str, expected: Vec<Constraint>) {
        let a1 = Constraint::from_wh_py_vers(input).unwrap();
        assert_eq!(a1, expected)
    }
}
