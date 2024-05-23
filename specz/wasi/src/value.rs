#[derive(PartialEq, Eq, Clone, Debug)]
pub enum WasiValue {
    Handle(u32),
    S64(i64),
    U64(u64),
    Flags(FlagsValue),
    Variant(Box<VariantValue>),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FlagsValue {
    pub fields: Vec<bool>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct VariantValue {
    pub case_idx: usize,
    pub payload:  Option<WasiValue>,
}
