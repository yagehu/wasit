use eyre::{Context, ContextCompat};
use wazzi_executor::RunningExecutor;
use wazzi_spec::package::{Function, Interface};
use wazzi_store::{Call, RuntimeStore};
use wazzi_wasi_component_model::value::Value;

use crate::resource_ctx::ResourceContext;

fn pb_func(name: &str) -> executor_pb::WasiFunc {
    use executor_pb::WasiFunc::*;

    match name {
        | "args_get" => ARGS_GET,
        | "args_sizes_get" => ARGS_SIZES_GET,
        | "environ_get" => ENVIRON_GET,
        | "environ_sizes_get" => ENVIRON_SIZES_GET,
        | "clock_res_get" => CLOCK_RES_GET,
        | "clock_time_get" => CLOCK_TIME_GET,
        | "fd_advise" => FD_ADVISE,
        | "fd_allocate" => FD_ALLOCATE,
        | "fd_close" => FD_CLOSE,
        | "fd_datasync" => FD_DATASYNC,
        | "fd_fdstat_get" => FD_FDSTAT_GET,
        | "fd_fdstat_set_flags" => FD_FDSTAT_SET_FLAGS,
        | "fd_fdstat_set_rights" => FD_FDSTAT_SET_RIGHTS,
        | "fd_filestat_get" => FD_FILESTAT_GET,
        | "fd_filestat_set_size" => FD_FILESTAT_SET_SIZE,
        | "fd_filestat_set_times" => FD_FILESTAT_SET_TIMES,
        | "fd_pread" => FD_PREAD,
        | "fd_prestat_get" => FD_PRESTAT_GET,
        | "fd_prestat_dir_name" => FD_PRESTAT_DIR_NAME,
        | "fd_pwrite" => FD_PWRITE,
        | "fd_read" => FD_READ,
        | "fd_readdir" => FD_READDIR,
        | "fd_renumber" => FD_RENUMBER,
        | "fd_seek" => FD_SEEK,
        | "fd_sync" => FD_SYNC,
        | "fd_tell" => FD_TELL,
        | "fd_write" => FD_WRITE,
        | "path_create_directory" => PATH_CREATE_DIRECTORY,
        | "path_filestat_get" => PATH_FILESTAT_GET,
        | "path_filestat_set_times" => PATH_FILESTAT_SET_TIMES,
        | "path_link" => PATH_LINK,
        | "path_open" => PATH_OPEN,
        | "path_remove_directory" => PATH_REMOVE_DIRECTORY,
        | "path_rename" => PATH_RENAME,
        | "path_symlink" => PATH_SYMLINK,
        | "path_unlink_file" => PATH_UNLINK_FILE,
        | _ => panic!("{name}"),
    }
}

#[derive(Debug)]
pub struct Prog {
    executor:     RunningExecutor,
    resource_ctx: ResourceContext,
    store:        RuntimeStore,
}

impl Prog {
    pub fn new(executor: RunningExecutor, store: RuntimeStore) -> Self {
        Self {
            executor,
            resource_ctx: ResourceContext::new(),
            store,
        }
    }

    pub fn executor(&self) -> &RunningExecutor {
        &self.executor
    }

    pub fn resource_ctx_mut(&mut self) -> &mut ResourceContext {
        &mut self.resource_ctx
    }

    pub fn resource_ctx(&mut self) -> &ResourceContext {
        &self.resource_ctx
    }

    pub fn store(&self) -> &RuntimeStore {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut RuntimeStore {
        &mut self.store
    }

    pub fn call(
        &mut self,
        interface: &Interface,
        func: &Function,
        params: Vec<Value>,
        results: Vec<Value>,
    ) -> Result<(), eyre::Error> {
        self.store
            .trace_mut()
            .begin_call()
            .wrap_err("failed to begin recording call")?;

        let result_valtypes = func.unpack_expected_result();
        let response = self.executor.call(executor_pb::request::Call {
            func:           pb_func(func.name.as_str()).into(),
            params:         func
                .params
                .iter()
                .zip(params)
                .map(|(param, v)| -> Result<_, eyre::Error> {
                    let def = interface
                        .resolve_valtype(&param.valtype)
                        .wrap_err("failed to resolve valtype")?;

                    Ok(v.into_pb(interface, &def))
                })
                .collect::<Result<_, _>>()?,
            results:        result_valtypes
                .iter()
                .zip(results)
                .map(|(result_valtype, v)| -> Result<_, eyre::Error> {
                    let def = interface
                        .resolve_valtype(&result_valtype)
                        .wrap_err("failed to resolve valtype")?;

                    Ok(v.into_pb(interface, &def))
                })
                .collect::<Result<_, _>>()?,
            special_fields: Default::default(),
        })?;
        let errno = match response.errno_option.unwrap() {
            | executor_pb::response::call::Errno_option::ErrnoSome(i) => Some(i),
            | executor_pb::response::call::Errno_option::ErrnoNone(_) => None,
            | _ => panic!(),
        };

        self.store
            .trace_mut()
            .end_call(Call {
                func: func.name.clone(),
                errno,
                params: response
                    .params
                    .into_iter()
                    .map(|param| Value::from_pb(param))
                    .collect(),
                results: response.results.into_iter().map(Value::from_pb).collect(),
            })
            .wrap_err("failed to end call")?;

        Ok(())
    }
}
