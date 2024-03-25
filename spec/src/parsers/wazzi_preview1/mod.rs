use nom::{
    branch::alt,
    bytes::complete::{escaped, tag, take_till1, take_while1},
    character::complete::{char, multispace0, multispace1, none_of, one_of},
    combinator::{eof, map, peek},
    error::ParseError,
    sequence::{delimited, pair, tuple},
    Parser as _,
};
use nom_locate::{position, LocatedSpan};
use nom_supreme::{
    error::ErrorTree,
    final_parser::final_parser,
    multi::collect_separated_terminated,
    ParserExt,
};

use crate::ast::Package;

type Span<'a> = LocatedSpan<&'a str>;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Document<'a> {
    modules: Vec<Module<'a>>,
}

impl<'a> Document<'a> {
    pub fn parse(input: Span<'a>) -> Result<Self, ErrorTree<Span>> {
        let modules = final_parser(
            alt((
                collect_separated_terminated(Module::parse, multispace0, ws(eof)),
                peek(eof).value(vec![]),
            ))
            .cut(),
        )(input)?;

        Ok(Self { modules })
    }

    pub fn into_package(self) -> Result<Package, eyre::Error> {
        Ok(Package::new())
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Module<'a> {
    pos:   Span<'a>,
    name:  Option<Id<'a>>,
    decls: Vec<Decl<'a>>,
}

impl<'a> Module<'a> {
    pub fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, pos) = position(input)?;
        let (input, (_, name, decls)) = delimited(
            char('('),
            tuple((
                ws(tag("module")),
                ws(Id::parse).opt(),
                alt((
                    collect_separated_terminated(
                        Decl::parse,
                        multispace0,
                        multispace0.terminated(char(')')).peek(),
                    ),
                    peek::<_, _, ErrorTree<_>, _>(char(')')).value(vec![]),
                ))
                .cut(),
            )),
            char(')'),
        )(input)?;

        Ok((input, Self { pos, name, decls }))
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Decl<'a> {
    Typename(Typename<'a>),
    Func(Func<'a>),
}

impl<'a> Decl<'a> {
    pub fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        alt((
            map(ws(Typename::parse), Self::Typename),
            ws(Func::parse).map(Self::Func),
        ))
        .context("decl")
        .parse(input)
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Typename<'a> {
    name:        Id<'a>,
    ty:          Type<'a>,
    annotations: Vec<Annotation<'a>>,
}

impl<'a> Typename<'a> {
    pub fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (_, name, ty, annotations)) = paren(tuple((
            tag("typename"),
            ws(Id::parse),
            ws(Type::parse),
            alt((
                collect_separated_terminated(
                    Annotation::parse,
                    multispace0,
                    multispace0.terminated(char(')')).peek(),
                ),
                peek::<_, _, ErrorTree<_>, _>(char(')')).value(vec![]),
            ))
            .cut(),
        )))
        .context("typename")
        .parse(input)?;

        Ok((
            input,
            Self {
                name,
                ty,
                annotations,
            },
        ))
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Func<'a> {
    pub name:    Span<'a>,
    pub params:  Vec<FuncParam<'a>>,
    pub results: Vec<FuncResult<'a>>,
}

impl<'a> Func<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (_, _, (_, name), params, results)) = paren(tuple((
            ws(tag("@interface")),
            ws(tag("func")),
            ws(paren(pair(ws(tag("export")), ws(quoted_string)))),
            collect_separated_terminated(
                ws(FuncParam::parse),
                multispace0,
                multispace0
                    .terminated(alt((char(')').value(()), ws(FuncResult::parse).value(()))))
                    .peek(),
            ),
            collect_separated_terminated(
                ws(FuncResult::parse),
                multispace0,
                multispace0.terminated(char(')')).peek(),
            ),
        )))
        .context("func")
        .parse(input)?;

        Ok((
            input,
            Self {
                name,
                params,
                results,
            },
        ))
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FuncParam<'a> {
    pub name: Id<'a>,
    pub tref: TypeRef<'a>,
}

impl<'a> FuncParam<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (_, name, tref)) =
            paren(tuple((ws(tag("param")), ws(Id::parse), ws(TypeRef::parse))))
                .context("func param")
                .parse(input)?;

        Ok((input, Self { name, tref }))
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FuncResult<'a> {
    pub name: Id<'a>,
    pub tref: TypeRef<'a>,
}

impl<'a> FuncResult<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (_, name, tref)) = paren(tuple((
            ws(tag("result")),
            ws(Id::parse),
            ws(TypeRef::parse),
        )))
        .context("func result")
        .parse(input)?;

        Ok((input, Self { name, tref }))
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum TypeRef<'a> {
    Name(Id<'a>),
    Type(Box<Type<'a>>),
}

impl<'a> TypeRef<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        alt((
            Id::parse.map(Self::Name),
            Type::parse.map(|ty| Self::Type(Box::new(ty))),
        ))
        .cut()
        .context("type ref")
        .parse(input)
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Annotation<'a> {
    pub name: AnnotationId<'a>,
    pub strs: Vec<Span<'a>>,
}

impl<'a> Annotation<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (name, strs)) = paren(pair(
            ws(AnnotationId::parse),
            alt((collect_separated_terminated(
                take_till1(|c: char| c.is_whitespace() || c == ')'),
                multispace1,
                multispace0.terminated(char(')')).peek(),
            )
            .cut(),)),
        ))
        .context("annotation")
        .parse(input)?;

        Ok((input, Self { name, strs }))
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Type<'a> {
    // Fundamental value types.
    Handle,

    // Specialized value types.
    Flags(Flags<'a>),
    Result(ResultType<'a>),
    String,
}

impl<'a> Type<'a> {
    pub fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        alt((
            paren(tag("handle")).value(Self::Handle),
            Flags::parse.map(Self::Flags),
            ResultType::parse.map(Self::Result),
            tag("string").value(Self::String),
        ))
        .cut()
        .context("type")
        .parse(input)
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Flags<'a> {
    pub repr:    Repr,
    pub members: Vec<Id<'a>>,
}

impl<'a> Flags<'a> {
    pub fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (_, (_, _, repr), members)) = paren(ws(tuple((
            ws(tag("flags")),
            ws(paren(tuple((
                ws(tag("@witx")),
                ws(tag("repr")),
                ws(Repr::parse),
            )))),
            collect_separated_terminated(
                Id::parse,
                multispace1,
                multispace0.terminated(char(')')).peek(),
            ),
        ))))
        .context("flags")
        .parse(input)?;

        Ok((input, Self { repr, members }))
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ResultType<'a> {
    pub ok:    Option<TypeRef<'a>>,
    pub error: TypeRef<'a>,
}

impl<'a> ResultType<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (_, ok, (_, error))) = paren(tuple((
            ws(tag("expected")),
            ws(TypeRef::parse).opt(),
            paren(pair(ws(tag("error")), ws(TypeRef::parse))),
        )))
        .context("result type")
        .parse(input)?;

        Ok((input, Self { ok, error }))
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Repr {
    U16,
    U32,
    U64,
}

impl Repr {
    fn parse(input: Span) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, repr) = alt((
            tag("u16").value(Self::U16),
            tag("u32").value(Self::U32),
            tag("u64").value(Self::U64),
        ))
        .cut()
        .context("repr")
        .parse(input)?;

        Ok((input, repr))
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Id<'a> {
    pos:  Span<'a>,
    name: Span<'a>,
}

impl<'a> Id<'a> {
    pub fn parse(input: Span<'a>) -> nom::IResult<Span<'a>, Self, ErrorTree<Span>> {
        let (input, pos) = position(input)?;
        let (input, (_, name)) = pair(
            char('$'),
            take_while1(|c: char| c.is_alphanumeric() || c == '_'),
        )
        .context("id")
        .parse(input)?;

        Ok((input, Self { pos, name }))
    }

    pub fn name(&self) -> &str {
        self.name.as_ref()
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct AnnotationId<'a> {
    pos:  Span<'a>,
    name: Span<'a>,
}

impl<'a> AnnotationId<'a> {
    pub fn parse(input: Span<'a>) -> nom::IResult<Span<'a>, Self, ErrorTree<Span>> {
        let (input, pos) = position(input)?;
        let (input, (_, name)) = pair(
            char('@'),
            take_while1(|c: char| c.is_alphanumeric() || c == '_'),
        )
        .context("annotation-id")
        .parse(input)?;

        Ok((input, Self { pos, name }))
    }

    pub fn name(&self) -> &str {
        self.name.as_ref()
    }
}

fn quoted_string(input: Span) -> nom::IResult<Span, Span, ErrorTree<Span>> {
    delimited(
        char('"'),
        escaped(none_of(r#"\""#), '\\', one_of(r#"\""#)),
        char('"'),
    )
    .context("quoted-string")
    .parse(input)
}

fn paren<'a, F: 'a, O, E: ParseError<Span<'a>>>(
    inner: F,
) -> impl FnMut(Span<'a>) -> nom::IResult<Span, O, E>
where
    F: nom::Parser<Span<'a>, O, E>,
{
    delimited(char('('), inner, char(')'))
}

fn ws<'a, F: 'a, O, E: ParseError<Span<'a>>>(
    inner: F,
) -> impl FnMut(Span<'a>) -> nom::IResult<Span, O, E>
where
    F: FnMut(Span<'a>) -> nom::IResult<Span, O, E>,
{
    delimited(multispace0, inner, multispace0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok() {
        struct Case {
            input:  &'static str,
            assert: Box<dyn FnOnce(Document)>,
        }

        let cases = [
            Case {
                input:  "",
                assert: Box::new(|doc| {
                    assert!(doc.modules.is_empty());
                }),
            },
            Case {
                input:  "(module)",
                assert: Box::new(|doc| {
                    assert_eq!(doc.modules.len(), 1);
                    assert!(doc.modules[0].name.is_none());
                    assert!(doc.modules[0].decls.is_empty());
                }),
            },
            Case {
                input:  "(module $wasi_snapshot_preview1)",
                assert: Box::new(|doc| {
                    assert_eq!(
                        doc.modules[0].name.as_ref().unwrap().name(),
                        "wasi_snapshot_preview1"
                    );
                }),
            },
            Case {
                input:  include_str!("testdata/00.witx"),
                assert: Box::new(|doc| {
                    assert!(matches!(
                        &doc.modules[0].decls[0],
                        Decl::Typename(Typename {
                            name: id,
                            ty: Type::Handle,
                            annotations,
                        }) if id.name() == "fd" && annotations.is_empty(),
                    ));
                }),
            },
            Case {
                input:  include_str!("testdata/01.witx"),
                assert: Box::new(|doc| {
                    assert!(matches!(
                        &doc.modules[0].decls[1],
                        Decl::Typename(Typename {
                            name: id,
                            ty: Type::Handle,
                            annotations
                        }) if id.name() == "fd_reg" &&
                            matches!(
                                &annotations[..],
                                [Annotation { name, strs }]
                                    if name.name() == "wazzi"
                                        && *strs[0] == "fulfills"
                                        && *strs[1] == "$fd"
                            )
                    ));
                }),
            },
            Case {
                input:  include_str!("testdata/02.witx"),
                assert: Box::new(|doc| {
                    assert!(matches!(
                        &doc.modules[0].decls[0],
                        Decl::Typename(Typename {
                            name: id,
                            ty: Type::Flags(Flags { repr: Repr::U32, members }),
                            annotations: _,
                        }) if id.name() == "lookupflags"
                            && matches!(
                                &members[..],
                                [bit0] if bit0.name() == "symlink_follow"
                            )
                    ));
                }),
            },
            Case {
                input:  include_str!("testdata/03.witx"),
                assert: Box::new(|doc| {
                    assert!(matches!(
                        &doc.modules[0].decls[0],
                        Decl::Typename(Typename {
                            name:        _,
                            ty:          Type::String,
                            annotations: _,
                        })
                    ));
                }),
            },
            Case {
                input:  include_str!("testdata/04.witx"),
                assert: Box::new(|doc| {
                    assert!(matches!(
                        &doc.modules[0]
                            .decls
                            .iter()
                            .find(|decl| match decl {
                                | Decl::Typename(_) => false,
                                | Decl::Func(_func) => true,
                            })
                            .unwrap(),
                        Decl::Func(Func {
                            name,
                            params: _,
                            results: _,
                        })
                            if **name == "path_open"
                    ));
                }),
            },
        ];

        for (i, case) in cases.into_iter().enumerate() {
            let result = Document::parse(Span::new(case.input));

            match result {
                | Ok(doc) => (case.assert)(doc),
                | Err(err) => {
                    panic!("{i}: {err}")
                },
            }
        }
    }
}
