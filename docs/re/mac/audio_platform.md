# Mac release: audio, music, intro video and platform layer

Comparison of the Mac OS X build (`analysis/ghidra_macx`, original C++ names) with the
Amiga builds (OS4 `analysis/ghidra_os4`, 68k `analysis/ghidra_68k`) and with our
implementation (`crates/payback/src/audio.rs`, `intro.rs`). Amiga facts come from
`docs/re/frontend_audio.md` §4–§5 unless stated otherwise.

Confidence labels: **C** = CONFIRMED (read in the decompile or disassembly, address given),
**I** = INFERRED.

**Policy (from the lead):** the Mac release is treated as **more definitive** than the Amiga
builds. Every recommendation below therefore **adopts the Mac behaviour by default** wherever it
differs. The exceptions are clear Mac-platform artefacts: behaviour that comes from the Sound
Manager, QuickTime, cddafs or Carbon plumbing itself rather than from the game code. Each
exception is marked **[artefact]** with the reason. Anything touching simulation timing is
checked against replay determinism (§4.1). The consolidated decision list is §6.

The review's working files (`analysis/mac/scratch_audio/`, local, not kept; `tools/re/fn.py`
replaces its `fn.py`):

- `fn.py`: function extractor (`SRC=os4|68k|macx python fn.py <name|addr>...`).
- `DoAudio.c`, `DoAudioMulti.c`, `os4_FUN_01007290.c`, `levelloop.c`, `gl.c`, `do_Other.c`,
  `os4_showoptions.c`: dumped functions.
- `sndcallback.txt`: disassembly of the Sound Manager callback that Ghidra missed.
- `keyhandlers.txt`: the in-game key handlers.
- `samples.tsv`: the sample tables of both builds compared.
- `mac_intro_contact*.png`: frames from the Mac intro.

PEF data and constants were read with `tools/re/symbols/pef.py`. The TOC base
(r2) is `0x10183440`.

---

## 0. Summary

| Area | Amiga (Update 7 / OS4) | Mac OS X | Impact on us |
|---|---|---|---|
| Mixer | Software mixer, 6 voices summed into 4 Paula channels at period 163 (21,760 Hz real, 22,050 nominal) | Same game-side voice logic (`DoAudio` is a line-for-line port of `FUN_01007290`). The mixing itself is handed to the Sound Manager: 6 `SndChannel`s, one voice each | `audio.rs` design is right. Our 22,050 Hz rate and exact-Hz playback equal the Mac |
| Interpolation | None (step table) | None. Channels are opened with `initNoInterp` (C) | Keep nearest-sample resampling |
| Pitch | Rate byte = Hz/200 (200 Hz steps), Doppler ±, 1..255 | Identical. Hz = byte×200 is sent as a rate multiplier against a 22,050 Hz header | **We do not quantise** non-Doppler sounds: fix |
| Pan | `r = dx/|dy|` in **floating point**; far side `= min(A_unclamped/(|r|+1), 255)` | Identical (C) | **`audio.rs` pan is wrong** (integer `r`, clamped A): fix |
| Volume | `vt = s·A/768`; hardware volume `FX·{31,63}/100`; quiet voices doubled into 31-groups | Per-voice volume `FX·A·65536/1e7` (0.653 of full at FX 100, A 255). Quiet-voice doubling kept, but the group volume is **never applied** → quiet voices +6 dB | **Adopt** the Mac law and default FX 50. The +6 dB quirk is an [artefact] (§1.5) |
| One-shot lifetime | Slot clock runs (735t+399)/735t too fast → audible chunk skipping | Same slot clock, but playback is continuous → one-shots **cut off at ≈65 %** of their length at 30 fps | **Adopt** the Mac cut (§1.9) |
| Samples | 77 headerless s8 files; CarHorn read with 20 bytes too many | Byte-identical files; CarHorn length fixed (734) | No change; we already read the file size |
| Menu sounds | Paula period, PAL clock 3,546,895 | Period → Hz with the **NTSC** clock 3,579,545; mixed with game voices | **Adopt** the NTSC clock and mixing |
| Pause | Paula output stopped | Channels paused (`pauseCmd`) and resumed | **Adopt**: pause/resume the mixer |
| Music | Red Book CD audio via SCSI thread (OS4) or external handler (68k) | CD tracks played as QuickTime movies from the mounted audio-CD volume (cddafs AIFF). Volume is tied to the FX slider. Continuous mode has no random start and skips every other track (I) | **Adopt** the Mac game-side logic (slider crossfade, sequential continuous mode, no duck); the track skipping is an [artefact] |
| Intro | 30 s in-engine FLC, 320×200, 15 fps, s8 22,050 Hz | **A different film**: 72 s pre-rendered MPEG-1 at 640×480, 25 fps, MP2 48 kHz mono, via QuickTime. Hard-coded `payback_high_mpeg1.mpg`; the low/MPEG-2 files are never used | **Adopt**: Mac film by default when the Mac data is present, FLC fallback |
| Game loop | 68k: 25 Hz (DateStamp/2); OS4: 30 Hz | 30 Hz: `TickCount()` (60 Hz) / 2, same limiter, ticks 1..5; menus capped at 60 fps; one render per sim step; no interpolation; single-threaded | **Adopt 30 Hz** (replay-safe, §4.1) |
| Input | Amiga raw keys, lowlevel joysticks (Joystick/PSX/CD32) | Keyboard polled with `GetKeys` and diffed into fake `IDCMP_RAWKEY` messages with Mac key codes; up to 4 HID gamepads (HID Utilities, digital only); PSX/CD32 removed | **Adopt** the Mac defaults, unbound value, refusal list and digital pads |
| Display | Warp3D / software chunky | AGL double-buffered RGBA, no swap interval (no vsync), DrawSprocket fullscreen 640×480 / 800×600 / 1024×768, window mode same sizes | Mostly [artefact]; window/fullscreen options are already in our graphics options |

---

## 1. Sound engine

### 1.1 Architecture (C)

The Mac keeps the whole Amiga game-side sound layer and replaces only the bottom layer, the
mixer and Paula output.

| Layer | Amiga (OS4) | Mac | Notes |
|---|---|---|---|
| InitAudio | `FUN_0100622c` | `InitAudio__Fv` @ `100f2320` | 500 slots of 64 bytes, all set to −1; no chip buffers |
| InitSound | `FUN_01005864` | `InitSound__Fv` @ `100f1c30` | `CMSOpenSoundTool(6)` → 6 channels, then 77 `LoadSound` calls |
| LoadSample | `FUN_01005748(path,len)` | `LoadSound__FPcilb(path,len,rateFixed,0)` @ `100f1b40` → `GetPaybackSound` @ `100fcd90` + `CMSRegisterSound` @ `10155f10` | See §1.7 |
| AddSound | `FUN_0100815c` | `AddSound__Ficiiiii` @ `100f4000` | Identical. Same slot layout, `Hz/200`, `>>4`, prevDist −1, d² 1, split-screen fields, "Too many sounds" |
| ModifySound | `FUN_010082bc` | `ModifySound__Ficiiiii` @ `100f4180` | Identical |
| DeleteSound | `FUN_01008398` | `DeleteSound__Fi` @ `100f42a0` | Also `CMSQuietSound` on the slot's channel |
| get/set sample | `FUN_0100812c` / `FUN_01008144` | `Sound_GetSample` @ `100f3f40` / `Sound_SetSample` @ `100f3f80` | The Mac set also quiets the channel (§1.8) |
| UpdateSound (1 listener) | `FUN_01007290` | `DoAudio__Fiiii` @ `100f32f0` | Same selection, pan, Doppler and position code, then CMS calls instead of building voice tables |
| UpdateSound (4 listeners) | `FUN_01006750` | `DoAudioMulti__FiPiPiPiPi` @ `100f2610` | Same structure |
| mix / submit | `FUN_01004f54` / `FUN_01007d10` | `UpdateAudio__Fi` @ `100f3f10` is **empty** | The Sound Manager mixes |
| menu one-shot | `FUN_010053c4(id, period)` | `PlaySound__Fii(id, period)` @ `100f1960` | §1.10 |
| stop all | `FUN_010050a4` / `FUN_01005294` | `StopAllSounds` @ `100f17c0`, `StopSound` @ `100f18f0` (ignores its argument and stops all 6 channels), `ResetSound` @ `100f1670` | |
| FX volume get/set | `FUN_01006558` / `FUN_01006564` | `GetSoundVolume` @ `100f24b0` / `SetSoundVolume` @ `100f24f0` | §1.6. The Mac also sets the CD volume here |
| pause/resume | none (Paula stopped) | `PauseAllSounds` @ `100f1810`, `ResumeAllSounds` @ `100f1880` → `CMSPauseSound`/`CMSResumeSound` (`pauseCmd` 11 / `resumeCmd` 12) | Used by `ShowOptions` (the pause menu) |

The caller is identical too. `DrawView__Fv` @ `1006eb80` calls
`DoAudio(ticks*735 − 1, listener x, y, z)` (C, decompile line 28252), with the camera case
`(camX>>5, camY>>5, 22000)`, exactly as on the Amiga.

### 1.2 The CMS library (C)

The CMS library is a generic Sound Manager wrapper. The "PaybackSFX" entry points were
added for this game. Its global block is 0x358 bytes (`AllocateSoundSystem` @ `10155730`):

- channel pointers at +4;
- per-channel priority (short) at +0x1C;
- registered sound handles at +0x2C (100 of them);
- sound header pointers at +0x1BC.

Each channel record is 0x438 bytes: `SndChannel` + 0x424 playing sound id (−1 = idle),
+0x426 open, +0x428 locked handle, +0x42C loop flag, +0x42E/+0x430 left/right volume,
+0x434 last frequency.

| Function | Address | What it does |
|---|---|---|
| `CMSOpenSoundTool(n)` | `101556b0` | Clamps n to 1..6, calls `CMSInitSound(n)` and opens all channels |
| `CMSInitSound(n)` | `101552f0` | `SndSoundManagerVersion`; `Gestalt('snd ')` stores stereo capability (bit 0), stereo mixing (bit 1) and multichannel (bit 11) flags; allocates n channel records; `NewSndCallBackUPP(callback @ 10156500)` |
| `CMSOpenChannel(ch)` | `101559d0` | `SndNewChannel(sampledSynth=5, init = (stereo ? initStereo 0xC0 : initMono 0x80) \| initNoInterp 0x04, callback)` |
| `CMSRegisterSound(handle)` | `10155f10` | Stores the handle and its sound header (`CMSGetSoundHeader` @ `101562f0`); at most 100 |
| `CMSPlaySound(snd, ch, loop, L, R, Hz)` | `10156050` | `PrepareChannel` (open the channel, or `CMSStopSound` it); `CMSSetChannelVolumes(L,R)`; `CMSSetChannelFrequency(Hz, header rate)`; `StartSound` |
| `StartSound(chan, ch, snd, cbParam, loop)` | `101561f0` | `bufferCmd` (81) with the sound header, then `callBackCmd` (13, param 99) queued |
| `CMSSetChannelVolumes(ch, L, R)` | `101567c0` | `volumeCmd` (46), param `R<<16 \| L`, 0x100 = unity |
| `CMSSetChannelFrequency(ch, Hz, base)` | `10156880` | `rateMultiplierCmd` (86) with `(Fixed)(65536.0f · Hz / base)` (constant 65536.0 at code `0x1746f0`) |
| `CMSQuietSound(ch)` | `10155b10` | `rateMultiplierCmd` with param 10 (≈ 0.00015×, effectively frozen); frequency := 1, loop := 0, priority := −1. The channel is **not stopped**; it becomes the first candidate for reuse |
| `CMSStopSound(ch)` | `10155bd0` | Rate multiplier 1.0, then `flushCmd` (4), then `quietCmd` (3); playing id := −1 |
| `CMSPlaySoundDetailed(snd, prio, &ch, loop, L, R, Hz)` | `10156ac0` | Channel choice: the first idle channel (`CMSGetSoundPlaying` = −1); otherwise the **last** channel with the lowest priority (`<=` scan). If `prio` is below that priority it returns 2 (not played). It returns 1 when it steals a busy channel |
| `CMSPlayPaybackSFX(snd, &ch, loop, L, R, Hz)` | `10156a50` | `CMSPlaySoundDetailed` with priority **1** |
| `CMSSyncPaybackSFX(snd, ch, loop, L, R, Hz)` | `10156960` | Per-frame update: stores the loop flag; resends the volumes or the frequency only when they changed |
| `CMSSetToolVolume(v, relative)` | `10156cc0` | Adds v to every channel's current volume (`getVolumeCmd` 47), clamped to 0..0x100 |
| `CMSIdleSoundTask` | `10155e70` | Empty |
| `PlayMusicBlock` | `10156460` | A music-sequencer feature of the library (callback param 100). Unused by Payback: the music list at +0x34C stays NULL |
| callback | `10156500` (not recognised by Ghidra; `sndcallback.txt`) | Param 99/101 at buffer end. If the loop flag is set, it calls `StartSound` with the same sample (**looping = re-queueing the whole buffer**). Otherwise: rate multiplier 10, volume `0x10001` (1/256), `StartSound` again with loop on, frequency := 1, priority := −1. A finished one-shot is therefore turned into an inaudible "quiet" loop that keeps the channel busy but reusable |

**How `AddSound`/`ModifySound` map onto CMS (C, `DoAudio`).** AddSound and ModifySound only
write the 500-slot table, as on the Amiga. Every frame `DoAudio` picks the 6 loudest slots
and handles each one:

- The slot has no channel yet (+0x3E < 0, a field the Mac adds in the Amiga's unused
  slot bytes):
  `CMSPlayPaybackSFX(sample, &ch, loop, FX·L·256/1e7, FX·R·256/1e7, Hz)`. On return 0 or 1
  it stores `ch` in +0x3E. On 1 (a channel was stolen) it clears +0x3E in every other slot
  that pointed at `ch`.
- The slot already has a channel:
  `CMSSyncPaybackSFX(sample, ch, loop, Lvol, Rvol, Hz)`.
- A slot that drops out of the top 6, or whose amplitude falls to ≤ 19, gets
  `CMSQuietSound(ch)`, and +0x3E := −1.

So `CMSPlayPaybackSFX` corresponds to "this slot became audible" and `CMSSyncPaybackSFX` to
"keep updating its volume and pitch". The Amiga has no such split: each frame it re-mixes
from the slot's position.

### 1.3 Mixing rate, voice count, interpolation, sample format (C)

- **Voices:** 6 on both (`CMSOpenSoundTool(6)`; Amiga 3 per stereo group × 2). The
  selection (best 6 with `A > 19`, insertion into a sorted list, strict `<`, so earlier slots
  win ties) is the same code.
- **Mixing rate:** the Amiga mixes at a nominal 22,050 Hz and plays at 21,760 Hz (PAL period
  163), so all sounds come out about 1.3 % flat and slow. The Mac gives every sample a
  22,050 Hz header (§1.7) and sets the rate multiplier to `Hz/22050`. The Sound Manager
  resamples to the hardware rate (44.1/48 kHz on OS X), so **each sound plays at exactly the
  requested Hz**. The Mac's output is not limited to 22,050 Hz, but the source samples are
  8-bit, so there is no audible gain.
- **Interpolation:** none on either. The Mac explicitly asks for `initNoInterp` (drop-sample
  resampling). Whether OS X's Sound Manager emulation honours the flag is not known (I).
- **Sample format:** signed 8-bit mono on disk. The Mac converts it to offset-binary
  (`byte − 0x80`, `GetPaybackSound`), as the Sound Manager requires. Mixing and volume are
  done by the Sound Manager at 16-bit precision. The Amiga quantises each voice through the
  integer table `s·v/768` before summing in 8 bits; the Mac does not, so it is cleaner at low
  volume.
- **Effects:** no reverb, filter, echo or interpolation in either build. The Mac adds nothing.

**Is the Mac engine better?** Slightly: correct pitch (no PAL −1.3 %), continuous playback of
loops (no 30 Hz chunk skipping, §1.9) and no 8-bit voice quantisation. It has the same voice
count, the same rules and still no interpolation. It also adds two quirks: a +6 dB boost on
quiet voices (§1.5) and early cut-off of one-shots (§1.9).

**Impact:** `audio.rs` already mixes at 22,050 Hz with exact Hz and nearest-sample stepping.
That matches the Mac and is the intended behaviour. Keep it.

### 1.4 Pitch resolution and Doppler (C)

Both builds store `Hz/200` (the rate byte) in the slot (`AddSound`). Per frame:

```
if prevDist == -1:  Hz = rate*200                                 // first frame, NOT clamped
else:               Hz = clamp(rate + (prevDist - dist)*184/(len+400), 1, 255) * 200
prevDist = (int)sqrt(d2)
```

Here `len = ticks·735 − 1` (Mac `DoAudio` lines 300–345; Amiga stores `byte<<8` as a step
index, which is equivalent). The Mac contains a redundant special case for `rate == 40`
(8000 Hz) that gives the same result.

**Impact:** `audio.rs` quantises to 200 Hz steps only inside `doppler()`, i.e. for positional
sounds after their first frame. Global and first-frame sounds play at the exact Hz. Examples:
Beep1 at 4182 Hz should play at 4000 Hz; CarHorn uses `clamp(16e6/mass, …)` and should
snap to 200 Hz steps.

**Recommendation:** set `hz = floor(hz/200)*200` for every sound (unclamped on the first
frame, clamped 1..255×200 once Doppler applies).

### 1.5 Volume law and stereo pan (C)

**Amplitude.** Both builds use `A = vol / d²` (integer, d in world units >> 4, d² = 0 → 1);
non-positional sounds (y = −1) use `A = vol`. Same code.

**Pan** (Mac `DoAudio` lines 211–262; Amiga `FUN_01007290` lines 0x01007590..):

```
dy = |y_s - y_l|                         // integers (>>4)
r  = (double)(x_s - x_l) / (double)dy    // FLOAT division; rotated dx in Rotating-view mode
if dy < 1 or r == 0:  L = R = min(A,255)
if r > 0:  R = min(A,255);  L = min((int)(A / (r + 1.0)), 255)
if r < 0:  L = min(A,255);  R = min((int)(A / (1.0 - r)), 255)
```

`A` here is the **unclamped** amplitude. A loud nearby sound (e.g. A = 10,000) stays centred
until `A/(|r|+1)` drops below 255; only quieter sounds pan.

**Impact:** `audio.rs::pan` has two bugs, present relative to both builds:

1. It computes `r = dx / dy.abs()` as an **integer** division, so every sound with
   `|dx| < |dy|` is centred and panning jumps in steps.
2. It multiplies the far side by `1/(|r|+1)` *after* clamping A to 255, so loud close sounds
   pan hard instead of staying centred.

**Recommendation:** compute `r` as f32 and use `far = min(A_raw/(|r|+1), 255)/255` with the
unclamped `A_raw`. Also compute `dx`/`dy` from `(x>>4) − (lx>>4)` integers to match the
original exactly.

**Per-voice level.**

- Amiga: `vt[A][s] = s·A/768`, so a voice at A = 255 reaches 1/3 of full scale. The Paula
  hardware volume is `FX·V/100` with V = 63 (or 31 for doubled quiet voices).
  Full voice = `0.328 · FX/100`.
- Mac: `L = min(A,255)<<8`, then `volumeCmd = FX·L·256/1e7` (`DoAudio` lines 435–445),
  i.e. `FX·A·0.0065536`. At FX 100, A 255 this is 167/256 = **0.653** of unity, twice the
  Amiga's level. The Mac's **default FX is 50** (`ClearPreference` @ `100508c0`:
  `prefs[0] = 0x32`, prefs at `CPaybackPreferences+0x1C` = the config volume read by
  `LoadConfig` @ `10052be0`). At 50 the Mac gives 0.327, the same as the Amiga at 100.
- **Quiet-voice doubling.** Both builds double L/R (`<<1`) of voices with
  `max(L,R) ≤ 0x7F00` when they are placed in a "31" group (fewer than 4 loud voices).

  On the Amiga this is compensated by setting that Paula group's hardware volume to 31
  instead of 63: more precision, same loudness.

  On the Mac the group volumes (`0x1017ca3c`/`ca40`, data `0x1f8500`) are computed but read
  nowhere outside `DoAudio`/`DoAudioMulti`. A scan of every `lwz rX,-0x6a04(r2)` finds 8
  references, all inside those two functions. So **quiet voices play 2× (+6 dB) louder on
  the Mac** whenever fewer than 4 loud voices play. This is a porting bug.

**Recommendation (Mac adopted):**

- Use the Mac level law: per-voice gain = `FX · min(A,255) · 65536 / 1e7 / 256`, i.e.
  `FX·A·2.56e-5`. That is 0.653 at FX 100, A 255.
- Set `FxVolume` to **default 50**. At the default this sounds the same as today
  (`a/255/3·master` at 100), but FX 51..100 now goes up to twice as loud, as on the Mac.
- Sum the 6 voices and clip, as the Sound Manager does.
- **Do not** adopt the +6 dB quiet-voice boost, as an **[artefact]**. The doubling is the
  Amiga trick to gain precision inside a Paula group whose hardware volume is halved. The port
  kept the doubling but lost the compensation because the Sound Manager has no per-group
  hardware volume. It is a side effect of removing Paula, not a design change.

  If the lead prefers literal Mac output, it is one line: double L/R of voices with
  `max ≤ 127` when fewer than 4 voices have `A > 127`.

### 1.6 The FX volume slider and music volume (C, `SetSoundVolume` @ `100f24f0`, disassembled)

One option, 0..100 in steps of 10 (`ShowOptions` @ `10093240`), as on the Amiga. On the Mac
it drives three things:

1. **SFX:** the `FX` factor inside `DoAudio` (linear 0..100), and `AddSound` refusing
   one-shots at FX 0 (same as the Amiga).
2. `CMSSetToolVolume((int)(2.55·(2·min(new,50) − 2·min(old,50))), relative)`. This only
   nudges the channels that are playing at that moment. `CMSPlaySound` and
   `CMSSyncPaybackSFX` set absolute volumes, so the effect is lost on the next frame. In
   practice it does nothing.
3. **CD music:** `UCDAudio::SetVolume(v, v)` with `v = 200` for FX ≤ 50, else
   `200 − 4·(FX−50)`. So music plays at 200/256 up to FX 50 and **fades to silence at
   FX 100**.

So the Mac slider is effectively a balance control centred on its default of 50. The Amiga
has no music volume at all (§4.6 of `frontend_audio.md`).

**Recommendation (Mac adopted):**

- Keep one slider, 0..100 in steps of 10, **default 50**.
- SFX: gain law of §1.5 (linear in FX); FX 0 refuses one-shots.
- Music volume = `200/256` for FX ≤ 50, else `(200 − 4·(FX−50))/256` (silent at 100).
- Drop the `CMSSetToolVolume` nudge **[artefact]**: it is overwritten on the next frame and
  has no lasting effect.
- A separate music slider would be a pure extension. If added, default it to the Mac curve.

### 1.7 Samples (C)

- **Files:** all 77 load entries name the same 74 files, and the Mac `Sounds/` folder is
  byte-identical to the Amiga one (`samples.tsv`). Its only extra entries are the Finder
  search-index files `.FBCIndex` and `.FBCLockFolder`, which are not game data.
- **Lengths:** all 77 `LoadSound` lengths equal the Amiga's except **CarHorn: Mac 734 (=
  file size), Amiga 754** (reads 20 bytes past the end). This is a Mac bug fix. `audio.rs`
  already loads the whole file (734).
- **Header rate:** `LoadSound` passes a per-sample "natural" rate (Fixed), but
  `GetPaybackSound` @ `100fcd90` ignores it and builds every header with
  `SetupSndHeader(h, 1 ch, 22050.0, 8 bit, 'NONE', baseNote 60, len)`. The playback Hz is
  always relative to 22,050 Hz, as on the Amiga. For reference, the unused natural rates are:
  - 20000: most samples;
  - Footstep.* 12000, Footstep.water 15000;
  - FireMissile 17000, GrenadeBounce 8000, Minigun 16000;
  - PickupWeapon / ShotgunFire / Pistol / Ricochet / Rain 14000;
  - GarageDoor 10500, Bleep 16000, TickTock 8400;
  - MissionFail / MissionComplete / Alarm2 11025;
  - BigGunFire 9000, HitScenery 8500, ThumpingMusic 16000;
  - ChangeSelection 16000, ChangeSelection2 10000, ChangeSelection3 22500;
  - Ow1–3 8000, Doorslam 20500, Flamethrower 12500, Click 16000, Splash 8350;
  - ObjectAppear 8000, Alarm 10000, BrokenGlass 8000;
  - Beep1 4182, Beep2 / Beep3 / Hiss 8363;
  - Lightning 8000, Boom 8000.

  Most match the gameplay Hz in `frontend_audio.md` §4.4. They are useful as the authors'
  intended sample rates.
- **Resource fork:** no `snd ` resources in either Mac executable
  (`analysis/mac/scratch_frontend/rsrc_mac*.txt`: ALRT CNTL DITL DLOG MBAR MENU Mcmd NoPt
  PAYB PPob SIZE STR# Txtr WIND aete isap setl vers, plus carb/plst on OS X).

### 1.8 Looping, sample swaps and voice restarts (C)

- **Looping:** the Amiga wraps the slot position inside the mixer. The Mac re-queues the
  whole buffer from the Sound Manager callback when the loop flag is set, which may leave a
  small gap at the loop point (I).
- **Re-selection:** a slot that drops out of the top 6 is *quieted* (rate ≈ 0). When it comes
  back, `CMSPlayPaybackSFX` starts the sample **from the beginning**. On the Amiga the slot
  position keeps advancing while inaudible, and the voice resumes at that position.
- **Engine → clonk swap** (`Sound_SetSample`, called from `do_Other__Fv` when health < 50 with
  the same 1→46, 67→71 … 70→74 mapping): the Mac quiets the channel, so the new sample starts
  from its beginning next frame. On the Amiga it continues at the slot position.
- **Engine-sound update:** identical formulas in `do_Other__Fv`:
  - `Hz = |speed|·20000/clamp(maxSpd·health·2/100, maxSpd/4, maxSpd) + 6000`;
  - tank `·10000 … + 4000`;
  - the airborne unit (`PositionAirborneUnit` @ `10082620`) uses `|v|·20000/maxSpd + 6000`;
  - the per-type loop `AddSound(type+0x817C, loop, 10,000,000, 22050)`.

  These belong to the gameplay agent's area.

**Recommendation:** keep resuming at the tracked position, an **[artefact]** exception. The
restart-from-start happens because a Sound Manager `bufferCmd` can only start a buffer at its
beginning: `CMSQuietSound` / `CMSPlayPaybackSFX` have no seek. It is not a game-level decision;
the slot still tracks the position as on the Amiga. The loop gap is also a Sound Manager
re-queue artefact, so loop seamlessly.

### 1.9 One-shot lifetime: the "+400" slot clock (C, both builds)

Both builds advance each slot with `pos += (len+400)·200·rate/22050` per frame, where
`len = ticks·735 − 1`, and delete a non-looping slot when `pos > sampleLength` (Mac `DoAudio`
lines 486–497). Real playback consumes only `735·ticks` output samples per frame, so the slot
clock runs `(735t+399)/(735t)` too fast: ×1.54 at 1 tick per frame, ×1.27 at 2 ticks.

- **Amiga:** the mixer reads the voice *from* the slot position each frame and the previous
  Paula request is aborted after about 735·t samples. The audible result is the right pitch,
  but each frame drops the last ~400 source samples, so one-shots are chopped and finish
  after ≈65 % of their duration (at 30 fps). Loops are also chopped.
- **Mac:** playback is continuous (no chopping), but `DeleteSound → CMSQuietSound` still fires
  when the slot clock passes the end, so **one-shots are silenced after ≈65 % of their length**.
  For example, MissionComplete (20,224 B at 11,000 Hz, 1.84 s) is cut at about 1.19 s. Loops
  are fine.

**Impact:** `audio.rs` plays one-shots to the end and loops continuously, i.e. "Mac minus the
cut".

**Recommendation (Mac adopted):** reproduce the Mac exactly. Keep a per-sound slot clock that
advances by `(735·t + 399)·200·rate/22050` source bytes per game frame (t = that frame's
ticks; rate = Hz/200 without Doppler). When it passes the sample length:

- a one-shot is silenced and despawned;
- a loop wraps the clock.

This logic lives in the game code (`DoAudio`), not in the Sound Manager, so it is not an
artefact. Do **not** reproduce the Amiga chunk skipping, which comes from Paula request
aborts.

**Determinism:** drive the clock from the simulation's tick count, but keep it purely in the
audio layer. Nothing in the simulation reads slot lifetimes: one-shot handles are not kept by
objects, and ModifySound on a dead slot is a no-op that only prints a message. Replays are
unaffected.

### 1.10 Menu sounds (C, `PlaySound__Fii` @ `100f1960`)

`Hz = 1.0/(2.79365e-07·period)` = **3,579,545/period**, the NTSC Paula clock (constants at
code `0x174238`: 1.0, 2.79365e-7). The Amiga used the PAL clock 3,546,895:

| Sound | Period | Amiga | Mac |
|---|---|---|---|
| ChangeSelection | 235 | 15,093 Hz | 15,232 Hz |
| ChangeSelection2 | 270 | 13,137 Hz | 13,257 Hz |
| ChangeSelection3 | 160 | 22,168 Hz | 22,372 Hz |
| Boom | 448 | 7,917 Hz | 7,990 Hz |

- Volume `(FX<<8)/100` on both sides (Amiga `FX·63/100` on Paula), centred, priority 1, on
  any free channel. It **mixes with game voices** instead of stopping the mixer channels
  as the Amiga does. If it steals a channel it clears that channel from the game slots.

**Recommendation (Mac adopted):** change `frontend/ui.rs` to `3_579_545.0 / period` and
volume `FX·256/100` (unity at 100), centred. Play menu sounds through the normal mixer
alongside game voices instead of muting them. The NTSC constant is in the game code
(`PlaySound`), so it is not a platform artefact.

### 1.11 Pause menu (C)

- Mac `ShowOptions`: `PauseAllSounds()` then `PlaySound(0x20, 235)`; on exit
  `ResumeAllSounds()` or `StopAllSounds()`. Game sounds resume where they were.
- Amiga (`FUN_0101a7dc`, line 59): `FUN_01005294()` stops the Paula output; the slots survive
  and the mixer restarts on the next game frame.

**Recommendation (Mac adopted):**

- On the pause menu, freeze every game voice: positions and slot clocks stop, the menu sound
  still plays.
- On resume, continue them.
- On "quit level", stop all.

---

## 2. Music

### 2.1 What replaced the CD-audio thread (C)

| Amiga | Mac |
|---|---|
| 68k: external `CD_Audio_Handler`; OS4: "Payback CDDA Thread" `0x010087a0` with SCSI play commands (0x48/0xA5/0x45) | Mac OS X: `UCDAudio` (a PowerPlant `LPeriodical` + `LBroadcaster`, but it is never ticked as a periodical). Mac OS 9: the classic Apple CD driver (`OpenDriver` + `PBControlSync` imported by the OS 9 build) (I) |
| `CD_Init/CD_Play/CD_Stop/CD_SetVolume` messages | `CD_Init` @ `100fae00`, `CD_Shutdown` @ `100fae90`, `CD_Play` @ `100faf00`, `CD_Stop` @ `100faf80`, `CD_CheckForTrackEnd(n)` @ `100fb000`, `CD_SetVolume` @ `100fb100` = **empty stub** |

**How the OS X build plays a track:**

- `UCDAudio::GetTOC` @ `10103e20` → `FindFinderVolume` @ `10103c00` walks the mounted
  volumes. It notes whether a volume named **`PaybackMac`** (the game CD) is present and picks
  the first volume whose file system is `'JH'` (the OS X **audio-CD filesystem**, cddafs).
- `CountTracks` @ `10104520` reads that volume's **`.TOC.plist`** (keys `Sessions`,
  `Track Array`, `Point`, `Data`, …). Every track counts towards the total (+0x3E); only
  tracks whose `Data` is not true count as audio (+0x3C).
- `Play` @ `10103fc0` opens the catalogue entry `ioFDirIndex = index` of the CD root with
  `PBGetCatInfoSync`, then `OpenMovieFile` + `NewMovieFromFile` + `SetMovieVolume` +
  `StartMovie`. It plays the cddafs **AIFF track file as a QuickTime movie**.
  `IsPlaying` = `!IsMovieDone`.

So the music is still the Red Book audio tracks of the physical CD, not MP3 or a QuickTime
soundtrack. `PaybackMac.toast` is the HFS data volume only, so **the Mac release gives us no
music data either**.

### 2.2 Track mapping and modes (C unless marked)

| Situation | Amiga | Mac |
|---|---|---|
| Front-end menus | 13 | `CD_CheckForTrackEnd(13)` each frame: if not playing, `PlayTrack(13)` (loops the track) |
| Game Over | 16 | `CD_CheckForTrackEnd(16)` |
| Level / Game complete | 8 | `CD_CheckForTrackEnd(8)` |
| Pause menu | continue | `ShowOptions`: `CD_CheckForTrackEnd(0)` if mode ≠ 0 |
| Radio mode (1) | vehicle radio track; stopped on exit | `GetInOutVehicles__Fi` @ `10066140`: `CD_Stop()`, then `CD_Play(type+0x8194)` if > 0. Same per-vehicle tracks. Not restarted when the track ends (only mode 2 is polled in game) |
| Continuous mode (2) | track 0 → random first track, then sequential with wrap; checked ≤ 1/s | `amiga_levelloop` @ `10100f50`: `CD_CheckForTrackEnd(0)` every frame → `UCDAudio::Next()` when the movie is done. **No random start, no wrap** (`Next` only advances while `index < audioTracks`) |
| Disabled (3) | forced if the CD thread fails | Set by `DoCDAudioSetup` @ `100a1500` when `CD_Init` fails. `CD_Init` succeeds iff a `UCDAudio` object exists |
| CD setup menu | device name, unit, mode | Kept unchanged (device string and unit number are entered, then ignored by `CD_Init`) |
| Music volume / ducking | none (OS4) | Tied to the FX slider (§1.6). `DrawView` still calls `CD_SetVolume(0x80)` under cover or `CD_SetVolume(0x100)` otherwise, but the stub ignores it |

**Index quirks (I, from reading the code):**

- `Play()` uses `index = n + 1` for a requested track n. Catalogue entry 1 is `.TOC.plist`.
  If `n ≤ dataTracks` or `n > totalTracks`, it uses `dataTracks + 2`.
- `Next()` sets `index + 1` and `Play()` adds 1 again, so **continuous mode advances two
  entries per track (it plays every other track)**.
- Whether `n + 1` hits CD track n depends on the CD layout: data track first or last. With
  the data track last ("Enhanced CD") it does; with the data track first, everything is off
  by one.

**Recommendation (Mac adopted):** music is not implemented yet. Load user-supplied tracks,
e.g. `Music/Track13.*`, ripped from the CD, numbered by CD track. Then follow the Mac
game-side logic:

- **Menus, Game Over, Level/Game complete:** request track 13/16/8 every frame; if it is not
  playing, (re)start it, so these loop.
- **Radio (1):** on entering a vehicle, stop the music, then play the vehicle's radio track if
  > 0. It is **not** restarted when it ends. On exit, stop.
- **Continuous (2):** each game frame (and in the pause menu), if nothing plays, advance to
  the **next track after the one that played last**. There is no random first track (Mac)
  and playback stops after the last audio track (Mac `Next` does not wrap).
- **Disabled (3):** when no music source is available. **Off (0):** stop. The mode cycles
  Off → Radio → Continuous in the CD setup menu. The device/unit fields are still shown, but
  are meaningless; drop them or keep them inert.
- **Volume:** the FX-slider crossfade of §1.6. **No "under cover" duck**: on the Mac
  `CD_SetVolume` is a stub, so the duck is never heard.
- **Not adopted [artefact]:** the advance-by-two-entries / every-other-track behaviour and the
  `n+1` catalogue index. Both come from mapping CD track numbers onto the cddafs directory
  listing inside `UCDAudio::Play`/`Next`, which is pure OS X CD plumbing. Map track n to track
  n.

**Open point:** whether "no wrap" in continuous mode is intended. It sits in `UCDAudio::Next`
(plumbing). If the lead considers that plumbing too, wrap to the first audio track as on the
Amiga.

---

## 3. Intro video

### 3.1 Files (C, probed with `ffprobe` and the MPEG headers)

| File | Container | Video | Audio | Length |
|---|---|---|---|---|
| `Videos/payback_high_mpeg1.mpg` (45.8 MB) | MPEG-1 system stream | MPEG-1, 640×480, 25 fps, 4.8 Mbit/s, 1800 pictures | MP2 192 kbit/s, 48 kHz mono | 71.98 s |
| `Videos/payback_high_mpeg2.mpg` (38.4 MB) | MPEG-2 program stream | MPEG-2, 640×480, 25 fps, 10 Mbit/s | MP2 192 kbit/s, 48 kHz mono | 71.98 s |
| `Videos/payback_low_mpeg1.mpg` (12.8 MB) | MPEG-1 system stream | MPEG-1, 320×240, 25 fps, 1.2 Mbit/s | MP2 192 kbit/s, 48 kHz mono | 71.98 s |

**The content is a different intro.** The Amiga `intro.video` is a 30 s, 320×200, 15 fps
in-engine fly-through of the city ending in the burning PAYBACK logo
(`out/img/intro/intro_contact.png`). The Mac film (`scratch_audio/mac_intro_contact2.png`) is
a 72 s pre-rendered piece in this order:

1. Apex Designs logo;
2. headlights approaching on a night road, with timecode overlays;
3. green terminal text ("Unwanted by society", "authority by …");
4. distressed-type PAYBACK title build-up, a white flash and an explosion;
5. the flaming PAYBACK logo.

### 3.2 Player (C)

- `OpenAnimation__FPc` @ `100fb130` ignores its argument (`"intro"`). `GetPaybackMovie` @
  `100fcbc0` opens the **hard-coded** Pascal path `:Videos:` + `payback_high_mpeg1.mpg`
  (data `0x1018fac0`) with `OpenMovieFile`/`NewMovieFromFile`.
- There is **no quality choice** in either Mac build. `payback_low_mpeg1` and `_mpeg2` appear
  in neither the OS 9 nor the OS X strings, and the Preferences dialog (PPob 2200/2210) has no
  video option. The two extra files are presumably for other players or platforms (I).
- `PlayAnimation__FP8ViewPortPUcP11INPUTSTATUS` @ `100fb1d0`:
  1. `HideCursor`; `PauseGL` (detaches the AGL context).
  2. Draws to the window port, or in fullscreen to the DrawSprocket context through
     `SetQuickDraw` (with CLUT 128 applied to the movie in 8-bit QuickDraw mode).
  3. If the movie box differs from the window bounds: `ScaleRectToFit(box, bounds)`, which
     scales to fit and keeps the 4:3 aspect.
  4. If the resulting box's right edge is 640, it paints the box black and insets the movie
     by 1 px. This looks like a workaround for an edge artefact in 640×480 mode (I).
  5. `FlushEvents`, `GoToBeginningOfMovie`, an invisible `MovieController`,
     `MCDoAction(mcActionPrerollAndPlay, preferredRate)`.
  6. Loop: `Button()` (mouse) or `ALAnyKeyPressed()` (any key held, polled with `GetKeys`)
     → skip; otherwise `MCIdle` until `IsMovieDone`.
  7. `StopMovie`, wait for the mouse button release, `FlushEvents`, paint black, restore the
     cursor or DSp, `ResumeGL`.
- Gamepads do not skip; the movie volume is never set (full volume, independent of FX), as
  on the Amiga (fixed volume 63).
- **When it plays:**
  - at boot, at the end of loading (`InitAll__Fv` @ `100632a0`: `OpenAnimation("intro")`,
    `PlayAnimation`, `CloseAnimation`, then `CloseProgress`), the same place as the 68k build;
  - from the menu "View intro" (`DoGameMenu` @ `100a05e0`, option 7).

  The menu path first does `CD_Stop()` unless music is disabled, and plays sound 0x2D
  (refuse) if the file cannot be opened.
- Possible quirk (I): because the skip test polls the *held* keyboard state, a key still held
  from the menu selection could skip the intro immediately.

**Recommendation (Mac adopted):** make the **Mac film the default intro** whenever the Mac data
is present; fall back to the Amiga FLC (`intro.rs` today) otherwise.

- **Source:** `Videos/payback_high_mpeg1.mpg` only, the file the game hard-codes. There is no
  quality option, as on the Mac.
- **Decoding:** either a Rust MPEG-1 decoder (pl_mpeg bindings), or an extraction step in
  `tools/extract/extract_mac.py` that transcodes it with ffmpeg into `extracted/`, e.g. a frame
  sequence plus a WAV at 48 kHz. Never commit the result.
- **Playback:** 25 fps with its own audio at full volume (not scaled by FX). Scale to fit the
  window keeping 4:3, black elsewhere.
- **When:** at the end of start-up loading, and from "View intro", which first stops the CD
  music unless music is disabled. Play sound 0x2D if the film cannot be opened.
- **Skip:** any key or mouse button; wait for the release afterwards.
- **[artefact]**, not adopted:
  - the held-key polling, so skip only on *new* presses as `intro.rs` does, avoiding an
    instant skip from the menu key;
  - the 1-px inset at 640 wide (QuickTime blit workaround);
  - `PauseGL`/`HideCursor` plumbing.

  Pad-button skip is our extension and harmless.

---

## 4. Platform layer

### 4.1 Game loop, timing, threading (C)

- `amiga_levelloop__Fv` @ `10100f50` is the Amiga main loop with the same structure:

  ```
  ticks_raw = (TickCount() - last + 1) >> 1           // TickCount = 60 Hz → 30 Hz game ticks
  while TickCount() < last + ticks_prev*2: spin        // busy-wait frame limiter
  if (!paused) UpdateAudio(ticks*735 - 1)              // empty on the Mac
  ticks = clamp(ticks_raw, 1, 5); demo time-out 9000; last = TickCount()
  ```

  This is the 68k scheme (2 timer ticks per game tick) with a 60 Hz timer instead of 50 Hz,
  so a **game tick is 1/30 s**, like OS4. Audio frames are 735 samples per tick, as on both
  Amiga builds.
- **Rendering:** one frame per loop iteration, at most 30 fps in game (1 tick per frame), with
  a variable step (ticks 1..5). **No frame interpolation, no higher render rate.**
- **Menus:** `ch2pl__Fv` @ `10053320` (frame present) spins until
  `TickCount() ≥ next; next = TickCount()+1` when `Input_IsOnMenu`, so menus run at most at
  60 fps. Presenting is `WritePixelArray8` (software/QuickDraw) or `W3D_SetDrawRegion` @
  `10153ec0` → `glFinish(); aglSwapBuffers()`.
- **Threads:** the game is single-threaded. `YieldToThread`, `NewThread`, `InsTime`/
  `PrimeTime`/`RmvTime` and `ThreadBegin/EndCritical` come only from the linked PowerPlant
  `LThread`/`LSemaphore` library and `UMainThread` (created in `AppMain`). No game thread or
  Time Manager task is created; `LTimerTask` is used only by PowerPlant mouse tracking.

  The OS event loop is pumped from inside the game's input polling: `GetMsg` @ `100fc720` on
  the keyboard port calls a `CPaybackApp` virtual (I: the event pump) before
  `GetMsgFromKeyboard`. `MacHasQuit` @ `1004f560` turns the app's quit state into the Amiga
  quit flag `amiga_hasQuit`.

**Recommendation (Mac adopted):** **one game tick = 1/30 s**, as on the Mac and OS4. This
replaces the 25 Hz suggested for the 68k Update 7 in `docs/re/people.md` §1.1.

- Per frame: `ticks = clamp(elapsed_30Hz_ticks, 1, 5)` with the same limiter, so a frame
  lasts at least as long as the previous frame's ticks.
- Audio frame length 735·t.
- Menus are capped at 60 fps.

**Replay determinism:**

- The simulation only consumes the per-frame `ticks` values. Replays store them (frame word
  bits 2..0, `frontend_menus.md` §3.8) and playback reuses them, so changing the wall-clock
  length of a tick changes only the real-time speed, not the simulated result.
- The tick *source* must stay outside the simulation: wall clock when live, recorded value in
  replays.
- Do not introduce any Mac-only per-tick rate change into the simulation without
  re-recording the reference replays; `tools/checks/replay_check.py --suite` must still report MATCH.

**Not adopted [artefact]:** the busy-wait on `TickCount`, the absence of vsync and the
single-threaded event pumping. Rendering faster than the tick with interpolation is not in
the Mac (one render per simulation step, at most 30 fps). Adopt the Mac's 1:1 render/step by
default; any interpolation would be an optional extension that must not touch simulation
state.

### 4.2 Input (C)

**Keyboard.**

- `GetMsgFromKeyboard` @ `100fc280` polls `GetKeys` (the 128-bit key map), compares it with
  the previous map, and turns the **first** changed bit into a fake Intuition message: class
  `0x400` (`IDCMP_RAWKEY`), code = **Mac virtual key code**, `+0x80` on release, plus the
  character (`ALCharCodeFromKeyCodeAndMap`).
- One key change is reported per call; the rest come on later calls.
- Cmd(0x37)+Q(0x0C) sends the app's quit command.
- `UpdateInputStatus__FiP11INPUTSTATUS` @ `100f8490`:
  - first maps codes through the 11-entry binding table (`ButtonFromKey` @ `100f7fe0`);
  - then dispatches fixed keys through a jump table on `code − 0x12` (table `0x1018f760`;
    `scratch_audio/keyhandlers.txt`).

  Fixed keys:
  - Esc and P: pause/menu.
  - The number row: 1–7 and 0 are view or player hotkeys; 8 and 9 do nothing.
  - `-` and `=`.
  - Delete, F1, F8, keypad Enter.
  - Next weapon: Help, F3, F10, keypad +.
  - Previous weapon: Forward-Delete, F2, F9, keypad −.

  The meaning of the individual hotkeys belongs to the frontend agent.
- **Default key map** (`ClearPreference`): 7E up, 7D down, 7B left, 7C right, 31 Space, 7F (–),
  24 Return, 06 Z, 72 Help, 75 Fwd-Delete, 2E M. This is exactly the Amiga map (4C 4D 4F 4E
  40 00 44 31 5F 46 37) translated key for key.
- **Bug fix:** "unbound" is 0x7F, an unused Mac key code. The Amiga uses raw 0x00, which is a
  real key (`` ` ``), so the Amiga's unbound shoot-alt slot fired on the backtick key (I).
- **Rebinding** (`ReadFromInputSource` @ `100f93d0`): Esc cancels; the `0` key → 0x7F
  (unbind); 1–9, `-`, `=` and **F1** are refused (the Amiga refuses 1–9, `-`, `=` and P). Any
  other code < 0x7F is accepted.

**Gamepads.**

- The class is named `UInputSprocket`, but the OS X build uses Apple's **HID Utilities**
  through `HID.bundle` (`SetupHIDCFM` @ `10159d00` loads it into the CFM app;
  `HIDBuildDeviceList(GenericDesktop, GamePad)`). The OS 9 build really uses InputSprocket
  (`ISpStartup`, `ISpConfigure`, … imported).
- Each control binds to one HID element (`AssignControl` @ `10157fd0`,
  `ConfigureHIDControl` @ `101581f0` = `HIDConfigureAction` with a 0.2 s poll).
  - Buttons: pressed when the value equals the recorded value.
  - Axes: pressed when the calibrated, scaled value is within ±50 of the recorded extreme
    (+1000 offset) (`IsHIDControlPressed` @ `10158300`).
- **Everything is digital**; there is no analogue steering.
- Defaults: axis "X" min/max and "Y" min/max for the directions, buttons for the rest
  (`ClearPreference`).
- `UpdateInputStatusNoKeys` @ `100f8090` polls the 11 controls each frame:
  - controls 4 and 5 are OR-ed into fire;
  - 8 and 9 are edge-triggered next/previous weapon;
  - 10 is repeat-message.
- `ControlMethodExists` @ `100f8420`: Keyboard (12) and Off always exist; Joystick 0–3 need
  HID; **PSX and CD32 (4–11) are removed**.

**Mouse.** It is not used in game or in the game's own menus. `GetMouse`/`GetGlobalMouse`
appear only in PowerPlant; `Button()` skips the intro.

**Recommendation (Mac adopted), for `frontend/bindings.rs`:**

- **Default keyboard map:** the Mac one translated to physical keys: Up, Down, Left, Right,
  Space, *(unbound)*, Return, Z, Help/Insert, Forward-Delete, M. It is the same as the Amiga
  except that slot 5 (shoot alt) is **truly unbound**.
- **Rebinding:**
  - Esc cancels; `0` unbinds;
  - refuse 1–9, `-`, `=` and **F1**;
  - accept **P** (the Mac's change), even though P is also a pause hotkey.
- **Fixed hotkeys:** keep the Mac set, including the extra next/previous-weapon aliases
  (F2/F9/keypad− and F3/F10/keypad+) and Esc/P for the pause menu; details for the frontend
  agent.
- **Pads:** digital only. Axes act as buttons at their configured extreme (±50 of 255 on the
  HID scaled range); 11 bindable controls; 4/5 OR-ed into fire; 8/9 edge-triggered weapon
  cycling; 10 repeat-message. Up to 4 pads plus the keyboard; no PSX/CD32 entries.
- **[artefact]:**
  - Cmd-Q quit and the Mac key codes themselves (map to physical keys);
  - HID.bundle/InputSprocket plumbing;
  - one key event per poll (process all changes per frame).

### 4.3 Display (C)

- **Preferences dialog** (PPob 2200, OS 9; 2210, Carbon), opened at launch with Option held
  or from Edit ▸ "Payback Setup…" (Cmd-;):
  - Play in window / Play fullscreen;
  - Size 640×480 / 800×600 / 1024×768;
  - (OS 9 only) Use OpenGL / Use QuickDraw, Gouraud shading, motion blur;
  - Use rotating view;
  - FX: Off / Flares only / Flares and artifacts;
  - "Don't show this dialog again".
- **Fullscreen:** `UDrawSprocket::GetDSpContext` @ `10157090` → `DSpFindBestContext` +
  `DSpContext_Reserve` at the chosen size, 8-bit (software/QuickDraw) or 16-bit (I: OpenGL)
  depth, CLUT 128. `Activate` @ `10157490`: black blanking colour, `HideCursor`, gamma fade
  out, `SetState(active)`, fade in. "Sorry, cannot play fullscreen at that resolution." if no
  context is found (STR# 1100).
- **Window:** `CAmigaWindow::ResizeAndCenter` @ `10102570` sets the window to the same three
  sizes and paints it black. The game resolution is the window's content size
  (`GetDisplayInfoData` @ `10103300` → `ReadScreenParams` @ `10051fc0`).
- **OpenGL:** `W3D_CreateContextTags` @ `10153f80`:
  - fullscreen: `aglChoosePixelFormat(device, {AGL_RGBA, AGL_FULLSCREEN, AGL_DOUBLEBUFFER})`;
  - windowed: `{AGL_RGBA, AGL_DOUBLEBUFFER}`;
  - **no depth-size attribute** although `GL_DEPTH_TEST` is enabled (rendering agent);
  - **`aglSetInteger` is not imported, so no swap interval and no vsync request.** The 30 Hz
    limiter does the pacing.
  - `PauseGL`/`ResumeGL` @ `10153490`/`10153520` detach and reattach the drawable around the
    intro movie.

**Recommendation:** this is almost all **[artefact]** (DrawSprocket, AGL, CLUTs, gamma fades).
Adopt the user-facing parts:

- window or fullscreen;
- 640×480 / 800×600 / 1024×768 as the offered sizes, alongside any modern sizes;
- the FX (lens flare) and rotating-view options.

Our graphics options already cover these. Vsync has no effect on the simulation; leaving it
on is fine.

---

## 5. Bug fixes and new quirks in the Mac (checklist)

| # | Item | Kind | Conf. | Decision (Mac definitive) |
|---|---|---|---|---|
| 1 | CarHorn loaded at its true 734 bytes (Amiga 754) | fix | C | Adopt (already done) |
| 2 | Loops play continuously (no 30 Hz chunk skipping) | fix | C | Adopt (already so) |
| 3 | Exact pitch, no PAL −1.3 % | fix | C | Adopt (already so) |
| 4 | Pan uses float `r` and unclamped A (both builds) | original | C | **Fix `audio.rs::pan`** |
| 5 | Rate always quantised to 200 Hz (both builds) | original | C | **Quantise in `audio.rs`** |
| 6 | Mac per-voice level law, FX default 50 | Mac change | C | **Adopt** |
| 7 | Quiet voices +6 dB (group volume lost) | [artefact] of removing Paula | C | Not adopted (one-line option) |
| 8 | One-shots cut at ≈65 % (slot clock +400) | Mac game code | C | **Adopt** (audio layer only, replay-safe) |
| 9 | Voices restart from the sample start when re-selected or swapped; loop re-queue gap | [artefact] (Sound Manager cannot seek) | C | Not adopted |
| 10 | Menu sounds at the NTSC clock, volume FX·256/100, mixed with game voices | Mac change | C | **Adopt** |
| 11 | Pause menu pauses/resumes voices | Mac change | C | **Adopt** |
| 12 | One slider: SFX linear, music 200/256 fading to 0 above 50; default 50 | Mac change | C | **Adopt** |
| 13 | No music duck (`CD_SetVolume` stub) | Mac behaviour | C | **Adopt** (no duck) |
| 14 | Continuous music: sequential from the last track, no random start, no wrap | Mac game logic / `UCDAudio::Next` | C/I | **Adopt** (wrap is an open point) |
| 15 | Every-other-track skip, `n+1` catalogue index | [artefact] (cddafs mapping) | I | Not adopted |
| 16 | Unbound key = a non-key code | fix | C | **Adopt** |
| 17 | Rebind refuses F1 instead of P; extra weapon-cycle hotkeys; PSX/CD32 removed; digital pads | Mac change | C | **Adopt** |
| 18 | Intro: 72 s MPEG-1 film, hard-coded high quality, fit to window 4:3 | Mac change | C | **Adopt as default** (FLC fallback) |
| 19 | Intro skip polls held keys; 1-px inset | [artefact] | I/C | Not adopted |
| 20 | 30 Hz tick, ticks 1..5, frame limiter, 60 fps menus, 1 render per step | Mac timing | C | **Adopt** (ticks recorded in replays → deterministic) |
| 21 | No vsync, busy-wait, DSp/AGL plumbing | [artefact] | C | Not adopted |

## 6. Implementation order (suggested)

1. `audio.rs`:
   - float pan with unclamped A;
   - 200 Hz quantisation;
   - Mac gain law with `FxVolume` default 50;
   - the slot clock for the one-shot cut (fed by the frame's ticks);
   - mixer pause/resume.

   Update the doc comments citing `DoAudio` @ `100f32f0` / `CMSPlayPaybackSFX` @ `10156a50`
   next to `FUN_01007290`, and the unit tests in `audio.rs`.
2. `frontend/ui.rs`: NTSC menu-sound clock and volume. `frontend/bindings.rs`: Mac defaults,
   unbound value, refusal list.
3. Music module: the Mac modes and the slider crossfade.
4. Intro: Mac film (with an extraction/transcode step) as default, FLC fallback.
5. Timing: 30 Hz tick with the gameplay agent; re-run
   `tools/checks/replay_check.sh … --map 0 --seed 1234`, which must still report MATCH.
