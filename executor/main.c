#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdnoreturn.h>
#include <string.h>

#include "wasi/api.h"

// protobuf-c generated
#include "wazzi-executor.pb-c.h"

#include "wasi_snapshot_preview1.h"

noreturn static void fail(const char * err) {
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
    uint8_t * buf = malloc(message_size);

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

static void set_ptr_value(void * ptr, const Value * value) {
    switch (value->which_case) {
        case VALUE__WHICH_BUILTIN: {
            switch (value->builtin->which_case) {
                case VALUE__BUILTIN__WHICH_U8:
                    * (uint8_t *) ptr = (uint8_t) value->builtin->u8;
                    break;
                case VALUE__BUILTIN__WHICH_U32:
                    * (uint32_t *) ptr = value->builtin->u32;
                    break;
                case VALUE__BUILTIN__WHICH_U64:
                    * (uint64_t *) ptr = value->builtin->u64;
                    break;
                case VALUE__BUILTIN__WHICH_S64:
                    * (int64_t *) ptr = value->builtin->s64;
                    break;
                case VALUE__BUILTIN__WHICH__NOT_SET:
                case _VALUE__BUILTIN__WHICH__CASE_IS_INT_SIZE: fail("set_ptr_value: invalid builtin");
            }

            break;
        }
        case VALUE__WHICH_STRING: {
            * (char **) ptr = calloc(value->string.len, sizeof(uint8_t));
            memcpy(* (char **) ptr, value->string.data, value->string.len);

            break;
        }
        case VALUE__WHICH_BITFLAGS: {
            uint64_t val = 0;

            for (int i = 0; i < value->bitflags->n_members; i++)
                if (value->bitflags->members[i]->value)
                    val |= 0x1 << i;

            switch (value->bitflags->repr) {
                case INT_REPR__U8: * (uint8_t *) ptr = (uint8_t) val; break;
                case INT_REPR__U16: * (uint16_t *) ptr = (uint16_t) val; break;
                case INT_REPR__U32: * (uint32_t *) ptr = (uint32_t) val; break;
                case INT_REPR__U64: * (uint64_t *) ptr = val; break;
                case _INT_REPR_IS_INT_SIZE: fail("set_ptr_value: invalid bitflags repr");
            }

            break;
        }
        case VALUE__WHICH_HANDLE: * (uint32_t *) ptr = value->handle; break;
        case VALUE__WHICH_ARRAY: {
            * (void **) ptr = calloc(value->array->n_items, value->array->item_size);

            for (int i = 0; i < value->array->n_items; i++)
                set_ptr_value(
                    (* (uint8_t **) ptr) + (i * value->array->item_size),
                    value->array->items[i]
                );

            break;
        }
        case VALUE__WHICH_RECORD: {
            for (int i = 0; i < value->record->n_members; i++)
                set_ptr_value(
                    (uint8_t *) ptr + value->record->members[i]->offset,
                    value->record->members[i]->value
                );

            break;
        }
        case VALUE__WHICH_CONST_POINTER: {
            * (void **) ptr = calloc(value->const_pointer->n_items, value->const_pointer->item_size);

            for (int i = 0; i < value->const_pointer->n_items; i++)
                set_ptr_value(
                    (* (uint8_t **) ptr) + (i * value->const_pointer->item_size),
                    value->const_pointer->items[i]
                );

            break;
        }
        case VALUE__WHICH_POINTER: {
            * (void **) ptr = calloc(value->pointer->n_items, value->pointer->item_size);

            for (int i = 0; i < value->pointer->n_items; i++)
                set_ptr_value(
                    (* (uint8_t **) ptr) + (i * value->pointer->item_size),
                    value->pointer->items[i]
                );

            break;
        }
        case VALUE__WHICH_VARIANT: {
            switch (value->variant->tag_repr) {
                case INT_REPR__U8: * (uint8_t *) ptr = (uint8_t) value->variant->case_idx; break;
                case INT_REPR__U16: * (uint16_t *) ptr = (uint16_t) value->variant->case_idx; break;
                case INT_REPR__U32: * (uint32_t *) ptr = (uint32_t) value->variant->case_idx; break;
                case INT_REPR__U64: * (uint64_t *) ptr = value->variant->case_idx; break;
                case _INT_REPR_IS_INT_SIZE: fail("set_ptr_value: invalid variant tag repr");
            }

            if (
                value->variant->payload_option_case
                == VALUE__VARIANT__PAYLOAD_OPTION_PAYLOAD_SOME
            )
                set_ptr_value(
                    (uint8_t *) ptr + value->variant->payload_offset,
                    value->variant->payload_some
                );

            break;
        }
        case VALUE__WHICH__NOT_SET:
        case _VALUE__WHICH__CASE_IS_INT_SIZE: fail("set_ptr_value: invalid value");
    }
}

static void free_ptr_value(void * ptr, const Value * value) {
    switch (value->which_case) {
        case VALUE__WHICH_BUILTIN: break;
        case VALUE__WHICH_STRING: free(* (void **) ptr); break;
        case VALUE__WHICH_BITFLAGS: break;
        case VALUE__WHICH_HANDLE: break;
        case VALUE__WHICH_ARRAY: {
            for (int i = 0; i < value->array->n_items; i++)
                free_ptr_value(
                    * (uint8_t **) ptr + (i * value->array->item_size),
                    value->array->items[i]
                );

            free(ptr);

            break;
        }
        case VALUE__WHICH_RECORD: {
            for (int i = 0; i < value->record->n_members; i++)
                free_ptr_value(
                    (uint8_t *) ptr + value->record->members[i]->offset,
                    value->record->members[i]->value
                );

            break;
        }
        case VALUE__WHICH_CONST_POINTER: {
            for (int i = 0; i < value->const_pointer->n_items; i++)
                free_ptr_value(
                    * (uint8_t **) ptr + (i * value->const_pointer->item_size),
                    value->const_pointer->items[i]
                );

            free(ptr);

            break;
        }
        case VALUE__WHICH_POINTER: {
            for (int i = 0; i < value->pointer->n_items; i++)
                free_ptr_value(
                    * (uint8_t **) ptr + (i * value->pointer->item_size),
                    value->pointer->items[i]
                );

            free(ptr);

            break;
        }
        case VALUE__WHICH_VARIANT: {
            if (
                value->variant->payload_option_case
                == VALUE__VARIANT__PAYLOAD_OPTION_PAYLOAD_SOME
            )
                free_ptr_value(
                    (uint8_t *) ptr + value->variant->payload_offset,
                    value->variant->payload_some
                );

            break;
        }
        case VALUE__WHICH__NOT_SET:
        case _VALUE__WHICH__CASE_IS_INT_SIZE: fail("free_ptr_value: invalid value");
    }
}

static Value * value_new(const Value * v, const void * ptr) {
    Value * value = malloc(sizeof(Value));
    value__init(value);

    value->which_case = v->which_case;

    switch (v->which_case) {
        case VALUE__WHICH_BUILTIN: {
            value->builtin = malloc(sizeof(Value__Builtin));
            value__builtin__init(value->builtin);

            value->builtin->which_case = v->builtin->which_case;

            switch (v->builtin->which_case) {
                case VALUE__BUILTIN__WHICH_U8: value->builtin->u8 = * (uint8_t *) ptr; break;
                case VALUE__BUILTIN__WHICH_U32: value->builtin->u32 = * (uint32_t *) ptr; break;
                case VALUE__BUILTIN__WHICH_U64: value->builtin->u64 = * (uint64_t *) ptr; break;
                case VALUE__BUILTIN__WHICH_S64: value->builtin->s64 = * (int64_t *) ptr; break;
                case VALUE__BUILTIN__WHICH__NOT_SET:
                case _VALUE__BUILTIN__WHICH__CASE_IS_INT_SIZE: fail("value_new: invalid builtin value");
            }

            break;
        }
        case VALUE__WHICH_STRING: {
            uint8_t * data = calloc(v->string.len, 1);

            memcpy(data, ptr, v->string.len);

            value->string = (ProtobufCBinaryData) {
                .data = data,
                .len = v->string.len,
            };

            break;
        }
        case VALUE__WHICH_BITFLAGS: {
            value->bitflags = malloc(sizeof(Value__Bitflags));

            value__bitflags__init(value->bitflags);

            Value__Bitflags__Member ** members =
                calloc(v->bitflags->n_members, sizeof(Value__Bitflags__Member *));

            for (int i = 0; i < v->bitflags->n_members; i++) {
                members[i] = malloc(sizeof(Value__Bitflags__Member));

                value__bitflags__member__init(members[i]);

                size_t name_len = strlen(v->bitflags->members[i]->name);
                char * name = calloc(name_len + 1, sizeof(char));

                strncpy(name, v->bitflags->members[i]->name, name_len);

                bool value = false;

                switch (v->bitflags->repr) {
                    case INT_REPR__U8: value = 0x1 & (* (uint8_t *) ptr >> i); break;
                    case INT_REPR__U16: value = 0x1 & (* (uint16_t *) ptr >> i); break;
                    case INT_REPR__U32: value = 0x1 & (* (uint32_t *) ptr >> i); break;
                    case INT_REPR__U64: value = 0x1 & (* (uint64_t *) ptr >> i); break;
                    case _INT_REPR_IS_INT_SIZE: fail("value_new: invalid bitflags repr");
                }

                members[i]->name = name;
                members[i]->value = value;
            }

            value->bitflags->repr = v->bitflags->repr;
            value->bitflags->members = members;
            value->bitflags->n_members = v->bitflags->n_members;

            break;
        }
        case VALUE__WHICH_HANDLE: value->handle = * (uint32_t *) ptr; break;
        case VALUE__WHICH_ARRAY: {
            value->array = malloc(sizeof(Value__Array));
            value__array__init(value->array);

            Value ** items = calloc(v->array->n_items, sizeof(Value *));

            for (int i = 0; i < v->array->n_items; i++)
                items[i] = value_new(
                    v->array->items[i],
                    (* (uint8_t **) ptr) + i * v->array->item_size
                );

            value->array->items = items;
            value->array->n_items = v->array->n_items;
            value->array->item_size = v->array->item_size;

            break;
        }
        case VALUE__WHICH_RECORD: {
            value->record = malloc(sizeof(Value__Record));

            value__record__init(value->record);

            Value__Record__Member ** members =
                calloc(v->record->n_members, sizeof(Value__Record__Member *));

            for (int i = 0; i < v->record->n_members; i++) {
                members[i] = malloc(sizeof(Value__Record__Member));

                value__record__member__init(members[i]);

                size_t name_len = strlen(v->record->members[i]->name);
                char * name = calloc(name_len + 1, sizeof(char));

                strncpy(name, v->record->members[i]->name, name_len);

                members[i]->name = name;
                members[i]->value = value_new(
                    v->record->members[i]->value,
                    ((uint8_t *) ptr) + v->record->members[i]->offset
                );
                members[i]->offset = v->record->members[i]->offset;
            }

            value->record->members = members;
            value->record->n_members = v->record->n_members;
            value->record->size = v->record->size;

            break;
        }
        case VALUE__WHICH_CONST_POINTER: {
            value->const_pointer = malloc(sizeof(Value__Array));
            value__array__init(value->const_pointer);

            Value ** items = calloc(v->const_pointer->n_items, sizeof(Value *));

            for (int i = 0; i < v->const_pointer->n_items; i++)
                items[i] = value_new(
                    v->const_pointer->items[i],
                    (* (uint8_t **) ptr) + i * v->const_pointer->item_size
                );

            value->const_pointer->items = items;
            value->const_pointer->item_size = v->const_pointer->item_size;
            value->const_pointer->n_items = v->const_pointer->n_items;

            break;
        }
        case VALUE__WHICH_POINTER: {
            value->pointer = malloc(sizeof(Value__Array));
            value__array__init(value->pointer);

            Value ** items = calloc(v->pointer->n_items, sizeof(Value *));

            for (int i = 0; i < v->pointer->n_items; i++)
                items[i] = value_new(
                    v->pointer->items[i],
                    (* (uint8_t **) ptr) + i * v->pointer->item_size
                );

            value->pointer->items = items;
            value->pointer->item_size = v->pointer->item_size;
            value->pointer->n_items = v->pointer->n_items;

            break;
        }
        case VALUE__WHICH_VARIANT: {
            value->variant = malloc(sizeof(Value__Variant));
            value__variant__init(value->variant);

            uint64_t case_idx = 0;

            switch (v->variant->tag_repr) {
                case INT_REPR__U8: case_idx = * (uint8_t *) ptr; break;
                case INT_REPR__U16: case_idx = * (uint16_t *) ptr; break;
                case INT_REPR__U32: case_idx = * (uint32_t *) ptr; break;
                case INT_REPR__U64: case_idx = * (uint64_t *) ptr; break;
                case _INT_REPR_IS_INT_SIZE: fail("value_new: invalid variant tag repr");
            }

            switch (v->variant->payload_option_case) {
                case VALUE__VARIANT__PAYLOAD_OPTION_PAYLOAD_NONE: {
                    Empty * payload_none = malloc(sizeof(Empty));
                    empty__init(payload_none);
                    value->variant->payload_none = payload_none;
                    break;
                }
                case VALUE__VARIANT__PAYLOAD_OPTION_PAYLOAD_SOME: {
                    Value * payload_some = value_new(
                        v->variant->payload_some,
                        ((uint8_t *) ptr) + v->variant->payload_offset
                    );
                    value->variant->payload_some = payload_some;
                    break;
                }
                case VALUE__VARIANT__PAYLOAD_OPTION__NOT_SET:
                case _VALUE__VARIANT__PAYLOAD_OPTION__CASE_IS_INT_SIZE: fail("value_new: invalid variant payload option");
            }

            value->variant->case_idx = case_idx;
            value->variant->payload_option_case = v->variant->payload_option_case;
            value->variant->payload_offset = v->variant->payload_offset;
            value->variant->tag_repr = v->variant->tag_repr;
            value->variant->size = v->variant->size;

            break;
        }
        case VALUE__WHICH__NOT_SET:
        case _VALUE__WHICH__CASE_IS_INT_SIZE: fail("value_new: invalid value");
    }

    return value;
}

static void value_free(Value * value) {
    switch (value->which_case) {
        case VALUE__WHICH_BUILTIN: free(value->builtin); break;
        case VALUE__WHICH_STRING: free(value->string.data); break;
        case VALUE__WHICH_BITFLAGS: {
            for (int i = 0; i < value->bitflags->n_members; i++)
                free(value->bitflags->members[i]);

            free(value->bitflags->members);
            free(value->bitflags);

            break;
        }
        case VALUE__WHICH_HANDLE: break;
        case VALUE__WHICH_ARRAY: {
            for (int i = 0; i < value->array->n_items; i++)
                value_free(value->array->items[i]);

            free(value->array->items);
            free(value->array);

            break;
        }
        case VALUE__WHICH_RECORD: {
            for (int i = 0; i < value->record->n_members; i++) {
                value_free(value->record->members[i]->value);
                free(value->record->members[i]->name);
                free(value->record->members[i]);
            }

            free(value->record->members);
            free(value->record);

            break;
        }
        case VALUE__WHICH_CONST_POINTER: {
            for (int i = 0; i < value->const_pointer->n_items; i++)
                value_free(value->const_pointer->items[i]);

            free(value->const_pointer->items);
            free(value->const_pointer);

            break;
        }
        case VALUE__WHICH_POINTER: {
            for (int i = 0; i < value->pointer->n_items; i++)
                value_free(value->pointer->items[i]);

            free(value->pointer->items);
            free(value->pointer);

            break;
        }
        case VALUE__WHICH_VARIANT: {
            switch (value->variant->payload_option_case) {
                case VALUE__VARIANT__PAYLOAD_OPTION_PAYLOAD_NONE: {
                    free(value->variant->payload_none);
                    break;
                }
                case VALUE__VARIANT__PAYLOAD_OPTION_PAYLOAD_SOME: {
                    value_free(value->variant->payload_some);
                    break;
                }
                case VALUE__VARIANT__PAYLOAD_OPTION__NOT_SET:
                case _VALUE__VARIANT__PAYLOAD_OPTION__CASE_IS_INT_SIZE: fail("value_free: invalid variant payload option");
            }

            free(value->variant);

            break;
        }
        case VALUE__WHICH__NOT_SET:
        case _VALUE__WHICH__CASE_IS_INT_SIZE: fail("value_free: invalid value");
    }

    free(value);
}

static void * value_ptr_new(const Value * value, int32_t * len) {
    void * ptr = NULL;

    switch (value->which_case) {
        case VALUE__WHICH_BUILTIN: {
            switch (value->builtin->which_case) {
                case VALUE__BUILTIN__WHICH_U8: ptr = calloc(1, sizeof(uint8_t)); break;
                case VALUE__BUILTIN__WHICH_U32: ptr = calloc(1, sizeof(uint32_t)); break;
                case VALUE__BUILTIN__WHICH_U64: ptr = calloc(1, sizeof(uint64_t)); break;
                case VALUE__BUILTIN__WHICH_S64: ptr = calloc(1, sizeof(int64_t)); break;
                case VALUE__BUILTIN__WHICH__NOT_SET:
                case _VALUE__BUILTIN__WHICH__CASE_IS_INT_SIZE: fail("invalid builtin");
            }

            break;
        }
        case VALUE__WHICH_STRING: {
            ptr = calloc(1, sizeof(char *));
            * len = value->string.len;

            break;
        }
        case VALUE__WHICH_BITFLAGS: {
            switch (value->bitflags->repr) {
                case INT_REPR__U8: ptr = calloc(1, sizeof(uint8_t)); break;
                case INT_REPR__U16: ptr = calloc(1, sizeof(uint16_t)); break;
                case INT_REPR__U32: ptr = calloc(1, sizeof(uint32_t)); break;
                case INT_REPR__U64: ptr = calloc(1, sizeof(uint64_t)); break;
                case _INT_REPR_IS_INT_SIZE: fail("value_ptr_new: invalid int repr");
            }

            break;
        }
        case VALUE__WHICH_HANDLE: ptr = calloc(1, sizeof(int32_t)); break;
        case VALUE__WHICH_ARRAY: {
            ptr = malloc(sizeof(void *));
            * len = value->array->n_items;

            break;
        }
        case VALUE__WHICH_RECORD: ptr = calloc(1, value->record->size); break;
        case VALUE__WHICH_CONST_POINTER: ptr = calloc(1, sizeof(void *)); break;
        case VALUE__WHICH_POINTER: ptr = calloc(1, sizeof(void *)); break;
        case VALUE__WHICH_VARIANT: ptr = calloc(1, value->variant->size); break;
        case VALUE__WHICH__NOT_SET:
        case _VALUE__WHICH__CASE_IS_INT_SIZE: fail("value_ptr_new: invalid value");
    }
    if (ptr == NULL) fail("failed to allocate ptr");

    set_ptr_value(ptr, value);

    return ptr;
}

// Translate the value represented by `ptr` to a new `Value` an return it while
// freeing the old `Value` and the `ptr` itself.
// Must be paired with a call to `value_free()`.
static Value * value_ptr_free(const Value * value, void * ptr) {
    Value * v = value_new(value, ptr);

    free_ptr_value(ptr, value);
    free(ptr);

    return v;
}

#define SET_N_ALLOC(name, n) \
    n_ ## name = n; \
    name = calloc(n, sizeof(Value *));

static void handle_call(Request__Call * call) {
    Response__Call response = RESPONSE__CALL__INIT;
    Value ** params = NULL;
    Value ** results = NULL;
    size_t n_params = 0;
    size_t n_results = 0;

    response.errno_option_case = RESPONSE__CALL__ERRNO_OPTION_ERRNO_SOME;

    switch (call->func) {
        case _WASI_FUNC_IS_INT_SIZE: fail("unreachable");
        case WASI_FUNC__ARGS_GET: {
            void * p0_argv_ptr = value_ptr_new(call->params[0], NULL);
            void * p1_argv_buf_ptr = value_ptr_new(call->params[1], NULL);
            int32_t p0_argv = (int32_t) (* (void **) p0_argv_ptr);
            int32_t p1_argv_buf = (int32_t) (* (void **) p1_argv_buf_ptr);

            response.errno_some = __imported_wasi_snapshot_preview1_args_get(
                p0_argv,
                p1_argv_buf
            );

            SET_N_ALLOC(params, 2);
            SET_N_ALLOC(results, 0);

            params[1] = value_ptr_free(call->params[1], p1_argv_buf_ptr);
            params[0] = value_ptr_free(call->params[0], p0_argv_ptr);

            break;
        }
        case WASI_FUNC__ARGS_SIZES_GET: {
            void * r0_argv_size_ptr = value_ptr_new(call->results[0], NULL);
            void * r1_argv_buf_size_ptr = value_ptr_new(call->results[1], NULL);
            int32_t r0_argv_size = (int32_t) r0_argv_size_ptr;
            int32_t r1_argv_buf_size = (int32_t) r1_argv_buf_size_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_args_sizes_get(
                r0_argv_size,
                r1_argv_buf_size
            );

            SET_N_ALLOC(params, 0);
            SET_N_ALLOC(results, 2);

            results[1] = value_ptr_free(call->results[1], r1_argv_buf_size_ptr);
            results[0] = value_ptr_free(call->results[0], r0_argv_size_ptr);

            break;
        }
        case WASI_FUNC__ENVIRON_GET: {
            void * p0_environ_ptr = value_ptr_new(call->params[0], NULL);
            void * p1_environ_buf_ptr = value_ptr_new(call->params[1], NULL);
            int32_t p0_environ = * (int32_t *) p0_environ_ptr;
            int32_t p1_environ_buf = * (int32_t *) p1_environ_buf_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_environ_get(
                p0_environ,
                p1_environ_buf
            );

            SET_N_ALLOC(params, 2);
            SET_N_ALLOC(results, 0);

            params[1] = value_ptr_free(call->params[1], p1_environ_buf_ptr);
            params[0] = value_ptr_free(call->params[0], p0_environ_ptr);

            break;
        }
        case WASI_FUNC__ENVIRON_SIZES_GET: {
            void * r0_environ_size_ptr = value_ptr_new(call->results[0], NULL);
            void * r1_environ_buf_size_ptr = value_ptr_new(call->results[1], NULL);
            int32_t r0_environ_size = (int32_t) r0_environ_size_ptr;
            int32_t r1_environ_buf_size = (int32_t) r1_environ_buf_size_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_environ_sizes_get(
                r0_environ_size,
                r1_environ_buf_size
            );

            SET_N_ALLOC(params, 0);
            SET_N_ALLOC(results, 2);

            results[1] = value_ptr_free(call->results[1], r1_environ_buf_size_ptr);
            results[0] = value_ptr_free(call->results[0], r0_environ_size_ptr);

            break;
        }
        case WASI_FUNC__CLOCK_RES_GET: {
            void * p0_id_ptr = value_ptr_new(call->params[0], NULL);
            void * r0_clock_res_ptr = value_ptr_new(call->results[0], NULL);
            int32_t p0_id = * (int32_t *) p0_id_ptr;
            int32_t r0_clock_res = (int32_t) r0_clock_res_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_clock_res_get(
                p0_id,
                r0_clock_res
            );

            SET_N_ALLOC(params, 1);
            SET_N_ALLOC(results, 1);

            results[0] = value_ptr_free(call->results[0], r0_clock_res_ptr);
            params[0] = value_ptr_free(call->params[0], p0_id_ptr);

            break;
        }
        case WASI_FUNC__CLOCK_TIME_GET: {
            void * p0_id_ptr = value_ptr_new(call->params[0], NULL);
            void * p1_precision_ptr = value_ptr_new(call->params[1], NULL);
            void * r0_timestamp_ptr = value_ptr_new(call->results[0], NULL);
            int32_t p0_id = * (int32_t *) p0_id_ptr;
            int64_t p1_precision = * (int64_t *) p1_precision_ptr;
            int32_t r0_timestamp = (int32_t) r0_timestamp_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_clock_time_get(
                p0_id,
                p1_precision,
                r0_timestamp
            );

            SET_N_ALLOC(params, 2);
            SET_N_ALLOC(results, 1);

            results[0] = value_ptr_free(call->results[0], r0_timestamp_ptr);
            params[1] = value_ptr_free(call->params[1], p1_precision_ptr);
            params[0] = value_ptr_free(call->params[0], p0_id_ptr);

            break;
        }
        case WASI_FUNC__FD_ADVISE: {
            void * p0_fd_ptr = value_ptr_new(call->params[0], NULL);
            void * p1_offset_ptr = value_ptr_new(call->params[1], NULL);
            void * p2_len_ptr = value_ptr_new(call->params[2], NULL);
            void * p3_advice_ptr = value_ptr_new(call->params[3], NULL);
            int32_t p0_fd = * (int32_t *) p0_fd_ptr;
            int64_t p1_offset = * (int64_t *) p1_offset_ptr;
            int64_t p2_len = * (int64_t *) p2_len_ptr;
            int32_t p3_advice = * (uint8_t *) p3_advice_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_fd_advise(
                p0_fd,
                p1_offset,
                p2_len,
                p3_advice
            );

            SET_N_ALLOC(params, 4);
            SET_N_ALLOC(results, 0);

            params[3] = value_ptr_free(call->params[3], p3_advice_ptr);
            params[2] = value_ptr_free(call->params[2], p2_len_ptr);
            params[1] = value_ptr_free(call->params[1], p1_offset_ptr);
            params[0] = value_ptr_free(call->params[0], p0_fd_ptr);

            break;
        }
        case WASI_FUNC__FD_ALLOCATE: {
            void * p0_fd_ptr = value_ptr_new(call->params[0], NULL);
            void * p1_offset_ptr = value_ptr_new(call->params[1], NULL);
            void * p2_len_ptr = value_ptr_new(call->params[2], NULL);
            int32_t p0_fd = * (int32_t *) p0_fd_ptr;
            int64_t p1_offset = * (int64_t *) p1_offset_ptr;
            int64_t p2_len = * (int64_t *) p2_len_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_fd_allocate(
                p0_fd,
                p1_offset,
                p2_len
            );

            SET_N_ALLOC(params, 3);
            SET_N_ALLOC(results, 0);

            params[2] = value_ptr_free(call->params[2], p2_len_ptr);
            params[1] = value_ptr_free(call->params[1], p1_offset_ptr);
            params[0] = value_ptr_free(call->params[0], p0_fd_ptr);

            break;
        }
        case WASI_FUNC__FD_CLOSE: {
            void * p0_fd_ptr = value_ptr_new(call->params[0], NULL);
            int32_t p0_fd = * (int32_t *) p0_fd_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_fd_close(p0_fd);

            SET_N_ALLOC(params, 1);
            SET_N_ALLOC(results, 0);

            params[0] = value_ptr_free(call->params[0], p0_fd_ptr);

            break;
        }
        case WASI_FUNC__FD_DATASYNC: {
            void * p0_fd_ptr = value_ptr_new(call->params[0], NULL);
            int32_t p0_fd = * (int32_t *) p0_fd_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_fd_datasync(p0_fd);

            SET_N_ALLOC(params, 1);
            SET_N_ALLOC(results, 0);

            params[0] = value_ptr_free(call->params[0], p0_fd_ptr);

            break;
        }
        case WASI_FUNC__FD_FDSTAT_GET: {
            void * p0_fd_ptr = value_ptr_new(call->params[0], NULL);
            void * r0_fdstat_ptr = value_ptr_new(call->results[0], NULL);
            int32_t p0_fd = * (int32_t *) p0_fd_ptr;
            int32_t r0_fdstat = (int32_t) r0_fdstat_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_fd_fdstat_get(p0_fd, r0_fdstat);

            SET_N_ALLOC(params, 1);
            SET_N_ALLOC(results, 1);

            results[0] = value_ptr_free(call->results[0], r0_fdstat_ptr);
            params[0] = value_ptr_free(call->params[0], p0_fd_ptr);

            break;
        }
        case WASI_FUNC__FD_FDSTAT_SET_FLAGS: {
            void * p0_fd_ptr = value_ptr_new(call->params[0], NULL);
            void * p1_flags_ptr = value_ptr_new(call->params[1], NULL);
            int32_t p0_fd = * (int32_t *) p0_fd_ptr;
            int32_t p1_flags = * (uint16_t *) p1_flags_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_fd_fdstat_set_flags(
                p0_fd,
                p1_flags
            );

            SET_N_ALLOC(params, 2);
            SET_N_ALLOC(results, 0);

            params[1] = value_ptr_free(call->params[1], p1_flags_ptr);
            params[0] = value_ptr_free(call->params[0], p0_fd_ptr);

            break;
        }
        case WASI_FUNC__FD_FDSTAT_SET_RIGHTS: {
            void * p0_fd_ptr = value_ptr_new(call->params[0], NULL);
            void * p1_fs_rights_base_ptr = value_ptr_new(call->params[1], NULL);
            void * p2_fs_rights_inheriting_ptr = value_ptr_new(call->params[2], NULL);
            int32_t p0_fd = * (int32_t *) p0_fd_ptr;
            int64_t p1_fs_rights_base = * (uint64_t *) p1_fs_rights_base_ptr;
            int64_t p2_fs_rights_inheriting = * (uint64_t *) p2_fs_rights_inheriting_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_fd_fdstat_set_rights(
                p0_fd,
                p1_fs_rights_base,
                p2_fs_rights_inheriting
            );

            SET_N_ALLOC(params, 3);
            SET_N_ALLOC(results, 0);

            params[2] = value_ptr_free(call->params[2], p2_fs_rights_inheriting_ptr);
            params[1] = value_ptr_free(call->params[1], p1_fs_rights_base_ptr);
            params[0] = value_ptr_free(call->params[0], p0_fd_ptr);

            break;
        }
        case WASI_FUNC__FD_FILESTAT_GET: {
            void * p0_fd_ptr = value_ptr_new(call->params[0], NULL);
            void * r0_filestat_ptr = value_ptr_new(call->results[0], NULL);
            int32_t p0_fd = * (int32_t *) p0_fd_ptr;
            int32_t r0_filestat = (int32_t) r0_filestat_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_fd_filestat_get(p0_fd, r0_filestat);

            SET_N_ALLOC(params, 1);
            SET_N_ALLOC(results, 1);

            results[0] = value_ptr_free(call->results[0], r0_filestat_ptr);
            params[0] = value_ptr_free(call->params[0], p0_fd_ptr);

            break;
        }
        case WASI_FUNC__FD_FILESTAT_SET_SIZE: {
            void * p0_fd_ptr = value_ptr_new(call->params[0], NULL);
            void * p1_size_ptr = value_ptr_new(call->params[1], NULL);
            int32_t p0_fd = * (int32_t *) p0_fd_ptr;
            int64_t p1_size = * (int64_t *) p1_size_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_fd_filestat_set_size(p0_fd, p1_size);

            SET_N_ALLOC(params, 2);
            SET_N_ALLOC(results, 0);

            params[1] = value_ptr_free(call->params[1], p1_size_ptr);
            params[0] = value_ptr_free(call->params[0], p0_fd_ptr);

            break;
        }
        case WASI_FUNC__FD_FILESTAT_SET_TIMES: {
            void * p0_fd_ptr = value_ptr_new(call->params[0], NULL);
            void * p1_atim_ptr = value_ptr_new(call->params[1], NULL);
            void * p2_mtim_ptr = value_ptr_new(call->params[2], NULL);
            void * p3_fst_flags_ptr = value_ptr_new(call->params[3], NULL);
            int32_t p0_fd = * (int32_t *) p0_fd_ptr;
            int64_t p1_atim = * (int64_t *) p1_atim_ptr;
            int64_t p2_mtim = * (int64_t *) p2_mtim_ptr;
            int32_t p3_fst_flags = * (uint16_t *) p3_fst_flags_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_fd_filestat_set_times(
                p0_fd,
                p1_atim,
                p2_mtim,
                p3_fst_flags
            );

            SET_N_ALLOC(params, 4);
            SET_N_ALLOC(results, 0);

            params[3] = value_ptr_free(call->params[3], p3_fst_flags_ptr);
            params[2] = value_ptr_free(call->params[2], p2_mtim_ptr);
            params[1] = value_ptr_free(call->params[1], p1_atim_ptr);
            params[0] = value_ptr_free(call->params[0], p0_fd_ptr);

            break;
        }
        case WASI_FUNC__FD_PREAD: {
            int32_t p1_iovs_len = 0;
            void * p0_fd_ptr = value_ptr_new(call->params[0], NULL);
            void * p1_iovs_ptr = value_ptr_new(call->params[1], &p1_iovs_len);
            void * p2_offset_ptr = value_ptr_new(call->params[2], NULL);
            void * r0_size_ptr = value_ptr_new(call->results[0], NULL);
            int32_t p0_fd = * (int32_t *) p0_fd_ptr;
            int64_t p2_offset = * (int64_t *) p2_offset_ptr;
            int32_t r0_size = (int32_t) r0_size_ptr;

            int iovs_idx = 0;
            __wasi_size_t to_read = 0;
            __wasi_size_t n_read  = 0;


            for (int i = 0; i < p1_iovs_len; i++)
                to_read += (* (__wasi_iovec_t **) p1_iovs_ptr)[i].buf_len;

            __wasi_iovec_t iovs_curr = (* (__wasi_iovec_t **) p1_iovs_ptr)[iovs_idx];

            while (n_read < to_read) {
                response.errno_some = __imported_wasi_snapshot_preview1_fd_pread(
                    p0_fd,
                    (int32_t) &iovs_curr,
                    p1_iovs_len - iovs_idx,
                    p2_offset + n_read,
                    r0_size
                );
                if (response.errno_some != __WASI_ERRNO_SUCCESS) {
                    if (
                        response.errno_some == __WASI_ERRNO_INTR
                        || response.errno_some == __WASI_ERRNO_AGAIN
                    ) continue;

                    break;
                }

                __wasi_size_t read_this_time = * (__wasi_size_t *) r0_size_ptr;

                n_read += read_this_time;

                while (n_read < to_read && read_this_time >= iovs_curr.buf_len) {
                    read_this_time -= iovs_curr.buf_len;
                    iovs_idx += 1;
                }

                iovs_curr.buf += read_this_time;
                iovs_curr.buf_len -= read_this_time;
            }

            * (int32_t *) r0_size_ptr = n_read;

            SET_N_ALLOC(params, 3);
            SET_N_ALLOC(results, 1);

            results[0] = value_ptr_free(call->results[0], r0_size_ptr);
            params[2] = value_ptr_free(call->params[2], p2_offset_ptr);
            params[1] = value_ptr_free(call->params[1], p1_iovs_ptr);
            params[0] = value_ptr_free(call->params[0], p0_fd_ptr);

            break;
        }
        case WASI_FUNC__FD_PRESTAT_GET: {
            void * p0_fd_ptr = value_ptr_new(call->params[0], NULL);
            void * r0_prestat_ptr = value_ptr_new(call->results[0], NULL);
            int32_t p0_fd = * (int32_t *) p0_fd_ptr;
            int32_t r0_prestat = (int32_t) r0_prestat_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_fd_prestat_get(p0_fd, r0_prestat);

            SET_N_ALLOC(params, 1);
            SET_N_ALLOC(results, 1);

            results[0] = value_ptr_free(call->results[0], r0_prestat_ptr);
            params[0] = value_ptr_free(call->params[0], p0_fd_ptr);

            break;
        }
        case WASI_FUNC__FD_PRESTAT_DIR_NAME: {
            void * p0_fd_ptr = value_ptr_new(call->params[0], NULL);
            void * p1_path_ptr = value_ptr_new(call->params[1], NULL);
            void * p2_path_len_ptr = value_ptr_new(call->params[2], NULL);
            int32_t p0_fd = * (int32_t *) p0_fd_ptr;
            int32_t p1_path = * (int32_t *) p1_path_ptr;
            int32_t p2_path_len = * (int32_t *) p2_path_len_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_fd_prestat_dir_name(
                p0_fd,
                p1_path,
                p2_path_len
            );

            SET_N_ALLOC(params, 3);
            SET_N_ALLOC(results, 0);

            params[2] = value_ptr_free(call->params[2], p2_path_len_ptr);
            params[1] = value_ptr_free(call->params[1], p1_path_ptr);
            params[0] = value_ptr_free(call->params[0], p0_fd_ptr);

            break;
        }
        case WASI_FUNC__FD_READ: {
            int32_t p1_iovs_len = 0;
            void * p0_fd_ptr = value_ptr_new(call->params[0], NULL);
            void * p1_iovs_ptr = value_ptr_new(call->params[1], &p1_iovs_len);
            void * r0_size_ptr = value_ptr_new(call->results[0], NULL);
            int32_t p0_fd = * (int32_t *) p0_fd_ptr;
            int32_t r0_size = (int32_t) r0_size_ptr;

            int iovs_idx = 0;
            __wasi_size_t to_read = 0;
            __wasi_size_t n_read  = 0;


            for (int i = 0; i < p1_iovs_len; i++)
                to_read += (* (__wasi_iovec_t **) p1_iovs_ptr + i)->buf_len;

            __wasi_iovec_t iovs_curr = (* (__wasi_iovec_t **) p1_iovs_ptr)[iovs_idx];

            while (n_read < to_read) {

                response.errno_some = __imported_wasi_snapshot_preview1_fd_read(
                    p0_fd,
                    (int32_t) &iovs_curr,
                    p1_iovs_len - iovs_idx,
                    r0_size
                );
                if (response.errno_some != __WASI_ERRNO_SUCCESS) {
                    if (
                        response.errno_some == __WASI_ERRNO_INTR
                        || response.errno_some == __WASI_ERRNO_AGAIN
                    ) continue;

                    break;
                }

                __wasi_size_t read_this_time = * (__wasi_size_t *) r0_size_ptr;

                n_read += read_this_time;

                while (n_read < to_read && read_this_time >= iovs_curr.buf_len) {
                    read_this_time -= iovs_curr.buf_len;
                    iovs_idx += 1;
                }

                iovs_curr.buf += read_this_time;
                iovs_curr.buf_len -= read_this_time;
            }

            * (int32_t *) r0_size_ptr = n_read;

            SET_N_ALLOC(params, 2);
            SET_N_ALLOC(results, 1);

            results[0] = value_ptr_free(call->results[0], r0_size_ptr);
            params[1] = value_ptr_free(call->params[1], p1_iovs_ptr);
            params[0] = value_ptr_free(call->params[0], p0_fd_ptr);

            break;
        }
        case WASI_FUNC__FD_SEEK: {
            void * p0_fd_ptr = value_ptr_new(call->params[0], NULL);
            void * p1_offset_ptr = value_ptr_new(call->params[1], NULL);
            void * p2_whence_ptr = value_ptr_new(call->params[2], NULL);
            void * r0_offset_ptr = value_ptr_new(call->results[0], NULL);
            int32_t p0_fd = * (int32_t *) p0_fd_ptr;
            int64_t p1_offset = * (int64_t *) p1_offset_ptr;
            int32_t p2_whence = * (uint8_t *) p2_whence_ptr;
            int32_t r0_offset = (int32_t) r0_offset_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_fd_seek(
                p0_fd,
                p1_offset,
                p2_whence,
                r0_offset
            );

            SET_N_ALLOC(params, 3);
            SET_N_ALLOC(results, 1);

            results[0] = value_ptr_free(call->results[0], r0_offset_ptr);
            params[2] = value_ptr_free(call->params[2], p2_whence_ptr);
            params[1] = value_ptr_free(call->params[1], p1_offset_ptr);
            params[0] = value_ptr_free(call->params[0], p0_fd_ptr);

            break;
        }
        case WASI_FUNC__FD_WRITE: {
            int32_t p1_iovs_len = 0;
            void * p0_fd_ptr = value_ptr_new(call->params[0], NULL);
            void * p1_iovs_ptr = value_ptr_new(call->params[1], &p1_iovs_len);
            void * r0_size_ptr = value_ptr_new(call->results[0], NULL);
            int32_t p0_fd = * (int32_t *) p0_fd_ptr;
            int32_t r0_size = (int32_t) r0_size_ptr;

            int iovs_idx = 0;
            __wasi_size_t to_write = 0;
            __wasi_size_t written  = 0;

            for (int i = 0; i < p1_iovs_len; i++)
                to_write += (* (__wasi_ciovec_t **) p1_iovs_ptr + i)->buf_len;

            __wasi_ciovec_t iovs_curr = (* (__wasi_ciovec_t **) p1_iovs_ptr)[iovs_idx];

            while (written < to_write) {
                response.errno_some = __imported_wasi_snapshot_preview1_fd_write(
                    p0_fd,
                    (int32_t) &iovs_curr,
                    p1_iovs_len - iovs_idx,
                    r0_size
                );
                if (response.errno_some != __WASI_ERRNO_SUCCESS) {
                    if (
                        response.errno_some == __WASI_ERRNO_INTR
                        || response.errno_some == __WASI_ERRNO_AGAIN
                    ) continue;

                    break;
                }

                __wasi_size_t written_this_time = * (__wasi_size_t *) r0_size_ptr;

                written += written_this_time;

                while (written < to_write && written_this_time >= iovs_curr.buf_len) {
                    written_this_time -= iovs_curr.buf_len;
                    iovs_idx += 1;
                }

                iovs_curr.buf += written_this_time;
                iovs_curr.buf_len -= written_this_time;
            }

            * (int32_t *) r0_size_ptr = written;

            SET_N_ALLOC(params, 2);
            SET_N_ALLOC(results, 1);

            results[0] = value_ptr_free(call->results[0], r0_size_ptr);
            params[1] = value_ptr_free(call->params[1], p1_iovs_ptr);
            params[0] = value_ptr_free(call->params[0], p0_fd_ptr);

            break;
        }
        case WASI_FUNC__PATH_OPEN: {
            int32_t p2_path_len = 0;
            void * p0_fd_ptr = value_ptr_new(call->params[0], NULL);
            void * p1_dirflags_ptr = value_ptr_new(call->params[1], NULL);
            void * p2_path_ptr = value_ptr_new(call->params[2], &p2_path_len);
            void * p3_oflags_ptr = value_ptr_new(call->params[3], NULL);
            void * p4_fs_rights_base_ptr = value_ptr_new(call->params[4], NULL);
            void * p5_fs_rights_inheriting_ptr = value_ptr_new(call->params[5], NULL);
            void * p6_fdflags_ptr = value_ptr_new(call->params[6], NULL);
            void * r0_fd_ptr = value_ptr_new(call->results[0], NULL);
            int32_t p0_fd = * (int32_t *) p0_fd_ptr;
            int32_t p1_dirflags = * (int32_t *) p1_dirflags_ptr;
            int32_t p2_path = (int32_t) * (void **) p2_path_ptr;
            int32_t p3_oflags = * (int16_t *) p3_oflags_ptr;
            int64_t p4_fs_rights_base = * (int64_t *) p4_fs_rights_base_ptr;
            int64_t p5_fs_rights_inheriting = * (int64_t *) p5_fs_rights_inheriting_ptr;
            int32_t p6_fdflags = * (int16_t *) p6_fdflags_ptr;
            int32_t r0_fd = (int32_t) r0_fd_ptr;

            response.errno_some = __imported_wasi_snapshot_preview1_path_open(
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

            SET_N_ALLOC(params, 7);
            SET_N_ALLOC(results, 1);

            results[0] = value_ptr_free(call->results[0], r0_fd_ptr);
            params[6] = value_ptr_free(call->params[6], p6_fdflags_ptr);
            params[5] = value_ptr_free(call->params[5], p5_fs_rights_inheriting_ptr);
            params[4] = value_ptr_free(call->params[4], p4_fs_rights_base_ptr);
            params[3] = value_ptr_free(call->params[3], p3_oflags_ptr);
            params[2] = value_ptr_free(call->params[2], p2_path_ptr);
            params[1] = value_ptr_free(call->params[1], p1_dirflags_ptr);
            params[0] = value_ptr_free(call->params[0], p0_fd_ptr);

            break;
        }
        default: fail("func unimplemented");
    }
    
    response.params = params;
    response.results = results;
    response.n_params = n_params;
    response.n_results = n_results;

    Response msg = RESPONSE__INIT;

    msg.call = &response;
    msg.which_case = RESPONSE__WHICH_CALL;

    const size_t msg_size = response__get_packed_size(&msg);
    void * buf = malloc(msg_size);

    uint8_t size_buf[8];

    u64_to_bytes(size_buf, msg_size);
    response__pack(&msg, buf);

    size_t blks_written = fwrite(size_buf, 8, 1, stdout);
    if (blks_written != 1) fail("failed to write message size out");

    blks_written = fwrite(buf, msg_size, 1, stdout);
    if (blks_written != 1) fail("failed to write message out");

    fflush(stdout);
    free(buf);

    for (int i = 0; i < n_params; i++) value_free(params[i]);
    for (int i = 0; i < n_results; i++) value_free(results[i]);

    free(params);
    free(results);

    if (response.errno_option_case == RESPONSE__CALL__ERRNO_OPTION_ERRNO_NONE) {
        free(response.errno_none);
    }
}

int main(void) {
    while (true) {
        Request * req = read_request();

        switch (req->which_case) {
            case REQUEST__WHICH_CALL: handle_call(req->call); break;
            case REQUEST__WHICH__NOT_SET:
            case _REQUEST__WHICH__CASE_IS_INT_SIZE: fail("invalid request");
        }

        free_request(req);
    }
}
