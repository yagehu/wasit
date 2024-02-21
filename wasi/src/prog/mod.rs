pub mod seed;

pub(crate) mod stateful;

mod r#final;

pub use r#final::{FinalProg, Value};
pub use stateful::Prog;

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
