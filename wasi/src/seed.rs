use color_eyre::eyre::{self, Context, ContextCompat};
use serde::{Deserialize, Serialize};
use wazzi_executor::RunningExecutor;

use crate::{
    prog::{self, Prog},
    resource_ctx::ResourceContext,
    store::ExecutionStore,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Seed {
    pub mount_base_dir: bool,
    pub actions:        Vec<Action>,
}

impl Seed {
    pub fn execute(
        self,
        spec: &witx::Document,
        store: ExecutionStore,
        executor: RunningExecutor,
    ) -> Result<Prog, eyre::Error> {
        let base_dir_fd = executor.base_dir_fd();
        let mut prog = Prog::new(executor, store).wrap_err("failed to init prog")?;
        let module_spec = spec
            .module(&witx::Id::new("wasi_snapshot_preview1"))
            .unwrap();

        if self.mount_base_dir {
            prog.resource_ctx_mut().register_resource(
                "fd_base",
                prog::Value::Handle(base_dir_fd),
                0,
            );
        }

        for action in self.actions {
            match action {
                | Action::Decl(decl) => {
                    let resource = spec
                        .resource(&witx::Id::new(&decl.resource_type))
                        .wrap_err("resource not found in spec")?;
                    let value = decl
                        .value
                        .into_prog_value(resource.tref.type_().as_ref(), prog.resource_ctx());

                    prog.resource_ctx_mut().register_resource(
                        &decl.resource_type,
                        value,
                        decl.resource_id,
                    );
                },
                | Action::Call(call) => {
                    let func_spec = module_spec
                        .func(&witx::Id::new(&call.func))
                        .wrap_err("func not found")?;
                    let result_trefs = func_spec.unpack_expected_result();
                    let params = func_spec
                        .params
                        .iter()
                        .zip(call.params)
                        .map(|(param_type, rv)| {
                            rv.into_prog_value(
                                param_type.tref.type_().as_ref(),
                                prog.resource_ctx(),
                            )
                        })
                        .collect();

                    prog.call(
                        &func_spec,
                        params,
                        result_trefs
                            .iter()
                            .map(prog::Value::zero_value_from_spec)
                            .collect(),
                    )?;

                    let call_result = prog.store().recorder().last()?.unwrap().read_result()?;

                    for ((result_tref, result), result_spec) in result_trefs
                        .iter()
                        .zip(call_result.results)
                        .zip(call.results)
                    {
                        if let ResultSpec::Resource(id) = result_spec {
                            prog::register_resource_rec(
                                prog.resource_ctx_mut(),
                                spec,
                                result_tref,
                                result,
                                Some(id),
                            )
                        }
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
    fn into_prog_value(self, ty: &witx::Type, resource_ctx: &ResourceContext) -> prog::Value {
        match self {
            | Self::Resource(id) => resource_ctx.get_resource(id).unwrap().value,
            | Self::Value(value) => value.into_prog_value(ty, resource_ctx),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    Builtin(BuiltinValue),
    String(StringValue),
    Bitflags(BitflagsValue),
    Record(RecordValue),
    List(ListValue),
    ConstPointer(ListValue),
    Pointer(PointerValue),
    Variant(VariantValue),
}

impl Value {
    fn into_prog_value(self, ty: &witx::Type, resource_ctx: &ResourceContext) -> prog::Value {
        match (ty, self) {
            | (_, Value::Builtin(builtin)) => prog::Value::Builtin(builtin),
            | (_, Value::String(string)) => prog::Value::String(string),
            | (witx::Type::Record(record_type), Value::Bitflags(bitflags))
                if record_type.bitflags_repr().is_some() =>
            {
                prog::Value::Bitflags(bitflags)
            },
            | (witx::Type::Record(record_type), Value::Record(record)) => {
                prog::Value::Record(prog::RecordValue {
                    members: record_type
                        .members
                        .iter()
                        .zip(record.0)
                        .map(|(member_type, member)| prog::RecordMemberValue {
                            name:  member.name,
                            value: match member.value {
                                | ResourceOrValue::Resource(id) => {
                                    resource_ctx.get_resource(id).unwrap().value
                                },
                                | ResourceOrValue::Value(value) => value.into_prog_value(
                                    member_type.tref.type_().as_ref(),
                                    resource_ctx,
                                ),
                            },
                        })
                        .collect(),
                })
            },
            | (witx::Type::List(item_tref), Value::List(list)) => prog::Value::List(
                list.0
                    .into_iter()
                    .map(|item| item.into_prog_value(item_tref.type_().as_ref(), resource_ctx))
                    .collect(),
            ),
            | (witx::Type::ConstPointer(tref), Value::ConstPointer(list)) => {
                prog::Value::ConstPointer(
                    list.0
                        .into_iter()
                        .map(|item| item.into_prog_value(tref.type_().as_ref(), resource_ctx))
                        .collect(),
                )
            },
            | (witx::Type::Pointer(tref), Value::Pointer(pointer)) => {
                let value = match pointer.default_value {
                    | Some(value) => value.into_prog_value(ty, resource_ctx),
                    | None => prog::Value::zero_value_from_spec(tref),
                };
                let resource = resource_ctx
                    .get_resource(pointer.alloc_from_resource)
                    .unwrap();
                let len = match resource.value {
                    | prog::Value::Builtin(BuiltinValue::U32(i)) => i,
                    | _ => panic!(),
                };

                prog::Value::Pointer(vec![value; len as usize])
            },
            | (witx::Type::Variant(variant_type), Value::Variant(variant)) => {
                let (case_idx, case) = variant_type
                    .cases
                    .iter()
                    .enumerate()
                    .find(|(_i, case)| case.name.as_str() == variant.name)
                    .unwrap();

                prog::Value::Variant(prog::VariantValue {
                    idx:     case_idx as u64,
                    name:    variant.name,
                    payload: case
                        .tref
                        .as_ref()
                        .zip(variant.payload)
                        .map(|(tref, payload)| {
                            Box::new(payload.into_prog_value(tref.type_().as_ref(), resource_ctx))
                        }),
                })
            },
            | (_, Value::Bitflags(_))
            | (_, Value::Record(_))
            | (_, Value::List(_))
            | (_, Value::Pointer(_))
            | (_, Value::ConstPointer(_))
            | (_, Value::Variant(_)) => panic!(),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinValue {
    U8(u8),
    U32(u32),
    U64(u64),
    S64(i64),
}

impl From<BuiltinValue> for executor_pb::value::Builtin {
    fn from(x: BuiltinValue) -> Self {
        let which = match x {
            | BuiltinValue::U8(i) => executor_pb::value::builtin::Which::U8(i.into()),
            | BuiltinValue::U32(i) => executor_pb::value::builtin::Which::U32(i),
            | BuiltinValue::U64(i) => executor_pb::value::builtin::Which::U64(i),
            | BuiltinValue::S64(i) => executor_pb::value::builtin::Which::S64(i),
        };

        Self {
            which:          Some(which),
            special_fields: Default::default(),
        }
    }
}

impl From<executor_pb::value::Builtin> for BuiltinValue {
    fn from(x: executor_pb::value::Builtin) -> Self {
        match x.which.unwrap() {
            | executor_pb::value::builtin::Which::U8(i) => Self::U8(i as u8),
            | executor_pb::value::builtin::Which::U32(i) => Self::U32(i),
            | executor_pb::value::builtin::Which::U64(i) => Self::U64(i),
            | executor_pb::value::builtin::Which::S64(i) => Self::S64(i),
            | _ => panic!(),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum StringValue {
    Utf8(String),
    Bytes(Vec<u8>),
}

impl From<StringValue> for Vec<u8> {
    fn from(x: StringValue) -> Self {
        match x {
            | StringValue::Utf8(string) => string.into_bytes(),
            | StringValue::Bytes(bytes) => bytes,
        }
    }
}

impl From<Vec<u8>> for StringValue {
    fn from(x: Vec<u8>) -> Self {
        match String::from_utf8(x) {
            | Ok(s) => Self::Utf8(s),
            | Err(err) => Self::Bytes(err.into_bytes()),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct BitflagsValue(pub Vec<BitflagsMemberValue>);

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct BitflagsMemberValue {
    pub name:  String,
    pub value: bool,
}

impl From<executor_pb::value::bitflags::Member> for BitflagsMemberValue {
    fn from(x: executor_pb::value::bitflags::Member) -> Self {
        Self {
            name:  x.name,
            value: x.value,
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
