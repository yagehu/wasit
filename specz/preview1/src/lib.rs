pub mod term;
pub mod witx;

pub use term::Term;

use std::collections::{BTreeMap, HashMap};

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Spec {
    types:      HashMap<String, WazziType>,
    interfaces: HashMap<String, Interface>,
}

impl Spec {
    pub fn new() -> Self {
        Self {
            types:      Default::default(),
            interfaces: Default::default(),
        }
    }

    pub fn preview1() -> Result<Self, eyre::Error> {
        let mut spec = Self::new();

        witx::preview1(&mut spec)?;

        Ok(spec)
    }
}

impl Default for Spec {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct WazziType {
    wasi: WasiType,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum WasiType {
    S64,
    U8,
    U16,
    U32,
    U64,
    Handle,
    Flags(FlagsType),
    Variant(VariantType),
    Record(RecordType),
    String,
    List(Box<ListType>),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FlagsType {
    pub repr:   IntRepr,
    pub fields: Vec<String>,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum IntRepr {
    U8,
    U16,
    U32,
    U64,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct VariantType {
    pub tag_repr: IntRepr,
    pub cases:    Vec<VariantCaseType>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct VariantCaseType {
    pub name:    String,
    pub payload: Option<WazziType>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RecordType {
    pub members: Vec<RecordMemberType>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct RecordMemberType {
    pub name: String,
    pub ty:   WazziType,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ListType {
    pub item: WazziType,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Interface {
    pub functions: BTreeMap<String, Function>,
}

impl Interface {
    pub fn new() -> Self {
        Self {
            functions: Default::default(),
        }
    }
}

impl Default for Interface {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Function {
    pub name:           String,
    pub params:         Vec<FunctionParam>,
    pub results:        Vec<FunctionResult>,
    pub r#return:       Option<()>,
    pub input_contract: Option<Term>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FunctionParam {
    pub name: String,
    pub ty:   WazziType,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FunctionResult {
    pub name: String,
    pub ty:   WazziType,
}

#[cfg(test)]
mod tests {
    use std::io;

    use eyre::Context as _;
    use tracing::level_filters::LevelFilter;
    use tracing_error::ErrorLayer;
    use tracing_subscriber::{layer::SubscriberExt as _, EnvFilter};

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

        let _spec = Spec::preview1().unwrap();

        Ok(())
    }
}
