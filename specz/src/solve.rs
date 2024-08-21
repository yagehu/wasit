use std::collections::HashMap;

use arbitrary::Unstructured;
use eyre::Context as _;
use idxspace::IndexSpace;
use z3::ast::Ast;

use crate::{
    preview1::{
        spec::{EncodedType, Function, Spec, TypeRef},
        witx::slang::Term,
    },
    resource::Context,
    Environment,
    Value,
};

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FunctionScope<'ctx, 'c, 'e, 's> {
    ctx:       &'c Context,
    spec:      &'s Spec<'ctx>,
    env:       &'e Environment,
    function:  &'e Function,
    variables: IndexSpace<String, z3::ast::Dynamic<'ctx>>,
}

impl<'ctx, 'c, 'e, 's> FunctionScope<'ctx, 'c, 'e, 's> {
    pub fn new(
        ctx: &'ctx z3::Context,
        spec: &'s Spec<'ctx>,
        resource_ctx: &'c Context,
        env: &'e Environment,
        function: &'e Function,
    ) -> Self {
        let mut variables: IndexSpace<_, _> = Default::default();

        for param in function.params.iter() {
            variables.push(
                param.name.clone(),
                spec.get_encoded_type_by_tref(&param.tref)
                    .unwrap()
                    .declare_const(ctx),
            );
        }

        Self {
            ctx: resource_ctx,
            spec,
            env,
            function,
            variables,
        }
    }

    pub fn solve_input_contract(
        &self,
        spec: &Spec,
        solver: &z3::Solver,
        u: &mut Unstructured,
    ) -> Result<Option<Vec<Value>>, eyre::Error> {
        let mut value_resource_id_map: HashMap<z3::ast::Dynamic, usize> = Default::default();

        for (param, (_param_name, var)) in self.function.params.iter().zip(self.variables.iter()) {
            let param_type = self.spec.get_encoded_type_by_tref(&param.tref).unwrap();

            if let Some(tdef) = param.tref.resource_type_def(self.spec) {
                let resource_ids = self
                    .env
                    .resources_by_types
                    .get(&tdef.name)
                    .cloned()
                    .unwrap_or_default();
                let clauses = resource_ids
                    .iter()
                    .map(|&resource_id| {
                        let resource_value = param_type.encode_resource(
                            self.spec,
                            self.env.resources.get(resource_id).unwrap(),
                        );

                        value_resource_id_map.insert(resource_value.clone(), resource_id);
                        resource_value._eq(var)
                    })
                    .collect::<Vec<_>>();

                solver.assert(&z3::ast::Bool::or(spec.ctx, clauses.as_slice()));
            }
        }

        let input_contract = match &self.function.input_contract {
            | Some(term) => self.term_to_constraint(spec.ctx, spec, term)?.0,
            | None => z3::ast::Dynamic::from_ast(&z3::ast::Bool::from_bool(spec.ctx, true)),
        };

        solver.assert(
            &input_contract._eq(&z3::ast::Dynamic::from_ast(&z3::ast::Bool::from_bool(
                spec.ctx, true,
            ))),
        );

        let mut solutions = Vec::new();
        let mut nsolutions = 0;

        loop {
            if solver.check() != z3::SatResult::Sat || nsolutions == 100 {
                break;
            }

            let model = solver.get_model().unwrap();
            let mut clauses = Vec::new();

            for (_name, var) in self.variables.iter() {
                if let Some(v) = model.get_const_interp(&var.simplify()) {
                    clauses.push(var._eq(&v).not());
                }
            }

            nsolutions += 1;
            solutions.push(model);
            solver.assert(&z3::ast::Bool::or(
                spec.ctx,
                &clauses.iter().collect::<Vec<_>>(),
            ));
        }

        if solutions.is_empty() {
            return Ok(None);
        }

        let model = u.choose(&solutions).wrap_err("failed to pick a solution")?;
        let solution = model.iter().collect::<Vec<_>>();
        let mut resources: HashMap<String, HashMap<z3::ast::Dynamic, usize>> = Default::default();
        let mut solved_params = HashMap::new();

        for decl in solution {
            let param_name = decl.name();
            let param = self
                .function
                .params
                .iter()
                .find(|p| p.name == param_name)
                .unwrap();
            let value = model.get_const_interp(&decl.apply(&[])).unwrap();

            if let Some(tdef) = param.tref.resource_type_def(self.spec) {
                let resource_id = *value_resource_id_map.get(&value).unwrap();

                resources
                    .entry(tdef.name.clone())
                    .or_default()
                    .insert(value.clone(), resource_id);
            }

            solved_params.insert(param_name.to_string(), value);
        }

        let mut params = Vec::with_capacity(self.function.params.len());
        // This is necessary because some runtimes (notably Wasmer) map preopened direcotries to a
        // virtual root and pass the fd of the root to the Wasm module.
        let mut path_prefix: Option<Vec<u8>> = None;

        for param in self.function.params.iter() {
            match solved_params.get(&param.name) {
                | Some(solved_param) => {
                    if let Some(param_tdef) = param.tref.resource_type_def(self.spec) {
                        // Param is a resource.
                        let resource_idx = *resources
                            .get(&param_tdef.name)
                            .unwrap()
                            .get(solved_param)
                            .unwrap();
                        let (resource_value, preopened) =
                            self.ctx.resources.get(&resource_idx).unwrap();

                        if let Some(preopened_name) = preopened {
                            if path_prefix.is_none() {
                                path_prefix = Some(preopened_name.clone());
                            }
                        }

                        params.push(Value {
                            wasi:     resource_value.clone(),
                            resource: Some(resource_idx),
                        });
                    } else {
                        params.push(Value {
                            wasi:     spec
                                .get_encoded_type_by_tref(&param.tref)
                                .unwrap()
                                .wasi_value(solved_param),
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
                    let value = param.tref.arbitrary_value(spec, u, path_prefix)?;

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
        spec: &'s Spec<'ctx>,
        term: &Term,
    ) -> Result<(z3::ast::Dynamic, &EncodedType), eyre::Error> {
        Ok(match term {
            | Term::Not(t) => (
                z3::ast::Dynamic::from_ast(
                    &self
                        .term_to_constraint(ctx, spec, &t.term)?
                        .0
                        .as_bool()
                        .unwrap()
                        .not(),
                ),
                spec.get_encoded_type_by_tref(&TypeRef::Named("bool".to_string()))
                    .unwrap(),
            ),
            | Term::And(t) => {
                let clauses = t
                    .clauses
                    .iter()
                    .map(|clause| self.term_to_constraint(ctx, spec, clause))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|clause| clause.0.as_bool().unwrap())
                    .collect::<Vec<_>>();

                (
                    z3::ast::Dynamic::from_ast(&z3::ast::Bool::and(
                        ctx,
                        clauses.iter().collect::<Vec<_>>().as_slice(),
                    )),
                    spec.get_encoded_type_by_tref(&TypeRef::Named("bool".to_string()))
                        .unwrap(),
                )
            },
            | Term::Or(t) => {
                let clauses = t
                    .clauses
                    .iter()
                    .map(|clause| self.term_to_constraint(ctx, spec, clause))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|clause| clause.0.as_bool().unwrap())
                    .collect::<Vec<_>>();

                (
                    z3::ast::Dynamic::from_ast(&z3::ast::Bool::or(
                        ctx,
                        clauses.iter().collect::<Vec<_>>().as_slice(),
                    )),
                    spec.get_encoded_type_by_tref(&TypeRef::Named("bool".to_string()))
                        .unwrap(),
                )
            },
            | Term::AttrGet(t) => {
                let (target, target_type) = self.term_to_constraint(ctx, spec, &t.target)?;

                target_type.attr_get(spec, &target, &t.attr).unwrap()
            },
            | Term::Param(param) => {
                let param = self
                    .function
                    .params
                    .iter()
                    .find(|p| p.name == param.name)
                    .unwrap();

                (
                    self.variables.get_by_key(&param.name).unwrap().clone(),
                    spec.get_encoded_type_by_tref(&param.tref).unwrap(),
                )
            },
            | Term::FlagsGet(t) => {
                let (target, target_type) = self
                    .term_to_constraint(ctx, spec, &t.target)
                    .wrap_err("failed to translate flags get target to z3")?;

                target_type.flags_get(spec, &target, &t.field)
            },
            | Term::IntConst(t) => {
                let ty = spec
                    .get_encoded_type_by_tref(&TypeRef::Named("int".to_string()))
                    .unwrap();

                (ty.const_int_from_str(ctx, t.to_string().as_str()), ty)
            },
            | Term::IntAdd(t) => {
                let (lhs, lhs_type) = self.term_to_constraint(ctx, spec, &t.lhs)?;
                let (rhs, _rhs_type) = self.term_to_constraint(ctx, spec, &t.rhs)?;

                (
                    lhs_type.int_add(ctx, &lhs, &rhs),
                    spec.get_encoded_type_by_tref(&TypeRef::Named("int".to_string()))
                        .unwrap(),
                )
            },
            | Term::IntLe(t) => {
                let (lhs, lhs_type) = self.term_to_constraint(ctx, spec, &t.lhs)?;
                let (rhs, rhs_type) = self.term_to_constraint(ctx, spec, &t.rhs)?;
                eprintln!("lhs {:?} {:?}\nrhs {:?} {:?}", lhs, lhs_type, rhs, rhs_type);
                (
                    lhs_type.int_le(&lhs, &rhs),
                    spec.get_encoded_type_by_tref(&TypeRef::Named("int".to_string()))
                        .unwrap(),
                )
            },
            | Term::ValueEq(t) => {
                let (lhs, _) = self.term_to_constraint(ctx, spec, &t.lhs)?;
                let (rhs, _) = self.term_to_constraint(ctx, spec, &t.rhs)?;

                (
                    z3::ast::Dynamic::from_ast(&lhs._eq(&rhs)),
                    spec.get_encoded_type_by_tref(&TypeRef::Named("bool".to_string()))
                        .unwrap(),
                )
            },
            | Term::VariantConst(t) => {
                let (payload, _payload_type) = t
                    .payload
                    .as_ref()
                    .map(|term| self.term_to_constraint(ctx, spec, &term))
                    .transpose()?
                    .unzip();
                let encoded_type = spec
                    .get_encoded_type_by_tref(&TypeRef::Named(t.ty.clone()))
                    .unwrap();

                (
                    encoded_type.const_variant(ctx, &t.case, payload),
                    encoded_type,
                )
            },
        })
    }
}
