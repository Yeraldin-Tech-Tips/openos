use abi::input::{GestureAction, KeyboardBinding, DEFAULT_BINDINGS, MOD_ALT, MOD_CTRL, MOD_SHIFT};
use std::fs;

#[derive(Clone, Debug)]
struct Surface {
    id: u32,
    title: String,
}

#[derive(Default)]
struct ShellState {
    home_visible: bool,
    control_center_visible: bool,
    notification_center_visible: bool,
    running_apps: Vec<Surface>,
}

fn main() {
    let mut state = ShellState {
        home_visible: true,
        ..ShellState::default()
    };

    let bindings = load_bindings("userspace/shell/config/default-bindings.toml")
        .unwrap_or_else(|| DEFAULT_BINDINGS.to_vec());

    println!("OpenOS Shell started");
    println!("Gesture bindings loaded: {}", bindings.len());

    for binding in &bindings {
        apply_action(&mut state, binding.action);
    }

    println!(
        "Shell ready: home={}, control_center={}, notification_center={}",
        state.home_visible, state.control_center_visible, state.notification_center_visible
    );
}

fn load_bindings(path: &str) -> Option<Vec<KeyboardBinding>> {
    let content = fs::read_to_string(path).ok()?;
    let mut out = Vec::new();

    let mut pending_keys: Option<Vec<String>> = None;
    let mut pending_action: Option<String> = None;

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with("[[binding]]") {
            flush_binding(&mut out, &mut pending_keys, &mut pending_action);
            continue;
        }
        if let Some(keys) = parse_keys_line(line) {
            pending_keys = Some(keys);
            continue;
        }
        if let Some(action) = parse_action_line(line) {
            pending_action = Some(action);
            continue;
        }
    }

    flush_binding(&mut out, &mut pending_keys, &mut pending_action);

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn flush_binding(
    out: &mut Vec<KeyboardBinding>,
    pending_keys: &mut Option<Vec<String>>,
    pending_action: &mut Option<String>,
) {
    let Some(keys) = pending_keys.take() else {
        return;
    };
    let Some(action_name) = pending_action.take() else {
        return;
    };

    if let Some(action) = parse_action(&action_name) {
        if let Some(binding) = build_binding(&keys, action) {
            out.push(binding);
        }
    }
}

fn parse_keys_line(line: &str) -> Option<Vec<String>> {
    if !line.starts_with("keys") {
        return None;
    }
    let start = line.find('[')?;
    let end = line.rfind(']')?;
    let body = &line[start + 1..end];

    let mut keys = Vec::new();
    for part in body.split(',') {
        let key = part.trim().trim_matches('"');
        if !key.is_empty() {
            keys.push(key.to_string());
        }
    }

    if keys.is_empty() {
        None
    } else {
        Some(keys)
    }
}

fn parse_action_line(line: &str) -> Option<String> {
    if !line.starts_with("action") {
        return None;
    }
    let (_, rhs) = line.split_once('=')?;
    let action = rhs.trim().trim_matches('"');
    if action.is_empty() {
        None
    } else {
        Some(action.to_string())
    }
}

fn parse_action(name: &str) -> Option<GestureAction> {
    match name {
        "Home" => Some(GestureAction::Home),
        "AppSwitcherLeft" => Some(GestureAction::AppSwitcherLeft),
        "AppSwitcherRight" => Some(GestureAction::AppSwitcherRight),
        "ControlCenter" => Some(GestureAction::ControlCenter),
        "NotificationCenter" => Some(GestureAction::NotificationCenter),
        _ => None,
    }
}

fn build_binding(keys: &[String], action: GestureAction) -> Option<KeyboardBinding> {
    let mut modifier_mask = 0u32;
    let mut keycode = None;

    for key in keys {
        match key.as_str() {
            "Alt" => modifier_mask |= MOD_ALT,
            "Shift" => modifier_mask |= MOD_SHIFT,
            "Ctrl" => modifier_mask |= MOD_CTRL,
            "Up" => keycode = Some(0x52),
            "Down" => keycode = Some(0x51),
            "Left" => keycode = Some(0x50),
            "Right" => keycode = Some(0x4F),
            _ => {}
        }
    }

    keycode.map(|keycode| KeyboardBinding {
        modifier_mask,
        keycode,
        action,
    })
}

fn apply_action(state: &mut ShellState, action: GestureAction) {
    match action {
        GestureAction::Home => {
            state.home_visible = true;
            state.control_center_visible = false;
            state.notification_center_visible = false;
        }
        GestureAction::AppSwitcherLeft | GestureAction::AppSwitcherRight => {
            state.home_visible = false;
        }
        GestureAction::ControlCenter => {
            state.control_center_visible = !state.control_center_visible;
            state.notification_center_visible = false;
        }
        GestureAction::NotificationCenter => {
            state.notification_center_visible = !state.notification_center_visible;
            state.control_center_visible = false;
        }
        _ => {}
    }
}
