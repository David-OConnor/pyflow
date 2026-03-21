use std::str::FromStr;

use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{tag, take_till, take_while1},
    character::complete::{digit1, space0, space1},
    combinator::{map, map_res, opt, value},
    multi::separated_list0,
    sequence::{delimited, preceded, separated_pair},
};

use crate::{
    dep_types::{Constraint, Extras, Req, ReqType, Version, VersionModifier},
    util::Os,
};

enum ExtrasPart {
    Extra(String),
    SysPlatform(ReqType, Os),
    PythonVersion(Constraint),
}

pub fn parse_req(input: &str) -> IResult<&str, Req> {
    // eg saturn = ">=0.3.4", as in pyproject.toml
    map(
        alt((
            separated_pair(
                parse_package_name,
                (space0, tag("="), space0),
                delimited(quote, parse_constraints, quote),
            ),
            map(parse_package_name, |x| (x, vec![])),
        )),
        |(name, constraints)| Req::new(name.to_string(), constraints),
    )
    .parse(input)
}

pub fn parse_req_pypi_fmt(input: &str) -> IResult<&str, Req> {
    // eg saturn (>=0.3.4) or argon2-cffi (>=16.1.0) ; extra == 'argon2'
    map(
        alt((
            (
                (parse_package_name, opt(parse_install_with_extras)),
                alt((
                    preceded(space0, delimited(tag("("), parse_constraints, tag(")"))),
                    preceded(space0, parse_constraints),
                )),
                opt(preceded((space0, tag(";"), space0), parse_extras)),
            ),
            map(
                (
                    (parse_package_name, opt(parse_install_with_extras)),
                    opt(preceded((space0, tag(";"), space0), parse_extras)),
                ),
                |(x, y)| (x, vec![], y),
            ),
        )),
        |((name, install_with_extras), constraints, extras_opt)| {
            let mut r = if let Some(extras) = extras_opt {
                Req::new_with_extras(name.to_string(), constraints, extras)
            } else {
                Req::new(name.to_string(), constraints)
            };
            r.install_with_extras = install_with_extras;
            r
        },
    )
    .parse(input)
}

pub fn parse_pip_str(input: &str) -> IResult<&str, Req> {
    map(
        (parse_package_name, opt(parse_constraint)),
        |(name, constraint)| Req::new(name.to_string(), constraint.into_iter().collect()),
    )
    .parse(input)
}

pub fn parse_pep508_str(input: &str) -> IResult<&str, Req> {
    // PEP 508 format: name followed directly by zero or more constraints (no `=` separator)
    // eg "requests>=2.0,<3.0" or just "requests"
    map(
        (parse_package_name, parse_constraints),
        |(name, constraints)| Req::new(name.to_string(), constraints),
    )
    .parse(input)
}

pub fn parse_wh_py_vers(input: &str) -> IResult<&str, Vec<Constraint>> {
    alt((
        map(tag("any"), |_| {
            vec![Constraint::new(ReqType::Gte, Version::new(2, 0, 0))]
        }),
        map(tag("source"), |_| {
            vec![Constraint::new(ReqType::Gte, Version::new(2, 0, 0))]
        }),
        map(parse_version, |v| vec![Constraint::new(ReqType::Caret, v)]),
        separated_list0(tag("."), parse_wh_py_ver),
    ))
    .parse(input)
}

fn parse_wh_py_ver(input: &str) -> IResult<&str, Constraint> {
    map(
        (
            alt((tag("cp"), tag("py"), tag("pp"))),
            alt((tag("2"), tag("3"), tag("4"))),
            opt(digit1),
        ),
        |(prefix, major, minor): (&str, &str, Option<&str>)| {
            // <-- Capture `prefix`
            let major: u32 = major.parse().unwrap();

            // Determine constraint type based on the wheel tag prefix
            let req_type = match prefix {
                "py" => ReqType::Gte, // Pure Python is >=
                _ => ReqType::Exact,  // CPython (cp) & PyPy (pp) are exact
            };

            match minor {
                Some(digits) => {
                    let (mi, patch) = if major == 2 && digits.len() > 1 {
                        let mi: u32 = digits[..1].parse().unwrap();
                        let patch: u32 = digits[1..].parse().unwrap();
                        (mi, Some(patch))
                    } else {
                        (digits.parse().unwrap(), None)
                    };
                    Constraint::new(
                        req_type,
                        match patch {
                            Some(p) => Version::new(major, mi, p),
                            None => Version::new_opt(Some(major), Some(mi), None),
                        },
                    )
                }
                None => {
                    if major == 2 {
                        Constraint::new(ReqType::Lte, Version::new_short(2, 10))
                    } else {
                        Constraint::new(ReqType::Gte, Version::new_short(3, 0))
                    }
                }
            }
        },
    )
    .parse(input)
}

fn quote(input: &str) -> IResult<&str, &str> {
    alt((tag("\""), tag("'"))).parse(input)
}

fn parse_install_with_extras(input: &str) -> IResult<&str, Vec<String>> {
    map(
        delimited(
            tag("["),
            separated_list0(tag(","), parse_package_name),
            tag("]"),
        ),
        |extras| extras.iter().map(|x| x.to_string()).collect(),
    )
    .parse(input)
}

pub fn parse_extras(input: &str) -> IResult<&str, Extras> {
    map(
        separated_list0(
            delimited(space0, tag("and"), space0),
            delimited(
                opt(preceded(tag("("), space0)),
                parse_extra_part,
                opt(preceded(space0, tag(")"))),
            ),
        ),
        |ps| {
            let mut extra = None;
            let mut sys_platform = None;
            let mut python_version = None;

            for p in ps {
                match p {
                    ExtrasPart::Extra(s) => extra = Some(s),
                    ExtrasPart::SysPlatform(r, o) => sys_platform = Some((r, o)),
                    ExtrasPart::PythonVersion(c) => python_version = Some(c),
                }
            }

            Extras {
                extra,
                sys_platform,
                python_version,
            }
        },
    )
    .parse(input)
}

fn parse_extra_part(input: &str) -> IResult<&str, ExtrasPart> {
    let (input, type_) =
        alt((tag("extra"), tag("sys_platform"), tag("python_version"))).parse(input)?;
    match type_ {
        "extra" => map(
            preceded(
                separated_pair(space0, tag("=="), space0),
                delimited(quote, parse_package_name, quote),
            ),
            |x| ExtrasPart::Extra(x.to_string()),
        )
        .parse(input),
        "sys_platform" => map(
            (
                delimited(space0, tag("=="), space0),
                delimited(quote, parse_package_name, quote),
            ),
            |(_, o)| ExtrasPart::SysPlatform(ReqType::Exact, Os::from_str(o).unwrap()),
        )
        .parse(input),
        "python_version" => map(
            (
                delimited(space0, parse_req_type, space0),
                delimited(quote, parse_version, quote),
            ),
            |(r, v)| ExtrasPart::PythonVersion(Constraint::new(r, v)),
        )
        .parse(input),
        _ => panic!("Found unexpected extra part type"),
    }
}

pub fn parse_constraints(input: &str) -> IResult<&str, Vec<Constraint>> {
    separated_list0((space0, tag(","), space0), parse_constraint).parse(input)
}

pub fn parse_constraint(input: &str) -> IResult<&str, Constraint> {
    map(
        alt((
            value((Some(ReqType::Gte), Version::new(0, 0, 0)), tag("*")),
            (opt(parse_req_type), preceded(space0, parse_version)),
        )),
        |(r, v)| Constraint::new(r.unwrap_or(ReqType::Exact), v),
    )
    .parse(input)
}

pub fn parse_version(input: &str) -> IResult<&str, Version> {
    let (remain, (major, minor, patch, extra_num)) = (
        parse_digit_or_wildcard,
        opt(preceded(tag("."), parse_digit_or_wildcard)),
        opt(preceded(tag("."), parse_digit_or_wildcard)),
        opt(preceded(tag("."), parse_digit_or_wildcard)),
    )
        .parse(input)?;
    let (remain, modifire) = parse_modifier(remain)?;
    let mut version = Version::new_opt(Some(major), minor, patch);
    version.extra_num = extra_num;
    version.modifier = modifire;
    // check if u32::MAX in any version. (marker for `*`). then set that field
    // and any subsequent fields to `None`
    version.star = vec![Some(major), minor, patch, extra_num].contains(&Some(u32::MAX));
    if version.star {
        if version.major == Some(u32::MAX) {
            version.major = None;
            version.minor = None;
            version.patch = None;
            version.extra_num = None;
            version.modifier = None;
        } else if version.minor == Some(u32::MAX) {
            version.minor = None;
            version.patch = None;
            version.extra_num = None;
            version.modifier = None;
        } else if version.patch == Some(u32::MAX) {
            version.patch = None;
            version.extra_num = None;
            version.modifier = None;
        } else if version.extra_num == Some(u32::MAX) {
            version.extra_num = None;
            version.modifier = None;
        }
    }

    Ok((remain, version))
}

pub fn parse_req_type(input: &str) -> IResult<&str, ReqType> {
    map_res(
        alt((
            tag("=="),
            tag(">="),
            tag("<="),
            tag(">"),
            tag("<"),
            tag("!="),
            tag("^"),
            tag("~="),
            tag("~"),
        )),
        ReqType::from_str,
    )
    .parse(input)
}

fn parse_package_name(input: &str) -> IResult<&str, &str> {
    take_while1(is_package_char).parse(input)
}

fn is_package_char(c: char) -> bool {
    match c {
        '-' => true,
        '.' => true,
        '_' => true,
        _ => c.is_ascii_alphanumeric(),
    }
}

fn parse_digit_or_wildcard(input: &str) -> IResult<&str, u32> {
    map(
        alt((digit1, value("4294967295", tag("*")))),
        |digit: &str| digit.parse().unwrap(),
    )
    .parse(input)
}

fn parse_modifier(input: &str) -> IResult<&str, Option<(VersionModifier, u32)>> {
    opt(map(
        (opt(tag(".")), parse_modifier_version, digit1),
        |(_, version_modifier, n)| (version_modifier, n.parse().unwrap()),
    ))
    .parse(input)
}

fn parse_modifier_version(input: &str) -> IResult<&str, VersionModifier> {
    map(
        take_till(|c: char| !c.is_ascii_alphabetic()),
        |x: &str| match x {
            "a" => VersionModifier::Alpha,
            "b" => VersionModifier::Beta,
            "rc" => VersionModifier::ReleaseCandidate,
            "dep" => VersionModifier::Dep,
            x => VersionModifier::Other(x.to_string()),
        },
    )
    .parse(input)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::dep_types::{Version, VersionModifier};

    #[test]
    fn dummy_test() {}

    #[rstest(input, expected,
        case("*", Ok(("", Constraint::new(ReqType::Gte, Version::new(0, 0, 0))))),
        case("==1.9.2", Ok(("", Constraint::new(ReqType::Exact, Version::new(1, 9, 2))))),
        case("1.9.2", Ok(("", Constraint::new(ReqType::Exact, Version::new(1, 9, 2))))),
        case("~=1.9.2", Ok(("", Constraint::new(ReqType::TildeEq, Version::new(1, 9, 2))))),
    )]
    fn test_parse_constraint(input: &str, expected: IResult<&str, Constraint>) {
        assert_eq!(parse_constraint(input), expected);
    }

    #[rstest(input, expected,
        case("3.12.5", Ok(("", Version {
            major: Some(3),
            minor: Some(12),
            patch: Some(5),
            extra_num: None,
            modifier: None,
            star: false,
        }))),
        case("0.1.0", Ok(("", Version {
            major: Some(0),
            minor: Some(1),
            patch: Some(0),
            extra_num: None,
            modifier: None,
            star: false,
        }))),
        case("3.7", Ok(("", Version {
            major: Some(3),
            minor: Some(7),
            patch: Some(0),
            extra_num: None,
            modifier: None,
            star: false,
        }))),
        case("1", Ok(("", Version {
            major: Some(1),
            minor: Some(0),
            patch: Some(0),
            extra_num: None,
            modifier: None,
            star: false,
        }))),
        case("3.2.*", Ok(("", Version {
            major: Some(3),
            minor: Some(2),
            patch: None,
            extra_num: None,
            modifier: None,
            star: true,
        }))),
        case("1.*", Ok(("", Version {
            major: Some(1),
            minor: None,
            patch: None,
            extra_num: None,
            modifier: None,
            star: true,
        }))),
        case("1.*.*", Ok(("", Version {
            major: Some(1),
            minor: None,
            patch: None,
            extra_num: None,
            modifier: None,
            star: true,
        }))),
        case("19.3", Ok(("", Version {
            major: Some(19),
            minor: Some(3),
            patch: Some(0),
            extra_num: None,
            modifier: None,
            star: false,
        }))),
        case("19.3b0", Ok(("", Version {
                 major: Some(19),
                 minor: Some(3),
                 patch: Some(0),
                 extra_num: None,
                 modifier: Some((VersionModifier::Beta, 0)),
                 star: false,
        }))),
        // This package version showed up in boltons history
        case("0.4.3.dev0", Ok(("", Version {
                 major: Some(0),
                 minor: Some(4),
                 patch: Some(3),
                 extra_num: None,
                 modifier: Some((VersionModifier::Other("dev".to_string()), 0)),
                 star: false,
        }))),
    )]
    fn test_parse_version(input: &str, expected: IResult<&str, Version>) {
        assert_eq!(parse_version(input), expected);
    }

    #[rstest(input, expected,
        case("pyflow", Ok(("", "pyflow"))),
        case("py-flow", Ok(("", "py-flow"))),
        case("py_flow", Ok(("", "py_flow"))),
        case("py.flow", Ok(("", "py.flow"))),
        case("py.flow2", Ok(("", "py.flow2"))),
    )]
    fn test_parse_package_name(input: &str, expected: IResult<&str, &str>) {
        assert_eq!(parse_package_name(input), expected);
    }

    #[rstest(input, expected,
        case(
            "extra == \"test\" and ( python_version == \"2.7\")",
            Ok(("", Extras{
                extra: Some("test".to_string()),
                sys_platform: None,
                python_version: Some(Constraint{ type_: ReqType::Exact, version: Version::new(2, 7, 0)})
            }))
        ),
       case(
            "( python_version == \"2.7\")",
            Ok(("", Extras{
                extra: None,
                sys_platform: None,
                python_version: Some(Constraint{ type_: ReqType::Exact, version: Version::new(2, 7, 0)})
            }))
        ),
       case(
            "python_version == \"2.7\"",
            Ok(("", Extras{
                extra: None,
                sys_platform: None,
                python_version: Some(Constraint{ type_: ReqType::Exact, version: Version::new(2, 7, 0)})
            }))
        ),
        case(
            "( python_version==\"2.7\")",
            Ok(("", Extras{
                extra: None,
                sys_platform: None,
                python_version: Some(Constraint{ type_: ReqType::Exact, version: Version::new(2, 7, 0)})
            }))
        ),
        case(
            "sys_platform == \"win32\" and python_version < \"3.6\"",
            Ok(("", Extras{
                extra: None,
                sys_platform: Some((ReqType::Exact, Os::Windows32)),
                python_version: Some(Constraint{ type_: ReqType::Lt, version: Version::new(3, 6, 0)})
            }))
        ),
    )]
    fn test_parse_extras(input: &str, expected: IResult<&str, Extras>) {
        assert_eq!(parse_extras(input), expected);
    }

    #[rstest(input, expected,
             case::gte("saturn = \">=0.3.4\"", Ok(("", Req::new(
                 "saturn".to_string(),
                 vec![Constraint::new(ReqType::Gte, Version::new(0, 3, 4))])))),
             case::no_version("saturn", Ok(("", Req::new("saturn".to_string(), vec![])))),
             case::star_patch("saturn = \"0.3.*\"", Ok(("", Req::new(
                 "saturn".to_string(),
                 vec![
                     Constraint::new(ReqType::Exact, Version::new_star(Some(0), Some(3), None, true))
                 ]
             )))),
             case::star_extra_num("saturn = \"0.3.4.*\"", Ok(("", Req::new(
                 "saturn".to_string(),
                 vec![
                     Constraint::new(ReqType::Exact, Version::new_star(Some(0), Some(3), Some(4), true))
                 ]
             ))))
    )]
    fn test_parse_req(input: &str, expected: IResult<&str, Req>) {
        assert_eq!(parse_req(input), expected);
    }

    #[rstest(input, expected,
    case("saturn (>=0.3.4)", Ok(("", Req::new("saturn".to_string(), vec![Constraint::new(ReqType::Gte, Version::new(0, 3, 4))])))),
    )]
    fn test_parse_req_pypi(input: &str, expected: IResult<&str, Req>) {
        assert_eq!(parse_req_pypi_fmt(input), expected);
    }
}
