use core::sync::atomic::{AtomicBool, Ordering};

use abi::input::{GestureAction, DEFAULT_BINDINGS};

const INPUT_QUEUE_CAPACITY: usize = 32;
const MOUSE_PACKET_SIZE: usize = 3;
const MOD_ALT: u32 = 0b0001;
const MOD_SHIFT: u32 = 0b0010;
const MOD_CTRL: u32 = 0b0100;

static INPUT_SUBSCRIBED: AtomicBool = AtomicBool::new(false);

static mut QUEUE: [u8; INPUT_QUEUE_CAPACITY] = [0; INPUT_QUEUE_CAPACITY];
static mut QUEUE_HEAD: usize = 0;
static mut QUEUE_TAIL: usize = 0;
static mut QUEUE_LEN: usize = 0;
static mut MODIFIER_STATE: u32 = 0;
static mut EXTENDED_PREFIX: bool = false;
static mut MOUSE_PACKET: [u8; MOUSE_PACKET_SIZE] = [0; MOUSE_PACKET_SIZE];
static mut MOUSE_PACKET_INDEX: usize = 0;
static mut MOUSE_LEFT_DOWN: bool = false;

pub fn init() {
    INPUT_SUBSCRIBED.store(false, Ordering::Release);
    unsafe {
        QUEUE_HEAD = 0;
        QUEUE_TAIL = 0;
        QUEUE_LEN = 0;
        MODIFIER_STATE = 0;
        EXTENDED_PREFIX = false;
        MOUSE_PACKET = [0; MOUSE_PACKET_SIZE];
        MOUSE_PACKET_INDEX = 0;
        MOUSE_LEFT_DOWN = false;
    }
}

pub fn subscribe(enable: bool) {
    INPUT_SUBSCRIBED.store(enable, Ordering::Release);
}

pub fn read_action() -> Option<GestureAction> {
    unsafe { pop_action() }
}

pub fn on_ps2_scancode(byte: u8) {
    unsafe {
        if byte == 0xE0 {
            EXTENDED_PREFIX = true;
            return;
        }

        let extended = EXTENDED_PREFIX;
        EXTENDED_PREFIX = false;

        let is_release = (byte & 0x80) != 0;
        let code = byte & 0x7F;

        if update_modifier(code, is_release, extended) {
            return;
        }

        if is_release {
            return;
        }

        let modifier_state = MODIFIER_STATE;
        if let Some(action) = handle_direct_key(code, extended, modifier_state) {
            dispatch_action(action);
            return;
        }

        let Some(keycode) = ps2_to_hid_usage(code, extended) else {
            return;
        };

        let Some(action) = select_action_for_key(keycode, modifier_state) else {
            return;
        };
        dispatch_action(action);
    }
}

pub fn on_ps2_mouse_byte(byte: u8) {
    unsafe {
        if byte == 0xFA || byte == 0xAA {
            return;
        }

        if MOUSE_PACKET_INDEX == 0 && (byte & 0x08) == 0 {
            return;
        }

        MOUSE_PACKET[MOUSE_PACKET_INDEX] = byte;
        MOUSE_PACKET_INDEX += 1;
        if MOUSE_PACKET_INDEX < MOUSE_PACKET_SIZE {
            return;
        }
        MOUSE_PACKET_INDEX = 0;

        let flags = MOUSE_PACKET[0];
        let dx_raw = MOUSE_PACKET[1];
        let dy_raw = MOUSE_PACKET[2];

        if (flags & 0x40) != 0 || (flags & 0x80) != 0 {
            return;
        }

        let dx = decode_mouse_delta(dx_raw, (flags & 0x10) != 0) as i32;
        let dy = decode_mouse_delta(dy_raw, (flags & 0x20) != 0) as i32;

        if dx != 0 || dy != 0 {
            let _ = crate::ui::compositor::move_pointer(dx, -dy);
        }

        let left_down = (flags & 0x01) != 0;
        if left_down != MOUSE_LEFT_DOWN {
            MOUSE_LEFT_DOWN = left_down;
            let action = crate::ui::compositor::set_pointer_button(left_down)
                .ok()
                .flatten();
            if let Some(action) = action {
                dispatch_action(action);
            }
        }
    }
}

fn select_action_for_key(keycode: u16, modifier_state: u32) -> Option<GestureAction> {
    let mut best: Option<(GestureAction, u32)> = None;

    for binding in DEFAULT_BINDINGS.iter().copied() {
        if binding.keycode != keycode {
            continue;
        }
        if (modifier_state & binding.modifier_mask) != binding.modifier_mask {
            continue;
        }

        let weight = binding.modifier_mask.count_ones();
        match best {
            Some((_, best_weight)) if best_weight >= weight => {}
            _ => {
                best = Some((binding.action, weight));
            }
        }
    }

    best.map(|(action, _)| action)
}

fn handle_direct_key(code: u8, extended: bool, modifier_state: u32) -> Option<GestureAction> {
    if extended && (modifier_state & MOD_ALT) == 0 {
        match code {
            0x48 | 0x4B => {
                let _ = crate::ui::compositor::focus_prev();
                return None;
            }
            0x50 | 0x4D => {
                let _ = crate::ui::compositor::focus_next();
                return None;
            }
            _ => {}
        }
    }

    if !extended && (modifier_state & (MOD_ALT | MOD_CTRL)) == 0 {
        match code {
            0x0F => {
                if (modifier_state & MOD_SHIFT) != 0 {
                    let _ = crate::ui::compositor::focus_prev();
                } else {
                    let _ = crate::ui::compositor::focus_next();
                }
                return None;
            }
            0x1C | 0x39 => {
                return crate::ui::compositor::activate_focused_target()
                    .ok()
                    .flatten()
            }
            0x01 => return Some(GestureAction::Home),
            _ => {}
        }
    }

    None
}

fn dispatch_action(action: GestureAction) {
    if crate::ui::compositor::is_transition_action(action) {
        let _ = crate::ui::compositor::apply_gesture(action);
    }

    if INPUT_SUBSCRIBED.load(Ordering::Acquire) {
        unsafe { push_action(action) };
    }
}

unsafe fn update_modifier(code: u8, is_release: bool, _extended: bool) -> bool {
    let bit = match code {
        0x38 => MOD_ALT,
        0x2A | 0x36 => MOD_SHIFT,
        0x1D => MOD_CTRL,
        _ => return false,
    };

    if is_release {
        MODIFIER_STATE &= !bit;
    } else {
        MODIFIER_STATE |= bit;
    }
    true
}

fn ps2_to_hid_usage(code: u8, extended: bool) -> Option<u16> {
    if !extended {
        return None;
    }

    match code {
        0x48 => Some(0x52), // Up
        0x50 => Some(0x51), // Down
        0x4B => Some(0x50), // Left
        0x4D => Some(0x4F), // Right
        _ => None,
    }
}

fn decode_mouse_delta(value: u8, negative: bool) -> i16 {
    if negative {
        value as i16 - 256
    } else {
        value as i16
    }
}

unsafe fn push_action(action: GestureAction) {
    if QUEUE_LEN >= INPUT_QUEUE_CAPACITY {
        return;
    }

    QUEUE[QUEUE_TAIL] = action as u8;
    QUEUE_TAIL = (QUEUE_TAIL + 1) % INPUT_QUEUE_CAPACITY;
    QUEUE_LEN += 1;
}

unsafe fn pop_action() -> Option<GestureAction> {
    if QUEUE_LEN == 0 {
        return None;
    }

    let code = QUEUE[QUEUE_HEAD];
    QUEUE_HEAD = (QUEUE_HEAD + 1) % INPUT_QUEUE_CAPACITY;
    QUEUE_LEN -= 1;
    decode_action(code)
}

fn decode_action(code: u8) -> Option<GestureAction> {
    match code {
        0 => Some(GestureAction::Home),
        1 => Some(GestureAction::AppSwitcherLeft),
        2 => Some(GestureAction::AppSwitcherRight),
        3 => Some(GestureAction::ControlCenter),
        4 => Some(GestureAction::NotificationCenter),
        5 => Some(GestureAction::LaunchShell),
        6 => Some(GestureAction::LaunchSettings),
        7 => Some(GestureAction::LaunchFiles),
        _ => None,
    }
}
