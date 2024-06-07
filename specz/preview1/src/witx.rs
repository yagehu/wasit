pub mod elang;
pub mod slang;

use eyre::{eyre as err, Context as _};
use pest::{
    iterators::{Pair, Pairs},
    Parser as _,
};
use pest_derive::Parser;
use std::collections::HashMap;

use wazzi_specz_wasi::{
    effects,
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
    VariantCaseType,
    VariantType,
    WasiType,
    WazziType,
};

#[derive(Parser)]
#[grammar = "witx.pest"]
pub struct Parser;

pub fn preview1(spec: &mut Spec) -> Result<(), eyre::Error> {
    const DOC: &str = include_str!("../main.witx");

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
                                if spec.types_map.get(name).is_some() {
                                    return Err(err!("typename {name} already defined"));
                                }

                                let ty = preview1_type(
                                    spec,
                                    pair,
                                    Some(name.to_owned()),
                                    annotation_pairs,
                                )
                                .wrap_err("failed to handle type pair")?;

                                spec.types.push(ty);
                                spec.types_map.insert(name.to_owned(), spec.types.len() - 1);
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
                let ty = preview1_module(spec, pairs)?;

                spec.interfaces.push(ty);
                spec.interfaces_map
                    .insert(name.to_owned(), spec.interfaces.len() - 1);
            },
            | Rule::EOI => (),
            | _ => panic!("{:?}", pair.as_rule()),
        }
    }

    Ok(())
}

fn preview1_module(spec: &mut Spec, pairs: Pairs<'_, Rule>) -> Result<Interface, eyre::Error> {
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
        let mut effects = effects::Program { stmts: Vec::new() };

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
                    let ty =
                        preview1_tref(spec, tref_pair).wrap_err("failed to handle param tref")?;

                    params.push(FunctionParam { name, ty });
                },
                | Rule::result => {
                    let mut pairs = pair.into_inner();
                    let name_pair = pairs.next().unwrap();
                    let name = match name_pair.as_rule() {
                        | Rule::id => name_pair.as_str().to_owned(),
                        | _ => unreachable!(),
                    };
                    let tref_pair = pairs.next().unwrap();
                    let ty =
                        preview1_tref(spec, tref_pair).wrap_err("failed to handle param tref")?;

                    match &ty.wasi {
                        | WasiType::Variant(variant) => {
                            match (variant.cases.first(), variant.cases.get(1)) {
                                | (Some(c1), Some(c2))
                                    if c1.name == "expected" && c2.name == "error" =>
                                {
                                    r#return = Some(());

                                    results.push(FunctionResult {
                                        name: "expected".to_owned(),
                                        ty:   c1.payload.as_ref().unwrap().clone(),
                                    });
                                },
                                | _ => results.push(FunctionResult { name, ty }),
                            }
                        },
                        | _ => results.push(FunctionResult { name, ty }),
                    }
                },
                | Rule::annotation_expr => {
                    let mut pairs = pair.into_inner();
                    let annot_pair = pairs.next().unwrap();

                    match annot_pair.as_rule() {
                        | Rule::annotation if annot_pair.as_str() == "@input" => {
                            let pair = pairs.next().unwrap();
                            let pair = slang::Parser::parse(slang::Rule::term_final, pair.as_str())
                                .wrap_err("Failed to parse slang")?
                                .next()
                                .unwrap();
                            let term = slang::to_term(pair)?;

                            input_contract = Some(term);
                        },
                        | Rule::annotation if annot_pair.as_str() == "@effects" => {
                            for pair in pairs {
                                let pair = elang::Parser::parse(elang::Rule::stmt, pair.as_str())
                                    .wrap_err("failed to parse elang")?
                                    .next()
                                    .unwrap();

                                effects.stmts.push(elang::to_stmt(spec, pair)?);
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

fn preview1_type(
    spec: &mut Spec,
    pair: Pair<'_, Rule>,
    name: Option<String>,
    annotations: Vec<Pair<'_, Rule>>,
) -> Result<WazziType, eyre::Error> {
    let pair = pair.into_inner().next().unwrap();
    let wasi = match pair.as_rule() {
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
            let tag_type = match spec.types_map.get(tag_name) {
                | Some(&i) => spec.types.get(i).unwrap(),
                | None => return Err(err!("unknown tag type {tag_name}")),
            };
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
                let payload = match spec.types_map.get(case_type_name) {
                    | Some(&i) => spec.types.get(i).unwrap().clone(),
                    | None => return Err(err!("unknown type id {case_type_name}")),
                };

                cases.push(VariantCaseType {
                    name:    case_name_type.name.clone(),
                    payload: Some(payload),
                });
            }

            WasiType::Variant(VariantType {
                tag_repr: tag.tag_repr,
                cases,
            })
        },
        | Rule::expected => {
            let mut pairs = pair.into_inner();
            let expected_pair = pairs.next().unwrap();
            let error_pair = pairs.next().unwrap();

            WasiType::Variant(VariantType {
                tag_repr: IntRepr::U8,
                cases:    vec![
                    VariantCaseType {
                        name:    "expected".to_owned(),
                        payload: Some(preview1_tref(spec, expected_pair)?),
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
                let ty = match tref_pair.as_rule() {
                    | Rule::id => {
                        let id = tref_pair.as_str().strip_prefix('$').unwrap();

                        match spec.types_map.get(id) {
                            | Some(&i) => spec.types.get(i).unwrap().clone(),
                            | None => return Err(err!("unknown type ref {id}")),
                        }
                    },
                    | Rule::r#type => preview1_type(spec, tref_pair, None, vec![])?,
                    | _ => unreachable!(),
                };

                members.push(RecordMemberType { name, ty });
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
    };
    let mut attributes = Vec::new();

    for pair in annotations {
        match pair.as_rule() {
            | Rule::annotation_expr => {
                let mut pairs = pair.into_inner();
                let annot = pairs.next().unwrap();

                if annot.as_str().strip_prefix('@').unwrap() == "attribute" {
                    let name = pairs.next().unwrap().as_str().strip_prefix('$').unwrap();
                    let type_name = pairs.next().unwrap().as_str().strip_prefix('$').unwrap();

                    attributes.push((
                        name.to_owned(),
                        spec.types
                            .get(
                                *spec
                                    .types_map
                                    .get(type_name)
                                    .expect(&format!("{type_name}")),
                            )
                            .unwrap()
                            .clone(),
                    ));

                    continue;
                }

                panic!("not attribute annotation")
            },
            | _ => panic!("not annotation"),
        }
    }

    Ok(WazziType {
        name,
        wasi,
        attributes,
    })
}

fn preview1_tref(spec: &mut Spec, pair: Pair<'_, Rule>) -> Result<WazziType, eyre::Error> {
    let pair = match pair.as_rule() {
        | Rule::type_ref => pair.into_inner().next().unwrap(),
        | _ => unreachable!(),
    };

    match pair.as_rule() {
        | Rule::id => {
            let id = pair.as_str().strip_prefix('$').unwrap();

            match spec.types_map.get(id) {
                | Some(i) => Ok(spec.types.get(*i).cloned().unwrap()),
                | None => Err(err!("unknown type ref {id}")),
            }
        },
        | Rule::r#type => preview1_type(spec, pair, None, vec![]),
        | _ => unreachable!("{:?}", pair),
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
