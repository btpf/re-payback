# Effects, explosions, damage, collide objects (working notes)

Sources: OS4 `analysis/ghidra_os4/decompiled.c` (primary), 68k `analysis/ghidra_68k/decompiled.c`.
Tags: **C** = CONFIRMED (read in code), **I** = INFERRED.
`ticks` = `DAT_0114c6a0` (OS4) = game ticks elapsed this frame; every per-tick rate below is multiplied by it.
`rand(n)` = `FUN_0103acc4(n)` / `FUN_0103ac94(n)` (0..n-1). Object = entry of `DAT_012a6270[32]` (OS4) / `DAT_0030c318[]` (68k).
68k object offsets = OS4 offset − 2 for fields past ~0x300 (C for 0x33a/0x33c, 0x3ce/0x3d0, 0x426/0x428).

## 0. Correction: FUN_000170ce is PlaySound, not AddEffect (C)

68k `FUN_000170ce` = OS4 `FUN_0100815c` = **PlaySound(id, loop, volume, rateHz, x, y, z)**: 500-slot list,
error "AddSound(): Too many sounds". x,y,z stored >>4; z = −1 → non-positional. Returns slot or −2.
Sound ids = load order in OS4 `FUN_01005864` (0 Explosion, 5 Thud, 10 Heartbeat, 12 Squelch, 13 Flames,
15 LongAaah, 18 GrenadeBounce, 22 GarageDoor, 29 Ricochet, 30 HitScenery, 37–39 Ow1–3, 43 Splash,
62 Lightning, 75 Thud2, 76 Screams, …).
The actual effect spawner is **AddEffect**: OS4 `FUN_01035da0`, 68k `FUN_0003d97a`.

## 1. Effect (particle/sprite) system

### 1.1 Lists (C)
- 4 lists, indexed by **layer** (0..3), not by view. Layer = object field `+0x3d0` (OS4; 68k `+0x3ce`) + 1,
  i.e. the cube storey the effect is in, so sprites interleave with cube layers when rendering. Layer 3
  is the top layer (weather, explosion fireballs from ignite/crates use 3).
- Max **5000** per list. Count `DAT_0114a934[4]` (OS4) / `DAT_00117d70[4]` (68k); −1 = empty.
- Storage interleaved: OS4 `DAT_0118ff2c + idx*0xb0 + layer*0x2c` (0x2c-byte record);
  68k `DAT_0014d760 + idx*0xa8 + layer*0x2a` (0x2a-byte packed record).
- Remove = copy last entry over it, count−1 (`FUN_01035f48` / 68k `FUN_00039650`, "RemoveEffect: Failed…").
  Stops the looping sound if `sound >= 0`. Update loops run from last to first.

| OS4 off | 68k off | field |
|---|---|---|
| 0x00 u16 | 0x00 | type (0..15) |
| 0x04 i32 | 0x02 | blend: alpha-table offset (0, 0x10000, 0x30000), see 1.4 |
| 0x08 ptr | 0x06 | texture (64×64 effect texture ptr); type 10: shockwave frame 0..3 |
| 0x0c/10/14 | 0x0a/0e/12 | x, y, z (same units as objects: x,y 1/2048 square; z down-positive) |
| 0x18/1c/20 | 0x16/1a/1e | vx, vy, vz (**stored = argument / 3**) |
| 0x24 | 0x22 | size = sprite half-extent in world units, also the life (removed at <= 0) |
| 0x28 | 0x26 | sound slot, −1 none |

### 1.2 Spawners (C)
- `AddEffect(type, tex, x, y, z, vx, vy, vz, size, layer, blend)`: OS4 `FUN_01035da0`, 68k `FUN_0003d97a`.
  Returns index or −1 when full. **type 15 also starts PlaySound(13 Flames, loop=1, 1500000, 19000 Hz, x,y,z)**
  and stores the slot.
- `FUN_01034468` = the same without the sound or return value (used inside the updaters).
- `FUN_01033c0c(tex,x,y,z,vx,vy,vz,size,layer,blend)` = always **type 6**.
- Script hook `FUN_010d13f4(x,y,z,vx,vy,vz,size,layer)` = AddEffect(9, Gas, …, blend 0x30000).
  Script texture getters: `FUN_01041058` → Gas, `FUN_0104104c` → Dust.

### 1.3 Textures (C, OS4 `FUN_0104feb0`, `FUN_010bdab0`, `FUN_01035978`; 68k init `FUN_000393d4`)
Loader `FUN_0103500c(name)` (68k `FUN_00038e38`/`FUN_0003df9e`): reads `PROGDIR:effecttextures/<name>`,
0x1000 bytes (64×64 8bpp) + 4-byte hardware handle → 0x1004 alloc.

| global | file | note |
|---|---|---|
| `DAT_0114c790` | Dust | normal (also `DAT_0114c438`, init p7) |
| `DAT_0114c78c` | Dust | loaded with `FUN_01002478(1)` (HW alpha variant); `DAT_0114c434` (init p8) — "Dust2", the smoke texture |
| `DAT_0114c784` | Flare | bullet-hit sparks |
| `DAT_012a6740..674c` | Flame.1..4 | animated; also `DAT_01282fc0..fcc` |
| `DAT_0114c780` | Bubble | **loaded but never used** in OS4 (only freed) |
| `DAT_0114c77c` | Blood | `FUN_01002490(1)` variant; also `DAT_0114c430` (init p9) |
| `DAT_0114c788` | Gas | `FUN_0100249c(1)`; script effects only |
| `DAT_0114c8f0` | Shad | object shadows (`FUN_01036044`, blend 0x10000) |
| `DAT_0114c8ec` | DirtSkidmark | skid marks on surface 0x22 |
| `DAT_0114c8e8` | Dirt | wheel dirt spray (`FUN_010cc288`) |
| `DAT_01282fd0..300c` | Shockwave.{1-4}{0,1}{0,1} | see 1.6 |
| `DAT_012679f4..01267a3c` | LensFlare.* | see 1.7 |

Gore option (I, callers in options code `FUN_0106ecf0`): `FUN_01035250` reloads **Blood.dark** into the Blood
texture buffer, `FUN_01035344` reloads **Blood** (68k `FUN_0003dbec` / `FUN_0003dc7e`).
`HealthBar`, `Shad`, `FadeDarken*` are effect textures used elsewhere (HUD/shadows), not by this list.

### 1.4 Blend modes (C mechanism, I meaning)
During drawing, `DAT_0114c440` (base = alpha-table pointer `DAT_0114c43c`, the `General/Alpha.256` blob of
**11 × 64 KiB** tables, built by `FUN_0103b540` if missing) is temporarily increased by the effect's blend field,
so blend selects table `blend >> 16`. Fitting the shipped Alpha.256 against the palette:
table 0 ≈ **additive** (clamped src+dst; default for fire/sparks/flares), table 1 ≈ **subtractive**
(dst − src: dark smoke, scorch marks, shadows), table 3 = non-linear (blood; ~0.6·a+0.9·b, likely tint),
table 4 = 50% average. In Warp3D mode, blend values with `(b & ~0x20000) == 0x10000` (1 and 3) switch to
SRC_ALPHA/ONE_MINUS_SRC_ALPHA, everything else draws ONE/ONE (additive). (C for the Warp3D test.)

### 1.5 Per-type update (C: OS4 single-view `FUN_010386e8`, split-screen `FUN_01036a0c`, simplified `FUN_010382e8`;
68k `FUN_0003b3c2`, `FUN_0003997c`, `FUN_0003af2c` — all called from renderer OS4 `FUN_01049374` / 68k `FUN_000d1058`)

Every tick: `pos += vel * ticks`. Removal (except type 5): `z > 24000` or `size < 1`.
Effects whose x,y fall outside the view rectangle (cam ± half-screen projected, ±0x800, in `>>9` units) are
**deleted immediately** (except weather types 12–14 which wrap around the view).
Flame animation: if the texture is Flame.N it advances to Flame.N+1 (4→1) **every update** (types 1,2,3,4,7 in
FUN_010386e8; all types in FUN_01036a0c).

| type | size Δ per tick | other per-tick behaviour | typical spawn (texture, size, blend) — meaning |
|---|---|---|---|
| 0 | 0 | none | unused |
| 1 | −20 | — | flame/smoke puff: petrol/flamethrower hits (Flame.N, 800 or 250), wheel dirt (Dirt), smoke from type 4/7 (Dust2) |
| 2 | −3 | vz += 17; if rand(5)<ticks spawn type 4 (rand Flame.N, ±50 pos, v=rand(20)−10 each, size 400) | **burning debris** thrown by explosions |
| 3 | −57 | — | dust cloud (Dust, 1500+rand1000): crate break, cube/wall destroyed |
| 4 | −23 | if 100<=size<=250 and rand(2)<ticks: spawn type 1 (Dust2, pos±100, v=self+rand(20)−10, size=size+50+rand(200)) | **flame** that turns into smoke; burning vehicles emit it (vz −600) |
| 5 | 0 | never size-removed; removed when z <= 23000 (rose to surface) or off-screen | I: underwater bubble (never spawned in OS4) |
| 6 | −13 | — | **dark smoke** (Dust2, blend 0x10000): damaged vehicles, exhaust (`FUN_0105e02c`), chimneys (`FUN_01049374`), ignite |
| 7 | −66 | same smoke emission rule as type 4 but new size 350+rand(150) | **fireball** of an explosion (Flame.N) |
| 8 | −1 | — | long-lived ground decal: blood pools (Blood, 0x30000), explosion scorch (Dust2, 0x10000, 1000–6000), skid marks, snow marks |
| 9 | −7 | — | spray particle: bullet sparks (Flare), blood spray (Blood, 0x30000), script Gas |
| 10 | **+300** | frame = size/2500 (0..3); size>9999 → size=0 (removed) | **shockwave** ring |
| 11 | −5 | vz += 17; drawn as a lens flare (AddFlare at sprite centre, −size) instead of a sprite | explosion **flash/flare** |
| 12 | 0 | wraps in view; at ground: type 8 Dust mark (size 200+rand50, z 0x57e4) and respawn a new flake | **snow** |
| 13 | 0 | wraps; at ground: respawn drop + 3× type 14 splash | **rain** (size 1) |
| 14 | 0 | vz += 17; wraps | rain splash (z 0x5780, v ±50/3, vz (−200−rand200)/3) |
| 15 | −2 | if rand(5)<ticks: spawn type 4 (rand Flame.N, ±50, v=(rand20−10, rand20−10, rand(100)−650), size=self) ; looping Flames sound | **fire source** (only via AddEffect; no fixed caller found) |

Weather (C, `FUN_010692bc` initial spawn, FUN_010386e8 respawn): layer 3; snow flake: random in a
0x10000×0xa000 window around camera(`>>9` units), z = 0x100 − camZ·16, v = ((rand20+10)/3, (rand20−10)/3,
(rand100+200)/3), size 100+rand50. Rain: 0x8000×0x5000 window, v = ((rand30+50)/3, (rand20−10)/3,
(rand200+1000)/3), size 1.

Coloured dynamic lights from effects (C, OS4 `FUN_01036574`, 68k `FUN_000396fa`, only if light option
`DAT_0114c2fc`): AddLight(x,y,z,R,G,B) via `FUN_010729c8`:
type 2: (size·140000+rand5M, size·120000+rand4M, size·100000+rand3M); type 7: (size·50000+rand2M,
size·40000+rand1.5M, size·30000+rand1M); type 11: (size·900000, size·900000, size·700000);
type 15: (size·100000+rand5M, size·70000+rand3M, size·50000+rand2M).

Projection (C): software: depth = z + camZ·16 − 256, must be > 256; screen half-size =
size·scale/(depth·16) (`DAT_0114c328`/`DAT_0114c324` = x/y projection scale, centre `DAT_0114c330/32c`).

### 1.6 Shockwave textures (C)
`Shockwave.F A B`: F = frame 1..4 (size/2500), **A = x half (0 left, 1 right), B = y half (0 top, 1 bottom)**:
the 128×128 ring image is split into four 64×64 quadrants and drawn as 4 sprites meeting at the centre
(`.F00` top-left, `.F01` bottom-left, `.F10` top-right, `.F11` bottom-right). Additive.

### 1.7 Lens flares (C, OS4 `FUN_0103451c` AddFlare, `FUN_01034804` draw, `FUN_010345ec` quad helper;
68k AddFlare `FUN_0003da7e`, draw `FUN_00038696`)
- List of **199** entries (`DAT_0114c42c` < 199, else "AddFlare(): Too many flares"), 16 bytes: screen x, y,
  radius = size>>4, blend (0x30000 → 0). Cleared after each frame's draw.
- Occlusion (software): AddFlare writes 0 into the framebuffer pixel at (x,y); at draw time if that pixel is
  non-zero (something was drawn over it) radius = 0. Negative size = skip occlusion test (type 11 uses −size).
- Option `DAT_0114c428`: 0 off, 1 glow only, 2 full (default 2; set by `FUN_01035960`).
- Draw, c = screen centre, p = flare pos, r = radius, sx/sy = projection scale:
  glow group 0 (LensFlare.200/201/210/211, quadrants as 1.6) at p, half-size 2r·s/(3·16);
  full mode adds group 1 (LensFlare.11xx) at c+(p−c)·(−2/3), half r·s/(2·16); group 2 (LensFlare.12xx) at
  c−(p−c), half r·s/16; and ghosts: .1 at c+1.5(p−c) half r·s/96; .3_4 at c+0.75(p−c) r·s/192 and
  c+(2/3)(p−c) r·s/112; .5 at c+(p−c)/2 r·s/144; .6_7 at c+(p−c)/3 r·s/640 and c+(p−c)/10 r·s/480;
  .8 at c−(p−c)/10 r·s/64; .9 at c−(p−c)/6 r·s/144; .10 at c−(p−c)/3 r·s/96. All additive.
- Other AddFlare users: lamp/headlight rendering (`FUN_010c101c`, `FUN_010c4fec`, `FUN_010c7218`).

### 1.8 Object fragments ("TankBits"-style debris) (C, OS4 `FUN_01035438`, 68k `FUN_00038f44`)
Separate list: max **500** per layer, count `DAT_0114a944[4]`, records 0x38 bytes at `DAT_01267a40 + idx*0xe0 + layer*0x38`:
`+0 model, +4 face index, +8/0c/10 x,y,z, +14/18/1c vx,vy,vz, +20/24/28 rot angles (0..0x1680 = 360°·16),
+2c/30/34 angular velocity`.
`SpawnFragments(model, layer, x,y,z, vx,vy,vz, ax,ay,az)`: one fragment **per model face** (person models:
min(nfaces, 20) at random ±500 offsets); position = face centroid + pos; v += rand(S)−S/2 with S = 250
(people 100), vz = vz − rand(S) − S; angular v = rand(250)−125.
Update: pos += v·ticks; rot += ω·ticks (wrap 5760); **gravity vz += 50 per update (not ×ticks)**; when
z > 0x57e4 (22500, ground): 1/3 chance bounce (z = 22500, vz = −vz/2), else removed next frame. Trail:
person fragments 1/3 chance per tick type-6 Blood puff; others type 1 Dust2 smoke (size 250+rand100).
On removal: person → blood pool type 8 (size 150+rand200, blend 0x30000); other → 3 Dust2 puffs at ground.
Each fragment is drawn as its single triangle (`FUN_010c43ac`). Off-screen fragments are removed.
"TankBits" (`FUN_010bf310`, 68k `FUN_00095efa`) is a scenery/object texture name lookup — I: the tank's
debris texture; not part of the effect list.

### 1.9 Other visual damage (I)
- `burnt` (object texture, `DAT_0114c91c`) is applied by the object renderer `FUN_010c101c` to faces whose
  slot mode low nibble is 1 (fixed UV 2,2,2,125,125,125) or 2 (own UVs) — exact trigger not traced.
- Skid marks `FUN_010cc010(x0,y0,x1,y1,z,layer,surface,size)`: line of type-8 decals every size·7/8:
  surface 0x22 → DirtSkidmark blend 0x30000; 0x32..0x34 → Dust additive; else Dust2 subtractive.

## 2. Explosions

### 2.1 Explosion(owner, x, y, z, power, layer, srcObj) (C, OS4 `FUN_0104405c`, 68k `FUN_000cd22e`)
```
PlaySound(0 Explosion, 0, 50000000, power<140000000 ? 12000 - power/20000 : 5000, x,y,z+100)
PanicPeds(owner,x,y,z)                       // FUN_01043788, below
for each object o != srcObj (all 32 slots):
  kind = o.model+0x4909                      // 0 vehicle, 1 person, 2 projectile/prop
  if kind==2 and model not in {barrier, cone, dummy, bullet(0x6320)}: skip
  dx,dy,dz = (o.pos - (x,y,z)) >> 8          // 1/8-square units
  d2 = dx²+dy²+dz² ; u = (d2>>4) * sqrt(d2)  // ~ d³/16
  k  = kind==1 ? (u ? 5u : 1) : max(u>>1, 1)
  p  = (power>>4) / mass(o.model+0x48ec)
  o.v(x,y,z) += clamp(p*d(x,y,z)/k, ±1000)   // OS4 +0x2fc/0x300/0x304
  if o+0x3cc not in {-1,1,3}: continue        // I: "only vulnerable to weapon class N" (-1 = all)
  dmg = kind==1 ? p/k : (p>>4)/k
  o.health(+0x34c) -= dmg
  scoring: single player, src is player-driven and o not: money(+0x384 of player obj) +=
           min((p)/(k>>5), 6000) * DAT_0114c74c ; FUN_01043924(o,1) (crime/wanted, I)
           split-screen: frag counters DAT_012a6730[]
if power >= -249999: flash DAT_0114c748 += power/250000
scorch = power >= 120000000 ? 6000 : max(power/20000, 1000)
AddEffect(8,  Dust2, x,y,z, 0,0,0, scorch, layer, 0x10000)   // scorch mark
AddEffect(10, 0,     x,y,z, 0,0,0, 300,    layer, 0)         // shockwave
AddEffect(11, Blood, x±200, y±200, 21000, 0,0,0, 1000, layer, 0) // flare (texture unused)
```
No hard radius: falloff ~1/d³ (persons 5× less impulse/damage per distance unit than vehicles get from impulse,
vehicles take 1/16·2 = 1/8 of the person damage formula). Example (I): car (mass 1400) blow-up power
1400·25000: a person 1 square away (d=8) takes ≈151 → dead; 2 squares ≈19.
**Screen flash, not camera shake** (C): `DAT_0114c748` decays ×3/4 per tick (`FUN_010692bc`), and while > 0
the renderer `FUN_0105bd98` brightens the palette (`FUN_0103ad6c`); lightning uses the same variable.
No camera shake found.

### 2.2 PanicPeds (C, OS4 `FUN_01043788`)
People (kind 1, model+0x490a != 1, state 0x428 == 3) with ((Δ>>3)² sum) < 1562500, i.e. within
10000 units (≈4.9 squares): PlaySound(37+rand3 Ow, 0, 5000000, 8000), state = 0x12 (flee), +0x4a8 = 100,
+0x420 = player object.

### 2.3 Ignite / first explosion of a vehicle (C, OS4 `FUN_01044b70`, 68k `FUN_000cda5e`)
Called when a vehicle's health drops to <= 0 (and by missiles hitting objects).
```
o.burnTimer(+0x33c) = 100; o.speed(+0x3b0) = 0; o.state(+0x428) = 5 (burning wreck)
o.vx += rand(100)-50; o.vy += rand(100)-50; o.vz -= 200+rand(300)      // hop
n = (mass>>10)+2; c = clamp(n/2, 2, 6)
c× AddEffect(7, Flame.rand4, pos±800, z+rand300-150, v+rand(500)-250, 2500+rand1500, layer 3)
c× AddEffect(2, Flame.rand4, pos±800, ..., v + rand(30n) - 15n (x,y), vz - rand(50n) - 20n, 800, layer 3)
Explosion(owner, x,y,z+500, mass*25000, layer, self)
```
`FUN_01044f28` (68k `FUN_000cdd02`) is a positional variant (fireballs + debris + type 6 smoke, layer 3).

### 2.4 Final destruction (C, OS4 `FUN_0104465c(obj, power)`, 68k `FUN_000cd6ae`)
Triggered when health < −70 (`FUN_010542b8`). Spawns fragments twice (velocity = obj v/2, angles = obj
rotation), state = 6 (gone), speed 0.
- Person: PlaySound(12 Squelch, 0, 15000000, 18000) + blood pool type 8 (size 500+rand300, 0x30000). (gib)
- Else power = arg or mass/2; n = min((power>>8)+2, 20); fireballs type 7 count clamp(n/2,2,6) size
  n·200+rand500; debris type 2 count (n==20 ? 10 : max(n/2,2)), v ±20n, vz −40n−rand(80n), size 64n+rand200;
  then Explosion(owner, x,y,z+500, power·20000, layer, self) — **chain reactions** happen because this
  explosion damages other vehicles, which then ignite/explode through the same health checks.

### 2.5 Crate break (C, OS4 `FUN_01040ce4(sqx, sqy, cube, z)`)
If the cube's scenery id is 6 (crate): fragments of crate.obj + dust clouds (`FUN_01035730`: type 3, Dust,
count clamp(n/2,2,6), size 1500+rand1000) at the square centre (sq·0x800+0x400), then replace scenery by
the content pickup (special 1→9 bazooka, 2→10 flamethrower, 3→11 grenade, 4→12 minigun, 5→13 shotgun,
6→27 pistol, 7→28 bigbomb, else 0). Triggered by missile/petrol/bullets hitting the square.

## 3. Health / damage

### 3.1 Fields (C)
OS4: health `+0x34c` (68k `+0x34a`), burn/death timer `+0x33c` (68k `+0x33a`), speed `+0x3b0`, state byte
`+0x428` (68k `+0x426`; 2 player-controlled?, 3 walking, 5 burning wreck, 6 destroyed/removed, 7 dead,
0x12 fleeing), weapon-vulnerability `+0x3cc` (I), velocity `+0x2fc/0x300/0x304`.
Model: `+0x48c4` radius (half width), `+0x48c8` front length, `+0x48cc` rear length, `+0x48ec` mass,
`+0x4904` (non-zero: ignored by object collisions, killed by water; I: RC car/helicopter-like special),
`+0x4908` pickup/kind byte, `+0x4909` kind (0 vehicle, 1 person, 2 projectile/prop).
- Every new object starts with health **100** (`FUN_010c0c1c`, `FUN_010bf8bc`). Player's on-foot health is
  kept in `DAT_012a6304[player]` while driving and copied back on exit (`FUN_01052398`).
- `FUN_01040f70(delta)` (I: script AddHealth/pickup) adds to the player's health, floor 0, **no max clamp**.
- Marker −71 (`0xffffffb9`) = force final explosion.
- No health regeneration found (I).

Masses (C, `+0x48ec`): people 90 (some 95/100/120); mundaneo 1400, pug 900, diblo 1400, bus 10000,
vapour 1800, pug GTI 850, fjord 1300, van 2200, limo 3000, police 1600, scooby 1235, tank 40000,
evo 1400, taxi 1380, icecreamvan 2000, pickup/hotrod 1800, helicopter 1500, pug racer 700, truck 12000,
policevan 2500 (red 2000), bombtruck 2500, RC car 5000, boat 10000, rocketcar 2500, vapour racer 1800;
missile 1000, bomb 200, barrier 50, cone 20, dummy 115, bullets/grenade/petrol 1.

### 3.2 Hearts HUD (C, OS4 `FUN_01018518(health, x, y)`; icons loaded in `FUN_010138dc`, 9×10 px)
5 hearts, 10 px apart (20 px in hi-res), starting at x−25 (x−50): heart i (0..4) is `heart.big` if
health/20 >= i else `heart.small`; health <= 0 → all small. So max display = 100 (80+ shows 5 big).

### 3.3 Per-frame vehicle/person health check (C, OS4 `FUN_010542b8`, 68k I `FUN_000dc5ac`)
For each live object (state not 0/6):
- health < −70 → final destruction (2.4) (+ player "wasted" message 0x7e9 vehicle / 0x7e4 person).
- burnTimer < 2:
  - dead person (state 7): rand(6) <= ticks → blood pool type 8 (size 120+rand120).
  - health < 1: vehicle → Ignite (2.3); person → PlaySound(15 LongAaah, 0, 10000000, 6000),
    burnTimer = 100 (then state 7 dead when timer reaches 1); `+0x344 == 1` objects (I: remote missile/
    RC) → Ignite + state 6.
  - 1 <= health <= 50 (kind != 2, not in water): with chance rand(health+5) <= 5·ticks:
    person → small blood drip type 8 (50+rand50); vehicle → **damage smoke** type 6 Dust2 blend 0x10000
    from the rear ((len−300) back along heading), v = v/2 + (30+rand30, rand20−10, −10),
    size = radius + rand300. (Smoke frequency rises as health falls.)
- burnTimer >= 2 (burning): loop Flames sound (vol radius·5000, or ·10000 when timer >= 16), dynamic light,
  rand(3) < ticks → type 4 flame (Flame.rand4) at pos±150, vz −600, size radius+rand150
  (1.5·radius+rand300 while timer >= 16); burnTimer −= ticks; on < 2 → dead/out (player respawn timer
  `DAT_012a65e0 = 0x50`). Burn timer 100 ticks ≈ 2 s at 50 Hz (I: tick rate).

### 3.4 Collision damage (C, OS4 `FUN_01056c58` object–object; `FUN_010cd198` cube walls; `FUN_01045288` scenery)
- Object vs object: per object, dmg = ((dvx²+dvy²+(dvz/2)²) >> 7) · min(mA,mB) / mSelf, then
  person: /8 if state 2 (player) else /2; vehicle: /32. Applied only if `+0x3cc == −1`.
  Vehicle-vs-vehicle: Thud (5) if dmg < 11, else Thud2 (75) + "Screams" (76) jingle if the player's
  model has `+0x4930`.
- **Run-over**: a person takes that damage only if the vehicle's |speed(+0x3b0)| > **50** (> **100** if the
  person is the player, state 2). Person dmg > 10 → Ow (37+rand3). Health < 1 → state 7 (dead).
  Blood: type 8 pool (Blood, size 16·min(dmg,50)+50+rand50, 0x30000) + 3× type 9 spray
  (size 8·dmg+80+rand(2·dmg), v = −v_victim/4 + rand80−40). Score: min((vmax·m/m)²>>9, 6000)·mult.
- Cube wall hit (per axis): dmg = v² >> 14, bounce v = −|v|/2, push 32 units out. Same thud sounds.
- Scenery hit (non-person, non-projectile): dmg = (dvx²+dvy²) >> 12.
- Ground impact (`FUN_0105b9ec`): vz > 160 → HitScenery sound + bounce vz = 128 − |vz|/4; **no fall damage found**.
- Water (`+0x33a` set): Splash sound (43, vol mass·8000/20000); objects with model `+0x4904` get health −71
  (destroyed). **No drowning damage for people found** (I: handled elsewhere / respawn).

## 4. Collide objects / hit detection

### 4.1 Grid (C, OS4 `FUN_01045288` AddCollideObj, `FUN_01046778` RemoveCollideObj,
`FUN_01047848` RemoveAllCollideObj; 68k `FUN_000cdfdc`, `FUN_000cefa0`, `FUN_000cf462`)
- Max **32** dynamic objects (`DAT_012a6270[32]`); object i owns bit `DAT_012b1b6c[i] = 1<<i`.
- Grid `DAT_012b1bec`: u32 masks [128 x][128 y][3 layers], offset x·0x600 + y·0xc + layer·4.
- AddCollideObj(i): error "Called twice" if `obj+0x48c (x1) >= 0`; skipped for model `DAT_012a6320`
  (light bullet). Footprint AABB: person = rotated square of radius r; vehicle = rotated box
  [−rear, +front] × [−r, +r]; converted to squares (>>11), clamped 0..127, +1 margin if `+0x4904`;
  stored at `+0x484..0x490` (x0,y0,x1,y1); layer = min(obj+0x3d0, 2). OR the bit into each cell.
  While walking cells it also resolves collisions with the cube's **scenery** (cube byte+1 = scenery id,
  model `DAT_012a61e0[id−1]`, radius from scenery model `+0x4910..0x4918`) — push-back, damage (3.4),
  crate break (2.5) for projectiles. Missile adds a light (100M, 90M, 80M) at z−500.
- RemoveCollideObj clears the bit in its AABB.

### 4.2 Pair test (C, OS4 `FUN_01056c58`, 68k I `FUN_000de0b6`/`FUN_000e3a58`)
Object i ORs the masks of its AABB±1 and tests only objects j < i (`DAT_012f5b20[i]` = (1<<i)−1),
skipping state 6 and `+0x4904` models. Shapes: person/projectile = 1 circle; vehicle = 3 circles along
the heading at offsets {r−rear, front−r, 2·front/3 − r} (plus centre). Contact when
dx²+dy²+(dz/2)² < ((rA+rB)·17/16)², with |dz|/2 < 20000; motion is sub-stepped per tick (and finer when
|v·ticks| >= 30). Response: momentum exchange weighted by min mass.

### 4.3 Projectiles are objects (C, models set up in OS4 loader ~line 40919)
Kind 2 objects: missile.obj (mass 1000), petrol.obj (flamethrower, 1), grenade.obj (1), bullet.obj ×2
(`DAT_012a675c`, `DAT_012a6764`), shotgunbullet.obj (1), bomb.obj (200). On contact with object o:
- missile → Ignite-style explosion `FUN_01044b70` on itself, state 6.
- petrol → type 1 flame (Flame.N, 800) on o; **burn dmg = clamp(min(health+71, ticks·3000/mass), 1..)**
  if `o+0x3cc ∈ {−1,2}`; person reaching 0 → LongAaah, burnTimer 100. Score dmg·32·mult.
- bullet (`675c`): dmg **10000/mass** if `+0x3cc ∈ {−1,4}` (person 90 → 111 = one-shot kill);
  bullet (`6764`): **5000/mass** (only `+0x3cc == −1`); shotgun pellet: **20000/mass** if line of sight
  (`FUN_010707f0`) and `+0x3cc ∈ {−1,5}`. Projectile then state 6. Vehicles hit by bullets: Ricochet (29)
  + 2 type-9 Flare sparks; persons: Ow sound.
- grenade/bomb → GrenadeBounce (18), bounce. barrier/cone/dummy → HitScenery (30), push.
- Projectile vs scenery/cube (in AddCollideObj): bullets/missile/petrol call crate break and spawn sparks.

## 5. Open
- Exact meaning of `+0x3cc` and `+0x344`; `+0x4904` model flag.
- `burnt` texture trigger; Bubble/type 5 users (maybe mission scripts); drowning of people.
- 68k addresses for crate break and the hearts HUD not mapped.
<!-- DONE -->
