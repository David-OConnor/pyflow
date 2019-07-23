use regex::Regex;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::{cmp, num, str::FromStr, string::ParseError};

/// An exact, 3-number Semver version.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
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

    // todo Notsure why I need this; FromStr's doesn't always work.
    pub fn from_str2(s: &str) -> Option<Self> {
        let re = Regex::new(r"^(\d{1,4})\.(\d{1,4})(?:\.(\d{1,4}))?(.*)$").unwrap();
        let caps = re
            .captures(s)
            .expect(&format!("Problem parsing version: {}", s));

        // The final match group indicates beta, release candidate etc. Ignore if we find it.
        // todo: Handle these.
        if !caps.get(4).unwrap().as_str().is_empty() {
            return None;
        }

        let major = caps.get(1).unwrap().as_str().parse::<u32>().unwrap();
        let minor = caps.get(2).unwrap().as_str().parse::<u32>().unwrap();

        let patch = match caps.get(3) {
            Some(p) => p.as_str().parse::<u32>().unwrap(),
            None => 0,
        };

        Some(Self {
            major,
            minor,
            patch,
        })
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
            //            Some(p) => Some(p.as_str().parse::<u32>().unwrap()),
            //            None => None,
            Some(p) => p.as_str().parse::<u32>().unwrap(),
            None => 0,
        };

        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        if self.major != self.major {
            self.major.cmp(&other.major)
        } else if self.minor != other.minor {
            self.minor.cmp(&other.minor)
        } else {
            let self_patch = self.patch.unwrap_or(0);
            let other_patch = other.patch.unwrap_or(0);
            self_patch.cmp(&other_patch)
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
        //        match self.patch {
        //            Some(patch) => format!("{}.{}.{}", self.major, self.minor, patch),
        //            None => format!("{}.{}", self.major, self.minor),
        //        }
    }
}

/// Specify the type of version requirement
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum ReqType {
    Exact,
    Gte,
    Lte,
    Ne,
    Carot,
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
            ReqType::Ne => "!=".into(),
            ReqType::Carot => "^".into(),
            ReqType::Tilde => "~".into(),
        }
    }
}

impl ReqType {
    fn from_str(s: &str) -> Self {
        match s {
            "==" => ReqType::Exact,
            ">=" => ReqType::Gte,
            "<=" => ReqType::Lte,
            "!=" => ReqType::Ne,
            "^" => ReqType::Carot,
            "~" => ReqType::Tile,
            _ => panic!("Problem parsing requirement"),
        }
    }
}

/// For holding semvar-style version requirements with carot, tilde etc
/// /// [Ref](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)
pub struct VersionReq {
    pub major: u32,
    pub minor: Option<u32>,
    pub patch: Option<u32>,
    pub type_: ReqType,
}

/// A single version req. Can be chained together.
impl VersionReq {
    pub fn from_str(s: &str) -> Result<Self, Box<Error>> {
        let re =
            Regex::new(r"^(\^|~|==|<=|>=|!=)(\d{1,9})\.?(?:(?:(\d{1,9})\.?)?\.?(\d{1,9})?)?\.?$")
                .unwrap();

        let caps = re.captures(s)?;

        // Only prefix and major are required.
        let type_ = match caps.get(1) {
            Some(p) => p.as_str(),
            None => return Err("Problem parsing version"),
        };

        let major = match caps.get(2) {
            Some(p) => p.as_str().parse::<u32>().unwrap(),
            None => return Err("Problem parsing version"),
        };

        let mut minor = match caps.get(3) {
            Some(p) => Some(p.as_str().parse::<u32>().unwrap()),
            None => None,
        };

        let mut patch = match caps.get(4) {
            Some(p) => Some(p.as_str().parse::<u32>().unwrap()),
            None => None,
        };

        Ok(Self {
            major,
            minor,
            patch,
            type_: ReqType::from_str(type_),
        })
    }

    /// From a comma-separated list
    pub fn from_str_multiple(vers: &str) -> Result<Vec<Self>, Box<Error>> {
        Ok(vers.split(",").map(|req| Self::from_str(req)).collect())
    }
}

/// Includes information for describing a `Python` dependency
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Dependency {
    pub name: String,
    version_reqs: Vec<VersionReq>,
    dependencies: Vec<Dependency>,
    // todo: Remove this if we discover a workaround.
    pub bin: bool,
}

/// [Ref](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)
impl Dependency {
    pub fn is_compatible(&self, version: Version) -> bool {
        // Version being unspecified means any version's acceptable; setting to 0s ensure that.
        let self_version = self.version.unwrap_r(Version::new(0, 0, 0));
        let min = self_version;
        let mut max = Version::new(0, 0, 0);

        match self_version_type {
            ReqType::Exact => self_version.expect("Missing version") == version,
            ReqType::Carot => {
                if self_version.major > 0 {
                    max = Version::new(self_version.major + 1, 0, 0);
                } else if self_version.minor > 0 {
                    max = Version::new(0, self_version.minor + 1, 0);
                } else {
                    max = Version::new(0, 0, self_version.patch + 1);
                }
                min < version && version < max
            }
            ReqType::Tilde => {}
        }
    }

    /// Find the version from a selection that's most compatible with this
    /// dependency's requirements.
    pub fn best_match(&self, versions: &[Version]) -> Option<Version> {
        // If no version specified, use the highest available.
        if self.version.is_none() {
            // This logic has to do with derefing the interior of Option.
            return match versions.into_iter().max() {
                Some(v) => Some(v.clone()),
                None => None,
            };
        }

        match self.version_type {
            // For an exact version type, there's only one correct answer.
            ReqType::Exact => {
                let result = versions
                    .into_iter()
                    .filter(|v| *v == &self.version.unwrap())
                    .collect::<Vec<&Version>>();

                let b = result.get(0);

                match b {
                    Some(v) => Some(*v.clone()),
                    None => None,
                }
            }
            // todo implement later.
            ReqType::Tilde => None,
            ReqType::Carot => None,
        }
    }

    /// eg `saturn>=0.3.1`
    pub fn to_pip_string(&self) -> String {
        match self.version {
            Some(version) => {
                self.name.clone() + &self.version_type.to_string() + &version.to_string()
            }
            None => self.name.clone(),
        }
    }

    /// eg `saturn = "^0.3.1"`
    pub fn to_toml_string(&self) -> String {
        match self.version {
            Some(version) => format!(
                "{} = \"{}{}\"",
                self.name.clone(),
                self.version_type.toml_string(),
                version.to_string()
            ),
            None => self.name.clone(),
        }
    }
}

/// This would be used when parsing from something like `pyproject.toml`
impl FromStr for Dependency {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // todo delegate out to Version_req::from_str
        let re = Regex::new(r#"^(.*?)\s*=\s*"(.*)"$"#).unwrap();
        let caps = re.captures(s)?;

        let name = caps.get(1).unwrap().as_str();
        let reqs = caps.get(2)?;
        let reqs = VersionReq::from_str_multiple(req.as_str());

        Ok(Self {
            name: name.to_string(),
            version_reqs: reqs,
            dependencies: vec![],
            bin: false,
        })
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn compat_carot() {
        let dep1 = VersionReq {
            major: 1,
            minor: Some(2),
            patch: Some(3),
            type_: ReqType::Carot,
        };
        let dep2 = VersionReq {
            major: 0,
            minor: Some(2),
            patch: Some(3),
            type_: ReqType::Carot,
        };
        let dep3 = VersionReq {
            major: 0,
            minor: Some(0),
            patch: Some(3),
            type_: ReqType::Carot,
        };
    }

    assert!(dep1.is_compatible(Version::new(1, 9, 9)));
    assert!(!dep1.is_compatible(Version::new(2, 0, 0)));
    assert!(dep2.is_compatible(Version::new(0, 2, 9)));
    assert!(!dep2.is_compatible(Version::new(0, 3, 0)));
    assert!(dep3.is_compatible(Version::new(0, 0, 3)));
    assert!(!dep3.is_compatible(Version::new(0, 0, 4)));

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
    }

    #[test]
    #[should_panic(expected = "Problem parsing version: 3-7")]
    fn bad_version() {
        Version::from_str("3-7").unwrap();
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
            type_: ReqType::Ne,
        };
        let req_b = VersionReq {
            major: 1,
            minor: Some(3),
            patch: Some(32),
            type_: ReqType::Carot,
        };
        let req_c = VersionReq {
            major: 2,
            minor: Some(3),
            patch: None,
            type_: ReqType::Tilde,
        };
        let req_d = VersionReq {
            major: 5,
            minor: None,
            patch: None,
            type_: ReqType::Exact,
        };
        let req_e = VersionReq {
            major: 11,
            minor: Some(2),
            patch: Some(3),
            type_: ReqType::Lte,
        };
        let req_f = VersionReq {
            major: 0,
            minor: Some(0),
            patch: Some(1),
            type_: ReqType::Gte,
        };
    }

    #[test]
    fn parse_dep_novers() {
        let p = Dependency::from_str("saturn").unwrap();
        assert_eq!(
            p,
            Dependency {
                name: "saturn".into(),
                version_reqs: vec![],
                dependencies: vec![],
                bin: false,
            }
        )
    }

    #[test]
    fn parse_dep_withvers() {
        let p = Dependency::from_str("bolt = \"3.1.4\"").unwrap();
        assert_eq!(
            p,
            Dependency {
                name: "bolt".into(),
                version_reqs: vec![VersionReq {
                    major: 3,
                    minor: Some(1),
                    patch: Some(4),
                    type_: ReqType::Carot,
                },],
                dependencies: vec![],
                bin: false,
            }
        )
    }

    #[test]
    fn parse_dep_carot() {
        let p = Dependency::from_str("chord = \"^2.7.18\"").unwrap();
        assert_eq!(
            p,
            Dependency {
                name: "chord".into(),
                version_reqs: vec![VersionReq {
                    major: 2,
                    minor: Some(7),
                    patch: Some(18),
                    type_: ReqType::Carot,
                }],
                dependencies: vec![],
                bin: false,
            }
        )
    }

    #[test]
    fn parse_dep_tilde_short() {
        let p = Dependency::from_str("sphere = \"~6.7\"").unwrap();
        assert_eq!(
            p,
            Dependency {
                name: "sphere".into(),
                version: Some(Version::new_short(6, 7)),
                version_type: ReqType::Tilde,
                bin: false,
            }
        )
    }

    #[test]
    fn version_ordering() {
        let a = Version::new(4, 9, 4);
        let b = Version::new(4, 8, 0);
        let c = Version::new(3, 3, 6);
        let d = Version::new(3, 3, 5);
        let e = Version::new(3, 3, 0);

        assert!(a > b && b > c && c > d && d > e);
    }
}
