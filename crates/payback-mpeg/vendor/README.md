# pl_mpeg (vendored)

`pl_mpeg.h` is PL_MPEG by Dominic Szablewski, an MPEG-1 video, MP2 audio and
MPEG-PS demuxer in a single C header, MIT licensed (`LICENSE`; the header carries
`SPDX-License-Identifier: MIT`).

- Upstream: https://github.com/phoboslab/pl_mpeg
- Commit: `c871f2be022ece7ef4f64230b4fb8e1fb9eb6023` (2025-12-30), file unmodified.

`pl_mpeg.c` next to it compiles the implementation; `build.rs` builds it with the
`cc` crate. To update, replace `pl_mpeg.h` and record the new commit here.
