use std::{collections::HashMap, path::PathBuf};

use crate::preview1::spec::WasiValue;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Context {
    pub resources:          HashMap<usize, (WasiValue, Option<Vec<u8>>)>,
    pub base_dir_host_path: PathBuf,
}

impl Context {
    pub fn new(base_dir_host_path: PathBuf) -> Self {
        Self {
            resources: Default::default(),
            base_dir_host_path,
        }
    }
}
