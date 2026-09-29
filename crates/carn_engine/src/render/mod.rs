//! Drawing the world: the ground, water, objects and sky, each with its own
//! small material and shader written to look the way the games did.

pub mod textures;

use bevy::asset::{load_internal_asset, weak_handle};
use bevy::pbr::{DistanceFog, FogFalloff, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::mesh::{MeshVertexAttribute, MeshVertexBufferLayoutRef};
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, ColorWrites,
    CompareFunction, Face, RenderPipelineDescriptor, ShaderRef, SpecializedMeshPipelineError,
    VertexFormat,
};

const COMMON_SHADER: Handle<Shader> = weak_handle!("6c8f1f4e-2b0e-4f7a-9a51-0a4c1d9e7a01");
const TERRAIN_SHADER: Handle<Shader> = weak_handle!("6c8f1f4e-2b0e-4f7a-9a51-0a4c1d9e7a02");
const OBJECT_SHADER: Handle<Shader> = weak_handle!("6c8f1f4e-2b0e-4f7a-9a51-0a4c1d9e7a03");
const WATER_SHADER: Handle<Shader> = weak_handle!("6c8f1f4e-2b0e-4f7a-9a51-0a4c1d9e7a04");
const SKY_SHADER: Handle<Shader> = weak_handle!("6c8f1f4e-2b0e-4f7a-9a51-0a4c1d9e7a05");
const SHADOW_SHADER: Handle<Shader> = weak_handle!("6c8f1f4e-2b0e-4f7a-9a51-0a4c1d9e7a06");
const SHADOW_RESOLVE_SHADER: Handle<Shader> = weak_handle!("6c8f1f4e-2b0e-4f7a-9a51-0a4c1d9e7a07");
const PARTICLE_SHADER: Handle<Shader> = weak_handle!("6c8f1f4e-2b0e-4f7a-9a51-0a4c1d9e7a08");
const RING_SHADER: Handle<Shader> = weak_handle!("6c8f1f4e-2b0e-4f7a-9a51-0a4c1d9e7a09");
const SUN_SHADER: Handle<Shader> = weak_handle!("6c8f1f4e-2b0e-4f7a-9a51-0a4c1d9e7a0a");
const SPRITE_SHADER: Handle<Shader> = weak_handle!("6c8f1f4e-2b0e-4f7a-9a51-0a4c1d9e7a0b");

/// Terrain vertex: x light map value, y random 0..1023, z Carnivores water.
pub const ATTR_TERRAIN: MeshVertexAttribute =
    MeshVertexAttribute::new("CarnTerrain", 0x6361_726e_0001, VertexFormat::Float32x4);
/// A map object that may be drawn as a sprite past the object distance:
/// this bit in its mesh tag (the low byte is its light).
pub const TAG_HAS_SPRITE: u32 = 0x100;

/// Sprite corner: x across, y up from the object's foot, z its light 0..1.
pub const ATTR_SPRITE: MeshVertexAttribute =
    MeshVertexAttribute::new("CarnSprite", 0x6361_726e_0005, VertexFormat::Float32x3);

/// A face's kind for ATTR_OBJECT's second value: 1 if colour-keyed, plus 2
/// if it is seen from the front only (object.wgsl). A weapon's add 4 for
/// the highlights and 8 for the environment map (weapons.rs).
pub fn face_kind(f: &carn_formats::model::Face) -> f32 {
    (f.keyed() as u8 + 2 * f.one_sided() as u8) as f32
}

/// Model vertex: x the vertex's own light, y the face's kind (face_kind).
pub const ATTR_OBJECT: MeshVertexAttribute =
    MeshVertexAttribute::new("CarnObject", 0x6361_726e_0002, VertexFormat::Float32x2);
/// Water vertex: depth, transparency, random, open water.
pub const ATTR_WATER: MeshVertexAttribute =
    MeshVertexAttribute::new("CarnWater", 0x6361_726e_0003, VertexFormat::Float32x4);
/// One value per vertex for the effects: how much light a shadow leaves,
/// or how strongly a ring adds, 0..1.
pub const ATTR_FX: MeshVertexAttribute =
    MeshVertexAttribute::new("CarnFx", 0x6361_726e_0004, VertexFormat::Float32);
/// A weapon vertex's texture coordinates on the highlights (xy) and on the
/// environment map (zw).
pub const ATTR_WEAPON_FX: MeshVertexAttribute =
    MeshVertexAttribute::new("CarnWeaponFx", 0x6361_726e_0006, VertexFormat::Float32x4);

pub struct WorldRenderPlugin;

impl Plugin for WorldRenderPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, COMMON_SHADER, "shaders/common.wgsl", Shader::from_wgsl);
        load_internal_asset!(
            app,
            TERRAIN_SHADER,
            "shaders/terrain.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(app, OBJECT_SHADER, "shaders/object.wgsl", Shader::from_wgsl);
        load_internal_asset!(app, WATER_SHADER, "shaders/water.wgsl", Shader::from_wgsl);
        load_internal_asset!(app, SKY_SHADER, "shaders/sky.wgsl", Shader::from_wgsl);
        load_internal_asset!(app, SHADOW_SHADER, "shaders/shadow.wgsl", Shader::from_wgsl);
        load_internal_asset!(
            app,
            SHADOW_RESOLVE_SHADER,
            "shaders/shadow_resolve.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            PARTICLE_SHADER,
            "shaders/particle.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(app, RING_SHADER, "shaders/ring.wgsl", Shader::from_wgsl);
        load_internal_asset!(app, SUN_SHADER, "shaders/sun.wgsl", Shader::from_wgsl);
        load_internal_asset!(app, SPRITE_SHADER, "shaders/sprite.wgsl", Shader::from_wgsl);
        app.add_plugins((
            MaterialPlugin::<TerrainMaterial> {
                prepass_enabled: false,
                shadows_enabled: false,
                ..default()
            },
            MaterialPlugin::<ObjectMaterial> {
                prepass_enabled: false,
                shadows_enabled: false,
                ..default()
            },
            MaterialPlugin::<WeaponMaterial> {
                prepass_enabled: false,
                shadows_enabled: false,
                ..default()
            },
            MaterialPlugin::<WaterMaterial> {
                prepass_enabled: false,
                shadows_enabled: false,
                ..default()
            },
            MaterialPlugin::<SkyMaterial> {
                prepass_enabled: false,
                shadows_enabled: false,
                ..default()
            },
            MaterialPlugin::<ShadowMaterial> {
                prepass_enabled: false,
                shadows_enabled: false,
                ..default()
            },
            MaterialPlugin::<ShadowResolveMaterial> {
                prepass_enabled: false,
                shadows_enabled: false,
                ..default()
            },
            MaterialPlugin::<ParticleMaterial> {
                prepass_enabled: false,
                shadows_enabled: false,
                ..default()
            },
            MaterialPlugin::<RingMaterial> {
                prepass_enabled: false,
                shadows_enabled: false,
                ..default()
            },
            MaterialPlugin::<SunMaterial> {
                prepass_enabled: false,
                shadows_enabled: false,
                ..default()
            },
            MaterialPlugin::<SpriteMaterial> {
                prepass_enabled: false,
                shadows_enabled: false,
                ..default()
            },
        ));
    }
}

/// The per-frame values every world shader reads (see common.wgsl).
#[derive(Clone, Copy, Debug, Default)]
pub struct Globals {
    /// Horizon colour, 0..1.
    pub fade_rgb: [f32; 3],
    pub fade_start: f32,
    pub fade_end: f32,
    pub water_level: f32,
    pub view_radius: f32,
    pub clouds: bool,
    pub c1: bool,
    pub height_scale: f32,
    pub underwater: bool,
    pub underwater_rgb: [f32; 3],
    /// Fogs drawn at all (the Fog option).
    pub fog: bool,
}

impl Globals {
    pub fn to_fog(&self) -> DistanceFog {
        DistanceFog {
            color: Color::LinearRgba(LinearRgba::new(
                self.fade_rgb[0],
                self.fade_rgb[1],
                self.fade_rgb[2],
                self.fade_end,
            )),
            directional_light_color: Color::LinearRgba(LinearRgba::new(
                self.underwater_rgb[0],
                self.underwater_rgb[1],
                self.underwater_rgb[2],
                if self.underwater { 1.0 } else { 0.0 },
            )),
            directional_light_exponent: if self.fog { 1.0 } else { 0.0 },
            falloff: FogFalloff::Atmospheric {
                extinction: Vec3::new(self.fade_start, self.water_level, self.view_radius),
                inscattering: Vec3::new(
                    if self.clouds { 1.0 } else { 0.0 },
                    if self.c1 { 1.0 } else { 2.0 },
                    self.height_scale,
                ),
            },
        }
    }
}

fn set_layout(
    descriptor: &mut RenderPipelineDescriptor,
    layout: &MeshVertexBufferLayoutRef,
    attrs: &[bevy::render::mesh::VertexAttributeDescriptor],
) -> Result<(), SpecializedMeshPipelineError> {
    let vertex_layout = layout.0.get_layout(attrs)?;
    descriptor.vertex.buffers = vec![vertex_layout];
    Ok(())
}

// ---------------------------------------------------------------------------

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct TerrainMaterial {
    #[texture(0, dimension = "2d_array")]
    #[sampler(1)]
    pub textures: Handle<Image>,
    #[texture(2, sample_type = "u_int")]
    pub fog_map: Handle<Image>,
    #[texture(3, sample_type = "float", filterable = false)]
    pub fog_table: Handle<Image>,
    #[texture(10)]
    #[sampler(11)]
    pub sky: Handle<Image>,
    #[texture(4, sample_type = "u_int")]
    pub cells: Handle<Image>,
    #[texture(5)]
    #[sampler(6)]
    pub skymap: Handle<Image>,
    /// x: texture count, y: the reverse-diagonal flag bit, z: map size,
    /// w: 1 for Carnivores.
    #[uniform(7)]
    pub params: Vec4,
    /// x: cloud shadows on, y: under water.
    #[uniform(8)]
    pub frame: Vec4,
}

impl Material for TerrainMaterial {
    fn vertex_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }
    fn specialize(
        _: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = Some(Face::Back);
        set_layout(
            descriptor,
            layout,
            &[
                Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
                ATTR_TERRAIN.at_shader_location(1),
            ],
        )
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ObjectMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub texture: Handle<Image>,
    #[texture(2, sample_type = "u_int")]
    pub fog_map: Handle<Image>,
    #[texture(3, sample_type = "float", filterable = false)]
    pub fog_table: Handle<Image>,
    #[texture(10)]
    #[sampler(11)]
    pub sky: Handle<Image>,
    /// x: past this distance a map object tagged TAG_HAS_SPRITE gives way
    /// to its sprite (0: never).
    #[uniform(4)]
    pub lod: Vec4,
}

impl Material for ObjectMaterial {
    fn vertex_shader() -> ShaderRef {
        OBJECT_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        OBJECT_SHADER.into()
    }
    fn specialize(
        _: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // One-sided faces are culled face by face in the shader.
        descriptor.primitive.cull_mode = None;
        set_layout(
            descriptor,
            layout,
            &[
                Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
                Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
                ATTR_OBJECT.at_shader_location(2),
            ],
        )
    }
}

/// A weapon in the hands in the second and third games: a model like the
/// others (object.wgsl), with the highlights and the environment
/// map the games added over its shiny faces.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct WeaponMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub texture: Handle<Image>,
    #[texture(2, sample_type = "u_int")]
    pub fog_map: Handle<Image>,
    #[texture(3, sample_type = "float", filterable = false)]
    pub fog_table: Handle<Image>,
    #[texture(10)]
    #[sampler(11)]
    pub sky: Handle<Image>,
    #[uniform(4)]
    pub lod: Vec4,
    /// FX/SPECULAR.TGA and FX/ENVMAP.TGA, holding their values as stored.
    #[texture(5)]
    #[sampler(6)]
    pub specular: Handle<Image>,
    #[texture(7)]
    #[sampler(8)]
    pub envmap: Handle<Image>,
    /// rgb: the highlights' colour as the games stored it - the sky's,
    /// raised by 64.
    #[uniform(9)]
    pub phong: Vec4,
}

impl Material for WeaponMaterial {
    fn vertex_shader() -> ShaderRef {
        OBJECT_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        OBJECT_SHADER.into()
    }
    fn specialize(
        _: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        descriptor.vertex.shader_defs.push("WEAPON_FX".into());
        if let Some(f) = descriptor.fragment.as_mut() {
            f.shader_defs.push("WEAPON_FX".into());
        }
        set_layout(
            descriptor,
            layout,
            &[
                Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
                Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
                ATTR_OBJECT.at_shader_location(2),
                ATTR_WEAPON_FX.at_shader_location(3),
            ],
        )
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct WaterMaterial {
    #[texture(0, dimension = "2d_array")]
    #[sampler(1)]
    pub textures: Handle<Image>,
    #[texture(2, sample_type = "u_int")]
    pub fog_map: Handle<Image>,
    #[texture(3, sample_type = "float", filterable = false)]
    pub fog_table: Handle<Image>,
    #[texture(10)]
    #[sampler(11)]
    pub sky: Handle<Image>,
    #[texture(4, sample_type = "u_int")]
    pub cells: Handle<Image>,
    #[uniform(7)]
    pub params: Vec4,
    #[uniform(8)]
    pub frame: Vec4,
}

impl Material for WaterMaterial {
    fn vertex_shader() -> ShaderRef {
        WATER_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        WATER_SHADER.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
    fn specialize(
        _: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        set_layout(
            descriptor,
            layout,
            &[
                Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
                ATTR_WATER.at_shader_location(1),
            ],
        )
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SkyMaterial {
    #[texture(2, sample_type = "u_int")]
    pub fog_map: Handle<Image>,
    #[texture(3, sample_type = "float", filterable = false)]
    pub fog_table: Handle<Image>,
    #[texture(10)]
    #[sampler(11)]
    pub sky: Handle<Image>,
}

impl Material for SkyMaterial {
    fn vertex_shader() -> ShaderRef {
        SKY_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        SKY_SHADER.into()
    }
    fn specialize(
        _: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        // Behind everything: it leaves the depth clear, so the sun drawn
        // after it (at the farthest depth) still shows over it.
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            ds.depth_write_enabled = false;
        }
        set_layout(
            descriptor,
            layout,
            &[Mesh::ATTRIBUTE_POSITION.at_shader_location(0)],
        )
    }
}

// ---------------------------------------------------------------------------

/// Sorting offsets that put the shadows first among the see-through things
/// (after all the solid ones) and their resolve last.
const SHADOW_FIRST: f32 = -1.0e9;
const RESOLVE_LAST: f32 = 1.0e9;
/// Particles and rings go over the water, as the games drew them after it.
const AFTER_WATER: f32 = 5.0e8;

/// All the creatures' shadows, in one mesh in world space (shadow.wgsl).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct ShadowMaterial {
    #[uniform(0)]
    pub unused: Vec4,
}

impl Material for ShadowMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADOW_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        SHADOW_SHADER.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
    fn depth_bias(&self) -> f32 {
        SHADOW_FIRST
    }
    fn specialize(
        _: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            ds.depth_write_enabled = false;
        }
        // Keep the least light left of every face that covers a pixel.
        let min = BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::One,
            operation: BlendOperation::Min,
        };
        if let Some(t) = descriptor
            .fragment
            .as_mut()
            .and_then(|f| f.targets.first_mut())
            .and_then(|t| t.as_mut())
        {
            t.blend = Some(BlendState {
                color: min,
                alpha: min,
            });
            t.write_mask = ColorWrites::ALPHA;
        }
        set_layout(
            descriptor,
            layout,
            &[
                Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
                ATTR_FX.at_shader_location(1),
            ],
        )
    }
}

/// Darkens the frame by the light the shadows left, each in its own colour
/// (shadow_resolve.wgsl).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct ShadowResolveMaterial {
    #[uniform(0)]
    pub unused: Vec4,
}

impl Material for ShadowResolveMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADOW_RESOLVE_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        SHADOW_RESOLVE_SHADER.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
    fn depth_bias(&self) -> f32 {
        RESOLVE_LAST
    }
    fn specialize(
        _: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            ds.depth_write_enabled = false;
            ds.depth_compare = CompareFunction::Always;
        }
        // frame * light + colour * (1 - light), and the alpha back to 1.
        let blend = BlendState {
            color: BlendComponent {
                src_factor: BlendFactor::OneMinusDstAlpha,
                dst_factor: BlendFactor::DstAlpha,
                operation: BlendOperation::Add,
            },
            alpha: BlendComponent {
                src_factor: BlendFactor::One,
                dst_factor: BlendFactor::Zero,
                operation: BlendOperation::Add,
            },
        };
        if let Some(t) = descriptor
            .fragment
            .as_mut()
            .and_then(|f| f.targets.first_mut())
            .and_then(|t| t.as_mut())
        {
            t.blend = Some(blend);
            t.write_mask = ColorWrites::ALL;
        }
        set_layout(
            descriptor,
            layout,
            &[
                Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
                Mesh::ATTRIBUTE_COLOR.at_shader_location(1),
            ],
        )
    }
}

/// Particles, blood spots and snow (particle.wgsl): one mesh of discs in
/// world space with their colours.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct ParticleMaterial {
    #[uniform(0)]
    pub unused: Vec4,
}

impl Material for ParticleMaterial {
    fn vertex_shader() -> ShaderRef {
        PARTICLE_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        PARTICLE_SHADER.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
    fn depth_bias(&self) -> f32 {
        AFTER_WATER
    }
    fn specialize(
        _: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        set_layout(
            descriptor,
            layout,
            &[
                Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
                Mesh::ATTRIBUTE_COLOR.at_shader_location(1),
            ],
        )
    }
}

/// Adds the colour times its alpha onto the frame, leaving the frame's
/// alpha (the shadows') alone.
fn additive() -> BlendState {
    BlendState {
        color: BlendComponent {
            src_factor: BlendFactor::SrcAlpha,
            dst_factor: BlendFactor::One,
            operation: BlendOperation::Add,
        },
        alpha: BlendComponent {
            src_factor: BlendFactor::Zero,
            dst_factor: BlendFactor::One,
            operation: BlendOperation::Add,
        },
    }
}

/// Rings on the water (ring.wgsl), added onto the frame.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct RingMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub texture: Handle<Image>,
}

impl Material for RingMaterial {
    fn vertex_shader() -> ShaderRef {
        RING_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        RING_SHADER.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
    fn depth_bias(&self) -> f32 {
        AFTER_WATER
    }
    fn specialize(
        _: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        if let Some(t) = descriptor
            .fragment
            .as_mut()
            .and_then(|f| f.targets.first_mut())
            .and_then(|t| t.as_mut())
        {
            t.blend = Some(additive());
        }
        set_layout(
            descriptor,
            layout,
            &[
                Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
                Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
                ATTR_FX.at_shader_location(2),
            ],
        )
    }
}

/// The sun or the moon (sun.wgsl), added onto the sky.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SunMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub texture: Handle<Image>,
    /// x: how strongly it adds, 0..1.
    #[uniform(2)]
    pub params: Vec4,
}

impl Material for SunMaterial {
    fn vertex_shader() -> ShaderRef {
        SUN_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        SUN_SHADER.into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
    fn depth_bias(&self) -> f32 {
        // before the shadows, which come first of the rest
        SHADOW_FIRST * 2.0
    }
    fn specialize(
        _: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        if let Some(t) = descriptor
            .fragment
            .as_mut()
            .and_then(|f| f.targets.first_mut())
            .and_then(|t| t.as_mut())
        {
            t.blend = Some(additive());
        }
        set_layout(
            descriptor,
            layout,
            &[
                Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
                Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
            ],
        )
    }
}

/// Far objects as flat upright sprites (sprite.wgsl).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SpriteMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub texture: Handle<Image>,
    #[texture(2, sample_type = "u_int")]
    pub fog_map: Handle<Image>,
    #[texture(3, sample_type = "float", filterable = false)]
    pub fog_table: Handle<Image>,
    #[texture(10)]
    #[sampler(11)]
    pub sky: Handle<Image>,
    /// x: sprites are drawn only past this distance.
    #[uniform(4)]
    pub lod: Vec4,
}

impl Material for SpriteMaterial {
    fn vertex_shader() -> ShaderRef {
        SPRITE_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        SPRITE_SHADER.into()
    }
    fn specialize(
        _: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        set_layout(
            descriptor,
            layout,
            &[
                Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
                Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
                ATTR_SPRITE.at_shader_location(2),
            ],
        )
    }
}
