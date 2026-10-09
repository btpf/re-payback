# Random missions in Rampage (Mac only)

The Mac release adds a seventh "level script" that runs in Rampage on any map with phones.
Neither Amiga build has it (no code in 68k or OS4; the Amiga `Global.*.phrases` lack
1400–1470 and 1800–1838). Source: the Mac OS X build (`analysis/ghidra_macx`, re-exported with the jump-table case
bodies; `jumptables.tsv` row 1012a0e8 maps slot 0..7 to the event handlers, 1012a124 the
answered phone's `Random(7)+1` to the job setups).
CONFIRMED unless marked. Port: `crates/payback/src/mission/levels/random.rs`, switched by
the Rampage setting (`RandomMissions`, recorded in replays; decision 8 in mac_version.md),
and only run when the phrases exist (phrase 1400 not empty): with Amiga-only data every
brief would be empty, so the mode stays off whatever the setting says.

## Entry

* `amiga_startlevel` @ 100fff40: in Rampage, if `GetNumAltPhones() > 0`, call
  `InitScript_Random` @ 10129e50. City index 3 sends `ProcessTrigger` @ 1013e2c0 to
  `ProcessTrigger_Random` @ 1012a0a0. The trigger loop runs in Rampage on maps with phones
  (script_api.md §3.3).
* `P` = `_DAT_10188fe4`, the Rampage police aggressiveness 0–5 (also the high-score table).

## Init (`InitScript_Random`)

```
Script_reset(); tracked = 0; curSlot = 0; city = 3
SetTrigger(-1, 0, GetHighestScore())     // money goal = top Rampage score for P (@ 100a05a0)
SetTrigger(100, 1, -1)                   // ring a random phone (GetPhoneCoords(-1))
Announce(1400 "ANSWER THE PHONE")
Say(2, 1401 "Welcome to %s. Prepare to rampage!", GetJustCityName(city))
```

Reaching the goal ends the game as complete (the Mac shows phrase 1799, missing from every
phrase file; the port shows 2023 "Level complete!"). `GetJustCityName` @ 10084e40 is
phrase 1840–1842 for the three cities, else the user map's name.

`FinishMission_Random` @ 1012a020: `SetTrigger(100, 1, -1)` (a new random phone), curSlot = 0.

## Jobs

Event 100 (phone answered, slot 0) sets `curSlot = Random(7) + 1`. Success: Say(2, 1404–1406
"%d grand", random of three, drawn after the amount), `AddScore(k·1000)` (× multiplier),
`Announce(2022)`. Failure: Say(2, 1407–1409), `Announce(2028)`. Both `StopStoredTriggers` and
`FinishMission_Random`. `Loc(p)` = phrase `1830 + GetPointLocation(p)` (@ 10129f50: squares
< 43 / > 85 split the map into N 0, NE 1, E 2, SE 3, S 4, SW 5, W 6, NW 7, centre 8; y small
is north). Briefs are spoken by speaker 0 (phone), the rest by speaker 2.

| Slot | Brief | Setup | Win | Fail |
|---|---|---|---|---|
| 1 Steal a car | m = P<3 ? Random(3) : Random(2)+1. m0 1410 (veh, Loc); m1 1411 (+t, t = ET(pos)+Random(20), ×3/2 if P<4); m2 1412 (+t = ET2(pos,dest,veh)+ET(pos)+Random(20), ×3/2 if P<4) | veh = `GetRandomVehicleNum`; pos, dest = `GetSafePos`; id 101 at pos, Random(8) colour, Random(5760) angle; T(150,6), T(151,9,101), T(101,8,101), T(152,5,t) if t | ev 101 (in): marker on; m2 says 1414 (Loc dest); else kill 152 and at 50 % t = ET(dest,veh)+Random(20) (×3/2 if P<4), 1415 + T(152,5,t), or 1414. Wanted: P1 ⅓ → 4–5, P2 ½ → 5, P3 5, P4 5–6, P5 6. T(102,0x0f,dest), T(140,0x0e). ev 140 (left the car): kill 102, marker off, 1417, T(141,8,101); ev 141: marker on, 1418, re-arm 102/140. ev 102: marker off, (Random(20)+50)k, release 101 | 150–152: marker off, release 101 |
| 2 Gang | n = Random(P+1)+2; timed if P=5, or P 3–4 at 50 %: t = 8n + (ET(pos)+Random(20))·3/2 (P<4) or 4n + ET + Random(20). 1420 (Loc) or 1421 (Loc, t) | n peds model −12, ids 201.., at pos ±0x400 (y offset drawn first); per ped T(201,0x18,id), T(202,0x28,id); T(250,6); T(252,5,t) if t | ev 201 or the first 202: every ped attacks (`SetAggressiveness(id, 15P+20, -1)`); 201 is then killed. Each 202: n−1, "%d left" (1422–1425, random of four); last: (20n+Random(20))k, release all | 250/252, release all |
| 3 Race | 5 × `GetSafePos`; t = ET2 legs + Random(20) (×2/3 if P ≥ 4); 1430 (Loc p0) | T(350,6), T(301,0x0f,p0), marker on | 301: 1431 "Go!", T(302,0x0f,p1), T(351,5,t); each 302 arms the next point; after p4: marker off, (Random(30)+70)k | 350/351, marker off |
| 4 Cops | n = P+Random(2)+1, t = 10n+Random(10)+15; 1440 (n, t); wanted 6 | n × T(401,0x29); T(450,6), T(452,5,t) | each 401: "%d left"; last: (35n+Random(20))k | 450/452 |
| 5 Watchers | veh = `GetRandomVehicleNum_Pursue`, pos = `GetSafePos`; t = ET(pos)+Random(10)+40 (×3/2 if P<4); 1450 (Loc, vehicle name 2330+) | id 501 at pos, controls off; T(501,0x28,501), T(502,0x15,pos), T(550,6) | 502 (close): 1451, `SetGeneralAggressiveness(501,100)`, T(552,0x43,501), 50 % T(551,5,t). 501: (Random(20)+70)k, release 501 | 550/551 (insult), 552 "You let them get away!" (1452); release 501 |
| 6 Survival | t = Random(10)+50; 1460, Announce 1461 "SURVIVAL!", wanted 6 | T(650,6), T(601,5,t), T(610,0x11,1) | 610: wanted 6, re-arm (every second); 601: t thousand | 650 |
| 7 Points | a = Random(30)+20, t = 2a (P<4) or 3a/2; 1470 (a, t); Announce 2029 | T(701,0x0a,a·1000), T(750,6), T(751,5,t) | 701: (Random(20)+30)k | 750/751 |

ET = `EstimateTime(p, veh)` @ 10057c60: Manhattan distance from the player to the square
centre, `>> 12` on foot, else `/ (veh+0x8134 · veh+0x813c · 2)` (max speed × acceleration × 2).
ET2 = `EstimateTime2(a, b, veh)` @ 10057d60: Manhattan squares `>> 1`, else `·0x800 /` the same.
The steal job's vehicle name is phrase `name − 530` (1800–1825, "a Pug"); the watchers' is
the plain name (2330–2355).

## Helpers

* `GetSafePos` @ 100b42b0: up to 5000 tries of x = Random(126)+1, y = Random(126)+1. A square
  qualifies if its cube 0 exists (slope ≠ off) with a top face that is neither off nor water
  (reflect), and cubes 1–2 are off. It is taken if more than 4000 tries were made, if no
  route node exists, or if the nearest route node (`FindNearestNode` @ 100f11d0) has line of
  sight to it (`LineOfSight_alt`). After 5000 tries the last square is taken anyway. The
  result is always cube 0. The node end of the sight line is at the height of the node's own
  cube (`GetNodeCubeNum` @ 100f1450 = route record byte 12, also the "unused" 0xFF marker;
  `GetCubeHeight` @ 100ae7d0). Map 1 has 5 nodes on cube 1, maps 0 and 2 none. CONFIRMED;
  the port does the same.
* `GetRandomVehicleNum` @ 10057e50: Random(26), 25 → 27 (Vapour GT); never the RC car (22)
  or boat (23). `GetHighestLevelCompleted` @ 1009b580 (story levels done in order, 0–6) then
  bars or thins out the tank (11), helicopter (17), Pug Racer (18), RC truck (21), rocket car
  (24) and Vapour GT (27): 0–1 bar all; 2 bar 11/24/21/18/27, heli 4/5 rejected; 3 bar
  11/24/21/27, heli 4/5; 4 bar 11/24/27, 21/17 4/5; 5 bar 11/24, 21/17 4/5; 6 bar 11, 21/17/24
  4/5. `_Pursue` @ 100580c0 also never picks the police car (9), tank or helicopter.
* `GetRandomColour` @ 10051e20 = `random(8)`.

## Port notes

* Uses the game Rng, so a Rampage with random missions draws differently from one without;
  the setting is part of the replay header. The money goal (the high-score file) and the
  story progress (vehicle choice) are read at level start from the local files (INFERRED
  sufficient: replays are played back on the machine that recorded them).
* No HUD arrows exist in the port yet, so the random phone is found by its flashing and ring.
