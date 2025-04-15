use std::{collections::HashSet, path::PathBuf};

use dyn_clone::{clone_trait_object, DynClone};
use wazzi_runners::{MappedDir, Node, Wamr, WasiRunner, Wasmedge, Wasmer, Wasmtime, Wazero};

use crate::{
    spec::{Spec, TypeRef, WasiValue},
    EnvironmentInitializer,
    RunningExecutor,
};

pub trait Runtime: InitializeState + WasiRunner + DynClone {
    fn name(&self) -> &str;
}

clone_trait_object!(Runtime);

impl Runtime for Node<'_> {
    fn name(&self) -> &str {
        self.name()
    }
}

impl Runtime for Wasmedge<'_> {
    fn name(&self) -> &str {
        self.name()
    }
}

impl Runtime for Wasmer<'_> {
    fn name(&self) -> &str {
        self.name()
    }
}

impl Runtime for Wasmtime<'_> {
    fn name(&self) -> &str {
        self.name()
    }
}

impl Runtime for Wamr<'_> {
    fn name(&self) -> &str {
        self.name()
    }
}

impl Runtime for Wazero<'_> {
    fn name(&self) -> &str {
        self.name()
    }
}

pub trait InitializeState {
    fn initialize_state(
        &self,
        name: String,
        spec: &Spec,
        executor: &RunningExecutor,
        mapped_dirs: Vec<MappedDir>,
    ) -> Result<EnvironmentInitializer, eyre::Error>;
}

fn initialize(
    name: String,
    spec: &Spec,
    executor: &RunningExecutor,
    mapped_dirs: Vec<MappedDir>,
) -> Result<EnvironmentInitializer, eyre::Error> {
    let mut fd: u32 = 3;
    let mut preopens: Vec<_> = Default::default();

    loop {
        let mut call = executor.call(pb::request::Call {
            func:           pb::WasiFunc::FD_PRESTAT_GET.into(),
            params:         vec![pb::Value {
                which:          Some(pb::value::Which::Handle(fd)),
                special_fields: Default::default(),
            }],
            results:        vec![spec
                .get_wasi_type("prestat")
                .unwrap()
                .zero_value(spec)
                .into_pb(spec, &TypeRef::Named("prestat".to_string()))],
            special_fields: Default::default(),
        })?;

        if call.errno_some() != 0 {
            break;
        }

        let pr_name_len = WasiValue::from_pb(
            call.results.pop().unwrap(),
            spec,
            spec.types.get_by_key("prestat").unwrap(),
        )
        .variant()
        .unwrap()
        .payload
        .as_ref()
        .unwrap()
        .record()
        .unwrap()
        .members
        .first()
        .unwrap()
        .r#u32()
        .unwrap();
        let call = executor.call(pb::request::Call {
            func:           pb::WasiFunc::FD_PRESTAT_DIR_NAME.into(),
            params:         vec![
                pb::Value {
                    which:          Some(pb::value::Which::Handle(fd)),
                    special_fields: Default::default(),
                },
                pb::Value {
                    which:          Some(pb::value::Which::Array(pb::value::Array {
                        items:          vec![
                            pb::Value {
                                which:          Some(pb::value::Which::Builtin(pb::value::Builtin {
                                    which:          Some(pb::value::builtin::Which::U8(0)),
                                    special_fields: Default::default(),
                                })),
                                special_fields: Default::default(),
                            };
                            pr_name_len as usize
                        ],
                        item_size:      1,
                        special_fields: Default::default(),
                    })),
                    special_fields: Default::default(),
                },
                pb::Value {
                    which:          Some(pb::value::Which::Builtin(pb::value::Builtin {
                        which:          Some(pb::value::builtin::Which::U32(pr_name_len)),
                        special_fields: Default::default(),
                    })),
                    special_fields: Default::default(),
                },
            ],
            results:        vec![],
            special_fields: Default::default(),
        })?;

        assert_eq!(call.errno_some(), 0);

        let full_dir_name = String::from_utf8(
            WasiValue::from_pb(call.params[1].clone(), spec, spec.types.get_by_key("path").unwrap())
                .string()
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        let full_dir_name = PathBuf::from(full_dir_name);

        let dir_name = full_dir_name.file_name().unwrap().to_string_lossy().to_string();
        let dir = mapped_dirs.iter().find(|dir| dir.name == dir_name).unwrap();

        preopens.push((dir_name, dir.host_path.clone(), WasiValue::Handle(fd)));
        fd += 1;
    }

    Ok(EnvironmentInitializer { name, preopens })
}

impl InitializeState for Node<'_> {
    fn initialize_state(
        &self,
        name: String,
        spec: &Spec,
        executor: &RunningExecutor,
        mapped_dirs: Vec<MappedDir>,
    ) -> Result<EnvironmentInitializer, eyre::Error> {
        initialize(name, spec, executor, mapped_dirs)
    }
}

impl InitializeState for Wamr<'_> {
    fn initialize_state(
        &self,
        name: String,
        spec: &Spec,
        executor: &RunningExecutor,
        mapped_dirs: Vec<MappedDir>,
    ) -> Result<EnvironmentInitializer, eyre::Error> {
        initialize(name, spec, executor, mapped_dirs)
    }
}

impl InitializeState for Wasmedge<'_> {
    fn initialize_state(
        &self,
        name: String,
        spec: &Spec,
        executor: &RunningExecutor,
        mapped_dirs: Vec<MappedDir>,
    ) -> Result<EnvironmentInitializer, eyre::Error> {
        initialize(name, spec, executor, mapped_dirs)
    }
}

impl InitializeState for Wasmer<'_> {
    fn initialize_state(
        &self,
        name: String,
        spec: &Spec,
        executor: &RunningExecutor,
        mapped_dirs: Vec<MappedDir>,
    ) -> Result<EnvironmentInitializer, eyre::Error> {
        const VIRTUAL_ROOT_FD: u32 = 5;

        let mut preopens: Vec<_> = Default::default();
        let rights_base = spec
            .get_wasi_type("rights")
            .unwrap()
            .flags()
            .unwrap()
            .value(
                [
                    "fd_read",
                    "fd_seek",
                    "fd_tell",
                    "fd_fdstat_set_flags",
                    "fd_write",
                    "path_create_directory",
                    "path_create_file",
                    "path_link_target",
                    "path_open",
                    "path_symlink",
                    "path_remove_directory",
                    "path_unlink_file",
                ]
                .into_iter()
                .collect(),
            )
            .into_pb(spec, &TypeRef::Named("rights".to_string()));
        let rights_inheriting = spec
            .get_wasi_type("rights")
            .unwrap()
            .flags()
            .unwrap()
            .value(
                [
                    "fd_read",
                    "fd_seek",
                    "fd_tell",
                    "fd_fdstat_set_flags",
                    "fd_write",
                    "path_create_directory",
                    "path_create_file",
                    "path_link_target",
                    "path_open",
                    "path_symlink",
                    "path_remove_directory",
                    "path_unlink_file",
                ]
                .into_iter()
                .collect(),
            )
            .into_pb(spec, &TypeRef::Named("rights".to_string()));

        for dir in mapped_dirs {
            let call = executor.call(pb::request::Call {
                func:           pb::WasiFunc::PATH_OPEN.into(),
                params:         vec![
                    pb::Value {
                        which:          Some(pb::value::Which::Handle(VIRTUAL_ROOT_FD)),
                        special_fields: Default::default(),
                    },
                    spec.get_wasi_type("lookupflags")
                        .unwrap()
                        .flags()
                        .unwrap()
                        .value(HashSet::new())
                        .into_pb(spec, &TypeRef::Named("lookupflags".to_string())),
                    pb::Value {
                        which:          Some(pb::value::Which::String(dir.name.clone().into_bytes())),
                        special_fields: Default::default(),
                    },
                    spec.get_wasi_type("oflags")
                        .unwrap()
                        .flags()
                        .unwrap()
                        .value(["directory"].into_iter().collect())
                        .into_pb(spec, &TypeRef::Named("oflags".to_string())),
                    rights_base.clone(),
                    rights_inheriting.clone(),
                    spec.get_wasi_type("fdflags")
                        .unwrap()
                        .flags()
                        .unwrap()
                        .value(HashSet::new())
                        .into_pb(spec, &TypeRef::Named("fdflags".to_string())),
                ],
                results:        vec![pb::Value {
                    which:          Some(pb::value::Which::Handle(0)),
                    special_fields: Default::default(),
                }],
                special_fields: Default::default(),
            })?;

            match call.errno_option.unwrap() {
                | pb::response::call::Errno_option::ErrnoSome(errno) => {
                    if errno != 0 {
                        panic!("initialization call failed with errno {errno} {}", dir.name);
                    }
                },
                | pb::response::call::Errno_option::ErrnoNone(_) => (),
                | _ => todo!(),
            }

            preopens.push((
                dir.name.clone(),
                dir.host_path.clone(),
                WasiValue::from_pb(call.results[0].clone(), spec, spec.types.get_by_key("fd").unwrap()),
            ));
        }

        Ok(EnvironmentInitializer { name, preopens })
    }
}

impl InitializeState for Wasmtime<'_> {
    fn initialize_state(
        &self,
        name: String,
        spec: &Spec,
        executor: &RunningExecutor,
        mapped_dirs: Vec<MappedDir>,
    ) -> Result<EnvironmentInitializer, eyre::Error> {
        initialize(name, spec, executor, mapped_dirs)
    }
}

impl InitializeState for Wazero<'_> {
    fn initialize_state(
        &self,
        name: String,
        spec: &Spec,
        executor: &RunningExecutor,
        mapped_dirs: Vec<MappedDir>,
    ) -> Result<EnvironmentInitializer, eyre::Error> {
        initialize(name, spec, executor, mapped_dirs)
    }
}
