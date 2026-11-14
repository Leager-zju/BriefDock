use serde::{Deserialize, Serialize};
use windows_sys::Win32::{Foundation::HWND, UI::Input::KeyboardAndMouse::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shortcut {
    modifiers: u32,
    key: u16,
}
impl Default for Shortcut {
    fn default() -> Self {
        Self {
            modifiers: MOD_CONTROL | MOD_ALT,
            key: b'L'.into(),
        }
    }
}
impl Shortcut {
    pub fn from_parts(modifiers: u32, key: u16) -> Option<Self> {
        let value = Self { modifiers, key };
        value.supported().then_some(value)
    }
    pub fn capture(key: u16) -> Option<Self> {
        let down = |key| unsafe { GetKeyState(i32::from(key)) } < 0;
        if down(VK_LWIN) || down(VK_RWIN) {
            return None;
        }
        Self::from_parts(
            u32::from(down(VK_CONTROL)) * MOD_CONTROL
                | u32::from(down(VK_MENU)) * MOD_ALT
                | u32::from(down(VK_SHIFT)) * MOD_SHIFT,
            key,
        )
    }
    pub(super) fn supported(self) -> bool {
        self.modifiers & (MOD_ALT | MOD_CONTROL) != 0
            && self.modifiers & !(MOD_ALT | MOD_CONTROL | MOD_SHIFT) == 0
            && ((b'A' as u16..=b'Z' as u16).contains(&self.key)
                || (b'0' as u16..=b'9' as u16).contains(&self.key)
                || (VK_F1..=VK_F24).contains(&self.key) && self.key != VK_F12)
            && !(self.modifiers == MOD_ALT && self.key == VK_F4)
    }
    pub fn label(self) -> String {
        let mut text = String::new();
        for (flag, name) in [
            (MOD_CONTROL, "Ctrl+"),
            (MOD_ALT, "Alt+"),
            (MOD_SHIFT, "Shift+"),
        ] {
            if self.modifiers & flag != 0 {
                text.push_str(name);
            }
        }
        if (VK_F1..=VK_F24).contains(&self.key) {
            text.push_str(&format!("F{}", self.key - VK_F1 + 1));
        } else {
            text.push(char::from(self.key as u8));
        }
        text
    }
    pub fn register(self, window: HWND, id: i32) -> bool {
        self.supported()
            && unsafe { RegisterHotKey(window, id, self.modifiers | MOD_NOREPEAT, self.key.into()) }
                != 0
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reserved_and_typing_shortcuts_are_rejected() {
        assert_eq!(Shortcut::default().label(), "Ctrl+Alt+L");
        for (modifiers, key) in [
            (0, b'L' as u16),
            (MOD_SHIFT, b'L' as u16),
            (MOD_ALT, VK_F4),
            (MOD_CONTROL, VK_F12),
            (MOD_WIN, b'L' as u16),
        ] {
            assert!(Shortcut::from_parts(modifiers, key).is_none());
        }
        assert_eq!(
            Shortcut::from_parts(MOD_CONTROL | MOD_SHIFT, VK_F2)
                .unwrap()
                .label(),
            "Ctrl+Shift+F2"
        );
    }
}
