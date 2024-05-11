use nom_supreme::{
    error::{ErrorTree, GenericErrorTree},
    final_parser::{final_parser, Location, RecreateContext},
};
use wazzi_dyn_spec::{environment::ResourceType, wasi, Environment, Term};
use wazzi_spec::parsers::wazzi_preview1;

#[test]
fn annotation_to_term() {
    let input_contract = r#"
        (@input-contract
          (@or
            (@and
              (@value.eq (param $whence) (enum $whence $cur))
              (i64.le_s
                (i64.add (param $offset) (@attr.get (param $fd) $offset))
                (i64.const 17592186040320)
              )
              (i64.ge_s
                (i64.add (param $offset) (@attr.get (param $fd) $offset))
                (i64.const 0)
              )
            )
          )
        )
    "#;
    let annot: Result<wazzi_preview1::Annotation, ErrorTree<&str>> =
        final_parser(wazzi_preview1::ws(wazzi_preview1::Annotation::parse))(input_contract);
    let annot = match annot {
        | Ok(annot) => annot,
        | Err(GenericErrorTree::Stack { base: _, contexts }) => {
            for context in contexts {
                eprintln!(
                    "{} {}",
                    Location::recreate_context(input_contract, context.0),
                    context.1
                )
            }
            panic!();
        },
        | Err(_) => panic!(),
    };
    let mut env = Environment::new();

    env.resource_types_mut().push(
        Some("whence".to_owned()),
        ResourceType {
            wasi_type:  wasi::Type::Variant(wasi::VariantType {
                cases: vec![
                    wasi::CaseType {
                        name:    "set".to_owned(),
                        payload: None,
                    },
                    wasi::CaseType {
                        name:    "cur".to_owned(),
                        payload: None,
                    },
                    wasi::CaseType {
                        name:    "end".to_owned(),
                        payload: None,
                    },
                ],
            }),
            attributes: Default::default(),
            fungible:   true,
        },
    );

    Term::from_preview1_annotation(&env, annot);
}

const PREVIEW1_SPEC: &str = include_str!("../../spec/preview1.dyn-constraint.witx");

#[test]
fn ok() {
    let mut env = Environment::new();
    let document = match wazzi_preview1::Document::parse(PREVIEW1_SPEC) {
        | Ok(doc) => doc,
        | Err(GenericErrorTree::Stack { base: _, contexts }) => {
            for context in contexts {
                eprintln!(
                    "{} {}",
                    Location::recreate_context(PREVIEW1_SPEC, context.0),
                    context.1
                )
            }
            panic!();
        },
        | Err(GenericErrorTree::Base { location, kind: _ }) => {
            eprintln!("{location}");
            panic!();
        },
        | Err(GenericErrorTree::Alt(alt)) => {
            for e in alt {
                eprintln!("{:?}", e);
            }

            panic!();
        },
    };
    let module = document
        .modules
        .iter()
        .find(|module| module.id.as_ref().unwrap().name() == "wasi_snapshot_preview1")
        .unwrap();

    env.ingest_preview1_spec(module.to_owned());
}
