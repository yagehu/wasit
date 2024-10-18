use std::str::FromStr;

use eyre::Context as _;
use itertools::Itertools;
use num_bigint::BigInt;
use pest::iterators::Pair;
use pest_derive::Parser;

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) enum Term {
    Foldl(Box<Foldl>),
    Lambda(Box<Lambda>),
    Map(Box<Map>),
    Binding(String),

    True,
    String(String),
    Not(Box<Not>),
    And(And),
    Or(Or),

    RecordField(Box<RecordField>),
    Param(Param),
    Result(Param),
    ResourceId(String),

    FlagsGet(Box<FlagsGet>),
    ListLen(Box<ListLen>),
    IntWrap(Box<IntWrap>),
    IntConst(BigInt),
    IntAdd(Box<IntAdd>),
    IntGt(Box<IntGt>),
    IntLe(Box<IntLe>),
    StrAt(Box<BinaryTerm>),
    U64Const(Box<UnaryTerm>),
    ValueEq(Box<ValueEq>),
    VariantConst(Box<VariantConst>),

    NoNonExistentDirBacktrack(Box<NoNonExistentDirBacktrack>),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct UnaryTerm {
    pub(crate) term: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct BinaryTerm {
    pub(crate) lhs: Term,
    pub(crate) rhs: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct Foldl {
    pub(crate) target: Term,
    pub(crate) acc:    Term,
    pub(crate) func:   Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct Lambda {
    pub(crate) bounds: Vec<Bound>,
    pub(crate) body:   Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct Map {
    pub(crate) target: Term,
    pub(crate) func:   Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct Bound {
    pub(crate) name: String,
    pub(crate) tref: TypeRef,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) enum TypeRef {
    Wasi(String),
    Wazzi(WazziType),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) enum WazziType {
    Bool,
    Int,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct Not {
    pub(crate) term: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct And {
    pub(crate) clauses: Vec<Term>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct Or {
    pub(crate) clauses: Vec<Term>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct RecordField {
    pub(crate) target: Term,
    pub(crate) member: String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct Param {
    pub(crate) name: String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct FlagsGet {
    pub(crate) target: Term,
    pub(crate) field:  String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct ListLen {
    pub(crate) op: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct IntWrap {
    pub(crate) op: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct IntAdd {
    pub(crate) lhs: Term,
    pub(crate) rhs: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct IntGt {
    pub(crate) lhs: Term,
    pub(crate) rhs: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct IntLe {
    pub(crate) lhs: Term,
    pub(crate) rhs: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct ValueEq {
    pub(crate) lhs: Term,
    pub(crate) rhs: Term,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct VariantConst {
    pub(crate) ty:      String,
    pub(crate) case:    String,
    pub(crate) payload: Option<Term>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct NoNonExistentDirBacktrack {
    pub(crate) fd_param:   String,
    pub(crate) path_param: String,
}

#[derive(Parser)]
#[grammar = "spec/witx/slang.pest"]
pub(super) struct Parser;

pub(super) fn to_term(pair: Pair<'_, Rule>) -> Result<Term, eyre::Error> {
    Ok(match pair.as_rule() {
        | Rule::id => Term::Binding(pair.as_str().strip_prefix('$').unwrap().to_string()),
        | Rule::r#true => Term::True,
        | Rule::foldl => {
            let mut pairs = pair.into_inner();

            Term::Foldl(Box::new(Foldl {
                target: to_term(pairs.next().unwrap())?,
                acc:    to_term(pairs.next().unwrap())?,
                func:   to_term(pairs.next().unwrap())?,
            }))
        },
        | Rule::lambda => {
            let mut pairs = pair.into_inner().collect_vec();
            let body_pair = pairs.pop().unwrap();
            let bounds = pairs
                .into_iter()
                .map(|p| {
                    let mut pairs = p.into_inner();
                    let id = pairs.next().unwrap();
                    let tref = pairs.next().unwrap();
                    let tref = match tref.as_rule() {
                        | Rule::id => {
                            TypeRef::Wasi(tref.as_str().strip_prefix('$').unwrap().to_string())
                        },
                        | Rule::r#bool => TypeRef::Wazzi(WazziType::Bool),
                        | Rule::int => TypeRef::Wazzi(WazziType::Int),
                        | _ => panic!("{:#?}", tref),
                    };

                    Bound {
                        name: id.as_str().strip_prefix('$').unwrap().to_string(),
                        tref,
                    }
                })
                .collect_vec();

            Term::Lambda(Box::new(Lambda {
                bounds,
                body: to_term(body_pair)?,
            }))
        },
        | Rule::map => {
            let mut pairs = pair.into_inner();

            Term::Map(Box::new(Map {
                target: to_term(pairs.next().unwrap())?,
                func:   to_term(pairs.next().unwrap())?,
            }))
        },
        | Rule::not => Term::Not(Box::new(Not {
            term: to_term(pair.into_inner().next().unwrap())?,
        })),
        | Rule::and => Term::And(And {
            clauses: pair
                .into_inner()
                .filter_map(|p| {
                    if p.as_rule() != Rule::comment {
                        Some(to_term(p))
                    } else {
                        None
                    }
                })
                .collect::<Result<_, _>>()?,
        }),
        | Rule::or => Term::Or(Or {
            clauses: pair
                .into_inner()
                .map(|p| to_term(p))
                .collect::<Result<_, _>>()?,
        }),
        | Rule::record_field => {
            let mut pairs = pair.into_inner();
            let target = to_term(pairs.next().unwrap())
                .wrap_err("failed to handle @record.field.get target")?;
            let attr = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_owned();

            Term::RecordField(Box::new(RecordField {
                target,
                member: attr,
            }))
        },
        | Rule::param => Term::Param(Param {
            name: pair
                .into_inner()
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_owned(),
        }),
        | Rule::result => Term::Result(Param {
            name: pair
                .into_inner()
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_owned(),
        }),
        | Rule::resource_id => Term::ResourceId(
            pair.into_inner()
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_string(),
        ),
        | Rule::flags_get => {
            let mut pairs = pair.into_inner();
            let target =
                to_term(pairs.next().unwrap()).wrap_err("failed to handle @flags.get target")?;
            let field = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_owned();

            Term::FlagsGet(Box::new(FlagsGet { target, field }))
        },
        | Rule::int_const => {
            let mut pairs = pair.into_inner();
            let op = to_term(pairs.next().unwrap())?;

            Term::IntWrap(Box::new(IntWrap { op }))
        },
        | Rule::int_add => {
            let mut pairs = pair.into_inner();
            let lhs = to_term(pairs.next().unwrap())?;
            let rhs = to_term(pairs.next().unwrap())?;

            Term::IntAdd(Box::new(IntAdd { lhs, rhs }))
        },
        | Rule::int_gt => {
            let mut pairs = pair.into_inner();
            let lhs = to_term(pairs.next().unwrap())?;
            let rhs = to_term(pairs.next().unwrap())?;

            Term::IntGt(Box::new(IntGt { lhs, rhs }))
        },
        | Rule::int_le => {
            let mut pairs = pair.into_inner();
            let lhs = to_term(pairs.next().unwrap())?;
            let rhs = to_term(pairs.next().unwrap())?;

            Term::IntLe(Box::new(IntLe { lhs, rhs }))
        },
        | Rule::list_len => Term::ListLen(Box::new(ListLen {
            op: to_term(pair.into_inner().next().unwrap())?,
        })),
        | Rule::num_lit => {
            let s = pair.as_str();

            Term::IntConst(BigInt::from_str(s)?)
        },
        | Rule::str_at => {
            let mut pairs = pair.into_inner();
            let lhs = pairs.next().unwrap();
            let rhs = pairs.next().unwrap();

            Term::StrAt(Box::new(BinaryTerm {
                lhs: to_term(lhs)?,
                rhs: to_term(rhs)?,
            }))
        },
        | Rule::u64_const => Term::U64Const(Box::new(UnaryTerm {
            term: to_term(pair.into_inner().next().unwrap())?,
        })),
        | Rule::string => Term::String(pair.into_inner().as_str().to_string()),
        | Rule::value_eq => {
            let mut pairs = pair.into_inner();
            let lhs = to_term(pairs.next().unwrap())?;
            let rhs = to_term(pairs.next().unwrap())?;

            Term::ValueEq(Box::new(ValueEq { lhs, rhs }))
        },
        | Rule::variant_const => {
            let mut pairs = pair.into_inner();
            let ty = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_owned();
            let case = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_owned();
            let payload = match pairs.next() {
                | Some(pair) => Some(to_term(pair)?),
                | None => None,
            };

            Term::VariantConst(Box::new(VariantConst { ty, case, payload }))
        },
        | Rule::no_nonexistent_dir_backtrack => {
            let mut pairs = pair.into_inner();
            let fd_param = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_string();
            let path_param = pairs
                .next()
                .unwrap()
                .as_str()
                .strip_prefix('$')
                .unwrap()
                .to_string();

            Term::NoNonExistentDirBacktrack(Box::new(NoNonExistentDirBacktrack {
                fd_param,
                path_param,
            }))
        },
        | _ => panic!("{:?} {:?}", pair.as_rule(), pair.as_str()),
    })
}
