use nom::{
    branch::alt,
    bytes::complete::{escaped, tag, take_while1},
    character::complete::{char, multispace0, multispace1, none_of, one_of},
    combinator::{eof, peek},
    error::ParseError,
    sequence::{delimited, pair, tuple},
    Parser,
};
use nom_locate::position;
use nom_supreme::{
    error::ErrorTree,
    final_parser::final_parser,
    multi::collect_separated_terminated,
    ParserExt,
};
use num_bigint::BigInt;

use super::Span;
use crate::{
    package::{self, Defvaltype, Package, StateEffect},
    Error,
};

#[derive(PartialEq, Eq, Debug, Clone)]
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

    pub fn into_package(self) -> Result<Package, Error> {
        let mut pkg = Package::new();

        for module in self.modules {
            let name = module.id.clone().map(|id| id.name().to_owned());
            let interface = module.into_package()?;

            pkg.register_interface(interface, name);
        }

        Ok(pkg)
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
struct Module<'a> {
    pos:   KeywordSpan<'a>,
    id:    Option<Id<'a>>,
    decls: Vec<Decl<'a>>,
}

impl<'a> Module<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (pos, id, decls)) = paren(tuple((
            ws(KeywordSpan::parse),
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
        )))
        .context("module")
        .parse(input)?;

        Ok((input, Self { pos, id, decls }))
    }

    fn into_package(self) -> Result<package::Interface, Error> {
        let mut interface = package::Interface::new();
        let mut typenames = Vec::new();
        let mut functions = Vec::new();

        for decl in self.decls {
            match decl {
                | Decl::Typename(typename) => typenames.push(typename),
                | Decl::Function(function) => functions.push(function),
            }
        }

        for typename in typenames {
            let defvaltype = typename.ty.into_package(&interface)?;
            let resource_name = typename.id.map(|id| id.name().to_owned());

            interface.register_resource(defvaltype, resource_name)?;
        }

        for function in functions {
            interface.register_function(
                function.name.to_string(),
                function.into_package(&interface)?,
            )?;
        }

        Ok(interface)
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
enum Decl<'a> {
    Typename(Typename<'a>),
    Function(Function<'a>),
}

impl<'a> Decl<'a> {
    pub fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        alt((
            ws(Typename::parse).map(Self::Typename),
            ws(Function::parse).map(Self::Function),
        ))
        .context("decl")
        .parse(input)
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct Typename<'a> {
    pos:         KeywordSpan<'a>,
    id:          Option<Id<'a>>,
    ty:          Type<'a>,
    annotations: Vec<Annotation<'a>>,
}

impl<'a> Typename<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (pos, id, ty, annotations)) = paren(tuple((
            ws(KeywordSpan::parse).verify(|span| span.keyword == Keyword::Typename),
            ws(Id::parse).opt(),
            ws(Type::parse),
            alt((
                collect_separated_terminated(
                    Annotation::parse,
                    multispace0,
                    multispace0.terminated(char(')')).peek(),
                ),
                peek::<_, _, ErrorTree<_>, _>(char(')')).value(vec![]),
            )),
        )))
        .context("typename")
        .parse(input)?;

        Ok((
            input,
            Self {
                pos,
                id,
                ty,
                annotations,
            },
        ))
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct Function<'a> {
    name:        Span<'a>,
    params:      Vec<FuncParam<'a>>,
    results:     Vec<FuncResult<'a>>,
    annotations: Vec<Annotation<'a>>,
}

impl<'a> Function<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (_, _, (_, name), params, results, annotations)) = paren(tuple((
            ws(tag("@interface")),
            ws(KeywordSpan::parse.verify(|span| span.keyword == Keyword::Func)),
            ws(paren(pair(
                ws(KeywordSpan::parse.verify(|span| span.keyword == Keyword::Export)),
                ws(quoted_string),
            ))),
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
                multispace0
                    .terminated(alt((char(')').value(()), ws(Annotation::parse).value(()))))
                    .peek(),
            ),
            alt((
                collect_separated_terminated(
                    ws(Annotation::parse),
                    multispace0,
                    multispace0.terminated(char(')')).peek(),
                ),
                peek::<_, _, ErrorTree<_>, _>(char(')')).value(vec![]),
            )),
        )))
        .context("func")
        .parse(input)?;

        Ok((
            input,
            Self {
                name,
                params,
                results,
                annotations,
            },
        ))
    }

    fn into_package(self, interface: &package::Interface) -> Result<package::Function, Error> {
        Ok(package::Function {
            name:    self.name.to_string(),
            params:  self
                .params
                .into_iter()
                .map(|p| p.into_package(interface))
                .collect::<Result<Vec<_>, _>>()?,
            results: self
                .results
                .into_iter()
                .map(|r| r.into_package(interface))
                .collect::<Result<Vec<_>, _>>()?,
            spec:    self
                .annotations
                .iter()
                .find(|annot| {
                    *annot.span.name == "wazzi"
                        && matches!(
                            annot.exprs.first(),
                            Some(Expr::SExpr(exprs))
                            if matches!(
                                exprs.first(),
                                Some(Expr::Keyword(span))
                                if span.keyword == Keyword::Spec
                            )
                        )
                })
                .map(|annot| {
                    let exprs = match annot.exprs.first().unwrap() {
                        | Expr::SExpr(exprs) => exprs,
                        | _ => panic!(),
                    };
                    let expr = exprs.get(1).unwrap();

                    expr.to_owned().into_constraint()
                }),
        })
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct FuncParam<'a> {
    name:        Id<'a>,
    tref:        TypeRef<'a>,
    annotations: Vec<Annotation<'a>>,
}

impl<'a> FuncParam<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (_, name, tref, annotations)) = paren(tuple((
            ws(KeywordSpan::parse.verify(|span| span.keyword == Keyword::Param)),
            ws(Id::parse),
            ws(TypeRef::parse),
            alt((
                collect_separated_terminated(
                    Annotation::parse,
                    multispace0,
                    multispace0.terminated(char(')')).peek(),
                ),
                peek::<_, _, ErrorTree<_>, _>(char(')')).value(vec![]),
            )),
        )))
        .context("func-param")
        .parse(input)?;

        Ok((
            input,
            Self {
                name,
                tref,
                annotations,
            },
        ))
    }

    fn into_package(self, interface: &package::Interface) -> Result<package::FunctionParam, Error> {
        Ok(package::FunctionParam {
            name:         self.name.name().to_owned(),
            valtype:      self.tref.into_package(interface)?,
            state_effect: self
                .annotations
                .iter()
                .find_map(|annot| {
                    if *annot.span.name != "wazzi" {
                        return None;
                    }

                    let exprs = match annot.exprs.first()? {
                        | Expr::SExpr(exprs) => exprs,
                        | _ => return None,
                    };

                    if !matches!(exprs.first()?, Expr::Keyword(span) if span.keyword == Keyword::State) {
                        return None;
                    }

                    match exprs.get(1)? {
                        Expr::Keyword(span) => match span.keyword {
                            Keyword::Read => Some(Ok(StateEffect::Read)),
                            Keyword::Write => Some(Ok(StateEffect::Write)),
                            _ => Some(Err(Error::UnexpectedToken { token: span.pos.to_string(), offset: span.pos.location_offset() })),
                        },
                        _ => None,
                    }
                })
                .transpose()?
                .unwrap_or(StateEffect::Read),
        })
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct FuncResult<'a> {
    name:        Id<'a>,
    tref:        TypeRef<'a>,
    annotations: Vec<Annotation<'a>>,
}

impl<'a> FuncResult<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (_, name, tref, annotations)) = paren(tuple((
            ws(KeywordSpan::parse.verify(|span| span.keyword == Keyword::Result)),
            ws(Id::parse),
            ws(TypeRef::parse),
            alt((
                collect_separated_terminated(
                    Annotation::parse,
                    multispace0,
                    multispace0.terminated(char(')')).peek(),
                ),
                peek::<_, _, ErrorTree<_>, _>(char(')')).value(vec![]),
            )),
        )))
        .context("func-result")
        .parse(input)?;

        Ok((
            input,
            Self {
                name,
                tref,
                annotations,
            },
        ))
    }

    fn into_package(
        self,
        interface: &package::Interface,
    ) -> Result<package::FunctionResult, Error> {
        Ok(package::FunctionResult {
            name:    self.name.name().to_owned(),
            valtype: self.tref.into_package(interface)?,
        })
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
enum TypeRef<'a> {
    Numeric(u32),
    Symbolic(Id<'a>),
    Type(Box<Type<'a>>),
}

impl<'a> TypeRef<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        alt((
            uint32.map(Self::Numeric),
            Id::parse.map(Self::Symbolic),
            Type::parse.map(|ty| Self::Type(Box::new(ty))),
        ))
        .context("type-ref")
        .parse(input)
    }

    fn into_package(self, interface: &package::Interface) -> Result<package::Valtype, Error> {
        match self {
            | TypeRef::Numeric(idx) => {
                if interface
                    .get_resource_type(package::TypeidxBorrow::Numeric(idx))
                    .is_none()
                {
                    return Err(Error::InvalidTypeidx(package::Typeidx::Numeric(idx)));
                }

                Ok(package::Valtype::Typeidx(package::Typeidx::Numeric(idx)))
            },
            | TypeRef::Symbolic(id) => {
                if interface
                    .get_resource_type(package::TypeidxBorrow::Symbolic(id.name()))
                    .is_none()
                {
                    return Err(Error::InvalidTypeidx(package::Typeidx::Symbolic(
                        id.name().to_owned(),
                    )));
                }

                Ok(package::Valtype::Typeidx(package::Typeidx::Symbolic(
                    id.name().to_owned(),
                )))
            },
            | TypeRef::Type(ty) => Ok(package::Valtype::Defvaltype(ty.into_package(interface)?)),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
enum Type<'a> {
    // Fundamental numerical value types
    S64(KeywordSpan<'a>),
    U8(KeywordSpan<'a>),
    U16(KeywordSpan<'a>),
    U32(KeywordSpan<'a>),
    U64(KeywordSpan<'a>),

    // Container value types
    Record(RecordType<'a>),
    Enum(EnumType<'a>),
    Union(UnionType<'a>),
    List(ListType<'a>),
    Handle(KeywordSpan<'a>),

    // Specialized value types.
    Flags(FlagsType<'a>),
    Result(ResultType<'a>),
    String,
}

impl<'a> Type<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        alt((
            KeywordSpan::parse
                .verify(|span| span.keyword == Keyword::S64)
                .map(|span| Self::S64(span)),
            KeywordSpan::parse
                .verify(|span| span.keyword == Keyword::U8)
                .map(|span| Self::U8(span)),
            KeywordSpan::parse
                .verify(|span| span.keyword == Keyword::U16)
                .map(|span| Self::U16(span)),
            KeywordSpan::parse
                .verify(|span| span.keyword == Keyword::U32)
                .map(|span| Self::U32(span)),
            KeywordSpan::parse
                .verify(|span| span.keyword == Keyword::U64)
                .map(|span| Self::U64(span)),
            RecordType::parse.map(Self::Record),
            EnumType::parse.map(Self::Enum),
            UnionType::parse.map(Self::Union),
            ListType::parse.map(Self::List),
            paren(ws(KeywordSpan::parse))
                .verify(|span| span.keyword == Keyword::Handle)
                .map(|span| Self::Handle(span)),
            FlagsType::parse.map(Self::Flags),
            ResultType::parse.map(Self::Result),
            tag("string").value(Self::String),
        ))
        .context("type")
        .parse(input)
    }

    fn into_package(self, interface: &package::Interface) -> Result<Defvaltype, Error> {
        Ok(match self {
            | Type::S64(_) => Defvaltype::S64,
            | Type::U8(_) => Defvaltype::U8,
            | Type::U16(_) => Defvaltype::U16,
            | Type::U32(_) => Defvaltype::U32,
            | Type::U64(_) => Defvaltype::U64,
            | Type::Record(record) => Defvaltype::Record(record.into_package(interface)?),
            | Type::Enum(e) => Defvaltype::Variant(e.into()),
            | Type::Union(union) => Defvaltype::Variant(union.into_package(interface)?),
            | Type::List(list) => Defvaltype::List(Box::new(list.into_package(interface)?)),
            | Type::Handle(_) => Defvaltype::Handle,
            | Type::Flags(flags) => Defvaltype::Flags(package::FlagsType {
                repr:    flags.repr.into(),
                members: flags
                    .members
                    .into_iter()
                    .map(|field| field.name().to_owned())
                    .collect(),
            }),
            | Type::Result(result) => Defvaltype::Result(Box::new(result.into_package(interface)?)),
            | Type::String => Defvaltype::String,
        })
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct RecordType<'a> {
    fields: Vec<RecordField<'a>>,
}

impl<'a> RecordType<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (_, fields)) = paren(tuple((
            ws(tag("record")),
            ws(collect_separated_terminated(
                RecordField::parse,
                multispace0,
                multispace0.terminated(char(')')).peek(),
            )),
        )))
        .parse(input)?;

        Ok((input, Self { fields }))
    }

    fn into_package(self, interface: &package::Interface) -> Result<package::Record, Error> {
        Ok(package::Record {
            members: self
                .fields
                .into_iter()
                .map(|field| field.into_package(interface))
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct EnumType<'a> {
    repr:  Repr<'a>,
    cases: Vec<Id<'a>>,
}

impl<'a> EnumType<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (_, (_, _, repr), cases)) = paren(tuple((
            ws(KeywordSpan::parse.verify(|span| span.keyword == Keyword::Enum)),
            paren(tuple((ws(tag("@witx")), ws(tag("tag")), ws(Repr::parse)))),
            ws(collect_separated_terminated(
                Id::parse,
                multispace1,
                multispace0.terminated(char(')')).peek(),
            )),
        )))
        .context("variant")
        .parse(input)?;

        Ok((input, Self { repr, cases }))
    }
}

impl From<EnumType<'_>> for package::Variant {
    fn from(value: EnumType) -> Self {
        package::Variant {
            tag_repr: value.repr.into(),
            cases:    value
                .cases
                .into_iter()
                .map(|id| package::VariantCase {
                    name:    id.name().to_owned(),
                    payload: None,
                })
                .collect(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct UnionType<'a> {
    tag:   Id<'a>,
    cases: Vec<Id<'a>>,
}

impl<'a> UnionType<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (_, (_, _, tag), cases)) = paren(tuple((
            ws(KeywordSpan::parse.verify(|span| span.keyword == Keyword::Union)),
            ws(paren(tuple((
                ws(tag("@witx")),
                ws(KeywordSpan::parse.verify(|span| span.keyword == Keyword::Tag)),
                ws(Id::parse),
            )))),
            ws(collect_separated_terminated(
                Id::parse,
                multispace1,
                multispace0.terminated(char(')')).peek(),
            )),
        )))
        .context("union")
        .parse(input)?;

        Ok((input, Self { tag, cases }))
    }

    fn into_package(self, interface: &package::Interface) -> Result<package::Variant, Error> {
        let tag_def = interface
            .get_resource_type(package::TypeidxBorrow::Symbolic(self.tag.name()))
            .ok_or(Error::InvalidTypeidx(package::Typeidx::Symbolic(
                self.tag.name().to_owned(),
            )))?;
        let tag_variant = match tag_def {
            | Defvaltype::Variant(variant) => variant,
            | _ => unreachable!(),
        };

        Ok(package::Variant {
            tag_repr: tag_variant.tag_repr,
            cases:    tag_variant
                .cases
                .iter()
                .zip(self.cases)
                .map(|(tag, case)| {
                    interface
                        .get_resource_type(package::TypeidxBorrow::Symbolic(case.name()))
                        .ok_or(Error::InvalidTypeidx(package::Typeidx::Symbolic(
                            case.name().to_owned(),
                        )))?;

                    Ok(package::VariantCase {
                        name:    tag.name.clone(),
                        payload: Some(package::Valtype::Typeidx(package::Typeidx::Symbolic(
                            case.name().to_owned(),
                        ))),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct RecordField<'a> {
    name: Id<'a>,
    tref: TypeRef<'a>,
}

impl<'a> RecordField<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (_, name, tref)) = paren(tuple((
            ws(KeywordSpan::parse.verify(|span| span.keyword == Keyword::Field)),
            ws(Id::parse),
            ws(TypeRef::parse),
        )))
        .parse(input)?;

        Ok((input, Self { name, tref }))
    }

    fn into_package(self, interface: &package::Interface) -> Result<package::RecordMember, Error> {
        Ok(package::RecordMember {
            name: self.name.name().to_owned(),
            ty:   self.tref.into_package(interface)?,
        })
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ListType<'a> {
    tref: TypeRef<'a>,
}

impl<'a> ListType<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (_, tref)) = paren(pair(
            ws(KeywordSpan::parse.verify(|span| span.keyword == Keyword::List)),
            ws(TypeRef::parse),
        ))
        .context("list")
        .parse(input)?;

        Ok((input, Self { tref }))
    }

    fn into_package(self, interface: &package::Interface) -> Result<package::ListType, Error> {
        Ok(package::ListType {
            element: self.tref.into_package(interface)?,
        })
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FlagsType<'a> {
    pub repr:    Repr<'a>,
    pub members: Vec<Id<'a>>,
}

impl<'a> FlagsType<'a> {
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

impl From<FlagsType<'_>> for package::FlagsType {
    fn from(value: FlagsType) -> Self {
        Self {
            repr:    value.repr.into(),
            members: value
                .members
                .into_iter()
                .map(|member| member.name().to_owned())
                .collect(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct ResultType<'a> {
    ok:    Option<TypeRef<'a>>,
    error: TypeRef<'a>,
}

impl<'a> ResultType<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (_, ok, (_, error))) = paren(tuple((
            ws(KeywordSpan::parse.verify(|span| span.keyword == Keyword::Expected)),
            ws(TypeRef::parse.context("result-type-ok")).opt(),
            ws(paren(pair(
                ws(KeywordSpan::parse.verify(|span| span.keyword == Keyword::Error)),
                ws(TypeRef::parse),
            )))
            .context("result-type-error"),
        )))
        .context("result-type")
        .parse(input)?;

        Ok((input, Self { ok, error }))
    }

    fn into_package(self, interface: &package::Interface) -> Result<package::ResultType, Error> {
        Ok(package::ResultType {
            ok:    self
                .ok
                .map(|tref| tref.into_package(interface))
                .transpose()?,
            error: self.error.into_package(interface)?,
        })
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Repr<'a> {
    U8(KeywordSpan<'a>),
    U16(KeywordSpan<'a>),
    U32(KeywordSpan<'a>),
    U64(KeywordSpan<'a>),
}

impl<'a> Repr<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, repr) = alt((
            KeywordSpan::parse
                .verify(|span| span.keyword == Keyword::U8)
                .map(Self::U8),
            KeywordSpan::parse
                .verify(|span| span.keyword == Keyword::U16)
                .map(Self::U16),
            KeywordSpan::parse
                .verify(|span| span.keyword == Keyword::U32)
                .map(Self::U32),
            KeywordSpan::parse
                .verify(|span| span.keyword == Keyword::U64)
                .map(Self::U64),
        ))
        .cut()
        .context("repr")
        .parse(input)?;

        Ok((input, repr))
    }
}

impl From<Repr<'_>> for package::IntRepr {
    fn from(value: Repr) -> Self {
        match value {
            | Repr::U8(_) => Self::U8,
            | Repr::U16(_) => Self::U16,
            | Repr::U32(_) => Self::U32,
            | Repr::U64(_) => Self::U64,
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
enum Expr<'a> {
    Annotation(AnnotationSpan<'a>),
    SymbolicIdx(Id<'a>),
    Keyword(KeywordSpan<'a>),
    NumLit(NumLit<'a>),
    SExpr(Vec<Expr<'a>>),
}

impl<'a> Expr<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, expr) = alt((
            AnnotationSpan::parse.map(Self::Annotation),
            Id::parse.map(Self::SymbolicIdx),
            KeywordSpan::parse.map(Self::Keyword),
            NumLit::parse.map(Self::NumLit),
            paren(alt((
                collect_separated_terminated(
                    ws(Self::parse),
                    multispace0,
                    multispace0.terminated(char(')')).peek(),
                ),
                char(')').peek().value(vec![]),
            )))
            .map(|exprs| Self::SExpr(exprs)),
        ))
        .context("expr")
        .parse(input)?;

        Ok((input, expr))
    }

    fn into_constraint(self) -> wazzi_spec_constraint::Program {
        fn into_unspecified(e: Expr) -> wazzi_spec_constraint::program::Unspecified {
            let exprs = match e {
                | Expr::SExpr(exprs) => exprs,
                | _ => panic!(),
            };
            let exprs = match exprs[1].clone() {
                | Expr::SExpr(exprs) => exprs,
                | _ => panic!(),
            };
            let exprs = match exprs[1].clone() {
                | Expr::SExpr(exprs) => exprs,
                | _ => panic!(),
            };
            let name = match exprs[1].clone() {
                | Expr::SymbolicIdx(s) => s.name().to_owned(),
                | _ => panic!("{:?}", exprs[1]),
            };

            wazzi_spec_constraint::program::Unspecified {
                tref: wazzi_spec_constraint::program::TypeRef::Result { name },
            }
        }

        fn into_expr(e: Expr) -> wazzi_spec_constraint::program::Expr {
            match e {
                | Expr::Annotation(_) => todo!(),
                | Expr::SymbolicIdx(_) => todo!(),
                | Expr::Keyword(_) => todo!(),
                | Expr::NumLit(_) => todo!(),
                | Expr::SExpr(exprs) => {
                    match exprs.first().unwrap() {
                        | Expr::Keyword(span) => match span.keyword {
                            | Keyword::I64Const => wazzi_spec_constraint::program::Expr::Number(
                                BigInt::from(match &exprs[1] {
                                    | Expr::NumLit(num) => num.0.parse::<u64>().unwrap(),
                                    | _ => panic!(),
                                }),
                            ),
                            | Keyword::I64GtU => wazzi_spec_constraint::program::Expr::U64Gt(
                                Box::new(wazzi_spec_constraint::program::U64Gt {
                                    lhs: into_expr(exprs[1].clone()),
                                    rhs: into_expr(exprs[2].clone()),
                                }),
                            ),
                            | Keyword::If => wazzi_spec_constraint::program::Expr::If(Box::new(
                                wazzi_spec_constraint::program::If {
                                    cond: into_expr(exprs[1].clone()),
                                    then: into_unspecified(exprs[2].clone()),
                                },
                            )),
                            | Keyword::Param => wazzi_spec_constraint::program::Expr::TypeRef(
                                wazzi_spec_constraint::program::TypeRef::Param {
                                    name: match &exprs[1] {
                                        | Expr::SymbolicIdx(s) => s.name().to_owned(),
                                        | _ => panic!(),
                                    },
                                },
                            ),
                            | _ => panic!("{:?}", span),
                        },
                        | _ => panic!(),
                    }
                },
            }
        }

        wazzi_spec_constraint::Program {
            expr: into_expr(self),
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Annotation<'a> {
    span:  AnnotationSpan<'a>,
    exprs: Vec<Expr<'a>>,
}

impl<'a> Annotation<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (span, exprs)) = paren(pair(
            ws(AnnotationSpan::parse),
            alt((
                collect_separated_terminated(
                    Expr::parse,
                    multispace0,
                    multispace0.terminated(char(')')).peek(),
                ),
                peek::<_, _, ErrorTree<_>, _>(char(')')).value(vec![]),
            ))
            .cut(),
        ))
        .context("annotation")
        .parse(input)?;

        Ok((input, Self { span, exprs }))
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct AnnotationSpan<'a> {
    pos:  Span<'a>,
    name: Span<'a>,
}

impl<'a> AnnotationSpan<'a> {
    pub fn parse(input: Span<'a>) -> nom::IResult<Span<'a>, Self, ErrorTree<Span>> {
        let (input, pos) = position(input)?;
        let (input, (_, name)) = pair(
            char('@'),
            take_while1(|c: char| c.is_alphanumeric() || c == '_' || c == '-'),
        )
        .context("annotation-span")
        .parse(input)?;

        Ok((input, Self { pos, name }))
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct KeywordSpan<'a> {
    pos:     Span<'a>,
    keyword: Keyword,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum Keyword {
    Enum,
    Error,
    Expected,
    Export,
    Field,
    Flags,
    Fulfills,
    Func,
    Handle,
    I64Const,
    I64GtU,
    I64LeU,
    If,
    List,
    Module,
    Param,
    Read,
    Result,
    S64,
    Spec,
    State,
    Tag,
    Then,
    Typename,
    U8,
    U16,
    U32,
    U64,
    Union,
    Unspecified,
    Write,
}

impl<'a> KeywordSpan<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, (span, keyword)) = alt((
            alt((
                tag("enum").map(|s| (s, Keyword::Enum)),
                tag("error").map(|s| (s, Keyword::Error)),
                tag("expected").map(|s| (s, Keyword::Expected)),
                tag("export").map(|s| (s, Keyword::Export)),
                tag("field").map(|s| (s, Keyword::Field)),
                tag("flags").map(|s| (s, Keyword::Flags)),
                tag("fulfills").map(|s| (s, Keyword::Fulfills)),
                tag("func").map(|s| (s, Keyword::Func)),
                tag("handle").map(|s| (s, Keyword::Handle)),
                tag("i64.const").map(|s| (s, Keyword::I64Const)),
                tag("i64.gt_u").map(|s| (s, Keyword::I64GtU)),
                tag("i64.le_u").map(|s| (s, Keyword::I64LeU)),
                tag("if").map(|s| (s, Keyword::If)),
                tag("list").map(|s| (s, Keyword::List)),
                tag("module").map(|s| (s, Keyword::Module)),
                tag("param").map(|s| (s, Keyword::Param)),
                tag("read").map(|s| (s, Keyword::Read)),
                tag("result").map(|s| (s, Keyword::Result)),
                tag("s64").map(|s| (s, Keyword::S64)),
                tag("spec").map(|s| (s, Keyword::Spec)),
                tag("state").map(|s| (s, Keyword::State)),
            )),
            alt((
                tag("tag").map(|s| (s, Keyword::Tag)),
                tag("then").map(|s| (s, Keyword::Then)),
                tag("typename").map(|s| (s, Keyword::Typename)),
                tag("u8").map(|s| (s, Keyword::U8)),
                tag("u16").map(|s| (s, Keyword::U16)),
                tag("u32").map(|s| (s, Keyword::U32)),
                tag("u64").map(|s| (s, Keyword::U64)),
                tag("union").map(|s| (s, Keyword::Union)),
                tag("unspecified").map(|s| (s, Keyword::Unspecified)),
                tag("write").map(|s| (s, Keyword::Write)),
            )),
        ))
        .context("keyword")
        .parse(input)?;

        Ok((input, Self { pos: span, keyword }))
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
struct NumLit<'a>(Span<'a>);

impl<'a> NumLit<'a> {
    fn parse(input: Span<'a>) -> nom::IResult<Span, Self, ErrorTree<Span>> {
        let (input, span) = take_while1(|c: char| c.is_numeric())
            .context("numeric-literal")
            .parse(input)?;

        Ok((input, Self(span)))
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

fn uint32(input: Span) -> nom::IResult<Span, u32, ErrorTree<Span>> {
    take_while1(|c: char| c.is_numeric())
        .map_res(|res: Span| u32::from_str_radix(*res, 10))
        .parse(input)
}

fn ws<'a, F: 'a, O, E: ParseError<Span<'a>>>(
    inner: F,
) -> impl FnMut(Span<'a>) -> nom::IResult<Span, O, E>
where
    F: nom::Parser<Span<'a>, O, E>,
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
                    assert!(doc.modules[0].id.is_none());
                    // assert!(doc.modules[0].decls.is_empty());
                }),
            },
            Case {
                input:  "(module $wasi_snapshot_preview1)",
                assert: Box::new(|doc| {
                    assert_eq!(
                        doc.modules[0].id.as_ref().unwrap().name(),
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
                            pos: _,
                            id,
                            ty,
                            ..
                        }) if id.as_ref().unwrap().name() == "fd" &&
                            matches!(ty, Type::Handle(_)),
                    ));
                }),
            },
            Case {
                input:  include_str!("testdata/01.witx"),
                assert: Box::new(|doc| {
                    assert!(matches!(
                        &doc.modules[0].decls[1],
                        Decl::Typename(Typename {
                            pos: _,
                            id,
                            ty: _,
                            annotations,
                        }) if id.as_ref().unwrap().name() == "fd_reg" &&
                            matches!(
                                &annotations[..],
                                [Annotation { span: _, exprs }]
                                if matches!(
                                    &exprs[..],
                                    [Expr::Keyword(span), Expr::SymbolicIdx(idx)]
                                    if span.keyword == Keyword::Fulfills
                                        && idx.name() == "fd"
                                )
                            )
                    ));
                }),
            },
            Case {
                input:  include_str!("testdata/02.witx"),
                assert: Box::new(|_doc| {}),
            },
            Case {
                input:  include_str!("testdata/03.witx"),
                assert: Box::new(|_doc| {}),
            },
            Case {
                input:  include_str!("testdata/04.witx"),
                assert: Box::new(|_doc| {}),
            },
            Case {
                input:  include_str!("testdata/05.witx"),
                assert: Box::new(|_doc| {}),
            },
            Case {
                input:  include_str!("testdata/06.witx"),
                assert: Box::new(|_doc| {}),
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
