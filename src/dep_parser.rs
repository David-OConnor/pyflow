use crate::dep_types::{Version, VersionModifier, ReqType, Constraint, Req, Extras, DependencyError};
use nom::{IResult, InputTakeAtPosition, AsChar};
use nom::sequence::{tuple, preceded, separated_pair, delimited};
use nom::character::complete::{digit1, space0};
use nom::bytes::complete::tag;
use nom::combinator::{opt, map, value, map_res, flat_map};
use nom::branch::alt;
use std::str::FromStr;
use std::io::ErrorKind;
use nom::multi::{many0, many_m_n, separated_list};
use crate::util::Os;

enum ExtrasPart {
    Extra(String),
    SysPlatform(ReqType, Os),
    PythonVersion(Constraint),
}


pub fn parse_req(input: &str) -> IResult<&str, Req> {
    // eg saturn = ">=0.3.4", as in pyproject.toml
    map(separated_pair(
        parse_package_name,
        tuple((space0, tag("="), space0)),
        delimited(quote, parse_constraints, quote),
    ), |(name, constraints)| Req::new(name.to_string(), constraints))(input)
}

pub fn parse_req_pypi_fmt(input: &str) -> IResult<&str, Req> {
    // eg saturn (>=0.3.4) or argon2-cffi (>=16.1.0) ; extra == 'argon2'
    // Note: We specify what chars are acceptable in a name instead of using
    // wildcard, so we don't accidentally match a semicolon here if a
    // set of parens appears later. The non-greedy ? in the version-matching
    // expression's important as well, in some cases of extras.
    map(tuple((
        parse_package_name,
        space0,
        delimited(tag("("), parse_constraints, tag(")")),
        tuple((space0, tag(";"), space0)),
        parse_extras,
    )),
        |(name, _, constraints, _, extras)| {
            Req::new_with_extras(name.to_string(), constraints, extras)
        })(input)
}

fn quote(input: &str) -> IResult<&str, &str> {
    alt((
        tag("\""),
        tag("'"),
    ))(input)
}

pub fn parse_extras(input: &str) -> IResult<&str, Extras> {
    map(separated_list(tag(","), parse_extra_part), |ps| {
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
    })(input)
}

fn parse_extra_part(input: &str) -> IResult<&str, ExtrasPart> {
    flat_map(alt((
        tag("extra"),
        tag("sys_platform"),
        tag("python_version"),
    )), |type_| {
        move |input: &str| {
            match type_ {
                "extra" => { map(
                    preceded(tag("=="), parse_package_name),
                    |x| ExtrasPart::Extra(x.to_string()))(input) },
                "sys_platform" => { map(tuple((parse_req_type, parse_package_name)), |(r, o)| {
                    ExtrasPart::SysPlatform(r, Os::from_str(o).unwrap())
                })(input) },
                "python_version" => { map(parse_constraint, |x| ExtrasPart::PythonVersion(x))(input) },
                _ => panic!("Found unexpected")
            }
        }

    })(input)
}

pub fn parse_constraints(input: &str) -> IResult<&str, Vec<Constraint>> {
    separated_list(tag(","), parse_constraint)(input)
}

pub fn parse_constraint(input: &str) -> IResult<&str, Constraint> {
    map(alt((
        value((Some(ReqType::Gte), Version::new(0, 0, 0)), tag("*")),
        tuple((opt(parse_req_type), parse_version)),
    )),
    |(r, v)| Constraint::new(r.unwrap_or(ReqType::Exact), v)
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
    map_res(alt((
        tag("=="),
        tag(">="),
        tag("<="),
        tag(">"),
        tag("<"),
        tag("!="),
        tag("^"),
        tag("~="),
        tag("~"),
    )), |x| ReqType::from_str(x))(input)
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
    map(
        alt((digit1, value("0", tag("*")))),
        |digit: &str| digit.parse().unwrap(),
    )(input)
}

fn parse_modifier(input: &str) -> IResult<&str, Option<(VersionModifier, u32)>> {
    opt(
        map(
            tuple((opt(tag(".")), parse_modifier_version, digit1)),
            |(_, version_modifier, n)| (version_modifier, n.parse().unwrap())
        )
    )(input)
}

fn parse_modifier_version(input: &str) -> IResult<&str, VersionModifier> {
    map(alt((
        tag("a"),
        tag("b"),
        tag("rc"),
        tag("dep"),
    )), |x| {
        match x {
            "a" => VersionModifier::Alpha,
            "b" => VersionModifier::Beta,
            "rc" => VersionModifier::ReleaseCandidate,
            "dep" => VersionModifier::Dep,
            _ => panic!("not execute this code"),
        }
    })(input)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use crate::dep_types::{Version, VersionModifier};
    use super::*;

    #[test]
    fn dummy_test() {

    }

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
}