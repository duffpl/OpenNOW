#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutChord {
    pub virtual_key: u16,
    pub modifiers: u16,
}

impl ShortcutChord {
    pub fn parse(value: &str) -> Option<Self> {
        let mut modifiers = 0_u16;
        let mut virtual_key = None;
        for part in value.split('+').map(str::trim) {
            if part.is_empty() {
                return None;
            }
            let normalized = part.to_ascii_lowercase();
            let modifier = match normalized.as_str() {
                "shift" => Some(0x01),
                "ctrl" | "control" => Some(0x02),
                "alt" | "option" => Some(0x04),
                "meta" | "command" | "cmd" | "super" | "win" => Some(0x08),
                key if virtual_key.is_none() => {
                    virtual_key = shortcut_virtual_key(key);
                    None
                }
                _ => return None,
            };
            if let Some(modifier) = modifier {
                if modifiers & modifier != 0 {
                    return None;
                }
                modifiers |= modifier;
            }
        }
        virtual_key.map(|virtual_key| Self {
            virtual_key,
            modifiers,
        })
    }
}

fn shortcut_virtual_key(value: &str) -> Option<u16> {
    if value.len() == 1 {
        let byte = value.as_bytes()[0].to_ascii_uppercase();
        if byte.is_ascii_alphanumeric() {
            return Some(u16::from(byte));
        }
    }
    if let Some(number) = value
        .strip_prefix('f')
        .and_then(|number| number.parse::<u16>().ok())
        && (1..=24).contains(&number)
    {
        return Some(0x6f + number);
    }
    Some(match value {
        "enter" | "return" => 0x0d,
        "escape" | "esc" => 0x1b,
        "backspace" => 0x08,
        "tab" => 0x09,
        "space" => 0x20,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "insert" => 0x2d,
        "delete" | "del" => 0x2e,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" | "pgup" => 0x21,
        "pagedown" | "pgdown" => 0x22,
        "printscreen" | "print" => 0x2a,
        "pause" => 0x13,
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamShortcutAction {
    ToggleStats,
    TogglePointerLock,
    ToggleFullscreen,
    StopStream,
    ToggleAntiAfk,
    Screenshot,
    ToggleRecording,
    SaveClip,
}

impl StreamShortcutAction {
    pub const fn protocol_name(self) -> &'static str {
        match self {
            Self::ToggleStats => "toggle-stats",
            Self::TogglePointerLock => "toggle-pointer-lock",
            Self::ToggleFullscreen => "toggle-fullscreen",
            Self::StopStream => "stop-stream",
            Self::ToggleAntiAfk => "toggle-anti-afk",
            Self::Screenshot => "screenshot",
            Self::ToggleRecording => "toggle-recording",
            Self::SaveClip => "save-clip",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamShortcutBindings {
    bindings: [(StreamShortcutAction, Option<ShortcutChord>); 8],
}

impl StreamShortcutBindings {
    pub fn from_json(value: &serde_json::Value) -> Self {
        let read = |key: &str, fallback: &str| match value
            .get(key)
            .and_then(serde_json::Value::as_str)
        {
            Some("") => None,
            Some(chord) => ShortcutChord::parse(chord).or_else(|| ShortcutChord::parse(fallback)),
            None => ShortcutChord::parse(fallback),
        };
        Self {
            bindings: [
                (
                    StreamShortcutAction::ToggleStats,
                    read("toggleStats", "Ctrl+N"),
                ),
                (
                    StreamShortcutAction::TogglePointerLock,
                    read("togglePointerLock", "F8"),
                ),
                (
                    StreamShortcutAction::ToggleFullscreen,
                    read("toggleFullscreen", "F11"),
                ),
                (
                    StreamShortcutAction::StopStream,
                    read("stopStream", "Ctrl+Shift+Q"),
                ),
                (
                    StreamShortcutAction::ToggleAntiAfk,
                    read("toggleAntiAfk", "Ctrl+Shift+K"),
                ),
                (
                    StreamShortcutAction::Screenshot,
                    read("screenshot", "Ctrl+F11"),
                ),
                (
                    StreamShortcutAction::ToggleRecording,
                    read("toggleRecording", "F12"),
                ),
                (StreamShortcutAction::SaveClip, read("saveClip", "Ctrl+F12")),
            ],
        }
    }

    pub const fn action(self, virtual_key: u16, modifiers: u16) -> Option<StreamShortcutAction> {
        let mut index = 0;
        while index < self.bindings.len() {
            let (action, chord) = self.bindings[index];
            if let Some(chord) = chord
                && chord.virtual_key == virtual_key
                && chord.modifiers == modifiers
            {
                return Some(action);
            }
            index += 1;
        }
        None
    }
}

impl Default for StreamShortcutBindings {
    fn default() -> Self {
        Self::from_json(&serde_json::Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_chords_parse_supported_keys_and_reject_ambiguous_input() {
        assert_eq!(
            ShortcutChord::parse("Ctrl+Shift+Q"),
            Some(ShortcutChord {
                virtual_key: u16::from(b'Q'),
                modifiers: 0x03,
            })
        );
        assert_eq!(
            ShortcutChord::parse("command+f24"),
            Some(ShortcutChord {
                virtual_key: 0x87,
                modifiers: 0x08,
            })
        );
        assert_eq!(ShortcutChord::parse("Ctrl"), None);
        assert_eq!(ShortcutChord::parse("Ctrl+Control+Q"), None);
        assert_eq!(ShortcutChord::parse("Ctrl+Q+W"), None);
        assert_eq!(ShortcutChord::parse("Ctrl+"), None);
        assert_eq!(ShortcutChord::parse("F25"), None);
    }

    #[test]
    fn shortcut_bindings_use_custom_values_defaults_and_stable_conflict_order() {
        let bindings = StreamShortcutBindings::from_json(&serde_json::json!({
            "toggleStats":"Alt+S",
            "togglePointerLock":"Alt+S",
            "toggleFullscreen":"not-a-key"
        }));
        assert_eq!(
            bindings.action(u16::from(b'S'), 0x04),
            Some(StreamShortcutAction::ToggleStats)
        );
        assert_eq!(
            bindings.action(0x7a, 0),
            Some(StreamShortcutAction::ToggleFullscreen)
        );
        assert_eq!(
            StreamShortcutBindings::default().action(0x7a, 0x02),
            Some(StreamShortcutAction::Screenshot)
        );
    }

    #[test]
    fn explicitly_cleared_shortcuts_do_not_capture_gameplay_keys() {
        let bindings = StreamShortcutBindings::from_json(&serde_json::json!({
            "toggleStats":"", "toggleRecording":"", "saveClip":""
        }));
        assert_eq!(bindings.action(u16::from(b'N'), 0x02), None);
        assert_eq!(bindings.action(0x7b, 0), None);
        assert_eq!(bindings.action(0x7b, 0x02), None);
        assert_eq!(
            bindings.action(0x7a, 0),
            Some(StreamShortcutAction::ToggleFullscreen)
        );
    }

    #[test]
    fn replay_shortcut_and_qt_portable_key_aliases_are_supported() {
        assert_eq!(
            StreamShortcutBindings::default().action(0x7b, 0x02),
            Some(StreamShortcutAction::SaveClip)
        );
        assert_eq!(
            StreamShortcutBindings::default().action(0x7b, 0),
            Some(StreamShortcutAction::ToggleRecording)
        );
        assert_eq!(
            StreamShortcutBindings::from_json(&serde_json::json!({"saveClip":"Alt+C"}))
                .action(u16::from(b'C'), 0x04),
            Some(StreamShortcutAction::SaveClip)
        );
        assert_eq!(StreamShortcutAction::SaveClip.protocol_name(), "save-clip");
        for (portable, native) in [
            ("Ctrl+PgUp", "Ctrl+PageUp"),
            ("Alt+PgDown", "Alt+PageDown"),
            ("Print", "PrintScreen"),
        ] {
            assert_eq!(ShortcutChord::parse(portable), ShortcutChord::parse(native));
            assert!(ShortcutChord::parse(portable).is_some());
        }
    }
}
