use nom::{
    bytes::complete::{is_not, tag, take_while1},
    character::{
        self,
        complete::{char, multispace0, multispace1},
    },
    combinator::{map_res, opt},
    multi::many0,
    sequence::{self, delimited, terminated, tuple},
};

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Trace<'a> {
    pub calls: Vec<Syscall<'a>>,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Syscall<'a> {
    name:      &'a str,
    arguments: &'a str,
    errno:     u32,
}

pub fn trace(input: &str) -> nom::IResult<&str, Trace> {
    let (input, calls) = terminated(
        many0(tuple((multispace0, syscall, multispace0))),
        opt(exit_status),
    )(input)?;
    let calls = calls.into_iter().map(|tuple| tuple.1).collect::<Vec<_>>();

    Ok((input, Trace { calls }))
}

fn syscall(input: &str) -> nom::IResult<&str, Syscall> {
    let (input, (name, arguments, _, _, _, errno)) = tuple((
        syscall_name,
        parens,
        multispace0,
        char('='),
        multispace0,
        map_res(take_while1(|c: char| c.is_digit(10)), |s| {
            u32::from_str_radix(s, 10)
        }),
    ))(input)?;

    Ok((
        input,
        Syscall {
            name,
            arguments,
            errno,
        },
    ))
}

fn parens(input: &str) -> nom::IResult<&str, &str> {
    sequence::delimited(char('('), is_not(")"), char(')'))(input)
}

fn syscall_name(input: &str) -> nom::IResult<&str, &str> {
    character::complete::alphanumeric1(input)
}

fn exit_status(input: &str) -> nom::IResult<&str, u32> {
    delimited(
        tag("+++ exited with "),
        map_res(take_while1(|c: char| c.is_digit(10)), |s| {
            u32::from_str_radix(s, 10)
        }),
        tag(" +++"),
    )(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok() {
        let (rest, trace) = trace(
            r#"
execve("/usr/bin/sleep", ["sleep", "1"], 0x7ffc8a0ec998 /* 54 vars */) = 0
+++ exited with 0 +++
            "#,
        )
        .unwrap();

        assert!(rest.trim().is_empty(), "{rest}");
    }
}
