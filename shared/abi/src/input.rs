#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureAction {
    Home,
    AppSwitcherLeft,
    AppSwitcherRight,
    ControlCenter,
    NotificationCenter,
    LaunchShell,
    LaunchSettings,
    LaunchFiles,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KeyboardBinding {
    pub modifier_mask: u32,
    pub keycode: u16,
    pub action: GestureAction,
}

pub const MOD_ALT: u32 = 0b0001;
pub const MOD_SHIFT: u32 = 0b0010;
pub const MOD_CTRL: u32 = 0b0100;

pub const DEFAULT_BINDINGS: &[KeyboardBinding] = &[
    KeyboardBinding {
        modifier_mask: MOD_ALT,
        keycode: 0x52,
        action: GestureAction::Home,
    },
    KeyboardBinding {
        modifier_mask: MOD_ALT,
        keycode: 0x50,
        action: GestureAction::AppSwitcherLeft,
    },
    KeyboardBinding {
        modifier_mask: MOD_ALT,
        keycode: 0x4F,
        action: GestureAction::AppSwitcherRight,
    },
    KeyboardBinding {
        modifier_mask: MOD_ALT,
        keycode: 0x51,
        action: GestureAction::ControlCenter,
    },
    KeyboardBinding {
        modifier_mask: MOD_ALT | MOD_SHIFT,
        keycode: 0x51,
        action: GestureAction::NotificationCenter,
    },
    KeyboardBinding {
        modifier_mask: MOD_ALT,
        keycode: 0x16,
        action: GestureAction::LaunchShell,
    },
    KeyboardBinding {
        modifier_mask: MOD_ALT,
        keycode: 0x08,
        action: GestureAction::LaunchSettings,
    },
    KeyboardBinding {
        modifier_mask: MOD_ALT,
        keycode: 0x09,
        action: GestureAction::LaunchFiles,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_bits_are_distinct() {
        assert_eq!(MOD_ALT, 0b0001);
        assert_eq!(MOD_SHIFT, 0b0010);
        assert_eq!(MOD_CTRL, 0b0100);
        assert_eq!(MOD_ALT & MOD_SHIFT, 0);
        assert_eq!(MOD_ALT & MOD_CTRL, 0);
        assert_eq!(MOD_SHIFT & MOD_CTRL, 0);
    }

    #[test]
    fn modifier_combinations() {
        let alt_shift = MOD_ALT | MOD_SHIFT;
        assert_ne!(alt_shift & MOD_ALT, 0);
        assert_ne!(alt_shift & MOD_SHIFT, 0);
        assert_eq!(alt_shift & MOD_CTRL, 0);
    }

    #[test]
    fn gesture_action_variants_exist() {
        let actions = [
            GestureAction::Home,
            GestureAction::AppSwitcherLeft,
            GestureAction::AppSwitcherRight,
            GestureAction::ControlCenter,
            GestureAction::NotificationCenter,
            GestureAction::LaunchShell,
            GestureAction::LaunchSettings,
            GestureAction::LaunchFiles,
        ];
        assert_eq!(actions.len(), 8);
    }

    #[test]
    fn default_bindings_count() {
        assert_eq!(DEFAULT_BINDINGS.len(), 8);
    }

    #[test]
    fn default_bindings_home() {
        let home = &DEFAULT_BINDINGS[0];
        assert_eq!(home.modifier_mask, MOD_ALT);
        assert_eq!(home.keycode, 0x52);
        assert_eq!(home.action, GestureAction::Home);
    }

    #[test]
    fn default_bindings_app_switcher() {
        let left = &DEFAULT_BINDINGS[1];
        assert_eq!(left.action, GestureAction::AppSwitcherLeft);
        let right = &DEFAULT_BINDINGS[2];
        assert_eq!(right.action, GestureAction::AppSwitcherRight);
    }

    #[test]
    fn default_bindings_notification_center_uses_alt_shift() {
        let binding = &DEFAULT_BINDINGS[4];
        assert_eq!(binding.modifier_mask, MOD_ALT | MOD_SHIFT);
        assert_eq!(binding.action, GestureAction::NotificationCenter);
    }

    #[test]
    fn default_bindings_include_app_launch_shortcuts() {
        let shell = &DEFAULT_BINDINGS[5];
        assert_eq!(shell.modifier_mask, MOD_ALT);
        assert_eq!(shell.keycode, 0x16);
        assert_eq!(shell.action, GestureAction::LaunchShell);

        let settings = &DEFAULT_BINDINGS[6];
        assert_eq!(settings.modifier_mask, MOD_ALT);
        assert_eq!(settings.keycode, 0x08);
        assert_eq!(settings.action, GestureAction::LaunchSettings);

        let files = &DEFAULT_BINDINGS[7];
        assert_eq!(files.modifier_mask, MOD_ALT);
        assert_eq!(files.keycode, 0x09);
        assert_eq!(files.action, GestureAction::LaunchFiles);
    }

    #[test]
    fn default_bindings_keycodes_unique() {
        for i in 0..DEFAULT_BINDINGS.len() {
            for j in (i + 1)..DEFAULT_BINDINGS.len() {
                let same_combo = DEFAULT_BINDINGS[i].modifier_mask
                    == DEFAULT_BINDINGS[j].modifier_mask
                    && DEFAULT_BINDINGS[i].keycode == DEFAULT_BINDINGS[j].keycode;
                assert!(!same_combo, "bindings {i} and {j} share the same key combo");
            }
        }
    }
}
