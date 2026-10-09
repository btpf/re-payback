## Corrections and doubts for the existing notes

The cross-reference disagrees with a few pairings in `docs/re/` (checked by hand):

- **`docs/re/vehicles.md`**: "Grip, surface friction, skid marks, skid sound — OS4 `FUN_010cc288`,
  68k `FUN_000a1758`". All three builds keep the unit-physics functions in the same order: `MoveUnit`,
  `PositionUnit`, `SkidUnit`, `PositionAirborneUnit`, `BounceUnit`. That order gives OS4 `010cc288` =
  `SkidUnit` = 68k `000a197a`. Both `010cc288` and `000a197a` call AddSound, ModifySound, AddEffect,
  random and GetSampleNum. 68k `000a1758` (math only) is `PositionUnit` (OS4 `010cbc7c`). The same
  table also puts "movement sub-step loop" OS4 `0105b9ec` inside 68k `000a197a`. In fact `0105b9ec`
  is `MoveObject_Vehicle` = 68k `000e109e`: both call `BounceUnit` (`010cd198` / `000a27c6`).
- "Create object of model at (x,y,h)": OS4 `01046ee4` = `NewObj` = 68k **`000cf11e`**. Both call
  `AddCollideObj`. The 68k `000db03e` that the notes give is a small wrapper that calls `000cf11e`.
- Atan: OS4 `010bcc18` = `GetAngle(double,double)` = 68k `00094b44`. The 68k `0003fd32` is the
  table-lookup helper, not the counterpart.
- The mission handlers follow the notes: `ProcessTrigger_00` = OS4 `010d2b24` = 68k `000b9b6a`, and
  `InitScript_00` = `010eabc0` = `000b91dc`. City 2 level 0 is a single `ProcessTrigger_20` on the
  Mac. OS4 splits it by event range into `010d9ff4` (100000–199999), `010da308` (300000–399999) and
  `010de0b4` (the rest).

## New code in the Mac build (no Amiga counterpart)

- **Platform layer, all new:** PowerPlant and the CodeWarrior runtime (647 functions below
  `0x10051910`, including `CPaybackApp`, `main` and `AppMain`) and MSL (198). Mac classes (91):
  `UCDAudio` (CD audio), `UDesktop`, `UInputSprocket` / `UDrawSprocket` (InputSprocket and
  DrawSprocket), `ALPreferences` (preferences instead of `S:Payback.config`), `CAmigaWindow`,
  `LAMControlImp`.
- **Amiga-API emulation (88):** exec/dos/intuition/graphics (`AllocMem`, `MatchFirst`/`Next`/`End`,
  `CreatePort`, `GetMsg`, `ReplyMsg`, `FindTask`, `SetTaskPri`, `EasyRequestArgs`, `AddPart`,
  `NameFromLock`, `GetProgramDir`, `OpenScreenTags`, `OpenWindowTags`, `CloseScreen`,
  `GetDisplayInfoData`, `LoadRGB32`, `WritePixelArray8`, `SetPointer`); Warp3D on OpenGL (`W3D_*`,
  `PauseGL`, `ResumeGL`, `convert16BitTextureARGB2RGBA`, plus game-named helpers `uploadTexture`,
  `convert32BitTexture`, `RoundToExp2`, `aglReportError`); the `CMS*` sound library
  (`AllocateSoundSystem`, `PrepareChannel`, `StartSound`, `PlayMusicBlock`); HID input (`HID*`,
  `NameOK`); file and path helpers (`mac_fopen`, `AmigaPath2FSSpec`, `GetPaybackMovie`,
  `GetPaybackSound`, `GetMsgFromKeyboard`, `NewColorTableByEntries`).
- **Main loop split:** on the Amiga the whole game session is one function (OS4 `010692bc`, 68k
  `000ec686`). The Mac splits it into `amiga_init`, `amiga_dotitlescreen`, `amiga_startlevel`,
  `amiga_levelloop`, `amiga_finishlevel`, `amiga_runloop` and `amiga_cleanup`, so that PowerPlant's
  event loop can drive it. The table maps `amiga_finishlevel` to `010692bc` (shared strings); the other
  pieces are slices of the same function. Likewise OS4 `0105f594` / 68k `000e3a58` (the per-object
  update) contain what the Mac has as `object_MainLoop`, `do_Person`, `do_Other` and `do_Weapon`.
  `MainLoop` (calls `DoScroll` + `object_MainLoop`) is inlined on the Amiga.
- **Game-code names with no Amiga match:** 115 of the 718 game functions. Most fall into four groups:
  - Functions that are inlined in the Amiga builds (GCC/SAS inline small statics):
    getters/setters such as `GetViewPort`, `GetNumCities`, `GetNight`, `GetMovieLevel`,
    `GetHighestScore`, `CharToInt`, `Average`, `AverageVals`, `OBJ_AverageVals` and `DISPLAY_Xor`.
  - Code that the Mac rewrote for its own APIs: `LoadSound`, `CloseSound`, `StopAllSounds`,
    `PauseAllSounds`, `ResumeAllSounds`, `UpdateAudio` and `CD_SetVolume`;
    `OpenAnimation`/`PlayAnimation` (QuickTime); `RequestPaybackSetup`; `SetupTmap`,
    `GetOrigChunkyScreen`, `SetFullChunky` and `Display_LoadTex`; and the software span routines
    `GroundVertLoop`, `GroundFlat2GouraudColour` and `GroundVertLoopGouraudColour2`, which are PPC
    assembly in the OS4 `CODE` section.
  - **Probably new in the Mac release (INFERRED, no counterpart found in either Amiga build):**
    `InitScript_Random`, `ProcessTrigger_Random`, `FinishMission_Random` and `GetPointLocation` (a
    fourth "city" in the script dispatch, `city == 3`, which the OS4 dispatcher does not have);
    `EstimateTime`/`EstimateTime2`; `GetRandomVehicleNum`/`GetRandomVehicleNum_Pursue`
    (user-map traffic?); `SetObjFollowObj`, `SetTrigger_Long` and `MessageThem_Parse` (script
    API); `GetSafePos`; `AddVoxels`/`CheckVoxels`; `GetCollideInfo_alt`, `LineOfSight_alt` and
    `GetCubeInfo2`; `DisplayTextWibble`; `DrawBigArtifact`; `DoEffectsLighting`;
    `DisplayAmmo`/`DisplayLights`; `GetOverallGameRating`; `GetHighestLevelCompleted`;
    `ResetMatchCount`; `GetNextFreeControl2`/`GetPrevFreeControl2`; and the replay helpers
    `WriteBits`, `ReadBits`, `GetAbbreviatedVals`, `GetNextFrame`, `WipeMovie` and `TidyMovie`.
    Some of these may still be inlined in, or renamed from, OS4 functions in the next list.
    Confirm each one against the decompile before you rely on it.
  - Small `Close*`/`Reset*` routines that the Amiga inlines into its callers: `CloseMovies`,
    `CloseUserMaps`, `CloseLanguages`, `CloseMap`, `ResetObject`, `ResetStarts`, `ResetAddVehicles`,
    `Obj_Init`, `MessageDequeue`, `FlushOldMessages`, `WipeMessageQueue_script`, `InitRouting` and
    `FindFurthestPoint`.

The complete list is the set of `symbols.tsv` rows whose evidence is `mac-only:game`.

## Amiga code missing from the Mac build

The OS4 functions with no Mac counterpart are the `symbols.tsv` rows with an empty `mac_addr`. Most
are platform code:

- **Startup and runtime glue:** `01000080`–`010008cc` (clib2 startup; "requires AmigaOS 4.0"),
  `010f7368`–`010f7fa4` (68k `000cb23a`, `000cb500`; screen-mode ID strings `ID%02lu…`).
- **Intro video player:** OS4 `01000978`/`01000c48` = 68k `00010364`/`00010556`; also 68k
  `000106c0` ("Unrecognised chunk type"), `00011402`, `0001021c` and `000104d4`. The Mac plays
  the intro with QuickTime instead (`GetPaybackMovie`, `UQuickTime`).
- **Warp3D open:** `01001eb8` (68k `000b0f58`, "Error opening Warp3D library").
- **audio.device mixer:** `01004f54` (mix inner loop), `01005294`/`01005548` (intro-audio stream),
  `01005608`, `01005748` (LoadSample; the Mac `LoadSound` is a rewrite), `01005fc8`, `01007d10`
  (submit/buffer flip), `0100872c`. CD over SCSI: `010084bc`. All replaced by `CMS*` and `UCDAudio`.
- **Text-drawing variants:** `0100b8a8`, `0100ba18`, `0100bd24`, `0100bfac`, `0100c234`,
  `0100c3b0` and `0100c52c` (lightened, word-wrapped and pitch variants in the font table of
  `docs/re/frontend_audio.md`). The Mac keeps `DisplayText*`, `WrapCentredText*` and
  `DisplayTextWibble`.
- **Input hardware:** PSX port (`0103c944`, "psxport.device"), lowlevel/CD32 key names
  (`0103d7b0`), `0103c5b8`, `0103cfb8`, `0103d138`, `0103d2a8`, `0103d438` and `0103d8e8`
  (68k `000409f2`/`00040ad2` are cited in `combat_police.md`), `01040ca0` and `01040e64`.
- **Chunky-to-planar and assembly span fillers:** `0112d1d0` (c2p, native non-RTG screens),
  `0112c224`, `0112ca44`, `0112ccf0` and `0112cd28`, and the second "Blood"/"Blood.dark" effect
  loaders `01035250`/`01035344`.
- **Gameplay-side functions with no Mac name** (they may be inlined into Mac callers or rewritten):
  dynamic lights `0106ba08` (docs: added each frame) and `010738d8`; relight toggle `010792dc`; the
  second `GetCubeTexture` variant `01071dac`; an extra perspective rasteriser `01086d50` (between
  `DrawPerspectiveTriangleGouraud` and `DrawPerspectiveTriangleGenlockGouraud`); replay comments
  `010bbe48` ("InsertComment(): No room…") and its helpers `010bbcf4`/`010bbd58`; the generic
  trigger handler `010cfa80` ("Recieved unknown trigger"); skidmark chain `010cc010`; and mission
  helpers `010cdb78`, `010d5fcc`–`010d7afc` and `010ea4b8`.
- 4-byte empty script-API stubs in OS4 (`010cefc4`, `010cefcc`, `010cf180`, `010d1660`) have
  non-empty Mac bodies; some got a Mac name through the order alignment.

## Files

- `docs/xref/symbols.tsv`: the table. `evidence` records the Mac↔OS4 link (`strings(n)`,
  `callgraph(v=votes,sim=similarity)`, `callee-set`, `sole-callee`/`sole-caller`, `order-gap`,
  `order-edge`, `order-repair`, `triangle-swap`, `via-68k`, `manual:`), with `order=g/t` (how many of
  the 4 nearest matched neighbours agree in address order). After that it records the 68k link:
  `triangle-agrees` (Mac→68k direct and Mac→OS4→68k agree), `via-os4(...)` or `direct(...)`.
- `tools/re/symbols/`: `load.py` (parses the exports, resolves Mac TOC strings via
  `pef.py`), `match.py` (matcher), `final_match.py` (three-way run, triangulation),
  `build_outputs.py` (manual anchors, TSV), `write_md.py` and `demangle.py` (CodeWarrior demangler).
  Re-run: `python tools/re/symbols/final_match.py && python tools/re/symbols/build_outputs.py &&
  python tools/re/symbols/write_md.py` (`write_md.py` appends this hand-written section from
  `tools/re/symbols/symbols_tail.md`; pickled intermediates go to `scratch/symbols_cache/`).
