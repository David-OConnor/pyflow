use std::str::FromStr;

use regex::Regex;
use serde::Deserialize;

use crate::dep_types::DependencyError;

#[derive(Copy, Clone, Debug, Deserialize, PartialEq)]
/// Used to determine which version of a binary package to download. Assume 64-bit.
pub enum Os {
    Linux32,
    Linux,
    Windows32,
    Windows,
    //    Mac32,
    Mac,
    Any,
}

impl FromStr for Os {
    type Err = DependencyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re_linux32 = Regex::new(r"(many)?linux.*i686").unwrap();
        let re_linux = Regex::new(r"((many)?linux.*|cygwin|(open)?bsd6*)").unwrap();
        let re_win = Regex::new(r"^win(dows|_amd64)?").unwrap();
        let re_mac = Regex::new(r"(macosx.*|darwin|.*mac.*)").unwrap();

        Ok(match s {
            x if re_linux32.is_match(x) => Self::Linux32,
            x if re_linux.is_match(x) => Self::Linux,
            "win32" => Self::Windows32,
            x if re_win.is_match(x) => Self::Windows,
            x if re_mac.is_match(x) => Self::Mac,
            "any" => Self::Any,
            _ => {
                return Err(DependencyError::new(&format!("Problem parsing Os: {}", s)));
            }
        })
    }
}

pub const fn get_os() -> Os {
    #[cfg(target_os = "windows")]
    return Os::Windows;
    #[cfg(target_os = "linux")]
    return Os::Linux;
    #[cfg(target_os = "macos")]
    return Os::Mac;
}
