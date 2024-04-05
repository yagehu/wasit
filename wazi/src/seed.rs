use eyre::{self, eyre as err, Context, ContextCompat};
use serde::{Deserialize, Serialize};
use wazzi_executor::RunningExecutor;
use wazzi_spec::package::{Defvaltype, Interface, Package, TypeidxBorrow};
use wazzi_store::RuntimeStore;
use wazzi_wasi_component_model::value::{
    FlagsMember,
    FlagsValue as WasiFlagsValue,
    RecordValue as WasiRecordValue,
    StringValue,
    Value as WasiValue,
    VariantValue as WasiVariantValue,
};

use crate::{prog::Prog, resource_ctx::ResourceContext};

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Seed {
    pub mount_base_dir: bool,
    pub actions:        Vec<Action>,
}

impl Seed {
    pub fn execute(
        self,
        spec: &Package,
        executor: RunningExecutor,
        store: RuntimeStore,
    ) -> Result<Prog, eyre::Error> {
        let base_dir_fd = executor.base_dir_fd();
        let mut prog = Prog::new(executor, store);
        let interface = spec
            .interface(TypeidxBorrow::Symbolic("wasi_snapshot_preview1"))
            .wrap_err("interface wasi_snapshot_preview1 not found")?;

        if self.mount_base_dir {
            prog.resource_ctx_mut()
                .register_resource("fd_base", WasiValue::Handle(base_dir_fd), 0);
        }

        for action in self.actions {
            match action {
                | Action::Decl(decl) => {
                    let resource_type = interface
                        .get_resource_type(TypeidxBorrow::Symbolic(&decl.resource_type))
                        .wrap_err(format!(
                            "resource {} not found in interface",
                            &decl.resource_type
                        ))?;
                    let value =
                        decl.value
                            .into_wasi_value(interface, prog.resource_ctx(), resource_type);

                    prog.resource_ctx_mut().register_resource(
                        &decl.resource_type,
                        value,
                        decl.resource_id,
                    );
                },
                | Action::Call(call) => {
                    let func_spec = interface
                        .function(&call.func)
                        .wrap_err(format!("func {} not found", call.func))?;
                    let result_valtypes = func_spec.unpack_expected_result();
                    let params = func_spec
                        .params
                        .iter()
                        .zip(call.params)
                        .map(|(param_type, rv)| -> Result<_, eyre::Error> {
                            let def = interface
                                .resolve_valtype(&param_type.valtype)
                                .ok_or(err!("failed to resolve valtype"))?;

                            Ok(rv.into_prog_value(interface, prog.resource_ctx(), &def))
                        })
                        .collect::<Result<Vec<_>, _>>()?;

                    prog.call(
                        interface,
                        &func_spec,
                        params,
                        result_valtypes
                            .iter()
                            .map(|valtype| interface.resolve_valtype(valtype).unwrap())
                            .map(|def| WasiValue::zero_value_from_spec(interface, &def))
                            .collect(),
                    )?;

                    let action = prog
                        .store_mut()
                        .trace_mut()
                        .last_action()
                        .wrap_err("failed to get last action")?
                        .wrap_err("trace is empty")?
                        .read()
                        .wrap_err("failed to read action from store")?;
                    let call_result = match action {
                        | wazzi_store::Action::Call(call) => call,
                        | wazzi_store::Action::Decl(_) => todo!(),
                    };

                    for ((result_valtype, result), result_spec) in result_valtypes
                        .iter()
                        .zip(call_result.results)
                        .zip(call.results)
                    {
                        let id = match result_spec {
                            | ResultSpec::Resource(id) => Some(id),
                            | ResultSpec::Ignore => None,
                        };

                        prog.resource_ctx_mut().register_resource_rec(
                            interface,
                            &result_valtype,
                            result,
                            id,
                        )?;
                    }
                },
            }
        }

        Ok(prog)
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Decl(Decl),
    Call(Call),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Decl {
    pub resource_id:   u64,
    pub resource_type: String,
    pub value:         Value,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub func: String,

    #[serde(default)]
    pub params: Vec<ResourceOrValue>,

    #[serde(default)]
    pub results: Vec<ResultSpec>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ResultSpec {
    Resource(u64),
    Ignore,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ResourceOrValue {
    Resource(u64),
    Value(Value),
}

impl ResourceOrValue {
    fn into_prog_value(
        self,
        interface: &Interface,
        resource_ctx: &ResourceContext,
        def: &Defvaltype,
    ) -> WasiValue {
        match self {
            | Self::Resource(id) => resource_ctx.get_resource(id).unwrap().value,
            | Self::Value(value) => value.into_wasi_value(interface, resource_ctx, def),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    S64(i64),
    U8(u8),
    U64(u64),

    Handle(u32),

    // Container types
    Record(RecordValue),
    Variant(VariantValue),
    List(Vec<ResourceOrValue>),

    // Specialized types
    Flags(Vec<FlagsMember>),
    String(StringValue),
}

impl Value {
    fn into_wasi_value(
        self,
        interface: &Interface,
        resource_ctx: &ResourceContext,
        ty: &Defvaltype,
    ) -> WasiValue {
        match (ty, self) {
            | (_, Value::S64(i)) => WasiValue::S64(i),
            | (_, Value::U8(i)) => WasiValue::U8(i),
            | (_, Value::U64(i)) => WasiValue::U64(i),
            | (_, Value::Handle(handle)) => WasiValue::Handle(handle),
            | (Defvaltype::Record(record_type), Value::Record(record)) => {
                WasiValue::Record(WasiRecordValue {
                    members: record_type
                        .members
                        .iter()
                        .zip(record.0)
                        .map(|(member_type, member)| {
                            let def = interface.resolve_valtype(&member_type.ty).unwrap();

                            member.value.into_prog_value(interface, resource_ctx, &def)
                        })
                        .collect(),
                })
            },
            | (Defvaltype::Variant(variant_type), Value::Variant(variant)) => {
                let (case_idx, case_type) = variant_type
                    .cases
                    .iter()
                    .enumerate()
                    .find(|(_i, case)| case.name == variant.name)
                    .unwrap();

                WasiValue::Variant(Box::new(WasiVariantValue {
                    case_idx:  case_idx as u32,
                    case_name: variant.name,
                    payload:   variant.payload.map(|payload| match *payload {
                        | ResourceOrValue::Resource(id) => {
                            resource_ctx.get_resource(id).unwrap().value
                        },
                        | ResourceOrValue::Value(value) => value.into_wasi_value(
                            interface,
                            resource_ctx,
                            &interface
                                .resolve_valtype(case_type.payload.as_ref().unwrap())
                                .unwrap(),
                        ),
                    }),
                }))
            },
            | (Defvaltype::List(list_type), Value::List(elements)) => {
                let mut list = Vec::with_capacity(elements.len());

                for element in elements {
                    let element = match element {
                        | ResourceOrValue::Resource(id) => {
                            resource_ctx.get_resource(id).unwrap().value
                        },
                        | ResourceOrValue::Value(value) => {
                            let item_def = interface.resolve_valtype(&list_type.element).unwrap();

                            value.into_wasi_value(interface, resource_ctx, &item_def)
                        },
                    };

                    list.push(element);
                }

                WasiValue::List(list)
            },
            | (_, Value::Flags(members)) => WasiValue::Flags(WasiFlagsValue {
                members: members
                    .into_iter()
                    .map(|member| FlagsMember {
                        name:  member.name,
                        value: member.value,
                    })
                    .collect(),
            }),
            | (_, Value::String(string)) => WasiValue::String(string),
            | (_, Value::Record(_)) | (_, Value::Variant(_)) | (_, Value::List(_)) => panic!(),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RecordValue(pub Vec<RecordMemberValue>);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RecordMemberValue {
    pub name:  String,
    pub value: ResourceOrValue,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ListValue(pub Vec<ResourceOrValue>);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct PointerValue {
    pub alloc_from_resource: u64,
    pub default_value:       Option<Box<Value>>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VariantValue {
    pub name:    String,
    pub payload: Option<Box<ResourceOrValue>>,
}
