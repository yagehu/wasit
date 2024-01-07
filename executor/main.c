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

void handle_decl(struct resource_map_entry ** resource_map, const struct DeclRequest);
void handle_call(struct resource_map_entry ** resource_map, const struct CallRequest);

void * handle_param_pre(struct resource_map_entry ** resource_map, struct ParamSpec spec, int32_t * len);
void   handle_param_post(struct ParamSpec spec, void * ptr);
void * result_pre(struct resource_map_entry ** resource_map, struct ResultSpec result);
void handle_result_post(
    struct resource_map_entry ** resource_map,
    struct capn_segment ** segment,
    struct ResultSpec spec,
    const size_t result_idx,
    Result_list result_list,
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

    return 0;
}

noreturn void fail(const char* err) {
    fprintf(stderr, "%s\n", err);
    exit(1);
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
            void *  r0_fd_ptr                   = result_pre(resource_map, r0_fd);
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

            Result_list result_list = new_Result_list(*segment, 1 /* sz */);

            handle_result_post(resource_map, segment, r0_fd, 0, result_list, r0_fd_ptr);

            call_return.which     = CallReturn_errno;
            call_return.errno     = errno;
            call_response.results = result_list;

            break;
        }
        default:
            break;
    }

    CallResponse_ptr call_response_ptr = new_CallResponse(*segment);

    write_CallResponse(&call_response, call_response_ptr);

    const int ret = capn_setp(capn_ptr, 0, call_response_ptr.p);
    if (ret != 0) fail("failed to capn_setp");

    capn_write_fd(&capn, write, out_fd, 0 /* packed */);
    capn_free(&capn);
}

void * handle_param(
    struct resource_map_entry ** resource_map,
    struct ParamSpec spec,
    int32_t * len
) {
    (void) len;

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
            const struct Value value;

            (void) value;

            fail("unimplemented: ParamSpec_value");

            break;
        }
    }

    return NULL;
}

void * result_pre(struct resource_map_entry ** resource_map, struct ResultSpec spec) {
    (void) resource_map;
    (void) spec;
    // switch (result.which) {
    //     case Result_ignore: {
    //         void * ptr = malloc(result.ignore);

    //         if (ptr == NULL) fail("failed to allocate result");

    //         return ptr;
    //     }
    //     case Result_resource: {
    //     }
    // }

    return NULL;
}

void handle_result_post(
    struct resource_map_entry ** resource_map,
    struct capn_segment ** segment,
    struct ResultSpec spec,
    const  size_t result_idx,
    Result_list result_list,
    void * ptr
) {
    (void) resource_map;
    (void) segment;
    (void) spec;
    (void) result_idx;
    (void) result_list;
    (void) ptr;
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
    // struct Value value;

    // set_result_value(segment, &value, wasi_type, ptr);
    // set_Value(&value, result_list, result_idx);
}
