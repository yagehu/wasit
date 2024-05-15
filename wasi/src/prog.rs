use arbitrary::Unstructured;
use eyre::{Context, ContextCompat};
use wazzi_executor::RunningExecutor;
use wazzi_spec::package::{Function, Interface, Package};
use wazzi_store::{Call, RuntimeStore};
use wazzi_wasi_component_model::value::ValueMeta;

use crate::{resource_ctx::ResourceContext, seed::ResultSpec};

#[derive(Debug)]
pub struct Prog {
    executor:     RunningExecutor,
    resource_ctx: ResourceContext,
    store:        RuntimeStore,
}

impl Prog {
    pub fn new(executor: RunningExecutor, store: RuntimeStore) -> Self {
        Self {
            executor,
            resource_ctx: ResourceContext::new(),
            store,
        }
    }

    pub fn executor(&self) -> &RunningExecutor {
        &self.executor
    }

    pub fn resource_ctx_mut(&mut self) -> &mut ResourceContext {
        &mut self.resource_ctx
    }

    pub fn resource_ctx(&mut self) -> &ResourceContext {
        &self.resource_ctx
    }

    pub fn store(&self) -> &RuntimeStore {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut RuntimeStore {
        &mut self.store
    }

    pub fn call(
        &mut self,
        interface: &Interface,
        func: &Function,
        params: Vec<ValueMeta>,
        results: Vec<ValueMeta>,
        result_specs: Option<&[ResultSpec]>,
    ) -> Result<(), eyre::Error> {
        tracing::debug!(
            count = self.store.trace().count(),
            func = func.name,
            "Calling func."
        );

        self.store
            .trace_mut()
            .begin_call(Call {
                func:    func.name.clone(),
                errno:   None,
                params:  params.clone(),
                results: results.clone(),
            })
            .wrap_err("failed to begin recording call")?;

        let result_valtypes = func.unpack_expected_result();
        let response = self.executor.call(executor_pb::request::Call {
            func:           pb_func(func.name.as_str()).into(),
            params:         func
                .params
                .iter()
                .zip(params.clone())
                .map(|(param, v)| -> Result<_, eyre::Error> {
                    let def = interface
                        .resolve_valtype(&param.valtype)
                        .wrap_err("failed to resolve valtype")?;

                    Ok(v.into_pb(interface, &def))
                })
                .collect::<Result<_, _>>()?,
            results:        result_valtypes
                .iter()
                .zip(results.clone())
                .map(|(result_valtype, v)| -> Result<_, eyre::Error> {
                    let def = interface
                        .resolve_valtype(&result_valtype)
                        .wrap_err("failed to resolve valtype")?;

                    Ok(v.into_pb(interface, &def))
                })
                .collect::<Result<_, _>>()?,
            special_fields: Default::default(),
        })?;
        let errno = match response.errno_option.unwrap() {
            | executor_pb::response::call::Errno_option::ErrnoSome(i) => Some(i),
            | executor_pb::response::call::Errno_option::ErrnoNone(_) => None,
            | _ => panic!(),
        };
        let mut results_after = response
            .results
            .iter()
            .zip(result_valtypes.iter())
            .zip(results.iter())
            .map(|((result, result_valtype), before)| {
                ValueMeta::from_pb(result.to_owned(), interface, result_valtype, before)
            })
            .collect::<Vec<_>>();

        if errno.is_none() || errno.unwrap() == 0 {
            for (i, (result, valtype)) in results_after
                .iter_mut()
                .zip(result_valtypes.iter())
                .enumerate()
            {
                let id = match result_specs {
                    | Some(result_specs) => match result_specs.get(i).unwrap() {
                        | ResultSpec::Resource(id) => Some(*id),
                        | ResultSpec::Ignore => continue,
                    },
                    | None => None,
                };

                self.resource_ctx
                    .register_resource_rec(interface, valtype, result, id)?;
            }
        }

        self.store
            .trace_mut()
            .end_call(Call {
                func: func.name.clone(),
                errno,
                params: func
                    .params
                    .iter()
                    .zip(response.params)
                    .zip(params)
                    .map(|((param_spec, param), before_param)| {
                        ValueMeta::from_pb(param, interface, &param_spec.valtype, &before_param)
                    })
                    .collect(),
                results: results_after,
            })
            .wrap_err("failed to end call")?;

        Ok(())
    }

    pub fn call_arbitrary(
        &mut self,
        u: &mut Unstructured,
        spec: &Package,
    ) -> Result<(), eyre::Error> {
        let interface = u.choose(spec.interfaces())?;
        let functions = interface.functions().collect::<Vec<_>>();
        let function = *u.choose(&functions)?;
        let result_valtypes = function.unpack_expected_result();
        let mut params = Vec::with_capacity(function.params.len());
        let mut results = Vec::with_capacity(result_valtypes.len());
        let cset = match &function.spec {
            | Some(prog) => wazzi_spec_constraint::evaluate(prog)
                .wrap_err("failed to evaluate function constraints")?,
            | None => wazzi_spec_constraint::program::ConstraintSet::new(),
        };

        for param_type in &function.params {
            let param = self
                .resource_ctx
                .arbitrary_value_from_valtype(
                    u,
                    interface,
                    &param_type.valtype,
                    &cset,
                    Some(wazzi_spec_constraint::program::TypeRef::Param {
                        name: param_type.name.clone(),
                    }),
                )
                .wrap_err(format!(
                    "failed to get arbitrary param value: {} {}",
                    function.name, param_type.name,
                ))?;

            params.push(param);
        }

        for result_valtype in &result_valtypes {
            results.push(ValueMeta::zero_value_from_spec(interface, &result_valtype));
        }

        self.call(interface, function, params, results, None)
    }
}
