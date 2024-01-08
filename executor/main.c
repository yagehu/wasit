#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdnoreturn.h>

#define STB_DS_IMPLEMENTATION
#include "stb_ds.h"

#include "wasi_snapshot_preview1.h"
#include "wazzi-executor.capnp.h"

const int in_fd  = 0;
const int out_fd = 1;

struct resource {
    void * ptr;
    size_t size;
};

struct resource_map_entry {
    uint64_t        key;
    struct resource value;
};

noreturn void fail(const char* err);

// Allocate some memory for the type and set its value.
void * malloc_set_value(const struct Type type, const struct Value value, int32_t * len);

void set_value_from_ptr(struct capn_segment **, struct Value *, const struct Type, void * ptr);

void handle_decl(struct resource_map_entry ** resource_map, const struct DeclRequest);
void handle_call(struct resource_map_entry ** resource_map, const struct CallRequest);

void * handle_param_pre(struct resource_map_entry ** resource_map, struct ParamSpec spec, int32_t * len);
void   handle_param_post(struct ParamSpec spec, void * ptr);
void * handle_result_pre(struct resource_map_entry ** resource_map, struct ResultSpec result);
void   handle_result_post(
    struct resource_map_entry ** resource_map,
    struct capn_segment ** segment,
    struct ResultSpec spec,
    const size_t result_idx,
    CallResult_list call_result_list,
    void * ptr
);

int main(void) {
    struct resource_map_entry * resource_map = NULL;

    FILE * f = fdopen(in_fd, "r");
    if (f == NULL) perror("failed to fdopen");

    while (true) {
        struct capn capn;

        int ret = capn_init_fp(&capn, f, 0 /* packed */);
        if (ret != 0) continue;

        Request_ptr root;
        struct Request request;

        root.p = capn_getp(capn_root(&capn), 0 /* off */, 1 /* resolve */);
        read_Request(&request, root);

        switch (request.which) {
            case Request_decl: {
                struct DeclRequest decl_request;

                read_DeclRequest(&decl_request, request.decl);
                handle_decl(&resource_map, decl_request);

                break;
            }
            case Request_call: {
                struct CallRequest call_request;

                read_CallRequest(&call_request, request.call);
                handle_call(&resource_map, call_request);

                break;
            }
        }

        capn_free(&capn);
    }
/
    return 0;
}

noreturn void fail(const char* err) {
    fprintf(stderr, "%s\n", err);
    exit(1);
}

void * malloc_set_value(const struct Type type, const struct Value value, int32_t * len) {
    switch (value.which) {
        case Value_builtin: {
            struct Value_Builtin builtin_value;

            read_Value_Builtin(&builtin_value, value.builtin);

            void * ptr;

            switch (builtin_value.which) {
                case Value_Builtin__char: {
                    ptr = malloc(sizeof(int32_t));
                    * (uint8_t *) ptr = builtin_value._char;
                    break;
                }
                case Value_Builtin_s8: {
                    ptr = malloc(sizeof(int32_t));
                    * (int8_t *) ptr = builtin_value.s8;
                    break;
                }
                case Value_Builtin_s16: {
                    ptr = malloc(sizeof(int32_t));
                    * (int16_t *) ptr = builtin_value.s16;
                    break;
                }
                case Value_Builtin_s32: {
                    ptr = malloc(sizeof(int32_t));
                    * (int32_t *) ptr = builtin_value.s32;
                    break;
                }
                case Value_Builtin_s64: {
                    ptr = malloc(sizeof(int64_t));
                    * (int64_t *) ptr = builtin_value.s64;
                    break;
                }
                case Value_Builtin_u8: {
                    ptr = malloc(sizeof(uint32_t));
                    * (uint8_t *) ptr = builtin_value.u8;
                    break;
                }
                case Value_Builtin_u16: {
                    ptr = malloc(sizeof(uint32_t));
                    * (uint16_t *) ptr = builtin_value.u16;
                    break;
                }
                case Value_Builtin_u32: {
                    ptr = malloc(sizeof(uint32_t));
                    * (uint32_t *) ptr = builtin_value.u32;
                    break;
                }
                case Value_Builtin_u64: {
                    ptr = malloc(sizeof(uint64_t));
                    * (uint64_t *) ptr = builtin_value.u64;
                    break;
                }
            }

            return ptr;
        }
        case Value__bool: {
            bool * ptr = malloc(sizeof(bool));

           * ptr = value._bool;

            return ptr;
        }
        case Value_string: {
            char * ptr = malloc(value.string.len);

            * len = value.string.len;

            return strncpy(ptr, value.string.str, value.string.len);
        };
        case Value_bitflags: {
            struct Value_Bitflags bitflags;
            struct Type_Bitflags  bitflags_type;

            read_Type_Bitflags(&bitflags_type, type.bitflags);
            read_Value_Bitflags(&bitflags, value.bitflags);

            uint64_t int_value = 0;

            for (int i = 0; i < capn_len(bitflags.members); i++) {
                const bool is_set = capn_get1(bitflags.members, i);

                int_value |= (0x1 & is_set) << i;
            }

            switch (bitflags_type.repr) {
                case Type_IntRepr_u8:
                case Type_IntRepr_u16:
                case Type_IntRepr_u32: {
                    uint32_t * ptr = malloc(sizeof(uint32_t));

                    * ptr = (uint32_t) int_value;

                    return ptr;
                }
                case Type_IntRepr_u64: {
                    uint64_t * ptr = malloc(sizeof(uint64_t));

                    * ptr = int_value;

                    return ptr;
                }
            }
        }
        case Value_handle: fail("unimplemeneted: handle value");
        case Value_array: {
            struct Value_Array array_value;
            struct Type_Array  array_type;

            read_Value_Array(&array_value, value.array);
            read_Type_Array(&array_type, type.array);

            struct Type item_type;

            read_Type(&item_type, array_type.item);

            * len = capn_len(array_value.items);

            for (int i = 0; i < capn_len(array_value.items); i++) {
                struct Value value;

                get_Value(&value, array_value.items, i);


            }
        }
    }
}

void set_value_from_ptr(
    struct capn_segment ** segment,
    struct Value * value,
    const struct Type type,
    void * ptr
) {
    switch (type.which) {
        case Type_builtin: {
            struct Type_Builtin builtin_type;

            read_Type_Builtin(&builtin_type, type.builtin);

            switch (builtin_type.which) {
                case Type_Builtin_u8:
                case Type_Builtin_u16:
                case Type_Builtin_u32:
                case Type_Builtin_s8:
                case Type_Builtin_s16:
                case Type_Builtin_s32:
                case Type_Builtin__char: ptr = malloc(sizeof(int32_t)); break;
                case Type_Builtin_u64:
                case Type_Builtin_s64: ptr = malloc(sizeof(int64_t)); break;
            }
        }
        case Type__bool: {
            value->which = Value__bool;
            value->_bool = * (bool *) ptr;

            break;
        }
        case Type_bitflags: {
            struct Type_Bitflags  bitflags_type;

            read_Type_Bitflags(&bitflags_type, type.bitflags);

            capn_list1 members      = capn_new_list1(*segment, capn_len(bitflags_type.members));
            uint64_t   bitflags_int = 0;

            switch (bitflags_type.repr) {
                case Type_IntRepr_u8:
                case Type_IntRepr_u16:
                case Type_IntRepr_u32:
                    bitflags_int = * (uint32_t *) ptr;
                    break;
                case Type_IntRepr_u64:
                    bitflags_int = * (uint64_t *) ptr;
                    break;
            }

            for (int i = 0; i < capn_len(bitflags_type.members); i++) {
                capn_set1(members, i, bitflags_int | 0x1);
                bitflags_int >>= 1;
            }

            struct Value_Bitflags bitflags_value = {
                .members = members,
            };

            value->which = Value_bitflags;
            write_Value_Bitflags(&bitflags_value, value->bitflags);

            break;
        }
        case Type_string: fail("unimplemented: string type result");
        case Type_handle: {
            value->which  = Value_handle;
            value->handle = * (uint32_t *) ptr;

            break;
        }
        case Type_array: fail("unimplemented: array type result");
        case Type_record: fail("unimplemented: record type result");
        case Type_constPointer: fail("unimplemented: constPointer type result");
    }
}

void handle_decl(
    struct resource_map_entry ** resource_map,
    const struct DeclRequest decl
) {
    struct capn capn;

    capn_init_malloc(&capn);

    struct capn_ptr        capn_ptr = capn_root(&capn);
    struct capn_segment ** segment  = &capn_ptr.seg;
    struct DeclResponse    decl_response;
    struct Value           value;
    struct resource        resource;

    read_Value(&value, decl.value);

    switch (value.which) {
        case Value_builtin:
        case Value__bool:
        case Value_string:
        case Value_bitflags: fail("only handle can be declared");
        case Value_handle: {
            uint32_t * ptr = malloc(sizeof(uint32_t));

            * ptr = value.handle;
            resource.ptr  = ptr;
            resource.size = sizeof(uint32_t);
        }
    }

    hmput(*resource_map, decl.resourceId, resource);

    DeclResponse_ptr decl_response_ptr = new_DeclResponse(*segment);

    write_DeclResponse(&decl_response, decl_response_ptr);

    const int ret = capn_setp(capn_ptr, 0, decl_response_ptr.p);
    if (ret != 0) fail("failed to capn_setp");

    capn_write_fd(&capn, write, out_fd, 0 /* packed */);
    capn_free(&capn);
}

void handle_call(
    struct resource_map_entry ** resource_map,
    const struct CallRequest call
) {
    struct capn capn;

    capn_init_malloc(&capn);

    struct capn_ptr        capn_ptr = capn_root(&capn);
    struct capn_segment ** segment  = &capn_ptr.seg;
    struct CallResponse    call_response;
    struct CallReturn      call_return;
    
    switch (call.func) {
        case Func_fdWrite: {
            struct ParamSpec  p0_fd;
            struct ParamSpec  p1_iovs;
            struct ResultSpec r0_size;

            get_ParamSpec(&p0_fd, call.params, 0);
            get_ParamSpec(&p1_iovs, call.params, 1);
            get_ResultSpec(&r0_size, call.results, 0);

            int32_t p1_iovs_len = 0;
            void *  p0_fd_ptr    = handle_param_pre(resource_map, p0_fd, NULL);
            void *  p1_iovs_ptr  = handle_param_pre(resource_map, p1_iovs, &p1_iovs_len);
            void *  r0_size_ptr  = handle_result_pre(resource_map, r0_size);
            int32_t p0_fd_       = * (int32_t *) p0_fd_ptr;
            int32_t p1_iovs_     = * (int32_t *) p1_iovs_ptr;
            int32_t r0_size_     = (int32_t) r0_size_ptr;

            fprintf(stderr, "fd_write()\n");

            int32_t errno = __imported_wasi_snapshot_preview1_fd_write(
                p0_fd_,
                p1_iovs_,
                p1_iovs_len,
                r0_size_
            );

            fprintf(stderr, "fd_write() ret %d written %d\n", errno, * (int32_t *) r0_size_ptr);

            CallResult_list call_result_list = new_CallResult_list(*segment, 1 /* sz */);

            handle_param_post(p0_fd, p0_fd_ptr);
            handle_param_post(p1_iovs, p1_iovs_ptr);
            handle_result_post(resource_map, segment, r0_size, 0, call_result_list, r0_size_ptr);

            call_return.which     = CallReturn_errno;
            call_return.errno     = errno;
            call_response.results = call_result_list;
            write_CallReturn(&call_return, call_response._return);

            break;
        }
        case Func_pathOpen: {
            struct ParamSpec  p0_fd;
            struct ParamSpec  p1_dirflags;
            struct ParamSpec  p2_path;
            struct ParamSpec  p3_oflags;
            struct ParamSpec  p4_fs_rights_base;
            struct ParamSpec  p5_fs_rights_inheriting;
            struct ParamSpec  p6_fdflags;
            struct ResultSpec r0_fd;

            get_ParamSpec(&p0_fd, call.params, 0);
            get_ParamSpec(&p1_dirflags, call.params, 1);
            get_ParamSpec(&p2_path, call.params, 2);
            get_ParamSpec(&p3_oflags, call.params, 3);
            get_ParamSpec(&p4_fs_rights_base, call.params, 4);
            get_ParamSpec(&p5_fs_rights_inheriting, call.params, 5);
            get_ParamSpec(&p6_fdflags, call.params, 6);
            get_ResultSpec(&r0_fd, call.results, 0);

            int32_t p2_path_len                 = 0;
            void *  p0_fd_ptr                   = handle_param_pre(resource_map, p0_fd, NULL);
            void *  p1_dirflags_ptr             = handle_param_pre(resource_map, p1_dirflags, NULL);
            void *  p2_path_ptr                 = handle_param_pre(resource_map, p2_path, &p2_path_len);
            void *  p3_oflags_ptr               = handle_param_pre(resource_map, p3_oflags, NULL);
            void *  p4_fs_rights_base_ptr       = handle_param_pre(resource_map, p4_fs_rights_base, NULL);
            void *  p5_fs_rights_inheriting_ptr = handle_param_pre(resource_map, p5_fs_rights_inheriting, NULL);
            void *  p6_fdflags_ptr              = handle_param_pre(resource_map, p6_fdflags, NULL);
            void *  r0_fd_ptr                   = handle_result_pre(resource_map, r0_fd);
            int32_t p0_fd_                      = * (int32_t *) p0_fd_ptr;
            int32_t p1_dirflags_                = * (int32_t *) p1_dirflags_ptr;
            int32_t p3_oflags_                  = * (int32_t *) p3_oflags_ptr;
            int64_t p4_fs_rights_base_          = * (int64_t *) p4_fs_rights_base_ptr;
            int64_t p5_fs_rights_inheriting_    = * (int64_t *) p5_fs_rights_inheriting_ptr;
            int32_t p6_fdflags_                 = * (int32_t *) p6_fdflags_ptr;

            fprintf(stderr, "path_open()\n");

            int32_t errno = __imported_wasi_snapshot_preview1_path_open(
                p0_fd_,
                p1_dirflags_,
                (int32_t) p2_path_ptr,
                p2_path_len,
                p3_oflags_,
                p4_fs_rights_base_,
                p5_fs_rights_inheriting_,
                p6_fdflags_,
                (int32_t) r0_fd_ptr
            );

            fprintf(stderr, "path_open ret %d %d\n", errno, * (int32_t *) r0_fd_ptr);

            CallResult_list call_result_list = new_CallResult_list(*segment, 1 /* sz */);

            handle_param_post(p0_fd, p0_fd_ptr);
            handle_param_post(p1_dirflags, p1_dirflags_ptr);
            handle_param_post(p2_path, p2_path_ptr);
            handle_param_post(p3_oflags, p3_oflags_ptr);
            handle_param_post(p4_fs_rights_base, p4_fs_rights_base_ptr);
            handle_param_post(p5_fs_rights_inheriting, p5_fs_rights_inheriting_ptr);
            handle_param_post(p6_fdflags, p6_fdflags_ptr);
            handle_result_post(resource_map, segment, r0_fd, 0, call_result_list, r0_fd_ptr);

            call_return.which     = CallReturn_errno;
            call_return.errno     = errno;
            call_response.results = call_result_list;

            break;
        }
        default:
            break;
    }

    CallReturn_ptr call_return_ptr = new_CallReturn(*segment);

    write_CallReturn(&call_return, call_return_ptr);

    CallResponse_ptr call_response_ptr = new_CallResponse(*segment);

    call_response._return = call_return_ptr;
    write_CallResponse(&call_response, call_response_ptr);

    const int ret = capn_setp(capn_ptr, 0, call_response_ptr.p);
    if (ret != 0) fail("failed to capn_setp");

    capn_write_fd(&capn, write, out_fd, 0 /* packed */);
    capn_free(&capn);
}

void * handle_param_pre(
    struct resource_map_entry ** resource_map,
    struct ParamSpec spec,
    int32_t * len
) {
    switch (spec.which) {
        case ParamSpec_resource: {
            struct ResourceRef resource_ref;

            read_ResourceRef(&resource_ref, spec.resource);

            // Use an existing resource.

            struct resource_map_entry * resource_entry =
                hmgetp_null(*resource_map, resource_ref.id);
            if (resource_entry == NULL) fail("param resource not found");

            return resource_entry->value.ptr;
        }
        case ParamSpec_value: {
            struct Value value;
            struct Type type;

            read_Value(&value, spec.value);
            read_Type(&type, spec.type);

            return malloc_set_value(type, value, len);
        }
    }

    return NULL;
}

void handle_param_post(
    struct ParamSpec spec,
    void * ptr
) {
    switch (spec.which) {
        case ParamSpec_resource: break;
        case ParamSpec_value: {
            free(ptr);

            break;
        }
    }
}

void * handle_result_pre(struct resource_map_entry ** resource_map, struct ResultSpec spec) {
    (void) resource_map;

    struct Type type;

    read_Type(&type, spec.type);

    switch (type.which) {
        case Type_builtin: {
            struct Type_Builtin builtin;

            read_Type_Builtin(&builtin, type.builtin);

            void * ptr;

            switch (builtin.which) {
                case Type_Builtin_u8:
                case Type_Builtin_u16:
                case Type_Builtin_u32:
                case Type_Builtin_s8:
                case Type_Builtin_s16:
                case Type_Builtin_s32:
                case Type_Builtin__char: ptr = malloc(sizeof(int32_t)); break;
                case Type_Builtin_u64:
                case Type_Builtin_s64: ptr = malloc(sizeof(int64_t)); break;
            }

            return ptr;
        };
        case Type__bool: fail("result cannot be bool");
        case Type_string: fail("result cannot be string");
        case Type_bitflags: {
            struct Type_Bitflags bitflags_type;

            read_Type_Bitflags(&bitflags_type, type.bitflags);

            void * ptr;

            switch (bitflags_type.repr) {
                case Type_IntRepr_u8:
                case Type_IntRepr_u16:
                case Type_IntRepr_u32: ptr = malloc(sizeof(int32_t)); break;
                case Type_IntRepr_u64: ptr = malloc(sizeof(int64_t)); break;
            }

            return ptr;
        }
        case Type_handle: return malloc(sizeof(int32_t));
        case Type_array: fail("result cannot be array");
        case Type_record: fail("result cannot be record");
        case Type_constPointer: fail("result cannot be constPointer");
    }
}

void handle_result_post(
    struct resource_map_entry ** resource_map,
    struct capn_segment ** segment,
    struct ResultSpec spec,
    const  size_t result_idx,
    CallResult_list call_result_list,
    void * ptr
) {
    struct CallResult call_result;
    struct Value      value;
    struct Type       type;

    read_Type(&type, spec.type);
    set_value_from_ptr(segment, &value, type, ptr);

    call_result.memoryOffset = (uint32_t) ptr;
    write_Value(&value, call_result.value);

    switch (spec.which) {
        case ResultSpec_ignore: {
            free(ptr);
            break;
        }
        case ResultSpec_resource: {
            // TODO(huyage): size.
            const struct resource resource = {
                .ptr  = ptr,
                .size = 0,
            };

            hmput(*resource_map, spec.resource, resource);

            break;
        }
    }

    // struct WasiType wasi_type;

    // read_WasiType(&wasi_type, result.wasiType);

    // switch (result.which) {
    //     case Result_ignore: fail("unimplemented result type ignore");
    //     case Result_resource: {
    //         struct Decl          decl;
    //         struct AllocStrategy alloc;
    //         size_t               size;

    //         read_Decl(&decl, result.resource);
    //         read_AllocStrategy(&alloc, decl.alloc);

    //         switch (alloc.which) {
    //             case AllocStrategy_none: fail("result must be alloc'd");
    //             case AllocStrategy_asArray: fail("result cannot be array");
    //             case AllocStrategy_fromSize: {
    //                 size = alloc.fromSize;
    //                 break;
    //             }
    //             case AllocStrategy_fromResource: {
    //                 struct resource_map_entry * resource_entry =
    //                     hmgetp_null(*resource_map, alloc.fromResource);

    //                 size = * (size_t *) resource_entry->value.ptr;
    //                 break;
    //             }
    //         }

    //         struct resource resource = {
    //             .ptr  = ptr,
    //             .size = size,
    //         };

    //         hmput(*resource_map, decl.resourceId, resource);

    //         break;
    //     }
    // }

    // // Send results back.

    // set_result_value(segment, &value, wasi_type, ptr);
    // set_Value(&value, call_result_list, result_idx);
    set_CallResult(&call_result, call_result_list, result_idx);
}
