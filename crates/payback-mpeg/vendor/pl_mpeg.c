/* Compiles the pl_mpeg implementation for the payback-mpeg crate.
 * Streams are decoded from memory, so the stdio file API is left out (the
 * header also uses FILE without including <stdio.h>, which MSVC rejects). */
#define PLM_NO_STDIO
#define PL_MPEG_IMPLEMENTATION
#include "pl_mpeg.h"
