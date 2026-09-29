//! Readers for the data files of Carnivores, Carnivores 2 and Carnivores Ice
//! Age.
//!
//! Everything here is plain data: files go in, vectors and structs come out.
//! What the engine then does with it (textures, meshes, sounds) lives in
//! `carn_engine`. The layouts follow the original games' loaders byte for
//! byte.

pub mod car;
pub mod color;
pub mod map;
pub mod model;
pub mod reader;
pub mod rsc;
pub mod tga;
pub mod wav;

pub use reader::{FormatError, Reader, Result};

/// Which engine's file layouts to expect. Ice Age shares Carnivores 2's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Engine {
    /// Carnivores (1998): 512x512 maps, one-byte texture indices.
    C1,
    /// Carnivores 2 and Ice Age: 1024x1024 maps, water bodies, day/night.
    C2,
}

impl Engine {
    /// Cells along each side of an area.
    pub fn map_size(self) -> usize {
        match self {
            Engine::C1 => 512,
            Engine::C2 => 1024,
        }
    }

    /// World units per step of the height maps.
    pub fn height_scale(self) -> f32 {
        match self {
            Engine::C1 => 32.0,
            Engine::C2 => 64.0,
        }
    }
}
