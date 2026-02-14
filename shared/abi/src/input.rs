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
    KeyboardBinding { modifier_mask: MOD_ALT, keycode: 0x52, action: GestureAction::Home },
    KeyboardBinding { modifier_mask: MOD_ALT, keycode: 0x50, action: GestureAction::AppSwitcherLeft },
    KeyboardBinding { modifier_mask: MOD_ALT, keycode: 0x4F, action: GestureAction::AppSwitcherRight },
    KeyboardBinding { modifier_mask: MOD_ALT, keycode: 0x51, action: GestureAction::ControlCenter },
    KeyboardBinding { modifier_mask: MOD_ALT | MOD_SHIFT, keycode: 0x51, action: GestureAction::NotificationCenter },
];
