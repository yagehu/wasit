use nom::{
    bytes::complete::is_not,
    character::{
        self,
        complete::{char, newline},
    },
    multi::many0,
    sequence::{self, tuple},
};

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Trace<'a> {
    pub calls: Vec<Syscall<'a>>,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Syscall<'a> {
    name:      &'a str,
    arguments: &'a str,
}

pub fn trace(input: &str) -> nom::IResult<&str, Trace> {
    let (input, calls) = many0(tuple((syscall, newline)))(input)?;
    let calls = calls.into_iter().map(|tuple| tuple.0).collect::<Vec<_>>();

    Ok((input, Trace { calls }))
}

fn syscall(input: &str) -> nom::IResult<&str, Syscall> {
    let (input, (name, arguments, _, _)) =
        tuple((syscall_name, parens, char('='), newline))(input)?;

    Ok((input, Syscall { name, arguments }))
}

fn parens(input: &str) -> nom::IResult<&str, &str> {
    sequence::delimited(char('('), is_not(")"), char(')'))(input)
}

fn syscall_name(input: &str) -> nom::IResult<&str, &str> {
    character::streaming::alphanumeric1(input)
}
