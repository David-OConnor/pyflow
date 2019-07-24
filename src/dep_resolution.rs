use crate::dep_types::{Dependency, Version, VersionReq};
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;

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
fn get_warehouse_data(name: &str) -> Result<(WarehouseData), Box<Error>> {
    let url = format!("https://pypi.org/pypi/{}/json", name);
    let resp = reqwest::get(&url)?.json()?;
    Ok(resp)
}

fn get_warehouse_versions(name: &str) -> Vec<Version> {
    let data =
        get_warehouse_data(name).expect(&format!("Problem getting warehouse data for {}", name));

    data.releases
        .keys()
        .map(|v| Version::from_str2(v).expect("Problem parsing version while making dep graph"))
        .collect()
}

fn get_warehouse_data_w_version(
    name: &str,
    version: &Version,
) -> Result<(WarehouseData), Box<Error>> {
    let url = format!(
        "https://pypi.org/pypi/{}/{}/json",
        name,
        version.to_string()
    );
    let resp = reqwest::get(&url)?.json()?;
    Ok(resp)
}

/// Find dependencies for a specific version.
fn get_warehouse_deps(name: &str, version: &Version) -> Vec<Dependency> {
    let data = get_warehouse_data_w_version(name, version).expect(&format!(
        "Problem getting warehouse data for {}: {}",
        name,
        version.to_string()
    ));

    data.info
        .requires_dist
        .expect("Can't find distros")
        .into_iter()
        .map(|dep| {
            Dependency::from_str(&dep, true)
                .expect("Problem parsing version while making dep graph")
        })
        .collect()
}

/// Fetch dependency data from our database, where it's cached.
fn get_dep_data(name: &str, version: &Version) -> Result<(Vec<String>), Box<Error>> {
    let url = format!(
        "https://pydeps.herokuapp.com/{}/{}",
        name,
        version.to_string()
    );
    let resp = reqwest::get(&url)?.json()?;
    Ok(resp)
}

/// Filter versions compatible with a set of requirements.
fn filter_compatible(reqs: &Vec<VersionReq>, versions: Vec<Version>) -> Vec<Version> {
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

/// Recursively add all dependencies. Pull avail versions from the PyPi warehouse, and sub-dep
/// requirements from our cached DB
pub fn populate_subdeps(dep: &mut Dependency) {
    let wh_data = get_warehouse_data(&dep.name).expect("Problem getting warehouse data");

    let versions = get_warehouse_versions(&dep.name);
    let compatible_versions = filter_compatible(&dep.version_reqs, versions);

    for vers in compatible_versions {
        let data = get_dep_data(&dep.name, &vers).expect("Can't get dep data");
    }
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
            get_warehouse_versions("scinot").sort(),
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
            get_warehouse_deps("requests", &Version::new(2, 22, 0)),
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
