pub mod resource;
pub mod solver;

use arbitrary::Unstructured;
use std::fmt;
use wazzi_specz_wasi::{Function, Interface};

use crate::{resource::Context, Environment};

pub trait FunctionPicker: fmt::Debug {
    fn pick_function<'i>(
        &self,
        u: &mut Unstructured,
        interface: &'i Interface,
        env: &Environment,
        ctx: &Context,
    ) -> Result<&'i Function, eyre::Error>;
}
