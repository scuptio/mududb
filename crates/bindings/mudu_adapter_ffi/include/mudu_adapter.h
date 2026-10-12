/* Native adapter ABI v1. Buffers must be freed by their allocating library. */
#ifndef MUDU_ADAPTER_H
#define MUDU_ADAPTER_H
#include <stdint.h>
#include <stddef.h>
#ifdef __cplusplus
extern "C" {
#endif
typedef struct { uint8_t *data; size_t len; } mudu_adapter_buffer;
uint32_t mudu_adapter_abi_version(void);
int32_t mudu_adapter_call(const uint8_t *operation, size_t operation_len,
                        const uint8_t *input, size_t input_len, mudu_adapter_buffer *output);
void mudu_adapter_free(mudu_adapter_buffer buffer);
#ifdef __cplusplus
}
#endif
#endif
