/* Minimal libc for wasm32 (see stdlib.h): the compiler builtins lower to
 * wasm bulk-memory instructions or Rust's compiler_builtins symbols. */
#ifndef PAYBACK_WASM_STRING_H
#define PAYBACK_WASM_STRING_H
#include <stddef.h>
#define memcpy(d, s, n) __builtin_memcpy(d, s, n)
#define memmove(d, s, n) __builtin_memmove(d, s, n)
#define memset(d, c, n) __builtin_memset(d, c, n)
#endif
