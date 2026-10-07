//! Water (Reflect faces): the sky picture in screen space, displaced by the
//! `#Water` texel values, with the ripples scrolling 1 byte per game tick
//! (docs/re/rendering.md §3, OS4 `FUN_01071d90`, `FUN_0112ca44`).

use crate::data::GameData;
use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    image::{ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat},
    shader::ShaderRef,
};
use payback_formats::IndexedImage;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct WaterMaterial {
    /// x = scroll offset in bytes (0..63).
    #[uniform(0)]
    pub params: Vec4,
    /// Cube texture atlas of raw palette indices.
    #[texture(1)]
    pub index_atlas: Handle<Image>,
    /// Background rescaled to 320x266, as in `FUN_0106e564`.
    #[texture(2)]
    pub sky: Handle<Image>,
}

impl Material for WaterMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://payback/shaders/water.wgsl".into()
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
            .add_systems(FixedUpdate, scroll_water);
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
