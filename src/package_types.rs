use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{cmp, num, str::FromStr, string::ParseError};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum VersionType {
    Exact,
    Carot,
    Tilde,
}

/// Specifies what range of versions is acceptable for a dependency requirement.
#[derive(Debug)]
pub struct VersionReq {
    pub min: Option<Version>,
    pub max: Option<Version>,
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

impl VersionType {
    pub fn toml_string(&self) -> String {
        match self {
            VersionType::Exact => "".into(),
            VersionType::Carot => "^".into(),
            VersionType::Tilde => "~".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct Version {
    // Attempted to use the semvar crate, but fuctionality/docs are lacking.
    // todo wildcard
    pub major: u32,
    pub minor: u32,
    pub patch: Option<u32>,
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch: Some(patch),
        }
    }

    /// No patch specified.
    pub fn new_short(major: u32, minor: u32) -> Self {
        Self {
            major,
            minor,
            patch: None,
        }
    }

    // todo Notsure why I need this; FromStr's doesn't always work.
    pub fn from_str2(s: &str) -> Self {
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

        Self {
            major,
            minor,
            patch,
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
        match self.patch {
            Some(patch) => format!("{}.{}.{}", self.major, self.minor, patch),
            None => format!("{}.{}", self.major, self.minor),
        }
    }
}

/// This is a thinly-wrapped tuple, which exists so we can implement
/// serialization for the lock file.
pub struct LockVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

//impl Serialize for ExactVersion {
//    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//    where
//        S: Serializer,
//    {
//        // 3 is the number of fields in the struct.
//        let mut s = serializer.serialize_struct("Person", 3)?;
//        state.serialize_field("r", &self.r)?;
//        state.serialize_field("g", &self.g)?;
//        state.serialize_field("b", &self.b)?;
//        state.end()
//    }
//}

/// Includes information for describing a `Python` dependency
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Dependency {
    pub name: String,
    pub version_type: VersionType, // Not used if version not specified.
    // None on version means not specified
    pub version: Option<Version>, // https://semver.org
    // When installing bin packages, we don't use the normal lib directory; we install
    // to the main virtualenv.
    // todo: Remove this if we discover a workaround.
    pub bin: bool,
}

impl Dependency {
    /// Find the version from a selection that's most compatible with this
    /// dependency's requirements.
    pub fn best_match(&self, versions: &[Version]) -> Option<Version> {
        // If no version specified, use the highest available.
        if self.version.is_none() {
            // This logic has to do with derefing the interior of Option.
            return match versions.into_iter().max() {
                Some(v) => Some(v.clone()),
                None => None,
            }
        }

        match self.version_type {
            // For an exact version type, there's only one correct answer.
            VersionType::Exact => {
                let result = versions
                    .into_iter()
                    .filter(|v| *v == &self.version.unwrap())
                    .collect::<Vec<&Version>>();

                let b = result.get(0);

                match b {
                    Some(v) => Some(*v.clone()),
                    None => None,
                }

            },
            // todo implement later.
            VersionType::Tilde => None,
            VersionType::Carot => None,
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

impl FromStr for Dependency {
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
            bin: false,
        })
    }
}
