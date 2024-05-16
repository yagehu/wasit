use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use arbitrary::Unstructured;
use nom_supreme::{
    error::{ErrorTree, GenericErrorTree},
    final_parser::{final_parser, Location, RecreateContext},
};
use tempfile::tempdir;
use wazzi_dyn_spec::{
    ast::Idx,
    environment::ResourceType,
    wasi,
    Environment,
    ResourceContext,
    Term,
};
use wazzi_executor::{ExecutorRunner, RunningExecutor};
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
    let mut ctx = ResourceContext::new();
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

    let mut u = Unstructured::new(&[]);
    let wasmtime = wazzi_runners::Wasmtime::new(Path::new("wasmtime"));
    let tmpdir = tempdir().unwrap();
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let executor = ExecutorRunner::new(
        &wasmtime,
        PathBuf::from("../target/debug/wazzi-executor-pb.wasm")
            .canonicalize()
            .unwrap(),
        tmpdir.path().to_path_buf(),
        Some(tmpdir.path().to_path_buf()),
    )
    .run(stderr)
    .unwrap();
    let solution = env.call(&mut u, &ctx, "path_open").unwrap();
    let function = env
        .functions_mut()
        .get(&Idx::Symbolic("path_open".to_owned()))
        .unwrap()
        .clone();

    executor.call(wazzi_executor_pb_rust::request::Call {
        func:           <&str as TryInto<wazzi_executor_pb_rust::WasiFunc>>::try_into("path_open")
            .unwrap()
            .into(),
        params:         solution
            .params
            .into_iter()
            .zip(function.params.iter())
            .map(|(p, func_param)| {
                let resource_type = env
                    .resource_types_mut()
                    .get(&Idx::Numeric(func_param.resource_type_idx))
                    .unwrap();

                p.inner.into_pb(&ctx, &resource_type.wasi_type)
            })
            .collect(),
        results:        function
            .results
            .iter()
            .map(|r| {
                let ty = env
                    .resource_types_mut()
                    .get(&Idx::Numeric(r.resource_type_idx))
                    .unwrap();

                wasi::Value::arbitrary(&ty.wasi_type, &mut u)
                    .unwrap()
                    .into_pb(&ty.wasi_type)
            })
            .collect(),
        special_fields: Default::default(),
    });
}
