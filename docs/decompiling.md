# Decompiling: how `analysis/` is produced

`analysis/` is git-ignored and holds **only** regenerable Ghidra exports. Nothing
hand-made goes there; delete it at any time and run one command to get it back:

```sh
docker build -t payback-re .            # once: Ghidra 12.1.4 + tools (Dockerfile)
tools/ghidra/decompile_all.sh           # every build, ~30-60 min with 2 jobs
tools/ghidra/decompile_all.sh os4 macx  # or only some: 68k os4 macx mac9 mapedx scripts
```

Inputs come from `extracted/` (run `tools/extract/extract.py` and
`tools/extract/extract_mac.py` first, see the top-level README). Options:
`--jobs N` (parallel Ghidra runs, default 2, ~4 GB RAM each) and `--out DIR` (a
directory inside the repository, e.g. `scratch/decomp`, to diff a re-run against
the current exports before replacing them).

## What it writes

Every export has the same layout, written by `tools/ghidra/ExportAll.java`:

| File | Content |
|---|---|
| `decompiled.c` | every function, each headed `// ==== <name> @ <address> size=<bytes>` (300 s decompile timeout per function) |
| `functions.tsv` | address, name, size |
| `strings.tsv` | address, referencing functions, value |
| `jumptables.tsv` | Mac builds only: the switch tables `MacJumpTables.java` recovered |

| Directory | Binary | Recipe |
|---|---|---|
| `ghidra_68k/` | Update 7 `Payback.exe` (68k hunk exe) | `tools/extract/hunk.py --flat` lays the hunks out at 0x10000, 4 KiB aligned (`analysis/flat/Payback68k.bin` + `.map`); raw-binary import as `68000:BE:32:MC68020`; `SeedFlat68k.java` creates functions at the entry and at every `LINK A5` up to 0x117000 |
| `ghidra_script00` … `ghidra_script21/` | the legacy CD mission scripts `Maps/NN.script` (68k hunk files) | flattened the same way (`analysis/flat/scriptNN.bin`), seeded up to the end of their code hunks |
| `ghidra_os4/` | `PaybackOS4` (AmigaOS 4 ELF, PowerPC) | ELF import, language `PowerPC:BE:32:e500`. Also `long_timeout.c`: `DecompOne.java` decompiles `FUN_0105f594` (the per-object main update, which failed at the normal timeout in older exports) and `FUN_01049374` with a 30-minute timeout |
| `ghidra_macx/`, `ghidra_mac9/` | `Payback (OS X)`, `Payback (OS 9)` (PEF, PowerPC) | language forced to `PowerPC:BE:32:default` (the PEF loader guesses VLE); `MacJumpTables.java` first, then the export ([`re/mac_version.md`](re/mac_version.md) explains the jump-table pass) |
| `ghidra_mapedx/` | `MapEditor (OS X)` | as the Mac game builds |

Which build to read: the Mac OS X export keeps the original C++ function names
and is the reference for behaviour; OS4 gives the cleanest Amiga decompile; 68k
Update 7 is the build the Amiga data matches. [`sources.md`](sources.md) describes
the binaries; [`xref/symbols.tsv`](xref/symbols.tsv) maps functions between builds.

## Working with the exports

All under `tools/re/` (see `tools/README.md`); they read `analysis/` and
`extracted/`, from any working directory:

- `fn.py BUILD NAME|ADDR` prints a function (any address inside it works);
  `--grep`, `--refs REGEX`, `--callers NAME`, `--line N`; `fn.py --sym KEY`
  translates a name or address between the Mac, OS4 and 68k builds.
- `calls68k.py`, `trace68k.py`, `dis68k.py`: annotated 68k disassembly of the flat
  images, with the mission-script API names of `m68k_api.py`.
- `os4bin.py`: strings, `bl` cross-references and disassembly straight from the
  OS4 ELF.
- `symbols/`: the matcher that built `docs/xref/symbols.tsv`.

To iterate on a Ghidra script without analysing again, import once into a Docker
volume (`docker run -v payback_ghidra_proj:/proj ... analyzeHeadless /proj p_macx
-import ...`), then run `analyzeHeadless /proj p_macx -process -readOnly -noanalysis
-scriptPath /work/impl/tools/ghidra -postScript ...` against it.

## Reproducibility

Checked on 2026-10-09 with `--out scratch/...`: the six script exports came out
byte-identical to the existing ones, and two OS4 re-runs were identical to each
other (with or without `-processor`; the ELF loader picks `e500` itself). Against
the OS4 export from 2026-10-07 the re-run differs in one string reference and in
`FUN_0105f594`, which it now decompiles within the normal timeout (40,232 bytes
instead of 39,680, and a body instead of "decompile failed"). Keep
`GHIDRA_VERSION` in the `Dockerfile` pinned and re-export everything when you bump
it; line numbers in `decompiled.c` are not stable across exports, so cite
functions, never lines.

## Legal

The exports contain decompiled original code and strings. They stay in
`analysis/`, which is git-ignored. Notes quote at most a few lines; see
[`CONVENTIONS.md`](CONVENTIONS.md).
