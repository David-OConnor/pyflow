use crate::{
    dep_types::{self, Constraint, DepNode, Req, ReqType, Version},
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

#[derive(Clone, Debug, Deserialize)]
pub struct WarehouseDigests {
    pub md5: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WarehouseRelease {
    // Could use digests field, which has sha256 as well as md5.
    pub filename: String,
    pub has_sig: bool,
    pub digests: WarehouseDigests,
    pub packagetype: String,
    pub python_version: String,
    pub requires_python: Option<String>,
    pub url: String,
    pub dependencies: Option<Vec<String>>,
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
    println!("Getting warehouse data for {}", name);
    let url = format!("https://pypi.org/pypi/{}/json", name);
    let resp = reqwest::get(&url)?.json()?;
    Ok(resp)
}

/// Find the latest version of a package by querying the warehouse.
pub fn get_latest_version(name: &str) -> Result<Version, reqwest::Error> {
    let data = get_warehouse_data(name)?;
    Ok(Version::from_str(&data.info.version).expect(&format!(
        "Problem parsing version from the warehouse: {} {}",
        name, data.info.version
    )))
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
    println!(
        "Getting warehouse data for {} {}",
        name,
        version.to_string()
    );
    let url = format!(
        "https://pypi.org/pypi/{}/{}/json",
        name,
        version.to_string()
    );
    let resp = reqwest::get(&url)?.json()?;
    Ok(resp)
}

/// Get release data from the warehouse, ie the file url, name, and hash.
pub fn get_warehouse_release(
    name: &str,
    version: &Version,
) -> Result<Vec<WarehouseRelease>, reqwest::Error> {
    let data = get_warehouse_data(name)?;

    // If there are 0s in the version, and unable to find one, try 1 and 2 digit versions on Pypi.
    let mut release_data = data.releases.get(&version.to_string());
    if release_data.is_none() && version.patch == 0 {
        release_data = data.releases.get(&version.to_string_med());
        if release_data.is_none() && version.minor == 0 {
            release_data = data.releases.get(&version.to_string_short());
        }
    }

    let release_data = release_data.expect(&format!(
        "Unable to find release for {} = \"{}\"",
        name,
        version.to_string()
    ));

    // todo: We need to find the right release! Notably for binary crates on diff oses.
    // todo: For now, go with the first while prototyping.

    Ok(release_data.clone())
}

/// Find dependencies for a specific version of a package.
fn get_warehouse_dep_data(name: &str, version: &Version) -> Result<DepNode, reqwest::Error> {
    let data = get_warehouse_data_w_version(name, version)?;
    let mut result = DepNode {
        name: name.to_owned(),
        version: *version,
        reqs: vec![],
        dependencies: vec![],

        constraints_for_this: vec![],
    };

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
#[derive(Clone, Debug, Deserialize)]
struct ReqCache {
    //    #[serde(default)] // We'll populate it after the fetch.
    //    name: String,
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

/// Fetch dependency data from our database, where it's cached. Only ask for reqs greater than
/// or equal to a specific version. Used to mitigate caching on the server.
fn get_req_cache_gte(name: &str, version: Version) -> Result<(Vec<ReqCache>), reqwest::Error> {
    // todo return Result with custom fetch error type
    let url = format!(
        "https://pydeps.herokuapp.com/gte/{}/{}",
        name,
        version.to_string()
    );
    Ok(reqwest::get(&url)?.json()?)
}

fn flatten(result: &mut Vec<DepNode>, tree: &DepNode) {
    for node in tree.dependencies.iter() {
        // We don't need sub-deps in the result; they're extraneous info. We only really care about
        // the name and version requirements.
        let mut result_dep = node.clone();
        result_dep.dependencies = vec![];
        result.push(result_dep);
        flatten(result, &node);
    }
}

// Build a graph: Start by assuming we can pick the newest compatible dependency at each step.
// If unable to resolve this way, subsequently run this with additional deconfliction reqs.
fn guess_graph(
    node: &mut DepNode,
    reqs_searched: &mut Vec<Req>,
    deps_searched: &mut Vec<(String, Version)>, // name, version
    deconfliction_reqs: &[Req],
    cache: &mut HashMap<String, Vec<ReqCache>>,
) -> Result<(), reqwest::Error> {
    // deconfliction_reqs: We gradually add constraits in subsequent iterations of this function, to resolve
    // conflicts as required.

    // reqs_searched is a cache of nodes we've already searched, so we know to skip over in the future
    // deps_searched is a cache of specific package/version combo's we've searched; similar idea.

    //    println!(
    //        "DEBUG: IN guess graph for {:#?} {:?}",
    //        node.name,
    //        node.version.to_string()
    //    );

    deps_searched.push((node.name.clone(), node.version));

    let filter_compat = |constraints: &[Constraint], r: &ReqCache| {
        for constraint in constraints.iter() {
            if let Ok(v) = Version::from_str(&r.version) {
                if !constraint.is_compatible(&v) {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    };

    for req in node.reqs.iter() {
        // todo: Perhaps use the suffix later.
        if req.suffix.is_some() || reqs_searched.contains(&req) {
            continue;
        }
        reqs_searched.push(req.clone());
        // Get subdependency info on all versions of this requirement.
        let info = match cache.get(&req.name) {
            Some(r) => r.clone(),
            None => {
                // http call and cache
                //                let subreq = get_req_cache(&req.name)?; // todo

                // todo: For now, we're only pulling info for versions gte our min req.
                // todo: Not robust / may break.

                let mut min_versions = vec![];
                for constr in req.constraints.iter() {
                    match constr.type_ {
                        // todo not good logic!
                        ReqType::Exact => min_versions.push(constr.version()),
                        ReqType::Lt => min_versions.push(constr.version()),
                        ReqType::Lte => min_versions.push(constr.version()),
                        ReqType::Gt => min_versions.push(constr.version()),
                        ReqType::Gte => min_versions.push(constr.version()),
                        ReqType::Ne => min_versions.push(constr.version()),
                        ReqType::Caret => min_versions.push(constr.version()),
                        ReqType::Tilde => min_versions.push(constr.version()),
                    }
                }
                if req.constraints.is_empty() {
                    min_versions
                        .push(get_latest_version(&req.name).expect("Problem finding latest v"));
                }

                let subreq = get_req_cache_gte(&req.name, *min_versions.iter().min().unwrap())?;
                cache.insert(req.name.to_owned(), subreq.clone());
                subreq
            }
        };

        let info1 = info.into_iter().filter(|r| {
            // We only care about examining subdependencies that meet our criteria.
            let mut compat = filter_compat(&req.constraints, r);
            if compat {
                for decon_req in deconfliction_reqs {
                    if !filter_compat(&decon_req.constraints, r) {
                        compat = false;
                    }
                }
            }
            compat
        });

        let mut sub_reqs = vec![];
        for r in info1 {
            // We may not be able to parse the version if it's something like "2004a".
            if let Ok(v) = Version::from_str(&r.version) {
                sub_reqs.push(DepNode {
                    name: req.name.to_owned(),
                    version: v,
                    reqs: r
                        .requires_dist
                        .iter()
                        .map(|vr| Req::from_str(vr, true).expect("Problem parsing req from string"))
                        .collect(),

                    constraints_for_this: req.constraints.clone(),
                    dependencies: vec![],
                });
            }
        }

        if sub_reqs.is_empty() {
            util::abort(&format!(
                "Can't find a compatible version for {}",
                &req.name
            ));
        }

        // Todo: Figure out when newest_compat isn't what you want, due to dealing with
        // todo conflicting sub-reqs.
        let newest_compat = sub_reqs
            .into_iter()
            .max_by(|a, b| a.version.cmp(&b.version))
            .expect("Problem finding newest compatible match");

        node.dependencies.push(newest_compat);

        for mut dep in node.dependencies.iter_mut() {
            // Without this check, we could get into infinite recursions with circular references,
            // ie a requires b which requires c which requires a.
            let mut searched_all_reqs = true;
            for req in dep.reqs.iter() {
                if !reqs_searched.contains(&req) {
                    searched_all_reqs = false;
                    break;
                }
            }
            if !searched_all_reqs {
                // todo
            }

            if deps_searched.contains(&(dep.name.clone(), dep.version)) {
                continue;
            }
            guess_graph(
                &mut dep,
                reqs_searched,
                deps_searched,
                deconfliction_reqs,
                cache,
            )
            .expect("Unable to resolve dependencies (recursion)");
        }
    }
    Ok(())
}

/// Determine which dependencies we need to install, using the newest ones which meet
/// all constraints. Gets data from a cached repo, and Pypi.
pub fn resolve(tree: &mut DepNode) -> Result<Vec<DepNode>, reqwest::Error> {
    // The tree starts as leafless.
    let mut reqs_searched = Vec::new();
    let mut deps_searched = Vec::new();
    let mut cache = HashMap::new();
    guess_graph(
        tree,
        &mut reqs_searched,
        &mut deps_searched,
        &vec![],
        &mut cache,
    )
    .expect("Unable to resolve dependencies");

    let mut flattened = vec![];
    flatten(&mut flattened, &tree);

    let mut by_name: HashMap<String, Vec<DepNode>> = HashMap::new();

    for dep in flattened.iter() {
        println!("Resolving {}, {}", dep.name, dep.version.to_string());

        match by_name.get_mut(&dep.name) {
            Some(k) => k.push(dep.clone()),
            None => {
                by_name.insert(dep.name.clone(), vec![dep.clone()]);
            }
        }
    }

    for (name, deps) in by_name.iter() {
        let constraints: Vec<Vec<Constraint>> = deps
            .iter()
            .map(|d| d.constraints_for_this.clone())
            .collect();
        let inter = dep_types::intersection_many(&constraints);
    }

    Ok(flattened)
}

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

    // todo: Make dep-resolver tests, including both simple, conflicting/resolvable, and confliction/unresolvable.
}
