//! Palette space and the final frame pass of the game views
//! (docs/re/rendering.md §3, §4, "Palette output").
//!
//! The original draws into an 8-bit chunky screen: every pixel is a palette
//! index, and every blend, Gouraud shade and night remap goes through a
//! table whose entries are the palette colour nearest the arithmetic result
//! (`Alpha.256`, `Gouraud.256`; metric `25|ΔR| + 36|ΔG| + 16|ΔB|`, first best
//! wins).
//!
//! Blending in palette space: the game views render into a linear float
//! target (Rgba16Float, [`setup_cameras`]) holding the gamma-encoded
//! palette RGB the original adds, subtracts and averages: every world
//! shader writes those bytes (`GAMMA_TARGET`), so the 3D passes' blend
//! states compute the `Alpha.256` arithmetic on them. After the 3D passes
//! a resolve pass ([`resolve_pass`], `shaders/resolve.wgsl`) snaps the
//! world to the palette (software) or cuts it to the hardware renderer's
//! dithered 16-bit 5-5-5 screen, and decodes it to linear light for the
//! HUD. One pass at the end of each game view (after the HUD, before the
//! window blit) then
//!
//! 1. snaps every pixel to the nearest palette colour through a 256³ lookup
//!    table computed exactly with that metric ([`nearest_lut`]);
//! 2. with "Motion blur" on (`DAT_0114c6c8`, software renderer only), shows
//!    `Alpha.256` table 4 of the previous and the current frame, i.e. the
//!    palette colour nearest their average (`FUN_01042f00` →
//!    `FUN_01034438` → `FUN_0112cdb8`, Mac `MotionBlur` @ 100a6d50 →
//!    `ScreenAlpha` @ 10154eb0: the frame is drawn alternately into two
//!    buffers `DAT_0114c7a8` / `DAT_0114c7ac` and the visible one,
//!    `DAT_0114c7b0`, is `T4[prev][cur]` per pixel). Switching it on blanks
//!    the buffers (Mac `ToggleMotionBlur` @ 10053210 → `BlankChunky`; the
//!    OS4 `FUN_01042308` left a stale frame), so the first blended frame
//!    is averaged with black. The blend loop runs `(W·H >> 5) − 1` times
//!    32 pixels (both builds, docs/re/mac/rendering.md §4), so the last 32
//!    pixels of the display buffer are never written while blur is on:
//!    here the view's last 32 pixels (bottom row, right end) show palette
//!    colour 0 (INFERRED: the display buffer's untouched content);
//! 3. adds the palette flash (`DAT_0114c748`, `FUN_0103ad6c`: every palette
//!    entry becomes `min(255, rgb + flash)`), after the snap as the original
//!    changes the palette, not the pixels.
//!
//! The hardware render mode (`crate::render_mode`, rendering.md §11) has
//! no palette: only the flash (a full-screen additive quad there) is
//! applied here.
//!
//! What stays approximate: the original snaps to the palette (and clamps
//! each channel to 0..255) after every blend; here the world is snapped
//! once before the HUD and the float target clamps only then. The HUD
//! draws in linear light (its subtract panels too, `crate::hud`).
//! `--linear-blend` restores the old pipeline (sRGB target, linear-light
//! blends) for comparison.

use super::{ExtraView, GameCamera};
use crate::data::GameData;
use crate::level::AppState;
use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    core_pipeline::{Core3d, FullscreenShader},
    image::ImageSampler,
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        camera::ExtractedCamera,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_resource::{
            binding_types::{texture_2d, texture_3d, uniform_buffer_sized},
            *,
        },
        renderer::{RenderContext, ViewQuery},
        texture::GpuImage,
        view::ViewTarget,
    },
};
use std::collections::HashMap;
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};

/// Weights of the colour metric (`FUN_0103b540`, rendering.md §3).
const WEIGHTS: [u32; 3] = [25, 36, 16];

/// The palette index nearest every RGB colour, 256³ entries indexed
/// `b·65536 + g·256 + r` (the layout of a 3D texture with x = R, y = G,
/// z = B). Ties go to the lower index; a result of 0 becomes 8 as in the
/// table generator (both are black in the shipped palette).
///
/// The metric is a weighted L1 distance, the shortest-path distance of the
/// colour grid, so it is computed exactly by a separable distance transform
/// (a forward and a backward sweep per axis) instead of 256 comparisons per
/// entry; ties stay exact because the per-axis minima compose
/// lexicographically on (distance, index).
pub fn nearest_lut(palette: &[u8]) -> Vec<u8> {
    const N: usize = 256;
    let mut dist = vec![u16::MAX; N * N * N];
    let mut idx = vec![0u8; N * N * N];
    for i in (0..256).rev() {
        let c = &palette[i * 3..i * 3 + 3];
        let k = c[2] as usize * N * N + c[1] as usize * N + c[0] as usize;
        dist[k] = 0;
        idx[k] = i as u8;
    }
    for (axis, stride) in [(0, 1usize), (1, N), (2, N * N)] {
        let w = WEIGHTS[axis] as u16;
        // Every line along the axis: the other two coordinates.
        let (sa, sb) = match axis {
            0 => (N, N * N),
            1 => (1, N * N),
            _ => (1, N),
        };
        for a in 0..N {
            for b in 0..N {
                let base = a * sa + b * sb;
                let mut relax = |from: usize, to: usize| {
                    let d = dist[from].saturating_add(w);
                    if d < dist[to] || (d == dist[to] && idx[from] < idx[to]) {
                        dist[to] = d;
                        idx[to] = idx[from];
                    }
                };
                for x in 1..N {
                    relax(base + (x - 1) * stride, base + x * stride);
                }
                for x in (0..N - 1).rev() {
                    relax(base + (x + 1) * stride, base + x * stride);
                }
            }
        }
    }
    for i in &mut idx {
        if *i == 0 {
            *i = 8;
        }
    }
    idx
}

/// [`nearest_lut`] of a palette, computed once per palette.
pub fn cached_lut(palette: &[u8]) -> Arc<Vec<u8>> {
    static CACHE: Mutex<Option<(Vec<u8>, Arc<Vec<u8>>)>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    match &*cache {
        Some((pal, lut)) if pal.as_slice() == palette => lut.clone(),
        _ => {
            let lut = Arc::new(nearest_lut(palette));
            *cache = Some((palette.to_vec(), lut.clone()));
            lut
        }
    }
}

/// One `Alpha.256` entry, table `k`, destination (framebuffer) colour `d`
/// and source (texel) colour `s` (Mac `CalcAlpha` @ 10083ec0, OS4
/// `FUN_0103b540`). Per channel, clamped to 0..255, then the nearest
/// palette colour (`ClosestMatch` @ 10083dc0):
///
/// | k | result |
/// |---|---|
/// | 0 | d + s |
/// | 1 | d − s |
/// | 2 | d − 16 if s ≠ 0 (index), else d |
/// | 3 | (s·255 + (255 − max(sR, sG, sB))·d) >> 8 |
/// | 4 | s if s = d (index), else (s + d) >> 1 |
/// | 5, 6, 7, 8 | d + s on R / R and G / G / B only |
/// | 9 | d + s − 32 |
/// | 10 | pal\[s\] + d (the index) − 128 |
///
/// The `s = d` shortcut of table 4 is the Mac's (the OS4 generator lacks
/// it; only duplicate colours differ).
fn alpha_entry(pal: &[u8], lut: &[u8], k: usize, d: usize, s: usize) -> u8 {
    let dc = |c: usize| pal[d * 3 + c] as i32;
    let sc = |c: usize| pal[s * 3 + c] as i32;
    let rgb: [i32; 3] = match k {
        0 => std::array::from_fn(|c| dc(c) + sc(c)),
        1 => std::array::from_fn(|c| dc(c) - sc(c)),
        2 if s != 0 => std::array::from_fn(|c| dc(c) - 16),
        2 => std::array::from_fn(dc),
        3 => {
            let m = sc(0).max(sc(1)).max(sc(2));
            std::array::from_fn(|c| (sc(c) * 255 + (255 - m) * dc(c)) >> 8)
        }
        4 if s == d => return s as u8,
        4 => std::array::from_fn(|c| (sc(c) + dc(c)) >> 1),
        5..=8 => {
            let add = [[true, false, false], [true, true, false], [false, true, false], [false, false, true]]
                [k - 5];
            std::array::from_fn(|c| dc(c) + if add[c] { sc(c) } else { 0 })
        }
        9 => std::array::from_fn(|c| dc(c) + sc(c) - 32),
        _ => std::array::from_fn(|c| d as i32 + sc(c) - 128),
    };
    let [r, g, b] = rgb.map(|v| v.clamp(0, 255) as usize);
    lut[b * 65536 + g * 256 + r]
}

/// The 11 `Alpha.256` tables `T[k][dst·256 + src]` computed from the
/// palette (Mac `GetAlpha` @ 10084710 → `CalcAlpha` @ 10083ec0). The Mac's
/// shipped file is exactly this; the Amiga Update 7 file was built from the
/// CD palette (colour 8 = 7,7,7) and is not used.
pub fn alpha_tables(data: &GameData) -> Arc<Vec<u8>> {
    static CACHE: Mutex<Option<(Vec<u8>, Arc<Vec<u8>>)>> = Mutex::new(None);
    let pal = &data.palette;
    if let Some((p, t)) = &*CACHE.lock().unwrap_or_else(|e| e.into_inner())
        && p.as_slice() == pal.as_slice()
    {
        return t.clone();
    }
    let lut = cached_lut(pal);
    let mut t = vec![0u8; 11 * 65536];
    for (i, v) in t.iter_mut().enumerate() {
        *v = alpha_entry(pal, &lut, i >> 16, (i >> 8) & 0xff, i & 0xff);
    }
    let t = Arc::new(t);
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some((pal.to_vec(), t.clone()));
    t
}

/// The night remap of cube and object textures (`FUN_0106ecf0(1)`,
/// `FUN_010bf310(1)`): `c → T1[c][0x67]`, the palette colour nearest
/// `pal[c] − pal[0x67]` = `pal[c] − (87, 87, 87)`; colour 0 unchanged.
/// Computed as `CalcAlpha` does (see [`alpha_tables`]).
pub fn night_remap(data: &GameData) -> [u8; 256] {
    let mut map = [0u8; 256];
    let pal = &data.palette;
    for (c, m) in map.iter_mut().enumerate().skip(1) {
        let want: [i32; 3] =
            std::array::from_fn(|k| (pal[c * 3 + k] as i32 - pal[0x67 * 3 + k] as i32).max(0));
        *m = nearest(pal, want);
    }
    map
}

/// The palette index nearest `rgb` (brute force; for small jobs).
pub fn nearest(palette: &[u8], rgb: [i32; 3]) -> u8 {
    let mut best = (u32::MAX, 0usize);
    for i in 0..256 {
        let d: u32 = (0..3)
            .map(|k| WEIGHTS[k] * (palette[i * 3 + k] as i32 - rgb[k]).unsigned_abs())
            .sum();
        if d < best.0 {
            best = (d, i);
        }
    }
    if best.1 == 0 { 8 } else { best.1 as u8 }
}

/// GPU copies of the tables: the nearest-colour LUT (256³ R8), the palette
/// (256×1, gamma-encoded RGB in a linear format) and `Alpha.256` table 4
/// (256×256 R8, `T4[prev][cur]` at x = cur, y = prev).
#[derive(Resource, Clone, ExtractResource)]
pub struct PaletteTextures {
    pub lut: Handle<Image>,
    pub palette: Handle<Image>,
    pub average: Handle<Image>,
}

/// The "Motion blur" option (`DAT_0114c6c8`, toggled by `FUN_01042308`):
/// `Settings::motion_blur`, `--motion-blur [0|1]` wins. The original
/// refuses it under hardware rendering, and so does the final pass in
/// [`crate::render_mode::RenderMode::Hardware`].
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MotionBlur(pub bool);

fn forced_blur() -> Option<bool> {
    crate::args()
        .any(|a| a == "--motion-blur")
        .then(|| crate::arg("--motion-blur").is_none_or(|v| v != "0"))
}

/// Per game view: the final pass's parameters (extracted).
#[derive(Component, Clone, Copy, Debug, Default, ExtractComponent)]
pub struct PostFx {
    /// Palette flash, 0..1 (`flash / 255`).
    pub flash: f32,
    pub blur: bool,
    /// Snap to the palette (`--no-palette-snap` turns it off, for checks).
    pub snap: bool,
    /// Game ticks run (wrapping): the blur's previous frame is the last
    /// one drawn before the latest tick, so the trail is one tick long at
    /// any display rate, as in the original (one frame per tick).
    pub tick: u32,
}

pub struct PostFxPlugin;

impl Plugin for PostFxPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/postfx.wgsl");
        embedded_asset!(app, "shaders/resolve.wgsl");
        app.insert_resource(MotionBlur(forced_blur().unwrap_or(false)))
            .add_plugins((
                ExtractComponentPlugin::<PostFx>::default(),
                ExtractComponentPlugin::<GammaWorld>::default(),
                ExtractResourcePlugin::<PaletteTextures>::default(),
            ))
            .add_systems(PreUpdate, blur_settings)
            .add_systems(PostUpdate, (setup_cameras, update_views, update_gamma_views));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<FrameParity>()
            .add_systems(RenderStartup, (init_pipeline, init_resolve_pipeline))
            .add_systems(Render, flip_parity.in_set(RenderSystems::Prepare))
            .add_systems(
                Core3d,
                (
                    resolve_pass.in_set(bevy::core_pipeline::Core3dSystems::EarlyPostProcess),
                    postfx_pass
                        .after(bevy::ui_render::ui_pass)
                        .before(bevy::core_pipeline::upscaling::upscaling),
                ),
            );
    }
}

// ---------------------------------------------------------------------------
// Palette-space blending (rendering.md §3 "Palette output")

/// `--linear-blend`: the old pipeline (blends in linear light on an sRGB
/// target, snapped once at the end), for comparison.
pub fn linear_blend() -> bool {
    crate::args().any(|a| a == "--linear-blend")
}

/// Every 3D camera got its target set up ([`setup_cameras`]).
#[derive(Component)]
struct TargetSetUp;

/// A game view whose world is drawn in gamma-encoded palette RGB and
/// resolved before the HUD ([`resolve_pass`], `shaders/resolve.wgsl`).
/// `mode`: 0 decode only, 1 snap to the palette (software), 2 the 16-bit
/// hardware screen.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, ExtractComponent)]
pub struct GammaWorld {
    pub mode: u32,
}

/// The views' targets: the world shaders write the gamma-encoded palette
/// bytes the original blends (`shaders/*.wgsl` under the `GAMMA_TARGET`
/// def, chosen by the view's HDR key), so the 3D passes need a linear
/// float target: every 3D camera gets `Hdr` (Rgba16Float), no tone
/// mapping and no deband dither — the menu-model camera too, as it draws
/// into the same window texture (main textures are shared per format);
/// its object materials stay linear (`objects::EnvMapped` `params.y`).
/// The game views clear to the clear colour's gamma bytes, as their
/// resolve pass decodes everything.
#[allow(clippy::type_complexity)]
fn setup_cameras(
    mut commands: Commands,
    clear: Res<ClearColor>,
    mut cameras: Query<
        (Entity, &mut Camera, Has<GameCamera>, Has<ExtraView>),
        (With<Camera3d>, Without<TargetSetUp>),
    >,
) {
    if cameras.is_empty() || linear_blend() {
        return;
    }
    for (e, mut camera, game, extra) in &mut cameras {
        let mut ec = commands.entity(e);
        ec.insert((
            TargetSetUp,
            bevy::camera::Hdr,
            bevy::core_pipeline::tonemapping::Tonemapping::None,
            bevy::core_pipeline::tonemapping::DebandDither::Disabled,
        ));
        if game || extra {
            ec.insert(GammaWorld::default());
            if matches!(camera.clear_color, ClearColorConfig::Default) {
                let s = clear.0.to_srgba();
                camera.clear_color =
                    ClearColorConfig::Custom(Color::linear_rgba(s.red, s.green, s.blue, s.alpha));
            }
        }
    }
}

/// The resolve mode of the game views: snap while a level is played in
/// software, the 16-bit screen in hardware.
fn update_gamma_views(
    state: Res<State<AppState>>,
    mode: Res<crate::render_mode::RenderMode>,
    mut views: Query<&mut GammaWorld>,
) {
    let playing = *state.get() == AppState::Playing;
    let want = if mode.hardware() {
        2
    } else if playing && !crate::args().any(|a| a == "--no-palette-snap") {
        1
    } else {
        0
    };
    for mut v in &mut views {
        v.set_if_neq(GammaWorld { mode: want });
    }
}

#[derive(Resource)]
struct ResolvePipeline {
    layout: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
    vertex: VertexState,
}

fn init_resolve_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
) {
    let float = TextureSampleType::Float { filterable: false };
    let layout = BindGroupLayoutDescriptor::new(
        "resolve_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(float),
                texture_3d(float),
                texture_2d(float),
                uniform_buffer_sized(false, NonZeroU64::new(16)),
            ),
        ),
    );
    commands.insert_resource(ResolvePipeline {
        layout,
        shader: asset_server.load("embedded://payback/shaders/resolve.wgsl"),
        vertex: fullscreen_shader.to_vertex_state(),
    });
}

#[derive(Default)]
struct ResolveState {
    /// Window-sized copy of the frame (each view touches its viewport).
    copy: Option<(UVec2, TextureFormat, Texture, TextureView)>,
    pipelines: HashMap<TextureFormat, CachedRenderPipelineId>,
}

/// After the 3D passes of a game view, before the HUD: snap / quantise and
/// decode the gamma-encoded world to linear light (`shaders/resolve.wgsl`).
fn resolve_pass(
    view: ViewQuery<(&ViewTarget, &ExtractedCamera, &GammaWorld)>,
    pipeline: Option<Res<ResolvePipeline>>,
    pipeline_cache: Res<PipelineCache>,
    tables: Option<Res<PaletteTextures>>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    mut state: Local<ResolveState>,
    mut ctx: RenderContext,
) {
    let (target, camera, gamma) = view.into_inner();
    let (Some(pipeline), Some(tables)) = (pipeline, tables) else {
        return;
    };
    let (Some(lut), Some(palette)) = (gpu_images.get(&tables.lut), gpu_images.get(&tables.palette)) else {
        return;
    };
    let Some(size) = camera.physical_target_size else {
        return;
    };
    let format = target.main_texture_format();
    let id = *state.pipelines.entry(format).or_insert_with(|| {
        pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("resolve_pipeline".into()),
            layout: vec![pipeline.layout.clone()],
            vertex: pipeline.vertex.clone(),
            fragment: Some(FragmentState {
                shader: pipeline.shader.clone(),
                targets: vec![Some(ColorTargetState {
                    format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
                ..default()
            }),
            ..default()
        })
    });
    let Some(render_pipeline) = pipeline_cache.get_render_pipeline(id) else {
        return;
    };
    let device = ctx.render_device().clone();
    if state.copy.as_ref().is_none_or(|c| c.0 != size || c.1 != format) {
        let t = device.create_texture(&TextureDescriptor {
            label: Some("resolve_copy"),
            size: Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let v = t.create_view(&TextureViewDescriptor::default());
        state.copy = Some((size, format, t, v));
    }
    let (_, _, copy, copy_view) = state.copy.as_ref().unwrap();
    let (origin, extent) = camera.viewport.as_ref().map_or((UVec2::ZERO, size), |v| {
        (v.physical_position, v.physical_size)
    });
    let extent = extent.min(size.saturating_sub(origin));
    if extent.x == 0 || extent.y == 0 {
        return;
    }
    let mut params = [0u8; 16];
    params[0..4].copy_from_slice(&(gamma.mode as f32).to_le_bytes());
    let uniform = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("resolve_params"),
        contents: &params,
        usage: BufferUsages::UNIFORM,
    });
    let bind_group = device.create_bind_group(
        "resolve_bind_group",
        &pipeline_cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((
            copy_view,
            &lut.texture_view,
            &palette.texture_view,
            uniform.as_entire_binding(),
        )),
    );
    let encoder = ctx.command_encoder();
    let region = Origin3d {
        x: origin.x,
        y: origin.y,
        z: 0,
    };
    encoder.copy_texture_to_texture(
        TexelCopyTextureInfo {
            texture: target.main_texture(),
            mip_level: 0,
            origin: region,
            aspect: TextureAspect::All,
        },
        TexelCopyTextureInfo {
            texture: copy,
            mip_level: 0,
            origin: region,
            aspect: TextureAspect::All,
        },
        Extent3d {
            width: extent.x,
            height: extent.y,
            depth_or_array_layers: 1,
        },
    );
    let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
        label: Some("resolve_pass"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: target.main_texture_view(),
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Load,
                store: StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_viewport(
        origin.x as f32,
        origin.y as f32,
        extent.x as f32,
        extent.y as f32,
        0.0,
        1.0,
    );
    pass.set_pipeline(render_pipeline);
    pass.set_bind_group(0, &bind_group, &[]);
    pass.draw(0..3, 0..1);
}

fn blur_settings(
    settings: Option<Res<crate::frontend::progress::Settings>>,
    mut blur: ResMut<MotionBlur>,
) {
    let Some(settings) = settings else { return };
    if settings.is_changed() && forced_blur().is_none() && blur.0 != settings.motion_blur {
        blur.0 = settings.motion_blur;
    }
}

/// Run by the start-up loader (`startup.rs`).
pub(crate) fn build_tables(mut commands: Commands, data: Res<GameData>, mut images: ResMut<Assets<Image>>) {
    let start = bevy::platform::time::Instant::now();
    let lut = cached_lut(&data.palette);
    debug!("palette LUT in {:?}", start.elapsed());
    let image = |w: u32, h: u32, d: u32, bytes: Vec<u8>, format: TextureFormat| {
        let mut image = Image::new(
            Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: d,
            },
            if d > 1 {
                TextureDimension::D3
            } else {
                TextureDimension::D2
            },
            bytes,
            format,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler = ImageSampler::nearest();
        image
    };
    let mut palette = vec![255u8; 256 * 4];
    for i in 0..256 {
        palette[i * 4..i * 4 + 3].copy_from_slice(&data.palette[i * 3..i * 3 + 3]);
    }
    // Table 4 (`CalcAlpha`): the nearest of the average.
    let average = (0..65536)
        .map(|i| alpha_entry(&data.palette, &lut, 4, i >> 8, i & 0xff))
        .collect();
    commands.insert_resource(PaletteTextures {
        lut: images.add(image(256, 256, 256, lut.to_vec(), TextureFormat::R8Unorm)),
        palette: images.add(image(256, 1, 1, palette, TextureFormat::Rgba8Unorm)),
        average: images.add(image(256, 256, 1, average, TextureFormat::R8Unorm)),
    });
}

/// Give the game views their pass parameters while a level is played (the
/// menus' views are left alone).
#[allow(clippy::type_complexity)]
fn update_views(
    mut commands: Commands,
    state: Res<State<AppState>>,
    blur: Res<MotionBlur>,
    flash: Res<crate::effects::ScreenFlash>,
    smooth: Res<crate::frame::Smooth>,
    mode: Res<crate::render_mode::RenderMode>,
    views: Query<(Entity, Option<&PostFx>), Or<(With<GameCamera>, With<ExtraView>)>>,
) {
    let playing = *state.get() == AppState::Playing;
    // Warp3D draws true colour (no palette) and has no motion blur; its
    // flash is an additive full-screen quad of `flash / 255`, the same sum.
    let hardware = mode.hardware();
    let snap = !hardware && !crate::args().any(|a| a == "--no-palette-snap");
    let want = PostFx {
        flash: flash.0.clamp(0, 255) as f32 / 255.0,
        blur: blur.0 && !hardware,
        snap,
        // Only the blur reads it; constant without, so the component is
        // not re-inserted every tick.
        tick: if blur.0 && !hardware { smooth.ticks() as u32 } else { 0 },
    };
    for (e, current) in &views {
        match (playing, current) {
            (true, Some(c))
                if c.flash == want.flash && c.blur == want.blur && c.snap == want.snap && c.tick == want.tick => {}
            (true, _) => {
                commands.entity(e).insert(want);
            }
            (false, Some(_)) => {
                commands.entity(e).remove::<PostFx>();
            }
            (false, None) => {}
        }
    }
}

/// Which of the two index buffers holds the previous frame, and the game
/// tick it was drawn in.
#[derive(Resource, Default)]
struct FrameParity(bool, Option<u32>);

/// Swap the buffers when a game tick has run since the last swap
/// (`MotionBlur` @ 100a6d50 blends consecutive frames, one per tick):
/// frames drawn between two ticks all blend with the last frame of the
/// tick before and overwrite the same buffer.
fn flip_parity(mut p: ResMut<FrameParity>, views: Query<&PostFx>) {
    let tick = views.iter().find(|fx| fx.blur).map(|fx| fx.tick);
    if tick.is_none() || tick != p.1 {
        p.0 = !p.0;
    }
    p.1 = tick;
}

#[derive(Resource)]
struct PostFxPipeline {
    layout: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
    vertex: VertexState,
}

fn init_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
) {
    let float = TextureSampleType::Float { filterable: false };
    let layout = BindGroupLayoutDescriptor::new(
        "postfx_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(float),
                texture_2d(float),
                texture_3d(float),
                texture_2d(float),
                texture_2d(float),
                uniform_buffer_sized(false, NonZeroU64::new(32)),
            ),
        ),
    );
    commands.insert_resource(PostFxPipeline {
        layout,
        shader: asset_server.load("embedded://payback/shaders/postfx.wgsl"),
        vertex: fullscreen_shader.to_vertex_state(),
    });
}

/// Window-sized working textures, shared by the views (each only touches
/// its own viewport): a copy of the frame and two palette-index buffers
/// (current / previous frame, swapped every frame).
struct Work {
    size: UVec2,
    format: TextureFormat,
    copy: Texture,
    copy_view: TextureView,
    index: [(Texture, TextureView); 2],
}

#[derive(Default)]
struct PassState {
    work: Option<Work>,
    pipelines: HashMap<TextureFormat, CachedRenderPipelineId>,
    /// Motion blur was on in the last pass.
    blur: bool,
}

fn postfx_pass(
    view: ViewQuery<(&ViewTarget, &ExtractedCamera, &PostFx)>,
    pipeline: Option<Res<PostFxPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    tables: Option<Res<PaletteTextures>>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    parity: Res<FrameParity>,
    mut state: Local<PassState>,
    mut ctx: RenderContext,
) {
    let (target, camera, fx) = view.into_inner();
    let (Some(pipeline), Some(tables)) = (pipeline, tables) else {
        return;
    };
    let (Some(lut), Some(palette), Some(average)) = (
        gpu_images.get(&tables.lut),
        gpu_images.get(&tables.palette),
        gpu_images.get(&tables.average),
    ) else {
        return;
    };
    let Some(size) = camera.physical_target_size else {
        return;
    };
    let format = target.main_texture_format();
    let id = *state.pipelines.entry(format).or_insert_with(|| {
        pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("postfx_pipeline".into()),
            layout: vec![pipeline.layout.clone()],
            vertex: pipeline.vertex.clone(),
            fragment: Some(FragmentState {
                shader: pipeline.shader.clone(),
                targets: vec![
                    Some(ColorTargetState {
                        format,
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    }),
                    Some(ColorTargetState {
                        format: TextureFormat::R8Unorm,
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    }),
                ],
                ..default()
            }),
            ..default()
        })
    });
    let Some(render_pipeline) = pipeline_cache.get_render_pipeline(id) else {
        return;
    };

    let device = ctx.render_device().clone();
    if state
        .work
        .as_ref()
        .is_none_or(|w| w.size != size || w.format != format)
    {
        let texture = |label, format, usage| {
            let t = device.create_texture(&TextureDescriptor {
                label: Some(label),
                size: Extent3d {
                    width: size.x,
                    height: size.y,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            });
            let v = t.create_view(&TextureViewDescriptor::default());
            (t, v)
        };
        let both = TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING;
        let (copy, copy_view) = texture(
            "postfx_copy",
            format,
            TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING,
        );
        state.work = Some(Work {
            size,
            format,
            copy,
            copy_view,
            index: [
                texture("postfx_index_a", TextureFormat::R8Unorm, both),
                texture("postfx_index_b", TextureFormat::R8Unorm, both),
            ],
        });
    }
    // Blur switched on: blank the previous frame (`ToggleMotionBlur` @
    // 10053210, `BlankChunky`), so the first blend is with black. The
    // buffer is shared by the views, so this clears every view's part.
    let blank = fx.blur && !state.blur;
    state.blur = fx.blur;
    let work = state.work.as_ref().unwrap();
    let (origin, extent) = camera.viewport.as_ref().map_or((UVec2::ZERO, size), |v| {
        (v.physical_position, v.physical_size)
    });
    let extent = extent.min(size.saturating_sub(origin));
    if extent.x == 0 || extent.y == 0 {
        return;
    }
    let (prev, next) = if parity.0 {
        (&work.index[0], &work.index[1])
    } else {
        (&work.index[1], &work.index[0])
    };

    let mut params = [0u8; 32];
    params[0..4].copy_from_slice(&fx.flash.to_le_bytes());
    params[4..8].copy_from_slice(&(fx.blur as u32 as f32).to_le_bytes());
    params[8..12].copy_from_slice(&(fx.snap as u32 as f32).to_le_bytes());
    // The view's bottom-right corner (window pixels): the blend skips the
    // view's last 32 pixels (`postfx.wgsl`).
    let end = origin + extent;
    params[16..20].copy_from_slice(&(end.x as f32).to_le_bytes());
    params[20..24].copy_from_slice(&(end.y as f32).to_le_bytes());
    let uniform = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("postfx_params"),
        contents: &params,
        usage: BufferUsages::UNIFORM,
    });
    let bind_group = device.create_bind_group(
        "postfx_bind_group",
        &pipeline_cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((
            &work.copy_view,
            &prev.1,
            &lut.texture_view,
            &palette.texture_view,
            &average.texture_view,
            uniform.as_entire_binding(),
        )),
    );

    let encoder = ctx.command_encoder();
    if blank {
        encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("postfx_blank"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &prev.1,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Default::default()),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    let region = Origin3d {
        x: origin.x,
        y: origin.y,
        z: 0,
    };
    encoder.copy_texture_to_texture(
        TexelCopyTextureInfo {
            texture: target.main_texture(),
            mip_level: 0,
            origin: region,
            aspect: TextureAspect::All,
        },
        TexelCopyTextureInfo {
            texture: &work.copy,
            mip_level: 0,
            origin: region,
            aspect: TextureAspect::All,
        },
        Extent3d {
            width: extent.x,
            height: extent.y,
            depth_or_array_layers: 1,
        },
    );
    let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
        label: Some("postfx_pass"),
        color_attachments: &[
            Some(RenderPassColorAttachment {
                view: target.main_texture_view(),
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            }),
            Some(RenderPassColorAttachment {
                view: &next.1,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            }),
        ],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_viewport(
        origin.x as f32,
        origin.y as f32,
        extent.x as f32,
        extent.y as f32,
        0.0,
        1.0,
    );
    pass.set_pipeline(render_pipeline);
    pass.set_bind_group(0, &bind_group, &[]);
    pass.draw(0..3, 0..1);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small palette check against brute force, on a sample of colours.
    #[test]
    fn lut_matches_brute_force() {
        let mut pal = [0u8; 768];
        let mut seed = 12345u32;
        for v in pal.iter_mut() {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            *v = (seed >> 16) as u8;
        }
        // Duplicates and black at 0 and 8, as in the game's palette.
        pal[..3].fill(0);
        pal[24..27].fill(0);
        pal[30..33].copy_from_slice(&[10, 20, 30]);
        pal[60..63].copy_from_slice(&[10, 20, 30]);
        let lut = nearest_lut(&pal);
        let mut seed = 777u32;
        for _ in 0..20000 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            let r = (seed >> 8) as u8 as usize;
            let g = (seed >> 16) as u8 as usize;
            let b = (seed >> 24) as u8 as usize;
            let want = nearest(&pal, [r as i32, g as i32, b as i32]);
            assert_eq!(lut[b * 65536 + g * 256 + r], want, "rgb {r} {g} {b}");
        }
        // Palette colours map to their first entry.
        assert_eq!(lut[30 * 65536 + 20 * 256 + 10], 10);
    }

    /// `CalcAlpha` entries on a small palette.
    #[test]
    fn alpha_formulas() {
        let mut pal = [0u8; 768];
        for i in 0..256 {
            pal[i * 3..i * 3 + 3].copy_from_slice(&[i as u8, (255 - i) as u8, (i / 2) as u8]);
        }
        // Colours 10 and 20 duplicated at 200 and 201.
        pal[600..603].copy_from_slice(&[10, 245, 5]);
        pal[603..606].copy_from_slice(&[10, 245, 5]);
        let lut = nearest_lut(&pal);
        let rgb = |i: u8| [pal[i as usize * 3], pal[i as usize * 3 + 1], pal[i as usize * 3 + 2]];
        // T4: the diagonal returns the index itself, even for a duplicate.
        assert_eq!(alpha_entry(&pal, &lut, 4, 201, 201), 201);
        assert_eq!(alpha_entry(&pal, &lut, 4, 10, 10), 10);
        // T3 lighten: s·255 + (255 − max s)·d, >> 8.
        let (d, s) = (100usize, 40usize);
        let m = 215i32;
        let want: [i32; 3] = std::array::from_fn(|c| {
            (pal[s * 3 + c] as i32 * 255 + (255 - m) * pal[d * 3 + c] as i32) >> 8
        });
        assert_eq!(alpha_entry(&pal, &lut, 3, d, s), nearest(&pal, want));
        // T1 subtract clamps at 0; T2 leaves d alone for texel 0.
        assert_eq!(alpha_entry(&pal, &lut, 1, 50, 60), nearest(&pal, [0, 10, 0]));
        assert_eq!(alpha_entry(&pal, &lut, 2, 50, 0), nearest(&pal, [50, 205, 25]));
    }
}
