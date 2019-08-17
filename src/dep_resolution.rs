use crate::dep_types::DependencyError;
use crate::{
    dep_types::{self, Constraint, Dependency, Req, ReqType, Version},
    util,
};
use serde::{Deserialize, Serialize};
use std::cmp::max;
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

/// Find the latest version of a package by querying the warehouse.  Also return
/// a vec of the versions found, so we can reuse this later without fetching a second time.
pub fn get_latest_version(name: &str) -> Result<(Version, Vec<Version>), DependencyError> {
    println!("(dbg) Getting available versions for {}...", name);
    let data = get_warehouse_data(name)?;

    let all_versions = data
        .releases
        .keys()
        .filter(|v| Version::from_str(v).is_ok())
        // todo: way to do this in one step, like filter_map?
        .map(|v| Version::from_str(v).unwrap())
        .collect();

    match Version::from_str(&data.info.version) {
        Ok(v) => Ok((v, all_versions)),
        // Unable to parse the version listed in info; iterate through releases.
        Err(_) => Ok((*all_versions.iter().max().unwrap(), all_versions)),
    }
}

//fn get_warehouse_versions(name: &str) -> Result<Vec<Version>, reqwest::Error> {
//    println!("Getting version data for {}", name);
//    // todo return Result with custom fetch error type
//    let data = get_warehouse_data(name)?;
//
//    let mut result = vec![];
//    for ver in data.releases.keys() {
//        if let Ok(v) = Version::from_str(ver) {
//            // If not Ok, probably due to having letters etc in the name - we choose to ignore
//            // those. Possibly to indicate pre-releases/alpha/beta/release-candidate etc.
//            result.push(v);
//        }
//    }
//    Ok(result)
//}

fn _get_warehouse_data_w_version(
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

    let release_data = release_data.unwrap_or_else(|| {
        panic!(
            "Unable to find release for {} = \"{}\"",
            name,
            version.to_string()
        )
    });

    Ok(release_data.clone())
}

/// Find dependencies for a specific version of a package.
fn _get_warehouse_dep_data(name: &str, version: &Version) -> Result<Dependency, reqwest::Error> {
    let data = _get_warehouse_data_w_version(name, version)?;
    let mut result = Dependency {
        name: name.to_owned(),
        version: *version,
        reqs: vec![],
        extras: vec![],

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

#[derive(Clone, Debug, Deserialize)]
struct ReqCache {
    // Name is present from pydeps if getting deps for multiple package names. Otherwise, we ommit
    // it since we already know the name when making the request.
    name: Option<String>,
    version: String,
    requires_python: Option<String>,
    requires_dist: Vec<String>,
}

impl ReqCache {
    fn reqs(&self) -> Vec<Req> {
        self.requires_dist
            .iter()
            // todo: way to filter ok?
            .filter(|vr| Req::from_str(vr, true).is_ok())
            .map(|vr| Req::from_str(vr, true).unwrap())
            //            .expect("Problem parsing req: ")  // todo how do I do this?
            .collect()
    }
}

/// Fetch dependency data from our database, where it's cached.
fn _get_req_cache(name: &str) -> Result<(Vec<ReqCache>), reqwest::Error> {
    let url = format!("https://pydeps.herokuapp.com/{}", name,);
    Ok(reqwest::get(&url)?.json()?)
}

/// Fetch dependency data from our database, where it's cached.
fn _get_req_cache_single(name: &str, version: &Version) -> Result<Vec<ReqCache>, reqwest::Error> {
    let url = format!(
        "https://pydeps.herokuapp.com/{}/{}",
        name,
        version.to_string()
    );
    Ok(reqwest::get(&url)?.json()?)
}

/// Fetch dependency data from our database, where it's cached.
fn _get_req_cache_range(
    name: &str,
    min_vers: &Version,
    max_vers: &Version,
) -> Result<Vec<ReqCache>, reqwest::Error> {
    println!(
        "Getting pydeps data for {}: {}-{}",
        name,
        min_vers.to_string(),
        max_vers.to_string()
    );
    let url = format!(
        "https://pydeps.herokuapp.com/range/{}/{}/{}",
        name,
        min_vers.to_string(),
        max_vers.to_string()
    );
    Ok(reqwest::get(&url)?.json()?)
}

#[derive(Debug, Serialize)]
struct MultipleBody {
    // name, (version, version). Having trouble implementing Serialize for Version.
    packages: HashMap<String, Vec<String>>,
}

/// Fetch items from multiple packages; cuts down on API calls.
fn get_req_cache_multiple(
    packages: &HashMap<String, Vec<Version>>,
) -> Result<Vec<ReqCache>, reqwest::Error> {
    // input tuple is name, min version, max version.
    println!("(dbg) Getting pydeps data for {:?}", packages);
    // parse strings here.
    let mut packages2 = HashMap::new();
    for (name, versions) in packages.into_iter() {
        let versions = versions.iter().map(|v| v.to_string()).collect();
        packages2.insert(name.to_owned(), versions);
    }

    //            let url = "https://pydeps.herokuapp.com/multiple/";
    let url = "http://localhost:8000/multiple/";

    Ok(reqwest::Client::new()
        .post(url)
        .json(&MultipleBody {
            packages: packages2,
        })
        .send()?
        .json()?)
}

/// Fetch dependency data from our database, where it's cached. Only ask for reqs greater than
/// or equal to a specific version. Used to mitigate caching on the server.
fn _get_req_cache_gte(name: &str, version: &Version) -> Result<(Vec<ReqCache>), reqwest::Error> {
    // todo return Result with custom fetch error type
    let url = format!(
        "https://pydeps.herokuapp.com/gte/{}/{}",
        name,
        version.to_string()
    );
    Ok(reqwest::get(&url)?.json()?)
}

//fn flatten(result: &mut Vec<Dependency>, tree: &Dependency) {
//    for node in tree.dependencies.iter() {
//        // We don't need sub-deps in the result; they're extraneous info. We only really care about
//        // the name and version requirements.
//        let mut result_dep = node.clone();
//        result_dep.dependencies = vec![];
//        result.push(result_dep);
//        flatten(result, &node);
//    }
//}

/// Helper fn for `guess_graph`.
fn filter_compat(constraints: &[Constraint], r: &ReqCache) -> bool {
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
}

/// Pull data on pydeps for a req. Only pull what we need.
/// todo: Group all reqs and pull with a single call to pydeps to improve speed?
fn fetch_req_data(
    reqs: &[Req],
    vers_cache: &mut HashMap<String, (Version, Vec<Version>)>,
) -> Result<Vec<ReqCache>, DependencyError> {
    // Narrow-down our list of versions to query.
    let mut query_data = HashMap::new();
    for req in reqs {
        // todo: cache version info; currently may get this multiple times.
        let (latest_version, all_versions) = match vers_cache.get(&req.name) {
            Some(c) => c.clone(),
            None => {
                let data = get_latest_version(&req.name)?;
                vers_cache.insert(req.name.clone(), data.clone());
                data
            }
        };

        // For no constraints, default to only getting the latest
        let mut min_v_to_query = latest_version;
        let mut max_v_to_query = latest_version;

        for constr in req.constraints.iter() {
            match constr.type_ {
                ReqType::Exact => {
                    // todo: impl add/subtr for version?
                    min_v_to_query = constr.version();
                    max_v_to_query = constr.version();
                    break;
                }
                ReqType::Lt => {
                    max_v_to_query = *all_versions
                        .iter()
                        .filter(|v| **v < constr.version())
                        .max()
                        .expect("Can't find max");
                }
                ReqType::Lte => max_v_to_query = constr.version(),
                ReqType::Gt => {
//                    min_v_to_query = *all_versions
//                        .iter()
//                        .filter(|v| **v > constr.version())
//                        .max()
//                        .expect("Can't find max");
                }
                //                ReqType::Gte => min_v_to_query = constr.version(),
                ReqType::Gte => (), // todo: Need to include more versions to satisfy conflicts
                ReqType::Ne => (),  // todo
                ReqType::Caret => (),
//                ReqType::Caret => min_v_to_query = constr.version(),
                ReqType::Tilde => (),
//                ReqType::Tilde => min_v_to_query = constr.version(),
            }
        }
        let versions_in_rng = all_versions
            .into_iter()
            .filter(|v| min_v_to_query <= *v && *v <= max_v_to_query)
            .collect();
        query_data.insert(req.name.to_owned(), versions_in_rng);
    }

    Ok(get_req_cache_multiple(&query_data)?)
    //    Ok(get_req_cache_single(&req.name, &max_v_to_query)?)
}

// Build a graph: Start by assuming we can pick the newest compatible dependency at each step.
// If unable to resolve this way, subsequently run this with additional deconfliction reqs.
fn guess_graph(
    reqs: &[Req],
    result: &mut Vec<Dependency>,
    cache: &mut HashMap<(String, Version), Vec<Dependency>>,
    vers_cache: &mut HashMap<String, (Version, Vec<Version>)>,
    reqs_searched: &mut Vec<Req>,
) -> Result<(), DependencyError> {
    // Single http call here for all this package's reqs.
    let query_data = match fetch_req_data(reqs, vers_cache) {
        Ok(d) => d,
        Err(e) => {
            util::abort(&format!("Problem getting dependency data: {:?}", e));
            vec![] // todo satisfy match
        }
    };
    //    println!("QUERY DATA: {:#?}", &query_data);
    for req in reqs {
        // todo: Use the extras later.
        if let Some(ex) = &req.extra {
            continue;
            // todo
        }

        // Move on if we've alreaydy satisfied this req.
        //        if reqs_searched.any(|r| r.fully_contains(req)) { // todo
        if reqs_searched.contains(req) {
            continue;
        }
        reqs_searched.push(req.clone());

        // Find matching packages for this requirement.
        let query_result: Vec<&ReqCache> = query_data
            .iter()
            .filter(|d| d.name == Some(req.name.clone()))
            .into_iter()
            .filter(|r| filter_compat(&req.constraints, r))
            .collect();

        let deps: Vec<Dependency> = query_result
            .into_iter()
            .map(|r| {
                Dependency {
                    name: req.name.to_owned(),
                    version: Version::from_str(&r.version).expect("Problem parsing vers"),
                    reqs: r.reqs(),
                    constraints_for_this: req.constraints.clone(),
                    extras: vec![], // todo
                }
            })
            .collect();

        //        for dep in deps.iter() {
        //            cache.insert(
        //                (
        //                    dep.name.clone(),
        //                    dep.version,
        //                ),
        //                dep.reqs.clone(),
        //            );
        //        }

        if deps.is_empty() {
            util::abort(&format!("Can't find a compatible package for {:#?}", &req));
        }

        // Todo: Figure out when newest_compat isn't what you want, due to dealing with
        // todo conflicting sub-reqs.
        let newest_compat = deps
            .into_iter()
            .max_by(|a, b| a.version.cmp(&b.version))
            .expect("Problem finding newest compatible match");

        result.push(newest_compat.clone());

        if let Err(e) = guess_graph(
            &newest_compat.reqs,
            result,
            cache,
            vers_cache,
            reqs_searched,
        ) {
            println!("Problem pulling dependency info for {}", &req.name);
            util::abort(&e.details)
        }
    }
    Ok(())
}

/// Determine which dependencies we need to install, using the newest ones which meet
/// all constraints. Gets data from a cached repo, and Pypi.
//pub fn resolve(tree: &mut DepNode) -> Result<Vec<DepNode>, reqwest::Error> {
pub fn resolve(reqs: &[Req]) -> Result<Vec<(String, Version)>, reqwest::Error> {
    let mut result = Vec::new();
    let mut cache = HashMap::new();
    let mut reqs_searched = Vec::new();

    //    guess_graph(
    //        tree,
    //        &mut reqs_searched,
    //        &mut deps_searched,
    //        &[],
    //        &mut cache,
    //    )
    //    .expect("Unable to resolve dependencies");

    guess_graph(reqs, &mut result, &mut cache, &mut HashMap::new(), &mut reqs_searched)
        .expect("Unable to resolve dependencies");

    //    let mut flattened = vec![];
    //    flatten(&mut flattened, &tree);

    let mut by_name: HashMap<String, Vec<Dependency>> = HashMap::new();

    for dep in result.iter() {
        println!("Resolving {}, {}", dep.name, dep.version.to_string());

        match by_name.get_mut(&dep.name) {
            Some(k) => k.push(dep.clone()),
            None => {
                by_name.insert(dep.name.clone(), vec![dep.clone()]);
            }
        }
    }

    // Deal with duplicates, conflicts etc. The code above assumed no conflicts, and that
    // we can pick the newest compatible version for each req.
    let mut result_cleaned = vec![];
    for (name, deps) in by_name.into_iter() {
        if deps.len() == 1 {
            // This dep is only specified once; no need to resolve conflicts.
            let dep = &deps[0];
            result_cleaned.push((dep.name.to_owned(), dep.version));
        } else {
            let constraints: Vec<Vec<Constraint>> = deps
                .iter()
                .map(|d| d.constraints_for_this.clone())
                .collect();

            let inter = dep_types::intersection_many(&constraints);
            println!("name: {}, inter: {:#?}", name, &inter);

            let newest = deps
                .iter()
                .max_by(|a, b| a.version.cmp(&b.version))
                .expect("Can't find max for newest");
            if inter
                .iter()
                .all(|(min, max)| *min <= newest.version && newest.version <= *max)
            {
                result_cleaned.push((newest.name.to_owned(), newest.version));
                continue;
            }

            for range in inter.iter() {
                let updated_vers = range.1;
            }

            //            let updated_data = get_req_cache_single(&updated_name, &updated_ver);

            //        result_cleaned.push(Dependency {
            //                name: name.to_owned(),
            //                version: updated_vers,
            //                reqs: vec![],  // No longer required.
            //                constraints_for_this: vec![], // todo still required?
            //                extras: vec![],               // todo
            //        });
            //            result_cleaned.push((name.to_owned(), updated_vers));
        }
    }

    //    println!("RESOLVED RESULT: {:#?}", &result);
    //    Ok(result
    //        .into_iter()
    //        .map(|dep| (dep.name, dep.version))
    //        .collect())
    Ok(result_cleaned)
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
            get_latest_version("scinot").unwrap().1.sort(),
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

    //    #[test]
    //    fn warehouse_deps() {
    //        // Makes API call
    //        let req_part = |name: &str, reqs| {
    //            // To reduce repetition
    //            Req::new(name.to_owned(), version_reqs)
    //        };
    //        let vrnew = |t, ma, mi, p| Constraint::new(t, ma, mi, p);
    //        let vrnew_short = |t, ma, mi| Constraint {
    //            type_: t,
    //            major: ma,
    //            minor: Some(mi),
    //            patch: None,
    //        };
    //        use crate::dep_types::ReqType::{Gte, Lt, Ne};

    //        assert_eq!(
    //            _get_warehouse_dep_data("requests", &Version::new(2, 22, 0)).unwrap(),
    //            vec![
    //                req_part("chardet", vec![vrnew(Lt, 3, 1, 0), vrnew(Gte, 3, 0, 2)]),
    //                req_part("idna", vec![vrnew_short(Lt, 2, 9), vrnew_short(Gte, 2, 5)]),
    //                req_part(
    //                    "urllib3",
    //                    vec![
    //                        vrnew(Ne, 1, 25, 0),
    //                        vrnew(Ne, 1, 25, 1),
    //                        vrnew_short(Lt, 1, 26),
    //                        vrnew(Gte, 1, 21, 1)
    //                    ]
    //                ),
    //                req_part("certifi", vec![vrnew(Gte, 2017, 4, 17)]),
    //                req_part("pyOpenSSL", vec![vrnew_short(Gte, 0, 14)]),
    //                req_part("cryptography", vec![vrnew(Gte, 1, 3, 4)]),
    //                req_part("idna", vec![vrnew(Gte, 2, 0, 0)]),
    //                req_part("PySocks", vec![vrnew(Ne, 1, 5, 7), vrnew(Gte, 1, 5, 6)]),
    //                req_part("win-inet-pton", vec![]),
    //            ]
    //        )

    // todo Add more of these, for variety.
    //    }

    // todo: Make dep-resolver tests, including both simple, conflicting/resolvable, and confliction/unresolvable.
}
