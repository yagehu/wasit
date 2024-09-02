pub(super) mod ilang;
pub(super) mod olang;

use std::collections::BTreeMap;

use eyre::{eyre as err, Context as _, ContextCompat as _};
use olang::Program;
use pest::{
    iterators::{Pair, Pairs},
    Parser as _,
};
use pest_derive::Parser;

use super::{
    FlagsType,
    Function,
    FunctionParam,
    FunctionResult,
    IntRepr,
    Interface,
    ListType,
    RecordMemberType,
    RecordType,
    Spec,
    TypeRef,
    VariantCaseType,
    VariantType,
    WasiType,
};

#[derive(Parser)]
#[grammar = "spec/witx.pest"]
struct Parser;

pub(super) fn preview1<'ctx>(ctx: &'ctx z3::Context) -> Result<Spec, eyre::Error> {
    const DOC: &str = include_str!("preview1.witx");

    let mut spec = Spec::new(ctx);
    let doc = Parser::parse(Rule::document, DOC)
        .wrap_err("failed to parse document")?
        .next()
        .unwrap();

    for pair in doc.into_inner() {
        match pair.as_rule() {
            | Rule::comment => {
                tracing::trace!("ignoring comment");
            },
            | Rule::typename => {
                let mut pairs = pair.into_inner();
                let id = pairs.next().unwrap();
                let tref = pairs.next().unwrap();
                let name = match id.as_rule() {
                    | Rule::id => id.as_str().strip_prefix('$').unwrap(),
                    | _ => return Err(err!("expected typename id")),
                };
                let annotation_pairs = pairs.collect::<Vec<_>>();

                match tref.as_rule() {
                    | Rule::type_ref => {
                        let pair = tref.into_inner().next().unwrap();

                        match pair.as_rule() {
                            | Rule::id => {
                                let target_type_name = pair.as_str().strip_prefix('$').unwrap();

                                panic!("{target_type_name}");
                            },
                            | Rule::r#type => {
                                let wasi = preview1_wasi_type(&spec, pair)
                                    .wrap_err("failed to handle type pair")?;
                                let attributes = if !annotation_pairs.is_empty() {
                                    let mut attributes = BTreeMap::new();

                                    for pair in annotation_pairs {
                                        match pair.as_rule() {
                                            | Rule::annotation_expr => {
                                                let mut pairs = pair.into_inner();
                                                let annot = pairs.next().unwrap();

                                                if annot.as_str().strip_prefix('@').unwrap()
                                                    == "attribute"
                                                {
                                                    let name = pairs
                                                        .next()
                                                        .unwrap()
                                                        .as_str()
                                                        .strip_prefix('$')
                                                        .unwrap();
                                                    let tref = preview1_tref(
                                                        &spec,
                                                        pairs.next().unwrap(),
                                                    )?;

                                                    attributes.insert(name.to_owned(), tref);

                                                    continue;
                                                }

                                                panic!("not attribute annotation")
                                            },
                                            | _ => panic!("not annotation"),
                                        }
                                    }

                                    Some(attributes)
                                } else {
                                    None
                                };

                                spec.insert_type_def(ctx, name.to_string(), wasi, attributes);
                            },
                            | _ => unreachable!(),
                        }
                    },
                    | _ => return Err(err!("expected type ref")),
                }
            },
            | Rule::module => {
                let mut pairs = pair.into_inner();
                let id = pairs.next().unwrap();
                let name = match id.as_rule() {
                    | Rule::id => id.as_str().strip_prefix('$').unwrap(),
                    | _ => return Err(err!("expected typename id")),
                };
                let interface = preview1_module(&spec, pairs)?;

                spec.interfaces.push(name.to_string(), interface);
            },
            | Rule::EOI => (),
            | _ => panic!("{:?}", pair.as_rule()),
        }
    }

    Ok(spec)
}

fn preview1_module<'ctx>(
    spec: &Spec<'ctx>,
    pairs: Pairs<'_, Rule>,
) -> Result<Interface, eyre::Error> {
    let mut interface = Interface::new();

    for pair in pairs {
        match pair.as_rule() {
            | Rule::function => (),
            | Rule::comment => {
                tracing::trace!("Ignoring function comment.");
                continue;
            },
            | _ => panic!(),
        }

        let mut pairs = pair.into_inner();
        let function_name_pair = pairs.next().unwrap();
        let function_name = match function_name_pair.as_rule() {
            | Rule::function_name => function_name_pair.as_str().to_owned(),
            | _ => unreachable!(),
        };
        let mut params = Vec::new();
        let mut results = Vec::new();
        let mut r#return = None;
        let mut input_contract = None;
        let mut effects = Program { stmts: Vec::new() };

        for pair in pairs {
            match pair.as_rule() {
                | Rule::comment => (),
                | Rule::param => {
                    let mut pairs = pair.into_inner();
                    let name_pair = pairs.next().unwrap();
                    let name = match name_pair.as_rule() {
                        | Rule::id => name_pair.as_str().strip_prefix('$').unwrap().to_owned(),
                        | _ => unreachable!(),
                    };
                    let tref_pair = pairs.next().unwrap();
                    let tref =
                        preview1_tref(spec, tref_pair).wrap_err("failed to handle param tref")?;

                    params.push(FunctionParam { name, tref });
                },
                | Rule::result => {
                    let mut pairs = pair.into_inner();
                    let name_pair = pairs.next().unwrap();
                    let name = match name_pair.as_rule() {
                        | Rule::id => name_pair.as_str().to_owned(),
                        | _ => unreachable!(),
                    };
                    let tref_pair = pairs.next().unwrap();
                    let tref =
                        preview1_tref(spec, tref_pair).wrap_err("failed to handle param tref")?;

                    match &tref.resolve_wasi(spec) {
                        | WasiType::Variant(variant) => {
                            match (variant.cases.first(), variant.cases.get(1)) {
                                | (Some(c1), Some(c2))
                                    if c1.name == "expected" && c2.name == "error" =>
                                {
                                    r#return = Some(());

                                    if let Some(payload) = c1.payload.as_ref() {
                                        results.push(FunctionResult {
                                            name: "expected".to_owned(),
                                            tref: payload.to_owned(),
                                        });
                                    }
                                },
                                | _ => results.push(FunctionResult { name, tref }),
                            }
                        },
                        | _ => results.push(FunctionResult { name, tref }),
                    }
                },
                | Rule::annotation_expr => {
                    let mut pairs = pair.into_inner();
                    let annot_pair = pairs.next().unwrap();

                    match annot_pair.as_rule() {
                        | Rule::annotation if annot_pair.as_str() == "@input" => {
                            let pair = pairs.next().unwrap();
                            let pair = ilang::Parser::parse(ilang::Rule::term_final, pair.as_str())
                                .wrap_err("Failed to parse slang")?
                                .next()
                                .unwrap();
                            let term = ilang::to_term(pair)?;

                            input_contract = Some(term);
                        },
                        | Rule::annotation if annot_pair.as_str() == "@effects" => {
                            for pair in pairs {
                                let pair = olang::Parser::parse(olang::Rule::stmt, pair.as_str())
                                    .wrap_err("failed to parse elang")?
                                    .next()
                                    .unwrap();

                                effects.stmts.push(olang::to_stmt(spec, pair)?);
                            }
                        },
                        | _ => panic!("{:?}", annot_pair),
                    }
                },
                | _ => unreachable!(),
            }
        }

        interface.functions.insert(
            function_name.clone(),
            Function {
                name: function_name,
                params,
                results,
                r#return,
                input_contract,
                effects,
            },
        );
    }

    Ok(interface)
}

fn preview1_wasi_type<'ctx>(
    spec: &Spec<'ctx>,
    pair: Pair<'_, Rule>,
) -> Result<WasiType, eyre::Error> {
    let pair = pair.into_inner().next().unwrap();

    Ok(match pair.as_rule() {
        | Rule::s64 => WasiType::S64,
        | Rule::r#u8 => WasiType::U8,
        | Rule::r#u16 => WasiType::U16,
        | Rule::r#u32 => WasiType::U32,
        | Rule::r#u64 => WasiType::U64,
        | Rule::handle => WasiType::Handle,
        | Rule::flags => {
            let mut fields = Vec::new();
            let mut pairs = pair.into_inner();
            let int_repr_pair = pairs.next().unwrap();
            let int_repr_pair = match int_repr_pair.as_rule() {
                | Rule::int_repr => int_repr_pair.into_inner().next().unwrap(),
                | _ => unreachable!(),
            };

            for case_pair in pairs {
                let name = match case_pair.as_rule() {
                    | Rule::id => case_pair.as_str().strip_prefix('$').unwrap().to_owned(),
                    | Rule::comment => {
                        tracing::trace!("Ignoring enum variant comment.");
                        continue;
                    },
                    | _ => return Err(err!("unexpected field {:?}", case_pair)),
                };

                fields.push(name);
            }

            WasiType::Flags(FlagsType {
                repr: preview1_int_repr(int_repr_pair)?,
                fields,
            })
        },
        | Rule::r#enum => {
            let mut cases = Vec::new();
            let mut pairs = pair.into_inner();
            let int_repr_pair = pairs.next().unwrap();
            let int_repr_pair = match int_repr_pair.as_rule() {
                | Rule::int_repr => int_repr_pair.into_inner().next().unwrap(),
                | _ => unreachable!(),
            };

            for case_pair in pairs {
                let name = match case_pair.as_rule() {
                    | Rule::id => case_pair.as_str().strip_prefix('$').unwrap().to_owned(),
                    | Rule::comment => {
                        tracing::trace!("Ignoring enum variant comment.");
                        continue;
                    },
                    | _ => return Err(err!("unexpected field {:?}", case_pair)),
                };

                cases.push(VariantCaseType {
                    name,
                    payload: None,
                });
            }

            WasiType::Variant(VariantType {
                tag_repr: preview1_int_repr(int_repr_pair)?,
                cases,
            })
        },
        | Rule::union => {
            let mut cases = Vec::new();
            let mut pairs = pair.into_inner();
            let tag_pair = pairs.next().unwrap();
            let tag_name = match tag_pair.as_rule() {
                | Rule::id => tag_pair.as_str().strip_prefix('$').unwrap(),
                | _ => unreachable!(),
            };
            let tag_type = spec
                .types
                .get_by_key(tag_name)
                .wrap_err("unknown tag type")?;
            let tag = match &tag_type.wasi {
                | WasiType::Variant(variant) => variant,
                | _ => panic!(),
            };

            for (case_pair, case_name_type) in pairs.zip(tag.cases.iter()) {
                let case_type_name = match case_pair.as_rule() {
                    | Rule::id => case_pair.as_str().strip_prefix('$').unwrap(),
                    | Rule::comment => {
                        tracing::trace!("Ignoring enum variant comment.");
                        continue;
                    },
                    | _ => return Err(err!("unexpected field {:?}", case_pair)),
                };

                cases.push(VariantCaseType {
                    name:    case_name_type.name.clone(),
                    payload: Some(TypeRef::Named(case_type_name.to_string())),
                });
            }

            WasiType::Variant(VariantType {
                tag_repr: tag.tag_repr,
                cases,
            })
        },
        | Rule::expected => {
            let mut pairs = pair.into_inner().collect::<Vec<_>>();
            let mut expected_pair = None;
            let error_pair;

            if pairs.len() >= 2 {
                error_pair = pairs.remove(1);
                expected_pair = Some(pairs.remove(0));
            } else {
                error_pair = pairs.remove(0);
            }

            WasiType::Variant(VariantType {
                tag_repr: IntRepr::U8,
                cases:    vec![
                    VariantCaseType {
                        name:    "expected".to_owned(),
                        payload: expected_pair.map(|p| preview1_tref(spec, p)).transpose()?,
                    },
                    VariantCaseType {
                        name:    "error".to_owned(),
                        payload: Some(preview1_tref(spec, error_pair)?),
                    },
                ],
            })
        },
        | Rule::record => {
            let mut members = Vec::new();
            let pairs = pair.into_inner();

            for member_pair in pairs {
                let mut pairs = match member_pair.as_rule() {
                    | Rule::record_field => member_pair.into_inner(),
                    | Rule::comment => {
                        tracing::trace!("Ignoring record member comment.");
                        continue;
                    },
                    | _ => return Err(err!("unexpected field {:?}", member_pair)),
                };
                let name_pair = pairs.next().unwrap();
                let name = match name_pair.as_rule() {
                    | Rule::id => name_pair.as_str().strip_prefix('$').unwrap().to_owned(),
                    | _ => unreachable!(),
                };
                let tref_pair = pairs.next().unwrap();
                let tref_pair = match tref_pair.as_rule() {
                    | Rule::type_ref => tref_pair.into_inner().next().unwrap(),
                    | _ => unreachable!(),
                };
                let tref = preview1_tref(spec, tref_pair)?;

                members.push(RecordMemberType { name, tref });
            }

            WasiType::Record(RecordType { members })
        },
        | Rule::string => WasiType::String,
        | Rule::list => {
            let tref_pair = pair.into_inner().next().unwrap();
            let item = preview1_tref(spec, tref_pair).wrap_err("failed to handle list type ref")?;

            WasiType::List(Box::new(ListType { item }))
        },
        | t => return Err(err!("unexpected type {:?}", t)),
    })
}

fn preview1_tref<'ctx>(spec: &Spec<'ctx>, pair: Pair<'_, Rule>) -> Result<TypeRef, eyre::Error> {
    match pair.as_rule() {
        | Rule::id => {
            let id = pair.as_str().strip_prefix('$').unwrap();
            let _tdef = spec
                .types
                .get_by_key(id)
                .wrap_err("type ref to non-existent type")?;

            Ok(TypeRef::Named(id.to_string()))
        },
        | Rule::type_ref => preview1_tref(spec, pair.into_inner().next().unwrap()),
        | Rule::r#type => Ok(TypeRef::Anonymous(preview1_wasi_type(spec, pair)?)),
        | _ => unreachable!(
            "{:?} {:?} -> {}",
            pair.as_span().lines().collect::<Vec<_>>(),
            pair.as_rule(),
            pair.as_str()
        ),
    }
}

fn preview1_int_repr(pair: Pair<'_, Rule>) -> Result<IntRepr, eyre::Error> {
    Ok(match pair.as_rule() {
        | Rule::r#u8 => IntRepr::U8,
        | Rule::r#u16 => IntRepr::U16,
        | Rule::r#u32 => IntRepr::U32,
        | Rule::r#u64 => IntRepr::U64,
        | _ => return Err(err!("unknown int repr {:?}", pair)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok() {
        let cfg = z3::Config::new();
        let ctx = z3::Context::new(&cfg);
        let _spec = preview1(&ctx).unwrap();
    }
}
