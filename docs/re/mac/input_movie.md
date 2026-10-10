# Weapons cycling, input and the replay ("movie") recorder: Mac OS X vs Amiga vs our port

Sources compared:

- Mac (`analysis/ghidra_macx/decompiled.c`, the new export with switch bodies; cases mapped
  with `jumptables.tsv`):
  - `GetWeaponAmmo` @ 100f7d00 (l.95650; table 1018f6c4), `GetNextWeapon` @ 100f7de0 (l.95704;
    table 1018f6e4), `GetPrevWeapon` @ 100f7ee0 (l.95763; table 1018f71c);
  - `ButtonFromKey` @ 100f7fe0, `UpdateInputStatusNoKeys` @ 100f8090 (l.95862),
    `UpdateInputStatus` @ 100f8490 (l.96020; tables 1018f904 = binding slot, 1018f760 = fixed
    keys), `ReadFromInputSource` @ 100f93d0, `WriteControlName` @ 100f7890;
  - `ReadInput` @ 10064bf0 (l.23633), `ReadPlaybackInput` @ 10064ad0, `GetInOutVehicles` @ 10066140;
  - `WriteBits`/`ReadBits` @ 100f9700/100f97b0, `RecordMovie` @ 100f9ab0, `RewindMovie_Init`
    @ 100f9c10, `RewindMovie_SetSeed` @ 100f9d20, `StopMovie` @ 100f9df0, `GetNextFrame` @
    100f9ea0 (table 1018f9b8), `GetAbbreviatedVals` @ 100fa2b0 (table 1018f9f8), `AddFrame` @
    100fa5d0, `TidyMovie` @ 100faaa0, `LoadMovie` @ 100fa7b0, `SaveMovie` @ 100fab30, and the
    `Get*Movie` getters (l.96812-97790);
  - `PlayAnimation` @ 100fb1d0 and `InitAll` (l.23346) for the intro.
- Amiga OS4 (`analysis/ghidra_os4`): `FUN_0103d1ec` (ammo), `FUN_0103d2a8` / `FUN_0103d438`
  (next/prev), `FUN_0103da68` (pad poll), `FUN_0103f3d0` (keyboard), `FUN_01051a10` (ReadInput),
  movie `FUN_010bb9a4..FUN_010bc3b0`. 68k: `FUN_00042fcc`, `FUN_000409f2` / `FUN_00040ad2`,
  `FUN_000947e4` (writer), `FUN_0009490e` (reader).
- Port: `crates/payback/src/{multiplayer.rs, sim/people.rs, sim/combat.rs, sim/boarding.rs,
  replay.rs, intro.rs, frontend/bindings.rs}` and `crates/payback-formats/src/replay.rs`.

## Findings (differences)

### 1. Mac replay records are a compressed bit stream, not raw u16 words (Mac change; port bug in its 0x3F3 support)

- **Mac:** `AddFrame` @ 100fa5d0, `GetAbbreviatedVals` @ 100fa2b0, `GetNextFrame` @ 100f9ea0,
  `WriteBits`/`ReadBits` @ 100f9700/100f97b0, `TidyMovie` @ 100faaa0, buffer from `InitMovie` @
  100f98b0.
- **Amiga:** the OS4 writer `FUN_010bbd58` stores `word | ticks` as a raw big-endian u16, into a
  150,000-byte buffer at +0x14. The reader is `FUN_010bbcb8`; the comment skip is `FUN_010bbcf4`.
  68k is the same: writer `FUN_000947e4`, reader `FUN_0009490e`.
- **What differs:** the word layout before encoding is the same as the Amiga's: bits 14..8 for
  the flags, weapon << 3 (0x50 = none), ticks in bits 2..0 (`ReadInput` l.23790-23812). The Mac
  writes those words as the following LSB-first bit stream (CONFIRMED):
  - **Bit I/O:** `WriteBits(v, n)` stores the low `n` bits of `v`, LSB first, starting at bit 0
    of each byte. The header's byte count starts at 1 (the partial byte) and goes up by 1 every
    8 bits. `ReadBits` is the mirror.
  - **State:** `last[p] = 0xFFFFFFFF` and `lastWeapon[p] = 0` for p = 0..3 (`RecordMovie`,
    `RewindMovie_SetSeed`), one run counter `run = 0` shared by the interleaved stream, and the
    player index cycles 1..players.
  - **`AddFrame(word, ticks)`**, with `v = word | ticks`:
    1. If v's weapon field is 0x50 and `(v & ~0xF8) == (last[p] & ~0xF8)`, set `last[p] = v`
       first. The frame then counts as a repeat (the "merge").
    2. If `v == last[p]`: `run++`. When it passes 15, write 0 (4 bits) and set run = 1.
    3. Otherwise: if run > 0, write run (4 bits). Then compute
       `c = GetAbbreviatedVals(last[p], v)`. Literal: `1` (1 bit) followed by v (16 bits).
       Abbreviation: `0` followed by c (4 bits). Then `last[p] = v`, run = 1, and
       `lastWeapon[p] = weapon` if it isn't 0x50.
    4. Totals: the frame count goes up by 1 per call. When the player index wraps, the duration
       goes up by `ticks`.
    5. Writing stops silently once the stream is ≥ 99,998 bytes. The buffer is 100,000 bytes.
       The Amiga's is 150,000 bytes of raw words.
  - **`TidyMovie`:** writes the pending run, then the literal 0xFFFF, then count 1.
  - **`GetAbbreviatedVals` codes** (only one field may change, otherwise −1 = literal):

    | Code | Field | Change |
    |---|---|---|
    | 0 / 1 | steer s ∈ {−1 left 0x4000 (wins), 0, +1 right 0x2000} | +1 / −1 cyclic (±2 wraps) |
    | 2 / 3 | pedal (−1 accel 0x1000 wins, +1 reverse 0x800) | +1 / −1 cyclic |
    | 4 / 5 / 6 | 0x400 fire / 0x200 in-out / 0x100 brake | toggled |
    | 7..13 | weapon w ≠ 0x50 | `w = lastWeapon + (code − 6)`, mod 8 |
    | 14 / 15 | ticks | +1 / −1 cyclic in 1..5 (only if nothing else changed) |

  - **`GetNextFrame`:** decrement the counter. While it is ≥ 1, return `last[p] & 0xFFFF`.
    Otherwise read 1 bit:
    - 1: read a 16-bit literal;
    - 0: read a 4-bit code and apply it to `last[p]` (as in the table).

    Then update `lastWeapon[p]` if the weapon isn't 0x50, read the 4-bit count, and while the
    read is 0, add 15 and read again. Once the read position reaches the byte count, return
    0xFFFF (end).
  - **No comments:** bit 15 is no longer a comment flag. The Mac has no `InsertComment`, and
    `ReadInput` never skips text.
- **The codec is lossy in four cases (CONFIRMED by simulating the encoder and decoder above):**
  1. **The merge.** A no-change frame that follows a weapon select is encoded as a repeat. The
     decoder returns the previous word, so the select is issued again on each following frame
     until any other field changes.
  2. **Both directions held.** A word with both left and right set (or both accel and reverse)
     reached by an abbreviation decodes to left only (or accel only).
  3. **Weapon field back to 0x50 together with a ±1 tick change.** It is coded as ticks only, so
     the old select repeats.
  4. **Re-selecting the last-selected weapon together with a ±1 tick change.** It is coded as
     ticks only, so the select is lost.

  In a random-input simulation, with constant ticks, no both-held states and no merge, the
  round trip is exact.
- **Classification:** Mac change (file format). The Amiga's records are raw.
- **Port today:** `payback-formats/src/replay.rs:136-147` parses a 0x3F3 body as raw u16 words
  with bit-15 comments (l.175-197). `write()` (l.201-238) writes 0x3F3 with raw words. The doc
  comment (l.9-14) says the records are "unchanged", and so do `docs/re/frontend_menus.md:368`
  and `docs/re/mac/frontend.md:455`. As a result, a real Mac replay decodes as garbage in the
  port, and a port replay labelled 0x3F3 decodes as garbage on the Mac.
- **Recommendation:**
  - **Decoder:** implement it exactly as above for 0x3F3 files without the `PBrm` trailer (real
    Mac files). It must reproduce the lossy cases, because that is how the Mac plays its own
    files.
  - **Encoder:** write the Mac stream, but without the merge, and emit a literal whenever
    decoding the abbreviation would not give back `v`. The result is still a valid Mac stream,
    but lossless, so our own recordings stay exactly deterministic.
  - **Old port files:** flag the encoding in the trailer (e.g. flags bit 1 = Mac bit stream). A
    trailer without that bit means the old raw-word body, so the existing replay
    baselines keep loading unchanged and the replay check stays MATCH.
  - **Docs:** fix the three "unchanged" statements.
  - **Buffer:** optionally cap Mac-format recording at 99,998 encoded bytes instead of
    `MAX_RECORDS` (`crates/payback/src/replay.rs:39`).
- **Status:** CONFIRMED (code). It hasn't been tested on a real Mac file, because the Mac CD
  ships none.

### 2. Replay bit 9 (in/out) is the *held* state, but the port records and plays it as a one-tick pulse (port bug; Amiga agrees with the Mac)

- **Mac:** `ReadInput` @ 10064bf0 records `status[6]`. That is the in/out key/button *level*: the
  keyboard case 100f89a4 sets it on press and clears it on release, and `UpdateInputStatusNoKeys`
  does the same for the pad. `GetInOutVehicles` @ 10066140 detects the edge itself, through a
  per-player latch (l.24700-24711, cleared or set from the held value at l.24957).
- **Amiga:** `FUN_01051a10` records `psVar4[6]` the same way (bit 0x200).
- **Port:** `replay.rs:320` records `use_pressed` (a just-pressed pulse). `replay.rs:369` plays bit 9
  back as `use_pressed` on every record. A second consecutive `use_vehicle` cancels boarding
  (`sim/boarding.rs:175-177`). So when an original replay holds in/out for ≥ 2 frames (likely
  in the shipped v3 replays and in any Mac replay), the port starts boarding, cancels it,
  starts again, and so on, and the playback desyncs.
- **Recommendation:** on playback, set `use_pressed = bit9 && !prev_bit9[p]` (keep the previous
  bit per player across records). Our own files contain isolated pulses, so they decode the same
  and the baselines stay MATCH. Optionally record the held level instead, as the original does.
- **Status:** CONFIRMED.

### 3. Weapon select is ignored while the player is stunned (port bug; Amiga agrees with the Mac)

- **Mac:** `ReadInput` l.23880ff applies the weapon field only if `health (+0x82c) > 0 &&
  stun (+0x97c) == 0`. `+0x97c` is the 50-tick lying-down timer (it is set to 0x32 in
  `GetInOutVehicles` l.24641/24652/24910).
- **Amiga:** `FUN_01051a10` checks `+0x34c > 0 && +0x494 == 0` (OS4 stun timer, people.md table).
- **Port:** `sim/combat.rs:249` checks only `obj.health > 0`, so a digit or next/prev press while
  knocked down changes the weapon.
- **Recommendation:** add `person.stun == 0` (`Person::stun`, `sim/people.rs:69`) to the
  condition. Re-record any baseline that happens to press a weapon key while stunned. The
  current scripted input selects at `t % 400 == 100`, so run `tools/checks/replay_check.py --suite` afterwards.
- **Status:** CONFIRMED.

### 4. Digit weapon keys don't work for keyboard players in multiplayer (port gap; Amiga agrees with the Mac)

- **Mac:** `UpdateInputStatus` fixed keys 1-7 and 0 (cases 100f8c18…100f8f24) write the SP weapon
  slot and, in multiplayer, the slot of every player whose device is the keyboard (12).
- **Amiga:** `FUN_0103f3d0` does the same (`param_2[0x15+p]` for `DAT_0114b59c[p] == 0xc`).
- **Port:** `multiplayer.rs:224` (the `Device::Keyboard` branch of `read_devices`) handles only
  next/prev, and `people.rs:411` (the digit keys) returns early in multiplayer.
- **Recommendation:** apply Digit0..7 to keyboard players in `read_devices`.
- **Status:** CONFIRMED.

### 5. Mac HID next/prev-weapon latch is shared by all players (Mac bug; Amiga is per port)

- **Mac:** `UpdateInputStatusNoKeys` @ 100f8090 uses the single globals `DAT_1017d301` (next)
  and `DAT_1017d300` (prev). Multiplayer calls it once per player (`ReadInput(p)`), so another
  pad that is not pressing clears the latch, and a held button would cycle every frame.
- **Amiga:** `FUN_0103da68` compares with the previous state of that port (`DAT_012a61c0`/
  `DAT_012a61a0` + port offset).
- **Port:** `pad_just` is per device (`multiplayer.rs:258-263`), which matches the Amiga.
- **Recommendation:** keep it. Classification: Mac bug. The code is CONFIRMED; the multi-pad
  effect is INFERRED.

### 6. P also ends replay playback (minor port gap)

- **Mac:** `ReadPlaybackInput` @ 10064ad0 ends playback on `status[5]`, which is set by Esc
  (0x35) *and* P (0x23) (cases 100f8c00/100f8c0c).
- **Amiga:** the same (`FUN_0103f3d0` raw 0x45 and 0x19 → +10).
- **Port:** `replay.rs:386` checks only Escape.
- **Recommendation:** also accept P. Status: CONFIRMED.

### 7. Symbol-map corrections (`docs/xref/symbols.tsv`)

- `GetNextWeapon`/`GetPrevWeapon` are mapped to OS4 0103d9a0/0103d844 and 68k 00040de4/00040c54.
  Those are the pad/CD32 button readers. The real counterparts are OS4 `FUN_0103d2a8` /
  `FUN_0103d438` and 68k `FUN_000409f2` / `FUN_00040ad2` (they use the same switch on the
  weapon id and call `GetWeaponAmmo` inline).
- `AddFrame` → OS4 010bbcb8 / 68k 0009490e is wrong: those are the frame **reader**. The writer
  is OS4 `FUN_010bbd58` / 68k `FUN_000947e4`. The Amiga has no `GetAbbreviatedVals` or
  `WriteBits` counterpart.
- Status: CONFIRMED.

### Platform artefacts (no action)

- **Key autorepeat:** the Mac keyboard comes from `GetKeys` diffs, so it has no autorepeat.
  Amiga RAWKEY repeats aren't filtered in `FUN_0103f3d0`, so a held next-weapon key probably
  cycles at the key-repeat rate there (INFERRED). The port uses just-pressed edges, like the Mac.
- **Pads:** HID buttons and axes are digital (axis "pressed" within ±50 of the recorded extreme).
  The port's stick threshold of 0.5 is equivalent.
- **Merging keyboard and pad:** the Mac writes the keyboard by event and the pad on change, so
  "last event wins". The port ORs them (`people.rs`). This is only visible when both are used at
  once.
- **`Input_OnMenu`** only paces rendering (`ch2pl` busy-wait, `DrawObject`). It has no input
  meaning.
- **`WriteControlName`** and `ButtonFromKey`: Mac key-code names and the HID name lookup.
  `ReadFromInputSource`: the refused keys are documented in `audio_platform.md` §4.2, and the
  port already adopts them (`bindings.rs:317`).

## Checked and identical

- **`GetWeaponAmmo`** (Mac/OS4/68k):
  - slot 0 = 1;
  - slots 1..6 = pistol +0x858, shotgun +0x844, flamethrower +0x84c, grenade +0x850, minigun
    +0x854, bazooka +0x848 (OS4 +0x378/+0x364/+0x36c/+0x370/+0x374/+0x368);
  - slot 7 = 1 if a bomb is placed (+0x860), else the bomb count (+0x85c).

  This matches the port's `Arsenal::ammo` by HUD slot plus `bomb.is_some()`.
- **`GetNextWeapon`/`GetPrevWeapon`:** current weapon id → start slot.
  - Next: 0→1, 0xd→2, 5→3, 2→4, 3→5, 4→6, 1→7, anything else → 0.
  - Prev: 0→7, 5→1, 2→2, 3→3, 4→4, 1→5, 0xe→6, anything else → 0.

  Then up to 8 steps of ±1 wrapping in 0..7, stopping at the first slot with ammo ≥ 1 (the fist
  always qualifies). Mac = Amiga, and the port's `multiplayer.rs:181` `cycle_weapon` gives the
  same result for every reachable state. Ids 6..12 (non-player weapons) fall to 0 on both
  platforms; the port can't hold them.
- **Weapon apply** (`ReadInput`): the slot → id mapping, "has ammo (or a placed bomb) and isn't
  already held", `AddSound(0x14, …, 255, 14000)`, the 0x24 triggers (`ChangeWeapon` @ 100f7500)
  and the person model following the weapon (`+0x90e`). They match `combat.rs:234-262`, apart
  from finding 3.
- **Binding table order:** accelerate, reverse, left, right, shoot, shoot alt (OR-ed into fire),
  in/out, brake, next, prev, repeat. Next/prev act on key-down in single player only, and repeat
  acts on key-*release*. The pad side is edge-triggered next/prev, and repeat fires every poll
  (`frontend.md` §3.4). Mac = Amiga = port (`bindings.rs:57-58`, `people.rs:392-398`).
- **Multiplayer keyboard:** one shared movement table for every keyboard player, plus fixed
  per-key-set keys:
  - set 0: F1 in/out, F2 prev, F3 next;
  - set 1: F8/F9/F10;
  - set 2: Delete/Fwd-Del/Help;
  - set 3: keypad Enter/−/+.

  Mac = Amiga. The port's per-set rebindable tables (`bindings.rs` `MP_KEYS`) are a known
  extension (`docs/re/frontend_menus.md:311`).
- **Movie header** (`SaveMovie`/`LoadMovie`, `RecordMovie`, `RewindMovie_Init`, `StopMovie`,
  getters):
  - the word order is version, players, controllers[4], kills, frames, ticks, seed, rampage
    (`!= 0` / `== 1`), police; then the path, level & 1, the byte count and the body;
  - players = 1 in single player, where controllers and kills keep stale values (the port writes
    `[12,-1,-1,-1]` and 5, which is harmless);
  - playback swaps the multiplayer device and kill globals in and back out;
  - the seed is restored by `RewindMovie_SetSeed`;
  - `GetSeed`/`SetSeed` wrap the input read in `ReadInput`, so the pause menu doesn't use up
    random numbers.

  This matches the port apart from finding 1.
- **Playback end:** the end word 0xFFFF sets the "continue"/F1 flag and ends playback. Esc ends
  it too (see 6). Ticks come from the frame word (`*puVar11 = w & 7`).
- **Intro video:** `OpenAnimation("intro")` → `GetPaybackMovie` @ 100fcbc0 opens
  `:Videos:payback_high_mpeg1.mpg` (the only video string in `strings.tsv`). `PlayAnimation`
  @ 100fb1d0:
  - it scales the movie to fit, or adds a 1-px border at 640 wide;
  - `Button()` or `ALAnyKeyPressed` skips it;
  - it waits for the mouse button to be released, then flushes events.

  It plays at start-up from `InitAll` (l.23346) and from "View intro". This was already
  documented (`frontend.md` §9.1) and is implemented in `crates/payback/src/intro.rs` (MPEG
  first, FLC fallback, any key or click skips). No difference.
