use nom::{
    branch::alt,
    bytes::{
        self,
        complete::{is_not, tag, take_until, take_while1},
    },
    character::complete::{
        alpha1,
        alphanumeric1,
        char,
        multispace0,
        multispace1,
        newline,
        none_of,
        one_of,
        space0,
        space1,
    },
    combinator::{fail, map, map_res, opt, peek, recognize},
    multi::{many0_count, separated_list0},
    sequence::{self, delimited, pair, separated_pair, tuple},
    AsChar,
    Parser,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    pub events: Vec<ThreadEvent>,
}

impl Trace {
    pub fn parse(input: &str) -> nom::IResult<&str, Self> {
        let (input, _) = multispace0(input)?;
        let (input, events) = ws(separated_list0(newline, ws(ThreadEvent::parse)))(input)?;
        let (input, _) = multispace0(input)?;

        Ok((input, Self { events }))
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CallResult {
    pub ret:     Value,
    pub decoded: Option<DecodedRetValue>,
}

impl CallResult {
    fn parse(input: &str) -> nom::IResult<&str, Self> {
        let (input, ret) = Value::parse(input)?;
        let (input, decoded) = opt(ws(DecodedRetValue::parse))(input)?;

        Ok((input, Self { ret, decoded }))
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum DecodedRetValue {
    Errno(Errno),
    Flags(FlagSet),
}

impl DecodedRetValue {
    fn parse(input: &str) -> nom::IResult<&str, Self> {
        if peek(ws(errno))(input).is_ok() {
            let (input, errno) = ws(errno)(input)?;

            Ok((input, Self::Errno(errno)))
        } else {
            let (input, (_, flag_set)) = ws(delimited(
                char('('),
                pair(tag("flags"), ws(FlagSet::parse)),
                char(')'),
            ))(input)?;

            Ok((input, Self::Flags(flag_set)))
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct Errno(String);

fn errno(input: &str) -> nom::IResult<&str, Errno> {
    let (input, (errno, _, _)) =
        tuple((take_while1(|c: char| c.is_uppercase()), multispace1, parens))(input)?;

    Ok((input, Errno(errno.to_owned())))
}

fn parens(input: &str) -> nom::IResult<&str, &str> {
    sequence::delimited(char('('), is_not(")"), char(')'))(input)
}

fn syscall_name(input: &str) -> nom::IResult<&str, &str> {
    ident(input)
}

fn oct(input: &str) -> nom::IResult<&str, u64> {
    let (input, (_, value)) = pair(
        char('0'),
        map_res(take_while1(|c: char| c.is_oct_digit()), |s| {
            u64::from_str_radix(s, 8)
        }),
    )(input)?;

    Ok((input, value))
}

fn int32(input: &str) -> nom::IResult<&str, i32> {
    let (input, (neg, i)) = tuple((
        opt(char('-')),
        map_res(take_while1(|c: char| c.is_dec_digit()), |s: &str| {
            s.parse::<i32>()
        }),
    ))(input)?;
    let i = if neg.is_some() { -i } else { i };

    Ok((input, i))
}

fn int64(input: &str) -> nom::IResult<&str, i64> {
    let (input, (neg, i)) = tuple((
        opt(char('-')),
        map_res(take_while1(|c: char| c.is_dec_digit()), |s: &str| {
            s.parse::<i64>()
        }),
    ))(input)?;
    let i = if neg.is_some() { -i } else { i };

    Ok((input, i))
}

fn hex(input: &str) -> nom::IResult<&str, u64> {
    let (input, _) = tag("0x")(input)?;
    let (input, value) = map_res(take_while1(|c: char| c.is_ascii_hexdigit()), |digits| {
        u64::from_str_radix(digits, 16)
    })(input)?;

    Ok((input, value))
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ThreadEvent {
    pub pid:   i32,
    pub event: Event,
}

impl ThreadEvent {
    fn parse(input: &str) -> nom::IResult<&str, Self> {
        let (input, pid) = ws(int32).parse(input)?;
        let (input, event) = ws(Event::parse)(input)?;

        Ok((input, Self { pid, event }))
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    Exit(ExitThreadEvent),
    Normal(NormalThreadEvent),
}

impl Event {
    fn parse(input: &str) -> nom::IResult<&str, Self> {
        if peek(ws(ExitThreadEvent::parse))(input).is_ok() {
            let (input, event) = ws(ExitThreadEvent::parse)(input)?;

            Ok((input, Self::Exit(event)))
        } else {
            let (input, event) = ws(NormalThreadEvent::parse)(input)?;

            Ok((input, Self::Normal(event)))
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ExitThreadEvent {
    pub status: u32,
}

impl ExitThreadEvent {
    fn parse(input: &str) -> nom::IResult<&str, Self> {
        let (input, status) = delimited(
            tag("+++ exited with "),
            map_res(take_while1(|c: char| c.is_ascii_digit()), |s: &str| {
                s.parse::<u32>()
            }),
            tag(" +++"),
        )(input)?;

        Ok((input, Self { status }))
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct NormalThreadEvent {
    pub func: String,
    pub case: NonExitEvent,
}

impl NormalThreadEvent {
    pub fn parse(input: &str) -> nom::IResult<&str, Self> {
        if peek(resumed_tag)(input).is_ok() {
            let (input, func) = ws(resumed_tag).parse(input)?;
            let (input, prev_changed) =
                if peek(ws(tag::<&str, &str, nom::error::Error<_>>("=>")))(input).is_ok() {
                    let (input, (_, value, _)) =
                        tuple((ws(tag("=>")), ws(Value::parse), ws(char(','))))(input)?;

                    (input, Some(value))
                } else {
                    (input, None)
                };

            let (input, args) = separated_list0(char(','), ws(Arg::parse))(input)?;
            let (input, (_, _)) = tuple((ws(char(')')), ws(char('='))))(input)?;
            let (input, call_result) =
                if peek(ws(char::<&str, nom::error::Error<_>>('?')))(input).is_ok() {
                    let (input, _) = ws(char('?'))(input)?;

                    (input, None)
                } else {
                    let (input, call_result) = ws(CallResult::parse)(input)?;

                    (input, Some(call_result))
                };

            Ok((
                input,
                Self {
                    func: func.to_owned(),
                    case: NonExitEvent::Resumed(FinishedEvent {
                        prev_changed,
                        args,
                        call_result,
                    }),
                },
            ))
        } else if peek(syscall_name)(input).is_ok() {
            let (input, (func, _)) = tuple((syscall_name, char('(')))(input)?;
            let (input, _) = opt(ws(tuple((
                tag("<... resuming interrupted"),
                ws(ident),
                tag("...>"),
            ))))(input)?;
            let (input, args) = separated_list0(char(','), ws(Arg::parse))(input)?;
            let (input, _) = opt(ws(char(',')))(input)?;

            if peek(unfinished_tag)(input).is_ok() {
                let (input, _) = ws(unfinished_tag).parse(input)?;

                Ok((
                    input,
                    Self {
                        func: func.to_owned(),
                        case: NonExitEvent::Unfinished(Unfinished { args }),
                    },
                ))
            } else if peek(detached_tag)(input).is_ok() {
                let (input, _) = ws(detached_tag).parse(input)?;

                Ok((
                    input,
                    Self {
                        func: func.to_owned(),
                        case: NonExitEvent::Detached(Unfinished { args }),
                    },
                ))
            } else {
                let (input, (_, _)) = tuple((ws(char(')')), ws(char('='))))(input)?;
                let (input, call_result) =
                    if peek(ws(char::<&str, nom::error::Error<_>>('?')))(input).is_ok() {
                        let (input, _) = ws(char('?'))(input)?;

                        (input, None)
                    } else {
                        let (input, call_result) = ws(CallResult::parse)(input)?;

                        (input, Some(call_result))
                    };

                Ok((
                    input,
                    Self {
                        func: func.to_owned(),
                        case: NonExitEvent::Complete(FinishedEvent {
                            prev_changed: None,
                            args,
                            call_result,
                        }),
                    },
                ))
            }
        } else {
            fail(input)
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum NonExitEvent {
    Complete(FinishedEvent),
    Unfinished(Unfinished),
    Resumed(FinishedEvent),
    Detached(Unfinished),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Unfinished {
    pub args: Vec<Arg>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FinishedEvent {
    pub prev_changed: Option<Value>,
    pub args:         Vec<Arg>,
    pub call_result:  Option<CallResult>,
}

fn detached_tag(input: &str) -> nom::IResult<&str, ()> {
    let (input, _) = delimited(char('<'), tag("detached ..."), char('>'))(input)?;

    Ok((input, ()))
}

fn unfinished_tag(input: &str) -> nom::IResult<&str, ()> {
    let (input, _) = delimited(char('<'), tag("unfinished ..."), char('>'))(input)?;

    Ok((input, ()))
}

fn resumed_tag(input: &str) -> nom::IResult<&str, &str> {
    let (input, (_, func, _)) = delimited(
        char('<'),
        tuple((tag("..."), ws(syscall_name), tag("resumed"))),
        char('>'),
    )(input)?;

    Ok((input, func))
}

fn ident(input: &str) -> nom::IResult<&str, &str> {
    recognize(pair(
        alt((alpha1, tag("_"))),
        many0_count(alt((alphanumeric1, tag("_")))),
    ))
    .parse(input)
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Arg {
    pub value:   Value,
    pub changed: Option<Value>,
}

impl Arg {
    fn parse(input: &str) -> nom::IResult<&str, Self> {
        let (input, (value, changed)) = tuple((
            ws(Value::parse),
            opt(map(
                tuple((ws(tag("=>")), ws(Value::parse))),
                |(_, value)| value,
            )),
        ))(input)?;

        Ok((input, Self { value, changed }))
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    Oct(u64),
    Int(i64),
    Addr(u64),
    Call(CallValue),
    Ident(String),
    FlagSet(FlagSet),
    String(StringValue),
    Record(RecordValue),
    List(Vec<Value>),
    InverseList(Vec<Value>),
}

impl Value {
    fn parse(input: &str) -> nom::IResult<&str, Self> {
        if peek(hex)(input).is_ok() {
            let (input, value) = ws(hex).parse(input)?;

            Ok((input, Self::Addr(value)))
        } else if peek(CallValue::parse)(input).is_ok() {
            let (input, value) = ws(CallValue::parse).parse(input)?;

            Ok((input, Self::Call(value)))
        } else if peek(ident)(input).is_ok() {
            // Parse the first ident.
            let (input_, ident) = ws(ident)(input)?;

            if peek(ws(char::<&str, nom::error::Error<_>>('|')))(input_).is_ok() {
                // This is actually a flag set.
                let (input, flag_set) = ws(FlagSet::parse)(input)?;

                Ok((input, Self::FlagSet(flag_set)))
            } else {
                Ok((input_, Self::Ident(ident.to_owned())))
            }
        } else if peek(oct)(input).is_ok() {
            let (input, value) = oct(input)?;

            Ok((input, Self::Oct(value)))
        } else if peek(int64)(input).is_ok() {
            let (input, value) = int64(input)?;
            let (input, _) =
                opt(ws(delimited(tag("/*"), take_until("*/"), tag("*/")))).parse(input)?;

            Ok((input, Self::Int(value)))
        } else if peek(StringValue::parse)(input).is_ok() {
            let (input, s) = ws(StringValue::parse).parse(input)?;

            Ok((input, Self::String(s)))
        } else if peek(char::<&str, nom::error::Error<_>>('['))(input).is_ok() {
            let (input, values) = delimited(
                char('['),
                separated_list0(alt((space1, tag(", "))), Value::parse),
                char(']'),
            )(input)?;

            Ok((input, Self::List(values)))
        } else if peek(char::<&str, nom::error::Error<_>>('~'))(input).is_ok() {
            let (input, (_, values)) = tuple((
                char('~'),
                delimited(
                    char('['),
                    separated_list0(char(' '), Value::parse),
                    char(']'),
                ),
            ))(input)?;

            Ok((input, Self::InverseList(values)))
        } else if peek(char::<&str, nom::error::Error<_>>('{'))(input).is_ok() {
            let (input, record) = ws(RecordValue::parse).parse(input)?;
            let (input, _) =
                ws(opt(delimited(tag("/*"), take_until("*/"), tag("*/")))).parse(input)?;

            Ok((input, Self::Record(record)))
        } else {
            fail(input)
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct FlagSet(Vec<String>);

impl FlagSet {
    fn parse(input: &str) -> nom::IResult<&str, Self> {
        let (input, values) = separated_list0(
            char('|'),
            take_while1(|c: char| c.is_alphanumeric() || c == '_'),
        )(input)?;

        Ok((
            input,
            Self(values.into_iter().map(ToOwned::to_owned).collect()),
        ))
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CallValue {
    pub func: String,
    pub args: Vec<Value>,
}

impl CallValue {
    fn parse(input: &str) -> nom::IResult<&str, Self> {
        let (input, (func, args)) = pair(
            ident,
            delimited(
                char('('),
                separated_list0(char(','), ws(Value::parse)),
                char(')'),
            ),
        )(input)?;

        Ok((
            input,
            Self {
                func: func.to_owned(),
                args,
            },
        ))
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StringValue {
    Complete(String),
    Clipped(String),
}

impl StringValue {
    fn parse(input: &str) -> nom::IResult<&str, Self> {
        let (input, s) = delimited(
            char('"'),
            bytes::streaming::escaped(none_of(r#"\""#), '\\', one_of(r#"""#)),
            char('"'),
        )(input)?;

        if peek(tag::<&str, &str, nom::error::Error<_>>("..."))(input).is_ok() {
            let (input, _) = tag("...")(input)?;

            Ok((input, Self::Clipped(s.to_owned())))
        } else {
            Ok((input, Self::Complete(s.to_owned())))
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct RecordValue(Vec<RecordMember>);

impl RecordValue {
    fn parse(input: &str) -> nom::IResult<&str, Self> {
        let (input, members) = delimited(
            char('{'),
            separated_list0(char(','), ws(RecordMember::parse)),
            char('}'),
        )(input)?;

        Ok((input, Self(members)))
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct RecordMember {
    pub name:  String,
    pub value: Arg,
}

impl RecordMember {
    fn parse(input: &str) -> nom::IResult<&str, Self> {
        let (input, (name, value)) = separated_pair(ident, char('='), Arg::parse)(input)?;

        Ok((
            input,
            Self {
                name: name.to_owned(),
                value,
            },
        ))
    }
}

/// A combinator that takes a parser `inner` and produces a parser that also consumes both leading and
/// trailing whitespace, returning the output of `inner`.
fn ws<'a, F, O, E: nom::error::ParseError<&'a str>>(
    inner: F,
) -> impl FnMut(&'a str) -> nom::IResult<&'a str, O, E>
where
    F: Parser<&'a str, O, E>,
{
    delimited(space0, inner, space0)
}

#[cfg(test)]
mod tests {
    use nom::combinator::all_consuming;

    use super::*;

    #[test]
    fn arg_ok() {
        let cases = vec![
            (
                "213486712",
                Arg {
                    value:   Value::Int(213486712),
                    changed: None,
                },
            ),
            (
                "0x00000000",
                Arg {
                    value:   Value::Addr(0),
                    changed: None,
                },
            ),
            (
                "0x00000001",
                Arg {
                    value:   Value::Addr(1),
                    changed: None,
                },
            ),
            (
                "NULL",
                Arg {
                    value:   Value::Ident("NULL".to_owned()),
                    changed: None,
                },
            ),
            (
                "FUTEX_WAIT_BITSET_PRIVATE",
                Arg {
                    value:   Value::Ident("FUTEX_WAIT_BITSET_PRIVATE".to_owned()),
                    changed: None,
                },
            ),
            (
                "O_RDONLY|O_NONBLOCK|O_LARGEFILE",
                Arg {
                    value:   Value::FlagSet(FlagSet(vec![
                        "O_RDONLY".to_owned(),
                        "O_NONBLOCK".to_owned(),
                        "O_LARGEFILE".to_owned(),
                    ])),
                    changed: None,
                },
            ),
        ];

        for (i, case) in cases.into_iter().enumerate() {
            let (_input, got) = Arg::parse(case.0).unwrap();

            assert_eq!(case.1, got, "{i}");
        }
    }

    #[test]
    fn unfinished() {
        let cases = vec![(
            r#"15807 futex(0x562811cdfee8, FUTEX_WAIT_BITSET_PRIVATE, 4294967295, NULL, FUTEX_BITSET_MATCH_ANY <unfinished ...>"#,
            ThreadEvent {
                pid:   15807,
                event: Event::Normal(NormalThreadEvent {
                    func: "futex".to_owned(),
                    case: NonExitEvent::Unfinished(Unfinished {
                        args: vec![
                            Arg {
                                value:   Value::Addr(94730097393384),
                                changed: None,
                            },
                            Arg {
                                value:   Value::Ident("FUTEX_WAIT_BITSET_PRIVATE".to_owned()),
                                changed: None,
                            },
                            Arg {
                                value:   Value::Int(4294967295),
                                changed: None,
                            },
                            Arg {
                                value:   Value::Ident("NULL".to_owned()),
                                changed: None,
                            },
                            Arg {
                                value:   Value::Ident("FUTEX_BITSET_MATCH_ANY".to_owned()),
                                changed: None,
                            },
                        ],
                    }),
                }),
            },
            r#"15805 read(0, <unfinished ...>"#,
            ThreadEvent {
                pid:   15805,
                event: Event::Normal(NormalThreadEvent {
                    func: "read".to_owned(),
                    case: NonExitEvent::Unfinished(Unfinished {
                        args: vec![Arg {
                            value:   Value::Int(0),
                            changed: None,
                        }],
                    }),
                }),
            },
            r#"15805 epoll_wait(3,  <unfinished ...>"#,
            ThreadEvent {
                pid:   15805,
                event: Event::Normal(NormalThreadEvent {
                    func: "epoll_wait".to_owned(),
                    case: NonExitEvent::Unfinished(Unfinished {
                        args: vec![Arg {
                            value:   Value::Int(3),
                            changed: None,
                        }],
                    }),
                }),
            },
        )];

        for (i, case) in cases.into_iter().enumerate() {
            let (_rest, got) =
                all_consuming(ThreadEvent::parse)(case.0).expect(&format!("case {i}: {}", case.0));

            assert_eq!(got, case.1, "{i}");
        }
    }

    #[test]
    fn lines() {
        let lines = r#"
        15805 read(0, <unfinished ...>
        15806 read(0, <unfinished ...>
        15805 epoll_wait(3,  <unfinished ...>
        15808 <... read resumed>""..., 8192)    = 1383
        15821 sched_getaffinity(15821, 32, [0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15]) = 8
        12812 readv(0, [{iov_base=""..., iov_len=7}, {iov_base=""..., iov_len=1024}], 2) = 1031
        89909 restart_syscall(<... resuming interrupted read ...> <unfinished ...>
        "#;

        let (_rest, trace) = all_consuming(Trace::parse)(lines).unwrap();

        assert_eq!(trace.events.len(), 7, "{:#?}", trace);
    }

    #[test]
    fn resumed_changed() {
        let lines = r#"
            37860 clone3({flags=CLONE_VM|CLONE_FS|CLONE_FILES|CLONE_SIGHAND|CLONE_THREAD|CLONE_SYSVSEM|CLONE_SETTLS|CLONE_PARENT_SETTID|CLONE_CHILD_CLEARTID, child_tid=0x7fc7c1000990, parent_tid=0x7fc7c1000990, exit_signal=0, stack=0x7fc7c0e00000, stack_size=0x1ff900, tls=0x7fc7c10006c0} <unfinished ...>
            37860 <... clone3 resumed> => {parent_tid=[37905]}, 88) = 37905
        "#;

        let (_rest, trace) = all_consuming(Trace::parse)(lines).unwrap();

        assert_eq!(trace.events.len(), 2, "{:#?}", trace);
    }

    #[test]
    fn resuming_futex() {
        let lines = r#"
            42526 restart_syscall(<... resuming interrupted futex ...> <unfinished ...>
        "#;

        let (_rest, trace) = all_consuming(Trace::parse)(lines).unwrap();

        assert_eq!(trace.events.len(), 1, "{:#?}", trace);
    }
}
