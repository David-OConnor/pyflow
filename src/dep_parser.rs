use std::str::FromStr;

use nom::branch::alt;
use nom::bytes::complete::{tag, take};
use nom::character::complete::{digit1, space0, space1};
use nom::combinator::{flat_map, map, map_parser, map_res, opt, value};
use nom::multi::separated_list;
use nom::sequence::{delimited, preceded, separated_pair, tuple};
use nom::{AsChar, IResult, InputTakeAtPosition};

use crate::dep_types::{Constraint, Extras, Req, ReqType, Version, VersionModifier};
use crate::util::Os;

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
                tuple((space0, tag("="), space0)),
                delimited(quote, parse_constraints, quote),
            ),
            map(parse_package_name, |x| (x, vec![])),
        )),
        |(name, constraints)| Req::new(name.to_string(), constraints),
    )(input)
}

pub fn parse_req_pypi_fmt(input: &str) -> IResult<&str, Req> {
    // eg saturn (>=0.3.4) or argon2-cffi (>=16.1.0) ; extra == 'argon2'
    // Note: We specify what chars are acceptable in a name instead of using
    // wildcard, so we don't accidentally match a semicolon here if a
    // set of parens appears later. The non-greedy ? in the version-matching
    // expression's important as well, in some cases of extras.
    map(
        alt((
            tuple((
                tuple((parse_package_name, opt(parse_install_with_extras))),
                alt((
                    preceded(space0, delimited(tag("("), parse_constraints, tag(")"))),
                    preceded(space1, parse_constraints),
                )),
                opt(preceded(tuple((space0, tag(";"), space0)), parse_extras)),
            )),
            map(
                tuple((
                    tuple((parse_package_name, opt(parse_install_with_extras))),
                    opt(preceded(tuple((space0, tag(";"), space0)), parse_extras)),
                )),
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
    )(input)
}

pub fn parse_pip_str(input: &str) -> IResult<&str, Req> {
    map(
        tuple((parse_package_name, opt(parse_constraint))),
        |(name, constraint)| Req::new(name.to_string(), constraint.into_iter().collect()),
    )(input)
}

pub fn parse_wh_py_vers(input: &str) -> IResult<&str, Vec<Constraint>> {
    alt((
        map(tag("any"), |_| {
            vec![Constraint::new(ReqType::Gte, Version::new(2, 0, 0))]
        }),
        map(parse_version, |v| vec![Constraint::new(ReqType::Caret, v)]),
        separated_list(tag("."), parse_wh_py_ver),
    ))(input)
}

fn parse_wh_py_ver(input: &str) -> IResult<&str, Constraint> {
    map(
        tuple((
            alt((tag("cp"), tag("py"), tag("pp"))),
            alt((tag("2"), tag("3"), tag("4"))),
            opt(map_parser(take(1u8), digit1)),
        )),
        |(_, major, minor): (_, &str, Option<&str>)| {
            let major: u32 = major.parse().unwrap();
            match minor {
                Some(mi) => Constraint::new(
                    ReqType::Exact,
                    Version::new_short(major, mi.parse().unwrap()),
                ),
                None => {
                    if major == 2 {
                        Constraint::new(ReqType::Lte, Version::new_short(2, 10))
                    } else {
                        Constraint::new(ReqType::Gte, Version::new_short(3, 0))
                    }
                }
            }
        },
    )(input)
}

fn quote(input: &str) -> IResult<&str, &str> {
    alt((tag("\""), tag("'")))(input)
}

fn parse_install_with_extras(input: &str) -> IResult<&str, Vec<String>> {
    map(
        delimited(
            tag("["),
            separated_list(tag(","), parse_package_name),
            tag("]"),
        ),
        |extras| extras.iter().map(|x| x.to_string()).collect(),
    )(input)
}

pub fn parse_extras(input: &str) -> IResult<&str, Extras> {
    map(
        separated_list(
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
    )(input)
}

fn parse_extra_part(input: &str) -> IResult<&str, ExtrasPart> {
    flat_map(
        alt((tag("extra"), tag("sys_platform"), tag("python_version"))),
        |type_| {
            move |input: &str| match type_ {
                "extra" => map(
                    preceded(
                        separated_pair(space0, tag("=="), space0),
                        delimited(quote, parse_package_name, quote),
                    ),
                    |x| ExtrasPart::Extra(x.to_string()),
                )(input),
                "sys_platform" => map(
                    tuple((
                        delimited(space0, tag("=="), space0),
                        delimited(quote, parse_package_name, quote),
                    )),
                    |(_, o)| ExtrasPart::SysPlatform(ReqType::Exact, Os::from_str(o).unwrap()),
                )(input),
                "python_version" => map(
                    tuple((
                        delimited(space0, parse_req_type, space0),
                        delimited(quote, parse_version, quote),
                    )),
                    |(r, v)| ExtrasPart::PythonVersion(Constraint::new(r, v)),
                )(input),
                _ => panic!("Found unexpected"),
            }
        },
    )(input)
}

pub fn parse_constraints(input: &str) -> IResult<&str, Vec<Constraint>> {
    separated_list(tuple((space0, tag(","), space0)), parse_constraint)(input)
}

pub fn parse_constraint(input: &str) -> IResult<&str, Constraint> {
    map(
        alt((
            value((Some(ReqType::Gte), Version::new(0, 0, 0)), tag("*")),
            tuple((opt(parse_req_type), parse_version)),
        )),
        |(r, v)| Constraint::new(r.unwrap_or(ReqType::Exact), v),
    )(input)
}

pub fn parse_version(input: &str) -> IResult<&str, Version> {
    let (remain, (major, minor, patch, extra_num)) = tuple((
        parse_digit_or_wildcard,
        opt(preceded(tag("."), parse_digit_or_wildcard)),
        opt(preceded(tag("."), parse_digit_or_wildcard)),
        opt(preceded(tag("."), parse_digit_or_wildcard)),
    ))(input)?;
    let (remain, modifire) = parse_modifier(remain)?;

    let mut version = Version::new(major, minor.unwrap_or(0), patch.unwrap_or(0));
    version.extra_num = extra_num;
    version.modifier = modifire;

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
        |x| ReqType::from_str(x),
    )(input)
}

fn parse_package_name(input: &str) -> IResult<&str, &str> {
    input.split_at_position1_complete(|x| !is_package_char(x), nom::error::ErrorKind::Tag)
}

fn is_package_char(c: char) -> bool {
    match c {
        '-' => true,
        '.' => true,
        '_' => true,
        _ => c.is_alpha() || c.is_dec_digit(),
    }
}

fn parse_digit_or_wildcard(input: &str) -> IResult<&str, u32> {
    map(alt((digit1, value("0", tag("*")))), |digit: &str| {
        digit.parse().unwrap()
    })(input)
}

fn parse_modifier(input: &str) -> IResult<&str, Option<(VersionModifier, u32)>> {
    opt(map(
        tuple((opt(tag(".")), parse_modifier_version, digit1)),
        |(_, version_modifier, n)| (version_modifier, n.parse().unwrap()),
    ))(input)
}

fn parse_modifier_version(input: &str) -> IResult<&str, VersionModifier> {
    map(
        alt((tag("a"), tag("b"), tag("rc"), tag("dep"))),
        |x| match x {
            "a" => VersionModifier::Alpha,
            "b" => VersionModifier::Beta,
            "rc" => VersionModifier::ReleaseCandidate,
            "dep" => VersionModifier::Dep,
            _ => panic!("not execute this code"),
        },
    )(input)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::dep_types::{Version, VersionModifier};

    use super::*;

    #[test]
    fn dummy_test() {}

    #[rstest(input, expected,
        case("*", Ok(("", Constraint::new(ReqType::Gte, Version::new(0, 0, 0))))),
        case("==1.9.2", Ok(("", Constraint::new(ReqType::Exact, Version::new(1, 9, 2))))),
        case("1.9.2", Ok(("", Constraint::new(ReqType::Exact, Version::new(1, 9, 2))))),
        case("~=1.9.2", Ok(("", Constraint::new(ReqType::Tilde, Version::new(1, 9, 2))))),
    )]
    fn test_parse_constraint(input: &str, expected: IResult<&str, Constraint>) {
        assert_eq!(parse_constraint(input), expected);
    }

    #[rstest(input, expected,
        case("3.12.5", Ok(("", Version {
            major: 3,
            minor: 12,
            patch: 5,
            extra_num: None,
            modifier: None,
        }))),
        case("0.1.0", Ok(("", Version {
            major: 0,
            minor: 1,
            patch: 0,
            extra_num: None,
            modifier: None,
        }))),
        case("3.7", Ok(("", Version {
            major: 3,
            minor: 7,
            patch: 0,
            extra_num: None,
            modifier: None,
        }))),
        case("1", Ok(("", Version {
            major: 1,
            minor: 0,
            patch: 0,
            extra_num: None,
            modifier: None,
        }))),
        case("3.2.*", Ok(("", Version {
            major: 3,
            minor: 2,
            patch: 0,
            extra_num: None,
            modifier: None,
        }))),
        case("1.*", Ok(("", Version {
            major: 1,
            minor: 0,
            patch: 0,
            extra_num: None,
            modifier: None,
        }))),
        case("1.*.*", Ok(("", Version {
            major: 1,
            minor: 0,
            patch: 0,
            extra_num: None,
            modifier: None,
        }))),
        case("19.3", Ok(("", Version {
            major: 19,
            minor: 3,
            patch: 0,
            extra_num: None,
            modifier: None,
        }))),
        case("19.3b0", Ok(("", Version {
            major: 19,
            minor: 3,
            patch: 0,
            extra_num: None,
            modifier: Some((VersionModifier::Beta, 0)),
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
        case("saturn = \">=0.3.4\"", Ok(("", Req::new("saturn".to_string(), vec![Constraint::new(ReqType::Gte, Version::new(0, 3, 4))])))),
        case("saturn", Ok(("", Req::new("saturn".to_string(), vec![])))),
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
