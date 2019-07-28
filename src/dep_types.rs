use regex::Regex;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::{cmp, fmt, num, str::FromStr};

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
    fn from(error: num::ParseIntError) -> Self {
        Self { details: "".into() }
    }
}

/// An exact, 3-number Semver version.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
} //impl Serialize for Version {
  //      fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  //    where
  //        S: Serializer,
  //    {
  //        // 3 is the number of fields in the struct.
  //        let mut state = serializer.serialize_struct("Color", 3)?;
  //        state.serialize_field("r", &self.r)?;
  //        state.serialize_field("g", &self.g)?;
  //        state.serialize_field("b", &self.b)?;
  //        state.end()
  //    }
  //}

//impl Serialize for Version {
//      fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//    where
//        S: Serializer,
//    {
//        // 3 is the number of fields in the struct.
//        let mut state = serializer.serialize_struct("Color", 3)?;
//        state.serialize_field("r", &self.r)?;
//        state.serialize_field("g", &self.g)?;
//        state.serialize_field("b", &self.b)?;
//        state.end()
//    }
//}

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
}

impl FromStr for Version {
    type Err = DependencyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"^(\d{1,9})\.?(?:(?:(\d{1,9})\.?)?\.?(\d{1,9})?)?\.?(.*)$").unwrap();
        let caps = re
            .captures(s)
            .unwrap_or_else(|| panic!("Problem parsing version: {}", s));

        let major = caps.get(1).unwrap().as_str().parse::<u32>()?;
        let minor = match caps.get(2) {
            Some(p) => p.as_str().parse::<u32>()?,
            None => 0,
        };

        let patch = match caps.get(3) {
            Some(p) => p.as_str().parse::<u32>()?,
            None => 0,
        };
        // todo: Ignore trailing part for now.
        //        match caps.get(4) {
        //            Some(p) => p.as_str().parse::<u32>()?,
        //            None => (),
        //        };

        Ok(Self {
            major,
            minor,
            patch,
        })
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
pub struct VersionReq {
    pub type_: ReqType,
    pub major: u32,
    pub minor: Option<u32>,
    pub patch: Option<u32>,
    // todo
    //    pub extras: Vec<String>
}

impl FromStr for VersionReq {
    type Err = DependencyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // todo: You could delegate part of this out, or at least share the regex with Version::to_string
        let re = Regex::new(
            r"^(\^|~|==|<=|>=|<|>|!=)?(\d{1,9})\.?(?:(?:(\d{1,9})\.?)?\.?(\d{1,9})?)?\.?$",
        )
        .unwrap();

        let caps = match re.captures(s) {
            Some(c) => c,
            None => return Err(DependencyError::new("Problem parsing Version requirement")),
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
impl VersionReq {
    pub fn new(type_: ReqType, major: u32, minor: u32, patch: u32) -> Self {
        Self {
            type_,
            major,
            minor: Some(minor),
            patch: Some(patch),
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

        match self.type_ {
            ReqType::Exact => vec![(self_version, self_version)],
            ReqType::Gte => vec![(self_version, highest)],
            ReqType::Lte => vec![(lowest, self_version)],
            ReqType::Gt => vec![(
                Version::new(self.major, self_version.minor, self_version.patch + 1),
                highest,
            )],
            ReqType::Lt => vec![(
                lowest,
                Version::new(self.major, self_version.minor, self_version.patch - 1),
            )],
            ReqType::Ne => vec![
                (
                    lowest,
                    Version::new(self.major, self_version.minor, self_version.patch - 1),
                ),
                (
                    Version::new(self.major, self_version.minor, self_version.patch + 1),
                    highest,
                ),
            ],
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

///// Find the intersection of two verison requirements
//pub fn intersection_single(req1: &VersionReq, req2: &VersionReq) -> Vec<VersionReq> {
//    // The maximum number of intersection requirements can be the lowest of the num
//    // requirements of our two starting sets. We start with one, which will only get narrower
//    // as we include requirements from the other.
//
//    let ranges1 = req1.compatible_range();
//    let ranges2 = req2.compatible_range();
//
//}

pub fn to_ranges(reqs: &[VersionReq]) -> Vec<(Version, Version)> {
    // If no requirement specified, return the full range.
    if reqs.is_empty() {
        vec![(Version::new(0, 0, 0), Version::new(MAX_VER, 0, 0))]
    } else {
        reqs.iter()
            .map(|r| r.compatible_range())
            .flatten()
            .collect()
    }
}

/// todo: Find a more elegant way to handle this; diff is second arg's type.
pub fn intersection_temp(
    reqs1: &[VersionReq],
    ranges2: &[(Version, Version)],
) -> Vec<(Version, Version)> {
    let mut ranges1 = vec![];

    for req in reqs1 {
        for range in req.compatible_range() {
            ranges1.push(range);
        }
    }

    let mut result = vec![];
    for rng1 in ranges1 {
        for rng2 in ranges2 {
            // 0 is min, 1 is max.
            if rng2.1 >= rng1.0 && rng1.0 <= rng2.1 {
                result.push((rng1.0, rng2.1))
            } else if rng1.1 >= rng2.0 && rng2.0 <= rng1.1 {
                result.push((rng2.0, rng1.1))
            }
        }
    }

    result
}

/// Find the intersection of two sets of version requirements. Result is a Vec of (min, max) tuples.
pub fn intersection(reqs1: &[VersionReq], reqs2: &[VersionReq]) -> Vec<(Version, Version)> {
    let mut ranges1 = vec![];
    for req in reqs1 {
        for range in req.compatible_range() {
            ranges1.push(range);
        }
    }

    let mut ranges2 = vec![];
    for req in reqs2 {
        for range in req.compatible_range() {
            ranges2.push(range);
        }
    }

    let mut result = vec![];
    for rng1 in &ranges1 {
        for rng2 in &ranges2 {
            // 0 is min, 1 is max.
            if rng2.1 >= rng1.0 && rng2.0 <= rng1.1 {
                result.push((cmp::min(rng2.0, rng1.1), cmp::max(rng1.0, rng2.1)));
            } else if rng1.1 >= rng2.0 && rng1.0 <= rng2.1 {
                result.push((cmp::min(rng1.0, rng2.1), cmp::max(rng2.0, rng1.1)))
            }
        }
    }

    result
}

/// Includes information for describing a `Python` dependency. Can be used in a dependency
/// graph. Uses a set of requirements, compared to `Package`, which is tied to an exact version.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Dependency {
    pub name: String,
    pub version_reqs: Vec<VersionReq>,
    pub dependencies: Vec<Dependency>,
    // todo: Is this a good place to store hash?
}

/// [Ref](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)
impl Dependency {
    // todo: Re-implement this.
    //    /// Find the version from a selection that's most compatible with this
    //    /// dependency's requirements.
    //    pub fn best_match(&self, versions: &[Version]) -> Option<Version> {
    //        // If no version specified, use the highest available.
    //        if self.version.is_none() {
    //            // This logic has to do with derefing the interior of Option.
    //            return match versions.into_iter().max() {
    //                Some(v) => Some(v.clone()),
    //                None => None,
    //            };
    //        }
    //
    //        match self.version_type {
    //            // For an exact version type, there's only one correct answer.
    //            ReqType::Exact => {
    //                let result = versions
    //                    .into_iter()
    //                    .filter(|v| *v == &self.version.unwrap())
    //                    .collect::<Vec<&Version>>();
    //
    //                let b = result.get(0);
    //
    //                match b {
    //                    Some(v) => Some(*v.clone()),
    //                    None => None,
    //                }
    //            }
    //            // todo implement later.
    //            ReqType::Tilde => None,
    //            ReqType::Caret => None,
    //        }
    //    }

    /// eg `saturn>=0.3.1`, or `'stevedore>=1.3.0,<1.4.0'` (Note single quotes
    /// when there are multiple requirements specified.
    pub fn _to_pip_string(&self) -> String {
        // Note that ^= may not be valid in Pip, but ~= is.
        match self.version_reqs.len() {
            0 => self.name.to_string(),
            1 => format!(
                "{}{}",
                self.name,
                self.version_reqs[0].to_string(false, true),
            ),
            _ => {
                let req_str = self
                    .version_reqs
                    .iter()
                    .map(|r| r.to_string(false, true))
                    .collect::<Vec<String>>()
                    .join(",");

                // Note the single quotes here, as required by pip when specifying
                // multiple requirements
                format!("'{}{}'", self.name, req_str)
            }
        }
    }

    /// eg `saturn = "^0.3.1"` or `matplotlib = "3.1.1"`
    pub fn to_cfg_string(&self) -> String {
        match self.version_reqs.len() {
            0 => self.name.to_owned(),
            _ => format!(
                r#"{} = "{}""#,
                self.name,
                self.version_reqs
                    .iter()
                    .map(|r| r.to_string(true, false))
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
        }
    }
}

/// This would be used when parsing from something like `pyproject.toml`
impl Dependency {
    pub fn from_str(s: &str, pypi_fmt: bool) -> Result<Self, DependencyError> {
        let re = if pypi_fmt {
            // ie saturn (>=0.3.4)
            Regex::new(r"^(.*?)\s+\((.*)\)(?:\s*;.*)?$").unwrap()
        } else {
            // ie saturn = ">=0.3.4"
            Regex::new(r#"^(.*?)\s*=\s*["'](.*)["']$"#).unwrap()
        };
        if let Some(caps) = re.captures(s) {
            let name = caps.get(1).unwrap().as_str().to_string();
            let reqs_m = caps.get(2).unwrap();
            let reqs = VersionReq::from_str_multiple(reqs_m.as_str())?;

            return Ok(Self {
                name,
                version_reqs: reqs,
                dependencies: vec![],
            });
        };

        // Check if no version is specified.
        let novers_re = if pypi_fmt {
            Regex::new(r"^([a-zA-Z\-0-9]+)(?:\s*;)?").unwrap()
        } else {
            Regex::new(r"^([a-zA-Z\-0-9]+)$").unwrap()
        };

        if let Some(m) = novers_re.captures(s) {
            return Ok(Self {
                name: m.get(1).unwrap().as_str().to_string(),
                version_reqs: vec![],
                dependencies: vec![],
            });
        }

        Err(DependencyError::new(&format!(
            "Problem parsing version requirement: {}",
            s
        )))
    }

    pub fn from_pip_str(s: &str) -> Option<Self> {
        // todo multiple ie single quotes support?
        // Check if no version is specified.
        if Regex::new(r"^([a-zA-Z\-0-9]+)$")
            .unwrap()
            .captures(s)
            .is_some()
        {
            return Some(Self {
                name: s.to_string(),
                version_reqs: vec![],
                dependencies: vec![],
            });
        }

        let re = Regex::new(r"^(.*?)((?:\^|~|==|<=|>=|<|>|!=).*)$").unwrap();

        let caps = match re.captures(s) {
            Some(c) => c,
            // todo: Figure out how to return an error
            None => return None,
        };

        let name = caps.get(1).unwrap().as_str().to_string();
        let req = VersionReq::from_str(caps.get(2).unwrap().as_str())
            .expect("Problem parsing requirement");

        Some(Self {
            name,
            version_reqs: vec![req],
            dependencies: vec![],
        })
    }
}

/// An exact package to install. Typed analog of LockPack.
#[derive(Clone, Debug)]
pub struct Package {
    pub name: String,
    pub version: Version,
    pub deps: Vec<Dependency>,
    pub source: Option<String>,
}

impl Package {
    pub fn to_pip_string(&self) -> String {
        format!("{}=={}", self.name, self.version.to_string())
    }

    pub fn from_lock_pack(lock_pack: &LockPackage) -> Self {
        // todo: Return result /propogate parse errors.
        Self {
            name: lock_pack.name.to_owned(),
            version: Version::from_str(&lock_pack.version)
                .expect("Problem converting from lock pack version"),
            deps: match &lock_pack.dependencies {
                Some(deps) => deps
                    .iter()
                    .map(|d| {
                        Dependency::from_str(d, false).expect("Problem converting Dep from string")
                    })
                    .collect(),
                None => vec![],
            },
            source: lock_pack.source.to_owned(),
        }
    }
}
// todo: Implement to/from Lockpack for Package?

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
    pub metadata: Option<String>, // ie checksums
}

impl Lock {
    fn add_packages(&mut self, packages: &[Package]) {
        // todo: Write tests for this.

        for package in packages {
            // Use the actual version installed, not the requirement!
            // todo: reconsider your package etc structs
            // todo: Perhaps impl to_lockpack etc from Package.
            let lock_package = LockPackage {
                name: package.name.to_owned(),
                version: package.version.to_string(),
                source: package.source.clone(),
                dependencies: None,
            };

            match &mut self.package {
                Some(p) => p.push(lock_package),
                None => self.package = Some(vec![lock_package]),
            }
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use ReqType::{Caret, Exact, Gt, Gte, Lt, Lte, Ne, Tilde};

    #[test]
    fn compat_caret() {
        let req1 = VersionReq {
            major: 1,
            minor: Some(2),
            patch: Some(3),
            type_: Caret,
        };
        let req2 = VersionReq {
            major: 0,
            minor: Some(2),
            patch: Some(3),
            type_: Caret,
        };
        let req3 = VersionReq {
            major: 0,
            minor: Some(0),
            patch: Some(3),
            type_: Caret,
        };

        assert!(req1.is_compatible(&Version::new(1, 9, 9)));
        assert!(!req1.is_compatible(&Version::new(2, 0, 0)));
        assert!(req2.is_compatible(&Version::new(0, 2, 9)));
        assert!(!req2.is_compatible(&Version::new(0, 3, 0)));
        assert!(req3.is_compatible(&Version::new(0, 0, 3)));
        assert!(!req3.is_compatible(&Version::new(0, 0, 5)));
    }

    #[test]
    fn compat_gt_eq() {
        let req1 = VersionReq {
            major: 1,
            minor: Some(2),
            patch: Some(3),
            type_: Gte,
        };
        let req2 = VersionReq {
            major: 0,
            minor: Some(2),
            patch: Some(3),
            type_: Gt,
        };
        let req3 = VersionReq {
            major: 0,
            minor: Some(0),
            patch: Some(3),
            type_: Exact,
        };

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
    }

    #[test]
    fn version_req_tostring() {
        let a = "!=2.3";
        let b = "^1.3.32";
        let c = "~2.3";
        let d = "==5";
        let e = "<=11.2.3";
        let f = ">=0.0.1";

        let req_a = VersionReq {
            major: 2,
            minor: Some(3),
            patch: None,
            type_: Ne,
        };
        let req_b = VersionReq {
            major: 1,
            minor: Some(3),
            patch: Some(32),
            type_: Caret,
        };
        let req_c = VersionReq {
            major: 2,
            minor: Some(3),
            patch: None,
            type_: Tilde,
        };
        let req_d = VersionReq {
            major: 5,
            minor: None,
            patch: None,
            type_: Exact,
        };
        let req_e = VersionReq {
            major: 11,
            minor: Some(2),
            patch: Some(3),
            type_: Lte,
        };
        let req_f = VersionReq {
            major: 0,
            minor: Some(0),
            patch: Some(1),
            type_: Gte,
        };

        assert_eq!(VersionReq::from_str(a).unwrap(), req_a);
        assert_eq!(VersionReq::from_str(b).unwrap(), req_b);
        assert_eq!(VersionReq::from_str(c).unwrap(), req_c);
        assert_eq!(VersionReq::from_str(d).unwrap(), req_d);
        assert_eq!(VersionReq::from_str(e).unwrap(), req_e);
        assert_eq!(VersionReq::from_str(f).unwrap(), req_f);
    }

    #[test]
    fn parse_dep_novers() {
        let actual1 = Dependency::from_str("saturn", false).unwrap();
        let actual2 = Dependency::from_str("saturn", true).unwrap();
        let actual3 = Dependency::from_str("saturn; extra == 'bcrypt", true).unwrap();
        let expected = Dependency {
            name: "saturn".into(),
            version_reqs: vec![],
            dependencies: vec![],
        };
        assert_eq!(actual1, expected);
        assert_eq!(actual2, expected);
        assert_eq!(actual3, expected);
    }

    #[test]
    fn parse_dep_pypi_semicolon() {
        // tod: Make this handle extras.
        let actual =
            Dependency::from_str("pyOpenSSL (>=0.14) ; extra == 'security'", true).unwrap();
        let expected = Dependency {
            name: "pyOpenSSL".into(),
            version_reqs: vec![VersionReq {
                type_: Gte,
                major: 0,
                minor: Some(14),
                patch: None,
            }],
            dependencies: vec![],
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn parse_dep_withvers() {
        let p = Dependency::from_str("bolt = \"3.1.4\"", false).unwrap();
        assert_eq!(
            p,
            Dependency {
                name: "bolt".into(),
                version_reqs: vec![VersionReq {
                    major: 3,
                    minor: Some(1),
                    patch: Some(4),
                    type_: Exact,
                },],
                dependencies: vec![],
            }
        )
    }

    #[test]
    fn parse_dep_caret() {
        let p = Dependency::from_str("chord = \"^2.7.18\"", false).unwrap();
        assert_eq!(
            p,
            Dependency {
                name: "chord".into(),
                version_reqs: vec![VersionReq {
                    major: 2,
                    minor: Some(7),
                    patch: Some(18),
                    type_: Caret,
                }],
                dependencies: vec![],
            }
        )
    }

    #[test]
    fn parse_dep_tilde_short() {
        let p = Dependency::from_str("sphere = \"~6.7\"", false).unwrap();
        assert_eq!(
            p,
            Dependency {
                name: "sphere".into(),
                version_reqs: vec![VersionReq {
                    major: 6,
                    minor: Some(7),
                    patch: None,
                    type_: Tilde,
                }],
                dependencies: vec![],
            }
        )
    }

    #[test]
    fn parse_dep_pip() {
        let p = Dependency::from_pip_str("Django>=2.22").unwrap();
        assert_eq!(
            p,
            Dependency {
                name: "Django".into(),
                version_reqs: vec![VersionReq {
                    major: 2,
                    minor: Some(22),
                    patch: None,
                    type_: Gte,
                },],

                dependencies: vec![],
            }
        )
    }

    #[test]
    fn parse_dep_pypi_cplx() {
        let p = Dependency::from_str("urllib3 (!=1.25.0,!=1.25.1,<=1.26)", true).unwrap();
        assert_eq!(
            p,
            Dependency {
                name: "urllib3".into(),
                version_reqs: vec![
                    VersionReq {
                        major: 1,
                        minor: Some(25),
                        patch: Some(0),
                        type_: Ne,
                    },
                    VersionReq {
                        major: 1,
                        minor: Some(25),
                        patch: Some(1),
                        type_: Ne,
                    },
                    VersionReq {
                        major: 1,
                        minor: Some(26),
                        patch: None,
                        type_: Lte,
                    }
                ],

                dependencies: vec![],
            }
        )
    }

    #[test]
    fn dep_tostring_single_reqs() {
        // todo: Expand this with more cases

        let a = Dependency {
            name: "package".to_string(),
            version_reqs: vec![VersionReq {
                major: 3,
                minor: Some(3),
                patch: Some(6),
                type_: Exact,
            }],
            dependencies: vec![],
        };

        assert_eq!(a.to_pip_string(), "package==3.3.6".to_string());
        assert_eq!(a.to_cfg_string(), r#"package = "3.3.6""#.to_string());
    }

    #[test]
    fn dep_tostring_multiple_reqs() {
        // todo: Expand this with more cases

        let a = Dependency {
            name: "package".to_string(),
            version_reqs: vec![
                VersionReq {
                    major: 2,
                    minor: Some(7),
                    patch: Some(4),
                    type_: Ne,
                },
                VersionReq {
                    major: 3,
                    minor: Some(7),
                    patch: None,
                    type_: Gte,
                },
            ],
            dependencies: vec![],
        };

        assert_eq!(a.to_pip_string(), "'package!=2.7.4,>=3.7'".to_string());
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
    fn intersections_empty() {
        let reqs1 = vec![VersionReq::new(Exact, 4, 9, 4)];
        let reqs2 = vec![VersionReq::new(Gte, 4, 9, 7)];

        let reqs3 = vec![VersionReq::new(Lte, 4, 9, 6)];
        let reqs4 = vec![VersionReq::new(Gte, 4, 9, 7)];

        assert!(intersection(&reqs1, &reqs2).is_empty());
        assert!(intersection(&reqs3, &reqs4).is_empty());
    }

    #[test]
    fn intersections_simple() {
        let reqs1 = vec![VersionReq::new(Gte, 4, 9, 4)];
        let reqs2 = vec![VersionReq::new(Gte, 4, 3, 1)];

        let reqs3 = vec![VersionReq::new(Caret, 3, 0, 0)];
        let reqs4 = vec![VersionReq::new(Exact, 3, 3, 6)];

        assert_eq!(
            intersection(&reqs1, &reqs2),
            vec![(Version::new(4, 9, 4), Version::new(MAX_VER, 0, 0))]
        );
        assert_eq!(
            intersection(&reqs3, &reqs4),
            vec![(Version::new(3, 3, 6), Version::new(3, 3, 6))]
        );
    }
}
