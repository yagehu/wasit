use std::{path::PathBuf, thread};

use eyre::Context;
use wazzi_dyn_spec::{Environment, ResourceContext};
use wazzi_runners::WasiRunner;
use wazzi_store::{FuzzStore, RunStore};

#[derive(Clone, Debug)]
pub struct Runtime<'r> {
    name:   String,
    runner: &'r dyn WasiRunner,
    ctx:    ResourceContext,
}

#[derive(Debug)]
pub struct Fuzzer<F> {
    new_env:  F,
    runtimes: Vec<(String, Box<dyn WasiRunner>)>,
    executor: PathBuf,
    store:    FuzzStore,
}

impl<F> Fuzzer<F>
where
    F: Fn() -> Environment + Send + Sync,
{
    pub fn new(
        new_env: F,
        runtimes: impl IntoIterator<Item = (String, Box<dyn WasiRunner>)>,
        executor: PathBuf,
        store: FuzzStore,
    ) -> Self {
        Self {
            new_env,
            runtimes: runtimes.into_iter().collect(),
            executor,
            store,
        }
    }

    pub fn fuzz(&mut self) -> Result<(), eyre::Error> {
        let scope = FuzzScope::new(self);

        Ok(())
    }
}

#[derive(Debug)]
pub struct FuzzScope<'f, F> {
    fuzzer: &'f Fuzzer<F>,
    env:    Environment,
    store:  RunStore,
}

impl<'f, F> FuzzScope<'f, F>
where
    F: Fn() -> Environment + Send + Sync,
{
    pub fn new(fuzzer: &'f mut Fuzzer<F>) -> Result<Self, eyre::Error> {
        let mut env = (fuzzer.new_env)();
        let store = fuzzer
            .store
            .new_run()
            .wrap_err("failed to init new run store")?;

        Ok(Self { fuzzer, env, store })
    }

    pub fn fuzz(&mut self) -> Result<(), eyre::Error> {
        thread::scope(|scope| -> Result<(), eyre::Error> {
            for (name, runtime) in &self.fuzzer.runtimes {
                let mut runtime_store = self
                    .store
                    .new_runtime(name.to_owned(), "")
                    .wrap_err("failed to init new runtime store")?;

                thread::Builder::new()
                    .name(name.clone())
                    .spawn_scoped(scope, {
                        let executor = self.fuzzer.executor.clone();
                        let working_dir = runtime_store.path.clone();

                        move || -> Result<(), eyre::Error> {
                            runtime
                                .run(executor, &working_dir, Some(runtime_store.base.clone()))
                                .wrap_err("failed to run executor")?;

                            Ok(())
                        }
                    });
            }

            Ok(())
        });

        Ok(())
    }
}
