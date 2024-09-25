use std::collections::{BTreeMap, HashMap};

use arbitrary::Unstructured;
use eyre::Context as _;
use idxspace::IndexSpace;
use itertools::Itertools;
use z3::ast::Ast;

use crate::{
    preview1::{
        spec::{EncodedType, Function, Spec, TypeRef},
        witx::slang::Term,
    },
    resource::Context,
    slang::{
        fs::{FdType, FileType, WasiFs, WasiFsEncoding},
        path::{PathParam, PathParamEncoding, SegmentType},
        OptionType,
    },
    Environment,
    Value,
};

#[derive(Debug)]
pub struct FunctionScope<'ctx, 'c, 'env, 'spec> {
    ctx:       &'c Context,
    spec:      &'spec Spec<'ctx>,
    env:       &'env Environment,
    function:  &'env Function,
    variables: IndexSpace<String, z3::ast::Dynamic<'ctx>>,
}

impl<'ctx, 'c, 'env, 'spec> FunctionScope<'ctx, 'c, 'env, 'spec>
where
    'spec: 'ctx,
{
    pub fn new(
        spec: &'spec Spec<'ctx>,
        resource_ctx: &'c Context,
        env: &'env Environment,
        function: &'env Function,
    ) -> Self {
        let mut variables: IndexSpace<_, _> = Default::default();

        for param in function.params.iter() {
            variables.push(
                param.name.clone(),
                spec.get_encoded_type_by_tref(&param.tref)
                    .unwrap()
                    .declare_const(spec.ctx),
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
        u: &mut Unstructured,
        solver: &z3::Solver<'ctx>,
    ) -> Result<Option<Vec<Value>>, eyre::Error> {
        // Encode the base directory filesystem.

        let mut fs = WasiFs::new();
        let base_dir_file_id = fs.push_dir(&self.ctx.base_dir_host_path)?;
        let fd_type = self
            .spec
            .get_encoded_type_by_tref(&TypeRef::Named("fd".to_string()))
            .unwrap();
        let file_type = self
            .spec
            .get_encoded_type_by_tref(&TypeRef::Named("file".to_string()))
            .unwrap();
        let fd_type: FdType = fd_type.into();
        let file_type: FileType = file_type.into();
        let fs_encoding = fs.encode(&self.spec.ctx, &fd_type, &file_type);

        fs_encoding.assert(solver);

        let mut value_resource_id_map: HashMap<z3::ast::Dynamic, usize> = Default::default();
        let segment_type = self
            .spec
            .get_encoded_type_by_tref(&TypeRef::Named("segment".to_string()))
            .unwrap();
        let segment_type: SegmentType<'_, 'ctx> = segment_type.into();
        let mut path_params: BTreeMap<String, PathParamEncoding<'_, z3::ast::Dynamic<'_>>> =
            Default::default();

        for (param, (_param_name, var)) in self.function.params.iter().zip(self.variables.iter()) {
            let param_type = self.spec.get_encoded_type_by_tref(&param.tref).unwrap();

            // `path` type is a special case.
            if param.name == "path" {
                let n_segments = u.choose_index(4).unwrap();
                let path_param = PathParam::new(param.name.clone(), n_segments);
                let path_param_encoding = path_param.encode(&self.spec.ctx, &segment_type);

                path_params.insert(param.name.clone(), path_param_encoding);

                continue;
            }

            if let Some(tdef) = param.tref.resource_type_def(self.spec) {
                if tdef.attributes.is_none() {
                    continue;
                }

                let attrs = tdef.attributes.as_ref().unwrap();
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

                        let clauses = attrs
                            .iter()
                            .map(|(attr_name, _attr_tref)| {
                                let (attr, _ty) =
                                    param_type.attr_get(self.spec, var, attr_name).unwrap();

                                attr._eq(
                                    &param_type
                                        .attr_get(self.spec, &resource_value, attr_name)
                                        .unwrap()
                                        .0,
                                )
                            })
                            .collect_vec();

                        z3::ast::Bool::and(self.spec.ctx, clauses.as_slice())
                    })
                    .collect_vec();

                solver.assert(&z3::ast::Bool::or(self.spec.ctx, clauses.as_slice()));
            }
        }

        for (_, enc) in &path_params {
            enc.assert(solver);
        }

        let input_contract = match &self.function.input_contract {
            | Some(term) => self.term_to_constraint(self.spec, term)?.0,
            | None => z3::ast::Dynamic::from_ast(&z3::ast::Bool::from_bool(self.spec.ctx, true)),
        };

        solver.assert(
            &input_contract._eq(&z3::ast::Dynamic::from_ast(&z3::ast::Bool::from_bool(
                self.spec.ctx,
                true,
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
                self.spec.ctx,
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
            let ast_node = decl.apply(&[]);
            let param_name = self
                .variables
                .iter()
                .find(|&(_name, node)| node == &ast_node)
                .unwrap()
                .0;
            let param = self
                .function
                .params
                .iter()
                .find(|p| &p.name == param_name)
                .unwrap();
            let value = model.get_const_interp(&decl.apply(&[])).unwrap();

            if let Some(tdef) = param.tref.resource_type_def(self.spec) {
                if tdef.attributes.is_some() {
                    let resource_id = *value_resource_id_map.get(&value).unwrap();

                    resources
                        .entry(tdef.name.clone())
                        .or_default()
                        .insert(value.clone(), resource_id);
                }
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
                        if param_tdef.attributes.is_some() {
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
                                wasi:     self
                                    .spec
                                    .get_encoded_type_by_tref(&param.tref)
                                    .unwrap()
                                    .wasi_value(solved_param),
                                resource: None,
                            });
                        }
                    } else {
                        params.push(Value {
                            wasi:     self
                                .spec
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
                    let value = param.tref.arbitrary_value(self.spec, u, path_prefix)?;

                    params.push(Value {
                        wasi:     value,
                        resource: None,
                    });
                },
            }
        }

        Ok(Some(params))
    }

}

#[derive(Debug)]
struct FunctionConstraintTranslationScope<'ctx, 'c, 'env, 'spec> {
    parent: FunctionScope<'ctx, 'c, 'env, 'spec>,
    path_params: BTreeMap<String, PathParamEncoding<'ctx, z3::ast::Dynamic<'ctx>>>,
}

impl<'ctx> FunctionConstraintTranslationScope<'ctx> {
    fn term_to_constraint(
        &self,
        spec: &'s Spec<'ctx>,
        term: &Term,
        fs_encoding: WasiFsEncoding<'ctx>,
        base_dir_file_id: usize,
    ) -> Result<(z3::ast::Dynamic, &EncodedType), eyre::Error> {
        Ok(match term {
            | Term::Not(t) => (
                z3::ast::dynamic::from_ast(
                    &self
                        .term_to_constraint(spec, &t.term)?
                        .0
                        .as_bool()
                        .unwrap()
                        .not(),
                ),
                spec.get_encoded_type_by_tref(&typeref::named("bool".to_string()))
                    .unwrap(),
            ),
            | Term::And(t) => {
                let clauses = t
                    .clauses
                    .iter()
                    .map(|clause| self.term_to_constraint(spec, clause))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|clause| clause.0.as_bool().unwrap())
                    .collect::<Vec<_>>();

                (
                    z3::ast::Dynamic::from_ast(&z3::ast::Bool::and(
                        spec.ctx,
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
                    .map(|clause| self.term_to_constraint(spec, clause))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|clause| clause.0.as_bool().unwrap())
                    .collect::<Vec<_>>();

                (
                    z3::ast::Dynamic::from_ast(&z3::ast::Bool::or(
                        spec.ctx,
                        clauses.iter().collect::<Vec<_>>().as_slice(),
                    )),
                    spec.get_encoded_type_by_tref(&TypeRef::Named("bool".to_string()))
                        .unwrap(),
                )
            },
            | Term::AttrGet(t) => {
                let (target, target_type) = self.term_to_constraint(spec, &t.target)?;

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
                    .term_to_constraint(spec, &t.target)
                    .wrap_err("failed to translate flags get target to z3")?;

                target_type.flags_get(spec, &target, &t.field)
            },
            | Term::IntConst(t) => {
                let ty = spec
                    .get_encoded_type_by_tref(&TypeRef::Named("int".to_string()))
                    .unwrap();

                (ty.const_int_from_str(spec.ctx, t.to_string().as_str()), ty)
            },
            | Term::IntAdd(t) => {
                let (lhs, lhs_type) = self.term_to_constraint(spec, &t.lhs)?;
                let (rhs, rhs_type) = self.term_to_constraint(spec, &t.rhs)?;
                let int_type = spec
                    .get_encoded_type_by_tref(&TypeRef::Named("int".to_string()))
                    .unwrap();

                (
                    lhs_type.int_add(spec.ctx, &lhs, rhs_type, &rhs, int_type),
                    spec.get_encoded_type_by_tref(&TypeRef::Named("int".to_string()))
                        .unwrap(),
                )
            },
            | Term::IntLe(t) => {
                let (lhs, lhs_type) = self.term_to_constraint(spec, &t.lhs)?;
                let (rhs, rhs_type) = self.term_to_constraint(spec, &t.rhs)?;

                (
                    lhs_type.int_le(&lhs, rhs_type, &rhs),
                    spec.get_encoded_type_by_tref(&TypeRef::Named("int".to_string()))
                        .unwrap(),
                )
            },
            | Term::ValueEq(t) => {
                let (lhs, _) = self.term_to_constraint(spec, &t.lhs)?;
                let (rhs, _) = self.term_to_constraint(spec, &t.rhs)?;

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
                    .map(|term| {
                        self.term_to_constraint(
                            spec,
                            &term,
                            fs_encoding,
                            path_params,
                            base_dir_file_id,
                        )
                    })
                    .transpose()?
                    .unzip();
                let encoded_type = spec
                    .get_encoded_type_by_tref(&TypeRef::Named(t.ty.clone()))
                    .unwrap();

                (
                    encoded_type.const_variant(spec.ctx, &t.case, payload),
                    encoded_type,
                )
            },
            | Term::NoNonExistentDirBacktrack(t) => {
                let fd = self.variables.get_by_key(&t.fd_param).unwrap().clone();
                let path = self.variables.get_by_key(&t.path_param).unwrap().clone();
                let path_param_encoding = path_params.get(&t.path_param).unwrap();
                let file_option_type = spec
                    .get_encoded_type_by_tref(&TypeRef::Named("file--option".to_string()))
                    .unwrap();
                let file_option_type = OptionType::from(file_option_type);
                let mut clauses: Vec<z3::ast::Bool<'_>> = Default::default();
                let mut curr_component_file = file_option_type.fresh_const(&spec.ctx);

                // Start by fixing the fd to the base directory.
                clauses.push(z3::ast::Bool::and(
                    &spec.ctx,
                    &[
                        file_option_type.is_some(&curr_component_file),
                        fs_encoding.fd_maps_to_file(&fd, base_dir_file_id),
                    ],
                ));

                //         for segment in path_encoding.segments() {
                //             let next_component = ast::Dynamic::fresh_const(&ctx, "", &option_segment.sort());
                //             let some_component_idx = ast::Int::fresh_const(&ctx, "");

                //             solver.assert(
                //                 &ast::Bool::and(
                //                     &ctx,
                //                     &[
                //                         segment_type.is_component(segment),
                //                         path_encoding
                //                             .component_idx_mapping()
                //                             .apply(&[
                //                                 segment,
                //                                 &ast::Int::sub(
                //                                     &ctx,
                //                                     &[ast::Int::from_u64(&ctx, num_components as u64)],
                //                                 ),
                //                             ])
                //                             .as_bool()
                //                             .unwrap()
                //                             .not(),
                //                     ],
                //                 )
                //                 .ite(
                //                     &ast::Bool::and(
                //                         &ctx,
                //                         &[
                //                             option_segment.is_some(&next_component),
                //                             segment_type.is_component(&option_segment.inner(&next_component)),
                //                             ast::Bool::or(
                //                                 &ctx,
                //                                 path_encoding
                //                                     .segments()
                //                                     .map(|seg| seg._eq(&option_segment.inner(&next_component)))
                //                                     .collect::<Vec<_>>()
                //                                     .as_slice(),
                //                             ),
                //                         ],
                //                     ),
                //                     &option_segment.is_none(&next_component),
                //                 ),
                //             );
                //             solver.assert(
                //                 &ast::Bool::and(
                //                     &ctx,
                //                     &[
                //                         segment_type.is_component(segment),
                //                         segment_type.is_component(&option_segment.inner(&next_component)),
                //                         option_segment.is_some(&next_component),
                //                     ],
                //                 )
                //                 .implies(&exists_const(
                //                     &ctx,
                //                     &[&some_component_idx],
                //                     &[],
                //                     &ast::Bool::and(
                //                         &ctx,
                //                         &[
                //                             &path_encoding
                //                                 .component_idx_mapping()
                //                                 .apply(&[segment, &some_component_idx])
                //                                 .as_bool()
                //                                 .unwrap(),
                //                             &path_encoding
                //                                 .component_idx_mapping()
                //                                 .apply(&[
                //                                     &option_segment.inner(&next_component),
                //                                     &ast::Int::add(
                //                                         &ctx,
                //                                         &[&some_component_idx, &ast::Int::from_i64(&ctx, 1)],
                //                                     ),
                //                                 ])
                //                                 .as_bool()
                //                                 .unwrap(),
                //                         ],
                //                     ),
                //                 )),
                //             );
                //         }
                (
                    todo!(),
                    spec.get_encoded_type_by_tref(&TypeRef::Named("bool".to_string()))
                        .unwrap(),
                )
            },
        })
    }
    }
}
