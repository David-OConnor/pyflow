use crate::{
    dep_types::{
        self, Constraint, Dependency, DependencyError, Package, Rename, Req, ReqType, Version,
    },
    util,
};

use crossterm::Color;
use serde::{Deserialize, Serialize};
use std::cmp::min;
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, Deserialize)]
struct WarehouseInfo {
    name: String, // Pulling this ensure proper capitalization
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

/// Format a name based on how it's listed on PyPi. Ie capitalize or convert - to _'
/// a required.
fn format_name(name: &str, cache: &HashMap<String, (String, Version, Vec<Version>)>) -> String {
    match cache.get(name) {
        Some(vc) => vc.0.clone(),
        None => name.to_owned(), // ie this is from a locked dep.
    }
}

/// Fetch data about a package from the [Pypi Warehouse](https://warehouse.pypa.io/api-reference/json/).
fn get_warehouse_data(name: &str) -> Result<WarehouseData, reqwest::Error> {
    let url = format!("https://pypi.org/pypi/{}/json", name);
    let resp = reqwest::get(&url)?.json()?;
    Ok(resp)
}

/// Find the latest version of a package by querying the warehouse.  Also return
/// a vec of the versions found, so we can reuse this later without fetching a second time.
/// Return name to, so we get correct capitalization.
pub fn get_version_info(name: &str) -> Result<(String, Version, Vec<Version>), DependencyError> {
    let data = get_warehouse_data(name)?;

    let all_versions = data
        .releases
        .keys()
        .filter(|v| Version::from_str(v).is_ok())
        // todo: way to do this in one step, like filter_map?
        .map(|v| Version::from_str(v).expect("Trouble parsing version while getting version info"))
        .collect();

    match Version::from_str(&data.info.version) {
        Ok(v) => Ok((data.info.name, v, all_versions)),
        // Unable to parse the version listed in info; iterate through releases.
        Err(_) => Ok((
            data.info.name,
            *all_versions
                .iter()
                .max()
                .unwrap_or_else(|| panic!("Can't find a valid version for {}", name)),
            all_versions,
        )),
    }
}

/// Get release data from the warehouse, ie the file url, name, and hash.
pub fn get_warehouse_release(
    name: &str,
    version: &Version,
) -> Result<Vec<WarehouseRelease>, reqwest::Error> {
    let data = get_warehouse_data(name)?;

    // If there are 0s in the version, and unable to find one, try 1 and 2 digit versions on Pypi.
    let mut release_data = data.releases.get(&version.to_string2());
    if release_data.is_none() && version.patch == 0 {
        release_data = data.releases.get(&version.to_string_med());
        if release_data.is_none() && version.minor == 0 {
            release_data = data.releases.get(&version.to_string_short());
        }
    }

    let release_data = release_data
        .unwrap_or_else(|| panic!("Unable to find a release for {} = \"{}\"", name, version));

    Ok(release_data.clone())
}

#[derive(Clone, Debug, Deserialize)]
struct ReqCache {
    // Name is present from pydeps if gestruct packagetting deps for multiple package names. Otherwise, we ommit
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
    // parse strings here.
    let mut packages2 = HashMap::new();
    for (name, versions) in packages.iter() {
        let versions = versions.iter().map(Version::to_string2).collect();
        packages2.insert(name.to_owned(), versions);
    }

    let url = "https://pydeps.herokuapp.com/multiple/";
    //            let url = "http://localhost:8000/multiple/";

    Ok(reqwest::Client::new()
        .post(url)
        .json(&MultipleBody {
            packages: packages2,
        })
        .send()?
        .json()?)
}

/// Helper fn for `guess_graph`.
fn is_compat(constraints: &[Constraint], vers: &Version) -> bool {
    for constraint in constraints.iter() {
        if !constraint.is_compatible(&vers) {
            return false;
        }
    }
    true
}

/// Pull data on pydeps for a req. Only pull what we need.
/// todo: Group all reqs and pull with a single call to pydeps to improve speed?
fn fetch_req_data(
    reqs: &[Req],
    vers_cache: &mut HashMap<String, (String, Version, Vec<Version>)>,
) -> Result<Vec<ReqCache>, DependencyError> {
    // Narrow-down our list of versions to query.

    let mut query_data = HashMap::new();
    for req in reqs {
        // todo: cache version info; currently may get this multiple times.
        let (_, latest_version, all_versions) = match vers_cache.get(&req.name) {
            Some(c) => c.clone(),
            None => {
                match get_version_info(&req.name) {
                    Ok(data) => {
                        vers_cache.insert(req.name.clone(), data.clone());
                        data
                    }
                    Err(_) => {
                        util::abort(&format!(
                            "Can't get version info for the dependency `{}`. \
                             Is it spelled correctly? Is the internet connection ok?",
                            &req.name
                        ));
                        ("".to_string(), Version::new(0, 0, 0), vec![]) // match-compatibility placeholder
                    }
                }
            }
        };

        let mut max_v_to_query = latest_version;

        // Find the maximum version compatible with the constraints.
        // todo: May need to factor in additional constraints here, and put
        // todo in fn signature for things that don't resolve with the optimal soln.
        for constr in &req.constraints {
            // For Ne, we have two ranges; the second one being ones higher than the version specified.
            // For other types, we only have one item in the compatible range.
            let i = match constr.type_ {
                ReqType::Ne => 1,
                _ => 0,
            };

            // Ensure we don't query past the latest.
            max_v_to_query = min(constr.compatible_range()[i].1, max_v_to_query);
        }

        // To minimimize request time, only query the latest compatible version.
        let best_version = match all_versions
            .into_iter()
            .filter(|v| *v <= max_v_to_query)
            .max()
        {
            Some(v) => vec![v],
            None => vec![],
        };

        query_data.insert(req.name.to_owned(), best_version);
    }

    if query_data.is_empty() {
        return Ok(vec![]);
    }

    Ok(get_req_cache_multiple(&query_data)?)
}

// Build a graph: Start by assuming we can pick the newest compatible dependency at each step.
// If unable to resolve this way, subsequently run this with additional deconfliction reqs.
fn guess_graph(
    parent_id: u32,
    reqs: &[Req],
    locked: &[crate::Package],
    os: util::Os,
    extras: &[String],
    py_vers: &Version,
    result: &mut Vec<Dependency>, // parent id, self id.
    cache: &mut HashMap<(String, Version), Vec<&ReqCache>>,
    vers_cache: &mut HashMap<String, (String, Version, Vec<Version>)>,
    reqs_searched: &mut Vec<Req>,
) -> Result<(), DependencyError> {
    let reqs: Vec<&Req> = reqs
        .iter()
        // If we've already satisfied this req, don't query it again. Otherwise we'll make extra
        // http calls, and could end up in infinite loops.
        .filter(|r| !reqs_searched.contains(*r))
        .filter(|r| match &r.extra {
            Some(ex) => extras.contains(ex),
            None => true,
        })
        .filter(|r| match r.sys_platform {
            Some((rt, os_)) => match rt {
                ReqType::Exact => os_ == os,
                ReqType::Ne => os_ != os,
                _ => {
                    util::abort("Reqtypes for Os must be == or !=");
                    unreachable!()
                }
            },
            None => true,
        })
        .filter(|r| match &r.python_version {
            Some(v) => v.is_compatible(py_vers),
            None => true,
        })
        .collect();

    let mut non_locked_reqs = vec![];
    let mut locked_reqs: Vec<Req> = vec![];

    // Partition reqs into ones we have lock-file data for, and ones where we need to make
    // http calls to the pypi warehouse (for versions) and pydeps (for deps).
    for req in &reqs {
        reqs_searched.push((*req).clone());

        let mut found_in_locked = false;
        for package in locked.iter() {
            if !util::compare_names(&package.name, &req.name) {
                continue;
            }

            if is_compat(&req.constraints, &package.version) {
                locked_reqs.push((*req).clone());
                found_in_locked = true;
                break;
            }
        }
        if !found_in_locked {
            non_locked_reqs.push((*req).clone());
        }
    }

    // Single http call here to pydeps for all this package's reqs, plus version calls for each req.
    let mut query_data = match fetch_req_data(&non_locked_reqs, vers_cache) {
        Ok(d) => d,
        Err(e) => {
            util::abort(&format!("Problem getting dependency data: {:?}", e));
            unreachable!()
        }
    };

    // Now add info from lock packs for data we didn't query. The purpose of passing locks
    // into the dep resolution process is to avoid unecessary HTTP calls and resolution iterations.
    for req in locked_reqs {
        // Find the corresponding lock package. There should be exactly one.
        let package = locked
            .iter()
            .find(|p| util::compare_names(&p.name, &req.name))
            .expect("Can't find matching lock package");

        let requires_dist = package
            .deps
            .iter()
            .map(|(_, name, vers)| format!("{} (=={})", name, vers.to_string()))
            .collect();

        // Note that we convert from normal data types to strings here, for the sake of consistency
        // with the http call results.
        query_data.push(ReqCache {
            name: Some(package.name.clone()),
            version: package.version.to_string(),
            requires_python: None,
            requires_dist,
        });
    }

    // todo: We must take locked ids into account, or will bork renames on subsequent runs!

    // We've now merged the query data with locked data. A difference though, is we've already
    // narrowed down the locked ones to one version with an exact constraint.

    for req in &reqs {
        // Find matching packages for this requirement.
        let query_result: Vec<&ReqCache> = query_data
            .iter()
            .filter(|d| util::compare_names(d.name.as_ref().unwrap(), &req.name))
            .collect();

        let deps: Vec<Dependency> = query_result
            .into_iter()
            // Our query data should already be compat, but QC here.
            .filter(|r| is_compat(&req.constraints, &Version::from_str(&r.version).unwrap()))
            .map(|r| Dependency {
                id: result.iter().map(|d| d.id).max().unwrap_or(0) + 1,
                name: req.name.to_owned(),
                version: Version::from_str(&r.version).expect("Problem parsing vers"),
                reqs: r.reqs(),
                parent: parent_id,
            })
            .collect();

        if deps.is_empty() {
            util::abort(&format!("Can't find a compatible package for {:?}", &req));
        }

        let newest_compat = deps
            .into_iter()
            .max_by(|a, b| a.version.cmp(&b.version))
            .expect("Problem finding newest compatible match");

        result.push(newest_compat.clone());

        if let Err(e) = guess_graph(
            newest_compat.id,
            &newest_compat.reqs,
            locked,
            os,
            &req.install_with_extras.as_ref().unwrap_or(&vec![]),
            py_vers,
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

fn find_constraints(
    all_reqs: &[Req],
    all_deps: &[Dependency],
    relevant_deps: &[Dependency],
) -> Vec<Constraint> {
    let mut result = vec![];

    for dep in relevant_deps.iter() {
        let parent = match all_deps.iter().find(|d| d.id == dep.parent) {
            Some(p) => p.clone(),
            // ie top-level; set up a dummy
            None => Dependency {
                id: 999,
                name: "top".to_owned(),
                version: Version::new(0, 0, 0),
                reqs: all_reqs.to_vec(),
                parent: 0,
            },
        };

        for req in parent
            .clone()
            .reqs
            .iter()
            .filter(|r| util::compare_names(&r.name, &dep.name))
        {
            result.append(&mut req.constraints.clone())
        }
    }
    result
}

/// We've determined we need to add all the included packages, and renamed all but one.
fn make_renamed_packs(
    _vers_cache: &HashMap<String, (String, Version, Vec<Version>)>,
    deps: &[Dependency],
    //    all_deps: &[Dependency],
    name: &str,
) -> Vec<Package> {
    util::print_color(
        &format!(
            "Installing multiple versions for {}. If this package uses \
             compiled code or importlib, this may fail when importing. Note that \
             your package may not be published unless this is resolved...",
            name
        ),
        Color::DarkYellow,
    );

    let dep_display: Vec<String> = deps
        .iter()
        .map(|d| {
            format!(
                "name: {}, version: {}, parent: {:?}",
                d.name, d.version, d.parent
            )
        })
        .collect();
    println!("Installing these versions: {:#?}", &dep_display);

    let mut result = vec![];
    // We were unable to resolve using the newest version; add and rename packages.
    for (i, dep) in deps.iter().enumerate() {
        // Don't rename the first one.
        let rename = if i != 0 {
            Rename::Yes(dep.parent, dep.id, format!("{}_renamed_{}", dep.name, i))
        } else {
            Rename::No
        };

        result.push(Package {
            id: dep.id,
            parent: dep.parent,
            name: dep.name.clone(),
            version: dep.version,
            deps: vec![], // to be filled in after resolution
            rename,
        });
    }
    result
}

/// Assign dependencies to packages-to-install, for use in the lock file.
/// Do this only after the dependencies are resolved.
fn assign_subdeps(packages: &mut Vec<Package>, updated_ids: &HashMap<u32, u32>) {
    // We run through the non-cleaned deps first, since the parent may point to
    // one that didn't make the cut, including cases where the versions were identical.
    let packs2 = packages.clone(); // to search
    for package in packages.iter_mut() {
        let mut children: Vec<(u32, String, Version)> = packs2
            .iter()
            .filter(|p| {
                // If there were multiple instances of this dep, the parent id may have been updated.
                let parent_id = match updated_ids.get(&p.parent) {
                    Some(updated_parent) => *updated_parent,
                    None => p.parent,
                };
                parent_id == package.id
            })
            .map(|child| (child.id, child.name.clone(), child.version))
            .collect();
        package.deps.append(&mut children);
    }
}

/// Determine which dependencies we need to install, using the newest ones which meet
/// all constraints. Gets data from a cached repo, and Pypi. Returns name, version, and name/version of its deps.
pub fn resolve(
    reqs: &[Req],
    locked: &[crate::Package],
    os: util::Os,
    py_vers: &Version,
    //) -> Result<Vec<(String, Version, Vec<Req>)>, reqwest::Error> {
) -> Result<Vec<crate::Package>, reqwest::Error> {
    let mut result = Vec::new();
    let mut cache = HashMap::new();
    let mut reqs_searched = Vec::new();

    let mut version_cache = HashMap::new();
    if guess_graph(
        0,
        reqs,
        locked,
        os,
        &[],
        py_vers,
        &mut result,
        &mut cache,
        &mut version_cache,
        &mut reqs_searched,
    )
    .is_err()
    {
        util::abort("Problem resolving dependencies");
    }

    let mut by_name: HashMap<String, Vec<Dependency>> = HashMap::new();
    for mut dep in result.clone().into_iter() {
        // The formatted name may be different from the pypi one. Eg `IPython` vice `ipython`.
        let fmtd_name = format_name(&dep.name, &version_cache);
        dep.name = fmtd_name.clone();

        if let Some(k) = by_name.get_mut(&dep.name) {
            k.push(dep)
        } else {
            by_name.insert(fmtd_name, vec![dep]);
        }
    }

    // Deal with duplicates, conflicts etc. The code above assumed no conflicts, and that
    // we can pick the newest compatible version for each req. We pass only the info
    // needed to build the locked dependencies, and strip intermediary info like ids.

    // updated_ids is used to remap lockpack dependencies, when a dep(version) other than their
    // parent is chosen for the package.
    let mut updated_ids = HashMap::new();
    let mut result_cleaned = vec![];
    for (name, deps) in &by_name {
        let fmtd_name = format_name(&name, &version_cache);

        if deps.len() == 1 {
            // This dep is only specified once; no need to resolve conflicts.
            let dep = &deps[0];

            result_cleaned.push(Package {
                id: dep.id,
                parent: dep.parent,
                name: fmtd_name,
                version: dep.version,
                deps: vec![], // to be filled in after resolution
                rename: Rename::No,
            });
        } else if deps.len() > 1 {
            // Find what constraints are driving each dep that shares a name.
            let constraints = find_constraints(reqs, &result, &deps);

            let _names: Vec<String> = deps.iter().map(|d| d.version.to_string()).collect();
            let inter = dep_types::intersection_many(&constraints);

            if inter.is_empty() {
                result_cleaned.append(&mut make_renamed_packs(&version_cache, &deps, &fmtd_name));
                continue;
            }

            // If a version we've examined meets all constraints for packages that use it, use it -
            // we've already built the graph to accomodate its sub-deps.

            // If unable, find the highest version that meets the constraints, and determine
            // what its dependencies are.

            // Otherwise install all,
            // and rename as-required(By which criteria? the older one?). This ensures our
            // graph is always resolveable, and avoids diving through the graph recursively,
            // dealing with cycles etc. There may be ways around this in some cases.
            // todo: Renaming may not work if the renamed dep uses compiled code.

            let newest_compatible = deps
                .iter()
                .filter(|dep| {
                    inter
                        .iter()
                        .any(|i| i.0 <= dep.version && dep.version <= i.1)
                })
                .max_by(|a, b| a.version.cmp(&b.version));

            match newest_compatible {
                Some(best) => {
                    result_cleaned.push(Package {
                        id: best.id,
                        parent: best.parent,
                        name: fmtd_name,
                        version: best.version,
                        deps: vec![], // to be filled in after resolution
                        rename: Rename::No,
                    });

                    // Indicate we need to update the parent. We can't do it here, since
                    // we don't know if we're pr
                    // ocessed the parent[s] yet. Not doing this will
                    // result in incorrect dependencies listed in lock packs.
                    for dep in deps {
                        // note that we push the old ids, so we can update the subdeps with the new versions.
                        //                        updated_ids.insert(dep.id, best.id).expect("Problem inserting updated id");
                        updated_ids.insert(dep.id, best.id);
                    }
                }

                None => {
                    // We consider the possibility there's a compatible version
                    // that wasn't one of the best-per-req we queried.
                    println!("⛏️ Digging deeper to resolve dependencies for {}...", name);

                    // I think we should query with the raw name, not fmted?
                    let versions = &version_cache.get(name).unwrap().2;

                    if versions.is_empty() {
                        result_cleaned.append(&mut make_renamed_packs(
                            &version_cache,
                            &deps,
                            //                            &result,
                            &fmtd_name,
                        ));
                        continue;
                    }

                    // Generate dependencies here for all avail versions.
                    let unresolved_deps: Vec<Dependency> = versions
                        .iter()
                        .filter(|vers| inter.iter().any(|i| i.0 <= **vers && **vers <= i.1))
                        .map(|vers| Dependency {
                            id: 0, // placeholder; we'll assign an id to the one we pick.
                            name: fmtd_name.clone(),
                            version: *vers,
                            reqs: vec![], // todo
                            parent: 0,    // todo
                        })
                        .collect();

                    let mut newest_unresolved = unresolved_deps
                        .into_iter()
                        .max_by(|a, b| a.version.cmp(&b.version))
                        .unwrap();

                    newest_unresolved.id = result.iter().map(|d| d.id).max().unwrap_or(0) + 1;

                    result_cleaned.push(Package {
                        id: newest_unresolved.id,
                        parent: newest_unresolved.parent,
                        name: fmtd_name,
                        version: newest_unresolved.version,
                        deps: vec![], // to be filled in after resolution
                        rename: Rename::No,
                    });

                    // todo: Do a check on newest_unresolved! If fails, execute renamed plan

                    for dep in deps {
                        // note that we push the old ids, so we can update the subdeps with the new versions.
                        updated_ids.insert(dep.id, newest_unresolved.id);
                    }
                }
            }
        } else {
            panic!("We shouldn't be seeing this!")
        }
    }

    // Now, assign subdeps, so we can store them in the lock.
    assign_subdeps(&mut result_cleaned, &updated_ids);

    let mut a = result;
    for b in &mut a {
        b.reqs = vec![];
    }

    Ok(result_cleaned)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn warehouse_versions() {
        // Makes API call
        // Assume no new releases since writing this test.
        assert_eq!(
            get_version_info("scinot").unwrap().2.sort(),
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
