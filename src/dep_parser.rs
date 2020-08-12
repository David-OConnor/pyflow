use crate::dep_types::{Version, VersionModifier};
use nom::IResult;
use nom::sequence::{tuple, preceded};
use nom::character::complete::digit1;
use nom::bytes::complete::tag;
use nom::combinator::{opt, map};
use nom::branch::alt;

pub fn parse_version(input: &str) -> IResult<&str, Version> {
    let (remain, (major, minor, patch, extra_num)) = tuple((
        parse_digit,
        opt(preceded(tag("."), parse_digit)),
        opt(preceded(tag("."), parse_digit)),
        opt(preceded(tag("."), parse_digit)),
    ))(input)?;
    let (remain, modifire) = parse_modifier(remain)?;

    let mut version = Version::new(major, minor.unwrap_or(0), patch.unwrap_or(0));
    version.extra_num = extra_num;
    version.modifier = modifire;

    Ok((remain, version))
}

fn parse_digit(input: &str)  -> IResult<&str, u32> {
    map(
        digit1,
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