use arbitrary::Unstructured;
use serde::{Deserialize, Serialize};

use wazzi_witx::InterfaceFunc;

use crate::{
    pb,
    prog::Prog,
    snapshot::{PureValue, ValueView},
};

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Decl(Decl),
    Call(Call),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Decl {
    pub resource: u64,
    pub r#type:   String,
    pub value:    RawValue,
}

impl Decl {
    pub fn to_pb(&self, spec: &witx::Document) -> executor_pb::request::Decl {
        let ty = spec.typename(&witx::Id::new(&self.r#type)).unwrap();

        executor_pb::request::Decl {
            resource_id:    self.resource,
            value:          Some(self.value.to_pb(ty.type_().as_ref())).into(),
            type_:          Some(pb::to_type(ty.type_().as_ref())).into(),
            special_fields: Default::default(),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub func: String,

    #[serde(default)]
    pub params: Vec<Value>,

    #[serde(default)]
    pub results: Vec<CallResultSpec>,
}

impl Call {
    pub fn arbitrary(
        u: &mut Unstructured,
        prog: &Prog,
        spec: &witx::Document,
    ) -> Result<Self, arbitrary::Error> {
        let module_spec = spec
            .module(&witx::Id::new("wasi_snapshot_preview1"))
            .unwrap();
        let func_specs = module_spec.funcs().collect::<Vec<_>>();
        let func_spec = u.choose(&func_specs)?;

        Self::arbitrary_from_func_spec(u, prog, func_spec)
    }

    pub fn arbitrary_from_func_spec(
        _u: &mut Unstructured,
        _prog: &Prog,
        spec: &InterfaceFunc,
    ) -> Result<Self, arbitrary::Error> {
        Ok(Self {
            func:    spec.name.as_str().to_owned(),
            params:  vec![],
            results: vec![],
        })
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum CallResultSpec {
    Ignore,
    Resource(u64),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    Resource(u64),
    RawValue(RawValue),
}

impl Value {
    fn from_value_view(x: &ValueView) -> Self {
        Self::RawValue(RawValue::from_pure_value(&x.value))
    }

    fn to_pb_value_spec(&self, ty: &witx::Type) -> executor_pb::ValueSpec {
        let which = match self {
            | &Value::Resource(id) => {
                executor_pb::value_spec::Which::Resource(executor_pb::Resource {
                    id,
                    special_fields: Default::default(),
                })
            },
            | Value::RawValue(raw_value) => {
                executor_pb::value_spec::Which::RawValue(Box::new(raw_value.to_pb(ty)))
            },
        };

        executor_pb::ValueSpec {
            type_:          Some(pb::to_type(ty)).into(),
            which:          Some(which),
            special_fields: Default::default(),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum RawValue {
    Builtin(BuiltinValue),
    String(StringValue),
    Bitflags(BitflagsValue),
    Handle(u32),
    Array(ArrayValue),
    Record(RecordValue),
    ConstPointer(ConstPointerValue),
    Pointer(PointerValue),
    Variant(VariantValue),
}

impl RawValue {
    pub fn from_pure_value(v: &PureValue) -> Self {
        match v {
            | PureValue::Builtin(builtin) => Self::Builtin(match builtin {
                | &BuiltinValue::U8(i) => BuiltinValue::U8(i),
                | &BuiltinValue::U32(i) => BuiltinValue::U32(i),
                | &BuiltinValue::U64(i) => BuiltinValue::U64(i),
                | &BuiltinValue::S64(i) => BuiltinValue::S64(i),
            }),
            | &PureValue::Handle(handle) => Self::Handle(handle),
            | PureValue::List(_) => todo!(),
            | PureValue::Record(members) => Self::Record(RecordValue(
                members
                    .iter()
                    .map(|m| RecordMemberValue {
                        name:  m.name.clone(),
                        value: Value::from_value_view(&m.view),
                    })
                    .collect(),
            )),
            | PureValue::Pointer(_) => todo!(),
            | PureValue::Variant(_) => todo!(),
        }
    }

    pub fn to_pb(&self, ty: &witx::Type) -> executor_pb::RawValue {
        let which = Some(match (self, ty) {
            | (RawValue::Builtin(builtin), witx::Type::Builtin(_)) => {
                let which = Some(match builtin {
                    | &BuiltinValue::U8(i) => executor_pb::raw_value::builtin::Which::U8(i as u32),
                    | &BuiltinValue::U32(i) => executor_pb::raw_value::builtin::Which::U32(i),
                    | &BuiltinValue::U64(i) => executor_pb::raw_value::builtin::Which::U64(i),
                    | &BuiltinValue::S64(i) => executor_pb::raw_value::builtin::Which::S64(i),
                });

                executor_pb::raw_value::Which::Builtin(executor_pb::raw_value::Builtin {
                    which,
                    special_fields: Default::default(),
                })
            },
            | (RawValue::String(_), witx::Type::List(_)) => todo!(),
            | (RawValue::Bitflags(_), witx::Type::Record(record))
                if matches!(record.kind, witx::RecordKind::Bitflags(_)) =>
            {
                todo!()
            },
            | (RawValue::Handle(_), witx::Type::Handle(_)) => todo!(),
            | (RawValue::Array(_), witx::Type::List(_)) => todo!(),
            | (RawValue::Record(record), witx::Type::Record(record_type)) => {
                executor_pb::raw_value::Which::Record(executor_pb::raw_value::Record {
                    members:        record
                        .0
                        .iter()
                        .zip(record_type.members.iter())
                        .map(
                            |(member, member_type)| executor_pb::raw_value::record::Member {
                                name:           member.name.as_bytes().to_vec(),
                                value:          Some(
                                    member
                                        .value
                                        .to_pb_value_spec(member_type.tref.type_().as_ref()),
                                )
                                .into(),
                                special_fields: Default::default(),
                            },
                        )
                        .collect(),
                    special_fields: Default::default(),
                })
            },
            | (RawValue::ConstPointer(_), witx::Type::ConstPointer(_)) => todo!(),
            | (RawValue::Pointer(_), witx::Type::Pointer(_)) => todo!(),
            | (RawValue::Variant(_), witx::Type::Variant(_)) => todo!(),
            | _ => unimplemented!(),
        });

        executor_pb::RawValue {
            which,
            special_fields: Default::default(),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinValue {
    U8(u8),
    U32(u32),
    U64(u64),
    S64(i64),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StringValue {
    Utf8(String),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct BitflagsValue {
    pub repr:    BitflagsRepr,
    pub members: Vec<BitflagsMember>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct BitflagsMember {
    pub name:  String,
    pub value: bool,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum BitflagsRepr {
    U8,
    U16,
    U32,
    U64,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
pub struct ArrayValue(pub Vec<Value>);

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
pub struct RecordValue(pub Vec<RecordMemberValue>);

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct RecordMemberValue {
    pub name:  String,
    pub value: Value,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
pub struct ConstPointerValue(pub Vec<Value>);

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PointerValue {
    Alloc(PointerAlloc),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum PointerAlloc {
    Resource(u64),
    Value(u32),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct VariantValue {
    pub name:    String,
    pub payload: Option<Box<Value>>,
}
