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
| +0x48c4 | from mesh | max\|X\|·16: half-width. Collision radius and skid-mark wheel offsets. Helicopter: divided by 3. |
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
- Brake key (`DAT_0114c62c[p]`): throttle = +1 if speed > 0, −1 if speed < 0, else 0. It brakes to a stop and
  never reverses.
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
Right (steer +1) while going forward lowers h, which matches people.md.

**Heading snap to 45°** (C, OS4 main.c 2436–2490; 68k 125690–125750). If speed ≠ 0, every frame:

```
target = ((h + 360) / 720) * 720        # nearest multiple of 720 (45°)
move h toward target by at most 4*ticks
```

It wraps correctly near 0/5760. This gives the game's "lane-aligned" feel.

**Visual lean** `[0xc2]`: target = `±speed·256/maxF`, with the sign set by steer and travel direction (inverted
when `+0x492e`, boats). It slews 64 per tick and relaxes by 0x20 per tick with no steering.

### 2.5 Grip, surfaces, skidding (C, OS4 `FUN_010cc288(obj, map, ticks)`)

Velocity is not tied to the heading. Every frame it is pulled toward `speed·(sin h, cos h)`:

```
tvx = speed*sin[h] >> 8;  tvy = speed*cos[h] >> 8
g   = m.grip  (4 if J-turn latch set)
surface = CubeTextureList flag of the top-face texture of the cube under the car (FUN_0106dfcc, default 4)
switch surface:
  0x22 (34, grass/mud):  r = g/2;          if speed > 450: speed -= min(3*ticks, speed-450)
  0x24 (36, sand):       r = g*2;          if speed > 350: speed -= min(20*ticks, speed-350)
  0x32 (50, snow):       r = 4;            if speed > 200: speed -= min(10*ticks, speed-200)
  0x33 (51, slush):      r = g*10/25
  0x34 (52, ice):        r = 1
  default (4 tarmac, 35, 65, off-map, layer>=3): r = g*4/5
vx += clamp(tvx-vx, -r*ticks, r*ticks);  vy likewise
if heli: return
skid = clamp(2*(|vx-tvx| + |vy-tvy|)/ticks, 0, 255)   # slip measured before the correction
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
- Skid sound handle `[0x10d]`:
  - On land and not 4928: volume `skid·40000`, pitch `skid·10+10000`, at z+250.
  - Otherwise volume 0.
- If skid == 0, the wheel trail points reset to −1.

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

### 2.6 Movement sub-steps, gravity, landing (C, OS4 `FUN_0105b9ec`)

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

Port (`collide.rs` `box_points` / `vehicle_walls`, `driving.rs`): as above (one sub-step per tick). Deviation kept:
the speed is halved on a hit (`BounceUnit` leaves it). Until 2026-10 the port tested a cross of half-size 256 and
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
speed = 0; pitch [4] += 4*ticks up to 0x5a0
if z < 27000: z += 32*ticks; random(5)==1 per wheel → bubble particles (DAT_0034c128)
else:
    if driven by a player: lose the vehicle; message 2018 "Drowned!" (FUN_00091b2c(0x7e2)); a life is lost/the player is reset
    object state +0x426 = 6 (removed)
```

The boat (`+0x492c`) is the reverse: it may only move on water (see §4).

### 2.9 Airborne (C, OS4 `FUN_010ccc5c(obj, ticks, steer)` for z < ground)

For a non-helicopter in the air:

- Steering rotates the heading at `min(ticks·m.yaw, |speed|/2)`.
- The pitch [4] tips the nose toward level at 16·ticks.
- Velocity is unchanged, apart from gravity in §2.6.

Engine pitch uses the wheel speed (revs freely). Helicopter: see §4.

### 2.10 Slopes (C, Mac `PositionUnit` @ 10081450 = OS4 `FUN_010cbc7c` = 68k `FUN_000a1758`)

Called from `do_Other` @ 10146280 while the car is on the ground (z ≥ centre height) and not sinking, after the
steering and before `SkidUnit`, as `PositionUnit(obj, ticks)` (k = ticks, CONFIRMED). Heights h are the 3×3 box
heights of §2.7 (z down, so larger = lower).

```
L, C, R = min of each row (−half-width, centre line, +half-width)
if L − C < 0 or C − R > 0:  ratio = (L − R)·256 / (2·hw)
else: ratio = (|C − R| < |L − C|) ? (C − R)·256/hw : (L − C)·256/hw     # a crest: the gentler half
roll target = arctan(ratio · K) + obj+0x7fc + lean(+0x7e8)            # K hidden; INFERRED 1/256
rear, mid, nose = centre line
if rear − mid < 0 or mid − nose > 0:  p = −arctan((rear − nose) / (front + rear))
else: p = −arctan(|mid − nose| < |rear − mid| ? (mid − nose)/front : (rear − mid)/rear)
speed += p·ticks >> 8                                                  # downhill speeds up, uphill slows
pitch target = p + obj+0x800 + dip(+0x7ec)
roll, pitch = (old + target) / 2, the short way round
```

`arctan` @ 10083170 returns angle units in ±0x5a0 from two tables built by `InitArctan` @ 10083050 (below 1:
`atan(i/1000)`; above: `atan(1 + i)` by whole units; 0x5a0 past the table; the table constants are INFERRED).
Positive pitch is nose down.

In the air (`PositionAirborneUnit` @ 10082620): the nose tips by 16·ticks, down when moving forward until 0x5a0,
up otherwise until −0x5a0; the flying helicopter takes pitch = speed and roll 0. Sinking raises the pitch by
4·ticks to 0x5a0 (§2.8).

Port (`driving.rs` `position_unit`, `box_heights`): as above, with ticks = 1. The `+0x7fc`/`+0x800` terms are
unknown and taken as 0, and the steering lean is drawn separately rather than added to the roll. The body roll
and pitch are drawn on the vehicle model. Nothing limits the slope other than `arctan`'s ±0x5a0: a flat
roll beside a wall depends on the box never reaching into the wall's square, which the §2.7 edge tests guarantee
(no step clamp, no level filter, no wall-cube skip in `GetCubeInfo2`).

### 2.11 Vehicle ↔ vehicle, people, scenery

See §3 (damage and collisions).

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
| 3 | traffic lane choice `FUN_000a143e` (square centres, junction picks). The next square is picked once the car is within r of the target centre on both axes: Amiga `r = max(5·speed²/yaw, front)`, **Mac** (`MoveUnit` @ 10081080) `r = max(speed·1000/yaw, front)` (front = `+0x48c8`; equal at speed 200), so fast traffic no longer re-targets squares ahead. The port follows the Mac (`traffic.rs`). CONFIRMED. |
| 8 / 10 | route node `node.xy << 3` (combat_police.md §1, §2.2) |
| 9 / 11 | the target object (`+0x41e`); police lead it by `v·(5−aggr)·3` |
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

### 6.3 Port deviations

Not facts about the original (those are in §6.1–6.2, C); this is what the port does differently on purpose.
The port follows §6.1–6.2 for states 0x0f/0x15/0x14 and police 10/11 (`sim/vehicle_ai.rs`,
`sim/pursuit.rs`). Where the original leaves a vehicle idle, the port drives it: attack = ram the target like
state 9 (an attacking tank fires within 6000 with the guards' `rand(101 − aggro)` rule when facing it), follow =
drive at the leader until within 0xc00, plain flee = flee by route, goto-and-vanish = goto, then remove. At the
farthest flee node the port stops instead of switching to state 11.

---

## 7. Sound

(See below.)

---

## 8. Timing and conventions summary

(See §0 header.)
