# Vehicles

Reverse-engineering notes for vehicles in Payback (Update 7). Status marks:
**C** = CONFIRMED in code (address given), **I** = INFERRED (reasoned from code/data, not fully traced).

Builds: **68k** = Update 7 `Payback.exe` (flat base 0x10000, `analysis/ghidra_68k`);
**OS4** = `PaybackOS4` (`analysis/ghidra_os4`). The OS4 main vehicle update `FUN_0105f594` failed in the bulk
export; it was re-decompiled with a long timeout to `analysis/vehicles_os4/main.c` (script
`analysis/ghidra_scripts_vehicles/DecompOne.java`). Model-struct offsets are quoted in **OS4** form; in the 68k
build every model field after `+0x4909` sits **2 bytes lower** (68k packs structs to 2 bytes, OS4 to 4). Example:
engine sound is OS4 `+0x4924` = 68k `+0x4922`. Instance offsets up to about `+0x338` are identical; later 68k
instance fields are also 2 lower (68k speed `+0x3ae` = OS4 `+0x3b0` = `[0xec]`).

Conventions are the same as in `people.md` §1 (C):

- Tick: 1/25 s on 68k, 1/30 s on OS4, the Mac and the port (`people.md` §1.1). `ticks` per frame = `DAT_00348090` (68k) / `DAT_0114c6a0` (OS4), clamped to 1..5.
- Positions: 2048 units per map square, `x>>11` = square. x points east, y points south. z is down-positive.
- Angles: 5760 (0x1680) per turn. Forward = (sin h, cos h); h = 0 faces +y (south).
- `random(n)` = OS4 `FUN_0103ac94`, 68k `FUN_0003fc8c`.

"Per tick" rates below are multiplied by `ticks`. Movement is sub-stepped: it runs `ticks` times.

---

## 0. Function map

| Role | OS4 | 68k |
|---|---|---|
| Load all models; vehicle parameter setup | `FUN_0104e13c` | `FUN_000d7874` |
| `.Obj` reader (sets defaults, bbox) | `FUN_010bf8bc` | `FUN_0009610c` |
| Per-frame object/vehicle update (all classes) | `FUN_0105f594` (re-decompiled) | `FUN_000e3a58` |
| Movement sub-step loop + landing + wall bounce | `FUN_0105b9ec` | inside `FUN_000a197a` |
| Cube collision of a moving object | `FUN_01070868` | `FUN_00044f82` |
| Wall bounce, crash damage and sound | `FUN_010cd198` | (inline) |
| Grip, surface friction, skid marks, skid sound | `FUN_010cc288` | `FUN_000a1758` |
| Skid-mark / dust trail emitter | `FUN_010cc010` | |
| Airborne/helicopter steering and engine pitch | `FUN_010ccc5c` | `FUN_000a241c` |
| Body pitch/roll from ground slope | `FUN_010cbc7c` | |
| Spawn a traffic car | `FUN_01047af4` | |
| Object constructor (instance 0x4b8 bytes) | `FUN_010c0c1c` | |
| Create object of model at (x,y,h) | `FUN_01046ee4` | `FUN_000db03e` |
| `.vehicles` loader | `FUN_0106dbbc` | `FUN_00043148` |
| Police route graph (`.route`) nearest node | `FUN_010cd590` / `FUN_010cd424` | |

---

## 1. Vehicle types

### 1.1 Type table (C)

Vehicle type id = index into the model pointer table OS4 `DAT_012a6364[id]` / 68k `DAT_00348120[id]`.
Traffic spawning picks `random(0x1c)` over ids 0..27 (§6). The `.vehicles` file stores these ids (§5).
Name = phrase `+0x493c` (Global phrases 2330 + n).

| id | model file | name (phrase) | door model | notes |
|---|---|---|---|---|
| 0 | mundaneo.obj | Mundaneo (2330) | default | |
| 1 | pug.obj | Pug (2331) | default | |
| 2 | diblo.obj | Diblo (2332) | default | |
| 3 | bus.obj | Bus (2333) | bus | passengers scream |
| 4 | vapour.obj | Vapour (2334) | default | |
| 5 | pug GTI.obj | Pug GTI (2335) | default | |
| 6 | fjord.obj | Fjord (2336) | default | |
| 7 | van.obj | Van (2337) | default | |
| 8 | limo.obj | Limo (2338) | default | |
| 9 | police.obj | Police car (2339) | police | variants police_red/police_blue (siren light frames) |
| 10 | scooby.obj | Scooby (2340) | default | |
| 11 | tank.obj | Tank (2341) | petrol.obj (*) | |
| 12 | evo.obj | Evo (2342) | default | |
| 13 | taxi.obj | Taxi (2343) | taxi | |
| 14 | icecreamvan.obj | Ice cream van (2344) | bus | |
| 15 | pickup.obj | Pickup truck (2345) | default | |
| 16 | hotrod.obj | Hotrod (2346) | default | |
| 17 | helicopter.obj | Helicopter (2347) | default | |
| 18 | pug racer.obj | Pug Racer (2348) | default | |
| 19 | truck.obj | Truck (2349) | bus | |
| 20 | policevan.obj | Police van (2350) | police | variants policevan_red/_blue |
| 21 | bombtruck.obj | RC truck (2351) | bus | `+0x4900` → remote-controlled car model |
| 22 | remotecontrolledcar.obj | Remote controlled car (2352) | default | `+0x4904 = 1` |
| 23 | boat.obj | Boat (2353) | petrol.obj (*) | |
| 24 | rocketcar.obj | Rocket car (2354) | default | |
| 25 | rocketcar_thrust1.obj | (2354) | default | animation frame, not a real type |
| 26 | rocketcar_thrust2.obj | (2354) | default | animation frame |
| 27 | vapour racer.obj | Vapour GT (2355) | default | |

(*) The tank and the boat use `objects/weapons/petrol.obj` (OS4 `DAT_012a6754`) as their "door".

Variant models outside the table: `police_red`/`police_blue` (OS4 `DAT_0114c720/71c`, base `DAT_0114c718`) and
`policevan_red`/`_blue` (`DAT_0114c714/710`, base `DAT_0114c70c`). They have the same parameters. The light bar
flashes by swapping the model pointer. When the siren stops, the model resets to the base (OS4 main.c ~2536)
(C).

### 1.2 Model fields used by vehicles (C unless marked)

Defaults come from the `.Obj` reader (OS4 `FUN_010bf8bc` tail). The per-type overrides are in §1.3.

| Off (OS4) | Default | Meaning |
|---|---|---|
| +0x48c4 | from mesh | max\|X\|·16: half-width. Collision radius and skid-mark wheel offsets. Helicopter: divided by 3, door at −that; the dummy prop: halved (C, Mac `InitStructures` @ 1005ffb0). |
| +0x48c8 | from mesh | max Y·16 of near-ground vertices: front extent (wheel base front) |
| +0x48cc | from mesh | \|min Y\|·16: rear extent |
| +0x48d4 | −1 | **Home city.** −1 = any city. 0/1/2 = traffic spawns it only in that city (`DAT_0114c694` = level/2). On user maps (city > 2) the `.vehicles` booleans are used instead (§5). |
| +0x48d8 | 500 | **Max forward speed** (units/tick) |
| +0x48dc | −250 | **Max reverse speed** (negative) |
| +0x48e0 | 10 | **Max acceleration** (speed units/tick²) |
| +0x48e4 | 20 | **Max yaw rate** (angle units/tick). Also sets the AI look-ahead (§6). |
| +0x48e8 | 30 | **Grip**: rate at which velocity is pulled toward the heading (§2.4) |
| +0x48ec | 1100 | **Mass.** Used in collision momentum; also sets the splash volume. Not hitpoints. |
| +0x48f0 | 60 | **Max braking deceleration** (used when the throttle opposes the motion) |
| +0x48f4, +0x48f8 | −max\|X\|, maxY/2 | Door position (lateral, longitudinal). Some types override `+0x48f8`. |
| +0x48fc | — | Door model pointer |
| +0x4900 | 0 | Linked model (bomb truck → RC car) |
| +0x4904 | 0 (u16) | 1 = remote-controlled car. It dies on landing in water, and fire detonates it. |
| +0x4908 | 0xff (u8) | pickup/pose code (unused for vehicles) |
| +0x4909 | 3 (u8) | class: 0 = vehicle, 1 = person, 2 = projectile/misc, 3 = scenery |
| +0x490a | 0 (u8) | 1 = police vehicle (a u16 write of 1 at +0x4909 sets it) |
| +0x491c | 3 | **Horn sound id**: 3 CarHorn, 0x0b Siren (police), 0x2c IceCream (ice-cream van) |
| +0x4920 | −1 | **Reversing sound id** (0x17 Bleep: bus, truck, bomb truck). Looped while reversing. |
| +0x4924 | 1 | **Engine sound id** (§7) |
| +0x4928 | 0 (u16) | "tracked/no-wheel" flag (tank, helicopter, boat). Yaw rate is not limited by speed, no skid marks, and no J-turn. **OS4 offset**: 68k `+0x4926`. The 68k `+0x4928` that `Heli_TakeOff` / `Heli_Land` write (mission docs use 68k offsets) is the next field, OS4 `+0x492a` "flies" (C). |
| +0x492a | 0 (u16) | flies (helicopter) |
| +0x492c | 0 (u16) | boat (water vehicle) |
| +0x492e | 0 (u16) | inverts the visual roll (boat) |
| +0x4930 | 0 (u16) | bus: screams (sound 0x4c) on skid or crash when the player drives |
| +0x4934 | 256 | **Traffic spawn acceptance**: accepted if `random(256) < value`. 0 = never spawned as traffic. |
| +0x4938 | −1 | **Car-radio CD track** (CDDA, music mode 1). −1 or 0 = none. Played by OS4 `FUN_01008bb0` / 68k `FUN_00017f20` when entering. |
| +0x493c | — | Name phrase id |

### 1.3 Handling table (C, OS4 `FUN_0104e13c` lines 40948–41470; 68k `FUN_000d7874` identical values)

| id | name | maxF | maxR | accel | brake | yaw | grip | mass | spawn/256 | engine | horn | rev | radio | city | special |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0 | Mundaneo | 280 | −150 | 5 | 18 | 40 | 25 | 1400 | 256 | 1 | 3 | – | 7 | any | |
| 1 | Pug | 320 | −120 | 6 | 18 | 45 | 26 | 900 | 256 | 0x45 | 3 | – | 11 | any | |
| 2 | Diblo | 550 | −250 | 15 | 35 | 55 | 34 | 1400 | 45 | 0x43 | 3 | – | 5 | any | |
| 3 | Bus | 1000 | −150 | 2 | 5 | 25 | 15 | 10000 | 160 | 0x46 | 3 | 0x17 | – | any | bus; f8=c8·15/16 |
| 4 | Vapour | 500 | −240 | 16 | 35 | 50 | 28 | 1800 | 65 | 0x43 | 3 | – | 4 | any | f8=c8/3 |
| 5 | Pug GTI | 400 | −200 | 10 | 24 | 50 | 30 | 850 | 240 | 0x45 | 3 | – | 6 | any | |
| 6 | Fjord | 300 | −190 | 6 | 22 | 45 | 25 | 1300 | 256 | 1 | 3 | – | 7 | any | |
| 7 | Van | 250 | −160 | 4 | 20 | 35 | 23 | 2200 | 256 | 0x46 | 3 | – | 15 | any | f8=c8·3/4 |
| 8 | Limo | 380 | −180 | 7 | 25 | 45 | 26 | 3000 | 100 | 0x46 | 3 | – | 2 | any | f8=c8·2/3 |
| 9 | Police car | 600 | −250 | 17 | 40 | 60 | 45 | 1600 | 200 | 0x44 | 0x0b | – | – | any | police |
| 10 | Scooby | 480 | −240 | 14 | 35 | 53 | 42 | 1235 | 150 | 0x44 | 3 | – | 10 | any | |
| 11 | Tank | 220 | −100 (Mac −220) | 6 | 20 | 55 | 50 | 40000 | 0 | 0x1b | 3 | – | – | any | 4928 |
| 12 | Evo | 470 | −230 | 15 | 35 | 55 | 45 | 1400 | 130 | 0x44 | 3 | – | 9 | any | |
| 13 | Taxi | 310 | −200 | 6 | 21 | 43 | 24 | 1380 | 256 | 1 | 3 | – | 3 | any | |
| 14 | Ice cream van | 240 | −170 | 5 | 20 | 32 | 23 | 2000 | 100 | 0x46 | 0x2c | – | – | 0 | f8=c8·5/6 |
| 15 | Pickup | 230 | −165 | 5 | 23 | 37 | 25 | 1800 | 160 | 0x46 | 3 | – | 14 | any | f8=c8·5/6 |
| 16 | Hotrod | 450 | −200 | 13 | 30 | 40 | 40 | 1800 | 100 | 0x43 | 3 | – | 2 | any | |
| 17 | Helicopter | 650 | −650 | 6 | 6 | 40 | 5 | 1500 | 0 | 0x36 | 3 | – | – | any | 4928, 492a; c4/=3; f4=−c4 |
| 18 | Pug Racer | 340 | −130 | 8 | 22 | 50 | 30 | 700 | 0 | 0x45 | 3 | – | 5 | any | |
| 19 | Truck | 250 | −150 | 2 | 4 | 15 | 14 | 12000 | 100 | 0x46 | 3 | 0x17 | 12 | 2 | f8=c8·15/16 |
| 20 | Police van | 300 | −150 | 6 | 15 | 40 | 25 | 2500 | 50 | 1 | 0x0b | – | – | 1 | police; f8=c8·3/4 |
| 21 | RC truck | 200 | −100 | 4 | 15 | 20 | 25 | 2500 | 0 | 0x46 | 3 | 0x17 | 16 | any | 4900=RC car; f8=c8·7/8 |
| 22 | RC car | 400 | −400 | 20 | 45 | 65 | 70 | 5000 | 0 | 0x3c | 3 | – | 16 | any | 4904 |
| 23 | Boat | 300 | −200 | 20 | 45 | 40 | 5 | 10000 | 0 | 0x3f | 3 | – | 8 | any | 4928, 492c, 492e |
| 24–26 | Rocket car | 2000 | −100 | 40 | 30 | 43 | 40 | 2500 | 0 | 0x29 | 3 | – | 12 | any | f8=c8/6 |
| 27 | Vapour GT | 510 | −150 | 18 | 38 | 60 | 28 | 1800 | 0 | 0x43 | 3 | – | 5 | any | f8=c8/3 |

Notes:

- The Mac build gives the tank a reverse limit of −220 (`InitStructures` @ 1005ffb0), so it reverses as fast as it
  drives forward. Every other value matches. The port uses the Mac value (CONFIRMED).
- `policevan_red` uses brake 10 and grip 20 (the other two use 15/25), and mass 2000. Its spawn value is 50 like
  the others. This looks like a typo in the original.
- In the 68k build, police vehicles set `+0x490a = 1` directly (68k `0x48c8+0x42`).
- Speeds: 280 units/tick = 7000 units/s = 3.4 squares/s at 25 Hz. In port map units (÷8) that is 875/s.

### 1.4 Colours (C, OS4 `FUN_01047af4` tail; textures loaded in OS4 init ~41818)

Each traffic car gets `c = random(8)`. Instance `+0x3c4` = `Shine[c]` and `+0x3c8` = `Cardef[c]`, with
c = 0 black, 1 blue, 2 green, 3 purple/orange, 4 red, 5 silver, 6 yellow, 7 white. Shine list: Black, Blue,
Green, Purple, Red, Silver, Yellow, White. Cardef list: black, blue, green, orange, red, silver, yellow, white.
So index 3 pairs Shine.Purple with Cardef.orange.

The object faces textured `Cardef` (and the matching `Shine` environment texture) are drawn with the instance's
colour. Vans have a special case: a white van (`Shine[c] == Shine.White` and model == van) gets a different
driver-aggression roll (below). See rendering.md for the texture substitution.

Spawn-time instance fields (C, `FUN_01047af4`):

- `+0x428 = 3`: driver kind is civilian traffic.
- `+0x425` = lane direction 1..4.
- `+0x3d0` = cube layer.
- `+0x348` = `clamp(DAT_0114c69c·8 − 30 + random(90), 0, 99)`; for a white van, `min(DAT_0114c69c·8 + random(90), 99)`.
  `DAT_0114c69c` is the level number. I: driver aggression/skill.
- `z = ground − 0x200` (spawns slightly above the ground and drops).
- Speed: the Amiga leaves it 0. The **Mac** (`AddThing` @ 10058350) sets `+0x890` speed = `min(maxF, 100)`
  (velocity untouched; grip brings it up), so new traffic is already rolling. The port follows the Mac
  (`traffic.rs` `spawn_traffic`). CONFIRMED.
- `[0xce] = 1`.
- The engine sound `+0x4924` starts looped at volume 10000000, pitch 20000 (handle `[0x10b]`). The skid sound 2
  starts silent (handle `[0x10d]`).

---

## 2. Driving physics

### 2.1 Instance fields (OS4 int index; byte offset = 4·index) (C)

| Field | Meaning |
|---|---|
| [0],[1],[2] | x, y, z |
| [3],[4] | body roll / pitch angles (from slope or airborne) |
| [5] | heading h |
| [6] | helicopter rotor animation phase (0..11) |
| [9] | model pointer |
| [0xbf],[0xc0],[0xc1] | velocity vx, vy, vz (units/tick) |
| [0xc2] | visual lean (−256..256) |
| [0xca] | yaw rate |
| [0xcb] | current acceleration |
| short +0x338 ([0xce]) | on-ground flag |
| short +0x33a | in water (cube face type 4 under the vehicle) |
| [0xd3] | **health** 0..100. ≤0 = wrecked; −71 (0xffffffb9) = "explode now" |
| [0xe2] | speed cap (scripted limits) |
| [0xe3..0xea] | previous skid-mark points of the 4 wheels (−1 = none) |
| [0xec] | **speed** (signed scalar along the heading; + = forward) |
| [0xed] | wheel speed (engine revs; differs from speed in the air) |
| [0xf4] | cube layer 0..2 |
| [0xf6..0xfe] | ground heights of a 3×3 neighbourhood; [0xfa] = ground z under the centre |
| [0x103..0x105] | previous x, y, z (collision rollback) |
| byte +0x3c3 | J-turn / handbrake-spin latch |
| byte +0x428 | driver kind: 2 player, 3 civilian, 8–11 police pursuit states, 0x0f–0x14 scripted |
| [0x10b] | engine sound handle; [0x10d] skid sound; [0x10f] reversing beeper |
| short +0x44e | "hit hard" latch (landing in water) |
| short +0x450 ([0x114]) | wheels-driven flag |
| short +0x452 | set to 1 on a hard wall crash (damage ≥ 11) |
| [0x125] | timer; [0x127] short = engine on (helicopter); [0x128] short = controls enabled |

### 2.2 Input (C, OS4 main.c 640–700, 2560–2600)

`throttle` (`DAT_0114c5c8`) and `steer` (`DAT_0114c5cc`) are each −1, 0 or +1.

- Accelerate key (`DAT_0114c65c[p]`) → throttle −1. **Throttle −1 means "drive forward".**
- Reverse/brake key (`DAT_0114c654[p]`) → throttle +1.
- Brake key (`DAT_0114c62c[p]`): throttle = +1 if speed > 0, −1 if speed < 0, else unchanged. It brakes to a
  stop and never reverses. It does **not** change the grip; grip 4 comes from the J-turn latch only (§2.5; C, Mac
  `do_Other` @ 10146280, `SkidUnit` @ 10081830).
- Left (`DAT_0114c66c`) → steer −1; right (`DAT_0114c664`) → steer +1.
- Fire (`DAT_0114c64c`) → fire flag `DAT_0114c544` (tank gun etc.). On the RC car, fire sets health = −71
  (detonate).
- While `+0x4a0` (`[0x128]`) is 0 (entering, engine not started), throttle = steer = 0.
- After entering, `[0x106]` counts down by 50/tick. At 0 it plays sound 0x28 (Doorslam), volume 5000000, pitch
  0x5014.

### 2.3 Throttle, braking, reverse, J-turn (C, OS4 main.c 1380–1640; 68k 124840–125170)

```
m = model; cap = [0xe2]
if speed > cap: speed = cap

if throttle == -1 (forward):
    if airborne (z < ground) and not heli:            # wheels spin freely
        a = (wheelspeed >= 1) ? m.accel : m.brake      # (sic: uses 48e0/48f0)
        wheelspeed += min(4*a*ticks, 2*m.maxF - wheelspeed)
    elif speed < 0:                                    # was reversing: brake
        if not m.flag4928 and not jturn_latch and steer != 0:
            jturn_latch = 1; accel = 0                 # J-TURN
            yawrate = (steer<0 ? -2 : +2) * m.yaw      # sign as in code: steer<0 → +2·yaw in reverse
        accel = min(max(accel,0) + 5*ticks, m.brake)
        speed += min(ticks*accel, m.maxF - speed)
        if speed > 0 and wheelsDriven: speed = 0; wheelsDriven = 0
        tyre smoke at each wheel: random(2)==0 → particle(Flare/dust), size min(len·|speed|>>12, 1000)
    else:                                              # accelerate
        accel = min(max(accel,0) + 5*ticks, m.accel)
        speed += min(ticks*accel, m.maxF - speed); wheelsDriven = 1
elif throttle == +1: the mirror image with m.maxR, m.accel and m.brake swapped.
    Reversing plays the looped beeper m.+0x4920 (if ≥0) at volume 10000000, pitch 0x5622 (22050 Hz).
else (coast):
    speed moves toward 0 by 1 per tick; accel = 0; stop the beeper

# damage limits top speed (health H = [0xd3]):
fwdCap = clamp(H*m.maxF/50, m.maxF/4, m.maxF)
revCap = clamp(H*|m.maxR|/50, |m.maxR|/4, |m.maxR|)
speed = clamp(speed, -revCap, fwdCap)
```

Corrections from the Mac (C, `do_Other` @ 10146280; OS4 main.c 1422/1784):

- Airborne means `z < +0x8c8` (the ground of the last clear collision step) and not a flier. In the air the
  throttle only revs the wheels (`+0x894`): forward `a = ws < 1 ? brake : accel; ws += min(4·a·t, 2·maxF − ws)`,
  reverse `a = ws < 1 ? accel : brake; ws += max(−4·a·t, 2·maxR − ws)`. The speed itself only decays (1 per tick
  without throttle). On the ground `ws = speed` each tick. The engine pitch follows `ws`.
- Both brake branches puff tyre smoke: per wheel at 0.7 of the box extents (`cos·70/100`, `sin·70/100`),
  `random(2) == 0`: type 1 `Dust` at `+ random(300)` per axis, velocity `v/2 + random(50)`, size
  `min(|speed|·front >> 12, 1000)`, level `+0x8b0`, table 0.
- `UseHandbrake` (trigger 0x27) fires only for player 1's object.
- Rocket car (types 24-26): with the throttle forward the model swaps rocketcar/thrust2 → thrust1 → thrust2,
  back to rocketcar otherwise; the box follows the model.
- A police van with its siren on alternates its model with `policevan_red` (brake 10, grip 20, mass 2000), so its
  handling alternates; not reproduced by the port.
- Engine pitch (C): on the ground `|ws|·20000/eff + 6000` (tank `·10000 + 4000`) with the damage-capped top speed
  `eff`, and the clonk swap below 50 health; in the air (`PositionAirborneUnit`) the same over the plain maxF; the
  helicopter on the ground with its engine off 4000.

Acceleration ramps up: `accel` (`[0xcb]`) grows by 5 per tick to the model maximum. Sanity check: a Mundaneo
(accel 5, max 280) needs 56 ticks = 2.2 s to reach top speed.

The J-turn latch `+0x3c3` is cleared at the top of each update. While it is set, `FUN_010cc288` uses grip 4 (a
slide). In 68k the latch is `+0x3c1`, and `FUN_000a6c12` is called for the player (I: the handbrake sound).

### 2.4 Steering (C, OS4 main.c 2280–2420; 68k 125460–125690)

```
if steer == 0: yawrate = 0
else:
    dir = steer * (speed >= 0 ? -1 : +1)                # steering reverses when going backwards
    if sign(yawrate) != dir: yawrate = 0
    yawrate += 5*dir*ticks
    lim = m.flag4928 ? m.yaw : min(|speed|, m.yaw)     # tank/heli/boat turn on the spot
    yawrate = clamp(yawrate, -lim, lim)
heading = (heading + ticks*yawrate) mod 5760
```

So a car cannot turn faster than its speed (in angle units/tick), and the yaw rate builds up at 5 units/tick².
Steering, the heading change and the 45° snap run **only on the ground** (C, Mac `do_Other` @ 10146280); in
the air see §2.9. The steering of each ground tick is stored (`+0x804`, OS4 `[0xc9]`).
Right (steer +1) while going forward lowers h, which matches people.md.

**Heading snap to 45°** (C, OS4 main.c 2436–2490; 68k 125690–125750). If speed ≠ 0, every frame:

```
target = ((h + 360) / 720) * 720        # nearest multiple of 720 (45°)
move h toward target by at most 4*ticks
```

It wraps correctly near 0/5760. This gives the game's "lane-aligned" feel.

**Lean** `[0xc2]` (Mac `+0x7e8`; C): every ground tick it relaxes 32·t toward 0, then (no J-turn) slews 64·t
toward `−steer·|speed|·256/maxF` (the other sign with `+0x492e`, boats). It is **added to the roll target** in
`PositionUnit` (§2.10), so a car at top speed in a turn rolls 256 units (16°) to the outside.

**Dip** (Mac `+0x7ec`; C): relaxes 16·t toward 0; throttle forward below maxF: −16·t (floor −0x80); throttle
back above maxR: +16·t (cap 0x80). At one tick per frame it sits at ∓16 while accelerating. Added to the pitch
target.

### 2.5 Grip, surfaces, skidding (C, OS4 `FUN_010cc288(obj, map, ticks)`)

Velocity is not tied to the heading. Every **ground** tick (`SkidUnit` is not called in the air; C, Mac
`SkidUnit` @ 10081830) it is pulled toward `speed·(sin h, cos h)`, taken before the surface bleeds the speed:

```
tvx = speed*sin[h] >> 8;  tvy = speed*cos[h] >> 8
g   = m.grip  (4 if J-turn latch set)
surface = CubeTextureList flag of the top-face texture of the centre square on level +0x8b0
          (FUN_0106dfcc; no visibility test; 4 off the map or level > 2)
switch surface:
  0x22 (34, grass/mud):  r = g/2;          if speed > 450: speed -= min(3*ticks, speed-450)
  0x24 (36, sand):       r = g*2;          if speed > 350: speed -= min(20*ticks, speed-350)
  0x32 (50, snow):       r = 4;            if speed > 200: speed -= min(10*ticks, speed-200)
  0x33 (51, slush):      r = g*10/25
  0x34 (52, ice):        r = 1
  default (4 tarmac, 35, 65, off-map, layer>=3): r = g*4/5
vx += clamp(tvx-vx, -r*ticks, r*ticks);  vy likewise
if heli: return
skid = clamp(2*(|vx-tvx| + |vy-tvy|)/ticks, 0, 255)   # slip measured after the correction (Mac, C)
if in water (+0x33a): skid *= 2
```

Note: sand gives **more** grip (×2), but caps speed at 350.

If skid > 0:

- Bus (`+0x4930`) driven by the player (kind 2) when a new skid starts: sound 0x4c Screams, volume 0xff, pitch
  `15000+random(2000)`, non-positional.
- Not a boat: for each of the 4 wheels at local (±halfwidth, front `+0x48c8` / rear `−0x48cc`):
  - On land and not 4928: lay a skid-mark segment from the last wheel point (`FUN_010cc010`).
    - Grass/sand (0x22 / 0x24) gives DirtSkidmark dust particles (`DAT_0114c8ec`).
    - Snow/slush/ice (0x32–0x34) gives `DAT_0114c8f8` particles.
    - Else black skid marks (`DAT_0114c8f4`).
    - Spacing is `width·7/8`; the size is `0xe6`, or 100 for the RC car.
  - With probability ~(ticks−random(2))>0: a smoke/dust puff (`FUN_01035da0`) at wheel ± random(300). Velocity is
    `v/2 + random(50)` and size is `skid·halfwidth>>7`.
  - In water: spray particles instead, size 500+random(500).
- Marks start from the wheel's last point or, when its trail is −1, from where the wheel was at the tick start;
  `n = max(|dx|,|dy|)/(size·7/8) + 1` points at `k·d/n`, z = the car's, level `+0x8b0`. Tables: dirt 0x30000
  (DirtSkidmark), snow/slush/ice 0 (Dust), else 0x10000 (Dust with the shadow flag) (C, Mac `SkidUnit`,
  `analysis/mac/scratch_followup/addeffect_calls.tsv`). Puffs: grass/sand `Dirt` 0x30000 size `skid·hw>>7`, snow
  (0x32 only) `Dust` `>>7`, else `Dust` `>>8`; the tank puffs too (only the marks and the sound need `!4928`);
  boats get nothing. Spray: `random(2) == 0`, `Dust`, `500 + random(500)`, velocity `v/4 + random(50)`, level + 1.
- Skid sound handle `[0x10d]`:
  - On land and not 4928: volume `skid·40000`, pitch `skid·10+10000`, at z+250.
  - Otherwise volume 0.
- If skid == 0, the wheel trail points reset to −1. In the air they reset too, and the skid sound keeps its
  last volume.

The CubeTextureList surface flags (C for the codes above; texture lists in `extracted/game/CubeTextures`):

| flag | textures | effect |
|---|---|---|
| 4 | roads, pavements, buildings | normal grip |
| 34 (0x22) | Mud, Grass* | grip/2, speed ≤ 450 |
| 35 | Fence, Mesh, CorrugatedIron, $Bars | normal (I: see-through walls) |
| 36 (0x24) | Sand* | grip·2, speed ≤ 350 |
| 50 (0x32) | Snow* | grip 4, speed ≤ 200 |
| 51 (0x33) | Slush roads | grip·0.4 |
| 52 (0x34) | Ice, Ice2 | grip 1 (pure slide) |
| 65 | #Water, Pond | water (boats; cars sink, §2.8) |

### 2.6 Movement sub-steps, gravity, landing (C, OS4 `FUN_0105b9ec`, Mac `MoveObject_Vehicle` @ 1006e890)

Order per object (C, Mac `object_MainLoop` @ 1014a670): `MoveObject_Vehicle` (lift, gravity, move, cube collision,
bounce), then `do_Other` (controls, the first sub-step's landing / ground follow, steering, `PositionUnit`,
`SkidUnit`, the hit speed reset), then the scenery push (`AddCollideObj`). In `MoveObject_Vehicle` the vertical
block below runs only from the second sub-step. Gravity `vz += 0x20; z += vz` runs on every sub-step unless the car
is both in water and on the ground — on dry ground too, so the walls are tested at `z` after gravity (on flat
ground alternately the ground and 0xa0 below it, as the follow sets `vz = ground + 0x80 − z`); the drawn z is the
snapped one.

```
repeat ticks times:
    x += vx; y += vy
    hit = cube_collide(obj, map, last_substep ? 0 : 1)     # FUN_01070868 → (hitX<<8)|hitY, signed bytes
    wall_bounce(obj, hitX, hitY)                           # FUN_010cd198
    vertical:
      if z >= ground [0xfa]:                               # touching
         if not onGround:                                   # landing
            if vz > 160: sound 0x1e HitScenery vol vz*20000 pitch 15000; vz = 0x80 - |vz|/4
            else vz = 0x80
            onGround = 1; z = ground
         elif in water and not latched:                     # splashed down
            if RC car: health = -71
            sound 0x2b Splash vol mass*8000 pitch 0x209e; latch
         else: vz = clamp(ground + 0x80 - z, -1000, 1000); z = ground  # follow terrain
      else onGround = 0
      if [0x127] (helicopter engine on) and z > 0 and vz > -256: vz -= 0xaa   # lift
      vz += 0x20 (gravity 32/tick²); z += vz   (when airborne)
```

The ground is `+0x8c8` = the centre height of the last **clear** collision step (§2.7), not a fresh lookup. The
water flag `+0x81a` is the centre cube's top type 4 on that step, also while the car is still above it. The
splash latch `+0x936` clears on landing, on dry ground and in the air. The landing sound plays at z + 100.

Gravity is 32 z-units/tick², the same as people.md §4.3. A car leaving a ramp is handled by `FUN_010ccc5c` while
z < ground (§2.9).

### 2.7 Collision with cube walls (C, OS4 `FUN_01070868` + `FUN_010cd198`)

For vehicles (class 0), `FUN_01070868` builds the oriented box of the car: half-width `+0x48c4`, front `+0x48c8`,
rear `+0x48cc`, rotated by heading h and pitch/roll [3]/[4]. It tests the 8 box points (4 corners + edge points)
against the cube heights at the car's z (`z>>4 − 0x40`). A point inside a cube that rises above the car blocks
it. Blocking an axis restores that coordinate from the previous position [0x103]/[0x104]. The return value gives
the push sign per axis: +1 = blocked moving −, −1 = blocked moving +. Positions are clamped to `[0, 0x3ffff]` at
the map edge, which counts as a wall.

Corrections from the Mac comparison (CONFIRMED, Mac `CheckForCubeCollision` @ 100af980 = 68k `FUN_00044f82`):

- The vehicle branch does **not** restore the position; `BounceUnit` pushes the car off the wall. The box edges are
  tested against the walls beyond the box-centre square, the centre path against those beyond the previous
  square (`GetCollideInfo`, people.md §5.1). Glass faces (type 3) break instead of blocking.
- **Map edge.** Only the Mac clamps the 3×3 points of the last sub-step and flags them as a hit. The Amiga does it
  only on the earlier sub-steps (the 2×2 path), so at one tick per frame the edge never bounced a car there
  (effect INFERRED). The port follows the Mac: a probe beyond the edge is a wall hit (damage, sound, deflection).
- **Heading deflection (N7, all builds).** If a hit opposes the velocity (`hitX > 0 && vx < 1` or
  `hitX < 0 && vx >= 0`), the heading turns by 0x5a: for x, −0x5a with the heading in [1, 0x5a0] or
  [0xb41, 0x10e0], +0x5a otherwise; for y the signs swap (applied after x). This makes cars glance off walls.
- **Box points** (CONFIRMED, Mac @ 100af980 last sub-step; OS4 `FUN_01070868` reads them from `DAT_0130e1a0`
  x[9], `…e1c4` y[9], `…e1e8` z[9]). Index `lat·3 + long`, across w ∈ (−hw, 0, +hw), along l ∈ (−rear, 0, front):
  `lat = cos(roll)·w >> 8`, `up = −(sin(roll)·w) >> 8`, `fwd = cos(pitch)·l − up·sin(pitch)`,
  `dz = up·cos(pitch) + sin(pitch)·l`; `x = X + (lat·cos h + (fwd>>8)·sin h) >> 8`,
  `y = Y + ((fwd>>8)·cos h − lat·sin h) >> 8`; probe depth `(Z>>4) − 0x40 + (dz>>10)`, the offset only when it raises
  the probe. Points are clamped to `[0, 0x3ffff]` (Mac: a clamp is a hit). The reference square of the edge tests
  is the mean of the 4 unclamped corners `>> 11` (`sum >> 13`).
- **Tests**, merged per axis with a later hit replacing an earlier one (`MergeCollide` @ 100af7b0): the clamp flags,
  then `GetCollideInfo` along the 4 box edges in both directions — 0↔6 (rear), 2↔8 (front), 0↔2 (−hw side),
  6↔8 (+hw side), each at its two points' probe depths — then the centre path from (X, Y, (Z−0x400)>>4) to the
  last position without a hit (`+0x8f0..+0x8f8`, written only on a clear step), reference = that position's
  square. A car can therefore never get a box edge across a wall face (fences included: map 0's wooden fences are
  type-2 side faces of an otherwise empty level-1 cube, on both squares of the edge).
- When nothing is hit (CONFIRMED): `+0x8f0` = position; the ground heights under the 9 box points are stored
  (+0x8b8..+0x8d8: rows −half-width, centre, +half-width; each rear, centre, front). The centre uses `GetCubeInfo`
  @ 100ae3c0 (the floor rule; its level → `+0x8b4`), the others `GetCubeInfo2` @ 100ae580 (the surface nearest the
  car's z, a higher one winning within 32 h). `PositionUnit` and the scenery test read them. Then, if
  `h[centre] − 0x200 < Z` (near the ground), per point in index order: a **player-driven** car (state 2) runs
  `CheckForWeapon(obj, sx, sy, level, h[i])` @ 100597e0 (collects a pickup on the point's square), and **any** car
  `CheckForCrate(sx, sy, level, h[i])` @ 100593e0 (a crate there is `Bang`ed into its contents), level = the
  centre's cube level. These run before `AddCollideObj`, which registers every non-zero scenery cell (crates and
  pickups too, radius `size/2` from `+0x8174`): the box's front points reach a crate's square long before the
  front circle reaches the crate, so cars smash crates without stopping, and the player's car takes the pickup the
  next step. AI cars are still bumped by a lying pickup (the original does the same).
- On a hit the heights, `+0x8f0` and the level stay from the last clear step, so a car pressed against a wall keeps
  the roll and pitch it had before touching it.

**A hit stops the car** (C, all builds): `BounceUnit` leaves the speed, but the tail of the vehicle update (Mac
`do_Other` @ 10146280 after `SkidUnit`; OS4 main.c ~2503) sets `speed = 0` whenever a hit flag is set and restores
the pre-move heights and level. The grip step of that tick still ran with the old speed, so the bounce velocity
then decays at the grip rate.

Levels: `+0x8b0` = the highest cube level of the 9 points (the surface, the scenery and effects), `+0x8b4` = the
centre's (water, crates, pickups). Nothing outside the collision code writes `+0x8f0`; contact pushes keep it.

Port (`collide.rs` `box_points` / `vehicle_walls`, `driving.rs`): as above (one sub-step per tick), including the
speed reset (until 2026-10-08 it halved the speed). Left: glass is removed at the end of the tick, not at the
moment it breaks. Until 2026-10 the port tested a cross of half-size 256 and
restored the previous position: corners of the box (half-width and front/rear are larger than 256) could get into a
building or through a fence, and the 9 height samples then read a roof (`GetCubeInfo2` of a building square has only
the roof), rolling the car up to 90° beside walls and jittering as the contact came and went.

Wall bounce `FUN_010cd198(obj, hitX, hitY)`, per blocked axis (shown for x; y is identical with vy):

```
dmg = vx*vx >> 14;  health -= dmg
if dmg < 11: sound 5 Thud,  vol speed*10000, pitch 12000
else:        sound 0x4b Thud2, vol speed*10000, pitch 8000; crash flag +0x452 = 1
             if bus and player-driven: sound 0x4c Screams (pitch 15000+random(2000))
v = |vx| / 2
if hitX > 0: vx = +v;  x += v + 0x20
else:        vx = -v;  x -= v + 0x20
```

The bounce keeps 50% of the normal velocity and the tangential velocity is untouched, so cars slide along walls.
Example: a head-on hit at 300 costs 300²/16384 = 5 health; at 600 it costs 21.

### 2.8 Water (C)

`+0x33a` is set when the cube under the car has face type 4 (reflect/water, `cube+9 & 0x7f == 4`). For non-boats
the update takes the **sinking branch** (68k 125750; OS4 equivalent):

```
speed = 0; if pitch [4] (raw, 0..5759) < 0x5a0: pitch += 4*ticks
if z < 27000: z += 32*ticks; at each of the 9 rotated box points random(5)==1 → type 5 `Bubble`, ±250,
              v (random(10)−5, random(10)−5, random(100)−300), size 100 + random(150), level + 1
else:
    if driven by a player: lose the vehicle; message 2018 "Drowned!" (FUN_00091b2c(0x7e2)); a life is lost/the player is reset
    object state +0x426 = 6 (removed)
```

Mac (C, `do_Other` @ 10146280): the drowned player gets phrase 2018, the respawn timer 80 and loses a
multiplayer life; `ObjectDead`; state 6. Splash: on the ground in water, once (latch `+0x936`): the RC car gets
health −71, `ObjectSunk`, sound 0x2b at mass·10000, pitch 0x209e.

The boat (`+0x492c`) is the reverse: it may only move on water (see §4). On water it bobs (C, Mac): `CreateObject`
@ 10075630 sets roll/pitch rates `+0x7f4 = −25`, `+0x7f8 = −11`; each tick `+0x7fc += +0x7f4`, `+0x800 += +0x7f8`,
each rate ±2 toward 0; the offsets are added to the roll and pitch targets (§2.10). With the throttle it leaves a
wake: 2·t type-8 `Dust` puffs at `start + k·(pos − start)` (k = 1..2t) minus `rear·(sin, cos)`, ±50, z −100..100,
velocity `v ∓ (sin, cos)/2 + random(300) − 150`, vz `random(10) − 5`, size `0x82 + random(50)`.

### 2.9 Airborne (C, OS4 `FUN_010ccc5c(obj, ticks, steer)` for z < ground)

For a non-helicopter in the air (C, Mac `PositionAirborneUnit` @ 10082620; OS4 reads `[0xc9]`):

- The wheel trails reset, `onGround = 0`, the splash latch clears.
- The heading turns by `min(ticks·m.yaw, |speed >> 1|)` with the steering **of the last ground tick** (`+0x804`),
  not the live stick; left raises the heading going forward.
- The nose tips 16·ticks: down when moving forward until 0x5a0, up otherwise until −0x5a0.
- Velocity is unchanged, apart from gravity in §2.6 (no grip step in the air).

The flying helicopter (model flag `+0x8186`; engine on `+0x984`, set by `ObjTakeOff` @ 10105570 / `ObjLand`): the
lift `vz −= 0xaa` while `z > 0` and `vz > −0x100` keeps it near z = 0; in the air its velocity is pulled toward
the heading by its grip·t, the yaw rate ramps like a tracked vehicle on the live steer, pitch = speed, roll 0,
level 2.

Engine pitch uses the wheel speed (revs freely). Helicopter: see §4.

### 2.10 Slopes (C, Mac `PositionUnit` @ 10081450 = OS4 `FUN_010cbc7c` = 68k `FUN_000a1758`)

Called from `do_Other` @ 10146280 while the car is on the ground (z ≥ centre height) and not sinking, after the
steering and before `SkidUnit`, as `PositionUnit(obj, ticks)` (k = ticks, CONFIRMED). Heights h are the 3×3 box
heights of §2.7 (z down, so larger = lower).

```
L, C, R = min of each row (−half-width, centre line, +half-width)
if L − C < 0 or C − R > 0:  ratio = (L − R)·256 / (2·hw)
else: ratio = (|C − R| < |L − C|) ? (C − R)·256/hw : (L − C)·256/hw     # a crest: the gentler half
roll target = arctan(ratio / 256) + bob(+0x7fc) + lean(+0x7e8)      # 1/256: C, TOC 1017ceec+0x38
rear, mid, nose = centre line
if rear − mid < 0 or mid − nose > 0:  p = −arctan((rear − nose) / (front + rear))
else: p = −arctan(|mid − nose| < |rear − mid| ? (mid − nose)/front : (rear − mid)/rear)
speed += p·ticks >> 8                                                  # downhill speeds up, uphill slows
pitch target = p + bob(+0x800) + dip(+0x7ec)
roll, pitch = (old + target) / 2, the short way round
```

`arctan` @ 10083170 returns angle units in ±0x5a0 from two tables built by `InitArctan` @ 10083050 (C, constants
at TOC `1017cef8` → 101740e0 = {2880, 100, 10, …, 0, 1000, −10, −100, −1000}): `small[i] = (int)(2880·atan(i/100)/π)`,
`big[i] = (int)(2880·atan(10 + i)/π)`, i < 1000; `arctan(x) = small[(int)(100x)]` for 0 ≤ x < 10,
`big[(int)(x − 10)]` up to 1000, 0x5a0 beyond; mirrored for x < 0. `+0x7fc`/`+0x800` are the boat bob (§2.8), 0
for other vehicles.
Positive pitch is nose down.

In the air (`PositionAirborneUnit` @ 10082620): the nose tips by 16·ticks, down when moving forward until 0x5a0,
up otherwise until −0x5a0; the flying helicopter takes pitch = speed and roll 0. Sinking raises the pitch by
4·ticks to 0x5a0 (§2.8).

Port (`driving.rs` `position_unit`, `box_heights`): as above, with ticks = 1, lean, dip and bob included. The body roll
and pitch are drawn on the vehicle model. Nothing limits the slope other than `arctan`'s ±0x5a0: a flat
roll beside a wall depends on the box never reaching into the wall's square, which the §2.7 edge tests guarantee
(no step clamp, no level filter, no wall-cube skip in `GetCubeInfo2`).

### 2.11 Vehicle ↔ vehicle, people, scenery

See §3 (damage and collisions). Object contacts (Mac `CheckForCollisions` @ 1006a150) are reviewed item by item in
`analysis/mac/driving_review.md` (items 34-42: speed damping `(8s + s·m_self/(mi+mj))/9` per touching pair, z push,
integer distance, live positions, the RC car excluded, sparks per touching circle pair between two vehicles,
vehicle-vehicle Thud/Thud2 for both, run-over blood with droplets); the port follows them (`sim/contact.rs`).

---

## 3. Damage and collisions

(See §3.x below; merged from the damage investigation.)

---

## 4. Special vehicles

(See below.)

---

## 5. `Maps/<n>.vehicles`

Layout in `formats.md` (Companion map files, `.vehicles`). In short (C, loader OS4 `FUN_0106dbbc`, 68k
`FUN_00043148`): big-endian i32 version, three city booleans (allow the home-city 0/1/2 traffic models on a
user map, §1.2 `+0x48d4`), count ≤ 6, then count records {model, x, y, layer, heading}. The parked vehicles
spawn only in Rampage and multiplayer (`FUN_010692bc` ~49456).

---

## 6. Traffic AI

### 6.1 Which vehicles the AI drives (C, 68k `FUN_000e3a58` ~124165-124810)

The vehicle branch of the main update runs one shared driver for every vehicle whose state byte `+0x426` is
3, 8, 9, 10, 11, 0x0f, 0x14 or 0x15 (the state-2 branch reads the player's keys; state 0x0c police cars only
keep their siren on). Every other state (4 idle, 5 parked, 0x0e, 0x10, 0x11, 0x12) gives **no input**: the car
coasts to a stop. So `Actor_Attack`, `Actor_Follow`, plain flee (0x12) and goto-and-vanish (0x0e) only work on
people; a vehicle given them just stops. State 8 is never written anywhere (only 10 by the police code), so
"script chase" 8/9 is dead code.

Steering target `+0x2e6/+0x2ea` per state:

| State | Target |
|---|---|
| 3 | traffic lane choice `FUN_000a143e` (Mac `MoveUnit` @ 10081080), §6.4 |
| 8 / 10 | route node `node.xy << 3` (combat_police.md §1, §2.2) |
| 9 / 11 | the target object (`+0x41e`); police aim at `pos − v·(5−aggr)·3`, a point *behind* it (`pos += v`; combat_police.md §2.2) |
| 0x0f / 0x15 | the script point, written once by trigger types 0x17 / 0x32 / 0x44. For a **police car** (not a van) state 0x15 (and 0x14) is also in the pursuit-selection list (combat_police.md §2.2), so the car leaves the order at once: state 3 below wanted 3, state 10 from 3. State 0x0f is excluded. (68k ~124171, CONFIRMED.) |
| 0x14 | route toward the node farthest from the player (`+0x3ba`) |

There is **no path finding for scripted gotos**: states 0x0f/0x15 aim straight at the point. The scripts give
chains of nearby waypoints instead (races, convoys). Only police pursuit (10) and flee (0x14) use the route
graph. On reaching the goal node, 0x14 falls into the 8/10 transition code and becomes state 11 (straight chase
of `+0x41e`); this looks unintended.

**Arrival (0x0f / 0x15).** The test point is the bonnet: `p = pos + (sin h, cos h)·front·3/4` (front =
`+0x48c8`, `sin·front·3 >> 10`). It has arrived when `|p.x − x| + |p.y − y| <= 2·r`, with r = `+0x444` (1000 for
trigger types 0x17 and 0x44; the 4th argument for 0x32). Then `FUN_000a6640` fires the 0x17/0x44 trigger and
sets state 4. People use `|dx| + |dy| <= r` from their position instead.

**`+0x49e` "direct" flag.** Set only by trigger type 0x32 (`AddTrigger` 68k `0xb3866`) and never cleared. With it:
the aim offset is not clamped, the corner speeds are not used, the uncapped speed branch is open to non-police
drivers, and blocked drivers do not honk or push on (below). The race scripts (levels 10 and 21) use 0x32.

### 6.2 The driver (C)

Per frame (ticks = frame tick count):

```
d = pos − aim
if not +0x49e and state in (8,10,0xf,0x14,0x15): clamp d.x, d.y to ±1500     # far goals: approach at 45°
want = atan2(d)
e = want − heading (shortest way)
steer = e > 0 ? −1 (heading up) : +1, only if |e| > ticks·yaw/2               # dead zone, yaw = +0x48e4
    # exactly: up = wrap(want − h), down = wrap(h − want); up < down: −1 if up > dead; else +1 if down >= dead

# look-ahead, skipped when skill(+0x346) >= 99 and health >= 50 ("ramming" drivers)
look  = heading + ticks·yawrate·2
reach = max(front + speed²·4/brake + 0x400, 0x800)
for dist = 0x800, 0x1000, … <= reach: square of pos + dist·(sin, cos)(look)
    blocked if off the map or occupied (grid DAT_003178a0, own bits masked out)
traffic light: on the last square scanned, if cube byte +4 is 1..6: v = byte + phase (wrapped to 1..6);
    blocked if skill < 70 and v in {1, 2, 3, 6}                                 # skill >= 70 runs red lights
```

Not blocked: cruise band by skill, `lo = max(V − ticks·accel, L)`, `hi = V + ticks·brake`; throttle forward
if speed < lo, brake if speed > hi. "Corner" = steer ≠ 0 and not `+0x49e`.

| Skill | Straight V / L | Corner V / L |
|---|---|---|
| ≥ 98 and (police or `+0x49e`) | 730 / 500 | 240 / 120 |
| ≥ 95 | 330 / 220 | 200 / 100 |
| 91–94 | 230 / 150 | 180 / 90 |
| 51–90 | 200 / 100 | 140 / 70 |
| ≤ 50 | 150 / 80 | 100 / 50 |

For skill ≤ 50 the code subtracts **brake** instead of accel from V (sic). The car's top speed still limits
everything (§2.3).

Blocked: non-police drivers without `+0x49e` count an impatience timer `+0x450` (−ticks per frame, floor 0):

| Skill | When the timer is 0 | Honk while timer > |
|---|---|---|
| ≤ 30 | timer = 100 + rand(50), push on | never |
| 31–60 | honk, timer = 100 + rand(50), push on | 80 |
| 61–90 | honk, timer = 50 + rand(50), push on | 40 |
| ≥ 91 | honk, timer = 20 + rand(20), push on | 10 |

"Push on" clears the blocked flag and drives forward this frame. Still blocked: brake if speed > ticks·brake/2,
throttle if speed < −ticks·accel. "Honk" is the horn/fire input `_DAT_0034c3b4`, which plays the horn sound
(`+0x491a`). On the tank it fires the cannon instead (`FUN_000e2a50`), so a stuck AI tank shoots.

Anti-spin: below skill 81 the driver does not steer while the J-turn latch `+0x3c1` is set or while braking;
from 81 it only drops steering that fights the current yaw rate during a J-turn. The **Mac** (`do_Other`
@ 10146280) drops the "or while braking" (`throttle == 1`) part, so low-skill drivers steer while braking or
reversing; the port follows the Mac (`vehicle_ai.rs` `controller`). CONFIRMED.

Last, the script speed cap: `if speed > +0x386: speed = +0x386` (default 1000000; `Actor_SetMaxSpeed`). The
helicopter (`+0x4928`) never honks; a flying model (`+0x492a`) gets no input while `+0x338` is 0.

Default skill: traffic `clamp(rand(90) + aggr·8 − 30, 0, 99)` (§1.4); police in pursuit `wanted + 0x5c`
(`+0x5e` when aggr ≥ 4); script actors keep the object default `Random(100)` (0..99, set by the constructor 68k `FUN_000a2b80`) until `Actor_SetAggro` (all readers of
the skill `+0x346`: `script_api.md` §5). The race scripts set
95–100, so the leaders (98+) run the uncapped band and the 99/100 cars skip the look-ahead.

### 6.4 Lane choice (C, Mac `MoveUnit` @ 10081080, 68k `FUN_000a143e`, OS4 `FUN_010cb7e4`)

State 3 keeps a target point (`+0x2e6/+0x2ea`, a square centre) and runs the driver of §6.2 at it with no aim
clamp. Each frame, if the target is within r of the car on both axes (Amiga `r = max(5·speed²/yaw, front)`,
**Mac** `r = max(speed·1000/yaw, front)`; equal at speed 200, so fast Amiga traffic re-targeted squares
ahead; the port follows the Mac):

```
level = GetCubeNum(target square, (z − 0x200) >> 4) − 1   (clamped >= 0; 2 when above every cube)
f = cube[level].flags & 0x1f; if 0: cube[level+1] (level < 2); if still 0: cube[level−1] (level > 0)
drop the exit behind the car by heading: 0x2d0..0x86f (E) drops W, 0x870..0xe0f (N) drops S,
    0xe10..0x13af (W) drops E, otherwise (S) drops N — only if another bit remains (dead ends turn round)
n = bits set; none: keep the target
k = random(n) over N, E, S, W, then 0x10 = straight on in the stored direction +0x423 (0 counts as W)
target = centre of that neighbour of the target square; +0x423 = the direction taken (not for 0x10)
```

A new object's target is its own position (Mac `CreateObject` @ 10075630), so a spawned car picks its first
square from the spawn square itself. Every state shares the target: a car put back into state 3 (pursuit
off, `Actor_ReleaseIdle`) drives to wherever it was aiming and the lane code takes over there.

**Occupancy grid** (C, Mac `AddCollideObj` @ 10055c70, 68k `FUN_000cdfdc`): each object marks every square of
the axis-aligned box of its four corners (±radius across; −rear and +front along the heading; a person's corners
all at +radius along, sic) at level min(level, 2), the player's person model one square wider; projectiles
count. It is rebuilt for each object after its update. The look-ahead and the spawner test it.

**Horn** (C, Mac `DoVehicleFire` @ 10070c20): the honk is the fire input. It starts a *looped* horn
(`+0x491a`, 20e6, `clamp(16e6/mass, 3000, 20000)` Hz; police: the siren at 25e6/10000) that follows the car
and is deleted the first frame the input is off. Port: `vehicle_ai::horn`.

The same code serves the player (C, Mac `do_Other` @ 10146280 and `DoVehicleFire` @ 10070c20):

- Each object's update starts with `_DAT_1017cc70 = 0`. In state 2 (player driving, no RC car out:
  `_DAT_1017c484[+0x994] == −1`) the shoot key `_DAT_1017c388[player]` sets it to 1 — "Shoot/horn"
  (phrase 2461; default Space, alt unbound on the Amiga, Ctrl in the port). It is read regardless of the
  controls-off flag `+0x988`, which only zeroes throttle and steering. The shoot bit of the input word
  carries it into replays. On foot the same key shoots.
- At the end of `do_Other`: `cc70 != 0` → `DoVehicleFire(obj)`; else, for a police model, the `_red`/`_blue`
  model goes back to the plain one, and `if (+0x918 >= 0) { DeleteSound(+0x918); +0x918 = −1 }`.
- `DoVehicleFire` by type: tank (`+0x8164`) fires its gun (BigGunFire 0x1c, 15e6 @ 9000, a missile, 4×2
  dust puffs); a type with a linked model `+0x815c` (bomb truck) launches its RC car if the player has
  none out (port: not implemented); otherwise the horn: `+0x918 < 0` → `AddSound(+0x8178, loop, 20e6,
  clamp(16e6 / +0x8148 mass, 3000, 20000))`, police (`+0x8166 == 1`) `25e6 @ 10000`; else
  `ModifySound(+0x918, pos)`. Police also flash: `+0x940 += ticks`, mod 10; < 5 → the `_blue` model,
  else `_red`; `AddLighting(x, y, z − 500)` with (20e6, 20e6, 200e6) for blue, (200e6, 20e6, 20e6) for red.
- Horn sounds per type (`InitStructures` @ 1005ffb0, `+0x8178`): 3 CarHorn by default, 0x0b Siren for the
  police cars and vans, 0x2c IceCream for the ice-cream van. Helicopter, boat and RC car keep CarHorn.
- Nothing in the simulation reacts to the horn: no pedestrian or traffic code reads `cc70` or `+0x918`
  (people's `cc70` uses are their own fire input). Only sounds, the police model and lighting change.

Port: `driving::horns` (player, and deleting every horn not sounded this tick), `vehicle_ai::horn`
(slot `Drive::horn`), police via `pursuit::sirens` (also during pursuit) with the light glow in
`lighting::collect_lights`.

### 6.3 Port deviations

Not facts about the original (those are in §6.1–6.2, C); this is what the port does differently on purpose.
The port follows §6.1–6.2 and §6.4 for traffic (`sim/traffic.rs`, the shared `vehicle_ai::controller`),
states 0x0f/0x15/0x14 and police 10/11 (`sim/vehicle_ai.rs`, `sim/pursuit.rs`). Where the original leaves a vehicle idle, the port drives it: attack = ram the target like
state 9 (an attacking tank fires within 6000 with the guards' `rand(101 − aggro)` rule when facing it), follow =
drive at the leader until within 0xc00, plain flee = flee by route, goto-and-vanish = goto, then remove. At the
farthest flee node the port stops instead of switching to state 11.

---

## 7. Sound

(See below.)

---

## 8. Timing and conventions summary

(See §0 header.)
