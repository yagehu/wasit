#include <inttypes.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdnoreturn.h>

// protobuf-c generated
#include "wazzi-executor.pb-c.h"

#define STB_DS_IMPLEMENTATION
#include "stb_ds.h"

#include "wasi_snapshot_preview1.h"

#define MAX_MSG_SIZE 2048

struct resource {
    void * ptr;
    size_t size;
};

struct resource_map_entry {
    uint64_t        key;
    struct resource value;
};

struct resource_map_entry * resource_map;

noreturn static void fail(const char* err) {
    fprintf(stderr, "%s\n", err);
    exit(1);
}

static uint64_t u64_from_bytes(const uint8_t m[8]) {
    return ((uint64_t)m[7] << 56)
        | ((uint64_t)m[6] << 48)
        | ((uint64_t)m[5] << 40)
        | ((uint64_t)m[4] << 32)
        | ((uint64_t)m[3] << 24)
        | ((uint64_t)m[2] << 16)
        | ((uint64_t)m[1] << 8)
        | ((uint64_t)m[0] << 0);
}

static void u64_to_bytes(uint8_t buf[8], uint64_t i) {
    buf[7] = i >> 56;
    buf[6] = i >> 48;
    buf[5] = i >> 40;
    buf[4] = i >> 32;
    buf[3] = i >> 24;
    buf[2] = i >> 16;
    buf[1] = i >> 8;
    buf[0] = i >> 0;
}

static Request * read_request(void) {
    uint8_t size_buf[8];

    // Read message size as u64.
    size_t nread = fread(size_buf, 1, 8, stdin);
    if (nread != 8) fail("failed to read message size");

    const uint64_t message_size = u64_from_bytes(size_buf);
    uint8_t *      buf          = malloc(message_size);

    nread = fread(buf, 1, message_size, stdin);
    if (nread != message_size) fail("failed to read message");

    Request * req = request__unpack(NULL, message_size, buf);
    if (req == NULL) fail("failed to unpack request");

    free(buf);

    return req;
}

static void free_request(Request * req) {
    request__free_unpacked(req, NULL);
}

static size_t type_size(Type * type) {
    switch (type->which_case) {
        case TYPE__WHICH_BUILTIN: {
            switch (type->builtin->which_case) {
                case TYPE__BUILTIN__WHICH_U8:
                case TYPE__BUILTIN__WHICH_U32: return sizeof(uint32_t);
                case TYPE__BUILTIN__WHICH_U64: return sizeof(uint64_t);
                case TYPE__BUILTIN__WHICH__NOT_SET:
                case _TYPE__BUILTIN__WHICH__CASE_IS_INT_SIZE: fail("invalid builtin type");
            }

            break;
        }
        case TYPE__WHICH_STRING: fail("unimplemented: type_size string");
        case TYPE__WHICH_BITFLAGS: {
            switch (type->bitflags->repr) {
                case INT_REPR__INT_REPR_U8:
                case INT_REPR__INT_REPR_U16:
                case INT_REPR__INT_REPR_U32: return sizeof(uint32_t);
                case INT_REPR__INT_REPR_U64: return sizeof(uint64_t);
                case INT_REPR__INT_REPR_UNKNOWN:
                case _INT_REPR_IS_INT_SIZE: fail("invalid int repr");
            }

            break;
        }
        case TYPE__WHICH_HANDLE: return sizeof(uint32_t);
        case TYPE__WHICH__NOT_SET:
        case _TYPE__WHICH__CASE_IS_INT_SIZE: fail("invalid type");
    }
}

void set_ptr_value(const Type * type, const RawValue * raw_value, void * ptr) {
    switch (raw_value->which_case) {
        case RAW_VALUE__WHICH_BUILTIN: fail("unimplemented builtin");
        case RAW_VALUE__WHICH_STRING: memcpy(ptr, raw_value->string.data, raw_value->string.len); break;
        case RAW_VALUE__WHICH_BITFLAGS: {
            uint64_t repr = 0;

            for (int i = 0; i < raw_value->bitflags->n_members; i++) {
                if (raw_value->bitflags->members[i]) {
                    repr |= 0x1 << i;
                }
            }

            switch (type->bitflags->repr) {
                case INT_REPR__INT_REPR_U8:  * (uint8_t *)  ptr = (uint8_t)  repr; break;
                case INT_REPR__INT_REPR_U16: * (uint16_t *) ptr = (uint16_t) repr; break;
                case INT_REPR__INT_REPR_U32: * (uint32_t *) ptr = (uint32_t) repr; break;
                case INT_REPR__INT_REPR_U64: * (uint64_t *) ptr = (uint64_t) repr; break;
                case INT_REPR__INT_REPR_UNKNOWN:
                case _INT_REPR_IS_INT_SIZE: fail("invalid bitflags repr");
            }

            break;
        }
        case RAW_VALUE__WHICH_HANDLE:   fail("unimplemented handle");
        case RAW_VALUE__WHICH_POINTER: {
            switch (raw_value->pointer->alloc->which_case) {
                case VALUE_SPEC__WHICH_RESOURCE: {
                    struct resource_map_entry * resource_entry =
                        hmgetp_null(resource_map, raw_value->pointer->alloc->resource->id);
                    if (resource_entry == NULL) fail("pointer alloc resource not found");

                    * (void **) ptr = malloc(* (uint32_t *) resource_entry->value.ptr);

                    break;
                }
                case VALUE_SPEC__WHICH_RAW_VALUE: {
                    if (
                        raw_value->pointer->alloc->raw_value->which_case != RAW_VALUE__WHICH_BUILTIN
                        || raw_value->pointer->alloc->raw_value->builtin->which_case != RAW_VALUE__BUILTIN__WHICH_U32
                    ) fail("only an u32 can alloc pointer");

                    * (void **) ptr = malloc(raw_value->pointer->alloc->raw_value->builtin->u32);

                    break;
                }
                case VALUE_SPEC__WHICH__NOT_SET:
                case _VALUE_SPEC__WHICH__CASE_IS_INT_SIZE: fail("invalid pointer alloc");
            }

            break;
        }
        case RAW_VALUE__WHICH__NOT_SET:
        case _RAW_VALUE__WHICH__CASE_IS_INT_SIZE: fail("invalid raw value");
    }
}

static void * handle_param_pre(ValueSpec * spec, int32_t * len) {
    switch (spec->which_case) {
        case VALUE_SPEC__WHICH_RESOURCE: {
            struct resource_map_entry * resource_entry =
                hmgetp_null(resource_map, spec->resource->id);
            if (resource_entry == NULL) fail("param resource not found");

            return resource_entry->value.ptr;
        }
        case VALUE_SPEC__WHICH_RAW_VALUE: {
            void * ptr = NULL;

            switch (spec->raw_value->which_case) {
                case RAW_VALUE__WHICH_BUILTIN: {
                    switch (spec->raw_value->builtin->which_case) {
                        case RAW_VALUE__BUILTIN__WHICH_U8:  ptr = malloc(sizeof(uint32_t)); break;
                        case RAW_VALUE__BUILTIN__WHICH_U32: ptr = malloc(sizeof(uint32_t)); break;
                        case RAW_VALUE__BUILTIN__WHICH_U64: ptr = malloc(sizeof(uint64_t)); break;
                        case RAW_VALUE__BUILTIN__WHICH_S64: ptr = malloc(sizeof(int64_t));  break;
                        case RAW_VALUE__BUILTIN__WHICH__NOT_SET:
                        case _RAW_VALUE__BUILTIN__WHICH__CASE_IS_INT_SIZE: fail("invalid builtin type");
                    }

                    break;
                }
                case RAW_VALUE__WHICH_STRING: {
                    * len = spec->raw_value->string.len;
                    ptr   = malloc(spec->raw_value->string.len);

                    break;
                }
                case RAW_VALUE__WHICH_BITFLAGS: {
                    switch (spec->type->bitflags->repr) {
                        case _INT_REPR_IS_INT_SIZE:
                        case INT_REPR__INT_REPR_UNKNOWN: fail("unknown int repr");
                        case INT_REPR__INT_REPR_U8:
                        case INT_REPR__INT_REPR_U16:
                        case INT_REPR__INT_REPR_U32: ptr = calloc(1, sizeof(uint32_t)); break;
                        case INT_REPR__INT_REPR_U64: ptr = calloc(1, sizeof(uint64_t)); break;
                    }

                    break;
                }
                case RAW_VALUE__WHICH_HANDLE:  ptr = malloc(sizeof(uint32_t)); break;
                case RAW_VALUE__WHICH_POINTER: ptr = malloc(sizeof(void *));   break;
                case RAW_VALUE__WHICH__NOT_SET:
                case _RAW_VALUE__WHICH__CASE_IS_INT_SIZE: fail("invalid raw value type");
            }
            if (ptr == NULL) fail("failed to allocate param ptr");

            set_ptr_value(spec->type, spec->raw_value, ptr);

            return ptr;
        }
        case VALUE_SPEC__WHICH__NOT_SET:
        case _VALUE_SPEC__WHICH__CASE_IS_INT_SIZE: fail("invalid value spec type");
    }
}

static void handle_param_post(ValueSpec * spec, void * ptr) {
    switch (spec->which_case) {
        case VALUE_SPEC__WHICH_RAW_VALUE: break;
        case VALUE_SPEC__WHICH_RESOURCE: free(ptr); break;
        case VALUE_SPEC__WHICH__NOT_SET:
        case _VALUE_SPEC__WHICH__CASE_IS_INT_SIZE: fail("invalid param value spec");
    }
}

static void * handle_result_pre(ResultSpec * spec) {
    return malloc(type_size(spec->type));
}

static void handle_result_post(ResultSpec * spec, void * ptr) {
    switch (spec->which_case) {
        case RESULT_SPEC__WHICH_RESOURCE: {
            const size_t size = type_size(spec->type);
            const struct resource resource = {
                .ptr  = ptr,
                .size = size,
            };

            hmput(resource_map, spec->resource->id, resource);

            break;
        }
        case RESULT_SPEC__WHICH_IGNORE: free(ptr); break;
        case RESULT_SPEC__WHICH__NOT_SET:
        case _RESULT_SPEC__WHICH__CASE_IS_INT_SIZE: fail("unknown result spec");
    }
}

static void handle_decl(Request__Decl * decl) {
    struct resource resource;

    switch (decl->value->which_case) {
        case RAW_VALUE__WHICH_BUILTIN:  fail("cannot decl builtin");
        case RAW_VALUE__WHICH_STRING:   fail("cannot decl string");
        case RAW_VALUE__WHICH_BITFLAGS: fail("cannot decl bitflags");
        case RAW_VALUE__WHICH_HANDLE: {
            uint32_t * ptr = malloc(sizeof(uint32_t));

            * ptr         = decl->value->handle->value;
            resource.ptr  = ptr;
            resource.size = sizeof(uint32_t);

            break;
        }
        case RAW_VALUE__WHICH_POINTER: fail("cannot decl pointer");
        case RAW_VALUE__WHICH__NOT_SET:
        case _RAW_VALUE__WHICH__CASE_IS_INT_SIZE: fail("invalid decl valid");
    }

    hmput(resource_map, decl->resource_id, resource);

    Response       msg   = RESPONSE__INIT;
    Response__Decl decl_ = RESPONSE__DECL__INIT;

    msg.decl       = &decl_;
    msg.which_case = RESPONSE__WHICH_DECL;

    const size_t msg_size = response__get_packed_size(&msg);
    void *       buf      = malloc(msg_size);

    uint8_t size_buf[8];

    u64_to_bytes(size_buf, msg_size);
    response__pack(&msg, buf);

    size_t blks_written = fwrite(size_buf, 8, 1, stdout);
    if (blks_written != 1) fail("failed to write message size out");

    blks_written = fwrite(buf, msg_size, 1, stdout);
    if (blks_written != 1) fail("failed to write message out");

    fflush(stdout);
    free(buf);
}

static void handle_call(Request__Call * call) {
    Response__Call response = RESPONSE__CALL__INIT;
    ReturnValue    return_  = RETURN_VALUE__INIT;

    switch (call->func) {
        case WASI_FUNC__WASI_FUNC_UNKNOWN: fail("unknown func");
        case WASI_FUNC__WASI_FUNC_ARGS_GET: {
            void *  p0_argv_ptr     = handle_param_pre(call->params[0], NULL);
            void *  p1_argv_buf_ptr = handle_param_pre(call->params[1], NULL);
            int32_t p0_argv         = * (int32_t *) p0_argv_ptr;
            int32_t p1_argv_buf     = * (int32_t *) p1_argv_buf_ptr;

            int32_t errno = __imported_wasi_snapshot_preview1_args_get(p0_argv,  p1_argv_buf);

            handle_param_post(call->params[0], p0_argv_ptr);
            handle_param_post(call->params[1], p1_argv_buf_ptr);

            return_.which_case = RETURN_VALUE__WHICH_ERRNO;
            return_.errno      = errno;

            break;
        }
        case WASI_FUNC__WASI_FUNC_ARGS_SIZES_GET: {
            void *  r0_argv_size_ptr     = handle_result_pre(call->results[0]);
            void *  r1_argv_buf_size_ptr = handle_result_pre(call->results[1]);
            int32_t r0_argv_size         = (int32_t) r0_argv_size_ptr;
            int32_t r1_argv_buf_size     = (int32_t) r1_argv_buf_size_ptr;

            int32_t errno = __imported_wasi_snapshot_preview1_args_sizes_get(
                r0_argv_size,
                r1_argv_buf_size
            );

            handle_result_post(call->results[0], r0_argv_size_ptr);
            handle_result_post(call->results[1], r1_argv_buf_size_ptr);

            return_.which_case = RETURN_VALUE__WHICH_ERRNO;
            return_.errno      = errno;

            break;
        };
        case WASI_FUNC__WASI_FUNC_ENVIRON_GET: fail("unimplemented: environ_get");
        case WASI_FUNC__WASI_FUNC_ENVIRON_SIZES_GET: fail("unimplemented: environ_sizes_get");
        case WASI_FUNC__WASI_FUNC_CLOCK_RES_GET: fail("unimplemented: clock_res_get");
        case WASI_FUNC__WASI_FUNC_CLOCK_TIME_GET: fail("unimplemented: clock_time_get");
        case WASI_FUNC__WASI_FUNC_FD_ADVISE: fail("unimplemented: fd_advise");
        case WASI_FUNC__WASI_FUNC_FD_ALLOCATE: fail("unimplemented: fd_allocate");
        case WASI_FUNC__WASI_FUNC_PATH_OPEN: {
            int32_t p2_path_len                 = 0;
            void *  p0_fd_ptr                   = handle_param_pre(call->params[0], NULL);
            void *  p1_dirflags_ptr             = handle_param_pre(call->params[1], NULL);
            void *  p2_path_ptr                 = handle_param_pre(call->params[2], &p2_path_len);
            void *  p3_oflags_ptr               = handle_param_pre(call->params[3], NULL);
            void *  p4_fs_rights_base_ptr       = handle_param_pre(call->params[4], NULL);
            void *  p5_fs_rights_inheriting_ptr = handle_param_pre(call->params[5], NULL);
            void *  p6_fdflags_ptr              = handle_param_pre(call->params[6], NULL);
            void *  r0_fd_ptr                   = handle_result_pre(call->results[0]);
            int32_t p0_fd                       = * (int32_t *) p0_fd_ptr;
            int32_t p1_dirflags                 = * (int32_t *) p1_dirflags_ptr;
            int32_t p2_path                     = (int32_t) p2_path_ptr;
            int32_t p3_oflags                   = * (int32_t *) p3_oflags_ptr;
            int64_t p4_fs_rights_base           = * (int64_t *) p4_fs_rights_base_ptr;
            int64_t p5_fs_rights_inheriting     = * (int64_t *) p5_fs_rights_inheriting_ptr;
            int32_t p6_fdflags                  = * (int32_t *) p6_fdflags_ptr;
            int32_t r0_fd                       = (int32_t) r0_fd_ptr;

            int32_t errno = __imported_wasi_snapshot_preview1_path_open(
                p0_fd,
                p1_dirflags,
                p2_path,
                p2_path_len,
                p3_oflags,
                p4_fs_rights_base,
                p5_fs_rights_inheriting,
                p6_fdflags,
                r0_fd
            );

            handle_param_post(call->params[0], p0_fd_ptr);
            handle_param_post(call->params[1], p1_dirflags_ptr);
            handle_param_post(call->params[2], p2_path_ptr);
            handle_param_post(call->params[3], p3_oflags_ptr);
            handle_param_post(call->params[4], p4_fs_rights_base_ptr);
            handle_param_post(call->params[5], p5_fs_rights_inheriting_ptr);
            handle_param_post(call->params[6], p6_fdflags_ptr);
            handle_result_post(call->results[0], r0_fd_ptr);

            return_.which_case = RETURN_VALUE__WHICH_ERRNO;
            return_.errno      = errno;

            break;
        }
        case _WASI_FUNC_IS_INT_SIZE: fail("unreachable");
    }

    response.return_ = &return_;

    Response msg = RESPONSE__INIT;

    msg.call       = &response;
    msg.which_case = RESPONSE__WHICH_CALL;

    const size_t msg_size = response__get_packed_size(&msg);
    void *       buf      = malloc(msg_size);

    uint8_t size_buf[8];

    u64_to_bytes(size_buf, msg_size);
    response__pack(&msg, buf);

    size_t blks_written = fwrite(size_buf, 8, 1, stdout);
    if (blks_written != 1) fail("failed to write message size out");

    blks_written = fwrite(buf, msg_size, 1, stdout);
    if (blks_written != 1) fail("failed to write message out");

    fflush(stdout);
    free(buf);
}

int main(void) {
    // struct resource_map_entry * resource_map;

    while (true) {
        Request * req = read_request();

        switch (req->which_case) {
            case REQUEST__WHICH_CALL: handle_call(req->call); break;
            case REQUEST__WHICH_DECL: handle_decl(req->decl); break;
            case REQUEST__WHICH__NOT_SET:
            case _REQUEST__WHICH__CASE_IS_INT_SIZE: fail("invalid request");
        }

        free_request(req);
    }
}
