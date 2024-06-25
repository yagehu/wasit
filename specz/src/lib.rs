pub mod function_picker;
pub mod param_generator;
pub mod resource;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use self::resource::Context;
use crate::{function_picker::FunctionPicker, param_generator::ParamsGenerator};
use arbitrary::Unstructured;
use eyre::{eyre as err, Context as _, ContextCompat};
use serde::{Deserialize, Serialize};
use wazzi_executor::RunningExecutor;
use wazzi_executor_pb_rust::WasiFunc;
use wazzi_specz_wasi::{
    effects,
    FlagsValue,
    Function,
    Spec,
    Term,
    VariantValue,
    WasiType,
    WasiValue,
    WazziType,
};
use wazzi_store::TraceStore;
use z3::ast::Ast;

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
                | effects::Stmt::AttrSet(attr_set) => {
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

        Ok(())
    }

    fn eval_effects_expr(&self, expr: &effects::Expr) -> WasiValue {
        match expr {
            | effects::Expr::WasiValue(value) => value.clone(),
        }
    }

    pub fn call(
        &self,
        u: &mut Unstructured,
        ctx: &mut Context,
        executor: &RunningExecutor,
        store: &mut TraceStore<Call>,
        function: &Function,
        params_generator: &dyn ParamsGenerator,
    ) -> Result<(bool, Vec<Value>), eyre::Error> {
        let params = params_generator.generate_params(u, self, ctx, function)?;
        let mut next_resource_id = self.resources.len();
        let results = function
            .results
            .iter()
            .map(|result| Value {
                wasi:     result.ty.wasi.arbitrary_value(u, None).unwrap(),
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
            function: function.name.to_string(),
            errno:    None,
            params:   params.clone(),
            results:  results.clone(),
        })?;

        let response = executor
            .call(wazzi_executor_pb_rust::request::Call {
                func:           WasiFunc::try_from(function.name.as_str())
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
                    ctx.resources.insert(id, (result.wasi.clone(), None));
                }
            }
        }

        store.end_call(&Call {
            function: function.name.to_string(),
            // TODO(huyage)
            errno: match response.errno_option.as_ref().unwrap() {
                | &wazzi_executor_pb_rust::response::call::Errno_option::ErrnoSome(i) => Some(i),
                | wazzi_executor_pb_rust::response::call::Errno_option::ErrnoNone(_) => None,
                | _ => todo!(),
            },
            params,
            results: results.clone(),
        })?;

        Ok((ok, results))
    }
}

#[derive(Debug)]
pub struct FunctionScope<'ctx, 'e, 'r> {
    solver:                z3::Solver<'ctx>,
    env:                   &'e Environment,
    ctx:                   &'r Context,
    function:              &'e Function,
    datatypes:             HashMap<String, z3::DatatypeSort<'ctx>>,
    variables:             BTreeMap<String, z3::ast::Datatype<'ctx>>,
    value_resource_id_map: HashMap<z3::ast::Dynamic<'ctx>, usize>,
}

impl<'ctx, 'e, 'r> FunctionScope<'ctx, 'e, 'r> {
    fn wasi_type_to_z3_sort(
        ctx: &'ctx z3::Context,
        ty: &WazziType,
        datatypes: &mut HashMap<String, z3::DatatypeSort<'ctx>>,
    ) {
        let name = ty.name.as_ref().unwrap().to_string();
        let attributes_datatype_name = format!("{name}--attrs");

        if !ty.attributes.is_empty() {
            datatypes.insert(
                attributes_datatype_name.clone(),
                z3::DatatypeBuilder::new(ctx, attributes_datatype_name.as_str())
                    .variant(
                        &attributes_datatype_name,
                        ty.attributes
                            .iter()
                            .map(|(attr, ty)| {
                                (
                                    attr.as_str(),
                                    z3::DatatypeAccessor::Sort(
                                        datatypes
                                            .get(ty.name.as_ref().unwrap())
                                            .unwrap()
                                            .sort
                                            .clone(),
                                    ),
                                )
                            })
                            .collect(),
                    )
                    .finish(),
            );
        }

        match &ty.wasi {
            | WasiType::Handle => {
                datatypes.insert(
                    name.clone(),
                    z3::DatatypeBuilder::new(ctx, name.as_str())
                        .variant(
                            &name,
                            vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
                        )
                        .finish(),
                );
            },
            | WasiType::S64 | WasiType::U8 | WasiType::U32 | WasiType::U64 => {
                datatypes.insert(
                    name.clone(),
                    z3::DatatypeBuilder::new(ctx, name.as_str())
                        .variant(
                            &name,
                            vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::int(ctx)))],
                        )
                        .finish(),
                );
            },
            | WasiType::Flags(flags) => {
                datatypes.insert(
                    name.clone(),
                    z3::DatatypeBuilder::new(ctx, name.as_str())
                        .variant(
                            &name,
                            flags
                                .fields
                                .iter()
                                .map(|f| {
                                    (f.as_str(), z3::DatatypeAccessor::Sort(z3::Sort::bool(ctx)))
                                })
                                .collect::<Vec<_>>(),
                        )
                        .finish(),
                );
            },
            | WasiType::List(list) => {
                let item_sort = datatypes.get(list.item.name.as_ref().unwrap()).unwrap();

                datatypes.insert(
                    name.clone(),
                    z3::DatatypeBuilder::new(ctx, name.as_str())
                        .variant(
                            &name,
                            vec![(
                                "value",
                                z3::DatatypeAccessor::Sort(z3::Sort::array(
                                    ctx,
                                    &z3::Sort::int(ctx),
                                    &item_sort.sort,
                                )),
                            )],
                        )
                        .finish(),
                );
            },
            | WasiType::String => {
                datatypes.insert(
                    name.clone(),
                    z3::DatatypeBuilder::new(ctx, name.as_str())
                        .variant(
                            &name,
                            vec![("value", z3::DatatypeAccessor::Sort(z3::Sort::string(ctx)))],
                        )
                        .finish(),
                );
            },
            | WasiType::Record(record) => {
                let mut datatype = z3::DatatypeBuilder::new(ctx, name.as_str());

                for member in record.members.iter() {
                    datatype = datatype.variant(
                        &member.name,
                        record
                            .members
                            .iter()
                            .map(|member| {
                                (
                                    member.name.as_str(),
                                    z3::DatatypeAccessor::Sort(
                                        datatypes
                                            .get(member.ty.name.as_ref().unwrap())
                                            .expect(member.ty.name.as_ref().unwrap())
                                            .sort
                                            .clone(),
                                    ),
                                )
                            })
                            .collect(),
                    )
                }

                datatypes.insert(name.clone(), datatype.finish());
            },
            | WasiType::Variant(variant) => {
                let mut datatype = z3::DatatypeBuilder::new(ctx, name.as_str());

                for case in variant.cases.iter() {
                    let fields = match &case.payload {
                        | Some(payload) => {
                            let payload_datatype = datatypes.get(payload.name.as_ref().unwrap());

                            vec![(
                                "value",
                                z3::DatatypeAccessor::Sort(
                                    payload_datatype
                                        .expect(payload.name.as_ref().unwrap())
                                        .sort
                                        .clone(),
                                ),
                            )]
                        },
                        | None => vec![],
                    };

                    datatype = datatype.variant(&case.name, fields);
                }

                datatypes.insert(name.clone(), datatype.finish());
            },
            | _ => {
                tracing::trace!("Ignoring type {}", name);
            },
        }
    }

    pub fn new(
        z3_ctx: &'ctx z3::Context,
        solver: z3::Solver<'ctx>,
        env: &'e Environment,
        ctx: &'r Context,
        function: &'e Function,
    ) -> Self {
        let mut datatypes = HashMap::new();
        let mut variables = BTreeMap::new();
        let mut value_resource_id_map = HashMap::new();

        for ty in env.spec.types.iter() {
            Self::wasi_type_to_z3_sort(z3_ctx, ty, &mut datatypes);
        }

        solver.push();

        for param in function.params.iter() {
            if !param.ty.attributes.is_empty() {
                let type_name = format!("{}--attrs", param.ty.name.as_ref().unwrap());
                let datatype = datatypes.get(&type_name).expect(&type_name);
                let datatype_variant = datatype.variants.first().unwrap();
                let x = z3::ast::Datatype::new_const(
                    z3_ctx,
                    format!("var--{}", param.name),
                    &datatype.sort,
                );
                let resource_ids = env
                    .resources_by_types
                    .get(param.ty.name.as_ref().unwrap())
                    .unwrap();
                let clauses = resource_ids
                    .iter()
                    .map(|&id| {
                        let resource = env.resources.get(id).unwrap();
                        let mut subclauses = Vec::new();
                        let mut members = Vec::new();

                        for (i, (attr_name, attr_type)) in param.ty.attributes.iter().enumerate() {
                            let accessor = datatype_variant.accessors.get(i).unwrap();
                            let attr_datatype =
                                datatypes.get(attr_type.name.as_ref().unwrap()).unwrap();
                            let ctor = &attr_datatype.variants.first().unwrap().constructor;
                            let attr_value = resource.attributes.get(attr_name).expect(attr_name);
                            let value = match attr_value {
                                | &WasiValue::Handle(handle) => {
                                    ctor.apply(&[&z3::ast::Int::from_u64(z3_ctx, handle.into())])
                                },
                                | &WasiValue::S64(i) => {
                                    ctor.apply(&[&z3::ast::Int::from_i64(z3_ctx, i)])
                                },
                                | &WasiValue::U8(i) => {
                                    ctor.apply(&[&z3::ast::Int::from_u64(z3_ctx, i.into())])
                                },
                                | &WasiValue::U32(i) => {
                                    ctor.apply(&[&z3::ast::Int::from_u64(z3_ctx, i.into())])
                                },
                                | &WasiValue::U64(i) => {
                                    ctor.apply(&[&z3::ast::Int::from_u64(z3_ctx, i)])
                                },
                                | WasiValue::Record(record) => todo!(),
                                | WasiValue::Flags(flags) => {
                                    let fields = flags
                                        .fields
                                        .iter()
                                        .map(|&b| z3::ast::Bool::from_bool(z3_ctx, b))
                                        .collect::<Vec<_>>();
                                    let fields = fields
                                        .iter()
                                        .map(|ast| ast as &dyn z3::ast::Ast)
                                        .collect::<Vec<_>>();

                                    ctor.apply(fields.as_slice())
                                },
                                | WasiValue::List(list) => todo!(),
                                | WasiValue::String(_) => todo!(),
                                | WasiValue::Variant(variant) => {
                                    let ctor = &attr_datatype
                                        .variants
                                        .get(variant.case_idx)
                                        .unwrap()
                                        .constructor;

                                    match variant.payload {
                                        | Some(_) => todo!(),
                                        | None => ctor.apply(&[]),
                                    }
                                },
                            };

                            subclauses.push(accessor.apply(&[&x])._eq(&value));
                            members.push(value);
                            //subclauses.push(accessor.apply(&[&resource_const])._eq(&value));
                        }

                        let value = datatype_variant.constructor.apply(
                            members
                                .iter()
                                .map(|m| m as &dyn z3::ast::Ast)
                                .collect::<Vec<_>>()
                                .as_slice(),
                        );

                        value_resource_id_map.insert(value, id);

                        z3::ast::Bool::and(z3_ctx, subclauses.iter().collect::<Vec<_>>().as_slice())
                    })
                    .collect::<Vec<_>>();

                solver.assert(&z3::ast::Bool::or(z3_ctx, &clauses));
                variables.insert(type_name, x);
            } else {
                let type_name = param.ty.name.as_ref().expect(&format!("{:#?}", param.ty));
                let datatype = datatypes.get(type_name).expect(type_name);
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
            value_resource_id_map,
        }
    }

    pub fn solve_input_contract(
        &self,
        ctx: &'ctx z3::Context,
        u: &mut Unstructured,
    ) -> Result<Option<Vec<Value>>, eyre::Error> {
        let input_contract = match &self.function.input_contract {
            | Some(term) => self.term_to_constraint(ctx, term)?.0,
            | None => z3::ast::Dynamic::from_ast(&z3::ast::Bool::from_bool(ctx, true)),
        };

        self.solver
            .assert(
                &input_contract._eq(&z3::ast::Dynamic::from_ast(&z3::ast::Bool::from_bool(
                    ctx, true,
                ))),
            );

        let mut solutions = Vec::new();
        let mut nsolutions = 0;

        loop {
            if self.solver.check() != z3::SatResult::Sat || nsolutions == 100 {
                break;
            }

            let model = self.solver.get_model().unwrap();
            let mut clauses = Vec::new();

            for (_name, var) in &self.variables {
                if let Some(v) = model.get_const_interp(&var.simplify()) {
                    clauses.push(var._eq(&v).not());
                }
            }

            nsolutions += 1;
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
                let param = self
                    .function
                    .params
                    .iter()
                    .find(|p| p.name == param_name)
                    .unwrap();
                let ty = self
                    .env
                    .spec()
                    .types
                    .get(
                        *self
                            .env
                            .spec()
                            .types_map
                            .get(param.ty.name.as_ref().unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                let value = model.get_const_interp(&decl.apply(&[])).unwrap();

                if ty.attributes.is_empty() {
                    solved_params.insert(param_name.to_owned(), value);
                } else {
                    let resource_id = *self.value_resource_id_map.get(&value).unwrap();

                    resources
                        .entry(ty.name.as_ref().unwrap().to_owned())
                        .or_default()
                        .insert(value.clone(), resource_id);
                    solved_params.insert(param_name.to_owned(), value);
                }

                continue;
            }

            unreachable!("unknown solution decl {}", name);
        }

        let mut params = Vec::with_capacity(self.function.params.len());
        // This is necessary because some runtimes (notably Wasmer) map preopened direcotries to a
        // virtual root and pass the fd of the root to the Wasm module.
        let mut path_prefix = None;

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
                        let (resource_value, preopened) =
                            self.ctx.resources.get(&resource_idx).unwrap();

                        if let Some(preopened_name) = &preopened {
                            if path_prefix.is_none() {
                                path_prefix = Some(preopened_name.clone());
                            }
                        }

                        params.push(Value {
                            wasi:     resource_value.clone(),
                            resource: Some(resource_idx),
                        });
                    } else {
                        let value = match &param.ty.wasi {
                            | WasiType::S64 => {
                                let datatype =
                                    self.datatypes.get(param.ty.name.as_ref().unwrap()).unwrap();
                                let int = datatype
                                    .variants
                                    .first()
                                    .unwrap()
                                    .accessors
                                    .first()
                                    .unwrap()
                                    .apply(&[solved_param])
                                    .as_int()
                                    .unwrap();

                                WasiValue::S64(int.simplify().as_i64().unwrap())
                            },
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
                            | WasiType::Variant(_variant) => {
                                let datatype =
                                    self.datatypes.get(param.ty.name.as_ref().unwrap()).unwrap();
                                let mut case_idx = 0;
                                let payload = None;

                                for (i, datatype_variant) in datatype.variants.iter().enumerate() {
                                    if datatype_variant
                                        .tester
                                        .apply(&[solved_param])
                                        .simplify()
                                        .as_bool()
                                        .unwrap()
                                        .as_bool()
                                        .unwrap()
                                    {
                                        case_idx = i;

                                        if let Some(_accessor) = datatype_variant.accessors.first()
                                        {
                                            todo!();
                                        }

                                        break;
                                    }
                                }

                                WasiValue::Variant(Box::new(VariantValue { case_idx, payload }))
                            },
                            | _ => panic!("{:?}", solved_param),
                        };

                        params.push(Value {
                            wasi:     value,
                            resource: None,
                        });
                    }
                },
                | None => {
                    let path_prefix = path_prefix.clone();
                    let path_prefix = match &path_prefix {
                        | Some(s) => Some(s.as_slice()),
                        | None => None,
                    };

                    let value = param
                        .ty
                        .wasi
                        .arbitrary_value(u, path_prefix)
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
    ) -> Result<(z3::ast::Dynamic, Option<String>), eyre::Error> {
        Ok(match term {
            | Term::Not(t) => (
                z3::ast::Dynamic::from_ast(
                    &self
                        .term_to_constraint(ctx, &t.term)?
                        .0
                        .as_bool()
                        .unwrap()
                        .not(),
                ),
                None,
            ),
            | Term::And(t) => {
                let clauses = t
                    .clauses
                    .iter()
                    .map(|clause| self.term_to_constraint(ctx, clause))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|clause| clause.0.as_bool().unwrap())
                    .collect::<Vec<_>>();

                (
                    z3::ast::Dynamic::from_ast(&z3::ast::Bool::and(
                        ctx,
                        clauses.iter().collect::<Vec<_>>().as_slice(),
                    )),
                    None,
                )
            },
            | Term::Or(t) => {
                let clauses = t
                    .clauses
                    .iter()
                    .map(|clause| self.term_to_constraint(ctx, clause))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|clause| clause.0.as_bool().unwrap())
                    .collect::<Vec<_>>();

                (
                    z3::ast::Dynamic::from_ast(&z3::ast::Bool::or(
                        ctx,
                        clauses.iter().collect::<Vec<_>>().as_slice(),
                    )),
                    None,
                )
            },
            | Term::AttrGet(t) => {
                let (target, target_type) = self.term_to_constraint(ctx, &t.target)?;
                let (i, (_, attr_type)) = &self
                    .env
                    .spec()
                    .types
                    .get(
                        *self
                            .env
                            .spec()
                            .types_map
                            .get(target_type.as_ref().unwrap())
                            .unwrap(),
                    )
                    .unwrap()
                    .attributes
                    .iter()
                    .enumerate()
                    .find(|(_, (attr, _))| attr == &t.attr)
                    .unwrap();
                let datatype = self
                    .datatypes
                    .get(&format!("{}--attrs", target_type.as_ref().unwrap()))
                    .unwrap();
                let datatype_variant = datatype.variants.first().unwrap();

                (
                    datatype_variant
                        .accessors
                        .get(*i)
                        .unwrap()
                        .apply(&[&target]),
                    attr_type.name.clone(),
                )
            },
            | Term::Param(t) => {
                let param = self
                    .function
                    .params
                    .iter()
                    .find(|p| p.name == t.name)
                    .unwrap();

                if !param.ty.attributes.is_empty() {
                    let name = format!("{}--attrs", param.ty.name.as_ref().unwrap());
                    let var = self.variables.get(&name).expect(&name);

                    (z3::ast::Dynamic::from_ast(var), param.ty.name.clone())
                } else {
                    let var = self.variables.get(&t.name).expect(&t.name);

                    (z3::ast::Dynamic::from_ast(var), param.ty.name.clone())
                }
            },
            | Term::FlagsGet(t) => {
                let (target, target_type_name) = self
                    .term_to_constraint(ctx, &t.target)
                    .wrap_err("failed to translate flags get target to z3")?;
                let var = target.as_datatype().unwrap();
                let datatype = self.datatypes.get(&t.r#type).unwrap();
                let variant = datatype.variants.first().unwrap();
                let field_idx = match &self
                    .env
                    .spec
                    .types
                    .get(*self.env.spec.types_map.get(&t.r#type).unwrap())
                    .unwrap()
                    .wasi
                {
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

                (
                    variant.accessors.get(field_idx).unwrap().apply(&[&var]),
                    None,
                )
            },
            | Term::IntConst(t) => (
                z3::ast::Dynamic::from_ast(
                    &z3::ast::Int::from_str(ctx, t.to_string().as_str()).unwrap(),
                ),
                None,
            ),
            | Term::IntAdd(t) => {
                let (lhs, lhs_type_name) = self.term_to_constraint(ctx, &t.lhs)?;
                let (rhs, rhs_type_name) = self.term_to_constraint(ctx, &t.rhs)?;
                let lhs = match lhs_type_name {
                    | Some(name) => {
                        let datatype = self.datatypes.get(&name).unwrap();
                        let accessor = datatype
                            .variants
                            .first()
                            .unwrap()
                            .accessors
                            .first()
                            .unwrap();

                        accessor.apply(&[&lhs]).as_int().unwrap()
                    },
                    | None => lhs.as_int().unwrap(),
                };
                let rhs = match rhs_type_name {
                    | Some(name) => {
                        let datatype = self.datatypes.get(&name).unwrap();
                        let accessor = datatype
                            .variants
                            .first()
                            .unwrap()
                            .accessors
                            .first()
                            .unwrap();
                        accessor.apply(&[&rhs]).as_int().unwrap()
                    },
                    | None => rhs.as_int().unwrap(),
                };

                (
                    z3::ast::Dynamic::from_ast(&z3::ast::Int::add(ctx, &[&lhs, &rhs])),
                    None,
                )
            },
            | Term::IntLe(t) => {
                let (lhs, lhs_type_name) = self.term_to_constraint(ctx, &t.lhs)?;
                let (rhs, rhs_type_name) = self.term_to_constraint(ctx, &t.rhs)?;
                let lhs = match lhs_type_name {
                    | Some(name) => {
                        let datatype = self.datatypes.get(&name).expect(&name);
                        let accessor = datatype
                            .variants
                            .first()
                            .unwrap()
                            .accessors
                            .first()
                            .unwrap();

                        accessor.apply(&[&lhs]).as_int().unwrap()
                    },
                    | None => lhs.as_int().unwrap(),
                };
                let rhs = match rhs_type_name {
                    | Some(name) => {
                        let datatype = self.datatypes.get(&name).unwrap();
                        let accessor = datatype
                            .variants
                            .first()
                            .unwrap()
                            .accessors
                            .first()
                            .unwrap();

                        accessor.apply(&[&rhs]).as_int().unwrap()
                    },
                    | None => rhs.as_int().unwrap(),
                };

                (z3::ast::Dynamic::from_ast(&lhs.le(&rhs)), None)
            },
            | Term::ValueEq(t) => {
                let (lhs, _) = self.term_to_constraint(ctx, &t.lhs)?;
                let (rhs, _) = self.term_to_constraint(ctx, &t.rhs)?;

                (z3::ast::Dynamic::from_ast(&lhs._eq(&rhs)), None)
            },
            | Term::VariantConst(t) => {
                let datatype = self.datatypes.get(&t.ty).unwrap();
                let ty = self
                    .env
                    .spec()
                    .types
                    .get(*self.env.spec().types_map.get(&t.ty).unwrap())
                    .unwrap();
                let variant = ty.wasi.variant().unwrap();
                let (case_idx, _) = variant
                    .cases
                    .iter()
                    .enumerate()
                    .find(|(_, case)| case.name == t.case)
                    .unwrap();
                let datatype_variant = datatype.variants.get(case_idx).unwrap();
                let payload = match &t.payload {
                    | Some(payload) => vec![self.term_to_constraint(ctx, payload)?],
                    | None => vec![],
                };
                let payload = payload
                    .iter()
                    .map(|x| &x.0 as &dyn z3::ast::Ast)
                    .collect::<Vec<_>>();

                (
                    datatype_variant.constructor.apply(payload.as_slice()),
                    Some(t.ty.clone()),
                )
            },
        })
    }
}
