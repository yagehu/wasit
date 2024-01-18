#include <stdint.h>

int32_t __imported_wasi_snapshot_preview1_args_get(
    int32_t arg0,
    int32_t arg1
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("args_get")
));

int32_t __imported_wasi_snapshot_preview1_args_sizes_get(
    // results
    int32_t arg0,
    int32_t arg1
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("args_sizes_get")
));

int32_t __imported_wasi_snapshot_preview1_environ_get(
    int32_t arg0,
    int32_t arg1
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("environ_get")
));

int32_t __imported_wasi_snapshot_preview1_environ_sizes_get(
    // results
    int32_t arg0,
    int32_t arg1
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("environ_sizes_get")
));

int32_t __imported_wasi_snapshot_preview1_clock_res_get(
    int32_t arg0,
    // results
    int32_t arg1
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("clock_res_get")
));

int32_t __imported_wasi_snapshot_preview1_clock_time_get(
    int32_t param_0_id,
    int64_t param_1_precision,
    int32_t result_0_timestamp
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("clock_time_get")
));

int32_t __imported_wasi_snapshot_preview1_fd_advise(
    int32_t param_0_fd,
    int64_t param_1_offset,
    int64_t param_2_len,
    int32_t param_3_advice
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_advise")
));

int32_t __imported_wasi_snapshot_preview1_fd_allocate(
    int32_t param_0_fd,
    int64_t param_1_offset,
    int64_t param_2_len
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_allocate")
));

int32_t __imported_wasi_snapshot_preview1_fd_close(
    int32_t param_0_fd
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_close")
));

int32_t __imported_wasi_snapshot_preview1_fd_datasync(
    int32_t param_0_fd
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_datasync")
));

int32_t __imported_wasi_snapshot_preview1_fd_fdstat_get(
    int32_t param_0_fd,
    int32_t result_0_fdstat
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_fdstat_get")
));

int32_t __imported_wasi_snapshot_preview1_fd_fdstat_set_flags(
    int32_t param_0_fd,
    int32_t param_1_flags
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_fdstat_set_flags")
));

int32_t __imported_wasi_snapshot_preview1_fd_fdstat_set_rights(
    int32_t param_0_fd,
    int64_t param_1_fs_rights_base,
    int64_t param_2_fs_rights_inheriting
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_fdstat_set_rights")
));

int32_t __imported_wasi_snapshot_preview1_fd_filestat_get(
    int32_t param_0_fd,
    int32_t result_0_filestat
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_filestat_get")
));

int32_t __imported_wasi_snapshot_preview1_fd_filestat_set_size(
    int32_t param_0_fd,
    int64_t param_1_size
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_filestat_set_size")
));

int32_t __imported_wasi_snapshot_preview1_fd_filestat_set_times(
    int32_t param_0_fd,
    int64_t param_1_atim,
    int64_t param_2_mtim,
    int32_t param_3_fst_flags
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_filestat_set_times")
));

int32_t __imported_wasi_snapshot_preview1_fd_pread(
    int32_t param_0_fd,
    int32_t param_1_iovs,
    int32_t param_1_iovs_len,
    int64_t param_2_offset,
    int32_t result_0_size
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_pread")
));

int32_t __imported_wasi_snapshot_preview1_fd_prestat_get(
    int32_t param_0_fd,
    int32_t result_0_prestat
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_prestat_get")
));

int32_t __imported_wasi_snapshot_preview1_fd_prestat_dir_name(
    int32_t param_0_fd,
    int32_t param_1_path,
    int32_t param_2_path_len
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_prestat_dir_name")
));

int32_t __imported_wasi_snapshot_preview1_fd_pwrite(
    int32_t param_0_fd,
    int32_t param_1_iovs,
    int32_t param_2_iovs_len,
    int64_t param_3_offset,
    int32_t result_0_size
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_pwrite")
));

int32_t __imported_wasi_snapshot_preview1_fd_read(
    int32_t param_0_fd,
    int32_t param_1_iovs,
    int32_t param_2_iovs_len,
    int32_t result_0_size
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_read")
));

int32_t __imported_wasi_snapshot_preview1_fd_readdir(
    int32_t param_0_fd,
    int32_t param_1_buf,
    int32_t param_2_buf_len,
    int64_t param_3_cookie,
    int32_t result_0_size
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_readdir")
));

int32_t __imported_wasi_snapshot_preview1_fd_renumber(
    int32_t param_0_fd,
    int32_t param_1_to
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_renumber")
));

int32_t __imported_wasi_snapshot_preview1_fd_seek(
    int32_t param_0_fd,
    int64_t param_1_offset,
    int32_t param_2_whence,
    int32_t result_0_filesize
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_seek")
));

int32_t __imported_wasi_snapshot_preview1_fd_sync(
    int32_t param_0_fd
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_sync")
));

int32_t __imported_wasi_snapshot_preview1_fd_tell(
    int32_t param_0_fd,
    int32_t result_0_filesize
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_tell")
));

int32_t __imported_wasi_snapshot_preview1_fd_write(
    int32_t param_0_fd,
    int32_t param_1_iovs,
    int32_t param_2_iovs_len,
    int32_t result_0_size
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("fd_write")
));

int32_t __imported_wasi_snapshot_preview1_path_create_directory(
    int32_t param_0_fd,
    int32_t param_1_path,
    int32_t param_2_path_len
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("path_create_directory")
));

int32_t __imported_wasi_snapshot_preview1_path_filestat_get(
    int32_t param_0_fd,
    int32_t param_1_flags,
    int32_t param_2_path,
    int32_t param_2_path_len,
    int32_t result_0_filestat
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("path_filestat_get")
));

int32_t __imported_wasi_snapshot_preview1_path_filestat_set_times(
    int32_t param_0_fd,
    int32_t param_1_flags,
    int32_t param_2_path,
    int32_t param_2_path_len,
    int64_t param_3_atim,
    int64_t param_4_mtim,
    int32_t param_5_fst_flags
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("path_filestat_set_times")
));

int32_t __imported_wasi_snapshot_preview1_path_link(
    int32_t param_0_old_fd,
    int32_t param_1_old_flags,
    int32_t param_2_old_path,
    int32_t param_2_old_path_len,
    int32_t param_3_new_fd,
    int32_t param_4_new_path,
    int32_t param_4_new_path_len
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("path_link")
));

int32_t __imported_wasi_snapshot_preview1_path_open(
    int32_t p0_fd,
    int32_t p1_dirflags,
    int32_t p2_path,
    int32_t p2_path_len,
    int32_t p3_oflags,
    int64_t p4_fs_rights_base,
    int64_t p5_fs_rights_inheriting,
    int32_t p6_fdflags,
    int32_t r0_fd
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("path_open")
));

int32_t __imported_wasi_snapshot_preview1_path_readlink(
    int32_t p0_fd,
    int32_t p1_path,
    int32_t p1_path_len,
    int32_t p2_buf,
    int32_t p3_buf_len,
    int32_t r0_size
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("path_readlink")
));

int32_t __imported_wasi_snapshot_preview1_path_remove_directory(
    int32_t p0_fd,
    int32_t p1_path,
    int32_t p1_path_len
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("path_remove_directory")
));

int32_t __imported_wasi_snapshot_preview1_path_rename(
    int32_t p0_fd,
    int32_t p1_old_path,
    int32_t p1_old_path_len,
    int32_t p2_new_fd,
    int32_t p3_new_path,
    int32_t p3_new_path_len
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("path_rename")
));

int32_t __imported_wasi_snapshot_preview1_path_symlink(
    int32_t p0_old_path,
    int32_t p0_old_path_len,
    int32_t p1_fd,
    int32_t p2_new_path,
    int32_t p2_new_path_len
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("path_symlink")
));

int32_t __imported_wasi_snapshot_preview1_path_unlink_file(
    int32_t p0_fd,
    int32_t p1_path,
    int32_t p1_path_len
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("path_unlink_file")
));

int32_t __imported_wasi_snapshot_preview1_poll_oneoff(
    int32_t p0_in,
    int32_t p1_out,
    int32_t p2_nsubscriptions,
    int32_t r0_size
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("poll_oneoff")
));

void __imported_wasi_snapshot_preview1_proc_exit(int32_t p0_rval) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("proc_exit")
));

int32_t __imported_wasi_snapshot_preview1_proc_raise(int32_t p0_sig) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("proc_raise")
));

int32_t __imported_wasi_snapshot_preview1_sched_yield(void) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("sched_yield")
));

int32_t __imported_wasi_snapshot_preview1_random_get(int32_t p0_buf, int32_t p1_buf_len) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("random_get")
));

int32_t __imported_wasi_snapshot_preview1_sock_accept(
    int32_t p0_fd,
    int32_t p1_flags,
    int32_t r0_fd
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("sock_accept")
));

int32_t __imported_wasi_snapshot_preview1_sock_recv(
    int32_t p0_fd,
    int32_t p1_ri_data,
    int32_t p1_ri_data_len,
    int32_t p2_ri_flags,
    int32_t r0_size,
    int32_t r1_roflags
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("sock_recv")
));

int32_t __imported_wasi_snapshot_preview1_sock_send(
    int32_t p0_fd,
    int32_t p1_si_data,
    int32_t p1_si_data_len,
    int32_t p2_si_flags,
    int32_t r0_size
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("sock_send")
));

int32_t __imported_wasi_snapshot_preview1_sock_shutdown(
    int32_t p0_fd,
    int32_t p1_how
) __attribute__((
    __import_module__("wasi_snapshot_preview1"),
    __import_name__("sock_shutdown")
));
