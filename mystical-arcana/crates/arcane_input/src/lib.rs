//! arcane_input — keyboard / mouse / pointer abstraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InputAction { Move, Look, Jump, Cast, Interact }
