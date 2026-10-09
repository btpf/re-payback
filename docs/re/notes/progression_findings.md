# Progression, saving and script API: Update 7 working notes

Working notes from the mission-script pass (2026-10-07). 68k Update 7 addresses unless
marked OS4. The findings were folded into `docs/re/frontend_menus.md`, `docs/re/script_api.md`
and the `missions_*.md` notes; this file keeps the original list. C = CONFIRMED, I = INFERRED.

## KEY CORRECTIONS

- Tick rate: 68k game runs at 25 ticks/s, not 30 (C). OS4 = 30 Hz. Scripts still assume 30 ticks = 1 s.
- 0xb4666/0xb46dc = Heli_TakeOff/Heli_Land (not siren) (C).
- +0x444 = arrival radius, not speed (C).

## A. PROGRESSION

- **A.1** Title menu FUN_00033192: 2090 Start new game, 2091 Load, 2092 View replay, 2093 Credits, 2094 CD audio, 2454 Control setup, 2132 Language, 2109 Intro, 2130 Quit.
  New game: name default "Travis" (0x29836), edited under 2088 (max 25 chars) -> 0x136f30 (50 bytes); on OK best times 0x136f38[6]=-1, trophies 0x136f50[6]=0; opens game menu FUN_00031150. (C)
  Game menu FUN_00031150: 0 "Play single player game"(2134)->level select FUN_0002c8ea; 1 multiplayer(2133); 2 rampage(2197) sets 0x11a6e6=1 (= rampage flag); 3 Save game FUN_00028d9e; 4/5 view/save replay; 6 End game. "GAME COMPLETE!"(2047) under item 0 when all trophies>0. (C)
  Chosen idx -> 0x3481d8; city 0x348194=idx>>1, level 0x34c418=idx&1 (0xeddc8). Police AI also reads 0x3481d8 for difficulty. (C)
- **A.2** Reset at EVERY level start (before Mission_Init): mult 0x30c3b0=1, lives 0x34c428=3, 0x34c190=0, 0x34c194=0, 0x34c078=3, 0x11a734=-1, 0x34c288=-1, timer 0x349180=0, respawn 0x922ec(-1,0,0). Player recreated, nothing carried over; money 0 (I).
  Level-init AddLives: 00 +2 (0xb91dc) ->5; 01 +2 (0xb9324) ->5; 10 +1 (0xb953c) ->4; 11 +1 (0xb96ec) ->4; 20, 21 none ->3. (C)
- **A.3** Intro card FUN_0002c05a (0xee4fa): city name 2170+city, level name 2180+idx (Ready Or Not, Gangster Tripping, Going For Gold, Remote Control, Caught By The Fuzz, Born Slippy), gold/silver times, cup, best time. (C)
- **A.4** Level end (~0xefa40): state 3 -> FUN_0002b666(city,level,minutes=0x349180/1800), then back to menu FUN_00033192; player picks next level (no auto-advance). State 2 -> FUN_00029e1a (2045/2046), nothing recorded. 0x349180 only advances while state 0 (0xeef00). (C)
- **A.5** Level select FUN_0002c8ea (2087): cursor = first idx (00,01,10,11,20,21) with trophy 0; if all done, lowest trophy. Any level can be highlighted; confirm only if idx==0 or trophy[idx-1]!=0, else sound 0x2d. Strict linear unlock 00->01->10->11->20->21. Esc returns -1. Stores 0x13c9b8 city / 0x13c9bc level. (C)
- **A.6** Trophies FUN_0002b666 (OS4 FUN_0102395c): unit = game minutes (0x349180/0x708), shown h:mm.
  Table 0x137758[city*2+level] = {gold,silver} (OS4 0x117b6c8 same): 00 8/12, 01 10/14, 10 11/15, 11 8/12, 20 4/7, 21 10/15.
  t<=gold ->3 (2043); t<=silver ->2 (2042); else 1 (2041). 2040 "Total time:", 2110 "Continue".
  Best time+trophy written together only if t<best or best==-1 (ties do not update). t>=1,080,000 ignored.
  Overall rating = min trophy over 6 levels (cap 3); if it changed -> FUN_0002a842 GAME COMPLETE screen (2047/2048), 2072+r (2073-2075), 2075+r (2076/2077/2078). Shown on first full completion, all-silver, all-gold. (C)
- **A.7** 2159 "View outro" unreferenced in 68k and OS4, no "outro" string -> outro unused (I). 2049 also unreferenced in 68k.
- **A.8** HUD goal in pause menu FUN_0002550a (0x26456), single player only: GetGoal 0xf0154 (0x34c440) <=0 -> 2056 "Goal: Escape from jail", else 2058+n+2057. 0x34c440 is set by AddTrigger type 0 BEFORE the a<=0 check (0xb2b12 case 0); level 20 passes -1 -> "Escape from jail". Level 20 init 0xb98f4 calls SetRespawnPoint(12,20,1): affects respawns only (player already placed at 0xee32c before Mission_Init 0xeea3a); handler 0xc73b4 ev 0x30d43 resets it to -1 and calls LevelComplete. (C)
- **A.9** Demo build (0x34c114!=1): 9000-tick cap (0x34c414) -> 2490 "Time out!", state 1. (C)

## B. SAVING

- **B.1** 68k 0x1dfb4 Load (OS4 0x101182c), 0x1e198 Save (0x1011a70), 0x36d5c SlotExists (0x1011cc8), 0x36df8 ReadSlotName (0x1011da8, "Slot %d" fallback). Paths PROGDIR:Saves/PaybackSlot%d.save then S:PaybackSlot%d.save. 14 slots (1-14), menu item 14 = cancel. Load menu FUN_00028a1e (2082/2111/2112); save menu FUN_00028d9e (2084/2083). Default slot name (OS4 0x101f738) "%s - C%d, L%d" = name, city+1, level+1 of first trophy-0 level. (C)
- **B.2** File, 152 bytes big-endian: u32 version (=1, ignored on load); char[50] slot name; char[50] player name (0x136f30); then for c00,c01,c10,c11,c20,c21: s32 best_minutes (-1 none), s32 trophy (0..3). No lives/money/multiplier. Unlocks derived from trophies. (C)
- **B.3** Saves only on demand from the game menu between levels; no autosave. (C)

## C. LIVES / DEATH / EVENT 0

- **C.1** Death/arrest: 0x34c1c8[p]=0x50 (80 ticks), money stored in 0x34c1d8[p] (0xdc5ac, 0xde0b6, 0xe3a58). Arrest: 2017 "Busted!" + crime summary FUN_00094d52. (C)
- **C.2** Timer expiry 0xeed16: lives-1, mult=1; lives<1 -> lives 0, state 2, msg 2026, 0x34c194=100, 0x34c1c8=0xfa; else Trigger_PlayerRespawned 0xa69c2, on-foot player at respawn point (FUN_00043c96), money restored in full. No money loss found. (C)
- **C.3** 0x34c194 armed to 100 (game over 0xeee4c, demo timeout), 60 (quit to title, state 1, 0xda2e0; level complete after input slot 7 0x34c188 pressed in state 3 - I: "continue" key), 1 (Esc in replay 0xf1988). Decremented by ticks; at <=0 held at 1 and 0x34c42c=1 -> Mission_Event(0) called once (0xeef0c), loop exits (0xef1fa), Mission_Shutdown 0xef444. Event 0 = "level ending, any reason"; all handlers ignore it (00,01,10,11,21 return; 20 falls through). (C)

## D. API

- **D.1** Dialog_Begin(n) 0xb40ec -> 0x21e00 queues {1,n}; FUN_00021f7e sets 0x117cde=n; FUN_00022300 draws icon 0x1380d8+n*16: 0 Phone, 1 Lips, 2 Mobile, 3 Pager, 4 Info (scripts use 1,2,4). Say1/Say2 queue {2,text,style} via 0x35eae; style = font index for FUN_0001e36c (0x130f30+style*0x1000): 0 big_, 1 you_, 2 them_ -> Say1 = player (Travis), Say2 = other party. Type-2 entry starts sound 0x10. (C)
- **D.2** Actor_Follow 0xb2072: leader -1 -> +0x41a=1 (player); other negative -> 0 (stop); id>=0 -> first actor with that id (unchanged if none). AI 0xe3a58 (~0xe5200): +0x41a!=0 -> +0x422=1, target +0x41e. (C)
- **D.3** Actor_Attack 0xb28ac: mode 0 -> state 4, +0x346=0, +0x41e=0; mode>0,target -1 -> 0x10 attack player; mode>0,target id -> 0x11 attack last actor with id; both set +0x346=mode, weapon 5 shotgun with 100 shells (+0x35e=5,+0x362=100); mode<0,target -1 -> 0x12 flee player. Fire rule: target within ~6000, cooldown +0x490==0, rand(0x65-mode)<ticks -> +0x346 acts as fire rate. (C)
- **D.4** 0xb4510 -> state 5 inert/parked (I), id cleared; 0xb42b8 -> state 3 ambient drone, id cleared, +0x2ee=-1; 0xb459a -> state 6 removed. All skip state 2 and dead actors. (C for writes)
- **D.5** States: 0x0d walk to player's vehicle and board (FUN_000e361e waypoints +0x30e 5->1..4; nothing if target on foot; fires type 0x0c via 0xa6a7a; then state 6). 0x13 same for actor b, fires 0x3d via 0xa6ae2(id,target id). 0x0e goto (+0x2e6,+0x2ea), |dx|+|dy|<0x579 fires 0x12 (0xa6b5a), state 6. 0x0f goto, radius +0x444 (2x from vehicle front for vehicles), fires 0xa6640, state 4; police in 0x0f exempt from pursuit override. 0x15 same but not exempt (I: only difference). 0x14 flee via road graph to node farthest from player (FUN_000a430a, refreshed every 20 ticks); 0x43 fires beyond +-33000. 0x10/0x11 attack, 0x12 flee. +0x49e=1 (type 0x32) removes steering clamp (I). (C)
- **D.6** 0xb4284 -> 0xefe62: 0x11a734 = forced traffic model (index into 0x348120); spawner FUN_000cf5cc spawns it when none exists; -1 random. (C)
  0xb4360 ShowCheckpointMarker: 0x34c28c; with arrow target 0x34c288/94/90 (0xeff2a) >=0, renderer (0xe1a3x -> FUN_00027216) draws flashing Checkpoint.red/blue icon at the target. (C)
  0xb48be SetRespawnPoint(sx,sy,floor): 0x922ec -> 0x26c040..48; start position FUN_00043f0e and respawns FUN_00043c96; -1 -> random from 0x26c050[], avoiding last 4. (C)
  0xb475c: +0x386 AI max speed. (C)
  0xb483c: +0x49c player-controls enabled, default 1 (0xa2b80, 0x974a0); 0 zeroes steer/throttle when the player drives it. (C)
  0xb4666/0xb46dc: helicopter take-off/land; only models with +0x4928!=0; +0x498 flying 1/0, clears +0x49a; land sets speed 0. (C)
  0xb2590 8th arg: +0x3ca damage filter (-1 all; else 4 minigun, 7 pistol, 5 shotgun, 2 flame, 1/3 explosion). (C)
  0xb47b8 SetRaceLights(n): 0x117d3e = icons/lights_n (0-3, -1 none); sound 0x37 for 1/2, 0x38 for 3. (C)
  0xb47ca SetLap(cur,total): 0x117d42/0x139288, "Lap %d of %d"; sound 0x39 when cur>1. (C)
  0xb4c58 SetVisibleLayers(n): 0x34c078 = cube layers drawn by FUN_000d1058; 3 = all (default); rain loop 0x34c434 volume 0xff if n==3 else 0x7f. (C; exact cut-off rule I)
  0xb43d4(id,~m) ped exits player's vehicle; 0xb4414(newId,vehId,~m) ped exits vehId, side alternates via 0x34c1b0; 0xb448e(newId,vehId,~m,side) explicit side (0xdb196). All set +0x448=1. (C)
  0xb4382 AddEffect(x,y,z,vx,vy,vz,life,p8): particle type 9, "Gas" texture 0x34c12c, velocities /3, extra arg 0x30000; only level 10 (0xc1a92). (C type/texture; arg roles I)

## E. TRIGGERS / TICK RATE

- **E.1** 0x0c: actor boarded player's vehicle (see state 0x0d). (C)
- **E.2** 0x22 (0xa6c62(w)) from player fire code in 0xe3a58: 1 bazooka +0x366, 2 flamethrower +0x36a, 3 grenade +0x36e, 4 minigun +0x372, 5 shotgun +0x362 (pellets 0x30c410), 7 pistol +0x376, 8 remote bomb +0x37a (bomb gets id 0x11a77e or -2). Pickup ammo: 4/75/6/100/20/30/1. +0x35e ids differ: 0 fist,1 bazooka,2 flame,3 grenade,4 minigun,5 shotgun,0xd pistol,0xe bomb. (C)
- **E.3** 0x27 (0xa6c12): player vehicle brake/reverse branch with steering 0x34c3ac!=0, model +0x4926==0, once per press (+0x3c1); sets yaw +0x326=+-2*model[+0x48e4]. Handbrake spin (wording I).
- **E.4** 0x2a: cube byte +8 (slope) becomes 0 = glass (face type 3) smashed, sound 0x31 (FUN_000446b8, 4 sites). Level 01 shop windows. (C)
- **E.5** 0x2b (0xa6a12(id)): actor fell into water, +0x44a set, splash sound 0x2b, peds health -71. Sites 0xe0e46, 0xe109e, 0xe3a58. (C)
- **E.6** 0x3d: actor a boarded actor b's vehicle. (C)
- **E.7** 0x42 (0xa6d26): only if count 0x1196f0>0; fires each frame the player presses input 0x34c158 (bit 0x1000, RC throttle; direction I) while remote-controlling a model with +0x4904!=0 and 0x3480a0[p]==actor. Fire on RC = self-destruct (-71). Level 21 (0xc79cc). (C)
- **E.8** 0x29: cop killed (0xccc96, +0x490a==1); one trigger per kill; +2500*mult, every 11th +100000*mult (2037), wanted raised. (C)
- **E.9** Tick rate: frame loop 0xef13a; FUN_00110e28 = DateStamp ticks (dos LVO -0xC0, DOSBase 0x34cce0, 50 Hz); ticks=(elapsed+1)>>1 clamped 1..5 (0x348090); busy-wait until elapsed>=2*ticks -> 25 ticks/s on 68k. Scripts assume 30 ticks/s (type 5/0x11 x30, HUD /30, minute = 1800 ticks) -> 1 script second = 1.2 s real, trophy minute = 72 s real. OS4 converts x60/50 -> 30 Hz (0x106a058). (C)
