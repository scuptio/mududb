/*
 * Default libc-backed implementation of the `mp_realloc` allocation hook
 * declared in mpack.h. Replace this translation unit to retarget the
 * writer/arena allocations (e.g. to a static pool or a logging allocator).
 */
#include "mpack.h"

#include <stdlib.h>

void *mp_realloc(void *old_ptr, size_t old_size, size_t new_size) {
    (void)old_size;
    if (new_size == 0) {
        free(old_ptr);
        return 0;
    }
    return realloc(old_ptr, new_size);
}
