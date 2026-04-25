#pragma once
#ifdef __cplusplus
extern "C" {
#endif

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/**
 * Byte buffer returned by the Rust library.
 * Always free with bg3_free_bytes(); never free data directly.
 */
typedef struct Bg3Bytes {
    const uint8_t* data;
    size_t         len;
} Bg3Bytes;

/** Opaque handle to an open PAK package. Close with bg3pak_close(). */
typedef struct Bg3Pak_ Bg3Pak;

/** Open a PAK file. Returns NULL on error. Thread-safe. */
Bg3Pak* bg3pak_open(const char* path_utf8);

/** Close and free a PAK handle opened with bg3pak_open(). */
void bg3pak_close(Bg3Pak* pak);

/**
 * Returns true if the PAK contains at least one entry whose path begins with
 * "Localization/{language_utf8}/" (case-insensitive, backslash-normalised).
 */
bool bg3pak_has_localization(const Bg3Pak* pak, const char* language_utf8);

/**
 * Invoke callback(name, userdata) for every file entry in the PAK.
 * name is a null-terminated UTF-8 string (e.g. "Localization/Russian/strings.loca").
 * Do not call bg3pak_* functions from inside the callback.
 */
void bg3pak_for_each_file(
    const Bg3Pak*                         pak,
    void (*callback)(const char* name, void* userdata),
    void*                                 userdata
);

/**
 * Read a single file from the PAK by its internal path.
 * Backslash and forward-slash are equivalent; matching is case-insensitive.
 * Returns NULL if not found or on error. Free with bg3_free_bytes().
 */
Bg3Bytes* bg3pak_read_file(const Bg3Pak* pak, const char* path_utf8);

/**
 * Parse raw binary .loca bytes and return a Qt-compatible zlib-compressed
 * JSON object {"uuid":"text", ...}.
 *
 * Wire format: 4-byte big-endian uncompressed length + zlib-deflated data,
 * identical to Qt's qCompress() output; pass directly to qUncompress().
 *
 * Returns NULL on error or if the resource is empty. Free with bg3_free_bytes().
 */
Bg3Bytes* bg3loca_to_json_compressed(const uint8_t* data, size_t len);

/** Free a Bg3Bytes buffer returned by any bg3* function. */
void bg3_free_bytes(Bg3Bytes* bytes);

#ifdef __cplusplus
}
#endif
