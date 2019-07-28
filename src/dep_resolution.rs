use crate::{
    dep_types::{Dependency, Version, VersionReq},
    util,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, Deserialize)]
struct WarehouseInfo {
    requires_dist: Option<Vec<String>>,
    requires_python: Option<String>,
    version: String,
}

#[derive(Debug, Deserialize)]
struct WarehouseRelease {
    // Could use digests field, which has sha256 as well as md5.
    // md5 is faster, and should be good enough.
    has_sig: bool,
    md5_digest: String,
    packagetype: String,
    python_version: String,
    requires_python: Option<String>,
    url: String,
    dependencies: Option<Vec<String>>,
}

//#[derive(Debug, Deserialize)]
//struct WarehouseUrl {
//    // Could use digests field, which has sha256 as well as md5.
//    // md5 is faster, and should be good enough.
//    has_sig: bool,
//    md5_digest: String,
//    packagetype: String,
//    python_version: String,
//    requires_python: Option<String>,
//    url: String,
//}

/// Only deserialize the info we need to resolve dependencies etc.
#[derive(Debug, Deserialize)]
struct WarehouseData {
    info: WarehouseInfo,
    //    releases: Vec<WarehouseRelease>,
    releases: HashMap<String, Vec<WarehouseRelease>>,
    //    urls: Vec<WarehouseUrl>,
}

/// Fetch data about a package from the Pypi Warehouse.
/// https://warehouse.pypa.io/api-reference/json/
fn get_warehouse_data(name: &str) -> Result<WarehouseData, reqwest::Error> {
    let url = format!("https://pypi.org/pypi/{}/json", name);
    let resp = reqwest::get(&url)?.json()?;
    Ok(resp)
}

pub fn get_warehouse_versions(name: &str) -> Result<Vec<Version>, reqwest::Error> {
    // todo return Result with custom fetch error type
    let data = get_warehouse_data(name)?;

    let mut result = vec![];
    for ver in data.releases.keys() {
        match Version::from_str(ver) {
            Ok(v) => result.push(v),
            Err(e) => println!(
                "Problem parsing version: `{} {}` while making dependency graph",
                name, ver
            ),
        }
    }
    Ok(result)
}

fn get_warehouse_data_w_version(
    name: &str,
    version: &Version,
) -> Result<WarehouseData, reqwest::Error> {
    let url = format!(
        "https://pypi.org/pypi/{}/{}/json",
        name,
        version.to_string()
    );
    let resp = reqwest::get(&url)?.json()?;
    Ok(resp)
}

/// Find dependencies for a specific version of a package.
fn get_warehouse_deps(name: &str, version: &Version) -> Result<Vec<Dependency>, reqwest::Error> {
    // todo return Result with custom fetch error type
    let data = get_warehouse_data_w_version(name, version)?;

    let mut result = vec![];
    if let Some(reqs) = data.info.requires_dist {
        for req in reqs {
            match Dependency::from_str(&req, true) {
                Ok(d) => result.push(d),
                Err(_) => println!(
                    "Problem parsing dependency requirement: `{}` while making dependency graph",
                    &req
                ),
            }
        }
    }
    Ok(result)
}

/// Fetch dependency data from our database, where it's cached.
fn get_dep_data(name: &str, version: &Version) -> Result<(Vec<String>), reqwest::Error> {
    // todo return Result with custom fetch error type
    let url = format!(
        "https://pydeps.herokuapp.com/{}/{}",
        name,
        version.to_string()
    );
    let resp = reqwest::get(&url)?.json()?;
    Ok(resp)
}

/// Filter versions compatible with a set of requirements.
pub fn filter_compatible(reqs: &[VersionReq], versions: Vec<Version>) -> Vec<Version> {
    // todo: Test this
    versions
        .into_iter()
        .filter(|v| {
            let mut compat = true;
            for req in reqs {
                if !req.is_compatible(v) {
                    compat = false;
                }
            }
            compat
        })
        .collect()
}

/// Alternative reqs format
pub fn filter_compatible2(reqs: &[(Version, Version)], versions: Vec<Version>) -> Vec<Version> {
    // todo: Test this
    versions
        .into_iter()
        .filter(|v| {
            let mut compat = true;
            for req in reqs {
                if *v > req.1 || *v < req.0 {
                    compat = false;
                }
            }
            compat
        })
        .collect()
}

/// Recursively add all dependencies. Pull avail versions from the PyPi warehouse, and sub-dep
/// requirements from our cached DB
pub fn populate_subdeps(dep: &mut Dependency, cache: &[Dependency]) {
    println!("Getting warehouse versions for {}", &dep.name);
    let versions = match get_warehouse_versions(&dep.name) {
        Ok(v) => v,
        Err(_) => {
            println!("Can't find fdependencies for {}", dep.name);
            return;
        }
    };
    let compatible_versions = filter_compatible(&dep.version_reqs, versions);
    if compatible_versions.is_empty() {
        util::abort(&format!(
            "Can't find a compatible version for {:?}",
            dep.name
        ));
    }

    // todo cache these results.

    // todo: We currently assume the dep graph is resolvable using only the best match.
    // todo: This logic is flawed, but should work in many cases.
    // Let's start with the best match, and see if the tree resolves without conflicts using it.
    let newest_compat = compatible_versions.iter().max().unwrap();
    match get_warehouse_deps(&dep.name, newest_compat) {
        Ok(mut d) => {
            dep.dependencies = d.clone();
            for subdep in d.iter_mut() {
                populate_subdeps(subdep, cache);
            }
        }
        Err(_) => println!(
            "Can't find dependencies for {}: {}",
            dep.name,
            newest_compat.to_string()
        ),
    };
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::dep_types::VersionReq;

    #[test]
    fn warehouse_versions() {
        // Makes API call
        // Assume no new releases since writing this test.
        assert_eq!(
            get_warehouse_versions("scinot").unwrap().sort(),
            vec![
                Version::new(0, 0, 1),
                Version::new(0, 0, 2),
                Version::new(0, 0, 3),
                Version::new(0, 0, 4),
                Version::new(0, 0, 5),
                Version::new(0, 0, 6),
                Version::new(0, 0, 7),
                Version::new(0, 0, 8),
                Version::new(0, 0, 9),
                Version::new(0, 0, 10),
                Version::new(0, 0, 11),
            ]
            .sort()
        );
    }

    #[test]
    fn warehouse_deps() {
        // Makes API call
        let dep_part = |name: &str, version_reqs| {
            // To reduce repetition
            Dependency {
                name: name.to_string(),
                version_reqs,
                dependencies: vec![],
            }
        };
        let vrnew = |t, ma, mi, p| VersionReq::new(t, ma, mi, p);
        let vrnew_short = |t, ma, mi| VersionReq {
            type_: t,
            major: ma,
            minor: Some(mi),
            patch: None,
        };
        use crate::dep_types::ReqType::{Gte, Lt, Ne};

        assert_eq!(
            get_warehouse_deps("requests", &Version::new(2, 22, 0)).unwrap(),
            vec![
                dep_part("chardet", vec![vrnew(Lt, 3, 1, 0), vrnew(Gte, 3, 0, 2)]),
                dep_part("idna", vec![vrnew_short(Lt, 2, 9), vrnew_short(Gte, 2, 5)]),
                dep_part(
                    "urllib3",
                    vec![
                        vrnew(Ne, 1, 25, 0),
                        vrnew(Ne, 1, 25, 1),
                        vrnew_short(Lt, 1, 26),
                        vrnew(Gte, 1, 21, 1)
                    ]
                ),
                dep_part("certifi", vec![vrnew(Gte, 2017, 4, 17)]),
                dep_part("pyOpenSSL", vec![vrnew_short(Gte, 0, 14)]),
                dep_part("cryptography", vec![vrnew(Gte, 1, 3, 4)]),
                dep_part("idna", vec![vrnew(Gte, 2, 0, 0)]),
                dep_part("PySocks", vec![vrnew(Ne, 1, 5, 7), vrnew(Gte, 1, 5, 6)]),
                dep_part("win-inet-pton", vec![]),
            ]
        )

        // todo Add more of these, for variety.
    }
}
