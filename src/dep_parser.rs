use crate::dep_types::{Version, VersionModifier, ReqType, Constraint};
use nom::IResult;
use nom::sequence::{tuple, preceded};
use nom::character::complete::digit1;
use nom::bytes::complete::tag;
use nom::combinator::{opt, map, value, map_res};
use nom::branch::alt;
use std::str::FromStr;

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
        tag("~"),
        tag("~="),
    )), |x| ReqType::from_str(x))(input)
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
    )]
    fn parse_constraints(input: &str, expected: IResult<&str, Constraint>) {
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
    fn parse_versions(input: &str, expected: IResult<&str, Version>) {
        assert_eq!(parse_version(input), expected);
    }
}