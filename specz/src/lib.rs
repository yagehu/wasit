pub mod resource;

use std::collections::{BTreeSet, HashMap};

use arbitrary::Unstructured;
use eyre::{eyre as err, Context as _, ContextCompat};
use serde::{Deserialize, Serialize};
use wazzi_executor::RunningExecutor;
use wazzi_executor_pb_rust::WasiFunc;
use wazzi_specz_wasi::{effects, FlagsValue, Function, Spec, Term, WasiType, WasiValue};
use wazzi_store::TraceStore;
use z3::ast::Ast;

use self::resource::Context;

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct Call {
    pub function: String,
    pub errno:    Option<i32>,
    pub params:   Vec<Value>,
    pub results:  Vec<Value>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct Value {
    pub wasi:     WasiValue,
    pub resource: Option<usize>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Resource {
    pub attributes: HashMap<String, WasiValue>,
}

#[derive(Debug)]
pub struct Environment {
    spec:               Spec,
    resources:          Vec<Resource>,
    resources_by_types: HashMap<String, BTreeSet<usize>>,
}

impl Environment {
    pub fn preview1() -> Result<Self, eyre::Error> {
        let mut spec = Spec::new();

        wazzi_specz_preview1::witx::preview1(&mut spec)?;

        Ok(Environment {
            spec,
            resources: Default::default(),
            resources_by_types: Default::default(),
        })
    }

    pub fn spec(&self) -> &Spec {
        &self.spec
    }

    pub fn new_resource(&mut self, type_name: String, resource: Resource) -> usize {
        self.resources.push(resource);
        self.resources_by_types
            .entry(type_name)
            .or_default()
            .insert(self.resources.len() - 1);
        self.resources.len() - 1
    }

    pub fn execute_function_effects(
        &mut self,
        function: &Function,
        result_resources: HashMap<String, usize>,
    ) -> Result<(), eyre::Error> {
        for stmt in function.effects.stmts.iter() {
            match stmt {
                | wazzi_specz_wasi::effects::Stmt::AttrSet(attr_set) => {
                    let new_attr_value = self.eval_effects_expr(&attr_set.value);
                    let resource_id = *result_resources
                        .get(&attr_set.resource)
                        .expect(&attr_set.resource);
                    let resource = self.resources.get_mut(resource_id).unwrap();
                    let attribute = resource.attributes.get_mut(&attr_set.attr).unwrap();

                    *attribute = new_attr_value;
                },
            }
        }

        eprintln!("{:#?}", self.resources);

        Ok(())
    }

    fn eval_effects_expr(&self, expr: &effects::Expr) -> WasiValue {
        match expr {
            | effects::Expr::WasiValue(value) => value.clone(),
        }
    }

    pub fn call_arbitrary_function(
        &self,
        u: &mut Unstructured,
        ctx: &mut Context,
        executor: &RunningExecutor,
        store: &mut TraceStore<Call>,
    ) -> Result<(Function, bool, Vec<Value>), eyre::Error> {
        let interface = self.spec.interfaces.get("wasi_snapshot_preview1").unwrap();
        let functions = interface.functions.values().collect::<Vec<_>>();
        let z3_cfg = z3::Config::new();
        let mut candidates = Vec::new();

        for function in functions {
            let z3_ctx = z3::Context::new(&z3_cfg);
            let mut solver = z3::Solver::new(&z3_ctx);
            let scope = FunctionScope::new(&z3_ctx, &mut solver, self, ctx, function);

            if scope.solve_input_contract(&z3_ctx, u)?.is_some() {
                candidates.push(function);
            }
        }

        let function = *u.choose(&candidates)?;

        self.call(u, ctx, executor, store, &function.name)
    }

    pub fn call(
        &self,
        u: &mut Unstructured,
        ctx: &mut Context,
        executor: &RunningExecutor,
        store: &mut TraceStore<Call>,
        function_name: &str,
    ) -> Result<(Function, bool, Vec<Value>), eyre::Error> {
        let interface = self.spec.interfaces.get("wasi_snapshot_preview1").unwrap();
        let function = interface.functions.get(function_name).unwrap();
        let z3_cfg = z3::Config::new();
        let z3_ctx = z3::Context::new(&z3_cfg);
        let mut solver = z3::Solver::new(&z3_ctx);
        let function_scope = FunctionScope::new(&z3_ctx, &mut solver, self, ctx, function);
        let params = function_scope
            .solve_input_contract(&z3_ctx, u)?
            .wrap_err("no solution found")?;
        let mut next_resource_id = self.resources.len();
        let results = function
            .results
            .iter()
            .map(|result| Value {
                wasi:     result.ty.wasi.arbitrary_value(u).unwrap(),
                resource: if result.ty.attributes.is_empty() {
                    None
                } else {
                    let id = next_resource_id;

                    next_resource_id += 1;

                    Some(id)
                },
            })
            .collect::<Vec<_>>();

        store.begin_call(&Call {
            function: function_name.to_string(),
            errno:    None,
            params:   params.clone(),
            results:  results.clone(),
        })?;

        let response = executor
            .call(wazzi_executor_pb_rust::request::Call {
                func:           WasiFunc::try_from(function_name)
                    .map_err(|_| err!("unknown wasi function name"))?
                    .into(),
                params:         function
                    .params
                    .iter()
                    .zip(params.clone())
                    .map(|(param, value)| value.wasi.into_pb(&param.ty.wasi))
                    .collect(),
                results:        function
                    .results
                    .iter()
                    .zip(results.clone())
                    .map(|(result, value)| value.wasi.into_pb(&result.ty.wasi))
                    .collect(),
                special_fields: Default::default(),
            })
            .wrap_err("failed to call")?;
        let results = response
            .results
            .clone()
            .into_iter()
            .zip(results)
            .zip(function.results.iter())
            .map(|((result, before), ty)| Value {
                wasi:     WasiValue::from_pb(&ty.ty.wasi, result),
                resource: before.resource,
            })
            .collect::<Vec<_>>();
        let ok = match response.errno_option.as_ref().unwrap() {
            | &wazzi_executor_pb_rust::response::call::Errno_option::ErrnoSome(i) => i == 0,
            | wazzi_executor_pb_rust::response::call::Errno_option::ErrnoNone(_) => true,
            | _ => todo!(),
        };

        if ok {
            for result in results.iter() {
                if let Some(id) = result.resource {
                    ctx.resources.insert(id, result.wasi.clone());
                }
            }
        }

        store.end_call(&Call {
            function: function_name.to_string(),
            // TODO(huyage)
            errno: match response.errno_option.as_ref().unwrap() {
                | &wazzi_executor_pb_rust::response::call::Errno_option::ErrnoSome(i) => Some(i),
                | wazzi_executor_pb_rust::response::call::Errno_option::ErrnoNone(_) => None,
                | _ => todo!(),
            },
            params,
            results: results.clone(),
        })?;

        Ok((function.to_owned(), ok, results))
    }
}

#[derive(Debug)]
pub struct FunctionScope<'ctx, 'e, 'r, 's> {
    solver:    &'s mut z3::Solver<'ctx>,
    env:       &'e Environment,
    ctx:       &'r Context,
    function:  &'e Function,
    datatypes: HashMap<String, z3::DatatypeSort<'ctx>>,
    variables: HashMap<String, z3::ast::Datatype<'ctx>>,
}

impl<'ctx, 'e, 'r, 's> FunctionScope<'ctx, 'e, 'r, 's> {
    pub fn new(
        z3_ctx: &'ctx z3::Context,
        solver: &'s mut z3::Solver<'ctx>,
        env: &'e Environment,
        ctx: &'r Context,
        function: &'e Function,
    ) -> Self {
        let mut datatypes = HashMap::new();
        let mut variables = HashMap::new();

        for (name, ty) in env.spec.types.iter() {
            match &ty.wasi {
                | WasiType::Handle => {
                    datatypes.insert(
                        name.clone(),
                        z3::DatatypeBuilder::new(z3_ctx, name.as_str())
                            .variant(
                                name,
                                vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::int(z3_ctx)))],
                            )
                            .finish(),
                    );
                },
                | WasiType::Flags(flags) => {
                    datatypes.insert(
                        name.clone(),
                        z3::DatatypeBuilder::new(z3_ctx, name.as_str())
                            .variant(
                                name,
                                flags
                                    .fields
                                    .iter()
                                    .map(|f| {
                                        (
                                            f.as_str(),
                                            z3::DatatypeAccessor::Sort(z3::Sort::bool(z3_ctx)),
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
                        z3::DatatypeBuilder::new(z3_ctx, name.as_str())
                            .variant(
                                name,
                                vec![(
                                    "value",
                                    z3::DatatypeAccessor::Sort(z3::Sort::string(z3_ctx)),
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

        for param in function.params.iter() {
            let type_name = param.ty.name.as_ref().unwrap();
            let datatype = datatypes.get(type_name).unwrap();

            if !param.ty.attributes.is_empty() {
                let x = z3::ast::Datatype::new_const(
                    z3_ctx,
                    format!("var--{}", param.name),
                    &datatype.sort,
                );
                let resource_ids = env.resources_by_types.get(type_name).unwrap();
                let clauses = resource_ids
                    .iter()
                    .map(|&id| {
                        let resource_const = z3::FuncDecl::new(
                            z3_ctx,
                            format!("resource--{}--{}", param.ty.name.as_ref().unwrap(), id),
                            &[],
                            &datatype.sort,
                        )
                        .apply(&[]);

                        z3::ast::Dynamic::from_ast(&x)._eq(&resource_const)
                    })
                    .collect::<Vec<_>>();

                solver.assert(&z3::ast::Bool::or(
                    z3_ctx,
                    clauses.iter().collect::<Vec<_>>().as_slice(),
                ));
                variables.insert(param.name.clone(), x);
            } else {
                let x = z3::ast::Datatype::new_const(
                    z3_ctx,
                    format!("var--{}", param.name),
                    &datatype.sort,
                );

                variables.insert(param.name.clone(), x);
            }
        }

        Self {
            solver,
            env,
            ctx,
            function,
            datatypes,
            variables,
        }
    }

    pub fn solve_input_contract(
        &self,
        ctx: &'ctx z3::Context,
        u: &mut Unstructured,
    ) -> Result<Option<Vec<Value>>, eyre::Error> {
        let input_contract = match &self.function.input_contract {
            | Some(term) => self.term_to_constraint(ctx, term),
            | None => panic!(),
        }?;

        self.solver
            .assert(
                &input_contract._eq(&z3::ast::Dynamic::from_ast(&z3::ast::Bool::from_bool(
                    ctx, true,
                ))),
            );

        let mut solutions = Vec::new();

        loop {
            if self.solver.check() != z3::SatResult::Sat {
                break;
            }

            let model = self.solver.get_model().unwrap();
            let mut clauses = Vec::new();

            for (name, var) in &self.variables {
                if let Some(v) = model.get_const_interp(var) {
                    if name == "fd" {
                        continue;
                    }

                    clauses.push(var._eq(&v).not());
                }
            }

            solutions.push(model);
            self.solver
                .assert(&z3::ast::Bool::or(ctx, &clauses.iter().collect::<Vec<_>>()));
        }

        if solutions.is_empty() {
            return Ok(None);
        }

        let model = u.choose(&solutions).wrap_err("failed to pick a solution")?;
        let solution = model.iter().collect::<Vec<_>>();
        let mut resources: HashMap<String, HashMap<z3::ast::Dynamic, usize>> = Default::default();
        let mut solved_params = HashMap::new();

        for decl in solution {
            let name = decl.name();

            if let Some(param_name) = name.strip_prefix("var--") {
                let value = model.get_const_interp(&decl.apply(&[])).unwrap();

                solved_params.insert(param_name.to_owned(), value);

                continue;
            }

            if name.starts_with("resource--") {
                let mut rsplits = name.rsplitn(3, "--");
                let resource_id = rsplits.next().unwrap();
                let resource_id = resource_id.parse::<usize>().unwrap();
                let type_name = rsplits.next().unwrap();
                let value = model.get_const_interp(&decl.apply(&[])).unwrap();

                resources
                    .entry(type_name.to_owned())
                    .or_default()
                    .insert(value, resource_id);

                continue;
            }

            unreachable!("unknown solution decl {}", name);
        }

        let mut params = Vec::with_capacity(self.function.params.len());

        for param in self.function.params.iter() {
            match solved_params.get(&param.name) {
                | Some(solved_param) => {
                    if !param.ty.attributes.is_empty() {
                        // Param is a resource.

                        let resource_idx = *resources
                            .get(param.ty.name.as_ref().unwrap())
                            .unwrap()
                            .get(solved_param)
                            .unwrap();

                        eprintln!("{:#?}", self.env.resources);
                        params.push(Value {
                            wasi:     self
                                .ctx
                                .resources
                                .get(&resource_idx)
                                .expect(&resource_idx.to_string())
                                .clone(),
                            resource: Some(resource_idx),
                        });
                    } else {
                        let value = match &param.ty.wasi {
                            | WasiType::Flags(_flags) => {
                                let mut fields = Vec::new();
                                let datatype =
                                    self.datatypes.get(param.ty.name.as_ref().unwrap()).unwrap();
                                let variant = datatype.variants.first().unwrap();

                                for accessor in &variant.accessors {
                                    let field = accessor
                                        .apply(&[solved_param])
                                        .as_bool()
                                        .unwrap()
                                        .simplify()
                                        .as_bool()
                                        .unwrap();

                                    fields.push(field);
                                }

                                WasiValue::Flags(FlagsValue { fields })
                            },
                            | _ => panic!(),
                        };

                        params.push(Value {
                            wasi:     value,
                            resource: None,
                        });
                    }
                },
                | None => {
                    let value = param
                        .ty
                        .wasi
                        .arbitrary_value(u)
                        .wrap_err("failed to generate arbitrary value")?;

                    params.push(Value {
                        wasi:     value,
                        resource: None,
                    });
                },
            }
        }

        Ok(Some(params))
    }

    fn term_to_constraint(
        &self,
        ctx: &'ctx z3::Context,
        term: &Term,
    ) -> Result<z3::ast::Dynamic, eyre::Error> {
        Ok(match term {
            | Term::Not(t) => z3::ast::Dynamic::from_ast(
                &self
                    .term_to_constraint(ctx, &t.term)?
                    .as_bool()
                    .unwrap()
                    .not(),
            ),
            | Term::And(t) => {
                let clauses = t
                    .clauses
                    .iter()
                    .map(|clause| self.term_to_constraint(ctx, clause))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|clause| clause.as_bool().unwrap())
                    .collect::<Vec<_>>();

                z3::ast::Dynamic::from_ast(&z3::ast::Bool::and(
                    ctx,
                    clauses.iter().collect::<Vec<_>>().as_slice(),
                ))
            },
            | Term::Or(t) => {
                let clauses = t
                    .clauses
                    .iter()
                    .map(|clause| self.term_to_constraint(ctx, clause))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|clause| clause.as_bool().unwrap())
                    .collect::<Vec<_>>();

                z3::ast::Dynamic::from_ast(&z3::ast::Bool::or(
                    ctx,
                    clauses.iter().collect::<Vec<_>>().as_slice(),
                ))
            },
            | Term::Param(t) => {
                let var = self.variables.get(&t.name).expect(&t.name);

                z3::ast::Dynamic::from_ast(var)
            },
            | Term::FlagsGet(t) => {
                let target = self
                    .term_to_constraint(ctx, &t.target)
                    .wrap_err("failed to translate flags get target to z3")?;
                let var = target.as_datatype().unwrap();
                let datatype = self.datatypes.get(&t.r#type).unwrap();
                let variant = datatype.variants.first().unwrap();
                let field_idx = match &self.env.spec.types.get(&t.r#type).unwrap().wasi {
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
