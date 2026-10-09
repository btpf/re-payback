# Weapons & pickups (working notes)

Builds: 68k = Update 7 flat image (`analysis/ghidra_68k`), OS4 = `analysis/ghidra_os4`.
OS4 main object update `FUN_0105f594` (68k `FUN_000e3a58`) **failed to decompile**, so firing,
projectile flight and the per-weapon constants come from 68k only. OS4 cross-checks are listed
where they exist. C = CONFIRMED (read in code/data), I = INFERRED.

## 0. Common primitives

| Role | 68k | OS4 | Notes |
|---|---|---|---|
| PlaySound(id, loop, vol, rateHz, x, y, z) | `FUN_000170ce` | `FUN_0100815c` | C. Not an effect spawn. Returns a channel handle when loop=1. `FUN_000172e8` moves a looping sound, `FUN_000173c8` stops one. |
| Random(n) → [0,n) | `FUN_0003fc8c` | `FUN_0103ac94` | C: `n * tbl[++k] >> 9` (table of shorts 0..511). |
| SpawnObject(model, x, y, heading) → index | `FUN_000cf11e` | `FUN_01046ee4` | C. Global object list `DAT_0030c318[]` (OS4 `DAT_012a6270[]`), **max 32** (`0x20`; returns 0x20 when full), last index `DAT_0011a738`. |
| SpawnParticle(type, tex, x,y,z, vx,vy,vz, life, layer, flags) | `FUN_0003d97a` | `FUN_01035da0` | C. Velocities are stored /3. Pool of 5000 per layer. Type 0xf (ground fire) also starts looping sound 13 Flames. |
| Coloured dynamic light on cubes (map,x,y,z,R,G,B) | `FUN_00046650` | — | C. Muzzle flash / fire glow. Only when the lighting option `_DAT_0011a6a4` is on. |
| Trig tables, 5760 entries (0x1680 = 360°, 16 units per degree) | X `DAT_00311e20`, Y `DAT_0030c420` | X `DAT_012ac16c`, Y `DAT_012a676c` | C (OS4 init around decompiled.c:41856): X[0]=0, Y[0]=256, so X=256·sin, Y=256·cos. Heading = obj+0x14 (`obj[5]`). |
| Frame ticks | `DAT_00348090` | `DAT_0114c6a0` | C: number of game ticks this frame (input word & 7). All timers below are in ticks. The tick rate is I (probably 50 Hz). |
| Game clock | `DAT_00349180` (+= ticks) | `DAT_0114c738` | C |

Object fields (68k offsets; OS4 is **+2** for fields from about 0x300 up, e.g. weapon 0x35e→0x360, health 0x34a→0x34c, state byte 0x426→0x428, in-vehicle short 0x424→0x426; all C from FUN_01051a10 vs FUN_000da2e0):

| 68k off | Meaning |
|---|---|
| +0x00/04/08 | x, y, z (z negative = up) |
| +0x14 | heading 0..5759 |
| +0x24 | model ptr (`obj[9]`) |
| +0x2fa/2fe/302 | vx, vy, vz per tick (OS4 0x2fc/300/304) |
| +0x336, +0x338 (shorts) | "no gravity / on ground" flags. Bullets, missile and petrol set both to 1 every tick, so they fly straight (C, FUN_000e0e46). |
| +0x33a | burn / dying timer (100 when set on fire) |
| +0x342 | projectile owner kind: 1 = player-owned (player index in +0x4aa), −1 = AI-fired (shooter ptr in +0x41e). Default −1. |
| +0x346 | AI accuracy (AI fires when `Random(0x65-acc) < ticks`) |
| +0x34a | health, init **100** for every object (FUN_000a2b80). For projectiles it doubles as the lifetime counter. |
| +0x35e | current weapon id (see §1) |
| +0x362..+0x37a | ammo (int each), +0x37e = id (+0x494) of the placed remote bomb |
| +0x3aa | fire timer: <0 = recovering, counts up by ticks; grenade charge 0..25 |
| +0x3ae | recoil / spread kick (decremented on each shot, floor −0x40) (I: meaning) |
| +0x3ca | damage filter: −1 = any; else the object only takes damage from the matching weapon (4 minigun, 7 pistol, 5 shotgun, 2 flame, 1/3 explosions) (C mechanism, I purpose: mission scripting) |
| +0x426 | state: 2 = player-controlled, 3 = walking ped, 5 = projectile/thrown, 6 = dead/removed, 7 = dying, 0x10/0x11 = AI attack, 0x12 = fleeing |
| +0x43c (`obj[0x10f]`) | out-of-ammo click cooldown (set to 10) |
| +0x440 (`obj[0x110]`) | flamethrower loop sound handle |
| +0x470 (`obj[0x11c]`) | shots-fired counter (stats) |
| +0x4a6 | player index; +0x4aa = owner player index |

Model fields: +0x48c4 radius / half-width, +0x48c8 half-length, **+0x48ec toughness T** (used as divisor
in all damage), +0x4904 is-remote-controlled, +0x4908 weapon/pickup byte, +0x4909 class
(0 vehicle/door, 1 person, 2 projectile), +0x490a police flag.

## 1. Weapon ids, ammo slots, HUD, selection

Weapon id at obj+0x35e (68k) / +0x360 (OS4). C (68k FUN_000da2e0, FUN_0002458a; OS4 FUN_01051a10):

| HUD slot / key | Weapon | id (+0x35e) | ammo 68k | ammo OS4 | people model (68k / OS4 ptr) | model +0x4908 |
|---|---|---|---|---|---|---|
| 0 | fist | 0 | — | — | malenormal `DAT_003480e0` / `DAT_012a6600` | 0xfe |
| 1 | pistol | 0xd | +0x376 | +0x378 | malenormalpistol `DAT_00348114` / `DAT_012a6634` | 6 |
| 2 | shotgun | 5 | +0x362 | +0x364 | malenormalshotgun `DAT_003480f4` / `DAT_012a6614` | 4 |
| 3 | flamethrower | 2 | +0x36a | +0x36c | `DAT_003480e8` / `DAT_012a6608` | 1 |
| 4 | grenade | 3 | +0x36e | +0x370 | `DAT_003480ec` / `DAT_012a660c` | 2 |
| 5 | minigun | 4 | +0x372 | +0x374 | `DAT_003480f0` / `DAT_012a6610` | 3 |
| 6 | bazooka | 1 | +0x366 | +0x368 | `DAT_003480e4` / `DAT_012a6604` | 0 |
| 7 | bomb (remote) | 0xe | +0x37a (+0x37e placed-bomb id) | +0x37c (+0x380) | malenormalbomb `DAT_00348118` / `DAT_012a6638` | 7 |

- Note that weapon ids (+0x35e) match the pickup types for 1..5 (bazooka 1, flame 2, grenade 3, minigun 4, shotgun 5), but pistol is 0xd and bomb 0xe. **Firing dispatches on the held people model's +0x4908**, not on +0x35e (C). Firing-pose models: 0xfd punch (`DAT_0034c0d0`), 0xfc minigun_fire (`DAT_0034c0d4`), 0xfa pistol_fire (`DAT_0034c0d8`), 0xfb shotgun_fire (`DAT_0034c0dc`; police `DAT_0034c0e0`, pedmale5/7 `DAT_0034c0e4/e8`). They revert to the normal model the next frame (C, FUN_000e3a58 ~decompiled.c:123176).
- HUD (68k `FUN_0002458a`): icons are loaded in the order fist, pistol, bomb, shotgun, flamethrower, grenade, minigun, bazooka. Each pair of .normal/.bright images is 0x20 bytes, starting at `DAT_00138338` (`FUN_0001fcf8`, C). The bar is drawn left to right as fist, pistol, shotgun, flame, grenade, minigun, bazooka, bomb, showing only weapons with ammo > 0 (bomb also when one is placed). The x step is 10 px (20 px in hi-res). The selected icon alternates normal/bright via `(DAT_00117ce2>>1)`. A 5-digit ammo counter for the current weapon is drawn at y 0x1a (0x2f in hi-res) (C).
- **Direct select** (68k `FUN_000da2e0`, OS4 `FUN_01051a10`): slot 0..7 comes from the input record (`param_2+0x16`, keys 1..7, 0 via raw key 10; 0xa/‑1 = none). A slot is accepted only if its ammo > 0 (bomb: ammo > 0 or a bomb is placed) and it is not already current. On accept: PlaySound(20 PickupWeapon, 0, 0xff, 14000, 0, −1, 0). Then `FUN_000a6bc2` (clears tutorial hint type 0x24), set +0x35e, and if not in a vehicle (+0x424 == 0) set the model ptr +0x24. Requires health > 0 and +0x490 == 0 (C).
- **Next / prev weapon** (keys "Next weapon" / "Prev weapon"; input codes 8 and 9 in `FUN_00041fe8`): `FUN_000409f2(player, curId)` / `FUN_00040ad2` map the current id to a slot, then step ±1 cyclically through slots 0..7, skipping slots with no ammo (fist is always valid). They return a slot, which goes through the direct-select path above (C).
- **Auto-switch when empty** (68k FUN_000e3a58): firing with an empty weapon while the click cooldown `obj[0x10f]` is 0 → set the cooldown to 10 ticks and PlaySound(42 Click, 0, 3000000, 16000). Then switch by a fixed priority, first weapon with ammo wins (C):
  - from grenade / bomb / bazooka: minigun → shotgun → pistol
  - from flamethrower: minigun → shotgun → pistol
  - from minigun: shotgun → pistol → flamethrower
  - from pistol: shotgun → minigun → flamethrower
  - from shotgun (ammo < 2): pistol → minigun → flamethrower
  - If nothing matches, the weapon stays (it just clicks). The cooldown decreases by ticks every frame.
- No maximum ammo was found anywhere: the pickup code adds without clamping (C, absence in FUN_000d0988 / FUN_01048ee4). The HUD prints < 100000.

## 2. Firing (68k `FUN_000e3a58`, block around decompiled.c:123030–124080)

Fire button = `_DAT_0034c3b4` (player: `DAT_0034c168[player]`; AI: random as above).
`DAT_0034c330[player]` = fire held last frame. Bazooka, bomb and tank fire only on a **press edge** (C).
With the fire button released: +0x3aa recovers toward 0 if negative, otherwise it is reset to 0. The flame loop sound stops. Punch / minigun firing models revert (C).
With fire held, at the end of the block: `if (+0x3aa < 25) +0x3aa = min(+0x3aa + ticks, 25)` (C). So the "fire timer" is shared, and weapons with a negative reload need |reload| ticks before `+0x3aa >= 0` allows the next shot.

Every shot by a person also does the following (C): `obj[0x11c]++`; `FUN_000ccabe(player)` makes walking non-police peds within `(d>>3)² < 0x17d784`, i.e. 10000 units ≈ 4.9 squares, flee (state 0x12 for 100 ticks) and say Ow1-3 (ids 37+Random(3)); `FUN_000a6c62(n)` clears tutorial hints of type 0x22 for weapon n (1 bazooka, 2 flame, 3 grenade, 4 minigun, 5 shotgun, 7 pistol, 8 bomb).

Common projectile spawn: state +0x426 = 5, prev pos +0x40a/40e = shooter xy, z = shooter z − 50 (and +0x412 = z), cube layer +0x3ce copied, +0x342/+0x4aa owner. For bullets, pos = spawn − v, i.e. one tick behind (C).

| Weapon | Trigger / rate | Ammo use | Projectile (template) | Velocity | Spread | Sound (id, vol, Hz) | Light (R,G,B) | Recoil +0x3ae |
|---|---|---|---|---|---|---|---|---|
| Fist | model 0xfe, fire with +0x3aa == 0, i.e. once per press | — | none: melee, see below | — | — | 59 Punch_Hit (4000000, 0x5622=22050) if anything was hit | — | — |
| Pistol | +0x3aa ≥ 0; then +0x3aa = −15 → 1 shot / 15 ticks | 1 | bullet.obj #2 `DAT_0030c414` (OS4 `DAT_012a6764`) | Y/X[h']·(256) + shooter v | h' = h + Random(40) − 20 (±1.25°) | 53 Pistol (4000000, 14000) | 40M,40M,10M | −4 |
| Minigun | +0x3aa ≥ 0; then −3 → 1 shot / 3 ticks | 1 | bullet.obj #1 `DAT_0030c40c` (OS4 `DAT_012a675c`) | 256/tick + shooter v | h + Random(80) − 40 (±2.5°) | 19 Minigun (4000000, 16000) | 60M,60M,8M | −0x10 |
| Shotgun | needs ammo ≥ 2 and +0x3aa ≥ 0; then −30 → 1 shot / 30 ticks | **2** | shotgunbullet.obj `DAT_0030c410` (OS4 `DAT_012a6760`) ×**2 pellets** | 256/tick + shooter v | each pellet h + Random(160) − 80 (±5°); pellet 2 spawns offset (+Y[h']/2, +X[h']/2) sideways | 21 ShotgunFire (4000000, 14000) | 60M,60M,20M | −0x20 |
| Flamethrower | +0x3aa ≥ 0, every tick held: a petrol every 3rd sub-tick (`(DAT_0034c3f0+i) % 3 == 0`) | 1 per petrol | petrol.obj `DAT_0030c404` (OS4 `DAT_012a6754`) | dir·256 + shooter v + (Random(50) − 25) on x and y; advanced i sub-ticks | jitter only | 41 Flamethrower, **looped** (vol 5000000, 0x30d4=12500), handle in obj[0x110] | flame glow per petrol | — |
| Grenade | hold to charge +0x3aa 0..25; **throw on release** if +0x3aa > 0 | 1 | grenade.obj `DAT_0030c408` (OS4 `DAT_012a6758`), +0x34a = **80** (fuse) | v = shooter v + dir·P·10/256 (P = charge ≤ 25, so ≤ 250/tick); vz = shooter vz − P·40 (≤ −1000, up) | spawn at (x − Y[h], y + X[h]), i.e. 256 units to the side (hand) | none at throw | — | — |
| Bazooka | press edge | 1 | missile.obj `DAT_0030c400` (OS4 `DAT_012a6750`) at the shooter's position | dir·256 + shooter v | none | 17 FireMissile (5000000, 17000) | — | −0x20; plus 8 back-blast dust particles (type 6, `DAT_0030c3e4`) |
| Bomb (remote) | press edge. 1st press with no placed bomb: drop. Next press: **detonate** | 1 on drop | bomb.obj `DAT_0030c418` (OS4 `DAT_012a6768`), +0x34a = 100, spawned 512 units ahead (pos + 2·dir), v = shooter v | — | — | — | — | — |

Bomb detail (C, ~123210–123320): on drop, +0x37e = bomb's unique id (+0x494). The bomb's +0x47c is the mission id `DAT_0011a77e` (or −2). On the next press the bomb is found by id: the camera target `DAT_0034c1e8/1f8[player]` is set to the bomb pos<<5, then **`FUN_000cd6ae(bombIdx, 50000)`** (big explosion). Then `FUN_000a653a(+0x47c)` (script event), +0x37e = 0, `DAT_0034c1b8[player]` = 120 ticks. The bomb has no fuse. Bomb T (+0x48ec) = 200. If it is destroyed by damage it goes off as a small explosion (FUN_000cda5e, P = 200·25000).
Phrase 2107 "Remote-triggered bomb (1)" matches.

Fist (C, ~123605–123690): model → punch model. The hit point is pos + 256·dir. Targets are every person (class 1) other than self with |Δz| < 250, |Δx|, |Δy| < 3·r (r = puncher model +0x48c4), and line of sight (`FUN_0009217e`). Each target with health ≥ 0 says Ow (37+Random(3)) and loses **30** health (direct, not divided by T). At ≤ 0 → state 7 (dying). A hit on a non-player (+0x342 == −1) adds score `DAT_0030c3b0·500`. A punch also breaks a crate in the square at pos + 512·dir (`FUN_000d0670`).

AI (police/script attackers) shoot shotguns. Script attack mode (68k `FUN_000b28ac` ~88550) sets +0x35e = 5, shotgun ammo 100, state 0x10/0x11, accuracy +0x346. An AI shot sets the projectile's +0x342 = −1 and +0x41e = shooter (C).

### Tank cannon / vehicle fire — 68k `FUN_000e2a50`, OS4 `FUN_0105e02c` (C)
- When the vehicle model's +0x4908 == 0 (only tank.obj sets it to 0; OS4 decompiled.c:41164): on a fire press edge, no ammo. PlaySound(28 BigGunFire, 0, 15000000, 9000) at the muzzle. Spawn missile.obj at pos + 5·dir (1280 units ahead), heading = tank heading, v = tank v + 2·dir (512/tick), vz = 0, z − 50, +0x338/+0x336 = 1, owner = player. 8 dust particles (type 6, life 400) at pos + 10·dir.
- Otherwise (bombtruck, whose +0x4900 = remotecontrolledcar.obj): launch the remote-controlled car if the player has none (`DAT_003480a0[player] == −1`). Its engine sound is the model's +0x4922, plus a looping sound 2. This is not a weapon in the HUD sense.
- Tank T = 40000, so bullets do 0 damage (integer division) and only big explosions hurt it (C arithmetic).
- `TankBits` (68k `FUN_00095efa`, OS4 `FUN_010bf310`) is a texture name used when generating tank debris (I: not traced further).

## 3. Projectiles: flight, hits, lifetime

All projectiles are normal objects in the 32-slot object list, model class (+0x4909) = 2 (C).
Physics step `FUN_000e0e46` (per tick): if not flagged, `vz += 0x20` (gravity, 32 units/tick², +z down), then z += vz. After that x += vx, y += vy, then map collision `FUN_00044f82` (C).

| Projectile | Gravity | Per-tick lifetime (+0x34a from 100) | Range | On wall/ground/timeout | Notes |
|---|---|---|---|---|---|
| bullet (pistol, minigun) | no | −1/tick → 100 ticks | 100·256 = 25600 units ≈ 12.5 squares (+ shooter speed) | step back one tick, PlaySound(29 Ricochet, 0, 4000000+Random(2000000), 0x2cec+Random(1000)), 2 spark particles (type 9, Flare `DAT_00348098`), state 6 | C, ~121672–121754 |
| shotgun pellet | no | −2/tick → 50 ticks | ≈ 6.25 squares | same as bullet | C |
| petrol | no | −5/tick → 20 ticks | ≈ 2.5 squares | particle type 0xf ground fire (Flame.1 `DAT_0030c3f0`, life 800) + looping Flames, state 6. Each tick: random orange light + a flame particle (type 1, random Flame.1-4) | C, ~121557 and FUN_000e0e46 |
| missile | no | none (+0x34a not decremented) | until impact | wall/ground: ground-fire particle (life 0x514) and +0x34a = 0, then the death pass `FUN_000dc5ac` (+0x342 == 1) calls `FUN_000cda5e` (explode) → state 6. Spins (+0x0c += 0x140·ticks) | C. Note an AI missile (+0x342 == −1) would go through the "dying" path instead (I). |
| grenade | yes | fuse −1/tick from 80 | — | wall: reflect vx or vy + 18 GrenadeBounce (4000000, 8000). Ground: z = floor, vz = −vz/4, bounce sound if vz > 500. Friction toward 0 by 8·ticks on ground, 1·ticks in air. Airborne spin heading += 0xb4·ticks. Smoke particle (type 6 dust) each frame. Fuse < 2 → `FUN_000cd6ae(self, 3000)` | C, ~121576–121668 |
| bomb | yes | none | — | same bounce/friction as grenade, sound rate 5000 | C |

Hits on objects: in the pair loop of 68k `FUN_000de0b6` (object-vs-object, uses model extents +0x48c4/+0x48c8/+0x48cc). A projectile never hits its own shooter: excluded if `other == +0x41e`, or if player-owned and other is that player's current object (C, ~118885–119560):
- **bullet #1 (minigun)**: other.health −= **10000 / T_other** (if +0x3ca ∈ {−1, 4}); projectile → state 6.
- **bullet #2 (pistol)**: health −= **5000 / T** (+0x3ca ∈ {−1, 7}).
- **shotgun pellet**: health −= **20000 / T**, needs line of sight `FUN_0009217e` (+0x3ca ∈ {−1, 5}).
  - Examples: pedestrians T = 90 (0x5a; pedmale3 120, pedmale4/5 100, pedmale7 95), so minigun 111, pistol 55, pellet 222 against 100 health. Default car T = 1400 (0x578): 7, 3, 14.
  - Person with health < 1 → state 7. Deathmatch frag counters `DAT_0030c3d0[]`. Score for player-owned hits on non-players: min(8000, K·40/T)·`DAT_0030c3b0` with K = 400000 / 200000 / 800000 (scoring agent covers this).
- **missile**: `FUN_000cda5e(missileIdx)` explodes at the missile, projectile → 6.
- **petrol**: flame particle at the hit, and (+0x3ca ∈ {−1, 2}) burn damage dmg = clamp(ticks·3000/T, 1, health+71).
  - A person whose health goes below 0 is set to 0 and **set on fire**: +0x33a = 100, PlaySound(15 LongAaah, 0, 5000000, 6000) the first time, player stat +0x460++ (people set on fire).
  - A burning person (`FUN_000dc5ac` ~117330–117440): +0x33a −= ticks, looping 13 Flames, flame particles (type 4) and light. At < 2 → state 7 (dies).
- **grenade / bomb vs object**: no damage; they bounce: v += other.v_impulse(+0x352..0x35a)/4, plus the bounce sound.

Hits on scenery (68k `FUN_000cdfdc` ~111120–111240): a projectile entering a square with scenery stops. Missile → state 6 + `FUN_000d0670` (break crate). (I: no explosion on scenery; it only explodes via the wall/ground path.) Petrol → state 6, flame, break crate. Bullets/pellets → break crate, ricochet sparks, state 6. Grenade/bomb → bounce sound.
Non-projectiles (vehicles) driving into scenery whose model +0x4908 == 0xfe (**snowman**) destroy it: 5 dust particles, scenery set to 0 (C).

### Explosions
- `FUN_000cd22e(owner, x, y, z, P, layer, srcIdx)` — the core blast (68k; OS4 not located, I). C:
  - PlaySound(0 Explosion, 0, 50000000, 12000 − min(P/20000, 7000)).
  - `FUN_000ccbac`: walking peds within 10000 units flee.
  - For every object except srcIdx and except projectiles (special objects `DAT_0030c3c0..cc` are included):
    ```
    d = (Δx>>8, Δy>>8, Δz>>8)                // 256-unit cells = 1/8 square
    r2 = |d|²;  f = isqrt(r2) * (r2>>4)      // ≈ |d|³/16
    f = person ? f*5 : f>>1;  if f==0: f=1
    Q = P>>4
    if not projectile: v += clamp(d * (Q / T) / f, ±1000)   // knock-back
    if +0x3ca ∈ {−1,1,3}: health -= (Q/T [>>4 if projectile]) / f
    ```
    Falloff is ∝ 1/distance³ for damage (≈1/d² for impulse).
  - Screen shake `DAT_0030c3e0 += P/250000`. Particles: type 8 dust (life clamp(P/20000, 1000, 6000)), type 10 (flash, 300), type 0xb Blood-texture (I: scorch/debris).
  - Score for player-caused damage to non-players: min(6000, …)·mult.
- `FUN_000cd6ae(idx, power)` — destroy object idx (C):
  - Two debris passes `FUN_00038f44` (model shatter), state 6.
  - Person: 12 Squelch + blood.
  - Else: power defaults to T/2 when 0; flame particles (count from power>>8); then **Explode P = power·20000** at the object's z + 500.
  - Used by: grenade fuse (power 3000 → P = 6e7), remote bomb (50000 → P = 1e9), objects with health < −70 (power = T/2, e.g. car 700 → 1.4e7).
- `FUN_000cda5e(idx)` — impact explosion: sets 0x33a = 100, state 5, kicks v randomly, flames, then **Explode P = T_self·25000**. Missile T = 1000 → P = 2.5e7. Also used for vehicles reaching health < 1.
- `FUN_000cdd02(x, y, z, layer, power)` — script/world explosion, P = power·25000 (68k caller 90405).
- Lethal radius vs a T = 90 person (dmg ≥ 100; computed from the formula, I):
  - missile ≈ 8 cells (2100 units ≈ 1 square)
  - grenade ≈ 11 cells (≈ 1.4 squares)
  - bomb ≈ 28 cells (≈ 3.5 squares)
- Health ≤ −71 (`0xffffffb9`) is the "blow apart" marker → `FUN_000cd6ae(idx, 0)` in `FUN_000dc5ac` (C).

Weapon templates: missile T = 1000, petrol / grenade / bullets / pellets T = 1, bomb T = 200; all class 2 (OS4 FUN_0104e13c ~40919, 68k ~115020) (C).

## 4. Pickups

Map cube record in memory: 68k stride 0x26 per cube, 0x72 per y, 0x3900 per x (base `DAT_0030c3bc`); OS4 0x28 / 0x78 / 0x3c00 (base `DAT_0114c6dc`). Byte +1 = scenery id, byte +4 = special (crate contents), byte +0 = rotation (C).

Pickup models (+0x4908, OS4 FUN_0104e13c, C): bazooka 1, flamethrower 2, largegrenade 3, minigun 4, shotgun 5, pistol 6, bigbomb 7, Info 0x50, key 0x51, radar 100, snowman 0xfe (destructible, not a pickup). Briefcase (23) and phone have none.

**Collect** — 68k `FUN_000d0988(obj, sx, sy, layer, z)`, OS4 `FUN_01048ee4` (C):
- Called from movement `FUN_00044f82` when the mover's state +0x426 == 2 (player-controlled), for the square containing its position (`pos>>11`) at its cube layer. Vehicles check all squares they overlap (the 3×3 footprint loop ~33610), and also break crates there. So the **pickup radius is "same square"**; there is no distance test.
- Only types 1..99 qualify (radar 100 and negatives are excluded).
- Queue respawn: ring buffer of 1000 entries (sx `DAT_0034a128`, sy `DAT_00349188`, layer `DAT_0034b0c8`, time `DAT_003481e0` = clock + **1200 ticks**). OS4: `DAT_012e1c5c/3b9c/2bfc/4b3c`, head `DAT_0114c68c`.
- 2 sparkle particles (type 3, Dust `DAT_00348094`).
- Types < 0x50: PlaySound(20 PickupWeapon, 0, 1000000, 14000) at the player, add ammo, show phrase:

  | type | ammo | field | phrase |
  |---|---|---|---|
  | 1 bazooka | +4 | +0x366 | 2100 "Bazooka (4)" |
  | 2 flamethrower | +75 | +0x36a | 2101 "Flamethrower (75)" |
  | 3 grenade | +6 | +0x36e | 2102 "Grenade (6)" |
  | 4 minigun | +100 | +0x372 | 2103 "Minigun (100)" |
  | 5 shotgun | +20 (= 10 shots) | +0x362 | 2104 "Shotgun (20)" |
  | 6 pistol | +30 | +0x376 | 2106 "Pistol (30)" |
  | 7 bigbomb | +1 | +0x37a | 2107 "Remote-triggered bomb (1)" |

  The phrase id is `0x834 + type − 1` (with 2105 skipped for pistol and bomb). It is fetched with `FUN_00091b2c(id)` and shown via `FUN_000364ee(sx<<11|0x400, sy<<11|0x400, z, 2, text)` (floating text at the square centre). OS4 values are identical (C).
- 0x50 Info: same sound, phrase 2105 "Information" (I: the actual info text comes from the mission script).
- 0x51 Key: same sound, phrase 2108 "Key".
- Then the square's scenery byte is set to 0.

**Respawn** — 68k `FUN_000d079e`, OS4 `FUN_01048c40` (C):
- Every frame, for each queued entry whose time ≤ clock: set the square's scenery to **6 (crate)** (the special byte is untouched), 2 sparkle particles, PlaySound(47 ObjectAppear, 0, 20000000, 8000) at the square centre.
- So every collected pickup (including directly placed weapons, Info and Key, types 1..99) comes back after 1200 ticks as a **crate** with the square's original special. Weapons placed directly with special 0 therefore come back as empty crates (C logic; I that this was intended).

**Crate break** — 68k `FUN_000d0670(sx, sy, layer, z)`, OS4 `FUN_01040ce4` (C):
- If scenery == 6: crate debris `FUN_000384e8` (68k) / `FUN_01035730` (OS4)(crate model, layer+1, square centre, z, heading = rot·5760/256, 5 pieces).
- Then scenery := by special: 1 → 9 bazooka, 2 → 10 flamethrower, 3 → 11 largegrenade, 4 → 12 minigun, 5 → 13 shotgun, 6 → 27 pistol, 7 → 28 bigbomb, else 0 (empty).
- Breakers: punch (square 512 ahead), bullets, pellets, missile and petrol hitting the crate square, and vehicles driving over it.

Radar (20, type 100), Briefcase (23), phones: not handled by the pickup code above (I: mission script / radar logic elsewhere).

## 5. Sound ids (C, OS4 FUN_01005864 load order, matches the 68k calls above)
0 Explosion, 12 Squelch, 13 Flames, 15 LongAaah, 17 FireMissile, 18 GrenadeBounce, 19 Minigun,
20 PickupWeapon, 21 ShotgunFire, 28 BigGunFire, 29 Ricochet, 30 HitScenery, 37–39 Ow1–3,
41 Flamethrower, 42 Click, 43 Splash (objects landing in water, vol T·20000), 47 ObjectAppear,
53 Pistol, 59 Punch_Hit.

## 6. Open / unverified
- Tick rate (I: 50 Hz). Default ammo at level start (not found; probably set by the mission scripts).
- OS4 addresses for explode / projectile code (inside the failed FUN_0105f594 or nearby, not mapped).
- Whether a missile hitting scenery (not the cube wall) explodes (code sets state 6 only).
- Petrol ground-fire particles (type 0xf) do not seem to damage anything, and no "building on fire" state was found. Fire on buildings looks visual only (I).
<!-- DONE -->
