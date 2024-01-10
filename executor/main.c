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
void * malloc_set_value(
    struct resource_map_entry ** resource_map,
    const struct Type,
    const struct Value,
    int32_t * len
);
void set_ptr_value_no_alloc(
    struct resource_map_entry ** resource_map,
    const struct Type,
    const struct Value,
    void * ptr
);

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

size_t type_size(struct Type type);

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

    return 0;
}

noreturn void fail(const char* err) {
    fprintf(stderr, "%s\n", err);
    exit(1);
}

void * malloc_set_value(
    struct resource_map_entry ** resource_map,
    const struct Type type,
    const struct Value value,
    int32_t * len
) {
    void * ptr = NULL;

    switch (type.which) {
        case Type_builtin: {
            struct Type_Builtin builtin_type;

            read_Type_Builtin(&builtin_type, type.builtin);

            switch (builtin_type.which) {
                case Type_Builtin__char: ptr = malloc(sizeof(uint8_t));  break;
                case Type_Builtin_s8:    ptr = malloc(sizeof(int8_t));   break;
                case Type_Builtin_s16:   ptr = malloc(sizeof(int16_t));  break;
                case Type_Builtin_s32:   ptr = malloc(sizeof(int32_t));  break;
                case Type_Builtin_s64:   ptr = malloc(sizeof(int64_t));  break;
                case Type_Builtin_u8:    ptr = malloc(sizeof(uint8_t));  break;
                case Type_Builtin_u16:   ptr = malloc(sizeof(uint16_t)); break;
                case Type_Builtin_u32:   ptr = malloc(sizeof(uint32_t)); break;
                case Type_Builtin_u64:   ptr = malloc(sizeof(uint64_t)); break;
            }

            break;
        }
        case Type_string: {
            ptr = malloc(value.string.len);
            * len = value.string.len;

            break;
        }
        case Type_bitflags: {
            struct Type_Bitflags bitflags_type;

            read_Type_Bitflags(&bitflags_type, type.bitflags);

            switch (bitflags_type.repr) {
                case Type_IntRepr_u8:  ptr = malloc(sizeof(uint8_t));  break;
                case Type_IntRepr_u16: ptr = malloc(sizeof(uint16_t)); break;
                case Type_IntRepr_u32: ptr = malloc(sizeof(uint32_t)); break;
                case Type_IntRepr_u64: ptr = malloc(sizeof(uint64_t)); break;
            }

            break;
        }
        case Type_handle: ptr = malloc(sizeof(int32_t)); break;
        case Type_array: {
            struct Type_Array  array_type;
            struct Value_Array array_value;

            read_Type_Array(&array_type, type.array);
            read_Value_Array(&array_value, value.array);

            fprintf(stderr, "Type_array %d %d\n", capn_len(array_value.items), array_type.itemSize);
            ptr = malloc(capn_len(array_value.items) * array_type.itemSize);
            * len = capn_len(array_value.items);

            break;
        }
        case Type_record: {
            struct Type_Record record_type;

            read_Type_Record(&record_type, type.record);

            ptr = malloc(record_type.size);

            break;
        }
        case Type_constPointer: fail("malloc_set_value constPointer"); break;
        case Type_pointer: ptr = malloc(sizeof(int32_t)); break;
    }

    if (ptr == NULL) fail("failed to alloc");

    set_ptr_value_no_alloc(resource_map, type, value, ptr);

    return ptr;
}

void set_ptr_value_no_alloc(
    struct resource_map_entry ** resource_map,
    struct Type type,
    struct Value value,
    void * ptr
) {
    switch (value.which) {
        case Value_builtin: {
            struct Value_Builtin builtin_value;

            read_Value_Builtin(&builtin_value, value.builtin);

            switch (builtin_value.which) {
                case Value_Builtin__char: * (uint8_t *)  ptr = builtin_value._char; break;
                case Value_Builtin_s8:    * (int8_t *)   ptr = builtin_value.s8;    break;
                case Value_Builtin_s16:   * (int16_t *)  ptr = builtin_value.s16;   break;
                case Value_Builtin_s32:   * (int32_t *)  ptr = builtin_value.s32;   break;
                case Value_Builtin_s64:   * (int64_t *)  ptr = builtin_value.s64;   break;
                case Value_Builtin_u8:    * (uint8_t *)  ptr = builtin_value.u8;    break;
                case Value_Builtin_u16:   * (uint16_t *) ptr = builtin_value.u16;   break;
                case Value_Builtin_u32:   * (uint32_t *) ptr = builtin_value.u32;   break;
                case Value_Builtin_u64:   * (uint64_t *) ptr = builtin_value.u64;   break;
            }

            break;
        }
        case Value_string: strncpy(ptr, value.string.str, value.string.len); break;
        case Value_bitflags: {
            struct Value_Bitflags bitflags_value;
            struct Type_Bitflags  bitflags_type;

            read_Value_Bitflags(&bitflags_value, value.bitflags);
            read_Type_Bitflags(&bitflags_type, type.bitflags);

            uint64_t bitflags_int = 0;

            for (int i = 0; i < capn_len(bitflags_value.members); i++) {
                bool is_set = capn_get1(bitflags_value.members, i);

                bitflags_int |= is_set << i;
            }

            switch (bitflags_type.repr) {
                case Type_IntRepr_u8:  * (uint8_t *)  ptr = (uint8_t)  bitflags_int; break;
                case Type_IntRepr_u16: * (uint16_t *) ptr = (uint16_t) bitflags_int; break;
                case Type_IntRepr_u32: * (uint32_t *) ptr = (uint32_t) bitflags_int; break;
                case Type_IntRepr_u64: * (uint64_t *) ptr = (uint64_t) bitflags_int; break;
            }

            break;
        }
        case Value_handle: * (uint32_t *) ptr = value.handle; break;
        case Value_array: {
            struct Value_Array array_value;
            struct Type_Array  array_type;
            struct Type        item_type;

            read_Value_Array(&array_value, value.array);
            read_Type_Array(&array_type, type.array);
            read_Type(&item_type, array_type.item);

            for (int i = 0; i < capn_len(array_value.items); i++) {
                struct ParamSpec item_spec;

                get_ParamSpec(&item_spec, array_value.items, i);

                void * element_ptr = (uint8_t *) ptr + (array_type.itemSize * i);

                switch (item_spec.which) {
                    case ParamSpec_resource: {
                        struct ResourceRef resource_ref;

                        read_ResourceRef(&resource_ref, item_spec.resource);

                        struct resource_map_entry * resource_entry =
                            hmgetp_null(*resource_map, resource_ref.id);
                        if (resource_entry == NULL) fail("array element resource not found");

                        memcpy(element_ptr, resource_entry->value.ptr, resource_entry->value.size);

                        break;
                    }
                    case ParamSpec_value: {
                        struct Value item_value;

                        read_Value(&item_value, item_spec.value);
                        set_ptr_value_no_alloc(resource_map, item_type, item_value, element_ptr);

                        break;
                    }
                }
            }

            break;
        }
        case Value_record: {
            struct Value_Record record_value;
            struct Type_Record  record_type;

            read_Value_Record(&record_value, value.record);
            read_Type_Record(&record_type, type.record);

            for (int i = 0; i < capn_len(record_type.members); i++) {
                struct Type_Record_Member record_member_type;
                struct ParamSpec          record_member;

                get_Type_Record_Member(&record_member_type, record_type.members, i);
                get_ParamSpec(&record_member, record_value.members, i);

                void * member_ptr = (uint8_t *) ptr + record_member_type.offset;

                switch (record_member.which) {
                    case ParamSpec_resource: {
                        struct ResourceRef resource_ref;

                        read_ResourceRef(&resource_ref, record_member.resource);

                        struct resource_map_entry * resource_entry =
                            hmgetp_null(*resource_map, resource_ref.id);
                        if (resource_entry == NULL) fail("record member resource not found");

                        memcpy(member_ptr, resource_entry->value.ptr, resource_entry->value.size);

                        break;
                    }
                    case ParamSpec_value: {
                        struct Value record_member_value;
                        struct Type  record_member_type;

                        read_Value(&record_member_value, record_member.value);
                        read_Type(&record_member_type, record_member.type);
                        set_ptr_value_no_alloc(resource_map, record_member_type, record_member_value, member_ptr);

                        fprintf(stderr, "record %d = %d\n", (int32_t) member_ptr, * (int32_t *) member_ptr);

                        if (* (int32_t *) member_ptr != 2) {
                            fprintf(stderr, "printf %s\n", * (char **) member_ptr);
                        }

                        break;
                    }
                }
            }

            break;
        }
        case Value_constPointer: {
            struct Type element_type;

            read_Type(&element_type, type.constPointer);

            const size_t element_size = type_size(element_type);
            void * elements = malloc(capn_len(value.constPointer) * element_size);

            for (int i = 0; i < capn_len(value.constPointer); i++) {
                struct Value element_value;

                get_Value(&element_value, value.constPointer, i);

                void * element_ptr = (uint8_t *) elements + i * element_size;

                set_ptr_value_no_alloc(resource_map, element_type, element_value, element_ptr);
            }

            * (int32_t *) ptr = (int32_t) elements;

            break;
        }
        case Value_pointer: {
            struct Value_Pointer       pointer_value;
            struct Value_Pointer_Alloc alloc;

            read_Value_Pointer(&pointer_value, value.pointer);
            read_Value_Pointer_Alloc(&alloc, pointer_value.alloc);

            struct resource_map_entry * resource_entry = hmgetp_null(*resource_map, alloc.resourceId);
            if (resource_entry == NULL) fail("pointer resource not found");

            * (int32_t *) ptr = (int32_t) malloc(* (uint32_t *) resource_entry->value.ptr);

            break;
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
        case Type_pointer: fail("unimplemented: pointer type result");
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
        case Value_string:
        case Value_array:
        case Value_record:
        case Value_constPointer:
        case Value_pointer:
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
        case Func_argsGet: {
            struct ParamSpec p0_argv;
            struct ParamSpec p1_argv_buf;

            get_ParamSpec(&p0_argv, call.params, 0);
            get_ParamSpec(&p1_argv_buf, call.params, 1);

            void * p0_argv_ptr     = handle_param_pre(resource_map, p0_argv, NULL);
            void * p1_argv_buf_ptr = handle_param_pre(resource_map, p1_argv_buf, NULL);
            int32_t p0_argv_       = * (int32_t *) p0_argv_ptr;
            int32_t p1_argv_buf_   = * (int32_t *) p1_argv_buf_ptr;

            int32_t errno = __imported_wasi_snapshot_preview1_args_get(p0_argv_,  p1_argv_buf_);

            CallResult_list call_result_list = new_CallResult_list(*segment, 0 /* sz */);

            call_return.which     = CallReturn_errno;
            call_return.errno     = errno;
            call_response.results = call_result_list;

            break;
        }
        case Func_argsSizesGet: {
            struct ResultSpec r0_argv_size;
            struct ResultSpec r1_argv_buf_size;

            get_ResultSpec(&r0_argv_size, call.results, 0);
            get_ResultSpec(&r1_argv_buf_size, call.results, 1);

            void * r0_argv_size_ptr      = handle_result_pre(resource_map, r0_argv_size);
            void * r1_argv_buf_size_ptr  = handle_result_pre(resource_map, r1_argv_buf_size);
            int32_t r0_argv_size_        = (int32_t) r0_argv_size_ptr;
            int32_t r1_argv_buf_size_    = (int32_t) r1_argv_buf_size_ptr;

            int32_t errno = __imported_wasi_snapshot_preview1_args_sizes_get(r0_argv_size_, r1_argv_buf_size_);

            CallResult_list call_result_list = new_CallResult_list(*segment, 2 /* sz */);

            handle_result_post(resource_map, segment, r0_argv_size, 0, call_result_list, r0_argv_size_ptr);
            handle_result_post(resource_map, segment, r1_argv_buf_size, 1, call_result_list, r1_argv_buf_size_ptr);

            call_return.which     = CallReturn_errno;
            call_return.errno     = errno;
            call_response.results = call_result_list;

            break;
        }
        case Func_environGet: {
            struct ParamSpec p0_environ;
            struct ParamSpec p1_environ_buf;

            get_ParamSpec(&p0_environ, call.params, 0);
            get_ParamSpec(&p1_environ_buf, call.params, 1);

            void * p0_environ_ptr     = handle_param_pre(resource_map, p0_environ, NULL);
            void * p1_environ_buf_ptr = handle_param_pre(resource_map, p1_environ_buf, NULL);
            int32_t p0_environ_       = * (int32_t *) p0_environ_ptr;
            int32_t p1_environ_buf_   = * (int32_t *) p1_environ_buf_ptr;

            int32_t errno = __imported_wasi_snapshot_preview1_environ_get(p0_environ_,  p1_environ_buf_);

            CallResult_list call_result_list = new_CallResult_list(*segment, 0 /* sz */);

            call_return.which     = CallReturn_errno;
            call_return.errno     = errno;
            call_response.results = call_result_list;

            break;
        }
        case Func_environSizesGet: {
            struct ResultSpec r0_environ_size;
            struct ResultSpec r1_environ_buf_size;

            get_ResultSpec(&r0_environ_size, call.results, 0);
            get_ResultSpec(&r1_environ_buf_size, call.results, 1);

            void *  r0_environ_size_ptr      = handle_result_pre(resource_map, r0_environ_size);
            void *  r1_environ_buf_size_ptr  = handle_result_pre(resource_map, r1_environ_buf_size);
            int32_t r0_environ_size_         = (int32_t) r0_environ_size_ptr;
            int32_t r1_environ_buf_size_     = (int32_t) r1_environ_buf_size_ptr;

            int32_t errno = __imported_wasi_snapshot_preview1_environ_sizes_get(r0_environ_size_, r1_environ_buf_size_);

            CallResult_list call_result_list = new_CallResult_list(*segment, 2 /* sz */);

            handle_result_post(resource_map, segment, r0_environ_size, 0, call_result_list, r0_environ_size_ptr);
            handle_result_post(resource_map, segment, r1_environ_buf_size, 1, call_result_list, r1_environ_buf_size_ptr);

            call_return.which     = CallReturn_errno;
            call_return.errno     = errno;
            call_response.results = call_result_list;

            break;
        }
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
            int32_t p1_iovs_     = (int32_t) p1_iovs_ptr;
            int32_t r0_size_     = (int32_t) r0_size_ptr;

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

            fprintf(stderr, "%s %d\n", (char *) p2_path_ptr, * (int32_t *) p3_oflags_);
            fprintf(stderr, "path_open() ret %d %d\n", errno, * (int32_t *) r0_fd_ptr);

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
        default: fail("unimplemented func"); break;
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

            return malloc_set_value(resource_map, type, value, len);
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
        case Type_pointer: fail("result cannot be pointer");
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
            const size_t size = type_size(type);
            const struct resource resource = {
                .ptr  = ptr,
                .size = size,
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

size_t type_size(const struct Type type) {
    switch (type.which) {
        case Type_builtin: {
            struct Type_Builtin builtin_type;

            read_Type_Builtin(&builtin_type, type.builtin);

            switch (builtin_type.which) {
                case Type_Builtin__char: return sizeof(uint8_t);
                case Type_Builtin_u8:    return sizeof(uint8_t);
                case Type_Builtin_u16:   return sizeof(uint16_t);
                case Type_Builtin_u32:   return sizeof(uint32_t);
                case Type_Builtin_u64:   return sizeof(uint64_t);
                case Type_Builtin_s8:    return sizeof(int8_t);
                case Type_Builtin_s16:   return sizeof(int16_t);
                case Type_Builtin_s32:   return sizeof(int32_t);
                case Type_Builtin_s64:   return sizeof(int64_t);
            }
        }
        case Type_string: fail("string type does not have a size");
        case Type_bitflags: {
            struct Type_Bitflags bitflags_type;

            read_Type_Bitflags(&bitflags_type, type.bitflags);

            switch (bitflags_type.repr) {
                case Type_IntRepr_u8:  return sizeof(uint8_t);
                case Type_IntRepr_u16: return sizeof(uint16_t);
                case Type_IntRepr_u32: return sizeof(uint32_t);
                case Type_IntRepr_u64: return sizeof(uint64_t);
            }
        }
        case Type_handle: return sizeof(int32_t);
        case Type_array: fail("array type does not have a size");
        case Type_record: {
            struct Type_Record record_type;

            read_Type_Record(&record_type, type.record);

            return record_type.size;
        }
        case Type_constPointer: return sizeof(uint32_t);
        case Type_pointer: return sizeof(uint32_t);
    }
}
