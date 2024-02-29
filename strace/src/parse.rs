use nom::{
    bytes::complete::{is_not, tag, take_while1},
    character::{
        self,
        complete::{char, multispace0, multispace1},
    },
    combinator::{map_res, opt},
    multi::many0,
    sequence::{self, delimited, terminated, tuple},
    AsChar,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
pub struct Trace {
    pub calls: Vec<Syscall>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
pub struct Syscall {
    name:      String,
    arguments: String,
    result:    CallResult,
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
    let (input, (name, arguments, _, _, _, call_result)) = tuple((
        syscall_name,
        parens,
        multispace0,
        char('='),
        multispace0,
        call_result,
    ))(input)?;

    Ok((
        input,
        Syscall {
            name:      name.to_owned(),
            arguments: arguments.to_owned(),
            result:    call_result,
        },
    ))
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum CallResult {
    Ok(i32),
    Err { ret: i32, errno: Errno },
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct Errno(String);

fn call_result(input: &str) -> nom::IResult<&str, CallResult> {
    let (input, (neg, ret, _)) = tuple((
        opt(char('-')),
        map_res(take_while1(|c: char| c.is_dec_digit()), |s| {
            i32::from_str_radix(s, 10)
        }),
        multispace0,
    ))(input)?;
    let ret = if neg.is_some() { -ret } else { ret };

    match errno(input) {
        | Ok((input, errno)) => Ok((input, CallResult::Err { ret, errno })),
        | Err(_err) => Ok((input, CallResult::Ok(ret))),
    }
}

fn errno(input: &str) -> nom::IResult<&str, Errno> {
    let (input, (errno, _, _)) =
        tuple((take_while1(|c: char| c.is_uppercase()), multispace1, parens))(input)?;

    Ok((input, Errno(errno.to_owned())))
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
access("/etc/ld.so.preload", R_OK)      = -1 ENOENT (No such file or directory)
access("/etc/ld.so.preload", R_OK)      = 0
+++ exited with 0 +++
            "#,
        )
        .unwrap();

        assert!(rest.trim().is_empty(), "{rest}");
        assert!(matches!(
            &trace.calls[0],
            Syscall {
                name,
                arguments: _arguments,
                result:    CallResult::Err {
                    ret:   -1,
                    errno,
                },
            } if errno.0 == "ENOENT" && name == "access",
        ));
        assert!(matches!(
            &trace.calls[1],
            Syscall {
                name,
                arguments: _arguments,
                result:    CallResult::Ok(0),
            } if name == "access",
        ));
    }
}
