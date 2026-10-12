/* Generated from mududb/adapter/native.h by script/sdk/sync_native.py. Do not edit manually. */
/* Native adapter ABI v1 client. No static runtime link is required. */
#ifndef MUDU_ADAPTER_NATIVE_H
#define MUDU_ADAPTER_NATIVE_H
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif
typedef struct { uint8_t *data; size_t len; } mudu_native_buffer;
typedef struct {
    void *handle;
    uint32_t (*version)(void);
    int32_t (*call)(const uint8_t*,size_t,const uint8_t*,size_t,mudu_native_buffer*);
    void (*release)(mudu_native_buffer);
} mudu_native;
static inline void mudu_native_unload(mudu_native *client) {
    if (!client) return;
    if (client->handle) {
#ifdef _WIN32
        FreeLibrary((HMODULE)client->handle);
#else
        dlclose(client->handle);
#endif
    }
    free(client);
}
static inline mudu_native *mudu_native_load(const char *path) {
    if (!path || !*path) return NULL;
    mudu_native *client = (mudu_native*)calloc(1, sizeof(mudu_native));
    if (!client) return NULL;
#ifdef _WIN32
    int n = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, NULL, 0);
    wchar_t *wide = n > 0 ? (wchar_t*)calloc((size_t)n, sizeof(wchar_t)) : NULL;
    if (wide) { MultiByteToWideChar(CP_UTF8, 0, path, -1, wide, n); client->handle = (void*)LoadLibraryW(wide); free(wide); }
#define MUDU_SYM(name) GetProcAddress((HMODULE)client->handle, name)
#else
    client->handle = dlopen(path, RTLD_NOW | RTLD_LOCAL);
#define MUDU_SYM(name) dlsym(client->handle, name)
#endif
    if (!client->handle) { free(client); return NULL; }
    *(void**)(&client->version) = (void*)MUDU_SYM("mudu_adapter_abi_version");
    *(void**)(&client->call) = (void*)MUDU_SYM("mudu_adapter_call");
    *(void**)(&client->release) = (void*)MUDU_SYM("mudu_adapter_free");
#undef MUDU_SYM
    if (!client->version || !client->call || !client->release || client->version() != 1) {
        mudu_native_unload(client); return NULL;
    }
    return client;
}
static inline int mudu_native_call(mudu_native *client, const char *operation,
                                  const uint8_t *input, size_t length, mudu_native_buffer *output) {
    return client->call((const uint8_t*)operation, strlen(operation), input, length, output);
}
static inline void mudu_native_release(mudu_native *client, mudu_native_buffer output) { client->release(output); }
#endif
