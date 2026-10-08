# People: the player and pedestrians on foot

This document covers how people work in Payback: the player character and pedestrians on foot. It explains the
`Human.motion` animation, how people models are drawn, player control, collision, getting into and out of
vehicles, pedestrian AI, timing, and conventions.

The target is the **Update 7 68k build** (`Payback.exe`, decompiled in `analysis/ghidra_68k`). The OS4 PPC
port (`analysis/ghidra_os4`) is cited when it reads more clearly. Its main per-object AI and physics function,
`FUN_0105f594`, did **not** decompile, so the movement and AI code below is read from its 68k twin,
`FUN_000e3a58`.

**Status marks:**

- **C** = CONFIRMED: read directly in code.
- **I** = INFERRED: deduced from code or data, not proven.

Scratch dumps of the functions quoted here are in `analysis/people/` and `analysis/people_ai/`.
`python analysis/people/fn.py {os4|68k} ADDR` prints a function.

Units match `docs/re/rendering.md`:

- **internal**: object x and y; one square is 0x800.
- **z**: object height, pointing **down**; `z = (1500 − H)·16`.
- **h-unit**: map height unit; a flat ground cube is 256 h-units.
- **"map units"**: the port's 256-per-square convention, used by `crates/payback/src/city.rs`.

---

## 1. Conventions, tick and coordinates

### 1.1 Game tick (C)

The game uses a **variable timestep counted in whole ticks.**

- `ticks` is the number of ticks elapsed this frame: `DAT_00348090` in 68k, `DAT_0114c6a0` in OS4.
- It is clamped to **1..5**.
- Every rate in this document is "per tick" and gets multiplied by `ticks`. Some of the code instead loops
  `ticks` times (sub-stepping).

**68k build (Update 7, the target).** Main loop `FUN_000ec686`, around line 1950 of its decompile:

```
now = DateStamp ticks (50 Hz, FUN_00110e28; dos.library LVO −0xC0)
ticks_raw = (now − last + 1) >> 1                   // 2 DateStamp ticks per game tick → 25 Hz
wait while now < ticks_prev*2 + last                // frame limiter: at most 25 fps at 1 tick/frame
ticks = clamp(ticks_raw, 1, 5); last = now (end of frame)
```

So **one game tick is 1/25 s on 68k** (C).

**OS4 port.** It scales the 50 Hz timer by 60/50 (`t*0x3c/0x32`, in `FUN_010692bc` and `FUN_0110761c`), so a
tick there is **1/30 s** (C). A faithful port of Update 7 should use **25 Hz**. With exactly 1 tick per step,
every rule below reproduces the original exactly.

The demo time limit is 9000 ticks: `DAT_0034c414`, message 2490 "Time out!".

### 1.2 Coordinates (C)

| Quantity | Value |
|---|---|
| x, y | Integers. Square = 0x800 (2048). Square index = `pos >> 11`. World clamped to `[0, 0x3ffff]` (`FUN_00044f82`). x points east, y points south. |
| z | Down-positive. 16 per h-unit. Ground on a single 256-high cube: `z = (1500−256)·16 = 0x4dc0`, which is also the constructor default. |
| Square centre | `sq·0x800 + 0x400` |
| Angles | **0x1680 = 5760 units per turn** (16 per degree). `sin[a] = DAT_00311e20` / `DAT_012ac16c`, `cos[a] = DAT_0030c420` / `DAT_012a676c`, both 8.8 fixed (256 = 1.0). Exact generation is in rendering.md. |
| Heading h (+0x14) | Forward vector = **(sin h, cos h)**. h = 0 faces south (+y), 0x5a0 faces east. **Increasing h turns left**; the player's "turn right" lowers h. |
| Model axes in world (pitch 0, roll 0) | model +Y → (sin h, cos h) (forward); model +X → (cos h, −sin h) (the object's **left**); model +Z → down |
| Right-hand vector | (−cos h, sin h), which is model −X |

Conversion to the port's map units (256 per square, height up): `world_xy = pos/8`, `world_up = 1500 − z/16`.

### 1.3 Object table (C)

- `DAT_0030c318[0..31]` (68k) / `DAT_012a6270[]` (OS4) hold pointers to instances. An instance is 0x4b8 bytes in
  OS4; the constructor is `FUN_010c0c1c`.
- The last used index is `DAT_0011a738` (68k) / `DAT_0114c2d8` (OS4). At most 32 objects exist; `FUN_01046ee4`
  returns 0x20 when the table is full.
- The player's object index is `DAT_0030c3a0[player]` (68k) / `DAT_012a62f4[player]` (OS4). It points to the
  person, or to the vehicle while driving.

---

## 2. People models

### 2.1 Loading: the people special case (C, 68k `FUN_000d7874`, OS4 `FUN_0104e13c`)

The `.Obj` reader is `FUN_0009610c` (68k) / `FUN_010bf8bc` (OS4). It does the following:

- Stores every vertex coordinate **×16**:
  - X at `+8+2i`, Y at `+0x172+2i`, Z at `+0x2dc+2i`
  - 1-based i, at most 180 vertices
  - the 4th value (limb) as a byte at `+0x446+i`
- Computes these bounds (all in ×16 units):

| Field | Meaning |
|---|---|
| +0x48c4 | max \|X\| over all vertices (half-width, used as the **collision radius**) |
| +0x48c8 | max Y over vertices with Z > −0x29 (near-ground vertices) |
| +0x48cc | \|min Y\| over the same vertices |
| +0x48d0 | max \|Z\| (no gameplay reader found) |
| +0x48f4 | `= −max|X|`; door lateral position (vehicles) |
| +0x48f8 | `= maxY/2`; door longitudinal position (vehicles) |
| +0x490e/+0x4912 (68k) or +0x4910/+0x4914 (OS4) | mean X / mean Y of Z==0 vertices |
| +0x4916 (68k) or +0x4918 (OS4) | `max(min(xRange, yRange)/2, 0x80)`; scenery collision size |

After loading each **people** model, the loader sets:

| Field | Value | Meaning |
|---|---|---|
| +0x4909 | 1 | "person" class. 2 = projectile, 0 = vehicle/scenery. This selects the people render transform and the people physics/collision. |
| +0x48c4 | `MaleNormal.48c4 / 3` = 36·16/3 = **192** | Person radius. Every people model **copies MaleNormal's value**, including the bigger PedMale4. |
| +0x48d0 | `MaleNormal.48d0 / 3` = 12·16/3 = 64 | |
| +0x48f4, +0x48f8 | 0 | |
| +0x48ec | **mass** (see below) | Used in collision momentum and damage (`(m1v1+m2v2)/(m1+m2)` at OS4 `FUN_01056c58` ~line 45288). It is **not** hitpoints. |
| +0x490a | 1 | Police model: PoliceMale1, PoliceMale1_Fire |
| +0x4908 | weapon/pose code (see below) | |

**Mass (+0x48ec) per model:** 90 for MaleNormal*, PedMale1/2/6 and PoliceMale1; 120 for PedMale3; 100 for
PedMale4 and PedMale5; 95 for PedMale7.

**Weapon/pose code (+0x4908) per model:**

| Model | +0x4908 |
|---|---|
| MaleNormal, PedMale1–4, PedMale6_KKK | 0xfe (unarmed; fire = punch) |
| MaleNormalBazooka | 0 |
| MaleNormalFlamethrower | 1 |
| MaleNormalGrenade | 2 |
| MaleNormalMiniGun | 3 |
| MaleNormalShotgun, PedMale5, PedMale7, PoliceMale1 | 4 (shotgun) |
| MaleNormalPistol | 6 |
| MaleNormalBomb | 7 |
| MaleNormalPunch | 0xfd |
| MiniGun_Fire | 0xfc |
| Shotgun_Fire, Police/Ped5/Ped7_Fire | 0xfb |
| Pistol_Fire | 0xfa |

Player model by current weapon (instance +0x360 OS4 / +0x35e 68k) indexes the table `DAT_012a6600[w]` (C):

| w | Model | w | Model | w | Model |
|---|---|---|---|---|---|
| 0 | MaleNormal (fists) | 5 | Shotgun | 10 | PedMale4 |
| 1 | Bazooka | 6 | PedMale1 | 11 | PedMale5 |
| 2 | Flamethrower | 7 | PoliceMale1 | 12 | PedMale6 |
| 3 | Grenade | 8 | PedMale2 | 13 | Pistol |
| 4 | MiniGun | 9 | PedMale3 | 14 | Bomb |
| | | | | 15 | PedMale7 |

Number keys 1–7 and 0 select weapons (`FUN_01051a10`): 0 = fists, 1 = pistol (0xd), 2 = shotgun (5),
3 = flamethrower (2), 4 = grenade (3), 5 = minigun (4), 6 = bazooka (1), 7 = bomb (0xe). The switch only
happens if that weapon's ammo is above 0, and it plays sound 0x14.

**Firing and punching do not use `Human.motion`.** They swap the instance's model pointer (+0x24) for 1–2
frames:

- fists → MaleNormalPunch
- shotgun → *_Fire
- police → PoliceMale1_Fire
- PedMale5/7 → their _fire variants

Weapon models leave the arm vertices on limb 0, so only head, shoulders and legs animate:

| Limbs present | Models |
|---|---|
| 1–7, 9–14 | MaleNormal, PedMale1–4, Bomb, Grenade |
| [0,]1,2,5–7,9–14 (no 3,4,10,11 forearms/hands) | Shotgun, MiniGun, Flamethrower, Police, PedMale5/7 |
| 1,2,5,6,7,9–14 | Pistol, Punch, Bazooka |

### 2.2 Limb layout (C, from MaleNormal.Obj)

Limbs are **groups of vertices**, not bones. The faces between groups stretch.

The table below is in file units. Y points down the body (head −88, feet +90); Z is front/back (the model faces −Z); X is
lateral (negative X is the figure's right).

| Limb | Part (right side) | Y | Limb | Part (left side) |
|---|---|---|---|---|
| 1 | head (14 verts) | −88..−66 | — | — |
| 2 | right shoulder / upper arm | −60..−50 | 9 | left shoulder |
| 3 | right elbow | ≈ −30 | 10 | left elbow |
| 4 | right hand | 6 | 11 | left hand |
| 5 | right hip | 0..18 | 12 | left hip |
| 6 | right knee | 44 | 13 | left knee |
| 7 | right foot | 90 | 14 | left foot |

Limb **8 is never used** by any model; the motion data for it is zero-weight in practice. Limb 0 is static (no
animation).

### 2.3 Render transform for people (C: 68k `FUN_000974a0`, OS4 `FUN_010c101c`)

Normal objects use model coordinates rotated by roll (+0xc, X–Z plane), then pitch (+0x10, Y–Z plane), then
yaw (+0x14), and added to the position unscaled. For people (`+0x4909 == 1`) the same rotations apply, but:

```
if pitch == 0: Y += 0x5dc            // 1500 in ×16 model units (93.75 file units)
a  = (Z·cos r − X·sin r)>>8 ;  b = (X·cos r + Z·sin r)>>8
Y' = (Y·cos p − a·sin p)>>8
depth = (Y·sin p + a·cos p) >> 10    // normal objects: >> 8  → people are 1/4 vertically
horiz_x = (sin h·Y' + cos h·b) / 3   // normal objects: no /3 → people are 1/3 horizontally
horiz_y = (cos h·Y' − sin h·b) / 3
```

**Standing: pitch = 0x5a0 (90°).** Every frame the people physics sets pitch to 0x5a0 unless the person is dead
or stunned. The results:

- Model Y becomes vertical: depth = Y·16/4, so the head is up.
- Model −Z points forward.
- Model +X points to the object's left.

**Lying: pitch = 0.** This happens when the person is dead (state 7) or stunned (stun timer > 0). The body lies
flat along the heading. After the +1500 offset the head is about 31 internal units ahead of the origin and the
feet about 980 ahead, so the body extends forward from the spot where it stood. Model Z becomes thickness/4.

**Effective people scale (C):**

| Direction | Internal units per file unit | Port map units per file unit | Whole person |
|---|---|---|---|
| Horizontal | 16/3 = 5.33 | **2/3** (normal models: 2) | 72 wide (arms) → 384 internal ≈ 0.19 square = **48 map units**; collision radius 192 internal = 24 map units |
| Vertical, top-down view | 16/4 = 4 | **1/4** (normal models: 1) | 178 tall → 712 z = **44.5 h-units** (≈ 1/6 of a ground cube) |

- The rotating (Warp3D/PPC) view halves all depths, which gives the person **89 h-units tall** there (see rendering.md
  §0).
- A port that wants uniform proportions should use **2/3 horizontally and 1/2 vertically**.
- The current `PEOPLE_SCALE = 0.5` in `crates/payback/src/player.rs` is wrong on both axes.

**Feet height (C/I).** The logic keeps the **origin** (model Y = 0, hip height) at the floor z (§4.3). The
renderer puts the feet 90·16/4 = 360 z below the origin. The global object depth bias of −0x100 (objects drawn
16 h-units nearer the camera; rendering.md §0) partly offsets this, so the feet are drawn about **6.5 h-units
below the floor**. This is invisible at the normal camera height. A port should put the feet on the floor.

The in-water flag (OS4 +0x33a) clips drawn vertices deeper than z = 25000 (C, `FUN_010c101c` `local_90`), so
the parts of a swimmer below the water are hidden.

---

## 3. `Motion/Human.motion`

### 3.1 File format (C: 68k `FUN_0009508a`, OS4 `FUN_010bd3f0`)

- The file is ASCII.
- It holds **336 signed decimal integers, one per line, CRLF.** The 68k reads them with
  `fscanf("%d%1c%1c")`, so each number must be followed by exactly 2 characters.
- Layout: **12 frames × 14 limbs × 2 values**, with the pair for a frame and limb read together:

```
for f in 0..11: for l in 0..13:  A[f][l] = int; B[f][l] = int
```

- In memory these become two 168-int arrays: `A` at `DAT_01314464` (OS4) and `B` at `A + 0xa8` ints (0x540
  bytes in total).
- If the file is missing, everything is 0 and the message "Unable to load motion file %s" is printed.
- Array index = `frame*14 + (limb−1)`.

### 3.2 Meaning (C)

**This is a single walk cycle**, not several animations. Per frame and limb:

- `A` is a translation along model **Z** (forward/back swing). Negative Z is forward.
- `B` is a translation along model **Y** (vertical; negative = lift).

Values are in file units. At full weight the renderer multiplies them by 16, the same scale as stored vertices.

**There is no rotation and no pivot.** Every vertex of limb `l` is shifted by the limb's offset, and the
connecting faces stretch. Frames 6–11 are frames 0–5 with left and right swapped (limbs 1–7 ↔ 8–14), so the
cycle is symmetric.

Example, foot limbs 7/14:

- **A** (swing): 62, 92, 83, 55, 22, 17 | −18, −50, −62, −40, −5, 34
- **B** (lift) for limb 14: −14, −34, −52, −67, −70, −58

Arms swing opposite to the legs (limb 4: 15, 16, 10, −9, −27, −39). The head bobs slightly (limb 1, B 3..7).

### 3.3 Blending (C, render `FUN_010c101c` / `FUN_000974a0`)

Instance fields:

- `+0x18` = frame **N** ("next", −1 = none)
- `+0x1c` = frame **P** ("previous", −1 = none)
- `+0x20` = blend t, 0..256

For a vertex with limb l ≥ 1, using arrays `M ∈ {A, B}`:

```
both  N,P ≥ 0 : off = ((M[N][l]−M[P][l])·t >> 8) + M[P][l]) · 16
P == −1       : off = (t · M[N][l]) >> 4            // fade in from rest pose
N == −1       : off = (M[P][l] − (M[P][l]·t >> 8)) · 16   // fade out to rest pose
both −1       : no animation (static path)
Z += offA ; Y += offB      (before the transform of §2.3)
```

A special mode (`model+0x492a ≠ 0`, the helicopter) reuses the limb byte as a rotor-blade index. It does not
apply to people.

### 3.4 Frame advance (C, 68k `FUN_000e3a58`)

The frame advance is driven by **speed**, not time. It runs every frame for every person:

```
if speed == 0:
    if speed_prev != 0: N = −1; t = 0                  // start easing back to rest
    if P != −1: t += 64·ticks; if t > 255: t = 0; P = −1  // 4 ticks to rest
else:
    if speed_prev == 0: t = 0; P = −1; N = 5          // start: fade into frame 5
    t += speed·ticks·3/2
    while t >= 256 (max 3×): t −= 256; P = N; N = (N+1) mod 12;  footstep check
    while t < 0   (max 3×): t += 256; N = P; P = (P−1) mod 12   // walking backwards
footstep check (when the frame being left is 0 or 6, i.e. a foot plants):
    cube = square(x,y)[level]
    if top face type == 4 (water): sound 0x41 (splash)
    else sound = surface sound of top texture (FUN_00091dde); if sound is 0x32/0x33 (snow):
         footprint effect type 8 at (x ± cos h/2, y ∓ sin h/2) (left/right by frame)
```

At top speed 96 the blend gains 144 per tick, which is 0.5625 frames per tick. A full 12-frame cycle takes 21.3
ticks: **0.85 s at 25 Hz**.

There is no separate run animation. "Running" is just a higher speed, so the legs cycle faster.

**Other poses (C):**

- dead or stunned: lying (pitch 0), §2.3
- swimming: the normal walk cycle at water speeds (§4.4)
- punch and shoot: model swaps (§2.1)
- driving: the person object is removed
- There is no jump.

---

## 4. Player control on foot

### 4.1 Input → virtual stick (C)

Controls (phrases 2457–2467) and their input index `k` for `FUN_0103d9a0`:

| k | Control |
|---|---|
| 0 | Accelerate |
| 1 | Reverse/brake |
| 2 | Turn left |
| 3 | Turn right |
| 4, 5 | Shoot/horn, Shoot (alt) |
| 6 | In/out vehicle (edge-triggered) |
| 7 | Brake only |
| 8, 9 | Next/prev weapon |
| 10 | Repeat message |

Per-player input arrays are `short[4]` indexed by player:

| Input | 68k | OS4 |
|---|---|---|
| left | DAT_0034c148 | DAT_0114c66c |
| right | c150 | c664 |
| accelerate | c158 | c65c |
| reverse | c160 | c654 |
| fire | c168 | c64c |
| in/out | c170 | c634 |
| brake | c178 | c62c |

Replays store one u16 per frame (`FUN_01051a10`):

```
0x4000 left | 0x2000 right | 0x1000 accel | 0x800 reverse | 0x400 fire | 0x200 in/out | 0x100 brake
| (weapon_select & 0x1f) << 3 (10 = none) | ticks & 7
```

The people physics (68k `FUN_000e3a58`, branch `model+0x4909 == 1`) turns the input of the player-controlled
person (state 2) into three values:

```
steer    = left ? −1 : right ? +1 : 0
throttle = accel ? −1 : reverse ? +1 : 0
if brake: throttle = speed > 0 ? +1 : speed < 0 ? −1 : throttle
fire     = fire button
if dead (state 7) or stunned (stun timer > 0): steer = throttle = fire = 0; speed = 0; pitch = 0
else pitch = 0x5a0
```

AI people produce the same three values (§7); the movement code below is shared by everyone.

### 4.2 Turning, speed, velocity (C, 68k offsets / OS4 offsets)

```
// turn rate  (+0x326 / +0x328)
if steer > 0:  if rate > 0: rate = 0;  rate = max(rate − 15·ticks, −100)
if steer < 0:  if rate < 0: rate = 0;  rate = min(rate + 15·ticks, +100)
if steer == 0: rate = 0
heading = (heading + ticks·rate) mod 0x1680

// speed (+0x3ae / +0x3b0), health H = +0x34a / +0x34c
if throttle < 0:   // forward
    land:  speed += 24·ticks; if speed > clamp(H+50, 24, 96): speed = clamp(H+50, 24, 96)
    water: speed +=  6·ticks; cap clamp(H+50, 6, 24)
if throttle > 0:   // backward
    land:  speed −= 24·ticks; floor −clamp(H·70/100 + 35, 24, 64)
    water: speed −=  6·ticks; floor −clamp(H·70/100 + 35, 6, 16)
if throttle == 0:  speed moves toward 0 by 8·ticks
vx (+0x2fa/+0x2fc) = sin[h]·speed >> 8 ;  vy (+0x2fe/+0x300) = cos[h]·speed >> 8
```

| Quantity | Per tick | Per second at 25 Hz (68k) | At 30 Hz (OS4) |
|---|---|---|---|
| Max forward speed (health ≥ 46) | 96 internal | 2400 = **1.17 squares/s** = 300 map units/s | 1.41 sq/s |
| Max backward speed (health ≥ 42) | 64 | 0.78 sq/s | |
| Time from rest to full speed | 4 ticks | 0.16 s | |
| Max turn rate | 100 units | 2500 = **156°/s** (reached after 7 ticks) | 187°/s |
| Swim speed | 24 forward / 16 back | 0.29 sq/s | |

Speed falls off below health 46: at health H < 46 the cap is H + 50, with a minimum of 24.

### 4.3 Vertical motion, gravity, falling (C: 68k `FUN_000e0e46`, called with ticks sub-steps)

```
repeat ticks times:
    if not in_water:
        if z < floor: on_ground = 0
        else: vz = 32; z = floor; on_ground = 1
        vz += 32; z += vz            // gravity 32 z/tick²; on ground this re-snaps every tick
    elif not on_ground:
        if z >= floor: vz = 0; z = floor; on_ground = 1; first time: splash sound 0x2b (volume ∝ mass)
        vz += 32; z += vz
    x += vx; y += vy
    FUN_00044f82(obj, map, 0)        // map collision (§5.1), recomputes floor/level/in_water
```

After the step, in the people branch: `if floor <= z: vz = 0; on_ground = 1; if !in_water: z = floor` (swimming
is handled in §4.4).

- **There is no jump control** (C).
- **No fall damage** (C: no writer of health `+0x34a` depends on vz). Landing just snaps to the floor.
- People step down any height and fall at 32 z/tick². That is 2 h-units/tick², about 1250 h-units/s² at 25 Hz, or
  roughly 5 cubes/s².
- If you hit a ceiling while rising (vz < 0) and a cube above would be entered, the position is restored and
  `vz = −2/3·vz` (C, `FUN_00044f82`).

### 4.4 Water: swimming and drowning (C, 68k `FUN_000e3a58` people branch)

You are **in water** (68k +0x338 / OS4 +0x33a) when the top face of the cube you stand on has type 4
(Reflect).

- The floor of a flat water cube is its **water surface**, `prevTop − height2·16` (the cube's second height byte),
  not its top. In map 0 the river cubes have height2 = 0, so the water surface is a full cube below the bank:
  z = 24000.
- Speed is limited as in §4.2.
- When on or below the water surface:

```
if z < 27000:
    z += 24·ticks                                   // sinking
    1-in-5 chance per corner of a bubble effect
else:                                               // drowned
    if player and respawn timer idle: respawn timer = 80 ticks; message 2018 "Drowned!"
    state = 6 (removed)
```

- From a map-0 river surface (z 24000) you drown in (27000−24000)/24 = **125 ticks, 5 s**.
- Climbing out is only possible while the bank top is within the 64 h-unit step of your z (§5.1). In practice you
  cannot climb back out of the rivers (I).
- Footsteps in water play splash sound 0x41 (Update 3 "splashing sound when you walk through a pond").

### 4.5 Health, damage, death, respawn (C)

**Health.**

- Health is at +0x34a (68k) / +0x34c (OS4). Everyone starts at **100** (constructor).
- The player regains **+1 every 250 ticks** (10 s at 25 Hz) while health is 1..99. This is `DAT_0034c410`
  at the tail of `FUN_000e3a58`. While driving, the regen applies to the stored on-foot health, `DAT_0034c1a0[p]`
  (68k) / `DAT_012a6304[p]` (OS4).

**Damage sources:**

- **Impacts** with vehicles or other objects (`FUN_01056c58` / `FUN_000de0b6`):
  - `base = ((dvx²+dvy²+(dvz/2)²) >> 7)·min(m_self, m_other)/m_self`
  - The player on foot takes base>>3; NPC people take base>>1.
  - More than 10 damage plays scream 0x25–0x27 and blood effects 8/9.
  - Death from this shows "Flattened!" (2021).
  - If the killing car is the one this ped was pulled out of (car id `+0x498` == ped `+0x340`): "Irony bonus!"
    (2027) and +10000 × multiplier.
- **Punch** (fists, model code 0xfe, when the fire cooldown +0x3aa(68k) is 0):
  - The model switches to MaleNormalPunch.
  - The hit point is `pos + (sin h, cos h)·256`.
  - Every other person whose |dz| < 250, |dx| and |dy| < 3·192 = 576 of that point, and with line of sight at head
    height (`FUN_0009217e` at z−0x400) loses **30 health** (scream 0x25..0x27).
  - Sound 0x3b plays on a hit.
  - A crate 512 ahead is opened (`FUN_000d0670`).
- Weapons and fire are in `docs/re/combat_police.md`.

**Death.**

- When health < 1, state becomes **7 (dead)**: the person lies flat with all input ignored.
- For the player, `respawn timer DAT_0034c1c8[p] = 0x50` (**80 ticks = 3.2 s**) and the score is stored in
  `DAT_0034c1d8[p]`.
- Messages (phrase IDs):

| ID | Message | Cause |
|---|---|---|
| 2016 | Wasted! | shot |
| 2017 | Busted! | arrested |
| 2018 | Drowned! | drowned |
| 2019 | Toasted! | burned |
| 2020 | Splattered! | health < −70 → gibbed |
| 2021 | Flattened! | run over |

**Respawn** (main loop: 68k `FUN_000ec686` around line 1700, OS4 `FUN_010692bc` around line 49650):

1. When the timer reaches 0, single player decrements **lives** (`DAT_0034c428` / `DAT_0114c6d0`; **3** at game
   start; scripts can add more).
2. If lives reach 0: "Game over!" (2026), with a 250-tick timer.
3. Otherwise:
   - The old player object, if not dead, gets state 5.
   - A new **MaleNormal** is created.
   - Fields: state 2, frames 0/11, +0x344 = 0, player number, score restored.
   - Health is the new object's 100. The object is zero-filled, so weapons (fists only), ammo and wanted level
     start at 0 (C: 68k `FUN_000ec686` respawn ~127750, `FUN_0003feb2`; the same at level start).
4. Position (`FUN_0106ef6c`):
   - **If a script has set a respawn square** (`FUN_0106ef24(x, y, level)`; −1 = none), use that square's centre.
   - Otherwise pick a random start point from the LoadMap list, avoiding the last 4 used (19 retries).
   - z is the top of the cube at the given level: `(1500 − Σh[0..level])·16`.
5. Multiplayer respawns without lives.

**Busted (police on foot, §7.3).** The player gets stun = 10⁹ (lies down permanently), the respawn timer starts,
and the message is 2017.

---

## 5. Collision

### 5.1 People vs. the city (C: 68k `FUN_00044f82` / `FUN_000446b8` / `FUN_00044378`)

The in-memory cube record is 0x28 bytes in OS4 and 0x26 in 68k. Square stride is x·0x3c00 + y·0x78 (OS4) or
x·0x3900 + y·0x72 (68k).

| Offset | Field |
|---|---|
| +0 | rotation |
| +1 | scenery |
| +2 | **h** = height1·16 |
| +4 | special |
| +5 | flags |
| +6 | **h2** = height2·16 |
| +8 | slope |
| +9 | top face type (low 3 bits, flip = 0x80) |
| +0xa..+0xd | N, E, S, W face types |
| +0x10 | top texture |

Kind-0 cubes have h = h2 = the i16 height and slope 0. Detail is in `docs/formats.md` and LoadMap
`FUN_01079300`.

**Floor function `FUN_00044378(map, obj, sx, sy, x, y)` → (level, floor z).** Walk cubes 0..2 bottom-up with
`top = 1500` (h-units; z = top·16):

```
for c in 0..2:
    h = cube.h; base = top − h                           // top surface of this cube's own height
    if cube.slope != 0 and (cube.topface & 0x7f) != 0:
        Flat (1):  s = base;  if topface == 4 (water): s = top − cube.h2
        Horiz (3): s = base − ((x & 0x7ff)·(h2 − h) >> 11)      // h at west edge → h2 at east edge
        Vert (2):  s = base − ((y & 0x7ff)·(h2 − h) >> 11)      // h at north → h2 at south
        RHoriz(5): s = (top − h2) − ((x & 0x7ff)·(h − h2) >> 11)
        RVert (4): s = (top − h2) − ((y & 0x7ff)·(h − h2) >> 11)
        if |z/16 − s| <= best: best = 64; floor = s·16; level = c   // first valid always taken
    top = base
```

The result is the **lowest** surface, or a higher one if its surface is within **64 h-units** of the person's
current height. That is the step-up height: **64 h-units = 1/4 cube = 1024 z**.

The floor is written to all 9 floor samples (+0x3d6..+0x3f6 68k; centre +0x3e6/+0x3e8). The level goes to
+0x3ce/+0x3d0. `in_water` = (top face of the floor cube & 0x7f) == 4.

**Wall test `FUN_000446b8(obj, map, x0,y0,zh0, x1,y1,zh1)`** walks a segment square by square (zh in h-units).
At each square boundary crossed, it finds which cube level the segment height falls in:

```
level = 0 if zh >= top0 (below top of cube 0, where top0 = 1500 − h0) and
            (cube0 is a solid Flat non-water cube, or zh is below its lower edge top0+h0−h2) …
        (same test for cubes 1, 2; −1 if above all three)
```

The crossing is **blocked** when all of these hold:

- level ≠ −1
- the cube at that level exists
- the face on the side being entered has type ∉ {0, 5}
- the boundary is beyond the square the person started in

The face checked depends on the movement direction: +x checks the W face (+0xd), −x the E face (+0xb), +y the N
face (+0xa), −y the S face (+0xc).

- Faces of type 3 (Alpha/glass) are **broken by projectiles**: the cube is deleted and sound 0x31 plays. People
  are blocked by them.
- Map edges block.

**Per-step resolution for people (`FUN_00044f82`, 0x4909 ∈ {1, 2}):**

```
zh = (z − 0x400) >> 4               // probe 64 h-units above the origin → obstacles lower than this are stepped over
r  = model.0x48c4                    // 192 for every person ("People now have volume", Update 1)
test segments (x−r,y)→(x+r,y), (x+r,y)→(x−r,y), (x,y−r)→(x,y+r), (x,y+r)→(x,y−r) at zh
if blocked in x: x = prev_x ± 0x80   (sign = collision side)
if blocked in y: y = prev_y ± 0x80
loop: test (x,y,zh) → (prev_x,prev_y,(prev_z−0x400)>>4); while blocked, revert the blocked axis to prev
recompute floor/level/in_water at (x,y); ceiling check (§4.3)
clamp x,y to [0,0x3ffff]
prev_{x,y,z} (+0x40a.. 68k / +0x40c.. OS4) = x,y,z
```

The collision flags are returned per axis. The people branch uses them to set the velocity sign away from the
wall (`vx = ±|vx|`). AI peds use the "hit a wall" flag to choose a new direction (§7.2).

**What blocks a person (summary):**

- Any wall face of type 1–4 or 6–7 whose cube spans the probe height, i.e. more than 64 h-units above the feet.
- Glass.
- Map edges.

**What is walkable:**

- Any cube top within 64 h-units of the current height, including slopes, roofs reached by steps, and water
  (where you sink).
- People walk off roofs and fall.

### 5.2 People vs. scenery (C: OS4 `FUN_01045288` "AddCollideObj")

- Scenery on nearby squares is treated as a circle.
- Centre = square centre + the model's mean (X, Y) rotated by the scenery rotation (`rotation·0x5a & 0x7ffc` byte
  offset, i.e. rot·22.5 units).
- Radius R = `scenery.0x4918/2 + person.0x48c4` (= quarter of the smaller footprint extent, at least 64, + 192).
- Only applies when the z ranges overlap within ±500.
- Penetration is resolved by moving the person and its velocity out by **half the overlap** per sub-step.
- **People take no damage from scenery.** Vehicles do, except from the snowman (0x4908 = 0xfe).

Pickups (`0x4908` ∈ weapon IDs, 0x50 info, 0x51 key, 100 radar) are collected by walking over them. That logic
is in combat_police.md.

### 5.3 People vs. objects (people, vehicles) (C: OS4 `FUN_01056c58`)

- Pair test using spheres. A person is one sphere at its origin. A vehicle is a chain of 4 spheres along model Y at
  offsets {0, r−|minY|, maxY−r, 2·maxY/3−r}.
- Contact distance = **(r_a + r_b)·17/16**; for two people that is (192+192)·17/16 = 408.
- dz/2 is included in the distance (|dz/2| < 20000).
- Broad phase is a 128×128 per-square object bitmask (`DAT_012b1bec`). The test is sub-stepped per tick.
- Response: push apart and exchange momentum weighted by mass (+0x48ec). People (90) against cars (≥ 700) are
  thrown.
- Impact damage is the formula in §4.5.
- Touching a vehicle sets OS4 +0x4a4 = 1 on both objects. The walk-to-door logic uses this to re-route around the
  car (§6.1).

---

## 6. Entering and exiting vehicles (C: OS4 `FUN_01052a4c`, `FUN_01052398`, `FUN_01052714`/`FUN_010528b8`; 68k twins in `FUN_000db2e4`, `FUN_000e3a58`)

### 6.1 Getting in

**When the in/out button is pressed** (edge-triggered; latch `DAT_012f5ba0[p]`) while the player is a person in
state 2:

- **If not already heading for a car** (OS4 +0x424 == 0), pick the target:
  - Candidates are all objects except self that are either:
    - a vehicle (`0x4909 == 0`) whose state ≠ 0x14, or
    - a police vehicle in state 0x0f.
  - A candidate must also have +0x33c == 0 (not burning/destroyed) and (+0x33a == 0 or `model.0x492c ≠ 0`,
    the boat).
  - The **driver's-door point** is `D = veh.pos + R(h)·(lateral = −person.0x48c4 − veh.0x48f4, forward =
    veh.0x48f8)`, with lateral measured along model +X.
  - The nearest candidate by squared distance from the player to D is chosen, with **no range limit**: any of the
    ≤ 32 active objects.
  - **Boat** (`0x492c ≠ 0`): board immediately if within `veh.0x48c4/8` and |dz| ≤ 0xbff.
  - **Any other vehicle**: set +0x420 = vehicle, +0x424 = 1 (walking to car), waypoint +0x310 = 5.
- **If already heading for a car** (+0x424 == 1): pressing again cancels (+0x424 = 0).

**Walking to the car** (+0x424 == 1). The person steers automatically like the AI (§7.1). It turns toward the
waypoint and walks forward when the angle error is under 200 units (12.5°). The waypoint positions, relative to
the car (model +X is the car's left, −X its **right**):

| Waypoint | Lateral | Forward |
|---|---|---|
| 0 (door) | veh.0x48f4 (= −maxX: the **right-hand side**) | veh.0x48f8 |
| 1 (front-left) | +0x48c4 | +0x48c8 |
| 2 (front-right) | −0x48c4 | +0x48c8 |
| 3 (rear-left) | +0x48c4 | −0x48cc |
| 4 (rear-right) | −0x48c4 | −0x48cc |
| 5 | "door, re-plan on contact" | |

The door is on the **right** of the car, matching UK left-hand traffic; the map editor guide says "drive on the
left because otherwise the car doors will be on the wrong side".

**Re-planning.** With waypoint 5, the first time the person touches the car (+0x4a4), it computes the squared
distance to each of 0..4 and takes the nearest. This is Update 3's "walk to the other side of the car".

**Arrival.** A waypoint is reached when the distance is < `person.0x48c4/12` in >>5 units (= 512 internal) and
|dz| ≤ 0x7ff. Then:

- waypoint 0 or 5 → +0x424 = 2 (getting in)
- 1 → 2, 3 → 4, 2 or 4 → 0

So the person walks front-left → front-right → door, or rear-left → rear-right → door.

**Getting in** (+0x424 == 2). The door opens: `veh+0x418 += 100·ticks` up to **0x370 (880)**, about 9 ticks.
When it is fully open and the player's +0x33c ≤ 0:

1. The player becomes the vehicle: `DAT_012a62f4[p] = vehicle index`.
2. The vehicle's name phrase (`model+0x493c`) is shown.
3. **Hijack**, if the vehicle has an AI driver (+0x344 ≠ 0):
   - Wanted level goes to at least **3** for a police car (`0x490a`), otherwise at least **2**.
   - **If the vehicle is in state 5** (stopped and empty), no driver is ejected. Score += 1500 (police) or 1000,
     times the multiplier `DAT_0114c74c`.
   - **Otherwise the driver is thrown out** by `FUN_01052714`:
     - Model PoliceMale1 (police car: weapon +0x360 = 5 shotgun, ammo +0x364 = 100) or **PedMale1** (any other car).
     - Stun +0x494 = **50 ticks** lying down.
     - Scream sound 0x25 + rand(3).
     - State 3 (wanders off; never returns to the car).
     - Score += 1000/1500 × multiplier.
   - Buses (`model+0x4930 ≠ 0`) play passenger scream 0x4c.
   - The player's carjack counter +0x460 increments.
   - **Multiplayer, another player driving** (`DAT_0114c7c8 ≠ 0` and vehicle state 2; OS4 `FUN_01052a4c`
     ~0x010536f0): no wanted level, score or carjack count. The victim is put out by the normal exit
     `FUN_01052398` (§6.3: on foot at the door with his weapons, score and stored on-foot health) and
     knocked down, +0x494 = 50 ticks; his remote-controlled RC-car body, if any, is killed (health −71).
     He keeps playing on foot. (If the exit fails, a boat over water on both sides, the hijack still
     goes ahead.) Port: `boarding::walk_to_door` → `boarding::eject`; debug `--multiplayer 2 --map 0
     --hijack-test`.
4. **If not a hijack**, the vehicle inherits the person's wanted level (+0x330) and score (+0x384).
5. The person's data is copied to the vehicle:
   - player number +0x4ac
   - weapons and ammo (0x24 bytes from +0x360)
   - stats +0x45c..+0x474
   - on-foot health stored in `DAT_012a6304[p]`
6. Vehicle state = 2, +0x344 = 0, +0x426 = 1.
7. The person object is removed (`FUN_01047824`).

**Door closing.** While the vehicle is player-driven the door shuts at `0x418 −= 50·ticks`, about 18 ticks. It
plays sound 0x28 (door slam) when it reaches 0 (68k `FUN_000e3a58`, around line 2860).

### 6.2 Door models (`Objects/Doors/*.Obj`) (C: OS4 `FUN_010c101c` tail)

When `veh+0x418 ≠ 0`, the renderer creates a temporary instance of `veh.model+0x48fc` (the door model):

- **Hinge**: vehicle-local (0x48f4, 0x48f8), i.e. the right side at the door's longitudinal position.
- **Heading**: vehicle heading + 0x418, so the door swings open by up to 880 units = **55°**.
- Same pitch, roll and z as the vehicle.

Door models are thin panels hinged at y = 0 that extend backwards to y = −38 (bus: −24), z ±18.

Door model per vehicle (68k loader, field `0x48fc`):

| Door | Vehicles |
|---|---|
| default | mundaneo, pug, pug GTI, pug racer, diblo, vapour, vapour racer, fjord, van, limo, scooby, evo, pickup, hotrod, helicopter, RC car, rocketcar* |
| police | police (+red/blue), policevan (+red/blue) |
| bus | bus, icecreamvan, truck, bombtruck |
| taxi | taxi |
| none (field holds the petrol projectile model) | tank, boat |

The longitudinal door position overrides the default `0x48f8` (default = maxY/2):

| 0x48f8 | Vehicles |
|---|---|
| maxY·15/16 | bus, truck |
| maxY·7/8 | bombtruck |
| maxY/3 | vapour, vapour racer |
| maxY·2/3 | limo |
| maxY·3/4 | van, policevan* |
| maxY·5/6 | icecreamvan, pickup |
| maxY/6 | rocketcar* |

### 6.3 Getting out (C: OS4 `FUN_01052398`)

Press in/out while driving:

- **Helicopter** (`0x492a ≠ 0`): the use key cycles engine on (+0x49c) → engine off and exit armed (+0x49e)
  → get out; the full sequence is C, see §9 (Helicopter exit).
- **Every other vehicle** exits immediately, **even while moving** (no speed check found).

The new person:

```
L    = model[w].0x48c4 − veh.0x48f4          // 192 + vehicle half-width
pos  = veh.pos + (−cos h, sin h)·L·6/1280 + (sin h, cos h)·veh.0x48f8 >> 8
       // i.e. 1.2×(radius + half-width) out of the RIGHT side, at the door's longitudinal position
heading = veh.heading − 0x5a0 (facing away from the car, to its right); pitch 0x5a0
model = DAT_012a6600[weapon]
```

The person gets these fields:

| Field | Value |
|---|---|
| prev_pos (+0x40c, +0x410) | **vehicle centre** (non-boats) |
| z, level, 9 floor samples | vehicle's |
| state | 2 |
| frames | 0/11 |
| weapons, wanted level, score, stats | copied back from the vehicle |
| health | stored on-foot health `DAT_012a6304[p]` |

The vehicle gets state 5 (parked/empty), +0x426 = 0, and door +0x418 = 0x370 (fully open, then it closes).

Because prev_pos is the car centre, the first collision step tests the segment car centre → exit point. If a
wall is in between, the person is pulled back toward the car. This is the Update 4 fix "could get out on top of
a building" (I).

**Boat** (`0x492c ≠ 0`). If the normal exit point is on water (top face 4):

1. Try the **left** side at 1.6·L (`L·sin/160`), facing heading + 180°.
2. Otherwise try the right side at 1.6·L.
3. If both are water, the player cannot get out (returns 0).

Boat exits keep prev_pos = the exit point.

Exiting a moving car gives the person no extra velocity: speed starts at 0 (C: the exit code writes no
velocity, 68k `FUN_000dab40` / OS4 `FUN_01052398`, and the new person is zero-filled).

Separately, ejected drivers and peds spawned by `FUN_01052714` start with frames 0/11 and stand at
`1.2·(person.r − veh.0x48f4)` on alternating sides (`DAT_0114c628` toggles each call; toggled value 0 =
the right/door side, 1 = the left). The toggle is set to 1 by the hijack code right before its call
(so hijacked drivers always land on the door side), by the cop drop-off, at level start, and by the
main loop right before every frame's `Trigger_Update` (OS4 ~49717 `*local_54 = 1`; 68k ~127806
`DAT_0034c1b0 = 1`). So the first scripted alternating exit of a frame (`Ped_ExitVehicle`
`0xb43d4`/`0xb4414` → `0xdb03e`) is on the **driver's door side**, the second on the other side.
(CONFIRMED)

---

## 7. Pedestrian AI

Summary of the sub-investigation; details in `analysis/people_ai/`. All from 68k `FUN_000e3a58` unless noted.

### 7.1 States and virtual stick

People use the same steer/throttle/fire virtual stick as the player (§4.1). The state byte is at 68k +0x426 /
OS4 +0x428.

| State | Meaning |
|---|---|
| 2 | player-controlled |
| 3 | wandering pedestrian |
| 4 | idle / standing (constructor default) |
| 5 | parked or abandoned vehicle |
| 6 | removed this frame |
| 7 | dead |
| 0xc | barrier |
| 0xd | board the player's car (follower) |
| 0xe | walk to a goal (+0x2e8, +0x2ec) and vanish within \|dx\|+\|dy\| < 0x579 |
| 0xf | walk to a goal and stop within +0x448 |
| 0x10, 0x11 | scripted attack (fire when target is within 6000 and `rand(0x65 − aggression) < ticks`) |
| 0x12 | fleeing |
| 0x13 | board vehicle +0x41e (walk to door waypoints §6.1; reaching the door → state 6, inside) |
| 8–11, 0x14, 0x15 | follow the waypoint graph (`FUN_000a42ac`/`FUN_000a43f2`) |

Common AI steering, for any target heading T:

- If the signed angle to T is under 200 units (0xc8; 0xa0 on one branch), throttle forward.
- Otherwise steer toward T.
- Several states release the throttle above a speed cap so the person ambles: 0x30 for police and states 0xe,
  0x10, 0x11; 0x20 for state 0xf; 0x18 for flee.

### 7.2 Spawning and despawning (C: OS4 `FUN_010485cc` / `FUN_01047af4`; 68k `FUN_000d0cfc` / `FUN_000cf5cc`)

**Cap.** The spawner runs only while `lastIdx < min(nPlacedVehicles + 11, 20)`. Cars and peds share this budget.
It makes `2·ticks` attempts per frame.

**Where.** The edge of the view window:

- Camera square is (cx, cy) and camera height camH.
- `R = (camH + 1500) >> 8`; half-width `H = R + 5` squares.
- The edge is in the player's direction of motion: the dominant velocity axis, or a random one of N/E/S when the
  player is still.
- Each cube level 0..2 is tried at a random square along that edge.
- Spawns therefore appear exactly on the despawn boundary, off screen.

**Square acceptance:**

- Index 1..126, cube present, top face not empty.
- No object in the square or its 4 neighbours (occupancy grid).
- Options are the matching road-direction bits (cars) plus one option for **flag 0x20** (peds). Pick one at
  random.
- The ped option then succeeds with **1-in-5**.

**Ped model.** `DAT_012a6600[6 + rand(4)]` with index 7 (police) skipped, giving **PedMale1, 2, 3 or 4 at 25%
each**.

- PoliceMale1, PedMale5/7 (shotgun) and PedMale6 (KKK) **never come from the random spawner**.
- They come from scripts, roadblocks (wanted > 4, flag 0x40 squares: police cars, barriers and a PoliceMale1 per
  barrier with a shotgun and 100 ammo), police leaving cars, and hijacked police cars.
- No KKK-specific behaviour was found.

**Initial ped fields:**

- position = square centre
- heading = rand(0x167f), pitch = 0x5a0, state 3
- level = c
- z = cube top − 0x200 (drops into place)
- aggression +0x348 = rand(100) (constructor)

**Despawn** (OS4 `FUN_010542b8`, every frame):

- Condition: state ∈ {3, 5, 7, 10, 11, 12}, no script tag (+0x480 == −1), and the square is outside every
  camera's ±(R+5) window.
- State 6 is removed at once.
- Dead bodies stay until off screen (no timer).

### 7.3 Walking, panic, police

**Wander (state 3).** The target heading T is at +0x2ee (68k) / +0x2f0 (OS4).

```
if T < 0: T −= ticks                    // count straight-walking ticks
if T < −50 and (hit a wall this step or !(square.flags & 0x20 at current level)):
    T = heading + 0x5a0 + rand(0xb40)   // turn 90°..270°
if T >= 0: S = ((T + 0x168)/0x5a0)·0x5a0 (snap to the 90° grid); T → S by 16·ticks;
           steer toward T; when |T − heading| < 200: T = −1
if T < 0 and speed < 0x30: throttle forward
```

- Peds therefore walk straight for at least 50 ticks, and turn when they leave a 0x20 square or bump into a wall.
  Corners keep them on the pavements loosely, not strictly.
- Typical wandering speed oscillates around 40–64, average about **52 units/tick** (0.63 squares/s at 25 Hz).
- Peds do not avoid cars or other people; only the pair collision of §5.3 applies.

**Panic** (OS4 `FUN_010435f4` when the player fires; `FUN_01043788` for an explosion at a point):

- Affects every non-police person in state 3 within a 3D distance of **10000** (about 4.9 squares).
- Each one screams (0x25–0x27) and gets state **0x12**, flee timer +0x4a8 = **100 ticks**, and target = player.
- Fleeing steers **away** from the target with the throttle released above speed 0x18, so it shuffles at 24–48.
- When the timer ends: state 3.
- Hitting a ped (`FUN_01043924`) has the same effect and raises the wanted level.

**Police on foot** (model `0x490a == 1`) act only when the player's wanted level (+0x330) is **≥ 3** and neither
side is stunned or dead.

- The cop heads for the player's position, or the car's door point when the player is driving. The lead is
  `−3·(5 − difficulty)·player velocity`.
- **Arrest**:
  - Distance condition:
    - Player on foot: within `(r_cop + r_player)·3/32` in >>4 units, i.e. 576 internal.
    - Player in a car: within `r_cop/8` of the door point.
  - Also needs |player speed| < 30 and the same level.
  - Player on foot: **Busted** at once.
  - Player in a car: the cop opens the door (+0x418 += 100·ticks up to 0x370), then drags the player out
    (`FUN_000dab40`) and busts him.
- **Shooting** (shotgun, 2 pellets, ±0x50 spread):
  - Fires when `rand(N) < ticks`.
  - N = 10·(9 − wanted) when difficulty < 4, otherwise 10·(7 − wanted).
  - Only active when wanted > 4 (difficulty < 2) or wanted > 3 (otherwise).
- Police cars leave their car (spawning a PoliceMale1) when within |dx|+|dy| < 8000 of the player and both are
  slow, or the car's health is ≤ 49.

Armed civilians (PedMale5/7) only shoot when a script puts them in state 0x10/0x11. Shotgun firing details are
in combat_police.md.

### 7.4 Ped death

- Impact damage is in §4.5.
- Health < −70 → gibbed (`FUN_0104465c`): debris, splat sound 0xc, state 6.
- Dead peds bleed (blood pool particle type 8, size 120 + rand(120), when `rand(6) ≤ ticks`).
- Burning: 100-tick burn timer (+0x33c), then state 7.
- Drowning: §4.4.

---

## 8. Instance fields used by people

The two builds differ in alignment. In 68k, ints between 0x2e6 and 0x41e sit 2 bytes earlier than in OS4.

| 68k | OS4 | Field |
|---|---|---|
| +0x00/04/08 | same | x, y, z |
| +0x0c/10/14 | same | roll, pitch (0x5a0 standing, 0 lying), heading |
| +0x18/1c/20 | same | motion frame N, frame P, blend |
| +0x24 | same | model pointer (swapped for weapon/fire/punch poses) |
| +0x2fa/2fe/302 | +0x2fc/300/304 | vx, vy, vz (per tick) |
| +0x2ee | +0x2f0 | wander target heading of state-3 peds (C): starts as the spawn heading, snapped to the nearest 90°, turned toward at ±16/tick. Negative = countdown; past −50 (blocked, or off squares with flag 0x20) it picks heading + 90° + rand(180°). Scripts set −1 (68k `FUN_000e3a58` ~122305–122345, `FUN_000b42b8`). |
| +0x30e | +0x310 | door waypoint 0..5 |
| +0x326 | +0x328 | turn rate (±100) |
| +0x32e | +0x330 | wanted level |
| +0x336 | +0x338 | on ground |
| +0x338 | +0x33a | in water |
| +0x33a | +0x33c | burn timer (>0 blocks boarding) |
| +0x342 | +0x344 | AI driver / owner flag (vehicles) |
| +0x34a | +0x34c | **health** (100) |
| +0x346 | +0x348 | aggression rand(100) |
| +0x35e.. | +0x360.. | weapon, then ammo counts (0x24 bytes) |
| +0x382 | +0x384 | score |
| +0x386 | +0x388 | AI max speed |
| +0x3aa | +0x3ac | fire cooldown |
| +0x3ae | +0x3b0 | forward speed |
| +0x3ce | +0x3d0 | cube level 0..2 |
| +0x3e6 | +0x3e8 | floor z (centre of 9 samples) |
| +0x40a/40e/412 | +0x40c/410/414 | previous valid x, y, z |
| +0x416 | +0x418 | (vehicle) door open 0..0x370 |
| +0x41e | +0x420 | target object |
| +0x422 | +0x424 | walk-to-car / alerted (0 no, 1 walking, 2 entering) |
| +0x426 | +0x428 | state |
| +0x44a | +0x44c | splash played |
| +0x490 | +0x494 | stun timer (lying while > 0) |
| +0x4a2 | +0x4a8 | flee timer |
| +0x4a6 | +0x4ac | player number |
| — | +0x340 / +0x498 | origin-vehicle id / own unique id |

---

## 9. Open questions

- The feet-below-floor offset (§2.3) should be confirmed against a screenshot.
- Whether you can climb out of the rivers anywhere (§4.4): the step limit is confirmed; a map scan for banks
  within 64 h-units of the water surface is still needed.

Resolved:

- ~~Helicopter exit~~ (C: 68k `FUN_000db2e4` ~116985–117008, OS4 `FUN_01052a4c` ~43335–43348). OS4 +0x49c =
  engine on (68k +0x498), OS4 +0x49e = exit armed (68k +0x49a). The use key: engine on → engine off and armed;
  engine off and not armed → engine on (takes off); engine off and armed → get out. There is no speed or height
  check for any vehicle, so a mid-air bail-out is possible (with no fall damage). Not the same fields as the
  68k +0x49c/+0x49e of script_api.md (controls enabled, racing-line flag).
- ~~Atan helper~~ (C) `FUN_00094b44(u = x − tx, v = y − ty)` returns the heading from the object to the target,
  0..0x167f, measured from +y toward +x (sin ↔ x, cos ↔ y); a zero argument is replaced by 1.0; table
  `FUN_0003fd32`. OS4 `FUN_010bcc18`.
- ~~Zero-initialised instances~~ (C) 68k `FUN_0003feb2` = malloc + memset 0, OS4 `FUN_0103a73c` = calloc. A
  fresh ped's first wander turn does not depend on it: +0x2ee starts as the spawn heading.
- ~~Fall damage~~ (C) none: no health writer depends on vz. Explosions throw people via vz (combat doc).
