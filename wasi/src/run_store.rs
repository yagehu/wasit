use std::{
    collections::BTreeMap,
    convert::Infallible,
    fs,
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};

use crate::resource_ctx::{Resource, ResourceContext};

pub trait RunStore {
    type Error;

    fn finish_run(self, resource_ctx: &ResourceContext) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone)]
pub struct MemRunStore {
    resource_ctx: Arc<Mutex<Option<ResourceContext>>>,
}

impl RunStore for MemRunStore {
    type Error = Infallible;

    fn finish_run(self, resource_ctx: &ResourceContext) -> Result<(), Self::Error> {
        *self.resource_ctx.lock().unwrap() = Some(resource_ctx.to_owned());

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct FsRunStore {
    root: PathBuf,
}

impl FsRunStore {
    pub fn new(path: PathBuf) -> Result<Self, io::Error> {
        Ok(Self {
            root: path.canonicalize()?,
        })
    }

    fn resource_ctx_path(&self) -> PathBuf {
        self.root.join("resource_ctx.json")
    }
}

impl RunStore for FsRunStore {
    type Error = io::Error;

    fn finish_run(self, resource_ctx: &ResourceContext) -> Result<(), Self::Error> {
        serde_json::to_writer_pretty(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.resource_ctx_path())?,
            &Resources::from_ctx(resource_ctx),
        )?;

        Ok(())
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
struct Resources(BTreeMap<u64, Resource>);

impl Resources {
    fn from_ctx(ctx: &ResourceContext) -> Self {
        let mut map = BTreeMap::new();
        let mut id = 0;

        while let Some(resource) = ctx.get_resource(id) {
            map.insert(id, resource);
            id += 1;
        }

        Self(map)
    }
}
