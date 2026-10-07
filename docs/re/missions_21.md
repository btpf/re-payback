# Level 21 (city 2, level 1): mission script reconstruction

Target: **Update 7** `Payback.exe` (68k, flattened at 0x10000). OS4 port used to confirm.
CD `Maps/21.script` used for comparison only (§7).
Status markers: **CONFIRMED** = read directly from code, **INFERRED** = deduced.
API and trigger names follow `docs/re/script_api.md`. Coordinates are world units, with
map squares given as `(x>>11, y>>11)`. Map = `Maps/2.map`. North is −y: phrases that say
"North/South-West/..." agree with this, except the one noted in §8.

How this was produced: `analysis/missions21/trace68k.py` disassembles the handler
recursively and tracks the arguments each call pushes on the stack.
Its output is `analysis/missions21/handler_c79cc.txt` (full) and `handler_c79cc_short.txt`
(calls only). `; from X top=[...]` lines show the stack top on each branch into a shared tail.
`analysis/missions21/rng.py <listing> <a> <b>` prints an address range from a listing.
All argument values below were taken from this trace and checked against
the OS4 decompile (`0x010df934`).

## 0. Entry points and globals

| What | 68k | OS4 |
|---|---|---|
| Level init | `0xb9aaa` | `0x010ebbe8` |
| Event handler | `0xc79cc` (size 0x25fe) | `0x010df934` |
| Unlock routine ("mission finished → open phones") | `0xb538e` | inlined |
| Race helper (`Race21(ev)`) | `0xb6e48` | inlined |
| `Mission_Event` dispatcher | `0xca368` | `0x010f45cc` (ignores event 0) |

The handler returns at once on event 0 (`0xc79d8 tst.l a3; beq`), so event 0 is ignored.
CONFIRMED. Otherwise it switches on **`0x11a354` = current mission slot** (`0xc79ee`,
jump table base 1):

| `0x11a354` | handled at | events in the table |
|---|---|---|
| 1 | `0xc7a06` | 101..154 |
| 2 | `0xc80fe` | 201..252 |
| 3 | `0xc8384` | 1..10, others → `Race21(ev)` |
| 4 | `0xc8642` | 401..451 |
| 5 | `0xc87f2` | 520..529, then 501..552 |
| 6 | `0xc9080` | 601..651 |
| 7 | `0xc958a` | 701..751 |
| 8 | `0xc9a6a` | 801..812, 820, 830, 850, 851 |
| anything else (−1 or 0) | `0xc9c22` | mission starts: 100, 200, 300, 400, 500, 600, 700, 800 |

**Who sets `0x11a354`:** the level handler itself, when a mission starts. The phone
code (`RefreshPhones`/trigger type 1) does not set it. For this level the value equals
the phone slot index (mission N uses slot N). CONFIRMED. The script_api wording "the
answered slot index is stored in `0x11a354`" holds only because the level code writes it.
Start events are therefore accepted only when no mission is running.

Level globals (68k; cleared by `Mission_Init`: `0x2fba44/4c/50`):

| Addr | Use |
|---|---|
| `0x2fba4c` | M1: bomb flag (kidnapper killed after giving the location). M4: "van alerted" one-shot. M5: "gang attack started". |
| `0x2fba44` | M5: "first vehicle entry seen" flag. M7: "truck inside target zone". |
| `0x2fba50` | M5: Nick is in the player's car. M7: player is in the truck. |
| `0x2fba48` | M5: gang members remaining (4→0) |
| `0x2fba30..0x2fba3c` | M5: alive flags of gang peds 510..513 (set to 1, **never cleared**, see M5) |
| `0x2fba2c`, `0x2fba08`, `0x2fb154` | loop counters |
| `0x2fb068[5]` | race: AI waypoint index per racer |
| `0x2fb080[6]`, `0x2fb098[6]`, `0x2fb0b0` u16[6] | race: checkpoint progress, lap (starts 1), finished flag per car |

## 1. Level setup: init `0xb9aaa` (CONFIRMED, OS4 identical)

```
0x2fb220 = 0                       // clears "first RefreshPhones" flag BEFORE RefreshPhones:
                                   //   intro phrase 0x2fb224 is never shown on this level
AddTrigger(-1, 0x00, 2000000)      // money goal 2,000,000; firing = level complete (handler(-1) is a no-op)
RefreshPhones()                    // all 20 slots empty here -> adds nothing, says nothing
CreateActor(50, model 0 (vehicle), 0x26400, 0x17000, floor 0, angle 0, colour 4)     // sq (76,46)
AddMissionTrigger(100, 0x08 get-into-actor, 50)                                       // -> event 100
CreateActor(1000000, model 23 (vehicle), 0x1e2d0, 0x1db64, floor 0, angle 0x10e0, colour 0)  // sq (60,59)
ShowMessage(0)                     // big text phrase 0 "Get in the car"
Dialog_Begin(2); Dialog_Say2(1); Dialog_End()
   // 1 "It's good to finally meet you. Your target for this level is 2,000,000 points. Get in the car."
```

* Actor id 1000000 (vehicle model 23, facing 270°) is never referenced again: a static
  parked vehicle. INFERRED.
* Lives are not changed by the script (game default). No map edits at start.
* No phone slots are filled at init. Mission 1 is started by getting into car 50.
  The phones open through the unlock routine (§2).
* No global type 6 / 0x3e triggers. Each mission adds its own type-6 "player dead" trigger (§5).

### Phones of map 2 (from `Maps/2.map`, scenery 14 + special n)

| Phone | Square | Used by |
|---|---|---|
| 1 | (73,42) | slot 2 → ev 200 (M2 rocket car) |
| 2 | (73,43) | slot 3 → ev 300 (M3 race) |
| 3 | (73,44) | slot 6 → ev 600 (M6 bank van) |
| 4 | (126,14) | M1 kidnapper call (direct type-1 trigger, not a slot) |
| 5 | (71,122) | slot 4 → ev 400 (M4 surveillance) |
| 6 | (72,122) | slot 5 → ev 500 (M5 Nasty Nick) |
| 7 | (73,122) | slot 7 → ev 700 (M7 bomb truck) |
| 8 | (74,122) | slot 8 → ev 800 (M8 grenades; this phone is blown up by the mission) |
| 10 | (112,126) | unused |

All slots are visible: the hidden flag `0x2fb1f8` stays 0, so they use trigger type 1 with an arrow.

## 2. Progression: unlock routine `0xb538e` (CONFIRMED, CD identical)

```
Unlock21():
  s = 0x11a354
  slot_event[s] = -1                      // finished mission's slot is emptied: success OR failure
  if s == 1:
      slot 2 = (ev 200, phone 1); slot 3 = (ev 300, phone 2); slot 6 = (ev 600, phone 3)
  elif s in {2,3,6} and slot_event[2] == slot_event[3] == slot_event[6] == -1:
      slot 4 = (ev 400, phone 5); slot 5 = (ev 500, phone 6)
      slot 7 = (ev 700, phone 7); slot 8 = (ev 800, phone 8)
  RefreshPhones()                          // re-adds ringing type-1 triggers for every filled slot;
                                           // if any rings, speaker 2 says random global phrase 2150..2158
  0x11a354 = -1
```

```
M1 (car 50 → phone 4) ──► {M2 rocket car, M3 race, M6 bank van}  (any order)
                         └─ all three finished ─► {M4 surveillance, M5 Nick, M7 bomb truck, M8 grenades}
```

* **A failed mission is gone for good:** its slot is cleared and never refilled. Every mission
  runs once, pass or fail. CONFIRMED.
* When a mission starts, its start handler calls `KillAllPhones()`, which stops the other
  phones ringing. Their slots stay filled, and `RefreshPhones()` brings them back at the end.
* The level ends only when money ≥ 2,000,000 (type-0 trigger, can fire mid-mission).
  After the last mission there are no more phones.

Reward table (`AddMoney` is multiplied by the bonus multiplier. `AddMoney` runs before
`ShowMessage(2022)`, so a mission pays at the multiplier it was started with):

| Mission | Reward |
|---|---|
| M1 | 300,000 (kidnapper killed, bomb defused in time) / 150,000 (normal) |
| M2 | 250,000 |
| M3 | 1st 255,000 / 2nd 170,000 / 3rd 85,000 / 4th–6th fail |
| M4 | 200,000 |
| M5 | 200,000 |
| M6 | 200,000 |
| M7 | 200,000 |
| M8 | 150,000 |

Common fail tail `0xc9c02`: `Dialog_Say2(p); Dialog_End(); ShowMessage(2028 "Mission failed!"); Unlock21()`.
Common success tail `0xc9c12`: `ShowMessage(2022 "Mission complete!"); Unlock21()`.

---

## 3. Missions

Notation: `AMT(ev, type, a, b, c, d)` = `AddMissionTrigger`. `Say2(n)` means
`Dialog_Begin(spk); Dialog_Say2(n); Dialog_End()` with `spk` = 2 unless stated.
`FAIL(p)` = `Dialog_Begin(2); Say2(p); End; ShowMessage(2028); Unlock21()`.
`WIN(p, $)` = `Dialog_Begin(2); Say2(p); End; AddMoney($); ShowMessage(2022); Unlock21()`.
Speaker 0 is used for phone briefings, 1 for the player's own lines (INFERRED from the text).

### M1: The kidnapping (slot 1, start event 100) 68k `0xc9c76` + `0xc7a06..0xc80fe`

Actors: 101 = kidnapper (vehicle model 23), 102 = "blue Fjord" (vehicle model 6), 50 = start car.
Map cells: briefcase pickup (86,43), drop-off (59,33). Warehouse door (28,110) east face, floor 1.

```
ev 100  (player got into car 50)                                            @0xc9c76
  Actor_Release(50)
  Say2(2) "The boss helped spring you ... South-East corner ... answer it before he gives up."
  0x11a354 = 1
  AMT(101, 0x01 phone, 4)                       // phone 4 at (126,14), rings, arrow
  AMT(150, 0x06 player dead)
  AMT(151, 0x05 countdown, 30)                  // visible 30 s clock

ev 101  (phone 4 answered)                                                  @0xc7a86
  KillMissionTrigger(151)
  Dialog_Begin(0); Say2(6) "Ok, here's the deal. You collect 250 grand from your boss and leave it at the drop off point in the docks..."
  SetScenery(86,43, cube 0, scenery 23 Briefcase, byte0 0x85, byte4 0)
  AMT(102, 0x2c on-foot-at, 0x2b400, 0x15c00, floor 0)   // (86,43), arrow
  0x2fba4c = 0

ev 102  (picked up briefcase)                                               @0xc7af2
  SetScenery(86,43,0, 0, 0x59, 0)               // remove briefcase (0x59 = restore value used everywhere)
  Say2(4) "Now take it to the docks."
  AMT(103, 0x2c, 0x1dc00, 0x10c00, 0)           // drop point (59,33)

ev 103  (at drop point)                                                     @0xc7b4c
  SetScenery(59,33,0, 23 Briefcase, 0x9c, 0)    // case left on the ground
  Say2(7) "Now wait for the pickup."
  CreateActor(101, model 23, 0x17800, 0x9400, 0, 0x5a0, colour 0)    // sq (47,18)
  AMT(104, 0x17 actor-drive-to, 101, 0x1c460, 0xc400)               // (56,24)
  AMT(152, 0x09 actor-destroyed, 101)

ev 104  AMT(105, 0x17, 101, 0x1c460, 0xc400)    // same point again           @0xc7bee
ev 105  AMT(106, 0x17, 101, 0x1d400, 0x10c00)   // drives to the drop (58,33)   @0xc7c14

ev 106  (kidnapper reached the case)                                        @0xc7c3a
  SetScenery(59,33,0, 0, 0x59, 0)               // case collected
  KillMissionTrigger(152)
  AMT(120, 0x09, 101)                           // killing him from now on = bonus path
  CreateActor(102, model 6, 0x19c00, 0x27400, 0, 0x10e0, colour 1)   // Fjord at (51,78)
  AMT(154, 0x09, 102)                           // Fjord destroyed
  AMT(107, 0x08, 102)                           // get into Fjord (arrow)
  Dialog_Begin(1); Say2(12) "She's in a blue Fjord down an alley in the centre of the city."
  AMT(130, 0x17, 101, 0x18400, 0x17400)         // kidnapper drives off to (48,46)

ev 130  Actor_Release(101)                      // he escaped; becomes traffic   @0xc7ef2

ev 120  (kidnapper killed after giving the location)                        @0xc7eae
  Say2(9) "The kidnapper managed to remotely trigger the bomb - you've got sixty seconds..."
  AMT(153, 0x05 countdown, 60)
  0x2fba4c = 1

ev 107  (got into Fjord)                                                    @0xc7d20
  if 0x2fba4c == 1: Dialog_Begin(1); Dialog_Say1(13) "She's chained up - I've got to get her to the boss' warehouse..."
  else:             Dialog_Begin(2); Dialog_Say2(14) "Ok, now take her to the boss' warehouse in the South-West..."
  Dialog_End()
  AMT(108, 0x04 garage, 28, 110, floor 1, side 1)   // door on +x side of (28,110); player on floor 0
  AMT(109, 0x0e left-vehicle)

ev 109  (left the Fjord)                                                    @0xc7e1c
  KillMissionTrigger(108); AMT(110, 0x08, 102); Say2(15) "Get back in the car!"
ev 110  (back in)                                                           @0xc7e5c
  AMT(108, 0x04, 28,110,1,1); AMT(109, 0x0e); Say2(16) "Now take her to the boss' warehouse!"

ev 108  (delivered into warehouse)  SUCCESS                                @0xc7d9c
  Actor_Release(100); Actor_Release(101); Actor_Release(102)   // id 100 unused (harmless)
  KillAllMissionTriggers()                                      // also stops the bomb clock
  if 0x2fba4c == 1: Say2(17) "Nicely done! ... special bonus for killing the kidnapper."; AddMoney(300000)
  else:             Say2(18) "Here's the dough. Don't spend it all at once.";         AddMoney(150000)
  ShowMessage(2022); Unlock21()

Failures (all: SetScenery(59,33,0,0,0x59,0); SetScenery(86,43,0,0,0x59,0);
          Actor_Release(100,101,102); KillAllMissionTriggers(); FAIL(p)):
  ev 150 player dead            p=5  "You idiot!"                              @0xc7f00
  ev 151 phone not answered 30s p=3  "You took too long - the kidnapper's given up!"  @0xc7f64
  ev 152 kidnapper destroyed before he got the case  p=8 "You idiot! He hadn't told us where..." @0xc7fc8
  ev 153 bomb clock ran out     Actor_Kill(102) first; p=10 "Too slow! He blew the car up!" @0xc802c
  ev 154 Fjord destroyed        p=11 "You destroyed the car! The boss is seriously narked now." @0xc809a
```

### M2: Rocket car (slot 2, start event 200, phone 1) 68k `0xc9d00` + `0xc80fe..0xc8384`

```
ev 200                                                                       @0xc9d00
  KillAllPhones()
  Dialog_Begin(0); Say2(19) "They're testing an experimental rocket car in the South of the city..."
  CreateActor(201, model 24 (vehicle), 0x1400, 0x3ec00, 0, 0x5a0, colour 4)   // sq (2,125)
  0x11a354 = 2
  AMT(201, 0x08, 201)            // get into rocket car
  AMT(250, 0x06)                 // player dead
  AMT(252, 0x09, 201)            // car destroyed

ev 201  (got in)                                                             @0xc817e
  SetWantedLevel(5)
  Say2(20) "Crap! It's got an alarm - it'll blow if you don't get it to us within 50 seconds."
  AMT(202, 0x04, 107, 3, 1, 1)   // our garage, door +x of (107,3), North
  AMT(210, 0x0e)                 // left vehicle
  AMT(205, 0x42)                 // player uses the vehicle's special/boost control (INFERRED, see §8)
  AMT(251, 0x05, 50)             // 50 s countdown

ev 205  Dialog_Begin(1); Dialog_Say1(21) "Holy Shhhhiiiii...!!"; Dialog_End()   (one-shot) @0xc825e
ev 210  Say2(22) "Get back in the car!"; KillMissionTrigger(202); AMT(211, 0x08, 201)  @0xc8270
ev 211  Say2(23) "Now take it to our garage in the North of the city - NOW!";
        AMT(202, 0x04, 107,3,1,1); AMT(210, 0x0e)                              @0xc82b6
ev 202  SUCCESS: KillAllMissionTriggers(); Actor_Release(201);
        WIN(27 "Ooooh, she's a beaut ... a cool quarter of a million.", 250000)      @0xc8220
ev 250  KillAll; Actor_Release(201); FAIL(24 "You twat!")                       @0xc8314
ev 251  Actor_Kill(201); KillAll; Actor_Release(201); FAIL(25 "You're too slow for this game, boy.") @0xc8336
ev 252  KillAll; Actor_Release(201); FAIL(26 "Noooo! My car!")                  @0xc8362
```

### M3: The race (slot 3, start event 300, phone 2) 68k `0xc9dac` + `0xc8384..0xc8642` + `Race21` `0xb6e48`

Cars: 10 = the player's race car, 11..15 = AI racers. All are vehicle model 27.

```
ev 300                                                                       @0xc9dac
  KillAllPhones()
  Dialog_Begin(0); Say2(28) "So you fancy yourself as a bit of hotshot... The track's in the West of the city."
  CreateActor(10, model 27, 0x1c00, 0x21728, 0, angle 0, colour 1)   // sq (3,66)
  Actor_SetFlag49c(10, 0)        // INFERRED: car held (cannot drive) until "Go!"
  AMT(1, 0x08, 10)               // get in -> starts the countdown
  AMT(10, 0x06)                  // player dead      -> ev 10
  AMT(10, 0x09, 10)              // car 10 destroyed -> ev 10
  0x11a354 = 3

ev 1   Say2(29) "Ok, here we go!"                                           @0xc83ac
       CreateActor(11, 27, 0x2400, 0x23f32, 0, 0, colour 2)    // grid (4,71)
       CreateActor(12, 27, 0x1c00, 0x23730, 0, 0, colour 3)    // (3,70)
       CreateActor(13, 27, 0x2400, 0x22f38, 0, 0, colour 4)    // (4,69)
       CreateActor(14, 27, 0x1c00, 0x2272c, 0, 0, colour 5)    // (3,68)
       CreateActor(15, 27, 0x2400, 0x21f34, 0, 0, colour 6)    // (4,67)
       Actor_SetAggro(11,100); (12,100); (13,100); (14,99); (15,99)
       SetTimerMode(0)                                         // start lights stage 0 (INFERRED)
       AMT(2, 0x11 hidden timer, 2)
ev 2   SetTimerMode(1); AMT(3, 0x11, 1)
ev 3   SetTimerMode(2); AMT(4, 0x11, 1)
ev 4   Say2(30) "Go!"; SetTimerMode(3); Actor_SetFlag49c(10, 1); Race21(-1); AMT(5, 0x11, 1)
ev 5   SetTimerMode(-1); SetTimerMode2(1, 3)                   // HUD "lap 1 / 3" (INFERRED)
ev 6..9, 11.. (incl. 10000..10004, 20000..20005): Race21(ev)   @0xc8636
ev 10  (player dead or car 10 destroyed) KillAllMissionTriggers(); SetTimerMode(-1); SetTimerMode2(-1,3);
       Actor_Release(10..15)
       FAIL(37 "What the hell are doing?")                     @0xc85a4
```

`Race21(ev)` (`0xb6e48`, CONFIRMED; tables identical in the CD script):

```
WAYPOINTS (13): i: (x, y) sq, speed d (type 0x32), v386
   0: (0x2000, 0x30c00) (4,97)   3000  550      7: (0x5400, 0x27c00) (10,79) 1500 300
   1: (0x2400, 0x34400) (4,104)  2500  300      8: (0x6400, 0x26c00) (12,77) 1500 250
   2: (0x6800, 0x33800) (13,103) 2500  300      9: (0x5400, 0x23c00) (10,71) 1500 300
   3: (0x6800, 0x2d000) (13,90)  2000  400     10: (0x6c00, 0x22400) (13,68) 1500 300
   4: (0x6400, 0x2a400) (12,84)  2500  250     11: (0x6200, 0x20c00) (12,65) 2000 300
   5: (0x1400, 0x29400) (2,82)   1500  250     12: (0x1c00, 0x21400) (3,66)  2000 450
   6: (0x1c00, 0x27c00) (3,79)   1500  200
   (x = sx*0x800+0x400+dx, y = sy*0x800+0x400+dy; tables at the start of 0xb6e48)
CHECKPOINTS (5), per car, armed one at a time:
   0: type 0x36 actor y >  0x33000    1: type 0x34 actor x >  0x5000
   2: type 0x35 actor y <= 0x21800    3: type 0x33 actor x <= 0x3000
   4: type 0x36 actor y >  0x24c00    (finish line)

Race21(-1):
  for r in 0..4:                    // AI racer id 11+r
     wp[r] = 0
     AMT(10000+r, 0x32 drive-to-with-speed, 11+r, WP[0].x, WP[0].y, WP[0].speed)
     Actor_Set386(11+r, WP[0].v386 - 30*r)       // +0x386, INFERRED: AI speed cap; later grid slots are slower
  for c in 0..5:                    // car id 10+c (c=0 is the player's car)
     prog[c] = 0; lap[c] = 1; finished[c] = 0
     AMT(20000+c, CP[0].type, 10+c, CP[0].value)

Race21(ev = 10000+r):               // AI reached its waypoint
  wp[r] = (wp[r]+1) % 13
  AMT(ev, 0x32, 11+r, WP[wp].x, WP[wp].y, WP[wp].speed)
  Actor_Set386(11+r, WP[wp].v386 - 30*r)

Race21(ev = 20000+c):               // car passed its next checkpoint
  prog[c] += 1
  if prog[c] > 4:
     prog[c] = 0; lap[c] += 1
     if lap[c] < 4:
        if c == 0: SetTimerMode2(lap[0], 3); Say2(36 + lap[0])   // 38 "2 laps to go.", 39 "1 lap to go..."
     else:
        finished[c] = 1
        if c == 0:                  // player finished 3 laps
           KillAllMissionTriggers()
           Actor_Release(10..15)
           n = count of c' in 1..5 with finished[c'] (AI already home)
           SetTimerMode2(-1, 3)
           Say2(31 + n)             // 31 "You came first..." 32 "2nd" 33 "3rd" 34 "4th?" 35 "fifth" 36 "last"
           if n < 3: AddMoney((3-n) * 85000); ShowMessage(2022)
           else:     ShowMessage(2028)
           Unlock21()               // sets 0x11a354 = -1
  if finished[c] == 0 and 0x11a354 != -1:
     AMT(20000+c, CP[prog[c]].type, 10+c, CP[prog[c]].value)
```

There is no time limit and no check that the player stays in car 10: the checkpoint
triggers watch actor 10 itself. AI racers that finish keep looping their waypoints.

### M4: Police surveillance van (slot 4, start event 400, phone 5) 68k `0xc9e60` + `0xc8642..0xc87f2`

```
ev 400                                                                        @0xc9e60
  KillAllPhones()
  Dialog_Begin(0); Say2(40) "Fry thinks the cops have got him under surveillance. Go check it out."
  CreateActor(400, model 7 (vehicle), 0x3bc00, 0x2134, 0, angle 0, colour 6)   // sq (119,4)
  Actor_SetFlag49c(400, 0)              // INFERRED: parked
  AMT(401, 0x09, 400)                   // van destroyed
  AMT(420, 0x2e, 400)                   // van attacked first (or destroyed), arrow
  AMT(421, 0x1b, 400)                   // van crew sees the player
  AMT(450, 0x06)                        // player dead
  0x11a354 = 4; 0x2fba4c = 0

ev 420  if 0x2fba4c != 0: return                                               @0xc8708
        0x2fba4c = 1; SetWantedLevel(6); KillMissionTrigger(421)
        Say2(42) "Oh, that was subtle. Catch 'em up and teach 'em a lesson."
        Actor_SetAggro(400, 100); AMT(451, 0x43 escape/flee, 400)
ev 421  same with KillMissionTrigger(420) and Say2(43) "They've seen you. Go get 'em if you want your money."  @0xc873e
ev 401  SUCCESS: KillAll; Actor_Release(400); SetWantedLevel(6);
        WIN(41 "Nice one. Here's your cash.", 200000)                        @0xc86c0
ev 450  KillAll; Actor_Release(400); FAIL(44 "You imbecile!")                 @0xc87ae
ev 451  KillAll; Actor_Release(400); FAIL(45 "You let them get away!")        @0xc87d0
```

### M5: Nasty Nick escorts the mayor (slot 5, start event 500, phone 6) 68k `0xc9f30` + `0xc87f2..0xc9080`

Actors: 500 = Nick (pedestrian model ~(−16)=15). 501 = id given to the player's car while Nick
sits in it. 502 = mayor's limo (vehicle 8). 510..513 = gang pedestrians (model ~(−12)=11).
Warehouse door: east face of (28,110), cube 1, the same building as M1's garage.

```
ev 500                                                                        @0xc9f30
  KillAllPhones()
  Dialog_Begin(0); Say2(46) "...Go pick up Nasty Nick from the South-East of the city..."
  CreateActor(500, ped -16, 0x29400, 0x2a400, 0, 0, colour 6)    // sq (82,84)
  AMT(501, 0x0d near-actor (±6000), 500)
  AMT(550, 0x09, 500)                 // Nick dead
  AMT(551, 0x06)                      // player dead
  0x11a354 = 5

ev 501  (player reached Nick)                                                 @0xc8920
  Dialog_Begin(2); Dialog_Say1(49) "Ok, here's the job. The boss has bribed the mayor ... Go protect him."; End
  Actor_Follow(500, -1)               // Nick follows the player
  AMT(510, 0x0b player-in-any-vehicle)
  0x2fba50 = 0
  CreateActor(502, model 8, 0x19c00, 0x3d400, 0, 0xb40, colour 1)   // limo at (51,122)
  AMT(552, 0x28 actor-destroyed+arrow, 502)
  AMT(540, 0x11 hidden timer, 10)
  CreateActor(510, ped -12, 0x12c00, 0x2f400, 0, 0xb40, 1)   // (37,94)
  CreateActor(511, ped -12, 0x13400, 0x2f400, 0, 0xb40, 1)   // (38,94)
  CreateActor(512, ped -12, 0x12c00, 0x2cc00, 0, 0,     1)   // (37,89)
  CreateActor(513, ped -12, 0x13400, 0x2cc00, 0, 0,     1)   // (38,89)
  for i in 0..3: alive[i] = 1; AMT(520+i, 0x09, 510+i)
  0x2fba48 = 4; 0x2fba4c = 0; 0x2fba44 = 0

ev 540  AMT(502, 0x17, 502, 0x19c00, 0x2e400)          // limo moves off -> (51,92)   @0xc8ac8
ev 502  AMT(503, 0x17, 502, 0x13c00, 0x2e400)          // -> (39,92), by the gang      @0xc8aee
ev 503  AMT(504, 0x17, 502, 0xfc00, 0x2e400)           // -> (31,92)                   @0xc8b14
        for i in 0..3: Actor_Attack(510+i, 90, 502)    // gang shoots at the limo
        0x2fba4c = 1
        if 0x2fba50 == 0: Actor_Attack(500, 90, 510)   // Nick (on foot) engages
ev 504  AMT(505, 0x17, 502, 0xfc00, 0x37400)           // -> (31,110)                  @0xc8b9c
        SetFaceE(28,110, cube 1, type 0, tex 0, flip 0)   // open warehouse door

ev 510  (player got into any vehicle)                                         @0xc8cee
  AMT(511, 0x0e left-vehicle)
  AMT(512, 0x0c, 500)                 // Nick walks to and boards the player's vehicle (INFERRED meaning of type 0x0c)
  if 0x2fba44 == 1: Dialog_Begin(2); Dialog_Say1(50) "Wait for Nick."; End
  else 0x2fba44 = 1                   // since 510 is never re-armed, phrase 50 is never shown (see §8)

ev 512  (Nick boarded)                                                        @0xc8e26
  SetPlayerId(501)                    // tags the player's VEHICLE with id 501 (see §8)
  KillMissionTrigger(550)             // Nick can't die while inside
  0x2fba50 = 1

ev 511  (player left the vehicle)                                             @0xc8d60
  if 0x2fba50 == 1:
     PedHelper_b448e(500, 501, -16, 1)   // spawn ped Nick (id 500, model 15) out of vehicle 501
     Actor_Release(501)                  // clear the car's tag
     AMT(550, 0x09, 500)
     0x2fba50 = 0
     if 0x2fba48 > 0 and 0x2fba4c == 1:
        Actor_Attack(500, 90, first id in 510..513 whose alive flag == 1)   // always 510, see §8
  else:
     KillMissionTrigger(512)

ev 520..529 (gang ped ev-10 destroyed)                                        @0xc87f2
  0x2fba48 -= 1
  Say2(53 + 0x2fba48)   // 3: 56 "That's one less scumbag..." 2: 55 "Two more to go." 1: 54 "Ok, just one left." 0: 53 "Nice one - they're all dead!"
  if 0x2fba48 > 0 and 0x2fba50 == 0 and 0x2fba4c == 1:
     Actor_Attack(500, 90, first alive id)   // always 510
  Actor_Release(ev - 10)

ev 505  (limo reached (31,110) next to the warehouse)  SUCCESS                @0xc8bda
  KillAllMissionTriggers()
  SetFaceE(28,110,1, type 1, tex 0x76, flip 1)    // close door
  if 0x2fba50 == 1: PedHelper_b448e(500, 501, -16, 1); Actor_Release(501)
  Actor_Follow(500, -2)                           // stop following
  AMT(560, 0x12 go-to-and-vanish, 500, 0xe420, 0x37400)   // Nick walks into the warehouse (28,110). ev 560 has no handler
  Actor_Release(500); Actor_Release(502); Actor_Release(510..513)
  WIN(52 "Nice job. Here's your payment.", 200000)

Failures (all: KillAll; SetFaceE(28,110,1,1,0x76,1); Actor_Follow(500,-2); Actor_Release(500);
          if 0x2fba50==1 Actor_Release(501); Actor_Release(502); Actor_Release(510..513); FAIL(p)):
  ev 550 Nick dead     p=48 "Nick's dead! I told you to look after him."   @0xc8e46
  ev 551 player dead   p=47 "Was it too hard for the little baby?"         @0xc8f04
  ev 552 limo destroyed p=51 "You were supposed to protect him!"          @0xc8fc2
```

The gang does not have to be wiped out. The mission succeeds as soon as the limo
reaches (31,110). Killing gang members only produces the 53..56 lines.

### M6: Bank-robbery van hijack (slot 6, start event 600, phone 3) 68k `0xc9fd0` + `0xc9080..0xc958a`

Actors: 600 = robbers' van (vehicle 7). 601 = its driver, created when the player hijacks the van.
Rival garage = building at (124,67). Its west door is opened as the van's escape route.

```
ev 600                                                                        @0xc9fd0
  KillAllPhones()
  Dialog_Begin(0); Say2(57) "We've got word that our rivals have just robbed a bank down South..."
  CreateActor(600, model 7, 0x2f400, 0x3e400, 0, 0x5a0, colour 7)   // sq (94,124)
  AMT(650, 0x06)                      // player dead
  AMT(651, 0x09, 600)                 // van destroyed
  AMT(601, 0x08, 600)                 // hijack: player gets into van
  AMT(616, 0x11, 10)                  // van sets off after 10 s
  Actor_SetAggro(600, 100)
  0x11a354 = 6

Van escape route (each step: AMT(next, 0x44 AI-drive-to (state 0x15), 600, x, y)):
  ev 616 → 617 (0x37400,0x3e400) (110,124)     ev 620 → 621 (0x38c00,0x2a400) (113,84)
  ev 617 → 618 (0x37400,0x3b400) (110,118)     ev 621 → 622 (0x38c00,0x25400) (113,74)
  ev 618 → 619 (0x34400,0x3b400) (104,118)     ev 622 → 623 (0x3cc00,0x25400) (121,74)
  ev 619 → 620 (0x34400,0x2a400) (104,84)      ev 623 → 624 (0x3cc00,0x21c00) (121,67); Actor_Set386(600, 150)
  ev 624: Say2(60) "Quick! They're going to get away!"; SetFaceW(124,67, cube 1, 0,0,0) (open door);
          AMT(625, 0x44, 600, 0x3efd0, 0x21c00)          // into the garage (125,67)

ev 601  (hijacked)                                                            @0xc90fe
  for e in 617..625: KillMissionTrigger(e)               // stop the route
  Say2(60) "Quick! They're going to get away!"           // sic: same line as ev 624; phrase 62 is never used
  AMT(602, 0x04, 107, 3, 1, 1)                           // Fry's garage, door +x of (107,3)
  AMT(610, 0x0e)
  PedHelper_b448e(601, 600, -16, 1)                      // driver (ped model 15, id 601) thrown out
  Actor_Attack(601, 60, -1)                              // driver attacks player
  SetWantedLevel(5)
  Actor_Set386(600, 1000000)

ev 610  KillMissionTrigger(602); Say2(63) "Get back in the van!"; AMT(611, 0x08, 600);
        Actor_Attack(601, 100, -1)                                            @0xc9246
ev 611  Say2(64) "Ok, now take it to Fry's garage."; AMT(602, 0x04,107,3,1,1); AMT(610, 0x0e);
        Actor_Attack(601, 100, -1)                                            @0xc928e
ev 602  SUCCESS: KillAll; SetFaceE(124,67,1, 1, 0x76, 1) (sic: E, not W, see §8);
        Actor_Release(600); Actor_Release(601);
        WIN(65 "Good job. Here's your cut.", 200000)                         @0xc91dc
Failures (all: KillAll; SetFaceW(124,67,1, 1,0x76,1) (close door); Actor_Release(600,601)):
  ev 625 van escaped      FAIL(61 "You twat! You let them escape!")            @0xc949a
  ev 650 player dead      Dialog_Begin(1); Say1(58) "AH! I've been shot!"; End; ShowMessage(2028); Unlock21()  @0xc94e8
  ev 651 van destroyed    FAIL(59 "Idiot!")                                    @0xc953c
```

### M7: The bomb truck (slot 7, start event 700, phone 7) 68k `0xca0a4` + `0xc958a..0xc9a6a`

Target zone: rival garage centre (0x3dc00,0x21c00) = square (123,67), floor 0.

```
ev 700                                                                        @0xca0a4
  KillAllPhones()
  Dialog_Begin(0); Say2(66) "The boss has decided to wipe out our main rivals..."
  CreateActor(700, model 19, 0x1f400, 0x36400, 0, 0xb40, colour 7)   // sq (62,108)
  AMT(750, 0x06); AMT(751, 0x09, 700); AMT(701, 0x08, 700)
  0x11a354 = 7

ev 701  (got in truck)                                                        @0xc9608
  Say2(69) "Great, now take it to their garage. The boss has fitted a timer..."
  AMT(740, 0x05 countdown, 50)
  AMT(702, 0x0f near-point ±0x1000, 0x3dc00, 0x21c00, floor 0)   // arrow
  AMT(710, 0x0e left-vehicle)
  0x2fba44 = 0 (in zone); 0x2fba50 = 1 (in truck)

ev 702  (player reached zone)                                                 @0xc9690
  0x2fba44 = 1
  if 0x2fba50 != 1: return
  Say2(74) "Ok, now get the hell out of there!"
  AMT(703, 0x1c player-outside ±0x1800, 0x3dc00, 0x21c00)
ev 703  (drove out of zone in the truck)                                      @0xc96b4
  0x2fba44 = 0; Say2(75) "What are you doing? Put it back where it was!"
  AMT(702, 0x0f, 0x3dc00, 0x21c00, 0)
ev 710  (left the truck)                                                      @0xc96cc
  0x2fba50 = 0
  if 0x2fba44 == 0: KillMissionTrigger(702); Say2(70) "Don't turn chicken on me now - get back in the truck!"; AMT(711, 0x08, 700)
  else:             KillMissionTrigger(703); Say2(79) "Run!"
ev 711  (back in)                                                             @0xc9748
  0x2fba50 = 1
  if 0x2fba44 == 0: Say2(71) "Now take it to our rivals garage."; AMT(702, 0x0f, 0x3dc00,0x21c00,0)
  else:             Say2(72) "It's close enough - now get out of there before she blows."; AMT(703, 0x1c, 0x3dc00,0x21c00,0)
  // 710 (left-vehicle) is NOT re-armed here, see §8

ev 740  (50 s up: truck explodes)                                             @0xc97d4
  KillAllMissionTriggers(); Actor_Kill(700); Actor_Release(700)
  if 0x2fba44 == 1:                                     // SUCCESS: garage destroyed
     for x in 124..126, y in 64..68: SetCube(x,y, cube 1, h1 0x200, h2 0x200, scenery 0, byte0 0, flags 0)
     for x in 124..127, y in 64..68:
        SetCube(x,y, cube 2, 0x200, 0x200, 0, 0, 0)
        if Random(1) == 0:                              // always true if the Random table is < 512 (INFERRED)
           Explosion(x*0x800 + Random(0x800), y*0x800 + Random(0x800), 0x4268 + Random(1500), 0, 3000)
     SetFaceTop(124,69, cube 0, type 1, tex 0xd0, flip 1); same for (125,69), (126,69)
     if 0x2fba50 == 0: Say2(76) "I love the smell of burning napalm in the morning!"
     else:             Say2(77) "That would have been a lot more impressive if you'd actually got out the truck..."
     AddMoney(200000); ShowMessage(2022); Unlock21()
  else:
     KillAllMissionTriggers(); Say2(78) "What a botch! The truck wasn't anywhere near the garage!"
     ShowMessage(2028); Unlock21()
ev 750  KillAll; Actor_Release(700); FAIL(67 "Nice one, slick.")             @0xc9a26
ev 751  KillAll; Actor_Release(700); FAIL(68 "Oh, you f*cking bastard!")     @0xc9a48
```

If the player is still in the truck when it explodes he is probably killed. That can
happen in the same frame as ev 740. ev 750 (player dead) was already killed by
`KillAllMissionTriggers` in 740, so it does not fire. INFERRED.

### M8: Grenade kill frenzy (slot 8, start event 800, phone 8) 68k `0xca14c` + `0xc9a6a..0xc9c02`

```
ev 800                                                                        @0xca14c
  KillAllPhones()
  Dialog_Begin(0); Say2(80) "Fry's stashed some grenades on the roof of a building downtown... This phone will self-destruct in 10 seconds."
  e = 801
  for x in 55..58: for y in 96..98:                     // 12 roof cells, cube 1
     SetScenery(x,y, cube 1, scenery 6 Crate, byte0 0x2c, byte4 3)
     AMT(e, 0x25 cell-scenery-cleared, x, y, 1)          // arrow; e = 801 + (x-55)*3 + (y-96)
     e += 1
  SetFlag34c28c(1)              // unknown render/HUD flag (read in 0xe12a8), cleared below
  AMT(850, 0x06)                // player dead
  AMT(830, 0x05 countdown, 10)  // phone self-destruct clock
  0x11a354 = 8

ev 830  (phone self-destructs; cosmetic)                                      @0xc9b6a
  Explosion(0x253ce + Random(100), 0x3d3ce + Random(100), 20000, 0, 1500)   // at phone 8 (74,122)
  SetScenery(74,122, cube 0, scenery 0, byte0 0x2c, 0)  // phone removed from the map

ev 801..812  (first crate smashed / grenades collected)                      @0xc9a6a
  for e in 801..812: KillMissionTrigger(e)
  SetFlag34c28c(0)
  ShowMessage(2029 "Kill frenzy!")
  Say2(84) "Ok, now lob them off the building and nuke the cars below. Try to get 15,000 points or more in the next 60 seconds."
  AMT(820, 0x0a money-gained, 15000)
  AMT(851, 0x05 countdown, 60)

ev 820  SUCCESS: KillAll; WIN(83 "Well done - here's your money.", 150000)    @0xc9b30
ev 850  SetFlag34c28c(0); KillAll; FAIL(81 "You twat!")                        @0xc9bc2
ev 851  KillAll; FAIL(82 "Too slow!")                                          @0xc9be6
```

There is no time limit before the first crate is opened. Only death fails the mission at that stage.

## 4. Event-number index (per slot)

| Slot | Event → meaning |
|---|---|
| — | −1 money goal (level complete), 100..800 mission starts |
| 1 | 101 phone · 102 case picked · 103 at drop · 104/105/106 kidnapper waypoints · 107 in Fjord · 108 warehouse · 109 left · 110 back in · 120 kidnapper killed late · 130 kidnapper gone · 150 dead · 151 phone timeout · 152 kidnapper killed early · 153 bomb · 154 Fjord destroyed |
| 2 | 201 in car · 202 garage · 205 boost · 210 left · 211 back in · 250 dead · 251 timeout · 252 destroyed |
| 3 | 1 in car · 2,3,4,5 start sequence · 10 dead/destroyed · 10000–10004 AI waypoint · 20000–20005 checkpoint |
| 4 | 401 van destroyed · 420 attacked · 421 spotted · 450 dead · 451 escaped |
| 5 | 501 met Nick · 502–505 limo waypoints · 510 in vehicle · 511 left vehicle · 512 Nick boarded · 520–523 gang killed · 540 limo start · 550 Nick dead · 551 dead · 552 limo destroyed · (560 Nick gone, unhandled) |
| 6 | 601 hijack · 602 garage · 610 left · 611 back in · 616–624 van route · 625 escaped · 650 dead · 651 destroyed |
| 7 | 701 in truck · 702 in zone · 703 left zone · 710 left truck · 711 back in · 740 boom · 750 dead · 751 destroyed |
| 8 | 801–812 crates · 820 money reached · 830 phone boom · 850 dead · 851 timeout |

## 5. Global events

* **Event 0:** ignored (68k `0xc79da`; OS4 dispatcher skips it). The CD script used it as "shut down".
* **Player death/arrest:** no level-wide handling. Every mission adds its own type-0x06
  trigger, and it always means mission failure (M1 150, M2 250, M3 10, M4 450, M5 551,
  M6 650, M7 750, M8 850). Outside a mission, death has no script effect.
* **Respawn (type 0x3E):** not used by this level.
* **Secret/hidden phones (type 7), easter eggs:** none. All slots are visible type-1 phones.
* **Money goal:** `AddTrigger(-1, 0, 2000000)` at init. The handler ignores ev −1 and the
  game completes the level.

## 6. Phrase usage

All level phrases 0–84 are used except **62** ("Nice work, now take it to Fry's garage."; ev 601
says 60 instead) and **73** ("Run, Forrest, run!"). Globals used: 2022, 2028, 2029, and 2150–2158
(via `RefreshPhones`). Phrase 0 is shown only as `ShowMessage(0)` at init.

## 7. Differences vs the legacy CD script (`analysis/ghidra_script21/decompiled.c`, main `0x11bc2`)

The logic, event numbers, triggers, phone slots, race tables and rewards are the same. The CD
decompile shows truncated constants (16-bit coordinates, byte-sized money args, e.g.
`0xe0` = 300000, `0xf0` = 150000). Update 7 differences found:

1. **M6:** Update 7 adds `Actor_Set386(600, 1000000)` on hijack (ev 601) and
   `Actor_Set386(600, 150)` at ev 623 (van slows down before the garage). The CD calls
   `0x10b56` (= Set386) only in the race.
2. **Event 0:** CD = shutdown (`0x10578`, wait for goodbye). Update 7 = ignored.
3. The CD is a blocking loop with local booleans (`bVar1`, `bVar2`, `local_24`). Update 7 moves
   them to the globals `0x2fba4c`, `0x2fba50` and `0x2fba44`, with the same semantics.

The quirks in §8 (phrase 60 on hijack, `SetFaceE` vs `SetFaceW` in M6, alive flags never
cleared, triggers 510/710 not re-armed) are **already in the CD script**.

## 8. Notes, quirks, corrections to `script_api.md`

* **`SetPlayerId` (`0xb41b0`) condition is inverted in script_api.md.** It writes the id when the
  player model flag `+0x4909 == 0`, i.e. when the player is **in a vehicle**. So it tags
  the player's car, not the pedestrian. Level 21 uses it to tag the car Nick boarded (id 501),
  which `PedHelper_b448e(500, 501, -16, 1)` then uses to make Nick get out. CONFIRMED.
* **`PedHelper_b448e(newId, vehicleId, pedModel<0, p4)`:** for the first actor whose id is
  `vehicleId`, creates a pedestrian of model `~pedModel` with id `newId` from that vehicle
  (`0xdb196`), then sets `+0x448 = 1`. Used to make a passenger or driver get out. CONFIRMED.
* **`Actor_Follow(id, -1)`** = follow the player (`+0x41a = 1`). **`(id, -2)`** = stop
  following. CONFIRMED.
* **Trigger 0x0c:** used here as "Nick boards the player's vehicle". It fires when he is inside,
  and Nick's ped disappears. Strengthens the INFERRED meaning. **Trigger 0x42:** called from
  vehicle control code (`0xe9644` area, model flag `+0x4904` plus control input
  `0x34c158[]`). It fires when the player uses the vehicle's special/boost control. Here it
  triggers the rocket car's "Holy Shhhh" line. INFERRED.
* **`Actor_Set386`:** used as a per-actor AI speed cap. Race values 200–550 (−30 per grid slot),
  van 150 near the garage and 1000000 after hijack. INFERRED.
* **`Actor_SetFlag49c(id, 0/1)`:** 0 on the race car until "Go!" and on the parked surveillance
  van. INFERRED: 0 = AI vehicle held / not driving.
* **`SetTimerMode(0..3, -1)`** = race start-light stages 0,1,2 then 3 "Go" (−1 off).
  **`SetTimerMode2(lap, 3)`** = lap display "lap n of 3" (−1 off). INFERRED from the race flow.
* **`SetScenery` 5th/6th args** are raw cell bytes `+0` and `+4` of the in-memory cube. They are
  not simply the file's special/rotation: the game writes `0x59` back when removing the
  briefcase, while the file byte0 of (86,43) is `0x20`. Reproduce them byte-for-byte.
* `Random(n)` = `n * table[++i] >> 9` (`0x3fc8c`). `Random(1)` is 0 when table entries are
  < 512, so M7's `Random(1) == 0` test always passes and every roof cell explodes. INFERRED.
* **M5 bug:** alive flags `0x2fba30..3c` are never cleared, so Nick is always told to attack
  gang ped 510, even after 510 is dead. 68k and OS4 agree. CONFIRMED.
* **M5:** trigger 510 (player-in-vehicle) is never re-armed. After the first exit (ev 511),
  Nick can no longer board cars, and "Wait for Nick." (ev 510 second entry) can never be shown.
  The mission is still winnable because success depends only on the limo.
* **M7:** trigger 710 (left truck) is not re-armed in 711. After one exit and re-entry,
  `0x2fba50` stays 1, so a later bailout still earns line 77 ("...if you'd actually got out")
  instead of 76.
* **M6:** the success path closes `SetFaceE(124,67,1,…)`, but the door opened at ev 624 was
  `SetFaceW`. If the van reached the garage door and was then hijacked, the west door stays open.
* **M1 text:** phrase 2 says the phone is in the "South-East corner", but phone 4 is at
  (126,14), the north-east corner under the North = −y convention. Text quirk, or the
  convention needs re-checking.
* The init clears `0x2fb220` before `RefreshPhones()`, so this level never shows the
  generic intro phrase. Phrase 1 is said directly instead.

### OS4 call names in `0x010df934` (matched against the 68k calls; INFERRED)

`FUN_010d100c` AddTrigger (with tracked-list bookkeeping inlined) · `FUN_010d26b8` AddMissionTrigger ·
`FUN_010d28ac` KillMissionTrigger · `FUN_010f46b8` KillTrigger · `FUN_010ce5e8` CreateActor ·
`FUN_010cde90` SetScenery · `FUN_010cddc8` SetCube · `FUN_010ce1f4` SetFaceE · `FUN_010ce364` SetFaceW ·
`thunk_FUN_01015d7c` Dialog_Begin · `FUN_010d1614` Say1 · `FUN_010d1638` Say2 · `thunk_FUN_01015f68` Dialog_End ·
`FUN_010d153c` ShowMessage · `thunk_FUN_01041408` AddMoney · `FUN_010cf830` Actor_Release ·
`switchD_010d2ee0::caseD_10cf184` Actor_Attack · `FUN_010cdb78` Actor_Follow · `FUN_010cf57c` Actor_SetAggro ·
`FUN_010cf730` Actor_Set386 · `FUN_010cee9c` PedHelper_b448e · `FUN_010d152c` SetFlag34c28c ·
`FUN_010d2480` RefreshPhones. Globals: slot `0x0114c370`, tracked count `0x0114c9a0`/list `0x013792c4`,
M5 counters `0x0114c958/5c/60/64`, alive flags `0x01379b5c..68`.

## 9. Open questions

1. Exact AI meaning of `+0x386` (speed cap?) and `+0x49c` (hold/drive flag?).
2. `SetFlag34c28c` (`0x34c28c`, read in render loop `0xe12a8` with `0x34c288/90/94`): what it
   displays during the M8 crate phase (crate marker / camera?).
3. Trigger type 0x42 exact condition (which control, which vehicles).
4. Meaning of the in-memory cell bytes `+0` and `+4` written by `SetScenery` (0x85, 0x9c, 0x59, 0x2c, rot 3).
5. Vehicle model names for 0, 6, 7, 8, 19, 23, 24, 27, and pedestrian models 11 and 15 (table `0x348120` / `0x3480e0`).
6. Whether `Random`'s table can reach ≥ 512. If it can, M7's explosions are not every cell.
7. Phone 4 "South-East" vs map position (north-east): check the axis convention.
