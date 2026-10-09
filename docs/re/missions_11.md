# Level 11 (city 1, level 1) — mission script reconstruction

Status markers: **CONFIRMED** = read directly from code (68k Update 7; unless noted the OS4
build is identical); **INFERRED** = deduced from context, phrases, map data or naming.
API names follow `docs/re/script_api.md`. Coordinates are world units, with the square
given as `(x>>11, y>>11)`. Angles: 0x1680 = 360° (0x5a0 = 90°, 0x870 = 135°, 0xb40 = 180°,
0xe10 = 225°, 0x10e0 = 270°). Time: 30 ticks = 1 s. Phrase numbers are decimal, with hex in
brackets where the code uses hex. Level phrases come from `Phrases/11.English.phrases`
and globals from `Global.English.phrases` (2022 "Mission complete!", 2028 "Mission
failed!", 2029 "Kill frenzy!").

| Item | 68k (Update 7) | OS4 |
|---|---|---|
| Level init | `0xb96ec` | `0x010eb188` |
| Event handler | `0xc436a` (ends `0xc73b2`) | `0x010f16ec` |
| Mission 2 (bus) helper | `0xb8028` | `0x010e70b0` |
| Mission 3 (kidnap) helper | `0xb82ca` | `0x010e7738` (tracked-trigger helper inlined everywhere) |
| Mission 3 "alarm" sub-helper | `0xb824e` | inlined in `0x010e7738` |
| Mission 9 (RC car) helper | `0xb8f4e` | `0x010ea4b8` |
| Unlock routine | `0xb52c2` | `0x010d9dc8` |
| Common tails | `0xc6770` = `Say2(r); End; ShowMessage(2028); Unlock()`; `0xc6780` = `ShowMessage(2028); Unlock()`; `0xc6d4a` / `0xc6d56` likewise | `LAB_010f2068` / `LAB_010f2074` |
| Legacy CD script | `Maps/11.script`, `main` = `script11:0x12c64`; M2 `0x11502`, M3 `0x11900`, M9 `0x12928` | |

Map: city 1 → `Maps/1.map`. CONFIRMED by phone positions: the map has phones (scenery 14,
special *n*) 10–13 at squares (111,107..110) and 14–17 at (31,115..118). Those are exactly the
phone numbers this script uses.

Working files: `analysis/m11/` (call listings made with `analysis/m11/calls68k.py`, a
symbolic-stack 68k call tracker that follows switch tables).

## 0. Level globals

| 68k | OS4 | Use |
|---|---|---|
| `0x11a354` | `0x0114c370` | **mission state = slot index of the running mission** (1..9). It is −1 when idle: `Mission_Init` sets it and the unlock routine resets it. The phone handler sets it when it starts a mission. |
| `0x2fba08` | `0x0114c97c` | counter/flag "B" (M1 "player near block" + demolition x loop; M4 stage; M5 "boarded"; M8 "alerted") |
| `0x2fb150` | `0x01379c7c` | counter/flag "C" (M1 "bomb in range" + demolition y loop; M4 arrivals; M5 "killed"; M8 "remaining") |
| `0x2fb9f8` | `0x0114c994` | kidnap result: 0 = not done or failed, **1 = M3 succeeded**, **−1 = brother killed in M3 or M5 started** |
| `0x2fba44` | `0x0114c95c` | M5: player is at the meeting point |
| `0x2fba2c` | `0x0114c970` | loop index for release loops |
| `0x2fb130` | – | M2: bus waypoint index (0..16) |
| `0x2fb134` | – | M3: arrival counter for the guard pair 0x12d/0x12e |
| `0x2fb138` | – | M3: number of the pair guards still alive (2 → 1 …) |
| `0x2fb13c` | – | M3: player is at the helipad |
| `0x2fba04` | – | M3: alarm raised **or** brother grabbed (suppresses "seen" reactions) |
| `0x2fb140` | – | M9: target car waypoint index (0..5) |

Model arguments used (vehicle indices from `docs/re/missions_20.md` §6): 0 Mundaneo, 3 Bus,
4 Vapour, 6 Fjord, 13 Taxi, 16 HotRod, 17 Helicopter, 21 BombTruck (RC-car launcher).
Peds: `-7` → model 6, `-9` → 8, `-10` → 9, `-12` → 11.

---

## 1. Level setup — `Init11` 68k `0xb96ec` / OS4 `0x010eb188` (CONFIRMED)

Called by `Mission_Init`. Before this call, `Mission_Init` cleared all 20 slots, set
`0x11a354 = -1` and set the first-refresh flag.

```
intro_phrase(0x2fb224) = 1   // "Good to have you back. Your target score for this level is 2,000,000 points."
slot 1: ev 100  phone 10  visible   -> M1 apartment bomb
slot 2: ev 200  phone 11  visible   -> M2 drug bus
slot 3: ev 300  phone 12  visible   -> M3 kidnap
slot 4: ev 400  phone 13  visible   -> M4 taxi / boss' wife
(no hidden phones in this level; 0x2fb1f8[] stays 0)

AddTrigger(-1, 0x00, 2000000)            // money goal 2,000,000 -> level complete
AddLives(1)                              // +1 life on entering the level
ShowMessage(0)                           // level phrase 0 "Answer the phone" (big centre text)

// Visible-layer zones (see §4.2). AddTriggerEx type 0x39 (rectangle, e = floor):
AddTriggerEx(100000, 0x39, 0x1000,0x34000, 0x9800,0x3d800, 0)   // zone A sq (2,104)-(19,123), floor 0
AddTriggerEx(100001, 0x39, 0x1000,0x34000, 0x9800,0x3d800, 1)   //                           floor 1
AddTriggerEx(100002, 0x39, 0x1000,0x34000, 0x9800,0x3d800, 2)   //                           floor 2
AddTriggerEx(100010, 0x39, 0x1a800,0x36800, 0x1e000,0x39800, 0) // zone B sq (53,109)-(60,115), floor 0
AddTriggerEx(100011, 0x39, 0x1a800,0x36800, 0x1e000,0x39800, 1)
AddTriggerEx(100012, 0x39, 0x1a800,0x36800, 0x1e000,0x39800, 2)

// Optional "kill frenzy" RC-truck bonus (§4.3):
CreateActor(100020, 21 /*BombTruck*/, 0x2bc00,0x25400 /*(87,74)*/, 0, 0xb40 /*180°*/, 1)
AddTrigger(100020, 0x2d, 100020)         // player gets into the truck (no arrow)
AddTrigger(100030, 0x09, 100020)         // truck destroyed before use

0x2fb9f8 = 0
RefreshPhones()                          // rings phones 10-13; first call says intro phrase 1 (speaker 2)
```

INFERRED: zone A contains the hotel, because the M4 end point (9,106) and the "hotel" stop
(20,109) are on its edge. Zone B is the kidnap house of M3, because all M3 guards stand at
(53..59,109..113).

---

## 2. Phone and mission progression (unlock routine `0xb52c2`, CONFIRMED)

Every mission end (success **and** failure) calls `Unlock11()`:

```
Unlock11():
  slot_event[mission_state] = -1                       // the finished slot is emptied
  if mission_state == 3 and kidnap_result(0x2fb9f8) != -1:
      slot 5 = (ev 500, phone 12)                       // ransom handover (M5)
  else if mission_state in 1..5 and slots 1,2,3,4,5 are all empty:
      slot 6 = (ev 600, phone 14)                       // M6 dumped car
      slot 7 = (ev 700, phone 15)                       // M7 love rival
      slot 8 = (ev 800, phone 16)                       // M8 rival gang
      slot 9 = (ev 900, phone 17)                       // M9 RC car on the wife
  RefreshPhones()        // re-adds ringing triggers; if any phone rings, says a random 2150-2158
  mission_state = -1
```

The resulting graph:

```
start ── phones 10 (M1), 11 (M2), 12 (M3), 13 (M4)
M3 ends ── unless the brother was killed (0x2fb9f8 = -1) ──> phone 12 again = M5 (ransom)
all of slots 1-5 empty (M1, M2, M3, M4 done; M5 done or never opened)
       ──> phones 14 (M6), 15 (M7), 16 (M8), 17 (M9)
```

* Every mission is offered once. Failure also consumes the slot: there is no retry.
* The M3 path: success sets 0x2fb9f8 = 1 (M5 briefing phrase 30). Failure by death or by a
  destroyed helicopter leaves 0 (M5 briefing phrase 31, "Even though you failed…"). Killing
  the brother sets −1, so M5 is never offered.
* The "else" in the unlock code means that finishing M3 with slot 5 opened never opens 6–9
  directly. They open only after M5 finishes, or after M1/M2/M4 finish when 3 and 5 are
  already empty.
* Phone events (100..900) are handled only while no mission runs. 68k `0xc4a98`:
  `if (mission_state-1) unsigned > 8` → phone dispatch at `0xc6d6e`. Ringing phones are
  killed with `KillAllPhones()` when one is answered, so this guard is a second safety.
* The level ends only through the money goal (type-0 trigger, 2,000,000).

---

## 3. Handler structure (`0xc436a`, CONFIRMED)

```
Handler11(ev):
  if ev == 0: return
  if ev in {100000..100003, 100010..100013}: zone logic (§4.2); return
  if ev in {100020, 100021, 100022, 100025, 100030, 100031}: RC bonus (§4.3); return
  switch mission_state:                    // table 0xc4ab0
    1: M1 (inline, table 0xc4ad6)     2: M2_Handler(ev) 0xb8028
    3: M3_Handler(ev) 0xb82ca         4: M4 (inline, table 0xc5056, ev-0x191)
    5: M5 (inline, table 0xc57f4, ev-0x1f5)
    6: M6 (inline, table 0xc631a, ev-0x259)
    7: M7 (inline, table 0xc653e, ev-0x2bd)
    8: M8 (inline, table 0xc67a8, ev-0x321)
    9: M9_Handler(ev) 0xb8f4e
    default (idle): phone dispatch 0xc6d6e: ev 100,200,...,900
```

The phone handlers all start with `KillAllPhones(); Dialog_Begin(0); Say2(briefing); End`.
Speaker 0 is the phone; the mission-time voice is speaker 2, and the passengers in M3/M4 are
speaker 1. CONFIRMED: the argument only selects the HUD icon (0 Phone, 1 Lips, 2 Mobile, 3 Pager, 4 Info; `0x21e00`/`0x22300`); the voice is always the same. In the helpers M2/M3/M9 the phone event calls the
helper with ev = −1, which falls into the helper's `default:` branch = mission setup.

Pseudocode notation: `T(ev,type,a,b,c,d)` = `AddMissionTrigger`; `K(ev)` = `KillMissionTrigger`;
`KA()` = `KillAllMissionTriggers`; `D(s, p…)` = `Dialog_Begin(s); Say2(p)…; Dialog_End()`;
`WIN(m)` = `AddMoney(m); ShowMessage(2022); Unlock11()`; `FAIL(p)` = `D(2,p); ShowMessage(2028); Unlock11()`.
All `AddMoney` amounts are multiplied by the bonus multiplier.

---

## 4. Global events

### 4.1 Event 0, death and respawn
* **ev 0** → return (68k `0xc439c`, OS4 first test). CONFIRMED.
* No type 0x3E (respawn) trigger and no level-wide death handling. Every mission adds its
  own type-6 trigger (player dead/arrested) and fails on it. CONFIRMED.
* No secret phones or easter eggs in this level. CONFIRMED.

### 4.2 Visible-layer zones (68k `0xc45e2..0xc4a94`, CONFIRMED)

`SetAmbience(n)` (`0xb4c58`) is the call; per `missions_20.md` it is really
"SetVisibleLayers(n)" (roof cut-away) plus rain volume. For zone A (base 100000) and zone B
(base 100010), `R` = that zone's rectangle:

```
ev base+f (player entered R on floor f, f = 0,1,2):
    SetAmbience(f==0 ? 1 : f==1 ? 2 : 3)
    KillTrigger(the other two floor events, base+3)
    AddTriggerEx(the other two floor events, 0x39, R, their floor)
    AddTrigger(base+3, 0x3a, R)                 // fires when the player leaves R
ev base+3 (left R):
    SetAmbience(3)
    KillTrigger(base+0, base+1, base+2)
    AddTriggerEx(base+0/1/2, 0x39, R, 0/1/2)
```
So on floor 0 one layer is drawn, on floor 1 two, and on floor 2 or outside all three.
The zone triggers are untracked (`AddTrigger`/`AddTriggerEx`), so mission clean-ups never
remove them.

### 4.3 RC-truck bonus "Kill frenzy" (68k `0xc4410..0xc45e0`, CONFIRMED)

Available from level start, independent of the mission state, and only once (the type-0x2d
trigger is one-shot).

```
ev 100020 (player got into BombTruck 100020):
    KillTrigger(100030); AddTrigger(100031, 0x09, 100020)
    D(2, 85)            // "Inside this truck are several remote controlled cars ... more than 15,000 points in the next minute ..."
    ShowMessage(2029)   // "Kill frenzy!"
    AddTrigger(100025, 0x06)            // death
    AddTrigger(100021, 0x0a, 15000)     // money >= money_now + 15000
    AddTrigger(100022, 0x05, 60)        // visible 60 s countdown
ev 100021: D(2,87); KillTrigger(100025,100022); AddMoney(100000); ShowMessage(2022)   // multiplier +1
ev 100022: D(2,88); KillTrigger(100025,100021); ShowMessage(2028)
ev 100025: D(2,89); KillTrigger(100021,100022); ShowMessage(2028)
ev 100030 (truck destroyed, never used): Actor_Release(100020); KillTrigger(100020); D(2,86)
ev 100031 (truck destroyed after use):   Actor_Release(100020); KillTrigger(100021,100022,100025); D(2,90)
```
The bonus does not call `Unlock11` and does not touch `mission_state`, so it can overlap a
mission. Success counts as "Mission complete!" for the multiplier.

---

## 5. Missions

### M1 — Apartment bomb (slot 1, ev 100, phone 10)
68k phone `0xc6dca`, events `0xc4ac2` (table `0xc4ad6`, ev−0x65). OS4 `case 1` of `0x010f16ec`.

Briefing (speaker 0): **2** "It seems the residents of a local apartment block have decided
not to pay their protection money… Ike's stashed a remote-triggered bomb down a nearby
alleyway - go pick it up."

```
ev 100:
  KillAllPhones(); D(0, 2)
  SetScenery(0x6b,0x51,0, 6 /*crate*/, 0x2c /*rot*/, 7 /*special: big bomb*/)   // crate at (107,81)
  T(0x65, 0x25, 0x6b,0x51,0)        // crate at (107,81) smashed / pickup taken
  T(0x96, 0x06)                     // player dead
  SetProjectileId(0x65)             // the placed bomb gets actor id 0x65
  mission_state = 1; near(0x2fba08) = 0
```
State machine (`near` = player at the block when placing, `inrange` (0x2fb150) = bomb
within range of the block). Block centre B = (0x28400,0x38400) = (80,112), floor 0.

```
0x65 (picked up): D(2, 3 "Ok, now put it down next to the apartment block. <Press fire once to place the bomb.>")
                  T(0x66, 0x22, 8)               // player places the remote bomb (weapon 8, CONFIRMED)
                  T(0x67, 0x0f, B, 0)            // player within 0x1000 of B
0x67 (at block):  near = 1; D(2, 6 "Now drop the bomb."); T(0x6e, 0x1c, B)    // player leaves ±0x1800
0x6e (left):      near = 0; D(2, 47 "You're too far away - move closer."); T(0x67, 0x0f, B, 0)
0x66 (bomb placed):
    if near == 1: inrange = 1; D(2, 5 "Great, now get out of there and blow them sky high. <Press fire again to detonate the bomb.>")
                  K(0x6e); T(0x68, 0x16, 0x65, B)          // bomb actor leaves ±0x1800 of B
    else:         inrange = 0; D(2, 7 "You idiot - the bomb is too far away! Move it closer!")
                  K(0x67); T(0x69, 0x3b, 0x65, B)          // bomb actor comes within ±0x1800 of B
    T(0x6a, 0x09, 0x65)                                     // bomb destroyed = detonated
0x68: D(2,7); T(0x69, 0x3b, 0x65, B); inrange = 0
0x69: D(2,5); T(0x68, 0x16, 0x65, B); inrange = 1
0x6a (detonated):
    if inrange == 1:                                        // SUCCESS
        for x = 78..82: for y = 110..113:                   // 5x4 squares, loop vars reuse 0x2fba08/0x2fb150
            if Random(2) == 0:
                Explosion(x*0x800+Random(0x800), y*0x800+Random(0x800), 17000+Random(1500), 0, 3000)
                // drawn last argument first: z, y, x (68k 0xc4d9c..0xc4dc4, Mac 10137f88;
                // OS4 draws x, y, z). CONFIRMED
            SetCube(x,y,1, 0x200,0x200, 0,0,0)              // remove cube level 1 (the block)
        SetFaceTop(sx,sy,0, 1, 0x1dd, 1) for (78..83,114) and (83,110..113)   // retexture surrounding ground [INFERRED rubble]
        KA(); D(2, 9 "Excellent. Here's the dosh."); WIN(220000)
    else: KA(); FAIL(8 "You moron; the bomb was too far away …"); SetScenery(0x6b,0x51,0, 0,0,0)
    SetProjectileId(-1)
0x96 (death): KA(); FAIL(4 "You idiot!"); SetScenery(0x6b,0x51,0,0,0,0); SetProjectileId(-1)
```
Tutorial hints: phrases 3 and 5 (`<…>` parts).

### M2 — Drug bus (slot 2, ev 200, phone 11)
68k phone `0xc6e64` + helper `0xb8028`; OS4 `0x010e70b0`.

Briefing: **10** "A weasel who fancies himself as a druglord has got a shipment of dope
stashed in a bus to the East of the hotel… there's a bazooka stashed down the alleyway
nearest this phone… the petrol tank is at the back of the bus… you've only got 75 seconds…"

```
ev 200: KillAllPhones(); D(0,10)
        SetScenery(0x73,0x6c,0, 6 /*crate*/, 0x2c, 1 /*special: bazooka*/)   // (115,108); the map already has a bazooka crate there
        M2_Handler(-1); mission_state = 2
M2_Handler default (setup):
        CreateActor(0xc9, 3 /*Bus*/, 0x27c00,0x3ec00 /*(79,125)*/, 0, 0x10e0, 6)
        wp = 0; T(0xc9, 0x17, 0xc9, WP[0])           // drive to waypoint
        T(0xca, 0x28, 0xc9)                          // bus destroyed
        T(0xfa, 0x06)                                // death
        T(0xfb, 0x05, 75)                            // visible 75 s countdown
0xc9 (waypoint reached): wp = (wp+1 > 16) ? 0 : wp+1; T(0xc9, 0x17, 0xc9, WP[wp])
0xca: KA(); Actor_Release(0xc9); D(2, 12 "Nice one. Here's your money, but keep an eye out for the cops.")
      WIN(200000); SetWantedLevel(5)                 // wanted level set after the unlock
0xfa: KA(); Actor_Release(0xc9); FAIL(11 "You can't even do the simplest jobs can you?")
0xfb: KA(); Actor_Release(0xc9); FAIL(13 "Too slow, bucko.")
```
WP[i] = (X[i]<<11|0x400, Y[i]<<11|0x400). X is at `0xb7fa0` and Y at `0xb7fe4` (17 ints
each, copied to the stack). In squares: (65,125) (50,125) (50,118) (50,110) (42,110)
(33,110) (33,120) (38,120) (38,116) (50,116) (61,116) (61,109) (69,109) (76,109) (76,115)
(79,115) (79,125), then it loops. CONFIRMED; the CD script has the same tables.
The crate is not removed on failure (unlike M1).

### M3 — Kidnap the rival's brother (slot 3, ev 300, phone 12)
68k phone `0xc6eba` + helper `0xb82ca` (switch ev−0x12d, table `0xb82ec`) and alarm
sub-helper `0xb824e`. OS4 `0x010e7738`.

Briefing: **14** "A rival crimelord's brother has just bought a house in the South of the
city. We've decided to kidnap him. His house is heavily guarded… slip in unnoticed…"

Setup (`default:` branch `0xb8be6`):
```
SetCube(0x38,0x72,1, 0x200,0x200, 0,0,0); SetCube(0x38,0x73,1, 0x200,0x200, 0,0,0)
      // remove cube level 1 at (56,114),(56,115) (slope 0 = not drawn, not solid; CONFIRMED); in the map these are slope-4 cubes with a genlocked face [INFERRED: fence gap = sneak entrance]
SetFaceTop(0x38,0x73,0, 1,0x20e,1); SetFaceTop(0x39,0x73,0, 1,0x20b,1)
T(0x15e, 0x06)                                         // death
CreateActor(0x12d, -12, 0x1ac00,0x38c00 /*(53,113)*/, 0, 0, 0)    // patrol pair, guard A
CreateActor(0x12e, -12, 0x1dc00,0x38c00 /*(59,113)*/, 0, 0, 0)    // guard B
T(0x12d, 0x17, 0x12d, 0x1c000,0x38c00)                 // A walks to (56,113)
T(0x12e, 0x17, 0x12e, 0x1c800,0x38c00)                 // B walks to (57,113)
T(0x15f, 0x31, 0x12d); T(0x160, 0x31, 0x12e)           // guard sees player (no arrow)
T(0x154, 0x09, 0x12d); T(0x155, 0x09, 0x12e)           // guard destroyed
pairAlive(0x2fb138) = 2; pairArrived(0x2fb134) = 0
CreateActor(0x12f, -12, 0x1c400,0x36c00 /*(56,109)*/, 0, 0, 0)   // guard C
T(0x137, 0x17, 0x12f, 0x1c400,0x38400 /*(56,112)*/); T(0x161, 0x31, 0x12f); T(0x156, 0x09, 0x12f)
CreateActor(0x130, -12, 0x1bc00,0x38400 /*(55,112)*/, 1 /*floor*/, 0, 0)   // guard D upstairs
T(0x141, 0x17, 0x130, 0x1bc00,0x37400 /*(55,110)*/); T(0x162, 0x31, 0x130); T(0x157, 0x09, 0x130)
CreateActor(0x131, -9, 0x1b400,0x37c00 /*(54,111)*/, 1, 0xe10, 0)        // the brother, floor 1
T(0x16d, 0x31, 0x131)                                   // brother sees player -> alarm
T(0x164, 0x09, 0x131)                                   // brother destroyed
T(0x14a, 0x3c, 0x131)                                   // player within 1000 of brother, same floor
CreateActor(0x132, 17 /*Helicopter*/, 0x1b800,0x37800 /*(55,111)*/, 2 /*roof*/, 0, 1)
T(0x165, 0x09, 0x132)                                   // helicopter destroyed
alarm(0x2fba04) = 0
```
Patrols (CONFIRMED):
```
0x12d,0x12e (pair reached the middle) / 0x12f,0x130 (pair reached the ends):
    old = pairArrived++
    if old == 1 or pairAlive == 1:       // both arrived, or only one guard left
        if ev in {0x12d,0x12e}: T(0x12f,0x17,0x12d, 0x1ac00,0x38c00); T(0x130,0x17,0x12e, 0x1dc00,0x38c00)  // back to the ends
        else:                   T(0x12d,0x17,0x12d, 0x1c000,0x38c00); T(0x12e,0x17,0x12e, 0x1c800,0x38c00)  // to the middle
        pairArrived = 0
0x137: T(0x138,0x17,0x12f, 0x1c400,0x36c00)     0x138: T(0x137,0x17,0x12f, 0x1c400,0x38400)   // C: (56,109)<->(56,112)
0x141: T(0x142,0x17,0x130, 0x1ac00,0x37400)  (53,110)
0x142: T(0x143,0x17,0x130, 0x1ac00,0x38400)  (53,112)
0x143: T(0x144,0x17,0x130, 0x1bc00,0x38400)  (55,112)
0x144: T(0x141,0x17,0x130, 0x1bc00,0x37400)  (55,110)              // D: square loop, floor 1
```
Detection and alarm:
```
0x15f/0x160/0x161/0x162 (guard k = 0x12d..0x130 sees player):
    if alarm == 0: D(2, 15 "You idiot - one of the guards saw you! Kill him before he sets off the alarm!")
                   Actor_Attack(k, 0x50, -1); T(0x168+(k-0x12d), 0x11, 3)     // 3 s to kill him
0x154 (A died): K(0x168); T(0x130,0x17,0x12e, 0x1dc00,0x38c00); pairAlive--   // partner stops waiting
0x155 (B died): K(0x169); T(0x12f,0x17,0x12d, 0x1ac00,0x38c00); pairAlive--
0x156 (C died): K(0x16a)          0x157 (D died): K(0x16b)
0x168..0x16b (3 s expired) and 0x16d (brother saw player): Alarm()    // 0xb824e
Alarm(): if alarm == 0: alarm = 1; D(2, 16 "Well, that's just great - he tripped the alarm!")
                        Actor_Attack(0x12d..0x130, 0x50, -1)
```
Kidnap and flight:
```
0x14a (reached the brother):
    Dialog_Begin(1); Say1(19 "Move and I'll blow your head clean off!"); Say2(20 "Ok, ok, I'll come quietly."); End
    D(2, 21 "Great, now take him to the helicopter on the roof.")
    Actor_Follow(0x131, -1)                                  // follows the player
    if alarm == 1: Actor_Attack(0x12d..0x130, 0, -1)         // guards stand down
    else alarm = 1                                           // no more "seen" reactions
    T(0x14b, 0x08, 0x132)                                    // player gets into the helicopter
0x14b: D(2, 23 "Now wait for the kidnapee to get on board."); T(0x14c, 0x0c, 0x131)   // brother boards
0x14c: Actor_Release(0x12d..0x130)
       D(2, 24 "Ok, now fly him to our helipad to the North-East. <Press return to take off and land.>")
       P = (0x35000,0x1b800) = (106,55)
       T(0x14d, 0x0f, P, 0)                                  // within 0x1000 of the pad
       T(0x14e, 0x0e)                                        // player left the vehicle
       CreateActor(0x133, -12, 0x35c00,0x1c400 /*(107,56)*/, 0, 0x10e0, 0)   // gang members at the pad
       CreateActor(0x134, -12, 0x35c00,0x1c018 /*(107,56)*/, 0, 0x10e0, 0)
       atPad(0x2fb13c) = 0
0x14d: atPad = 1; D(2, 29 "Ok, now get out."); T(0x14f, 0x1c, P)       // left the pad area
0x14f: atPad = 0; T(0x14d, 0x0f, P, 0)
0x14e (got out):
    if atPad == 1: D(2, 26 "Bring him to us.")
                   PedHelper_b448e(0x131, 0x132, -9, 1)       // brother exits the helicopter
                   Actor_Follow(0x131, -1); T(0x150, 0x3c, 0x134)
    else:          D(2, 25 "Land on the damn helipad, you imbecile!"); K(0x14d); T(0x151, 0x08, 0x132)
0x151 (back in): D(2, 28 "Now fly him to our helipad."); T(0x14d, 0x0f, P, 0); T(0x14e, 0x0e)
0x150 (delivered): KA(); Actor_Follow(0x131, 0x133); Actor_Release(0x131..0x134)
       D(2, 26)        // both builds (and the CD script) say 26 again; phrase 27 "A job well done…" is never used
       AddMoney(350000); ShowMessage(2022); kidnap_result = 1; Unlock11()
```
Failures:
```
0x15e (death):              KA(); D(2,17 "You're an arse, you know that?"); Actor_Release(0x12d..0x134); FAIL-tail   (kidnap_result stays 0)
0x164 (brother destroyed):  KA(); D(2,18 "You idiot - you let the scumball die!"); release 0x12d..0x134; ShowMessage(2028); kidnap_result = -1; Unlock11()
0x165 (helicopter destroyed): KA(); D(2,22 "You blew up the helicopter - how're you going to escape now?"); release; ShowMessage(2028); Unlock11()
```
Note that the `default:` branch of `0xb82ca` is the setup. Any unlisted event reaching M3
would rebuild the mission (CONFIRMED structure; no such event is generated in practice).

### M4 — Taxi: the boss' wife (slot 4, ev 400, phone 13)
68k phone `0xc6ee8`, events `0xc503e` (table `0xc5056`, ev−0x191). OS4 second switch of `0x010f16ec`.

Briefing: **48** "There's a stolen taxi parked in the centre of the city. Fetch."
```
ev 400: KillAllPhones(); D(0,48)
        CreateActor(0x191, 13 /*Taxi*/, 0x27c00,0x1f400 /*(79,62)*/, 0, 0xb40, 1)
        T(0x191, 0x08, 0x191); T(0x1c5, 0x09, 0x191); T(0x1c2, 0x06); mission_state = 4
0x191 (in taxi): D(2, 50 "The boss' wife has been visiting a friend - go pick her up.")
        CreateActor(0x192, -10, 0xac00,0x16400 /*(21,44)*/, 0, 0x5a0, 0)      // wife
        T(0x192, 0x0e); T(0x1c3, 0x09, 0x192); T(0x195, 0x0d, 0x192); stage = 0
0x192 (left taxi): K(stage trigger: 0->0x195, 1->0x196, 2->0x197, 3->0x198, 4->0x199)
        D(2, 52 "Get back in the car!"); T(0x193, 0x08, 0x191)
0x193 (back in): Dialog_Begin(2); per stage:
        0: Say2(53 "Great, now pick up the boss' wife.");        T(0x195, 0x0d, 0x192)
        1: Say2(54 "Ok, now wait for the boss' wife to get in."); T(0x196, 0x0c, 0x192)
        2: Say2(57 "What are you playing at? Pick up the instructor."); T(0x197, 0x0d, 0x193)
        3: Say2(58 "Ok, now wait for him to get in the car.");   T(0x198, 0x0c, 0x193)
        4: Say2(60 "What are you playing at? Take them to the hotel."); T(0x199, 0x10, 0xa400,0x36c00, 0)
        End; T(0x192, 0x0e)
0x195 (near wife): stage = 1; D(2,54); T(0x196, 0x0c, 0x192)               // wife walks to and boards the taxi
0x196 (wife in):   stage = 2; D(1, 55 "Hi handsome. Take me to the hotel. Could you … pick up my - umm - fitness instructor on the way?")
        CreateActor(0x193, -7, 0x2400,0x27400 /*(4,78)*/, 0, 0x5a0, 0)        // instructor
        T(0x1c4, 0x09, 0x193); T(0x197, 0x0d, 0x193)
0x197: stage = 3; D(1, 58); T(0x198, 0x0c, 0x193)
0x198: stage = 4; D(1, 59 "Take us to the hotel, and take your time."); T(0x199, 0x10, 0xa400,0x36c00, 0)   // stopped near (20,109)
0x199 (stopped at hotel): K(0x192); D(1, 61 "Thanks for the lift, hon … watch our backs as we go in the hotel?")
        PedHelper_b448e(0x192, 0x191, -10, 0); PedHelper_b448e(0x193, 0x191, -7, 1)   // both get out
        arr = 0
        T(0x19a, 0x17, 0x192, 0x9858,0x37018); T(0x19a, 0x17, 0x193, 0x9a4c,0x372d0)  // (19,110)
0x19a/0x19b/0x19c (walk legs, both must arrive): old = arr++; if old == 1: arr = 0; next leg:
        after 0x19a: T(0x19b,0x17,0x192, 0x9858,0x38c00); T(0x19b,0x17,0x193, 0x9a4c,0x38c00)   // (19,113)
        after 0x19b: T(0x19c,0x17,0x192, 0x5c00,0x38a40); T(0x19c,0x17,0x193, 0x5c00,0x38e28)   // (11,113)
        after 0x19c: T(0x19d,0x17,0x192, 0x55f0,0x35400); T(0x19d,0x17,0x193, 0x5800,0x35400)   // (10/11,106)
0x19d: old = arr++; if old == 1: arr = 0; T(0x19e, 0x12, 0x192, 0x4c64,0x35400); T(0x19f, 0x11, 1)   // wife enters (9,106) and vanishes
0x19f (1 s later): T(0x19e, 0x12, 0x193, 0x4c64,0x35400)                         // instructor follows
0x19e: old = arr++; if old == 1: KA(); Actor_Release(0x191); D(2, 62 "Nice one. Here's your payment."); WIN(150000)
```
Failures and the alternative ending:
```
0x1c2 (death):       KA(); Actor_Release(0x191,0x192,0x193); FAIL(49 "Idiot!")
0x1c3 (wife dead):   KA(); release; FAIL(51 "You killed her!")
0x1c4 (instructor dead): KA(); release; D(2, 56 "You killed him. Oh well, no great loss. Here's your money.")
                     WIN(200000)            // counts as success and pays more than the normal ending
0x1c5 (taxi destroyed): KA(); release; FAIL(49 "Idiot!")
```

### M5 — Ransom handover (slot 5, ev 500, phone 12; only after M3)
68k phone `0xc6f96`, events `0xc57dc` (table `0xc57f4`, ev−0x1f5). OS4 `case 5`.

Briefing: **30** "The rival crimelord has given in to our ransom demands. We've stashed his
brother in the boot of a blue mundaneo parked in the South-East… Go pick it up." if
kidnap_result == 1, else **31** "Even though you failed to kidnap the rival's brother
earlier, we managed to get someone else to do the job…".

```
ev 500: KillAllPhones(); Dialog_Begin(0); Say2(kidnap_result == 1 ? 30 : 31); End
        CreateActor(0x1f5, 0 /*Mundaneo*/, 0x39c00,0x3dc00 /*(115,123)*/, 0, 0x10e0, 1)
        T(0x226, 0x06); T(0x227, 0x09, 0x1f5); T(0x1f5, 0x08, 0x1f5)
        mission_state = 5; kidnap_result = -1
0x1f5 (in car): D(2, 34 "Ok, now drive to the car park in the South of the city and make the trade.")
        CreateActor(0x1f6, 6 /*Fjord*/, 0x21c00,0x3d400 /*(67,122)*/, 0, 0xb40, 4)   // their car
        CreateActor(0x1f7, -12, 0x21c00,0x3c400 /*(67,120)*/, 0, 0x5a0, 4)
        CreateActor(0x1f8, -12, 0x21c00,0x3cc00 /*(67,121)*/, 0, 0x5a0, 4)
        M = (0x24400,0x3c400) = (72,120)
        T(0x1f6, 0x0f, M, 0); T(0x1f8, 0x0e)
        T(0x230, 0x09, 0x1f6); T(0x231, 0x09, 0x1f7); T(0x232, 0x09, 0x1f8)
        atMeet(0x2fba44) = 0
0x1f6 (at meeting point): atMeet = 1; D(2, 35 "Get out here."); T(0x1f7, 0x1c, M)
0x1f7 (left area):        atMeet = 0; D(2, 36 "Get back to the meeting point."); T(0x1f6, 0x0f, M, 0)
0x1f8 (left the car):
    if atMeet == 1:
        K(0x1f7); PedHelper_b448e(0x1f9, 0x1f5, -9, 1)       // brother climbs out of the boot
        Actor_Follow(0x1f9, -1); K(0x227); Actor_Release(0x1f5)
        D(2, 39 "Wait for them to put the briefcase down.")
        T(0x1fa, 0x17, 0x1f8, 0x22c00,0x3cc00)               // gangster walks to (69,121)
    else: K(0x1f6); D(2, 37 "Get back in the car."); T(0x1f9, 0x08, 0x1f5)
0x1f9 (back in): D(2, 38 "Now head to the meeting point."); T(0x1f6, 0x0f, M, 0); T(0x1f8, 0x0e)
0x1fa: SetScenery(0x45,0x79,0, 23 /*Briefcase*/, 0xb2, 0)     // (69,121)
       T(0x1fb, 0x17, 0x1f8, 0x21c00,0x3cc00); K(0x230); K(0x231); K(0x232)
0x1fb: D(2, 41 "Ok, now pick it up."); T(0x1fc, 0x2c, 0x22c00,0x3cc00, 0)   // on foot within 0x200
0x1fc (briefcase taken): D(2, 42 "Great - kill them for extra money.")
       SetScenery(0x45,0x79,0, 0, 0x59, 0); Actor_Follow(0x1f9, -2)          // brother stops following
       T(0x1fd, 0x3d, 0x1f7, 0x1f6); T(0x1fe, 0x3d, 0x1f8, 0x1f6); T(0x1ff, 0x3d, 0x1f9, 0x1f6)   // all three run to their car
       AddMoney(150000)                                       // ransom paid immediately
       T(0x21d, 0x09, 0x1f6); T(0x21c, 0x09, 0x1f7); T(0x21c, 0x09, 0x1f8); T(0x21c, 0x09, 0x1f9)
       boarded(0x2fba08) = 0; killed(0x2fb150) = 0
0x1fd/0x1fe/0x1ff (one got in): boarded++; if boarded + killed == 3: Escape()
Escape(): T(0x200, 0x17, 0x1f6, 0x21c00,0x3ec00)               // car drives off
0x200: T(0x201, 0x17, 0x1f6, 0x1dc00,0x3ec00)   (59,125)
0x201: T(0x202, 0x17, 0x1f6, 0x19400,0x3ec00)   (50,125)
0x202: T(0x203, 0x17, 0x1f6, 0x19400,0x3e400)   (50,124)
0x203: SetFaceE(0x30,0x7c,1, 0,0,0)                            // open the garage door at (48,124), east face of cube 1
       T(0x204, 0x17, 0x1f6, 0x178f4,0x3e400)   (47,124)
0x204 (got away): SetFaceE(0x30,0x7c,1, 1,0x76,1)              // close the door (map texture 0x76)
       Actor_Release(0x1f6); KA(); FAIL(46 "You let them get away, which means you don't get the extra dosh.")
0x21c (gangster/brother killed): Actor_SetAggro(0x1f6, 100); Actor_Attack(0x1f7,0x50,-1); Actor_Attack(0x1f8,0x50,-1)
       killed++
       if boarded + killed == 3:
           if boarded == 0: Bonus()
           else: D(2, 71 "Now blow up the car."); Escape()
       else: D(2, 44 "Ok, now kill the rest!")
0x21d (car destroyed): killed += boarded; boarded = 0
       Actor_Attack(0x1f7/0x1f8/0x1f9, 0x50, -1)
       if killed == 3: Bonus() else D(2, 44)
Bonus(): KA(); SetFaceE(0x30,0x7c,1, 1,0x76,1); Actor_Release(0x1f6..0x1f9)
         D(2, 43 "Nice one - here's the extra money."); WIN(150000)
```
Failures:
```
0x226 (death):       KA(); SetFaceE(0x30,0x7c,1,1,0x76,1); Actor_Release(0x1f5..0x1f9); FAIL(32 "You plonker!")
0x227 (mundaneo destroyed while the brother is in the boot): KA(); release 0x1f5..0x1f9; FAIL(33 "…the crimelord's brother was in there! …")
0x230/0x231/0x232 (their car or a gangster destroyed before the briefcase is down):
                     KA(); release 0x1f5..0x1f9; FAIL(40 "Oh GREAT! They're never going to make the trade now.")
```
So the base reward is 150,000 at pickup. Killing all three (or destroying the car with the
boarded ones and killing the rest) pays another 150,000 plus "Mission complete!". If they
escape, the result is "Mission failed!", but the 150,000 is kept.

### M6 — Dumped car to Kenny's garage (slot 6, ev 600, phone 14)
68k phone `0xc7058`, events `0xc6302` (table `0xc631a`, ev−0x259). OS4 `case 6`.

Briefing: **63** "Ike's dumped a car in a backstreet. Find it before the cops do."
```
ev 600: KillAllPhones(); D(0,63)
        CreateActor(0x259, 4 /*Vapour*/, 0x1400,0xbc00 /*(2,23)*/, 0, 0xb40, 1)
        T(0x28a, 0x06); T(0x28b, 0x09, 0x259); T(0x259, 0x08, 0x259); T(0x28c, 0x05, 90)   // 90 s visible
        mission_state = 6
0x259 (in car): D(2, 67 "Great, now take it to Kenny's garage in the South of the city. The cops are on to you, so watch out.")
        SetWantedLevel(6); K(0x28c)
        T(0x25a, 0x0e); T(0x262, 0x04, 0x3f,0x74, 1, 3)       // garage (63,116), floor 1 -> player floor 0, west door
0x25a (left car): D(2, 68 "Get back in the car!"); K(0x262); T(0x25b, 0x08, 0x259)
0x25b: D(2, 69 "Now take it to Kenny's garage."); T(0x25a, 0x0e); T(0x262, 0x04, 0x3f,0x74,1,3)
0x262 (garage): KA(); Actor_Release(0x259); D(2, 70 "Excellent. Here's your danger money.")
        SetWantedLevel(0); WIN(225000)
0x28a (death): KA(); Actor_Release(0x259); FAIL(64 "You complete idiot!")
0x28b (car destroyed): …; FAIL(65 "You destroyed the car!")
0x28c (90 s out):      …; FAIL(66 "You were too slow - the law got there first.")
```
Map check: (63,116) cube 1 has a west face of type 1 and texture 0x76 (the same door texture
as the M5 garage). INFERRED: this is the door.

### M7 — Love rival (slot 7, ev 700, phone 15)
68k phone `0xc711e`, events `0xc6526` (table `0xc653e`, ev−0x2bd). OS4 `case 7`.

Briefing: **72** "A friend's just found out I'm banging his wife. Waste him before he tracks me down."
```
ev 700: KillAllPhones(); D(0,72)
        CreateActor(0x2bd, -12, 0x25400,0x4400 /*(74,8)*/, 0, 0, 1)
        T(0x2bd, 0x0d, 0x2bd); T(0x2d0, 0x28, 0x2bd); T(0x2ee, 0x06); mission_state = 7
0x2bd (within 6000): D(2, 73 "There he is! Wipe him out!"); Actor_Attack(0x2bd, 0x5a, -1); T(0x2be, 0x11, 2)
0x2be: Actor_Attack(0x2bd,0,0); T(0x2bf, 0x17, 0x2bd, 0x25400,0x7400)    // runs to (74,14)
0x2bf: Actor_Attack(0x2bd,0x5a,-1); T(0x2c0, 0x11, 2)
0x2c0: Actor_Attack(0x2bd,0,0); T(0x2c1, 0x17, 0x2bd, 0x23c00,0x7400)    // (71,14)
0x2c1: Actor_Attack(0x2bd,0x5a,-1); T(0x2c2, 0x11, 2)
0x2c2: Actor_Attack(0x2bd,0,0); T(0x2c3, 0x12, 0x2bd, 0x23c00,0x6c00)    // (71,13); removed on arrival
0x2c3 (escaped): KA(); Actor_Release(0x2bd); FAIL(74 "You let him escape! …")
0x2d0 (killed):  KA(); Actor_Release(0x2bd); D(2, 76 "Nicely done! Here's your money!"); SetWantedLevel(0); WIN(200000)
0x2ee (death):   KA(); Actor_Release(0x2bd); FAIL(75 "You useless twat! …")
```
He shoots for 2 s, runs to the next point, and repeats, three times, then vanishes into
(71,13).

### M8 — Rival gang (slot 8, ev 800, phone 16)
68k phone `0xc71c2`, events `0xc6790` (table `0xc67a8`, ev−0x321). OS4 `case 8`.

Briefing: **77** "A rival gang are meeting behind some buildings to the North-West. Waste 'em!"
```
ev 800: KillAllPhones(); D(0,77)
        CreateActor(0x321, -12, 0x9c00,0x26c00 /*(19,77)*/, 0, 0x870, 1)
        CreateActor(0x322, -12, 0x9c00,0x27400 /*(19,78)*/, 0, 0x5a0, 1)
        CreateActor(0x323, -12, 0x8c00,0x26c00 /*(17,77)*/, 0, 0xe10, 1)
        CreateActor(0x324, -12, 0x8c00,0x27400 /*(17,78)*/, 0, 0x10e0, 1)
        T(0x33e, 0x28, 0x321..0x324)        // one trigger per gangster, same event
        T(0x321, 0x18, 0x321..0x324)        // any gangster within ±6000 of the player
        T(0x352, 0x06)
        alerted(0x2fba08) = 0; remaining(0x2fb150) = 4; mission_state = 8
Alert(): D(2, 81 "Watch out - they're on to you!"); alerted = 1
         Actor_Attack(0x321..0x324, 0x50, -1)
         for k = 0..3: T(0x32b+k, 0x11, Random(4)+1)          // 1..4 s
0x321 (close): if alerted == 0: Alert()
0x32b+k: Actor_Attack(0x321+k, -1, -1); T(0x335+k, 0x11, Random(4)+1)   // flee from the player
0x335+k: Actor_Attack(0x321+k, 0x50, -1); T(0x32b+k, 0x11, Random(4)+1)  // attack again
0x33e (one killed): if alerted == 0: Alert()
        old = remaining--
        if old == 1: KA(); Actor_Release(0x321..0x324); D(2, 80 "Great! Here's your money."); WIN(250000)
        else D(2, 79 "Nice one. Now get the rest!")
0x352 (death): KA(); Actor_Release(0x321..0x324); FAIL(78 "You moron!")
```

### M9 — RC car on the boss' wife (slot 9, ev 900, phone 17)
68k phone `0xc7392` + helper `0xb8f4e`; OS4 `0x010ea4b8`.

Briefing: **82** "Ok, you should enjoy this. Get in the truck parked in the car park by the hotel."
```
ev 900: KillAllPhones(); M9_Handler(-1); mission_state = 9
M9_Handler default (setup): D(0, 82)
        CreateActor(0x385, 21 /*BombTruck*/, 0x5c00,0x3ec00 /*(11,125)*/, 0, 0x5a0, 1)
        T(0x385, 0x08, 0x385); T(0x3b6, 0x09, 0x385); T(0x3b6, 0x06)
0x385 (in truck): D(2, 83 "I've fitted explosives to a few remote controlled cars … Drive one of the RC's underneath her car and blow her to hell! <Press fire once to launch ac RC, and then press again to detonate it. …>")
        wp(0x2fb140) = 1
        CreateActor(0x386, 16 /*HotRod*/, 0x2b400,0x21c00 /*(86,67)*/, 0, 0x10e0, 6)   // wife's car
        T(0x386, 0x17, 0x386, WP[1]); T(0x3a2, 0x28, 0x386)
0x386 (waypoint): wp = (wp+1 > 5) ? 0 : wp+1; T(0x386, 0x17, 0x386, WP[wp])
0x3a2 (her car destroyed, by any means): KA(); D(2, 84 "Hah! That'll teach the bitch! Here's your money.")
        Actor_Release(0x385); Actor_Release(0x386); WIN(175000)
0x3b6 (truck destroyed or death): KA(); D(2, 17 "You're an arse, you know that?"); release both; FAIL-tail (ShowMessage 2028; Unlock11)
```
WP in squares (stack tables, CONFIRMED; the same in CD): (86,67) (82,67) (82,71) (72,71)
(72,59) (86,59), as `sq<<11|0x400`. The car starts at WP[0] heading to WP[1].
Tutorial hint: phrase 83 (`<…>`; the typo "ac RC" is in the phrase file).

### Reward summary (before multiplier)

| Mission | Success | Notes |
|---|---|---|
| M1 | 220,000 | |
| M2 | 200,000 | then wanted level 5 |
| M3 | 350,000 | sets kidnap_result = 1 |
| M4 | 150,000 (delivery) / 200,000 (instructor killed) | |
| M5 | 150,000 at pickup + 150,000 bonus | escape = "Mission failed!" but keeps the first 150,000 |
| M6 | 225,000 | wanted 6 on entering, 0 on success |
| M7 | 200,000 | wanted 0 |
| M8 | 250,000 | |
| M9 | 175,000 | |
| RC bonus | 100,000 | needs +15,000 in 60 s |

---

## 6. Differences from the legacy CD script (`Maps/11.script`)

I compared M1, M2, M4, M5, M6, M8 (start), M9 and the phone dispatch in detail, and
M3 only by its call list. The structure, events, phrases and rewards are the same.
Coordinates and angles in the CD decompile are truncated (e.g. `0x40` = U7 `0xb40`, money
shows only its low byte), which is not a real difference. Real differences:

1. **M4 taxi-destroyed failure is new.** The CD script has no `T(0x1c5, 9, 0x191)` and no
   0x1c5 case. CONFIRMED.
2. **M4 hotel exit is staggered.** The CD script sends wife and instructor into the hotel
   together on 0x19d. Update 7 sends the wife, then the instructor 1 s later (new 0x19f
   timer). CONFIRMED.
3. Everything else matches, including the M3 success line that reuses phrase 26 and the
   M5/M3 kidnap flag logic. The CD unlock logic was not traced (it uses the stub at
   `0x11384`).

## 7. API and trigger notes found while working on this level

* **`SetScenery(sx,sy,cube,scenery,a5,a6)`: a5 goes to cell `+0` (rotation) and a6 to
  `+4` (special, i.e. crate contents).** This is opposite to the names in `script_api.md`
  ("special, rot"). CONFIRMED in 68k `0xb21a6` (param_5 → +0, param_6 → +4) and OS4
  `0x010cde90`. Here: crate + special 7 = big bomb (M1), crate + special 1 = bazooka (M2).
* `SetCube(sx,sy,cube,h1,h2,scenery,rotation,slope)` (writes +2,+6,+1,+0,+8). Slope 0 = cube
  off. CONFIRMED.
* `PedHelper_b448e(newId, vehicleId, pedModel(<0), p4)` spawns ped `~pedModel` with
  id `newId` stepping out of the vehicle with id `vehicleId`. It is used for passengers
  leaving (M3 brother, M4 wife/instructor, M5 brother from the boot). CONFIRMED structure;
  p4 is the side: 0 = right (the player's own exit side), non-zero = left (`0xdb196`, CONFIRMED).
* `Actor_Follow(id, -1)` makes id follow the player (`+0x41a = 1`). `-2` (any other negative)
  clears the follow. `Actor_Follow(id, other)` follows actor `other`. CONFIRMED (`0xb2072`).
* `0x11a354` is written by the level code (mission/slot index), not by the phone system.
* Type 0x17 on a pedestrian is "walk to" (M3 guards, M4 wife/instructor, M5 gangster).
  Type 0x12 is used as "walk to and vanish" (M4 hotel door, M7 escape).
* `ShowMessage(0)` in init displays the level's phrase 0 ("Answer the phone").

## 8. Open questions

1. ~~Type 0x22 with a = 8~~ Resolved: 8 = the remote bomb, sent when it is placed; the bomb
   carries the `SetProjectileId` id (CONFIRMED, `0xa6c62` in `0xe3a58`, `0xefece`).
2. ~~M5 counting~~ Resolved: no over-count. Boarding (`0xa6ae2`) sets the ped to state 6, removed
   by `0xdc5ac` → `0xcf290`, which never calls `0xa653a`; type 9 is not fired. CONFIRMED.
3. ~~M3 fence~~ Resolved: (55..57,115) cube 1 are the yard's front fence (genlocked south
   face `Fence2`, texture 125); (56,114) cube 1 is already a 2-byte (empty) cube under the
   wall's cube 2, i.e. the house's doorway (seen in the port: a dark door in the middle of
   the front wall behind a chain-link fence). M3 removes the fence piece in front of the
   door, opening a way in (CONFIRMED, data and a port screenshot).
4. ~~M1 ring~~ Resolved from the data: 0x1dd is `Snow.chunky`, plain snow. The ring squares
   are the block's painted shadow (map 1 cube 0 tops: (78..82,114) `Snow.Dark`, (83,114),
   (83,112..113) `SnowShad3`, (83,111) `SnowShad1`, (83,110) already `Snow`), so the edit
   removes the shadow of the demolished block (CONFIRMED, data).
5. ~~Speaker ids~~ Resolved: `Dialog_Begin(icon)` only selects the HUD icon (0 Phone, 1 Lips,
   2 Mobile, 3 Pager, 4 Info); the voice is always SqueakyVoice. CONFIRMED (`0xb40ec` → `0x21e00`).
6. ~~RC bonus clock~~ Resolved from the code: the HUD shows the first type-5 trigger in table
   order (Mac `GetTimerVal` @ 100f6b10 scans from index 0; 68k `0xa6978` the same). Triggers
   are appended (`ProcessToDoList` @ 100f4880) and a removed one is replaced by the last
   (`RemoveTrigger` @ 100f4560). So when the bonus starts during M2 (75 s) or M6 (90 s), its
   60 s clock (ev 100022, the last of the three triggers added) is behind the mission's timer
   and the mission clock shows. The first removal of any trigger at a lower index afterwards
   (a zone rectangle 100000-100012 firing as the player crosses its edge, a mission trigger
   firing or being killed) moves the bonus clock into that slot, in front of the mission
   timer, and from then on the bonus clock shows until it goes. Only one clock is ever
   drawn. CONFIRMED (code); the port does the same (`mission/eval.rs`,
   `mission/triggers.rs`, MS-2).
7. The helper `default:` branches (M2/M3/M9) rebuild the mission on any unknown event.
   Harmless in practice, but a reimplementation should handle setup only for ev = −1.
8. Ped models 6/8/9/11: 6 PedMale1 (white + blue), 8 PedMale2 (red + white), 9 PedMale3 (all
   blue), 11 PedMale5_shotgun (dark blue + white, holds a shotgun). All are recolours of the
   male body; there is no female model, so the "wife" is a male recolour (CONFIRMED, load order
   `0xd7874`). Body parts (limb groups of people.md §2.2, from the `.Obj` files): PedMale1
   white shoulders/arms, blue hips/legs; PedMale2 red top, white trousers; PedMale3 dark blue
   top, blue trousers; PedMale5_shotgun dark blue top, white trousers, a static shotgun
   (limb 0, `Assorted1`/`Tree` faces). Head and hands are the shared `AmigaFront`/`Face`
   skin (CONFIRMED, data).
