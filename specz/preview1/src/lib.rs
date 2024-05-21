use std::collections::{BTreeMap, HashMap};

use eyre::{eyre as err, Context};
use pest::{
    iterators::{Pair, Pairs},
    Parser,
};
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "witx.pest"]
pub struct WitxParser;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Spec {
    types:      HashMap<String, WazziType>,
    interfaces: HashMap<String, Interface>,
}

impl Spec {
    pub fn new() -> Self {
        Self {
            types:      Default::default(),
            interfaces: Default::default(),
        }
    }

    pub fn preview1() -> Result<Self, eyre::Error> {
        const DOC: &str = include_str!("../main.witx");

        let mut spec = Self::new();
        let doc = WitxParser::parse(Rule::document, DOC)
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

                    match tref.as_rule() {
                        | Rule::type_ref => {
                            let pair = tref.into_inner().next().unwrap();

                            match pair.as_rule() {
                                | Rule::id => {
                                    let target_type_name = pair.as_str().strip_prefix('$').unwrap();

                                    panic!("{target_type_name}");
                                },
                                | Rule::r#type => {
                                    if spec.types.get(name).is_some() {
                                        return Err(err!("typename {name} already defined"));
                                    }

                                    spec.types.insert(
                                        name.to_owned(),
                                        spec.preview1_type(pair)
                                            .wrap_err("failed to handle type pair")?,
                                    );
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

                    spec.interfaces
                        .insert(name.to_owned(), spec.preview1_module(pairs)?);

                    eprintln!("{:#?}", spec);
                },
                | Rule::EOI => (),
                | _ => panic!("{:?}", pair.as_rule()),
            }
        }

        Ok(spec)
    }

    fn preview1_module(&self, pairs: Pairs<'_, Rule>) -> Result<Interface, eyre::Error> {
        let mut interface = Interface::new();

        for pair in pairs {
            match pair.as_rule() {
                | Rule::function => (),
                | Rule::comment => {
                    tracing::trace!("Ignoring function comment.");
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

            for pair in pairs {
                match pair.as_rule() {
                    | Rule::param => {
                        let mut pairs = pair.into_inner();
                        let name_pair = pairs.next().unwrap();
                        let name = match name_pair.as_rule() {
                            | Rule::id => name_pair.as_str().strip_prefix('$').unwrap().to_owned(),
                            | _ => unreachable!(),
                        };
                        let tref_pair = pairs.next().unwrap();
                        let ty = self
                            .preview1_tref(tref_pair)
                            .wrap_err("failed to handle param tref")?;

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
                        let ty = self
                            .preview1_tref(tref_pair)
                            .wrap_err("failed to handle param tref")?;

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
                    | Rule::annotation_expr => {},
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
                },
            );
        }

        Ok(interface)
    }

    fn preview1_type(&self, pair: Pair<'_, Rule>) -> Result<WazziType, eyre::Error> {
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
                    repr: IntRepr::try_from(int_repr_pair)?,
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
                    tag_repr: IntRepr::try_from(int_repr_pair)?,
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
                let tag_type = match self.types.get(tag_name) {
                    | Some(ty) => ty,
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
                    let payload = match self.types.get(case_type_name) {
                        | Some(t) => t.clone(),
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
                let mut expected_pair = pairs.next().unwrap();
                let mut error_pair = pairs.next().unwrap();

                WasiType::Variant(VariantType {
                    tag_repr: IntRepr::U8,
                    cases:    vec![
                        VariantCaseType {
                            name:    "expected".to_owned(),
                            payload: Some(self.preview1_tref(expected_pair)?),
                        },
                        VariantCaseType {
                            name:    "error".to_owned(),
                            payload: Some(self.preview1_tref(error_pair)?),
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

                            match self.types.get(id) {
                                | Some(t) => t.clone(),
                                | None => return Err(err!("unknown type ref {id}")),
                            }
                        },
                        | Rule::r#type => self.preview1_type(tref_pair)?,
                        | _ => unreachable!(),
                    };

                    members.push(RecordMemberType { name, ty });
                }

                WasiType::Record(RecordType { members })
            },
            | Rule::string => WasiType::String,
            | Rule::list => {
                let tref_pair = pair.into_inner().next().unwrap();
                let item = self
                    .preview1_tref(tref_pair)
                    .wrap_err("failed to handle list type ref")?;

                WasiType::List(Box::new(ListType { item }))
            },
            | t => return Err(err!("unexpected type {:?}", t)),
        };

        Ok(WazziType { wasi })
    }

    fn preview1_tref(&self, pair: Pair<'_, Rule>) -> Result<WazziType, eyre::Error> {
        let pair = match pair.as_rule() {
            | Rule::type_ref => pair.into_inner().next().unwrap(),
            | _ => unreachable!(),
        };

        match pair.as_rule() {
            | Rule::id => {
                let id = pair.as_str().strip_prefix('$').unwrap();

                match self.types.get(id) {
                    | Some(t) => Ok(t.clone()),
                    | None => Err(err!("unknown type ref {id}")),
                }
            },
            | Rule::r#type => self.preview1_type(pair),
            | _ => unreachable!(),
        }
    }
}

impl Default for Spec {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct WazziType {
    wasi: WasiType,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum WasiType {
    S64,
    U8,
    U16,
    U32,
    U64,
    Handle,
    Flags(FlagsType),
    Variant(VariantType),
    Record(RecordType),
    String,
    List(Box<ListType>),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FlagsType {
    pub repr:   IntRepr,
    pub fields: Vec<String>,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum IntRepr {
    U8,
    U16,
    U32,
    U64,
}

impl TryFrom<Pair<'_, Rule>> for IntRepr {
    type Error = eyre::Error;

    fn try_from(pair: Pair<'_, Rule>) -> Result<Self, Self::Error> {
        Ok(match pair.as_rule() {
            | Rule::r#u8 => Self::U8,
            | Rule::r#u16 => Self::U16,
            | Rule::r#u32 => Self::U32,
            | Rule::r#u64 => Self::U64,
            | _ => return Err(err!("unknown int repr {:?}", pair)),
        })
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct VariantType {
    pub tag_repr: IntRepr,
    pub cases:    Vec<VariantCaseType>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct VariantCaseType {
    pub name:    String,
    pub payload: Option<WazziType>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RecordType {
    pub members: Vec<RecordMemberType>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RecordMemberType {
    pub name: String,
    pub ty:   WazziType,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ListType {
    pub item: WazziType,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Interface {
    pub functions: BTreeMap<String, Function>,
}

impl Interface {
    pub fn new() -> Self {
        Self {
            functions: Default::default(),
        }
    }
}

impl Default for Interface {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Function {
    pub name:     String,
    pub params:   Vec<FunctionParam>,
    pub results:  Vec<FunctionResult>,
    pub r#return: Option<()>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FunctionParam {
    pub name: String,
    pub ty:   WazziType,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FunctionResult {
    pub name: String,
    pub ty:   WazziType,
}

#[cfg(test)]
mod tests {
    use std::io;

    use tracing::level_filters::LevelFilter;
    use tracing_error::ErrorLayer;
    use tracing_subscriber::{layer::SubscriberExt as _, EnvFilter};

    use super::*;

    #[test]
    fn ok() -> Result<(), eyre::Error> {
        color_eyre::install()?;
        tracing::subscriber::set_global_default(
            tracing_subscriber::Registry::default()
                .with(
                    EnvFilter::builder()
                        .with_env_var("WAZZI_LOG_LEVEL")
                        .with_default_directive(LevelFilter::INFO.into())
                        .from_env_lossy(),
                )
                .with(ErrorLayer::default())
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_thread_names(true)
                        .with_writer(io::stderr)
                        .pretty(),
                ),
        )
        .wrap_err("failed to configure tracing")?;

        let _spec = Spec::preview1().unwrap();

        Ok(())
    }
}
