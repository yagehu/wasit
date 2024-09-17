use arbitrary::Unstructured;
use idxspace::IndexSpace;

use super::CallStrategy;
use crate::{
    spec::{Function, Spec, WasiValue},
    Environment,
    RuntimeContext,
};

pub struct StatefulStrategy<'u, 'data, 'env, 'ctx> {
    u:   &'u mut Unstructured<'data>,
    env: &'env Environment,
    ctx: &'ctx RuntimeContext,
}

impl<'u, 'data, 'env, 'ctx> StatefulStrategy<'u, 'data, 'env, 'ctx> {
    pub fn new(
        u: &'u mut Unstructured<'data>,
        env: &'env Environment,
        ctx: &'ctx RuntimeContext,
    ) -> Self {
        Self { u, env, ctx }
    }
}

impl CallStrategy for StatefulStrategy<'_, '_, '_, '_> {
    fn select_function<'spec>(
        &mut self,
        spec: &'spec Spec,
    ) -> Result<&'spec Function, eyre::Error> {
        todo!()
    }

    fn prepare_arguments(
        &mut self,
        spec: &Spec,
        function: &Function,
    ) -> Result<Vec<WasiValue>, eyre::Error> {
        todo!()
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct State {
    preopens: IndexSpace<usize, PreopenFs>,
}

impl State {
}

#[derive(PartialEq, Eq, Clone, Debug)]
struct PreopenFs {}
