use regex::Regex;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::{cmp, num, str::FromStr};

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

impl ReqType {
    fn from_str(s: &str) -> Self {
        match s {
            "==" => ReqType::Exact,
            ">=" => ReqType::Gte,
            "<=" => ReqType::Lte,
            ">" => ReqType::Gt,
            "<" => ReqType::Lt,
            "!=" => ReqType::Ne,
            "^" => ReqType::Caret,
            "~" => ReqType::Tilde,
            _ => panic!("Problem parsing requirement"),
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

    pub fn from_str(s: &str) -> Result<Self, Box<Error>> {
        let re = Regex::new(
            r"^(\^|~|==|<=|>=|<|>|!=)?(\d{1,9})\.?(?:(?:(\d{1,9})\.?)?\.?(\d{1,9})?)?\.?$",
        )
        .unwrap();

        let caps = re
            .captures(s)
            .expect(&format!("Problem parsing version: {}", s));

        // Only major is required.
        let type_ = match caps.get(1) {
            Some(t) => ReqType::from_str(t.as_str()),
            None => ReqType::Exact,
        };

        let major = match caps.get(2) {
            Some(p) => p.as_str().parse::<u32>().expect("Problem parsing major"),
            //            None => return Box::new(Err("Problem parsing version")),
            None => panic!("Problem parsing version"),
        };

        let minor = match caps.get(3) {
            Some(p) => Some(p.as_str().parse::<u32>().expect("Problem parsing minor")),
            None => None,
        };

        let patch = match caps.get(4) {
            Some(p) => Some(p.as_str().parse::<u32>().expect("Problem parsing patch")),
            None => None,
        };

        Ok(Self {
            major,
            minor,
            patch,
            type_,
        })
    }

    /// From a comma-separated list
    pub fn from_str_multiple(vers: &str) -> Result<Vec<Self>, Box<Error>> {
        Ok(vers
            .split(",")
            .map(|req| Self::from_str(req).expect("Prob parsing a concatonated version req."))
            .collect())
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

    pub fn is_compatible(&self, version: &Version) -> bool {
        // Version being unspecified means any version's acceptable; setting to 0s ensure that.
        // Treat missing minor and patch values as 0
        let minor = self.minor.unwrap_or(0);
        let patch = self.patch.unwrap_or(0);

        let self_version = Version::new(self.major, minor, patch);

        let min = self_version;
        let max;

        match self.type_ {
            ReqType::Exact => &self_version == version,
            ReqType::Gte => &self_version >= version,
            ReqType::Lte => &self_version <= version,
            ReqType::Gt => &self_version > version,
            ReqType::Lt => &self_version < version,
            ReqType::Ne => &self_version != version,
            ReqType::Caret => {
                if self.major > 0 {
                    max = Version::new(self.major + 1, 0, 0);
                } else if self_version.minor > 0 {
                    max = Version::new(0, self_version.minor + 1, 0);
                } else {
                    max = Version::new(0, 0, self_version.patch + 2);
                }
                &min <= version && version < &max
            }
            // For tilde, if minor's specified, can only increment patch.
            // If not, can increment minor or patch.
            ReqType::Tilde => {
                if self.minor.is_some() {
                    max = Version::new(self.major, self_version.minor + 1, 0);
                } else {
                    max = Version::new(self.major + 1, 0, 0);
                }
                &min < version && version < &max
            }
        }
    }
}

/// Includes information for describing a `Python` dependency. Can be used in a dependency
/// graph.
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
    pub fn to_pip_string(&self) -> String {
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
            0 => self.name.clone(),
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
    pub fn from_str(s: &str, pypi_fmt: bool) -> Result<Self, Box<Error>> {
        // Check if no version is specified.
        let novers_re = if pypi_fmt {
            Regex::new(r"^([a-zA-Z\-]+)\s*;.*$").unwrap()
        } else {
            Regex::new(r"^([a-zA-Z]+)$").unwrap()
        };

        match novers_re.captures(s) {
            Some(m) => {
                return Ok(Self {
                    name: m.get(1).unwrap().as_str().to_string(),
                    version_reqs: vec![],
                    dependencies: vec![],
                })
            }
            None => (),
        }

        let re = if pypi_fmt {
            // ie saturn (>=0.3.4)
            Regex::new(r"^(.*?)\s+\((.*)\)(?:\s;.*)?$").unwrap()
        } else {
            //ie saturn = ">=0.3.4"
            Regex::new(r#"^(.*?)\s*=\s*["'](.*)["']$"#).unwrap()
        };
        let caps = match re.captures(s) {
            Some(c) => c,
            //                None => return Err(Box::new()),
            // todo: Figure out how to return an error
            None => panic!(format!("Problem parsing dependency from string: {}", s)),
        };

        let name = caps.get(1).unwrap().as_str().to_string();
        let reqs_m = caps.get(2).unwrap();
        let reqs = VersionReq::from_str_multiple(reqs_m.as_str())
            .expect("Problem parsing version requirement");

        Ok(Self {
            name,
            version_reqs: reqs,
            dependencies: vec![],
        })
    }
}

/// An exact package to install.
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
}
// todo: Implement to/from Lockpack for Package?

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn compat_caret() {
        let req1 = VersionReq {
            major: 1,
            minor: Some(2),
            patch: Some(3),
            type_: ReqType::Caret,
        };
        let req2 = VersionReq {
            major: 0,
            minor: Some(2),
            patch: Some(3),
            type_: ReqType::Caret,
        };
        let req3 = VersionReq {
            major: 0,
            minor: Some(0),
            patch: Some(3),
            type_: ReqType::Caret,
        };

        assert!(req1.is_compatible(&Version::new(1, 9, 9)));
        assert!(!req1.is_compatible(&Version::new(2, 0, 0)));
        assert!(req2.is_compatible(&Version::new(0, 2, 9)));
        assert!(!req2.is_compatible(&Version::new(0, 3, 0)));
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
            type_: ReqType::Caret,
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

        assert_eq!(VersionReq::from_str(a).unwrap(), req_a);
        assert_eq!(VersionReq::from_str(b).unwrap(), req_b);
        assert_eq!(VersionReq::from_str(c).unwrap(), req_c);
        assert_eq!(VersionReq::from_str(d).unwrap(), req_d);
        assert_eq!(VersionReq::from_str(e).unwrap(), req_e);
        assert_eq!(VersionReq::from_str(f).unwrap(), req_f);
    }

    #[test]
    fn parse_dep_novers() {
        let p = Dependency::from_str("saturn", false).unwrap();
        assert_eq!(
            p,
            Dependency {
                name: "saturn".into(),
                version_reqs: vec![],
                dependencies: vec![],
            }
        )
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
                    type_: ReqType::Exact,
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
                    type_: ReqType::Caret,
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
                    type_: ReqType::Tilde,
                }],
                dependencies: vec![],
            }
        )
    }

    #[test]
    fn parse_dep_pypi() {
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
                        type_: ReqType::Ne,
                    },
                    VersionReq {
                        major: 1,
                        minor: Some(25),
                        patch: Some(1),
                        type_: ReqType::Ne,
                    },
                    VersionReq {
                        major: 1,
                        minor: Some(26),
                        patch: None,
                        type_: ReqType::Lte,
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
                type_: ReqType::Exact,
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
                    type_: ReqType::Ne,
                },
                VersionReq {
                    major: 3,
                    minor: Some(7),
                    patch: None,
                    type_: ReqType::Gte,
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
}
