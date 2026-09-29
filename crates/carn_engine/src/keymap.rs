//! The controls the player can rebind (KeyMap), in the order the games'
//! Controls panel lists them. A binding is a key or a mouse button.

use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseButton;
use bevy::input::ButtonInput;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Binding {
    None,
    Key(KeyCode),
    Mouse(MouseButton),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    Forward,
    Backward,
    TurnUp,
    TurnDown,
    TurnLeft,
    TurnRight,
    Fire,
    GetWeapon,
    StepLeft,
    StepRight,
    Strafe,
    Jump,
    Run,
    Crouch,
    Call,
    ChangeCall,
    Binoculars,
}

pub const ACTS: [Act; 17] = [
    Act::Forward,
    Act::Backward,
    Act::TurnUp,
    Act::TurnDown,
    Act::TurnLeft,
    Act::TurnRight,
    Act::Fire,
    Act::GetWeapon,
    Act::StepLeft,
    Act::StepRight,
    Act::Strafe,
    Act::Jump,
    Act::Run,
    Act::Crouch,
    Act::Call,
    Act::ChangeCall,
    Act::Binoculars,
];

impl Act {
    pub fn label(self) -> &'static str {
        match self {
            Act::Forward => "Forward",
            Act::Backward => "Backward",
            Act::TurnUp => "Turn Up",
            Act::TurnDown => "Turn Down",
            Act::TurnLeft => "Turn Left",
            Act::TurnRight => "Turn Right",
            Act::Fire => "Fire",
            Act::GetWeapon => "Get weapon",
            Act::StepLeft => "Step Left",
            Act::StepRight => "Step Right",
            Act::Strafe => "Strafe",
            Act::Jump => "Jump",
            Act::Run => "Run",
            Act::Crouch => "Crouch",
            Act::Call => "Call",
            Act::ChangeCall => "Change Call",
            Act::Binoculars => "Binoculars",
        }
    }

    /// The name in the options file.
    pub fn key(self) -> &'static str {
        match self {
            Act::Forward => "forward",
            Act::Backward => "backward",
            Act::TurnUp => "turn_up",
            Act::TurnDown => "turn_down",
            Act::TurnLeft => "turn_left",
            Act::TurnRight => "turn_right",
            Act::Fire => "fire",
            Act::GetWeapon => "get_weapon",
            Act::StepLeft => "step_left",
            Act::StepRight => "step_right",
            Act::Strafe => "strafe",
            Act::Jump => "jump",
            Act::Run => "run",
            Act::Crouch => "crouch",
            Act::Call => "call",
            Act::ChangeCall => "change_call",
            Act::Binoculars => "binoculars",
        }
    }
}

/// Keys and their names, for the options panel and the options file.
const NAMES: &[(KeyCode, &str)] = &[
    (KeyCode::KeyA, "A"),
    (KeyCode::KeyB, "B"),
    (KeyCode::KeyC, "C"),
    (KeyCode::KeyD, "D"),
    (KeyCode::KeyE, "E"),
    (KeyCode::KeyF, "F"),
    (KeyCode::KeyG, "G"),
    (KeyCode::KeyH, "H"),
    (KeyCode::KeyI, "I"),
    (KeyCode::KeyJ, "J"),
    (KeyCode::KeyK, "K"),
    (KeyCode::KeyL, "L"),
    (KeyCode::KeyM, "M"),
    (KeyCode::KeyN, "N"),
    (KeyCode::KeyO, "O"),
    (KeyCode::KeyP, "P"),
    (KeyCode::KeyQ, "Q"),
    (KeyCode::KeyR, "R"),
    (KeyCode::KeyS, "S"),
    (KeyCode::KeyT, "T"),
    (KeyCode::KeyU, "U"),
    (KeyCode::KeyV, "V"),
    (KeyCode::KeyW, "W"),
    (KeyCode::KeyX, "X"),
    (KeyCode::KeyY, "Y"),
    (KeyCode::KeyZ, "Z"),
    (KeyCode::Digit0, "0"),
    (KeyCode::Digit1, "1"),
    (KeyCode::Digit2, "2"),
    (KeyCode::Digit3, "3"),
    (KeyCode::Digit4, "4"),
    (KeyCode::Digit5, "5"),
    (KeyCode::Digit6, "6"),
    (KeyCode::Digit7, "7"),
    (KeyCode::Digit8, "8"),
    (KeyCode::Digit9, "9"),
    (KeyCode::Space, "Space"),
    (KeyCode::ShiftLeft, "Shift"),
    (KeyCode::ShiftRight, "Right Shift"),
    (KeyCode::ControlLeft, "Ctrl"),
    (KeyCode::ControlRight, "Right Ctrl"),
    (KeyCode::AltLeft, "Alt"),
    (KeyCode::AltRight, "Right Alt"),
    (KeyCode::Tab, "Tab"),
    (KeyCode::Enter, "Enter"),
    (KeyCode::Backspace, "Backspace"),
    (KeyCode::CapsLock, "Caps Lock"),
    (KeyCode::ArrowUp, "Up"),
    (KeyCode::ArrowDown, "Down"),
    (KeyCode::ArrowLeft, "Left"),
    (KeyCode::ArrowRight, "Right"),
    (KeyCode::Insert, "Insert"),
    (KeyCode::Delete, "Delete"),
    (KeyCode::Home, "Home"),
    (KeyCode::End, "End"),
    (KeyCode::PageUp, "Page Up"),
    (KeyCode::PageDown, "Page Down"),
    (KeyCode::Comma, ","),
    (KeyCode::Period, "."),
    (KeyCode::Slash, "/"),
    (KeyCode::Semicolon, ";"),
    (KeyCode::Quote, "'"),
    (KeyCode::BracketLeft, "["),
    (KeyCode::BracketRight, "]"),
    (KeyCode::Backslash, "\\"),
    (KeyCode::Backquote, "`"),
    (KeyCode::Minus, "-"),
    (KeyCode::Equal, "="),
    (KeyCode::Numpad0, "Num 0"),
    (KeyCode::Numpad1, "Num 1"),
    (KeyCode::Numpad2, "Num 2"),
    (KeyCode::Numpad3, "Num 3"),
    (KeyCode::Numpad4, "Num 4"),
    (KeyCode::Numpad5, "Num 5"),
    (KeyCode::Numpad6, "Num 6"),
    (KeyCode::Numpad7, "Num 7"),
    (KeyCode::Numpad8, "Num 8"),
    (KeyCode::Numpad9, "Num 9"),
    (KeyCode::NumpadAdd, "Num +"),
    (KeyCode::NumpadSubtract, "Num -"),
    (KeyCode::NumpadMultiply, "Num *"),
    (KeyCode::NumpadDivide, "Num /"),
    (KeyCode::NumpadEnter, "Num Enter"),
    (KeyCode::NumpadDecimal, "Num ."),
    (KeyCode::F1, "F1"),
    (KeyCode::F2, "F2"),
    (KeyCode::F3, "F3"),
    (KeyCode::F4, "F4"),
    (KeyCode::F5, "F5"),
    (KeyCode::F6, "F6"),
    (KeyCode::F7, "F7"),
    (KeyCode::F8, "F8"),
    (KeyCode::F9, "F9"),
    (KeyCode::F10, "F10"),
    (KeyCode::F11, "F11"),
];

impl Binding {
    pub fn name(self) -> String {
        match self {
            Binding::None => "...".into(),
            Binding::Mouse(MouseButton::Left) => "Mouse1".into(),
            Binding::Mouse(MouseButton::Right) => "Mouse2".into(),
            Binding::Mouse(MouseButton::Middle) => "Mouse3".into(),
            Binding::Mouse(MouseButton::Back) => "Mouse4".into(),
            Binding::Mouse(MouseButton::Forward) => "Mouse5".into(),
            Binding::Mouse(MouseButton::Other(n)) => format!("Mouse{}", n + 1),
            Binding::Key(k) => NAMES
                .iter()
                .find(|(c, _)| *c == k)
                .map(|(_, n)| n.to_string())
                .unwrap_or_else(|| format!("{k:?}")),
        }
    }

    pub fn parse(s: &str) -> Binding {
        let s = s.trim();
        match s {
            "" | "..." | "none" => return Binding::None,
            "Mouse1" => return Binding::Mouse(MouseButton::Left),
            "Mouse2" => return Binding::Mouse(MouseButton::Right),
            "Mouse3" => return Binding::Mouse(MouseButton::Middle),
            "Mouse4" => return Binding::Mouse(MouseButton::Back),
            "Mouse5" => return Binding::Mouse(MouseButton::Forward),
            _ => {}
        }
        NAMES
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case(s))
            .map(|(c, _)| Binding::Key(*c))
            .unwrap_or(Binding::None)
    }

    /// A Windows virtual-key code, as the games' key map stored bindings.
    pub fn from_vk(vk: i32) -> Binding {
        use KeyCode as K;
        let k = match vk {
            0 => return Binding::None,
            1 => return Binding::Mouse(MouseButton::Left),
            2 => return Binding::Mouse(MouseButton::Right),
            4 => return Binding::Mouse(MouseButton::Middle),
            5 => return Binding::Mouse(MouseButton::Back),
            6 => return Binding::Mouse(MouseButton::Forward),
            0x41..=0x5A => {
                const L: [KeyCode; 26] = [
                    K::KeyA,
                    K::KeyB,
                    K::KeyC,
                    K::KeyD,
                    K::KeyE,
                    K::KeyF,
                    K::KeyG,
                    K::KeyH,
                    K::KeyI,
                    K::KeyJ,
                    K::KeyK,
                    K::KeyL,
                    K::KeyM,
                    K::KeyN,
                    K::KeyO,
                    K::KeyP,
                    K::KeyQ,
                    K::KeyR,
                    K::KeyS,
                    K::KeyT,
                    K::KeyU,
                    K::KeyV,
                    K::KeyW,
                    K::KeyX,
                    K::KeyY,
                    K::KeyZ,
                ];
                L[(vk - 0x41) as usize]
            }
            0x30..=0x39 => {
                const D: [KeyCode; 10] = [
                    K::Digit0,
                    K::Digit1,
                    K::Digit2,
                    K::Digit3,
                    K::Digit4,
                    K::Digit5,
                    K::Digit6,
                    K::Digit7,
                    K::Digit8,
                    K::Digit9,
                ];
                D[(vk - 0x30) as usize]
            }
            0x60..=0x69 => {
                const N: [KeyCode; 10] = [
                    K::Numpad0,
                    K::Numpad1,
                    K::Numpad2,
                    K::Numpad3,
                    K::Numpad4,
                    K::Numpad5,
                    K::Numpad6,
                    K::Numpad7,
                    K::Numpad8,
                    K::Numpad9,
                ];
                N[(vk - 0x60) as usize]
            }
            0x70..=0x7A => {
                const F: [KeyCode; 11] = [
                    K::F1,
                    K::F2,
                    K::F3,
                    K::F4,
                    K::F5,
                    K::F6,
                    K::F7,
                    K::F8,
                    K::F9,
                    K::F10,
                    K::F11,
                ];
                F[(vk - 0x70) as usize]
            }
            0x08 => K::Backspace,
            0x09 => K::Tab,
            0x0D => K::Enter,
            0x10 | 0xA0 => K::ShiftLeft,
            0xA1 => K::ShiftRight,
            0x11 | 0xA2 => K::ControlLeft,
            0xA3 => K::ControlRight,
            0x12 | 0xA4 => K::AltLeft,
            0xA5 => K::AltRight,
            0x14 => K::CapsLock,
            0x20 => K::Space,
            0x21 => K::PageUp,
            0x22 => K::PageDown,
            0x23 => K::End,
            0x24 => K::Home,
            0x25 => K::ArrowLeft,
            0x26 => K::ArrowUp,
            0x27 => K::ArrowRight,
            0x28 => K::ArrowDown,
            0x2D => K::Insert,
            0x2E => K::Delete,
            0x6A => K::NumpadMultiply,
            0x6B => K::NumpadAdd,
            0x6D => K::NumpadSubtract,
            0x6E => K::NumpadDecimal,
            0x6F => K::NumpadDivide,
            0xBA => K::Semicolon,
            0xBB => K::Equal,
            0xBC => K::Comma,
            0xBD => K::Minus,
            0xBE => K::Period,
            0xBF => K::Slash,
            0xC0 => K::Backquote,
            0xDB => K::BracketLeft,
            0xDC => K::Backslash,
            0xDD => K::BracketRight,
            0xDE => K::Quote,
            _ => return Binding::None,
        };
        Binding::Key(k)
    }

    /// Whether a key is one the panel can bind (Esc cancels a rebind).
    pub fn bindable(k: KeyCode) -> bool {
        NAMES.iter().any(|(c, _)| *c == k)
    }
}

/// The two halves of a modifier count as one key, as the games' did.
fn twin(k: KeyCode) -> Option<KeyCode> {
    match k {
        KeyCode::ShiftLeft => Some(KeyCode::ShiftRight),
        KeyCode::ControlLeft => Some(KeyCode::ControlRight),
        KeyCode::AltLeft => Some(KeyCode::AltRight),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyMap(pub [Binding; 17]);

impl Default for KeyMap {
    /// A WASD layout; Strafe unbound.
    fn default() -> Self {
        use Binding::{Key as K, Mouse as M};
        KeyMap([
            K(KeyCode::KeyW),
            K(KeyCode::KeyS),
            K(KeyCode::ArrowUp),
            K(KeyCode::ArrowDown),
            K(KeyCode::ArrowLeft),
            K(KeyCode::ArrowRight),
            M(MouseButton::Left),
            M(MouseButton::Right),
            K(KeyCode::KeyA),
            K(KeyCode::KeyD),
            Binding::None,
            K(KeyCode::Space),
            K(KeyCode::KeyQ),
            K(KeyCode::ShiftLeft),
            K(KeyCode::KeyF),
            K(KeyCode::KeyC),
            K(KeyCode::KeyB),
        ])
    }
}

/// The input a frame brought.
pub struct Input<'a> {
    pub keys: &'a ButtonInput<KeyCode>,
    pub mouse: &'a ButtonInput<MouseButton>,
}

impl KeyMap {
    pub fn get(&self, a: Act) -> Binding {
        self.0[ACTS.iter().position(|&x| x == a).unwrap_or(0)]
    }

    pub fn set(&mut self, a: Act, b: Binding) {
        if let Some(i) = ACTS.iter().position(|&x| x == a) {
            self.0[i] = b;
        }
    }

    pub fn held(&self, a: Act, i: &Input) -> bool {
        match self.get(a) {
            Binding::None => false,
            Binding::Key(k) => {
                i.keys.pressed(k) || twin(k).map(|t| i.keys.pressed(t)).unwrap_or(false)
            }
            Binding::Mouse(m) => i.mouse.pressed(m),
        }
    }

    pub fn pressed(&self, a: Act, i: &Input) -> bool {
        match self.get(a) {
            Binding::None => false,
            Binding::Key(k) => {
                i.keys.just_pressed(k) || twin(k).map(|t| i.keys.just_pressed(t)).unwrap_or(false)
            }
            Binding::Mouse(m) => i.mouse.just_pressed(m),
        }
    }
}
