fn main() {
}
// use std::io::stdin;

// extern crate wazzi_executor_pb_rust as pb;

// fn main() {
//     let mut stdin = stdin();
//     let mut input_stream = protobuf::CodedInputStream::new(&mut stdin);

//     loop {
//         let request: pb::Request = input_stream.read_message().unwrap();

//         match request.which.unwrap() {
//             | pb::request::Which::Call(call) => handle_call(call),
//             | _ => panic!(),
//         }
//     }
// }

// fn handle_call(call: pb::request::Call) {
//     use pb::WasiFunc::*;

//     let params: Vec;

//     match call.func.unwrap() {
//         | ARGS_GET => todo!(),
//         | ARGS_SIZES_GET => todo!(),
//         | ENVIRON_GET => todo!(),
//         | ENVIRON_SIZES_GET => todo!(),
//         | CLOCK_RES_GET => todo!(),
//         | CLOCK_TIME_GET => todo!(),
//         | FD_ADVISE => todo!(),
//         | FD_ALLOCATE => todo!(),
//         | FD_CLOSE => todo!(),
//         | FD_DATASYNC => todo!(),
//         | FD_FDSTAT_GET => todo!(),
//         | FD_FDSTAT_SET_FLAGS => todo!(),
//         | FD_FDSTAT_SET_RIGHTS => todo!(),
//         | FD_FILESTAT_GET => todo!(),
//         | FD_FILESTAT_SET_SIZE => todo!(),
//         | FD_FILESTAT_SET_TIMES => todo!(),
//         | FD_PREAD => todo!(),
//         | FD_PRESTAT_GET => todo!(),
//         | FD_PRESTAT_DIR_NAME => todo!(),
//         | FD_PWRITE => todo!(),
//         | FD_READ => todo!(),
//         | FD_READDIR => todo!(),
//         | FD_RENUMBER => todo!(),
//         | FD_SEEK => todo!(),
//         | FD_SYNC => todo!(),
//         | FD_TELL => todo!(),
//         | FD_WRITE => todo!(),
//         | PATH_CREATE_DIRECTORY => todo!(),
//         | PATH_FILESTAT_GET => todo!(),
//         | PATH_FILESTAT_SET_TIMES => todo!(),
//         | PATH_LINK => todo!(),
//         | PATH_OPEN => {
//             let fd = value(call.params[0]);

//             unsafe {
//                 wasi::path_open(
//                     call.params[0].handle(),
//                     dirflags,
//                     path,
//                     oflags,
//                     fs_rights_base,
//                     fs_rights_inheriting,
//                     fdflags,
//                 );
//             }
//         },
//         | PATH_READLINK => todo!(),
//         | PATH_REMOVE_DIRECTORY => todo!(),
//         | PATH_RENAME => todo!(),
//         | PATH_SYMLINK => todo!(),
//         | PATH_UNLINK_FILE => todo!(),
//     }
// }
