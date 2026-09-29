//! Which of the three games is running.

use carn_formats::Engine;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, bevy::prelude::Resource)]
pub enum GameKind {
    Carnivores,
    Carnivores2,
    IceAge,
}

impl GameKind {
    pub fn engine(self) -> Engine {
        match self {
            GameKind::Carnivores => Engine::C1,
            GameKind::Carnivores2 | GameKind::IceAge => Engine::C2,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            GameKind::Carnivores => "Carnivores",
            GameKind::Carnivores2 => "Carnivores 2",
            GameKind::IceAge => "Carnivores: Ice Age",
        }
    }

    /// Short name for the settings file.
    pub fn id(self) -> &'static str {
        match self {
            GameKind::Carnivores => "carnivores1",
            GameKind::Carnivores2 => "carnivores2",
            GameKind::IceAge => "carnivores-iceage",
        }
    }

    /// Whether the game has the dawn/day/night choice and the brightness
    /// option baked into its textures.
    pub fn has_time_of_day(self) -> bool {
        self.engine() == Engine::C2
    }
}
