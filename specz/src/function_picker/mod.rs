pub mod resource;
pub mod solver;

use std::fmt;

use arbitrary::Unstructured;

use crate::{
    preview1::spec::{Function, Interface, Spec},
    resource::Context,
    Environment,
};

pub trait FunctionPicker: fmt::Debug {
    fn pick_function<'i>(
        &self,
        u: &mut Unstructured,
        interface: &'i Interface,
        env: &Environment,
        ctx: &Context,
        spec: &Spec,
    ) -> Result<&'i Function, eyre::Error>;
}
