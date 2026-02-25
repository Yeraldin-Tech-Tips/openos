use core::sync::atomic::{AtomicBool, Ordering};

use abi::input::{GestureAction, DEFAULT_BINDINGS};

use crate::sync::IrqSafeLock;

const INPUT_QUEUE_CAPACITY: usize = 32;
const MOUSE_PACKET_SIZE: usize = 3;
const MOD_ALT: u32 = 0b0001;
const MOD_SHIFT: u32 = 0b0010;
const MOD_CTRL: u32 = 0b0100;

static INPUT_SUBSCRIBED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct InputState {
    queue: [u8; INPUT_QUEUE_CAPACITY],
    queue_head: usize,
    queue_tail: usize,
    queue_len: usize,
    modifier_state: u32,
    extended_prefix: bool,
    mouse_packet: [u8; MOUSE_PACKET_SIZE],
    mouse_packet_index: usize,
    mouse_left_down: bool,
    mouse_right_down: bool,
    mouse_middle_down: bool,
}

const EMPTY_INPUT_STATE: InputState = InputState {
    queue: [0; INPUT_QUEUE_CAPACITY],
    queue_head: 0,
    queue_tail: 0,
    queue_len: 0,
    modifier_state: 0,
    extended_prefix: false,
    mouse_packet: [0; MOUSE_PACKET_SIZE],
    mouse_packet_index: 0,
    mouse_left_down: false,
    mouse_right_down: false,
    mouse_middle_down: false,
};

static INPUT_STATE: IrqSafeLock<InputState> = IrqSafeLock::new(EMPTY_INPUT_STATE);

pub fn init() {
    INPUT_SUBSCRIBED.store(false, Ordering::Release);
    // State is already initialized at declaration time; explicit reset skipped.
}

pub fn subscribe(enable: bool) {
    INPUT_SUBSCRIBED.store(enable, Ordering::Release);
}

pub fn read_action() -> Option<GestureAction> {
    let mut state = INPUT_STATE.lock();
    pop_action(&mut state)
}

pub fn on_ps2_scancode(byte: u8) {
    let action = {
        let mut state = INPUT_STATE.lock();
        if byte == 0xE0 {
            state.extended_prefix = true;
            return;
        }

        let extended = state.extended_prefix;
        state.extended_prefix = false;

        let is_release = (byte & 0x80) != 0;
        let code = byte & 0x7F;

        if update_modifier(&mut state, code, is_release) || is_release {
            return;
        }

        let modifier_state = state.modifier_state;
        if let Some(action) = handle_direct_key(code, extended, modifier_state) {
            Some(action)
        } else {
            let Some(keycode) = ps2_to_hid_usage(code, extended) else {
                return;
            };
            select_action_for_key(keycode, modifier_state)
        }
    };

    if let Some(action) = action {
        dispatch_action(action);
    }
}

pub fn on_ps2_mouse_byte(byte: u8) {
    let mut motion = None;
    let mut left_transition = None;
    let mut right_pressed = false;
    let mut middle_pressed = false;
    let left_is_down = {
        let mut state = INPUT_STATE.lock();
        if byte == 0xFA || byte == 0xAA {
            return;
        }

        if state.mouse_packet_index == 0 && (byte & 0x08) == 0 {
            return;
        }

        let idx = state.mouse_packet_index;
        state.mouse_packet[idx] = byte;
        state.mouse_packet_index += 1;
        if state.mouse_packet_index < MOUSE_PACKET_SIZE {
            return;
        }
        state.mouse_packet_index = 0;

        let flags = state.mouse_packet[0];
        let dx_raw = state.mouse_packet[1];
        let dy_raw = state.mouse_packet[2];

        if (flags & 0x40) != 0 || (flags & 0x80) != 0 {
            return;
        }

        let dx = decode_mouse_delta(dx_raw, (flags & 0x10) != 0) as i32;
        let dy = decode_mouse_delta(dy_raw, (flags & 0x20) != 0) as i32;
        if dx != 0 || dy != 0 {
            motion = Some((dx, -dy));
        }

        let left_down = (flags & 0x01) != 0;
        if left_down != state.mouse_left_down {
            state.mouse_left_down = left_down;
            left_transition = Some(left_down);
        }

        let right_down = (flags & 0x02) != 0;
        if right_down != state.mouse_right_down {
            right_pressed = right_down;
            state.mouse_right_down = right_down;
        }

        let middle_down = (flags & 0x04) != 0;
        if middle_down != state.mouse_middle_down {
            middle_pressed = middle_down;
            state.mouse_middle_down = middle_down;
        }
        state.mouse_left_down
    };

    if let Some((dx, dy)) = motion {
        let _ = crate::ui::compositor::move_pointer(dx, dy);
    }
    if let Some(pressed) = left_transition {
        if let Some(action) = crate::ui::compositor::set_pointer_button(pressed)
            .ok()
            .flatten()
        {
            dispatch_action(action);
        }
    }
    if middle_pressed && !left_is_down {
        let _ = crate::ui::compositor::set_pointer_button(true);
        if let Some(action) = crate::ui::compositor::set_pointer_button(false)
            .ok()
            .flatten()
        {
            dispatch_action(action);
        }
    }
    if right_pressed {
        dispatch_action(GestureAction::Home);
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
            0x47 => return Some(GestureAction::Home),
            0x48 | 0x4B => {
                let _ = crate::ui::compositor::focus_prev();
                return None;
            }
            0x50 | 0x4D => {
                let _ = crate::ui::compositor::focus_next();
                return None;
            }
            0x1C => {
                return crate::ui::compositor::activate_focused_target()
                    .ok()
                    .flatten();
            }
            _ => {}
        }
    }

    if !extended && (modifier_state & (MOD_ALT | MOD_CTRL)) == 0 {
        match code {
            0x48 | 0x4B => {
                let _ = crate::ui::compositor::focus_prev();
                return None;
            }
            0x50 | 0x4D => {
                let _ = crate::ui::compositor::focus_next();
                return None;
            }
            0x47 => return Some(GestureAction::Home),
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
        let mut state = INPUT_STATE.lock();
        push_action(&mut state, action);
    }
}

fn update_modifier(state: &mut InputState, code: u8, is_release: bool) -> bool {
    let bit = match code {
        0x38 => MOD_ALT,
        0x2A | 0x36 => MOD_SHIFT,
        0x1D => MOD_CTRL,
        _ => return false,
    };

    if is_release {
        state.modifier_state &= !bit;
    } else {
        state.modifier_state |= bit;
    }
    true
}

fn ps2_to_hid_usage(code: u8, extended: bool) -> Option<u16> {
    match code {
        0x48 => Some(0x52),
        0x50 => Some(0x51),
        0x4B => Some(0x50),
        0x4D => Some(0x4F),
        _ => None,
    }
    .and_then(|usage| {
        if extended {
            Some(usage)
        } else {
            match code {
                0x48 | 0x50 | 0x4B | 0x4D => Some(usage),
                _ => None,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{decode_mouse_delta, ps2_to_hid_usage};

    #[test]
    fn decode_mouse_delta_signed() {
        assert_eq!(decode_mouse_delta(5, false), 5);
        assert_eq!(decode_mouse_delta(0xFF, true), -1);
    }

    #[test]
    fn arrow_key_usages_handle_extended_and_keypad_paths() {
        assert_eq!(ps2_to_hid_usage(0x48, true), Some(0x52));
        assert_eq!(ps2_to_hid_usage(0x48, false), Some(0x52));
        assert_eq!(ps2_to_hid_usage(0x50, true), Some(0x51));
        assert_eq!(ps2_to_hid_usage(0x50, false), Some(0x51));
        assert_eq!(ps2_to_hid_usage(0x1E, false), None);
    }
}

fn decode_mouse_delta(value: u8, negative: bool) -> i16 {
    if negative {
        value as i16 - 256
    } else {
        value as i16
    }
}

fn push_action(state: &mut InputState, action: GestureAction) {
    if state.queue_len >= INPUT_QUEUE_CAPACITY {
        return;
    }

    state.queue[state.queue_tail] = action as u8;
    state.queue_tail = (state.queue_tail + 1) % INPUT_QUEUE_CAPACITY;
    state.queue_len += 1;
}

fn pop_action(state: &mut InputState) -> Option<GestureAction> {
    if state.queue_len == 0 {
        return None;
    }

    let code = state.queue[state.queue_head];
    state.queue_head = (state.queue_head + 1) % INPUT_QUEUE_CAPACITY;
    state.queue_len -= 1;
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
