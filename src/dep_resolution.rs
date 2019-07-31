use crate::{
    dep_types::{self, Constraint, DepNode, Package, Req, Version},
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
    filename: String,
    has_sig: bool,
    md5_digest: String,
    packagetype: String,
    python_version: String,
    requires_python: Option<String>,
    url: String,
    dependencies: Option<Vec<String>>,
}

/// Only deserialize the info we need to resolve dependencies etc.
#[derive(Debug, Deserialize)]
struct WarehouseData {
    info: WarehouseInfo,
    //    releases: Vec<WarehouseRelease>,
    releases: HashMap<String, Vec<WarehouseRelease>>,
    urls: Vec<WarehouseRelease>,
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
        if let Ok(v) = Version::from_str(ver) {
            // If not Ok, probably due to having letters etc in the name - we choose to ignore
            // those. Possibly to indicate pre-releases/alpha/beta/release-candidate etc.
            result.push(v);
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
fn get_warehouse_dep_data(name: &str, version: &Version) -> Result<DepNode, reqwest::Error> {
    // todo return Result with custom fetch error type
    let data = get_warehouse_data_w_version(name, version)?;
    let mut result = DepNode {
        name: name.to_owned(),
        version: *version,
        reqs: vec![],
        dependencies: vec![],

        hash: "".into(),
        file_url: "".into(),
        filename: "".into(),
    };

    for url in data.urls.iter() {
        if url.packagetype != "bdist_wheel" {
            continue; // todo: Handle missing wheels
        }
        result.file_url = url.url;
        result.filename = url.filename;
        result.hash = url.md5_digest;
        break;
    }

    if let Some(reqs) = data.info.requires_dist {
        for req in reqs {
            match Req::from_str(&req, true) {
                Ok(d) => result.reqs.push(d),
                Err(_) => println!(
                    "Problem parsing dependency requirement: `{}` while making dependency graph",
                    &req
                ),
            }
        }
    }
    Ok(result)
}

// todo: Perhaps just use DepNode etc instead of a special type
#[derive(Debug, Deserialize)]
struct ReqCache {
    version: String,
    requires_python: Option<String>,
    requires_dist: Vec<String>,
}

/// Fetch dependency data from our database, where it's cached.
fn get_req_cache(name: &str) -> Result<(Vec<ReqCache>), reqwest::Error> {
    // todo return Result with custom fetch error type
    let url = format!("https://pydeps.herokuapp.com/{}", name,);
    Ok(reqwest::get(&url)?.json()?)
}

///// Filter versions compatible with a set of requirements.
//pub fn filter_compatible(reqs: &[VersionReq], versions: Vec<Version>) -> Vec<Version> {
//    // todo: Test this
//    versions
//        .into_iter()
//        .filter(|v| {
//            let mut compat = true;
//            for req in reqs {
//                if !req.is_compatible(v) {
//                    compat = false;
//                }
//            }
//            compat
//        })
//        .collect()
//}
//
///// Alternative reqs format
//pub fn filter_compatible2(reqs: &[(Version, Version)], versions: Vec<Version>) -> Vec<Version> {
//    // todo: Test this
//    versions
//        .into_iter()
//        .filter(|v| {
//            let mut compat = true;
//            for req in reqs {
//                if *v > req.1 || *v < req.0 {
//                    compat = false;
//                }
//            }
//            compat
//        })
//        .collect()
//}

//fn resolve_inner() -> {
//
//}

// todo: Overlap with crate::flatten_deps.
fn flatten(result: &mut Vec<DepNode>, tree: &DepNode) {
    for node in tree.dependencies.iter() {
        // We don't need sub-deps in the result; they're extraneous info. We only really care about
        // the name and version requirements.
        let mut result_dep = node.clone();
        result_dep.dependencies = vec![];
        result.push((level, result_dep));
        flatten(result, &node);
    }
}

// Build a graph: Start by assuming we can pick the newest compatible dependency at each step.
// If unable to resolve this way, subsequently run this with additional deconfliction reqs.
fn guess_graph(node: &mut DepNode, deconfliction_reqs: &[Req]) {
    // We gradually add constraits in subsequent iterations of this function, to resolve
    // conflicts as required.
    for req in node.reqs.iter() {
        // todo: Is Depnode the data structure we want here? Lots of unused fields.
        let mut sub_reqs: Vec<DepNode> = get_req_cache(&req.name)?
            .into_iter()
            .filter(|r| {
                // We only care about examining subdependencies that meet our criteria.
                let mut compat = true;
                for constraint in req.constraint {
                    if !constraint.is_compatible(&Version::from_str(&r.version).unwrap()) {
                        compat = false;
                    }
                }
                for decon_req in deconfliction_reqs {
                    for constraint in decon_req.constraints {
                        if !constraint.is_compatible(&Version::from_str(&r.version).unwrap()) {
                            compat = false;
                        }
                    }
                }
                compat
            })
            .map(|r| DepNode {
                name: req.name,
                version: Version::from_str(&r.version).unwrap(),
                reqs: r
                    .requires_dist
                    .iter()
                    .map(|vr| Req::from_str(vr, false).unwrap())
                    .collect(),

                filename: "".into(),
                hash: "".into(),
                file_url: "".into(),
                dependencies: vec![],
            })
            .collect();

        //        cache.append(&mut sub_reqs.clone());
        // todo: Reimplemment cache to cut down on http calls.

        if sub_reqs.is_empty() {
            util::abort(&format!(
                "Can't find a compatible version for {}",
                &req.name
            ));
        }
        let newest_compat = sub_reqs
            .into_iter()
            .max_by(|a, b| a.version.cmp(&b.version))
            .unwrap();
        node.dependencies.push(newest_compat)
    }
}

/// Determine which dependencies we need to install, using the newest ones which meet
/// all constraints. Gets data from a cached repo, and Pypi.
pub fn resolve(tree: &mut DepNode, cache: Vec<DepNode>) -> Result<Vec<Package>, reqwest::Error> {
    // The tree starts as leafless.
    guess_graph(tree, vec![]);
    let mut flattened = vec![];
    flatten(&mut flattened, &tree);

    let mut by_name: HashMap<String, Vec<DepNode>> = HashMap::new(); // todo need annotations

    for dep in flattened {
        if by_name.contains_key(&dep.name) {
            by_name.get(&dep.name).unwrap().push(dep.clone());
        } else {
            by_name.insert(dep.name.clone(), vec![dep.clone()]);
        }
    }

    for (name, deps) in by_name.iter() {
//        if dep.len() <= 1 {
//            continue; // Only specified once; no need to resolve.
//        }  todo put this back if necessary.

        let constraints: Vec<Vec<Constraint>> =
            deps.iter().map(|d| d.constraints_for_this).collect();
        let inter = dep_types::intersection_many(&constraints);
    }

    // Cache stores info for every dep/version we've looked up, while working attempts to build
    // a tree of only what we need.
    //        let mut working = working;
    //        let mut cache = cache;
    //

    //
    //

    //
    //        // Organize by name, so we can compare if there are multiple instances of one
    //        // package required, ie by different parents.

    //
    //        // Check if deps are specified by multiple parents, and if so, pick a version
    //        // that works for all requirements.
    //        for (name, dep) in flattened.iter() {
    //
    //        }

    //    }
    Ok(vec![])
}

///// Recursively add all dependencies. Pull avail versions from the PyPi warehouse, and sub-dep
///// requirements from our cached DB
///// // todo perhaps obsolete.
//pub fn create_dep_tree(reqs: &[Req], node: &mut DepNode, cache: &[DepNode]) {
//    for req in reqs {
//        println!("Getting available versions for {}", &req.name);
//        let versions = match get_warehouse_versions(&req.name) {
//            Ok(v) => v,
//            Err(_) => {
//                util::abort(&format!("Can't find dependencies for {}", &req.name));
//                vec![] // todo makes compile
//            }
//        };
//
//        let compatible_versions = filter_compatible(&req.reqs, versions);
//        if compatible_versions.is_empty() {
//            util::abort(&format!("Can't find a compatible version for {}", &req.name));
//        }
//
//        // todo cache these results.
//
//        // todo: We currently assume the dep graph is resolvable using only the best match.
//        // todo: This logic is flawed, but should work in many cases.
//        // Let's start with the best match, and see if the tree resolves without conflicts using it.
//        let newest_compat = compatible_versions.iter().max().unwrap();
//
//        match get_warehouse_dep_data(&req.name, newest_compat) {
//            Ok(mut d) => {
//                create_dep_tree(subdep, &mut d, cache);
//            }
//            Err(_) => util::abort(&format!(
//                "Can't find dependencies for {}: {}",
//                req.name,
//                newest_compat.to_string())
//            ),
//        };
//    }
//}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::dep_types::Constraint;

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
        let req_part = |name: &str, reqs| {
            // To reduce repetition
            Req::new(name.to_owned(), version_reqs)
        };
        let vrnew = |t, ma, mi, p| Constraint::new(t, ma, mi, p);
        let vrnew_short = |t, ma, mi| Constraint {
            type_: t,
            major: ma,
            minor: Some(mi),
            patch: None,
            suffix: None,
        };
        use crate::dep_types::ReqType::{Gte, Lt, Ne};

        assert_eq!(
            get_warehouse_dep_data("requests", &Version::new(2, 22, 0)).unwrap(),
            vec![
                req_part("chardet", vec![vrnew(Lt, 3, 1, 0), vrnew(Gte, 3, 0, 2)]),
                req_part("idna", vec![vrnew_short(Lt, 2, 9), vrnew_short(Gte, 2, 5)]),
                req_part(
                    "urllib3",
                    vec![
                        vrnew(Ne, 1, 25, 0),
                        vrnew(Ne, 1, 25, 1),
                        vrnew_short(Lt, 1, 26),
                        vrnew(Gte, 1, 21, 1)
                    ]
                ),
                req_part("certifi", vec![vrnew(Gte, 2017, 4, 17)]),
                req_part("pyOpenSSL", vec![vrnew_short(Gte, 0, 14)]),
                req_part("cryptography", vec![vrnew(Gte, 1, 3, 4)]),
                req_part("idna", vec![vrnew(Gte, 2, 0, 0)]),
                req_part("PySocks", vec![vrnew(Ne, 1, 5, 7), vrnew(Gte, 1, 5, 6)]),
                req_part("win-inet-pton", vec![]),
            ]
        )

        // todo Add more of these, for variety.
    }
}
