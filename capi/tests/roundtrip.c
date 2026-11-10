/*
 * Round trip through the C API, as a C program sees it: encode a synthetic image, read its
 * header, decode it two ways, and check the error codes and the ownership rules (every
 * allocation freed with the matching function, so that valgrind or AddressSanitizer find
 * nothing). Built and run by capi/tests/c_roundtrip.rs on Linux, macOS and Windows.
 *
 * The image is synthetic: never put a real fingerprint in this repository.
 */

#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "wsq.h"

static int failures = 0;

#define CHECK(cond)                                                                    \
    do {                                                                               \
        if (!(cond)) {                                                                 \
            fprintf(stderr, "%s:%d: check failed: %s\n", __FILE__, __LINE__, #cond);   \
            failures++;                                                                \
        }                                                                              \
    } while (0)

#define CHECK_STATUS(call, expected)                                                   \
    do {                                                                               \
        wsq_status status_ = (call);                                                   \
        if (status_ != (expected)) {                                                   \
            fprintf(stderr, "%s:%d: %s returned %d (%s), expected %d\n", __FILE__,     \
                    __LINE__, #call, (int)status_, wsq_status_message(status_),        \
                    (int)(expected));                                                  \
            failures++;                                                                \
        }                                                                              \
    } while (0)

#define WIDTH 256
#define HEIGHT 192

static uint8_t *make_pixels(size_t width, size_t height) {
    uint8_t *pixels = malloc(width * height);
    size_t x, y;
    if (pixels == NULL) {
        fprintf(stderr, "out of memory\n");
        exit(2);
    }
    for (y = 0; y < height; y++) {
        for (x = 0; x < width; x++) {
            /* Smooth waves plus a little texture. */
            double wave = 70.0 * sin((double)x / 9.0) * cos((double)y / 13.0);
            pixels[y * width + x] = (uint8_t)(128.0 + wave + (double)((x * 5 + y * 3) % 9));
        }
    }
    return pixels;
}

static int has_comment(const wsq_info *info, const void *payload, size_t len) {
    size_t i;
    for (i = 0; i < info->comment_count; i++) {
        if (info->comments[i].len == len && memcmp(info->comments[i].data, payload, len) == 0) {
            return 1;
        }
    }
    return 0;
}

static void test_version_and_messages(void) {
    int status;
    CHECK(wsq_version() != NULL && wsq_version()[0] != '\0');
    for (status = -3; status < 120; status++) {
        const char *message = wsq_status_message(status);
        CHECK(message != NULL && message[0] != '\0');
    }
    CHECK(WSQ_OK == 0);
    CHECK(strcmp(wsq_status_message(WSQ_OK), "success") == 0);
    CHECK(wsq_last_error_message() != NULL);
}

static void test_defaults(void) {
    wsq_encode_options e = wsq_encode_options_default();
    wsq_decode_options d = wsq_decode_options_default();
    CHECK(e.struct_size == sizeof(wsq_encode_options));
    CHECK(d.struct_size == sizeof(wsq_decode_options));
    CHECK(e.bitrate == 0.75f);
    CHECK(e.ppi == 0 && e.software == 0 && e.threads == 1);
    CHECK(e.profile == WSQ_PROFILE_CONFORMANT);
    CHECK(e.comments == NULL && e.comment_count == 0);
    CHECK(d.max_pixels == 100000000u && d.threads == 1);
}

static void test_round_trip(void) {
    static const uint8_t first[] = "scanned on site";
    static const uint8_t second[] = {'b', 'i', 'n', 0, 1, 2, 255};
    wsq_bytes comments[2];
    wsq_encode_options options = wsq_encode_options_default();
    uint8_t *pixels = make_pixels(WIDTH, HEIGHT);
    uint8_t *stream = NULL;
    size_t stream_len = 0;
    wsq_info info;
    wsq_image image;
    size_t i;
    double error = 0.0;

    comments[0].data = first;
    comments[0].len = sizeof(first) - 1;
    comments[1].data = second;
    comments[1].len = sizeof(second);
    options.ppi = 500;
    options.comments = comments;
    options.comment_count = 2;

    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, &options, &stream, &stream_len),
                 WSQ_OK);
    CHECK(stream != NULL);
    CHECK(stream_len > 100 && stream_len < (size_t)WIDTH * HEIGHT);
    CHECK(strlen(wsq_last_error_message()) == 0);
    /* Start-of-image marker, and end-of-image marker. */
    CHECK(stream[0] == 0xFF && stream[1] == 0xA0);
    CHECK(stream[stream_len - 2] == 0xFF && stream[stream_len - 1] == 0xA1);

    /* Header only. */
    memset(&info, 0x55, sizeof(info));
    CHECK_STATUS(wsq_read_info(stream, stream_len, &info), WSQ_OK);
    CHECK(info.width == WIDTH && info.height == HEIGHT);
    CHECK(info.ppi == 500 && info.software == 0);
    CHECK(info.comment_count == 3); /* NISTCOM, then ours */
    CHECK(info.comments[0].len > 8 && memcmp(info.comments[0].data, "NIST_COM", 8) == 0);
    CHECK(has_comment(&info, first, sizeof(first) - 1));
    CHECK(has_comment(&info, second, sizeof(second)));
    wsq_info_free(&info);
    CHECK(info.comments == NULL && info.comment_count == 0);
    wsq_info_free(&info); /* twice is harmless */

    /* Full decode. */
    memset(&image, 0x55, sizeof(image));
    CHECK_STATUS(wsq_decode(stream, stream_len, NULL, &image), WSQ_OK);
    CHECK(image.info.width == WIDTH && image.info.height == HEIGHT);
    CHECK(image.info.ppi == 500);
    CHECK(image.pixels != NULL && image.pixels_len == (size_t)WIDTH * HEIGHT);
    CHECK(has_comment(&image.info, first, sizeof(first) - 1));
    CHECK(has_comment(&image.info, second, sizeof(second)));
    for (i = 0; i < image.pixels_len; i++) {
        error += fabs((double)image.pixels[i] - (double)pixels[i]);
    }
    error /= (double)image.pixels_len;
    CHECK(error < 6.0); /* lossy, but close */

    /* Decode into our own buffer: same pixels. */
    {
        uint8_t *buffer = malloc(WIDTH * HEIGHT);
        wsq_decode_options d = wsq_decode_options_default();
        CHECK(buffer != NULL);
        CHECK_STATUS(wsq_decode_into(stream, stream_len, &d, buffer, WIDTH * HEIGHT, &info), WSQ_OK);
        CHECK(info.width == WIDTH && info.height == HEIGHT && info.ppi == 500);
        CHECK(memcmp(buffer, image.pixels, WIDTH * HEIGHT) == 0);
        wsq_info_free(&info);
        /* No metadata wanted. */
        memset(buffer, 0, WIDTH * HEIGHT);
        CHECK_STATUS(wsq_decode_into(stream, stream_len, NULL, buffer, WIDTH * HEIGHT, NULL),
                     WSQ_OK);
        CHECK(memcmp(buffer, image.pixels, WIDTH * HEIGHT) == 0);
        /* Wrong size. */
        CHECK_STATUS(wsq_decode_into(stream, stream_len, NULL, buffer, WIDTH * HEIGHT - 1, &info),
                     WSQ_ERR_BUFFER_SIZE);
        CHECK(info.comments == NULL && info.comment_count == 0 && info.width == 0);
        CHECK(strlen(wsq_last_error_message()) > 0);
        CHECK_STATUS(wsq_decode_into(stream, stream_len, NULL, NULL, WIDTH * HEIGHT, NULL),
                     WSQ_ERR_INVALID_ARGUMENT);
        free(buffer);
    }

    wsq_image_free(&image);
    CHECK(image.pixels == NULL && image.pixels_len == 0 && image.info.comments == NULL);
    wsq_image_free(&image); /* twice is harmless */
    wsq_free_bytes(stream, stream_len);
    free(pixels);
}

static void test_options(void) {
    uint8_t *pixels = make_pixels(WIDTH, HEIGHT);
    uint8_t *a = NULL, *b = NULL, *c = NULL;
    size_t a_len = 0, b_len = 0, c_len = 0;
    wsq_encode_options options = wsq_encode_options_default();
    wsq_info info;

    /* NULL options are the defaults. */
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, NULL, &a, &a_len), WSQ_OK);
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, &options, &b, &b_len), WSQ_OK);
    CHECK(a_len == b_len && memcmp(a, b, a_len) == 0);
    wsq_free_bytes(b, b_len);
    b = NULL;

    /* The number of threads does not change the output. */
    options.threads = 2;
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, &options, &b, &b_len), WSQ_OK);
    CHECK(a_len == b_len && memcmp(a, b, a_len) == 0);
    wsq_free_bytes(b, b_len);
    b = NULL;
    options.threads = 1;

    /* A structure from an older header that stops after `bitrate`: the later fields (here
     * the ppi) keep their defaults, and the library does not read beyond struct_size. */
    options.struct_size = offsetof(wsq_encode_options, ppi);
    options.bitrate = 2.25f;
    options.ppi = 999;
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, &options, &b, &b_len), WSQ_OK);
    CHECK(b_len > a_len); /* 2.25 bits per pixel is bigger than 0.75 */
    CHECK_STATUS(wsq_read_info(b, b_len, &info), WSQ_OK);
    CHECK(info.ppi == 0);
    wsq_info_free(&info);
    wsq_free_bytes(b, b_len);
    b = NULL;

    /* Profiles. */
    options = wsq_encode_options_default();
    options.profile = WSQ_PROFILE_OPTIMIZED;
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, &options, &b, &b_len), WSQ_OK);
    CHECK(b_len > 0);
    wsq_free_bytes(b, b_len);
    b = NULL;
    options.software = 5;
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, &options, &b, &b_len),
                 WSQ_ERR_SOFTWARE_REQUIRES_CONFORMANT);
    CHECK(b == NULL && b_len == 0);
    CHECK(strlen(wsq_last_error_message()) > 0);
    options.profile = WSQ_PROFILE_CONFORMANT;
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, &options, &c, &c_len), WSQ_OK);
    CHECK_STATUS(wsq_read_info(c, c_len, &info), WSQ_OK);
    CHECK(info.software == 5);
    wsq_info_free(&info);
    wsq_free_bytes(c, c_len);

    /* Decoding limits. */
    {
        wsq_decode_options d = wsq_decode_options_default();
        wsq_image image;
        d.max_pixels = 100;
        CHECK_STATUS(wsq_decode(a, a_len, &d, &image), WSQ_ERR_TOO_LARGE);
        CHECK(image.pixels == NULL && image.pixels_len == 0 && image.info.width == 0);
        d.max_pixels = WSQ_NO_PIXEL_LIMIT;
        CHECK_STATUS(wsq_decode(a, a_len, &d, &image), WSQ_OK);
        wsq_image_free(&image);
        d.threads = 0; /* one per core */
        CHECK_STATUS(wsq_decode(a, a_len, &d, &image), WSQ_OK);
        wsq_image_free(&image);
    }

    wsq_free_bytes(a, a_len);
    free(pixels);
}

static void test_encode_errors(void) {
    uint8_t *pixels = make_pixels(WIDTH, HEIGHT);
    uint8_t *out = (uint8_t *)pixels; /* must be reset by the call */
    size_t out_len = 77;
    wsq_encode_options options = wsq_encode_options_default();
    static uint8_t big_comment[70000];
    wsq_bytes comment;

    CHECK_STATUS(wsq_encode(pixels, 10 * 10, 10, 10, NULL, &out, &out_len),
                 WSQ_ERR_INVALID_DIMENSIONS);
    CHECK(out == NULL && out_len == 0);
    CHECK(strlen(wsq_last_error_message()) > 0);
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT - 1, WIDTH, HEIGHT, NULL, &out, &out_len),
                 WSQ_ERR_BUFFER_SIZE);
    CHECK(out == NULL && out_len == 0);

    options.bitrate = 0.0f;
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, &options, &out, &out_len),
                 WSQ_ERR_INVALID_BITRATE);
    options.bitrate = -1.0f;
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, &options, &out, &out_len),
                 WSQ_ERR_INVALID_BITRATE);
    options.bitrate = 0.75f;

    comment.data = big_comment;
    comment.len = sizeof(big_comment);
    options.comments = &comment;
    options.comment_count = 1;
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, &options, &out, &out_len),
                 WSQ_ERR_COMMENT_TOO_LONG);
    comment.data = NULL; /* NULL with a length */
    comment.len = 3;
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, &options, &out, &out_len),
                 WSQ_ERR_INVALID_ARGUMENT);
    options.comments = NULL; /* a count without an array */
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, &options, &out, &out_len),
                 WSQ_ERR_INVALID_ARGUMENT);
    options.comment_count = 0;

    options.profile = 7;
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, &options, &out, &out_len),
                 WSQ_ERR_INVALID_ARGUMENT);
    options.profile = -1;
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, &options, &out, &out_len),
                 WSQ_ERR_INVALID_ARGUMENT);
    options.profile = WSQ_PROFILE_CONFORMANT;
    options.struct_size = 1;
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, &options, &out, &out_len),
                 WSQ_ERR_INVALID_ARGUMENT);
    options.struct_size = sizeof(options);

    /* NULL pointers. */
    CHECK_STATUS(wsq_encode(NULL, WIDTH * HEIGHT, WIDTH, HEIGHT, NULL, &out, &out_len),
                 WSQ_ERR_INVALID_ARGUMENT);
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, NULL, NULL, &out_len),
                 WSQ_ERR_INVALID_ARGUMENT);
    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, NULL, &out, NULL),
                 WSQ_ERR_INVALID_ARGUMENT);
    /* Absurd length. */
    CHECK_STATUS(wsq_encode(pixels, (size_t)-1, WIDTH, HEIGHT, NULL, &out, &out_len),
                 WSQ_ERR_INVALID_ARGUMENT);

    free(pixels);
}

static void test_decode_errors(void) {
    uint8_t *pixels = make_pixels(WIDTH, HEIGHT);
    uint8_t *stream = NULL;
    size_t stream_len = 0, i;
    uint8_t *copy;
    wsq_image image;
    wsq_info info;
    static const uint8_t garbage[] = "this is not a WSQ stream at all";
    wsq_decode_options d = wsq_decode_options_default();

    CHECK_STATUS(wsq_encode(pixels, WIDTH * HEIGHT, WIDTH, HEIGHT, NULL, &stream, &stream_len),
                 WSQ_OK);

    CHECK_STATUS(wsq_decode(garbage, sizeof(garbage), NULL, &image), WSQ_ERR_INVALID_STREAM);
    CHECK(image.pixels == NULL && image.info.comments == NULL);
    CHECK(strlen(wsq_last_error_message()) > 0);
    CHECK_STATUS(wsq_read_info(garbage, sizeof(garbage), &info), WSQ_ERR_INVALID_STREAM);
    CHECK(info.comments == NULL && info.width == 0);
    CHECK_STATUS(wsq_decode(stream, 0, NULL, &image), WSQ_ERR_INVALID_STREAM); /* empty */
    CHECK_STATUS(wsq_decode(stream, stream_len / 2, NULL, &image), WSQ_ERR_INVALID_STREAM);
    CHECK_STATUS(wsq_decode(stream, 2, NULL, &image), WSQ_ERR_INVALID_STREAM);

    CHECK_STATUS(wsq_decode(NULL, 10, NULL, &image), WSQ_ERR_INVALID_ARGUMENT);
    CHECK_STATUS(wsq_decode(stream, stream_len, NULL, NULL), WSQ_ERR_INVALID_ARGUMENT);
    CHECK_STATUS(wsq_read_info(NULL, 0, &info), WSQ_ERR_INVALID_ARGUMENT);
    CHECK_STATUS(wsq_read_info(stream, stream_len, NULL), WSQ_ERR_INVALID_ARGUMENT);
    d.struct_size = 3;
    CHECK_STATUS(wsq_decode(stream, stream_len, &d, &image), WSQ_ERR_INVALID_ARGUMENT);
    CHECK(strlen(wsq_last_error_message()) > 0);
    CHECK_STATUS(wsq_decode(stream, (size_t)-1, NULL, &image), WSQ_ERR_INVALID_ARGUMENT);

    /* Damaged streams must fail cleanly or decode to something, never crash or leak. */
    copy = malloc(stream_len);
    CHECK(copy != NULL);
    for (i = 0; i < stream_len; i += 7) {
        memcpy(copy, stream, stream_len);
        copy[i] ^= 0xFF;
        d = wsq_decode_options_default();
        d.max_pixels = 10 * 1000 * 1000;
        if (wsq_decode(copy, stream_len, &d, &image) == WSQ_OK) {
            wsq_image_free(&image);
        } else {
            CHECK(image.pixels == NULL && image.info.comments == NULL);
        }
    }
    free(copy);

    /* Freeing nothing is fine. */
    wsq_free_bytes(NULL, 0);
    wsq_info_free(NULL);
    wsq_image_free(NULL);

    wsq_free_bytes(stream, stream_len);
    free(pixels);
}

int main(void) {
    test_version_and_messages();
    test_defaults();
    test_round_trip();
    test_options();
    test_encode_errors();
    test_decode_errors();
    if (failures != 0) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("wsq %s: C API round trip ok\n", wsq_version());
    return 0;
}
