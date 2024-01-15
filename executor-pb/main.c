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

static size_t read_buffer (unsigned int max_length, uint8_t * out) {
    size_t cur_len = 0;
    size_t nread;

    while ((nread = fread(out + cur_len, 1, max_length - cur_len, stdin)) != 0) {
        cur_len += nread;

        if (cur_len == max_length) fail("max message length exceeded");
    }

    return cur_len;
}

int main(void) {
    uint8_t buf[MAX_MSG_SIZE];

    while (true) {
        size_t msg_len = read_buffer(MAX_MSG_SIZE, buf);

        Request * req = request__unpack(NULL, msg_len, buf);	
        if (req == NULL) fail("failed to unpack request");

        switch (req->which_case) {
            case REQUEST__WHICH_CALL: {
                fprintf(stderr, "Calling %s\n", req->call->func);

                break;
            }
            case REQUEST__WHICH__NOT_SET:
            case _REQUEST__WHICH__CASE_IS_INT_SIZE: fail("invalid request");
        }
    }
}
