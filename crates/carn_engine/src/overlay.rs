//! Models drawn over the world: the weapon in the hunter's hands, the
//! binoculars, and the compass and wind gauge in the lower corners.
//!
//! Each gets a camera of its own that draws only it, after the world, with
//! the originals' projection for these models: the 4:3 one with
//! tan(half vertical view) = 0.6 whatever the field-of-view option says, so
//! they keep their size and place. The compass and wind gauge were drawn
//! with the centre of projection moved into their corner of the screen,
//! which a projection with an offset centre reproduces exactly.

use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::math::Vec3A;
use bevy::prelude::*;
use bevy::render::camera::{CameraProjection, ClearColorConfig, SubCameraView};
use bevy::render::view::RenderLayers;

use crate::MainCamera;

/// The originals' projection scale for near models: 1 / tan(half vfov).
pub const NEAR_K: f32 = 1.0 / 0.6;

#[derive(Debug, Clone)]
pub struct OverlayProjection {
    /// Horizontal scale; None for square pixels (ys / aspect).
    pub xs: Option<f32>,
    pub ys: f32,
    /// Centre of projection on the screen, 0..1 from the left and top.
    pub cx: f32,
    pub cy: f32,
    pub near: f32,
    aspect: f32,
}

impl OverlayProjection {
    pub fn centred() -> Self {
        OverlayProjection {
            xs: None,
            ys: NEAR_K,
            cx: 0.5,
            cy: 0.5,
            near: 4.0,
            aspect: 4.0 / 3.0,
        }
    }
    pub fn at(cx: f32, cy: f32) -> Self {
        OverlayProjection {
            cx,
            cy,
            ..Self::centred()
        }
    }
    fn x_scale(&self) -> f32 {
        self.xs.unwrap_or(self.ys / self.aspect)
    }
}

impl CameraProjection for OverlayProjection {
    fn get_clip_from_view(&self) -> Mat4 {
        let ox = self.cx * 2.0 - 1.0;
        let oy = 1.0 - self.cy * 2.0;
        // Bevy's infinite reverse-z perspective, with the centre moved.
        Mat4::from_cols(
            Vec4::new(self.x_scale(), 0.0, 0.0, 0.0),
            Vec4::new(0.0, self.ys, 0.0, 0.0),
            Vec4::new(-ox, -oy, 0.0, -1.0),
            Vec4::new(0.0, 0.0, self.near, 0.0),
        )
    }
    fn get_clip_from_view_for_sub(&self, _sub: &SubCameraView) -> Mat4 {
        self.get_clip_from_view()
    }
    fn update(&mut self, width: f32, height: f32) {
        if height > 0.0 {
            self.aspect = width / height;
        }
    }
    fn far(&self) -> f32 {
        10000.0
    }
    fn get_frustum_corners(&self, z_near: f32, z_far: f32) -> [Vec3A; 8] {
        // Only culling reads these, and every overlay model opts out of it.
        let c = |z: f32, x: f32, y: f32| Vec3A::new(x * z, y * z, -z);
        let (xs, ys) = (1.0 / self.x_scale(), 1.0 / self.ys);
        let n = z_near.abs();
        let f = z_far.abs();
        [
            c(n, xs, -ys),
            c(n, xs, ys),
            c(n, -xs, ys),
            c(n, -xs, -ys),
            c(f, xs, -ys),
            c(f, xs, ys),
            c(f, -xs, ys),
            c(f, -xs, -ys),
        ]
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Overlay {
    /// The weapon or the binoculars.
    Hands,
    Compass,
    Wind,
}

impl Overlay {
    pub fn layer(self) -> usize {
        match self {
            Overlay::Hands => 1,
            Overlay::Compass => 2,
            Overlay::Wind => 3,
        }
    }
}

pub fn setup(mut commands: Commands) {
    for (o, order, proj) in [
        (Overlay::Hands, 1, OverlayProjection::centred()),
        // The games' centres: a fifth in from the side, and ten
        // twenty-thirds of the height up from the bottom.
        (Overlay::Wind, 2, OverlayProjection::at(0.2, 13.0 / 23.0)),
        (Overlay::Compass, 3, OverlayProjection::at(0.8, 13.0 / 23.0)),
    ] {
        commands.spawn((
            Camera3d::default(),
            Camera {
                order,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            Projection::custom(proj),
            Tonemapping::None,
            Msaa::Off,
            crate::render::Globals::default().to_fog(),
            RenderLayers::layer(o.layer()),
            Transform::IDENTITY,
            o,
        ));
    }
}

/// The overlay cameras stand where the eye is (so the models are lit and
/// fogged as things beside the hunter are) and share its per-frame values
/// and anti-aliasing.
#[allow(clippy::type_complexity)]
pub fn follow(
    main: Query<(&Transform, &bevy::pbr::DistanceFog, &Msaa), (With<MainCamera>, Without<Overlay>)>,
    mut overlays: Query<(&mut Transform, &mut bevy::pbr::DistanceFog, &mut Msaa), With<Overlay>>,
) {
    let Ok((t, fog, msaa)) = main.single() else {
        return;
    };
    for (mut ot, mut of, mut om) in overlays.iter_mut() {
        *ot = *t;
        *of = fog.clone();
        if *om != *msaa {
            *om = *msaa;
        }
    }
}

/// Switches the hands camera between the plain projection and the one the
/// binoculars use, which stretches their 4:3 frame to the screen's width.
pub fn set_hands_stretch(cams: &mut Query<(&Overlay, &mut Projection)>, stretch: bool) {
    for (o, mut p) in cams.iter_mut() {
        if *o != Overlay::Hands {
            continue;
        }
        let want = if stretch { Some(1.25f32) } else { None };
        if let Projection::Custom(c) = p.as_mut() {
            if let Some(op) = c.get_mut::<OverlayProjection>() {
                if op.xs != want {
                    op.xs = want;
                }
            }
        }
    }
}
