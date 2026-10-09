# Conventions for the RE notes

How findings are written, cited and filed. [`README.md`](README.md) lists the notes.

## Confidence

Every statement about the original game is one of:

- **CONFIRMED** (short forms `(C)`, `[C]`, `**C**`): read in the code of at least one
  build, or verified byte for byte against the data. Give the function.
- **INFERRED** (`(I)`, `[I]`, `**I**`): consistent with the code and data but not fully
  traced. Say what it rests on, and add the question to
  [`re/open_questions.md`](re/open_questions.md) if it matters for the port.

Facts about our own code (the port) need neither tag; cite the Rust module instead.

## Citing the original

Name the function, in the most readable build you checked, and add the others when
you have them:

```
Mac DrawMap @ 1013fbd0 / OS4 FUN_01079300 / 68k FUN_000a4a68
```

- **Mac OS X** (`analysis/ghidra_macx`): the original C++ name, then `@` and the
  address: `DrawMap @ 1013fbd0`, `UCDAudio::SetVolume`, or the mangled
  `do_Person__Fv`. The Mac release is the reference for behaviour.
- **OS4** (`analysis/ghidra_os4`): `FUN_01xxxxxx`.
- **68k Update 7** (`analysis/ghidra_68k`, flat base 0x10000): `FUN_000xxxxx`, or
  `68k 0xa4a68` for an address inside a function.
- Mission scripts (`analysis/ghidra_scriptNN`): `script01:0x11704`.
- Data addresses as `DAT_…` or `0x…` with the build; struct fields as `+0x…`, saying
  whether the offset is OS4 or 68k (68k packs structs to 2 bytes, so fields past
  `+0x4909` of a model sit 2 bytes lower).

All existing forms in the notes (`FUN_01079300`, `0xb506e`, `68k 0xa4a68`,
`DrawMap @ 1013fbd0`, bare `01xxxxxx`) stay valid: `tools/reports/coverage.py`
recognises them all. There is no need to rewrite old citations.

[`xref/symbols.tsv`](xref/symbols.tsv) translates between builds
(`python tools/re/fn.py --sym FUN_01079300`). When you name a function that has no
row there yet, or find a wrong pairing, add it to the hand-written section
(`tools/re/symbols/symbols_tail.md`) or the manual anchors in
`tools/re/symbols/build_outputs.py`, and regenerate.

In Rust doc comments, cite the same way (`FUN_xxxxxxxx`, OS4 or 68k, or the Mac name
and address) and point at the note: `(docs/re/vehicles.md §2)`.

## Quoting decompiled code

The decompiles are the original's code and stay out of git. In notes and comments:

- Quote **at most a few lines**: a single expression, a condition, a constant table
  entry (`iVar41 = param_3 + 0x5dc`). Prefer your own pseudocode with the real names.
- Never paste a function body. Summarise it and cite the function instead.
- No game art (renders, screenshots of original assets) and no game text beyond
  short phrases needed to identify something.

## Adding a finding

1. Pick the note from [`README.md`](README.md) (subsystem notes in `re/`, one per
   mission in `re/missions_*.md`, Mac specifics in `re/mac/`). Add to the relevant
   section rather than starting a new file.
2. Write it with its tag and citation; say how the port follows it (module name) or
   why it doesn't.
3. Update [`re/open_questions.md`](re/open_questions.md) if the finding resolves or opens
   a question.
4. Dated reviews (a line-by-line comparison of an area at one point in time) go into
   `re/reviews/YYYY-MM-DD-topic.md`; working notes that a subsystem note summarises go
   into `re/notes/`.

## Paths in the notes

Notes cite committed files by repository path (`tools/re/fn.py`, `docs/re/people.md`)
and the regenerable exports (`analysis/ghidra_os4/decompiled.c`). Some older notes
mention working files that lived in the git-ignored `analysis/` tree
(`analysis/mac/scratch_*`, `analysis/scratch/`, per-topic dump folders, `.c` extracts,
`.txt` listings). Those were never committed and are no longer kept; the reusable
scripts among them were promoted to `tools/`. The facts in the notes do not depend on
them: every one cites the function it was read from.
