//! Water (Reflect faces): the sky picture in screen space, displaced by the
//! `#Water` texel values, with the ripples scrolling 1 byte per game tick
//! (docs/re/rendering.md §3, OS4 `FUN_01071d90`, `FUN_0112ca44`).
//!
//! Hardware renderer (`FUN_010920d8`, `DAT_0114c7d8 != 0`): the face is a
//! textured, Gouraud-shaded quad of the 24-bit background
//! (`Backgrounds/*.chunky2`, resampled to 256x256) at UVs from the screen
//! position plus a sine wobble per corner; see `shaders/water.wgsl`.

use crate::data::GameData;
use crate::city::{ATTRIBUTE_CORNER_LIGHT, ATTRIBUTE_CORNER_POS, ATTRIBUTE_QUAD_ST};
use crate::render_mode::RenderMode;
use crate::world::City;
use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    image::{ImageSampler, ImageSamplerDescriptor},
    mesh::MeshVertexBufferLayoutRef,
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    reflect::TypePath,
    render::render_resource::{
        AsBindGroup, Extent3d, RenderPipelineDescriptor, SpecializedMeshPipelineError,
        TextureDimension, TextureFormat,
    },
    shader::ShaderRef,
};
use payback_formats::IndexedImage;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct WaterMaterial {
    /// x = scroll offset in bytes (0..63), y = 1 in the rotating view,
    /// z = 1 for the hardware renderer.
    #[uniform(0)]
    pub params: Vec4,
    /// Cube texture atlas of raw palette indices.
    #[texture(1)]
    pub index_atlas: Handle<Image>,
    /// Background rescaled to 320x266, as in `FUN_0106e564`; for the
    /// hardware renderer the 24-bit `.chunky2` background at 256x256
    /// ([`load_sky_hw`]).
    #[texture(2)]
    pub sky: Handle<Image>,
}

impl Material for WaterMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://payback/shaders/water.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://payback/shaders/water.wgsl".into()
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Game views: gamma-encoded output (`crate::camera::postfx::GammaWorld`).
        if crate::objects::gamma_view(key.mesh_key)
            && let Some(fragment) = descriptor.fragment.as_mut()
        {
            fragment.shader_defs.push("GAMMA_TARGET".into());
        }
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_1.at_shader_location(2),
            ATTRIBUTE_QUAD_ST.at_shader_location(3),
            ATTRIBUTE_CORNER_POS[0].at_shader_location(4),
            ATTRIBUTE_CORNER_POS[1].at_shader_location(5),
            ATTRIBUTE_CORNER_POS[2].at_shader_location(6),
            ATTRIBUTE_CORNER_POS[3].at_shader_location(7),
            ATTRIBUTE_CORNER_LIGHT[0].at_shader_location(8),
            ATTRIBUTE_CORNER_LIGHT[1].at_shader_location(9),
            ATTRIBUTE_CORNER_LIGHT[2].at_shader_location(10),
            ATTRIBUTE_CORNER_LIGHT[3].at_shader_location(11),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

/// Handle of the level's water material, for the scroll update.
#[derive(Resource)]
pub struct Water(pub Handle<WaterMaterial>);

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/water.wgsl");
        app.add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .add_systems(FixedUpdate, scroll_water)
            .add_systems(Update, (rotating_water, hardware_water));
    }
}

/// Follow the renderer: the shader path and the background picture.
fn hardware_water(
    water: Option<Res<Water>>,
    city: Option<Res<City>>,
    mode: Res<RenderMode>,
    data: Res<GameData>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut done: Local<Option<bool>>,
) {
    let (Some(water), Some(city)) = (water, city) else {
        *done = None;
        return;
    };
    let hardware = mode.hardware();
    let fresh = water.is_added() || city.is_added();
    if !fresh && *done == Some(hardware) {
        return;
    }
    let Some(sky) = materials.get(&water.0).map(|m| m.sky.clone()) else {
        return;
    };
    // The level loader made the software picture.
    if hardware || (!fresh && *done == Some(true)) {
        let image = if hardware {
            load_sky_hw(&data, city.map.night)
        } else {
            load_sky(&data, city.map.night)
        };
        if let Some(mut slot) = images.get_mut(&sky) {
            *slot = image;
        }
    }
    let z = if hardware { 1.0 } else { 0.0 };
    if let Some(mut m) = materials.get_mut(&water.0) {
        m.params.z = z;
    }
    *done = Some(hardware);
}

/// The hardware background (`FUN_0106e564` with `DAT_0114c7d8 != 0`):
/// `Clouds.chunky2` / `Stars.chunky2`, 320x256 24-bit RGB, converted to a
/// 256x256 W3D texture by `FUN_010024fc` (nearest resampling to the
/// power-of-two size). Falls back to the 8-bit picture.
pub fn load_sky_hw(data: &GameData, night: bool) -> Image {
    const W: usize = 320;
    const H: usize = 256;
    const N: usize = 256;
    let name = if night { "Stars" } else { "Clouds" };
    let rgb: Vec<u8> = match data.read(format!("Backgrounds/{name}.chunky2")) {
        Ok(b) if b.len() >= W * H * 3 => b,
        _ => {
            warn!("missing background {name}.chunky2");
            let src = data
                .read(format!("Backgrounds/{name}.chunky"))
                .ok()
                .and_then(|b| IndexedImage::from_chunky(&b, W, H).ok());
            let mut out = vec![0u8; W * H * 3];
            if let Some(src) = src {
                for (i, &c) in src.pixels.iter().enumerate().take(W * H) {
                    out[i * 3..i * 3 + 3].copy_from_slice(&data.palette[c as usize * 3..c as usize * 3 + 3]);
                }
            }
            out
        }
    };
    let mut rgba = vec![255u8; N * N * 4];
    for y in 0..N {
        let sy = y * H / N;
        for x in 0..N {
            let sx = x * W / N;
            let p = (sy * W + sx) * 3;
            rgba[(y * N + x) * 4..(y * N + x) * 4 + 3].copy_from_slice(&rgb[p..p + 3]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: N as u32,
            height: N as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::nearest());
    image
}

/// The rotating view reads the background differently (`water.wgsl`).
fn rotating_water(
    water: Option<Res<Water>>,
    rotating: Res<crate::camera::RotatingView>,
    mut materials: ResMut<Assets<WaterMaterial>>,
) {
    let Some(water) = water else { return };
    let y = if rotating.0 { 1.0 } else { 0.0 };
    if materials.get(&water.0).is_some_and(|m| m.params.y != y)
        && let Some(mut m) = materials.get_mut(&water.0)
    {
        m.params.y = y;
    }
}

/// `scroll = (scroll - ticks) mod 64`, once per game tick.
fn scroll_water(water: Option<Res<Water>>, mut materials: ResMut<Assets<WaterMaterial>>) {
    let Some(water) = water else { return };
    if let Some(mut m) = materials.get_mut(&water.0) {
        m.params.x = (m.params.x - 1.0).rem_euclid(64.0);
    }
}

/// `Backgrounds/Clouds.chunky` by day, `Stars.chunky` at night, rescaled
/// (nearest) from 320x256 to 320x266.
pub fn load_sky(data: &GameData, night: bool) -> Image {
    const W: usize = 320;
    const H: usize = 256;
    const OUT_H: usize = H + 10;
    let name = if night { "Stars" } else { "Clouds" };
    let src = data
        .read(format!("Backgrounds/{name}.chunky"))
        .ok()
        .and_then(|b| IndexedImage::from_chunky(&b, W, H).ok());
    let mut rgba = vec![0u8; W * OUT_H * 4];
    if let Some(src) = src {
        for y in 0..OUT_H {
            let sy = y * H / OUT_H;
            for x in 0..W {
                let c = src.pixels[sy * W + x] as usize;
                let p = (y * W + x) * 4;
                rgba[p..p + 3].copy_from_slice(&data.palette[c * 3..c * 3 + 3]);
                rgba[p + 3] = 255;
            }
        }
    } else {
        warn!("missing background {name}");
    }
    let mut image = Image::new(
        Extent3d {
            width: W as u32,
            height: OUT_H as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::nearest());
    image
}
