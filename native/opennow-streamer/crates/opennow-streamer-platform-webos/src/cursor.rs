use std::sync::atomic::{AtomicU32, Ordering};

/// The game's cursor as the server last described it. It decides the embedder's mouse mode:
/// `Hidden` means the game wants locked relative input; a visible cursor means absolute input
/// with the cursor drawn locally (the video never contains it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteCursorState {
    /// No cursor message yet this session; do not assume relative input.
    Unknown,
    Hidden,
    /// A predefined shape: 1 arrow, 2 I-beam, 3 wait, 4 crosshair, 5 wait-arrow, 6-10 resize,
    /// 12 hand (the server's numbering).
    Predefined(u8),
    /// A server-supplied bitmap; its pixels are not kept, so embedders draw an arrow.
    Custom(u8),
}

const UNKNOWN: u32 = 0;
const HIDDEN: u32 = 1;
const PREDEFINED: u32 = 0x100;
const CUSTOM: u32 = 0x200;

#[derive(Debug, Default)]
pub struct RemoteCursor {
    state: AtomicU32,
}

impl RemoteCursor {
    pub fn state(&self) -> RemoteCursorState {
        match self.state.load(Ordering::Acquire) {
            HIDDEN => RemoteCursorState::Hidden,
            value if value & PREDEFINED != 0 => RemoteCursorState::Predefined(value as u8),
            value if value & CUSTOM != 0 => RemoteCursorState::Custom(value as u8),
            _ => RemoteCursorState::Unknown,
        }
    }

    pub(crate) fn reset(&self) {
        self.state.store(UNKNOWN, Ordering::Release);
    }

    /// Applies an NVST cursor message: byte 0 is the source (0 predefined, 1 custom bitmap),
    /// byte 1 the cursor id. Only predefined id 0 is the protocol's hidden cursor.
    pub fn apply(&self, message: &[u8]) {
        let state = match message {
            [0, 0, ..] => HIDDEN,
            [0, id, ..] => PREDEFINED | u32::from(*id),
            [1, id, ..] => CUSTOM | u32::from(*id),
            _ => return,
        };
        self.state.store(state, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_predefined_zero_hides_the_cursor() {
        let cursor = RemoteCursor::default();
        assert_eq!(cursor.state(), RemoteCursorState::Unknown);
        cursor.apply(&[0, 0, 9, 9]);
        assert_eq!(cursor.state(), RemoteCursorState::Hidden);
        cursor.apply(&[1, 0, 9]);
        assert_eq!(cursor.state(), RemoteCursorState::Custom(0));
        cursor.apply(&[0, 2]);
        assert_eq!(cursor.state(), RemoteCursorState::Predefined(2));
        cursor.apply(&[7, 0]);
        cursor.apply(&[0]);
        assert_eq!(cursor.state(), RemoteCursorState::Predefined(2));
        cursor.reset();
        assert_eq!(cursor.state(), RemoteCursorState::Unknown);
    }
}
