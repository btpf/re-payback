/* Minimal libc for building pl_mpeg for wasm32-unknown-unknown, which has
 * no C library: malloc/realloc/free are provided by the Rust side
 * (`src/wasm_libc.rs`), abs is the compiler builtin. */
#ifndef PAYBACK_WASM_STDLIB_H
#define PAYBACK_WASM_STDLIB_H
#include <stddef.h>
void *malloc(size_t size);
void *realloc(void *ptr, size_t size);
void free(void *ptr);
#define abs(x) __builtin_abs(x)
#endif
