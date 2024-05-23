pub mod resource;

use std::{
    collections::{BTreeSet, HashMap},
    ops::Not,
};

use eyre::{eyre as err, Context as _};
use wazzi_executor::RunningExecutor;
use wazzi_specz_wasi::{Function, Spec, Term, VariantValue, WasiType, WasiValue};
use z3::ast::Ast;

use self::resource::Context;

#[derive(PartialEq, Eq, Clone, Debug)]
struct Resource {
    attributes: HashMap<String, WasiValue>,
}

#[derive(Debug)]
pub struct Environment {
    spec:               Spec,
    z3_ctx:             z3::Context,
    resources:          Vec<Resource>,
    resources_by_types: HashMap<String, BTreeSet<usize>>,
}

impl Environment {
    pub fn preview1() -> Result<Self, eyre::Error> {
        let mut spec = Spec::new();

        wazzi_specz_preview1::witx::preview1(&mut spec)?;

        Ok(Environment {
            spec,
            z3_ctx: z3::Context::new(&z3::Config::new()),
            resources: Default::default(),
            resources_by_types: Default::default(),
        })
    }

    pub fn spec(&self) -> &Spec {
        &self.spec
    }

    fn iter_resource_ids_by_type(
        &self,
    ) -> impl Iterator<Item = (&str, impl Iterator<Item = usize> + '_)> + '_ {
        self.resources_by_types
            .iter()
            .map(|(name, pool)| (name.as_str(), pool.iter().cloned()))
    }

    pub fn new_resource(&mut self, type_name: String, resource: Resource) -> usize {
        self.resources.push(resource);
        self.resources_by_types
            .entry(type_name)
            .or_default()
            .insert(self.resources.len() - 1);
        self.resources.len() - 1
    }

    pub fn call(
        &self,
        ctx: &mut Context,
        executor: &RunningExecutor,
        function_name: &str,
    ) -> Result<(), eyre::Error> {
        let interface = self.spec.interfaces.get("wasi_snapshot_preview1").unwrap();
        let function = interface.functions.get(function_name).unwrap();
        let solver = z3::Solver::new(&self.z3_ctx);
        let types = self.spec.types.keys().collect::<Vec<_>>();
        let resource_sort = z3::Sort::uninterpreted(&self.z3_ctx, "resource".into());
        let mut datatypes = HashMap::new();

        for (name, ty) in self.spec.types.iter() {
            match &ty.wasi {
                | WasiType::Handle => {
                    datatypes.insert(
                        name.clone(),
                        z3::DatatypeBuilder::new(&self.z3_ctx, name.as_str())
                            .variant(
                                name,
                                vec![(
                                    "value",
                                    z3::DatatypeAccessor::Sort(z3::Sort::int(&self.z3_ctx)),
                                )],
                            )
                            .finish(),
                    );
                },
                | WasiType::Flags(flags) => {
                    datatypes.insert(
                        name.clone(),
                        z3::DatatypeBuilder::new(&self.z3_ctx, name.as_str())
                            .variant(
                                name,
                                flags
                                    .fields
                                    .iter()
                                    .map(|f| {
                                        (
                                            f.as_str(),
                                            z3::DatatypeAccessor::Sort(z3::Sort::bool(
                                                &self.z3_ctx,
                                            )),
                                        )
                                    })
                                    .collect::<Vec<_>>(),
                            )
                            .finish(),
                    );
                },
                | WasiType::String => {
                    datatypes.insert(
                        name.clone(),
                        z3::DatatypeBuilder::new(&self.z3_ctx, name.as_str())
                            .variant(
                                name,
                                vec![(
                                    "value",
                                    z3::DatatypeAccessor::Sort(z3::Sort::string(&self.z3_ctx)),
                                )],
                            )
                            .finish(),
                    );
                },
                | _ => {
                    tracing::warn!("Ignoring type {}", name);
                },
            }
        }

        for (name, pool) in self.iter_resource_ids_by_type() {
            let ty = self.spec.types.get(name).unwrap();
            let datatype = datatypes.get(name).unwrap();
            let pool = pool.collect::<Vec<_>>();

            for resource_id in pool {
                let resource_const = z3::FuncDecl::new(
                    &self.z3_ctx,
                    format!("resource--{name}--{}", resource_id),
                    &[],
                    &datatype.sort,
                )
                .apply(&[]);
                let resource_value = ctx.resources.get(&resource_id).unwrap();

                match (&ty.wasi, resource_value) {
                    | (WasiType::Handle, &WasiValue::Handle(handle)) => {
                        let datatype_variant = datatype.variants.first().unwrap();
                        let accessor = datatype_variant.accessors.first().unwrap();

                        solver.assert(&accessor.apply(&[&resource_const])._eq(
                            &z3::ast::Dynamic::from_ast(&z3::ast::Int::from_u64(
                                &self.z3_ctx,
                                handle.into(),
                            )),
                        ));
                    },
                    | (WasiType::Flags(ty), WasiValue::Flags(flags)) => {
                        let datatype_variant = datatype.variants.first().unwrap();

                        for (accessor, &field) in
                            datatype_variant.accessors.iter().zip(flags.fields.iter())
                        {
                            solver.assert(&accessor.apply(&[&resource_const])._eq(
                                &z3::ast::Dynamic::from_ast(&z3::ast::Bool::from_bool(
                                    &self.z3_ctx,
                                    field,
                                )),
                            ));
                        }
                    },
                    | (WasiType::Flags(_), _) => unreachable!(),
                    | _ => tracing::warn!("abc"),
                }
            }
        }

        let mut vars = HashMap::new();

        for param in function.params.iter() {
            let type_name = param.ty.name.as_ref().unwrap();
            let datatype = datatypes.get(type_name).unwrap();
            let datatype_variant = datatype.variants.first().unwrap();

            match &param.ty.wasi {
                | WasiType::S64 => todo!(),
                | WasiType::U8 => todo!(),
                | WasiType::U16 => todo!(),
                | WasiType::U32 => todo!(),
                | WasiType::U64 => todo!(),
                | WasiType::Handle => {
                    let x = z3::ast::Datatype::new_const(
                        &self.z3_ctx,
                        format!("var--{}", param.name),
                        // TODO: resources should use attributes, not raw value
                        &datatype.sort,
                    );
                    let resource_ids = self.resources_by_types.get(type_name).unwrap();
                    let clauses = resource_ids
                        .iter()
                        .map(|&id| {
                            let resource_const = z3::FuncDecl::new(
                                &self.z3_ctx,
                                format!("resource--{}--{}", param.ty.name.as_ref().unwrap(), id),
                                &[],
                                &datatype.sort,
                            )
                            .apply(&[]);

                            z3::ast::Dynamic::from_ast(&x)._eq(&resource_const)
                        })
                        .collect::<Vec<_>>();

                    solver.assert(&z3::ast::Bool::or(
                        &self.z3_ctx,
                        clauses.iter().collect::<Vec<_>>().as_slice(),
                    ));
                    vars.insert(param.name.clone(), x);
                },
                | WasiType::Flags(flags) => {
                    let x = z3::ast::Datatype::new_const(
                        &self.z3_ctx,
                        format!("var--{}", param.name),
                        &datatype.sort,
                    );

                    vars.insert(param.name.clone(), x);
                },
                | WasiType::Variant(_) => todo!(),
                | WasiType::Record(_) => todo!(),
                | WasiType::String => (),
                | WasiType::List(_) => todo!(),
            }
        }

        let input_contract = function.input_contract.as_ref().unwrap();
        let ast_node = self.term_to_constraint(function, &datatypes, &vars, input_contract)?;

        solver.assert(
            &ast_node._eq(&z3::ast::Dynamic::from_ast(&z3::ast::Bool::from_bool(
                &self.z3_ctx,
                true,
            ))),
        );

        if solver.check() != z3::SatResult::Sat {
            return Err(err!("no solution to function input contract"));
        }

        for _i in 0..10 {
            if solver.check() != z3::SatResult::Sat {
                break;
            }

            let model = solver.get_model().unwrap();

            eprintln!("model {:#?}", model);

            let mut clauses = Vec::new();

            for (name, var) in &vars {
                if let Some(v) = model.get_const_interp(var) {
                    if name == "fd" {
                        continue;
                    }

                    clauses.push(var._eq(&v).not());
                }
            }

            solver.assert(&z3::ast::Bool::or(
                &self.z3_ctx,
                &clauses.iter().collect::<Vec<_>>(),
            ));
        }

        panic!();

        Ok(())
    }

    fn term_to_constraint<'a>(
        &'a self,
        function: &Function,
        datatypes: &HashMap<String, z3::DatatypeSort<'a>>,
        vars: &HashMap<String, z3::ast::Datatype<'a>>,
        term: &Term,
    ) -> Result<z3::ast::Dynamic, eyre::Error> {
        Ok(match term {
            | Term::Not(t) => z3::ast::Dynamic::from_ast(
                &self
                    .term_to_constraint(function, datatypes, vars, &t.term)?
                    .as_bool()
                    .unwrap()
                    .not(),
            ),
            | Term::And(t) => {
                let clauses = t
                    .clauses
                    .iter()
                    .map(|clause| self.term_to_constraint(function, datatypes, vars, clause))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|clause| clause.as_bool().unwrap())
                    .collect::<Vec<_>>();

                z3::ast::Dynamic::from_ast(&z3::ast::Bool::and(
                    &self.z3_ctx,
                    clauses.iter().collect::<Vec<_>>().as_slice(),
                ))
            },
            | Term::Or(t) => {
                let clauses = t
                    .clauses
                    .iter()
                    .map(|clause| self.term_to_constraint(function, datatypes, vars, clause))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|clause| clause.as_bool().unwrap())
                    .collect::<Vec<_>>();

                z3::ast::Dynamic::from_ast(&z3::ast::Bool::or(
                    &self.z3_ctx,
                    clauses.iter().collect::<Vec<_>>().as_slice(),
                ))
            },
            | Term::Param(t) => {
                let var = vars.get(&t.name).expect(&t.name);

                z3::ast::Dynamic::from_ast(var)
            },
            | Term::FlagsGet(t) => {
                let target = self
                    .term_to_constraint(function, datatypes, vars, &t.target)
                    .wrap_err("failed to translate flags get target to z3")?;
                let var = target.as_datatype().unwrap();
                let datatype = datatypes.get(&t.r#type).unwrap();
                let variant = datatype.variants.first().unwrap();
                let field_idx = match &self.spec.types.get(&t.r#type).unwrap().wasi {
                    | WasiType::Flags(flags) => {
                        flags
                            .fields
                            .iter()
                            .enumerate()
                            .find(|(_, field)| *field == &t.field)
                            .unwrap()
                            .0
                    },
                    | _ => unreachable!(),
                };

                variant.accessors.get(field_idx).unwrap().apply(&[&var])
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        io,
        path::Path,
        sync::{Arc, Mutex},
    };

    use eyre::Context as _;
    use tempfile::tempdir;
    use tracing::level_filters::LevelFilter;
    use tracing_error::ErrorLayer;
    use tracing_subscriber::{layer::SubscriberExt as _, EnvFilter};
    use wazzi_executor::ExecutorRunner;
    use wazzi_runners::{WasiRunner, Wasmtime};

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

        let dir = tempdir().unwrap();
        let mut env = Environment::preview1()?;
        let mut ctx = Context::new();
        let fdflags = env
            .spec()
            .types
            .get("fdflags")
            .unwrap()
            .wasi
            .flags()
            .unwrap();
        let filetype = env
            .spec()
            .types
            .get("filetype")
            .unwrap()
            .wasi
            .variant()
            .unwrap();
        let resource_id = env.new_resource(
            "fd".to_owned(),
            Resource {
                attributes: HashMap::from([
                    ("offset".to_owned(), WasiValue::U64(0)),
                    ("flags".to_owned(), fdflags.value(HashSet::new())),
                    (
                        "file-type".to_owned(),
                        filetype.value_from_name("directory", None).unwrap(),
                    ),
                ]),
            },
        );
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let wasmtime = Wasmtime::new(Path::new("wasmtime"));
        let executor = ExecutorRunner::new(
            &wasmtime,
            "target/debug/wazzi-executor-pb.wasm".into(),
            ".".into(),
            Some(dir.path().to_path_buf()),
        )
        .run(stderr)
        .unwrap();

        ctx.resources
            .insert(resource_id, WasiValue::Handle(wasmtime.base_dir_fd()));

        env.call(&mut ctx, &executor, "path_open")?;

        Ok(())
    }
}
