use std::collections::HashSet;

use wazzi_runners::{MappedDir, WasiRunner, Wasmer};

use crate::{
    spec::{Spec, TypeDef, TypeRef, WasiValue},
    EnvironmentInitializer,
    RunningExecutor,
    RuntimeContext,
};

pub trait InitializeState: WasiRunner {
    fn initialize_state(
        &self,
        spec: &Spec,
        executor: &RunningExecutor,
        mapped_dirs: Vec<MappedDir>,
    ) -> Result<EnvironmentInitializer, eyre::Error>;
}

impl InitializeState for Wasmer<'_> {
    fn initialize_state(
        &self,
        spec: &Spec,
        executor: &RunningExecutor,
        mapped_dirs: Vec<MappedDir>,
    ) -> Result<EnvironmentInitializer, eyre::Error> {
        const VIRTUAL_ROOT_FD: u32 = 4;

        let mut preopens: Vec<_> = Default::default();
        let rights_base = spec
            .get_type("rights")
            .unwrap()
            .flags()
            .unwrap()
            .value(
                [
                    "fd_read",
                    "fd_seek",
                    "fd_tell",
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
            .get_type("rights")
            .unwrap()
            .flags()
            .unwrap()
            .value(
                [
                    "fd_read",
                    "fd_seek",
                    "fd_tell",
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
                    spec.get_type("lookupflags")
                        .unwrap()
                        .flags()
                        .unwrap()
                        .value(HashSet::new())
                        .into_pb(spec, &TypeRef::Named("lookupflags".to_string())),
                    pb::Value {
                        which:          Some(pb::value::Which::String(
                            dir.name.clone().into_bytes(),
                        )),
                        special_fields: Default::default(),
                    },
                    spec.get_type("oflags")
                        .unwrap()
                        .flags()
                        .unwrap()
                        .value(["directory"].into_iter().collect())
                        .into_pb(spec, &TypeRef::Named("oflags".to_string())),
                    rights_base.clone(),
                    rights_inheriting.clone(),
                    spec.get_type("fdflags")
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
                WasiValue::from_pb(
                    call.results[0].clone(),
                    spec,
                    spec.types.get_by_key("fd").unwrap(),
                ),
            ));
        }

        Ok(EnvironmentInitializer { preopens })
    }
}
