pub mod stateful;
pub mod stateless;

use std::fmt;

use arbitrary::Unstructured;

use crate::{
    preview1::spec::{Function, Spec},
    resource::Context,
    Environment,
    Value,
};

pub trait ParamsGenerator: fmt::Debug {
    fn generate_params(
        &self,
        u: &mut Unstructured,
        env: &Environment,
        ctx: &Context,
        spec: &Spec,
        function: &Function,
    ) -> Result<Vec<Value>, eyre::Error>;
}
