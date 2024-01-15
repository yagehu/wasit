#include <inttypes.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdnoreturn.h>

// protobuf-c generated
#include "wazzi-executor.pb-c.h"

#define MAX_MSG_SIZE 2048

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

int main(void) {
    while (true) {
        Request * req = read_request();

        switch (req->which_case) {
            case REQUEST__WHICH_CALL: {
                fprintf(stderr, "Calling %s\n", req->call->func);

                break;
            }
            case REQUEST__WHICH__NOT_SET:
            case _REQUEST__WHICH__CASE_IS_INT_SIZE: fail("invalid request");
        }

        free_request(req);
    }
}
