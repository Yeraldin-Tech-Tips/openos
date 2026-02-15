use core::{
    cmp::{max, min},
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};

use abi::input::GestureAction;

use crate::{
    ipc, lifecycle,
    lifecycle::{AppKind, AppState},
    ui::framebuffer::{self, FramebufferError},
};

#[derive(Clone, Copy)]
pub struct SurfaceId(pub u32);

#[derive(Clone, Copy)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    fn contains(&self, px: u32, py: u32) -> bool {
        let x0 = self.x.max(0) as u32;
        let y0 = self.y.max(0) as u32;
        let x1 = x0.saturating_add(self.width);
        let y1 = y0.saturating_add(self.height);
        px >= x0 && px < x1 && py >= y0 && py < y1
    }
}

#[derive(Clone, Copy)]
pub struct SceneNode {
    pub surface: SurfaceId,
    pub frame: Rect,
    pub opacity: u8,
    pub z_index: i32,
}

pub struct Scene {
    pub nodes: &'static [SceneNode],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SimpleScene {
    pub top_color: u32,
    pub bottom_color: u32,
    pub dock_color: u32,
    pub dock_height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentError {
    MissingScene,
    FramebufferUnavailable,
}

const DEFAULT_SCENE: SimpleScene = SimpleScene {
    top_color: 0x040A20,
    bottom_color: 0x07163A,
    dock_color: 0x1E2D3F,
    dock_height: 104,
};
const HOME_SCENE: SimpleScene = SimpleScene {
    top_color: 0x040A20,
    bottom_color: 0x07183F,
    dock_color: 0x1F2F42,
    dock_height: 104,
};
const APP_SWITCHER_LEFT_SCENE: SimpleScene = SimpleScene {
    top_color: 0x03091A,
    bottom_color: 0x061432,
    dock_color: 0x1B2838,
    dock_height: 104,
};
const APP_SWITCHER_RIGHT_SCENE: SimpleScene = SimpleScene {
    top_color: 0x040A1D,
    bottom_color: 0x07163A,
    dock_color: 0x1D2B3B,
    dock_height: 104,
};
const CONTROL_CENTER_SCENE: SimpleScene = SimpleScene {
    top_color: 0x020714,
    bottom_color: 0x06112A,
    dock_color: 0x182535,
    dock_height: 102,
};
const NOTIFICATION_SCENE: SimpleScene = SimpleScene {
    top_color: 0x020714,
    bottom_color: 0x06122D,
    dock_color: 0x182637,
    dock_height: 102,
};

const TRANSITION_STEPS: u32 = 7;
const TRANSITION_SPIN: u32 = 50_000;
const DOCK_ICON_COUNT: usize = 10;
const HOME_TARGET_COUNT: usize = 23;
const INVALID_TARGET_INDEX: usize = usize::MAX;

#[derive(Clone, Copy, Default)]
struct MotionState {
    content_dx: i32,
    content_dy: i32,
    dock_lift: i32,
}

#[derive(Clone, Copy)]
struct HomeLayout {
    left_margin: u32,
    top_y: u32,
    widget_size: u32,
    widget_gap: u32,
    weather_h: u32,
    icons_x0: u32,
    icon_size: u32,
    col_gap: u32,
    row_gap: u32,
}

#[derive(Clone, Copy)]
enum IconKind {
    Messages,
    FaceTime,
    Files,
    Reminders,
    AppStore,
    Navigation,
    Books,
    Podcasts,
    Photos,
    Settings,
    Safari,
    Music,
    Mail,
    Camera,
    Notes,
    Brush,
    Reddit,
    FolderGrid,
}

#[derive(Clone, Copy)]
struct IconSpec {
    label: &'static [u8],
    bg: u32,
    kind: IconKind,
    badge: u8,
}

#[derive(Clone, Copy)]
enum TargetAction {
    ToggleClock,
    ToggleMatch,
    ToggleWeather,
    CloseForegroundApp,
    ForegroundPrimary,
    ForegroundSecondary,
    Launch(GestureAction),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetId {
    ClockWidget,
    MatchWidget,
    WeatherWidget,
    AppIcon(usize),
    DockIcon(usize),
    AppClose,
    AppPrimary,
    AppSecondary,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DragTarget {
    None,
    ClockWidget,
    MatchWidget,
    WeatherWidget,
    DockIcon(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActiveApp {
    None,
    Shell,
    Settings,
    Files,
}

#[derive(Clone, Copy)]
struct HomeTarget {
    id: TargetId,
    frame: Rect,
    action: TargetAction,
}

impl HomeTarget {
    const fn empty() -> Self {
        Self {
            frame: Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            id: TargetId::AppIcon(0),
            action: TargetAction::Launch(GestureAction::Home),
        }
    }
}

#[derive(Clone, Copy)]
struct HomeTargets {
    items: [HomeTarget; HOME_TARGET_COUNT],
    len: usize,
}

impl HomeTargets {
    const fn empty() -> Self {
        Self {
            items: [HomeTarget::empty(); HOME_TARGET_COUNT],
            len: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct UiState {
    focus_index: usize,
    hover_index: usize,
    pressed_index: usize,
    pointer_x: u32,
    pointer_y: u32,
    pointer_initialized: bool,
    pointer_visible: bool,
    pointer_pressed: bool,
    drag_target: DragTarget,
    drag_moved: bool,
    clock_digital: bool,
    match_expanded: bool,
    weather_fahrenheit: bool,
    widget_clock_dx: i32,
    widget_clock_dy: i32,
    widget_match_dx: i32,
    widget_match_dy: i32,
    widget_weather_dx: i32,
    widget_weather_dy: i32,
    dock_order: [u8; DOCK_ICON_COUNT],
    active_app: ActiveApp,
    shell_network_flip: bool,
    settings_airplane: bool,
    files_cursor: u8,
}

impl UiState {
    const fn new() -> Self {
        Self {
            focus_index: 0,
            hover_index: INVALID_TARGET_INDEX,
            pressed_index: INVALID_TARGET_INDEX,
            pointer_x: 0,
            pointer_y: 0,
            pointer_initialized: false,
            pointer_visible: false,
            pointer_pressed: false,
            drag_target: DragTarget::None,
            drag_moved: false,
            clock_digital: false,
            match_expanded: false,
            weather_fahrenheit: false,
            widget_clock_dx: 0,
            widget_clock_dy: 0,
            widget_match_dx: 0,
            widget_match_dy: 0,
            widget_weather_dx: 0,
            widget_weather_dy: 0,
            dock_order: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
            active_app: ActiveApp::None,
            shell_network_flip: false,
            settings_airplane: false,
            files_cursor: 0,
        }
    }
}

static SCENE_SUBMITTED: AtomicBool = AtomicBool::new(false);
static FRAME_INDEX: AtomicU32 = AtomicU32::new(0);
static mut LAST_SCENE: SimpleScene = DEFAULT_SCENE;
static mut UI_STATE: UiState = UiState::new();

pub struct Compositor;

impl Compositor {
    pub const fn new() -> Self {
        Self
    }

    pub fn submit_scene(&self, _scene: &Scene) {
        // Placeholder for full scenegraph composition.
    }

    pub fn submit_simple_scene(&self, scene: SimpleScene) {
        submit_simple_scene(scene);
    }

    pub fn present(&self) -> Result<(), PresentError> {
        present_simple_scene()
    }
}

pub fn submit_simple_scene(scene: SimpleScene) {
    unsafe {
        LAST_SCENE = scene;
    }
    SCENE_SUBMITTED.store(true, Ordering::Release);
}

pub fn present_simple_scene() -> Result<(), PresentError> {
    if !SCENE_SUBMITTED.load(Ordering::Acquire) {
        return Err(PresentError::MissingScene);
    }

    let scene = unsafe { LAST_SCENE };
    let frame = FRAME_INDEX.fetch_add(1, Ordering::Relaxed);
    render_scene(scene, idle_motion(frame))
}

pub fn apply_gesture(action: GestureAction) -> Result<(), PresentError> {
    let target = scene_for_action(action);
    let from = unsafe { LAST_SCENE };

    let mut step = 0u32;
    while step <= TRANSITION_STEPS {
        let scene = interpolate_scene(from, target, step, TRANSITION_STEPS);
        let motion = transition_motion(action, step, TRANSITION_STEPS);
        render_scene(scene, motion)?;
        transition_spin();
        step += 1;
    }

    unsafe {
        LAST_SCENE = target;
        if matches!(action, GestureAction::Home) {
            UI_STATE.active_app = ActiveApp::None;
        }
    }
    SCENE_SUBMITTED.store(true, Ordering::Release);
    Ok(())
}

pub fn is_transition_action(action: GestureAction) -> bool {
    matches!(
        action,
        GestureAction::Home
            | GestureAction::AppSwitcherLeft
            | GestureAction::AppSwitcherRight
            | GestureAction::ControlCenter
            | GestureAction::NotificationCenter
    )
}

pub fn focus_next() -> Result<(), PresentError> {
    step_focus(1)
}

pub fn focus_prev() -> Result<(), PresentError> {
    step_focus(-1)
}

pub fn activate_focused_target() -> Result<Option<GestureAction>, PresentError> {
    if !SCENE_SUBMITTED.load(Ordering::Acquire) {
        return Err(PresentError::MissingScene);
    }

    let scene = unsafe { LAST_SCENE };
    let (width, height) = framebuffer::dimensions().map_err(map_framebuffer_error)?;
    let targets = build_home_targets(width, height, scene, MotionState::default());
    if targets.len == 0 {
        return Ok(None);
    }

    let action = unsafe {
        if UI_STATE.focus_index >= targets.len {
            UI_STATE.focus_index = 0;
        }
        UI_STATE.hover_index = UI_STATE.focus_index;
        apply_target_action(targets.items[UI_STATE.focus_index].action)
    };
    render_scene(scene, MotionState::default())?;
    Ok(action)
}

pub fn move_pointer(dx: i32, dy: i32) -> Result<(), PresentError> {
    if !SCENE_SUBMITTED.load(Ordering::Acquire) {
        return Err(PresentError::MissingScene);
    }

    let scene = unsafe { LAST_SCENE };
    let (width, height) = framebuffer::dimensions().map_err(map_framebuffer_error)?;
    let max_x = width.saturating_sub(1);
    let max_y = height.saturating_sub(1);
    let targets = build_home_targets(width, height, scene, MotionState::default());

    unsafe {
        if !UI_STATE.pointer_initialized {
            UI_STATE.pointer_x = width / 2;
            UI_STATE.pointer_y = height / 2;
            UI_STATE.pointer_initialized = true;
        }

        if dx != 0 || dy != 0 {
            UI_STATE.pointer_visible = true;
        }

        UI_STATE.pointer_x = shift_u32(UI_STATE.pointer_x, dx).min(max_x);
        UI_STATE.pointer_y = shift_u32(UI_STATE.pointer_y, dy).min(max_y);

        if UI_STATE.pointer_pressed {
            match UI_STATE.drag_target {
                DragTarget::ClockWidget => {
                    UI_STATE.widget_clock_dx = UI_STATE.widget_clock_dx.saturating_add(dx);
                    UI_STATE.widget_clock_dy = UI_STATE.widget_clock_dy.saturating_add(dy);
                    UI_STATE.drag_moved |= dx != 0 || dy != 0;
                }
                DragTarget::MatchWidget => {
                    UI_STATE.widget_match_dx = UI_STATE.widget_match_dx.saturating_add(dx);
                    UI_STATE.widget_match_dy = UI_STATE.widget_match_dy.saturating_add(dy);
                    UI_STATE.drag_moved |= dx != 0 || dy != 0;
                }
                DragTarget::WeatherWidget => {
                    UI_STATE.widget_weather_dx = UI_STATE.widget_weather_dx.saturating_add(dx);
                    UI_STATE.widget_weather_dy = UI_STATE.widget_weather_dy.saturating_add(dy);
                    UI_STATE.drag_moved |= dx != 0 || dy != 0;
                }
                DragTarget::DockIcon(from_slot) => {
                    if let Some(hit) = hit_test_targets(&targets, UI_STATE.pointer_x, UI_STATE.pointer_y) {
                        if let TargetId::DockIcon(to_slot) = targets.items[hit].id {
                            if from_slot != to_slot {
                                let from_value = UI_STATE.dock_order[from_slot];
                                UI_STATE.dock_order[from_slot] = UI_STATE.dock_order[to_slot];
                                UI_STATE.dock_order[to_slot] = from_value;
                                UI_STATE.drag_target = DragTarget::DockIcon(to_slot);
                                UI_STATE.drag_moved = true;
                                UI_STATE.focus_index = hit;
                            }
                        }
                    }
                }
                DragTarget::None => {}
            }
        }

        let max_dx = (width / 3) as i32;
        let max_dy = (height / 4) as i32;
        UI_STATE.widget_clock_dx = clamp_i32(UI_STATE.widget_clock_dx, -max_dx, max_dx);
        UI_STATE.widget_clock_dy = clamp_i32(UI_STATE.widget_clock_dy, -max_dy, max_dy);
        UI_STATE.widget_match_dx = clamp_i32(UI_STATE.widget_match_dx, -max_dx, max_dx);
        UI_STATE.widget_match_dy = clamp_i32(UI_STATE.widget_match_dy, -max_dy, max_dy);
        UI_STATE.widget_weather_dx = clamp_i32(UI_STATE.widget_weather_dx, -max_dx, max_dx);
        UI_STATE.widget_weather_dy = clamp_i32(UI_STATE.widget_weather_dy, -max_dy, max_dy);

    }

    let hover_targets = build_home_targets(width, height, scene, MotionState::default());
    unsafe {
        UI_STATE.hover_index = hit_test_targets(&hover_targets, UI_STATE.pointer_x, UI_STATE.pointer_y)
            .unwrap_or(INVALID_TARGET_INDEX);
    }

    render_scene(scene, MotionState::default())
}

pub fn set_pointer_button(pressed: bool) -> Result<Option<GestureAction>, PresentError> {
    if !SCENE_SUBMITTED.load(Ordering::Acquire) {
        return Err(PresentError::MissingScene);
    }

    let scene = unsafe { LAST_SCENE };
    let (width, height) = framebuffer::dimensions().map_err(map_framebuffer_error)?;
    let targets = build_home_targets(width, height, scene, MotionState::default());

    let action = unsafe {
        UI_STATE.pointer_visible = true;
        let mut action = None;

        if pressed {
            UI_STATE.pointer_pressed = true;
            UI_STATE.drag_moved = false;
            UI_STATE.drag_target = DragTarget::None;
            UI_STATE.pressed_index = INVALID_TARGET_INDEX;
            UI_STATE.hover_index = INVALID_TARGET_INDEX;
            if let Some(hit) = hit_test_targets(&targets, UI_STATE.pointer_x, UI_STATE.pointer_y) {
                UI_STATE.focus_index = hit;
                UI_STATE.hover_index = hit;
                UI_STATE.pressed_index = hit;
                UI_STATE.drag_target = drag_target_for_id(targets.items[hit].id);
            }
        } else if UI_STATE.pointer_pressed {
            UI_STATE.pointer_pressed = false;
            let release_hit = hit_test_targets(&targets, UI_STATE.pointer_x, UI_STATE.pointer_y);
            UI_STATE.hover_index = release_hit.unwrap_or(INVALID_TARGET_INDEX);

            if let Some(hit) = release_hit {
                UI_STATE.focus_index = hit;
                if !UI_STATE.drag_moved && UI_STATE.pressed_index == hit {
                    action = apply_target_action(targets.items[hit].action);
                }
            }
            UI_STATE.drag_target = DragTarget::None;
            UI_STATE.pressed_index = INVALID_TARGET_INDEX;
            UI_STATE.drag_moved = false;
        }
        action
    };

    render_scene(scene, MotionState::default())?;
    Ok(action)
}

fn step_focus(delta: i32) -> Result<(), PresentError> {
    if !SCENE_SUBMITTED.load(Ordering::Acquire) {
        return Err(PresentError::MissingScene);
    }

    let scene = unsafe { LAST_SCENE };
    let (width, height) = framebuffer::dimensions().map_err(map_framebuffer_error)?;
    let targets = build_home_targets(width, height, scene, MotionState::default());
    if targets.len == 0 {
        return Ok(());
    }

    unsafe {
        if UI_STATE.focus_index >= targets.len {
            UI_STATE.focus_index = 0;
        }

        let len = targets.len as i32;
        let mut index = UI_STATE.focus_index as i32 + delta;
        while index < 0 {
            index += len;
        }
        while index >= len {
            index -= len;
        }
        UI_STATE.focus_index = index as usize;
        UI_STATE.hover_index = UI_STATE.focus_index;

        let frame = targets.items[UI_STATE.focus_index].frame;
        UI_STATE.pointer_x = frame.x.max(0) as u32 + frame.width / 2;
        UI_STATE.pointer_y = frame.y.max(0) as u32 + frame.height / 2;
        UI_STATE.pointer_initialized = true;
    }

    render_scene(scene, MotionState::default())
}

fn scene_for_action(action: GestureAction) -> SimpleScene {
    match action {
        GestureAction::Home => HOME_SCENE,
        GestureAction::AppSwitcherLeft => APP_SWITCHER_LEFT_SCENE,
        GestureAction::AppSwitcherRight => APP_SWITCHER_RIGHT_SCENE,
        GestureAction::ControlCenter => CONTROL_CENTER_SCENE,
        GestureAction::NotificationCenter => NOTIFICATION_SCENE,
        GestureAction::LaunchShell | GestureAction::LaunchSettings | GestureAction::LaunchFiles => {
            HOME_SCENE
        }
    }
}

fn render_scene(scene: SimpleScene, motion: MotionState) -> Result<(), PresentError> {
    framebuffer::fill_vertical_gradient(scene.top_color, scene.bottom_color)
        .map_err(map_framebuffer_error)?;
    draw_scene_overlay(scene, motion).map_err(map_framebuffer_error)?;
    Ok(())
}

fn draw_scene_overlay(scene: SimpleScene, motion: MotionState) -> Result<(), FramebufferError> {
    let (width, height) = framebuffer::dimensions()?;
    let layout = compute_layout(width, height, scene.dock_height);
    let (clock_digital, match_expanded, weather_fahrenheit) = unsafe {
        (
            UI_STATE.clock_digital,
            UI_STATE.match_expanded,
            UI_STATE.weather_fahrenheit,
        )
    };

    draw_wallpaper(width, height, scene, motion)?;
    draw_status_bar(width)?;
    draw_widgets(
        layout,
        motion,
        clock_digital,
        match_expanded,
        weather_fahrenheit,
    )?;
    draw_app_grid(layout, motion)?;
    draw_dock(width, height, scene, motion)?;
    draw_page_dots(width, height, motion)?;
    draw_foreground_app(width, height)?;

    let targets = build_home_targets(width, height, scene, motion);
    draw_hover_press_effects(&targets)?;
    draw_focus_indicator(&targets)?;
    draw_pointer_cursor(width, height)?;
    Ok(())
}

fn compute_layout(width: u32, height: u32, dock_height: u32) -> HomeLayout {
    let icon_size = clamp_u32(width / 19, 56, 66);
    let left_margin = clamp_u32(width / 10, 42, 118);
    let top_y = clamp_u32(height / 11, 52, 84);
    let widget_size = clamp_u32(icon_size + 58, 118, 150);
    let widget_gap = clamp_u32(width / 58, 12, 22);
    let weather_h = clamp_u32(widget_size.saturating_sub(4), 108, 150);
    let row_gap = clamp_u32(height / 10, 52, 82);

    let mut icons_x0 =
        left_margin + widget_size.saturating_mul(2) + widget_gap + clamp_u32(width / 18, 30, 62);
    let mut col_gap = clamp_u32(width / 24, 22, 46);

    let icon_block_w = icon_size.saturating_mul(4) + col_gap.saturating_mul(3);
    let right_limit = width.saturating_sub(24);
    if icons_x0 + icon_block_w > right_limit {
        if right_limit > icons_x0 + icon_size.saturating_mul(4) {
            col_gap = (right_limit - icons_x0 - icon_size.saturating_mul(4)) / 3;
        } else {
            icons_x0 = left_margin + widget_size + widget_gap + 12;
            col_gap = clamp_u32(width / 30, 14, 26);
        }
    }

    let bottom_limit = height
        .saturating_sub(max(dock_height, 104))
        .saturating_sub(26);
    let third_row_y = top_y + widget_size + widget_gap + weather_h + clamp_u32(height / 23, 20, 38);
    let final_top_y = if third_row_y + icon_size + 18 > bottom_limit {
        top_y.saturating_sub((third_row_y + icon_size + 18) - bottom_limit)
    } else {
        top_y
    };

    HomeLayout {
        left_margin,
        top_y: final_top_y,
        widget_size,
        widget_gap,
        weather_h,
        icons_x0,
        icon_size,
        col_gap,
        row_gap,
    }
}

fn draw_wallpaper(
    width: u32,
    height: u32,
    scene: SimpleScene,
    motion: MotionState,
) -> Result<(), FramebufferError> {
    framebuffer::fill_rect_alpha(0, 0, width, height, 0x050A16, 138)?;

    let shift = motion.content_dx / 3;
    let max_dim = max(width, height);

    // Main blue sweep.
    let blue_cx = shift_u32(width.saturating_mul(56) / 100, shift);
    let blue_cy = height.saturating_mul(48) / 100;
    let blue_r = max_dim.saturating_mul(53) / 100;
    framebuffer::fill_circle_alpha(blue_cx, blue_cy, blue_r, 0x1B5BE3, 216)?;
    framebuffer::fill_circle_alpha(
        blue_cx,
        blue_cy,
        blue_r.saturating_sub(max_dim / 11),
        0x2D83F5,
        84,
    )?;

    // Dark right-side curtain seen in the reference.
    let dark_cx = shift_u32(width.saturating_mul(108) / 100, shift / 2);
    let dark_cy = height.saturating_mul(40) / 100;
    let dark_r = max_dim.saturating_mul(56) / 100;
    framebuffer::fill_circle_alpha(dark_cx, dark_cy, dark_r, 0x05070D, 214)?;

    // Green lower-right shape.
    let green_cx = shift_u32(width.saturating_mul(89) / 100, shift / 2);
    let green_cy = height.saturating_mul(104) / 100;
    let green_r = max_dim.saturating_mul(45) / 100;
    framebuffer::fill_circle_alpha(green_cx, green_cy, green_r, 0x6AD246, 232)?;

    // Blend seam where blue and green overlap.
    let seam_cx = shift_u32(width.saturating_mul(60) / 100, shift / 2);
    framebuffer::fill_circle_alpha(
        seam_cx,
        height.saturating_mul(87) / 100,
        max_dim / 3,
        0x1B4EAE,
        82,
    )?;
    framebuffer::fill_circle_alpha(
        seam_cx,
        height.saturating_mul(92) / 100,
        max_dim / 4,
        scene.dock_color,
        44,
    )?;
    Ok(())
}

fn draw_status_bar(width: u32) -> Result<(), FramebufferError> {
    framebuffer::draw_text(14, 12, b"9:48 PM  THU AUG 11", 0xF2F6FF)?;

    let percent_x = width.saturating_sub(62);
    framebuffer::draw_text(percent_x, 12, b"100%", 0xF2F6FF)?;

    let wifi_x = width.saturating_sub(92);
    framebuffer::fill_rect_alpha(wifi_x, 17, 3, 2, 0xF2F6FF, 224)?;
    framebuffer::fill_rect_alpha(wifi_x + 5, 15, 3, 4, 0xF2F6FF, 224)?;
    framebuffer::fill_rect_alpha(wifi_x + 10, 13, 3, 6, 0xF2F6FF, 224)?;

    let battery_x = width.saturating_sub(22);
    framebuffer::fill_rounded_rect_alpha(battery_x, 11, 14, 8, 2, 0xFFFFFF, 224)?;
    framebuffer::fill_rect_alpha(battery_x + 13, 13, 2, 4, 0xFFFFFF, 224)?;
    framebuffer::fill_rounded_rect_alpha(battery_x + 2, 13, 9, 4, 1, 0x7CE877, 224)?;
    Ok(())
}

fn draw_widgets(
    layout: HomeLayout,
    motion: MotionState,
    clock_digital: bool,
    match_expanded: bool,
    weather_fahrenheit: bool,
) -> Result<(), FramebufferError> {
    let dx = motion.content_dx;
    let dy = motion.content_dy;
    let (clock_dx, clock_dy, match_dx, match_dy, weather_dx, weather_dy) = unsafe {
        (
            UI_STATE.widget_clock_dx,
            UI_STATE.widget_clock_dy,
            UI_STATE.widget_match_dx,
            UI_STATE.widget_match_dy,
            UI_STATE.widget_weather_dx,
            UI_STATE.widget_weather_dy,
        )
    };

    let clock_x = shift_u32(layout.left_margin, dx + clock_dx);
    let clock_y = shift_u32(layout.top_y, dy + clock_dy);
    draw_clock_widget(clock_x, clock_y, layout.widget_size, clock_digital)?;

    let match_x = shift_u32(
        layout.left_margin + layout.widget_size + layout.widget_gap,
        dx + match_dx,
    );
    let match_y = shift_u32(layout.top_y, dy + match_dy);
    draw_match_widget(match_x, match_y, layout.widget_size, match_expanded)?;

    let weather_x = shift_u32(layout.left_margin, dx + weather_dx);
    let weather_y = shift_u32(
        layout.top_y + layout.widget_size + layout.widget_gap,
        dy + weather_dy,
    );
    let weather_w = layout.widget_size.saturating_mul(2) + layout.widget_gap;
    draw_weather_widget(
        weather_x,
        weather_y,
        weather_w,
        layout.weather_h,
        weather_fahrenheit,
    )?;
    Ok(())
}

fn draw_clock_widget(x: u32, y: u32, size: u32, digital: bool) -> Result<(), FramebufferError> {
    let radius = clamp_u32(size / 7, 18, 24);
    framebuffer::fill_rounded_rect_alpha(x + 4, y + 8, size, size, radius, 0x060B16, 136)?;
    framebuffer::fill_rounded_rect_alpha(x, y, size, size, radius, 0x0D1B43, 234)?;
    framebuffer::fill_rounded_rect_alpha(
        x + 2,
        y + 2,
        size.saturating_sub(4),
        size / 2,
        radius.saturating_sub(2),
        0xFFFFFF,
        22,
    )?;

    if digital {
        framebuffer::draw_text(x + 16, y + size / 2 - 10, b"09:48", 0xFFFFFF)?;
        framebuffer::draw_text(x + 16, y + size / 2 + 8, b"THU AUG 11", 0xC5D6EC)?;
    } else {
        let cx = x + size / 2;
        let cy = y + size / 2;
        let r = size / 3 + 2;
        framebuffer::fill_circle_alpha(cx, cy, r, 0xFFFFFF, 244)?;
        framebuffer::fill_circle_alpha(cx, cy, r.saturating_sub(3), 0xFFFFFF, 252)?;
        framebuffer::fill_circle_alpha(cx, cy, 3, 0x404854, 228)?;
        framebuffer::fill_rect_alpha(cx, cy.saturating_sub(r / 2), 2, r / 2 + 3, 0x39414C, 236)?;
        framebuffer::fill_rect_alpha(cx, cy, r / 2 + 3, 2, 0x39414C, 236)?;

        let mut i = 0u32;
        while i < 12 {
            let (ox, oy) = dial_offset(i, r.saturating_sub(5));
            framebuffer::fill_circle_alpha(shift_u32(cx, ox), shift_u32(cy, oy), 1, 0x5B6168, 220)?;
            i += 1;
        }
    }
    Ok(())
}

fn draw_match_widget(x: u32, y: u32, size: u32, expanded: bool) -> Result<(), FramebufferError> {
    let radius = clamp_u32(size / 7, 18, 24);
    framebuffer::fill_rounded_rect_alpha(x + 4, y + 8, size, size, radius, 0x060B16, 136)?;
    framebuffer::fill_rounded_rect_alpha(x, y, size, size, radius, 0x0A0C12, 236)?;
    framebuffer::fill_rounded_rect_alpha(
        x + 2,
        y + 2,
        size.saturating_sub(4),
        size / 2,
        radius.saturating_sub(2),
        0xFFFFFF,
        12,
    )?;

    framebuffer::fill_circle_alpha(x + 25, y + 24, 9, 0xDEE8F5, 228)?;
    framebuffer::fill_circle_alpha(x + size - 25, y + 24, 9, 0xC3D9EE, 228)?;
    framebuffer::fill_rounded_rect_alpha(x + 17, y + 40, 26, 16, 4, 0xD7263D, 210)?;
    framebuffer::draw_text(x + 14, y + 62, b"NEXT", 0xE7EFFD)?;
    framebuffer::draw_text(x + 14, y + 76, b"CRYSTAL PALACE", 0xE7EFFD)?;
    if expanded {
        framebuffer::draw_text(x + 14, y + 92, b"LIVERPOOL 2", 0xC5D8EE)?;
        framebuffer::draw_text(x + 14, y + 106, b"CRYSTAL PALACE 1", 0xC5D8EE)?;
    } else {
        framebuffer::draw_text(x + 14, y + 95, b"MON AUG 15  9:00 PM", 0xB9CDE6)?;
    }
    Ok(())
}

fn draw_weather_widget(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    fahrenheit: bool,
) -> Result<(), FramebufferError> {
    let radius = clamp_u32(height / 7, 16, 22);
    framebuffer::fill_rounded_rect_alpha(x + 4, y + 8, width, height, radius, 0x060B16, 136)?;
    framebuffer::fill_rounded_rect_alpha(x, y, width, height, radius, 0x081848, 234)?;
    framebuffer::fill_rounded_rect_alpha(
        x + 2,
        y + 2,
        width.saturating_sub(4),
        height / 2,
        radius.saturating_sub(2),
        0x4A79CC,
        28,
    )?;
    framebuffer::draw_text(x + 12, y + 14, b"DURBAN", 0xEEF4FF)?;
    if fahrenheit {
        framebuffer::draw_text(x + 12, y + 30, b"66F", 0xEEF4FF)?;
    } else {
        framebuffer::draw_text(x + 12, y + 30, b"19C", 0xEEF4FF)?;
    }
    framebuffer::draw_text(x + 12, y + 52, b"WARMER TOMORROW", 0xCAD9ED)?;
    if fahrenheit {
        framebuffer::draw_text(x + 12, y + 66, b"HIGH OF 79F", 0xCAD9ED)?;
    } else {
        framebuffer::draw_text(x + 12, y + 66, b"HIGH OF 26C", 0xCAD9ED)?;
    }

    let row_x = x + width.saturating_mul(56) / 100;
    let mut row_y = y + 18;
    let mut i = 0usize;
    while i < 5 {
        framebuffer::fill_rect_alpha(row_x, row_y, 42, 3, 0xF0CB56, 220)?;
        framebuffer::fill_rect_alpha(row_x + 46, row_y, 18, 3, 0x98C8F8, 216)?;
        row_y += 12;
        i += 1;
    }
    Ok(())
}

fn draw_app_grid(layout: HomeLayout, motion: MotionState) -> Result<(), FramebufferError> {
    let dx = motion.content_dx;
    let dy = motion.content_dy;

    let top_y = layout.top_y + 12;
    let second_y = top_y + layout.icon_size + layout.row_gap;
    let third_y = layout.top_y
        + layout.widget_size
        + layout.widget_gap
        + layout.weather_h
        + clamp_u32(layout.widget_size / 3, 24, 44);

    let top_icons = [
        IconSpec {
            label: b"MESSAGES",
            bg: 0x4FCA71,
            kind: IconKind::Messages,
            badge: 0,
        },
        IconSpec {
            label: b"FACETIME",
            bg: 0x57C96A,
            kind: IconKind::FaceTime,
            badge: 0,
        },
        IconSpec {
            label: b"FILES",
            bg: 0xF2F6FC,
            kind: IconKind::Files,
            badge: 0,
        },
        IconSpec {
            label: b"REMINDERS",
            bg: 0xF5F7FC,
            kind: IconKind::Reminders,
            badge: 0,
        },
    ];
    let second_icons = [
        IconSpec {
            label: b"APP STORE",
            bg: 0x5BA7F7,
            kind: IconKind::AppStore,
            badge: 0,
        },
        IconSpec {
            label: b"NAVIGATION",
            bg: 0x102D69,
            kind: IconKind::Navigation,
            badge: 0,
        },
        IconSpec {
            label: b"BOOKS",
            bg: 0xF3A744,
            kind: IconKind::Books,
            badge: 0,
        },
        IconSpec {
            label: b"PODCASTS",
            bg: 0x9C4BDD,
            kind: IconKind::Podcasts,
            badge: 0,
        },
    ];
    let third_icons = [
        IconSpec {
            label: b"PHOTOS",
            bg: 0xF6F8FC,
            kind: IconKind::Photos,
            badge: 0,
        },
        IconSpec {
            label: b"SETTINGS",
            bg: 0xD0D5DD,
            kind: IconKind::Settings,
            badge: 2,
        },
    ];

    let mut i = 0usize;
    while i < top_icons.len() {
        let x = shift_u32(
            layout.icons_x0 + i as u32 * (layout.icon_size + layout.col_gap),
            dx,
        );
        draw_app_icon(
            x,
            shift_u32(top_y, dy),
            layout.icon_size,
            top_icons[i],
            true,
        )?;
        i += 1;
    }

    let mut j = 0usize;
    while j < second_icons.len() {
        let x = shift_u32(
            layout.icons_x0 + j as u32 * (layout.icon_size + layout.col_gap),
            dx,
        );
        draw_app_icon(
            x,
            shift_u32(second_y, dy),
            layout.icon_size,
            second_icons[j],
            true,
        )?;
        j += 1;
    }

    let x0 = layout.left_margin + 10;
    let x1 = x0 + layout.icon_size + clamp_u32(layout.col_gap / 2 + 14, 18, 36);
    draw_app_icon(
        shift_u32(x0, dx),
        shift_u32(third_y, dy),
        layout.icon_size,
        third_icons[0],
        true,
    )?;
    draw_app_icon(
        shift_u32(x1, dx),
        shift_u32(third_y, dy),
        layout.icon_size,
        third_icons[1],
        true,
    )?;
    Ok(())
}

fn draw_app_icon(
    x: u32,
    y: u32,
    size: u32,
    spec: IconSpec,
    draw_label: bool,
) -> Result<(), FramebufferError> {
    let radius = clamp_u32(size / 4, 10, 18);
    framebuffer::fill_rounded_rect_alpha(x + 2, y + 6, size, size, radius, 0x081A3B, 110)?;
    framebuffer::fill_rounded_rect(x, y, size, size, radius, spec.bg)?;
    framebuffer::fill_rounded_rect_alpha(
        x + 1,
        y + 1,
        size.saturating_sub(2),
        size / 2,
        radius.saturating_sub(1),
        0xFFFFFF,
        62,
    )?;

    draw_icon_symbol(spec.kind, x, y, size)?;
    if spec.badge != 0 {
        draw_badge(x + size.saturating_sub(6), y + 8, spec.badge)?;
    }

    if draw_label {
        let label_w = (spec.label.len() as u32).saturating_mul(8);
        let label_x = x + size / 2 - label_w / 2;
        framebuffer::draw_text(label_x, y + size + 10, spec.label, 0xEEF4FF)?;
    }
    Ok(())
}

fn draw_dock(
    width: u32,
    height: u32,
    scene: SimpleScene,
    motion: MotionState,
) -> Result<(), FramebufferError> {
    let icon_size = clamp_u32(width / 22, 42, 52);
    let gap = clamp_u32(width / 118, 7, 10);
    let pad = 14;
    let separator_after = 5usize;
    let separator_w = 12u32;

    let icons_w = icon_size.saturating_mul(DOCK_ICON_COUNT as u32)
        + gap.saturating_mul((DOCK_ICON_COUNT - 1) as u32)
        + separator_w;
    let dock_w = icons_w + pad * 2;
    let dock_h = icon_size + 18;
    let dock_x = width.saturating_sub(dock_w) / 2;
    let base_dock_y = height.saturating_sub(max(scene.dock_height, 98));
    let dock_y = shift_u32(base_dock_y, -motion.dock_lift);

    framebuffer::fill_rounded_rect_alpha(
        dock_x + 2,
        dock_y + 6,
        dock_w,
        dock_h,
        24,
        0x05070D,
        178,
    )?;
    framebuffer::fill_rounded_rect_alpha(dock_x, dock_y, dock_w, dock_h, 24, 0x0D1621, 204)?;
    framebuffer::fill_rounded_rect_alpha(
        dock_x + 2,
        dock_y + 2,
        dock_w.saturating_sub(4),
        dock_h.saturating_sub(4),
        22,
        scene.dock_color,
        146,
    )?;
    framebuffer::fill_rect_alpha(
        dock_x + 18,
        dock_y + 3,
        dock_w.saturating_sub(36),
        2,
        0xF3FAFF,
        42,
    )?;

    let specs = [
        IconSpec {
            label: b"",
            bg: 0x67AEF8,
            kind: IconKind::Safari,
            badge: 0,
        },
        IconSpec {
            label: b"",
            bg: 0xF26382,
            kind: IconKind::Music,
            badge: 0,
        },
        IconSpec {
            label: b"",
            bg: 0x6BAEF8,
            kind: IconKind::Mail,
            badge: 66,
        },
        IconSpec {
            label: b"",
            bg: 0x87B4EC,
            kind: IconKind::Camera,
            badge: 8,
        },
        IconSpec {
            label: b"",
            bg: 0xF4E27C,
            kind: IconKind::Notes,
            badge: 0,
        },
        IconSpec {
            label: b"",
            bg: 0x171726,
            kind: IconKind::Brush,
            badge: 0,
        },
        IconSpec {
            label: b"",
            bg: 0xF06E3C,
            kind: IconKind::Reddit,
            badge: 0,
        },
        IconSpec {
            label: b"",
            bg: 0x5BA7F7,
            kind: IconKind::AppStore,
            badge: 0,
        },
        IconSpec {
            label: b"",
            bg: 0xF6F8FC,
            kind: IconKind::Photos,
            badge: 0,
        },
        IconSpec {
            label: b"",
            bg: 0xBED4F2,
            kind: IconKind::FolderGrid,
            badge: 0,
        },
    ];

    let mut x = dock_x + pad;
    let y = dock_y + (dock_h.saturating_sub(icon_size)) / 2;
    let order = unsafe { UI_STATE.dock_order };
    let mut i = 0usize;
    while i < DOCK_ICON_COUNT {
        let slot = (order[i] as usize).min(specs.len().saturating_sub(1));
        draw_app_icon(x, y, icon_size, specs[slot], false)?;
        x += icon_size + gap;
        if i == separator_after {
            framebuffer::fill_rect_alpha(
                x + 2,
                y + 6,
                2,
                icon_size.saturating_sub(12),
                0xA7B8CA,
                130,
            )?;
            x += separator_w;
        }
        i += 1;
    }
    Ok(())
}

fn draw_page_dots(width: u32, height: u32, motion: MotionState) -> Result<(), FramebufferError> {
    let y = shift_u32(height.saturating_sub(112), -motion.dock_lift / 2);
    let x = width / 2;
    framebuffer::fill_circle_alpha(x - 8, y, 3, 0xCBD8EA, 154)?;
    framebuffer::fill_circle_alpha(x + 8, y, 3, 0xF3F7FF, 232)?;
    Ok(())
}

fn draw_foreground_app(width: u32, height: u32) -> Result<(), FramebufferError> {
    let (active_app, shell_network_flip, settings_airplane, files_cursor) = unsafe {
        (
            UI_STATE.active_app,
            UI_STATE.shell_network_flip,
            UI_STATE.settings_airplane,
            UI_STATE.files_cursor,
        )
    };
    if active_app == ActiveApp::None {
        return Ok(());
    }

    let shell_status = lifecycle::status_for_kind(AppKind::Shell);
    let settings_status = lifecycle::status_for_kind(AppKind::Settings);
    let files_status = lifecycle::status_for_kind(AppKind::Files);
    let lifecycle_stats = lifecycle::stats();
    let ipc_stats = ipc::stats();

    let frame = app_window_frame(width, height);
    let fx = frame.x.max(0) as u32;
    let fy = frame.y.max(0) as u32;
    let fw = frame.width;
    let fh = frame.height;

    framebuffer::fill_rounded_rect_alpha(
        fx.saturating_sub(6),
        fy.saturating_add(8),
        fw.saturating_add(12),
        fh,
        26,
        0x04070D,
        178,
    )?;
    framebuffer::fill_rounded_rect_alpha(fx, fy, fw, fh, 24, 0x0A121F, 228)?;
    framebuffer::fill_rounded_rect_alpha(
        fx + 2,
        fy + 2,
        fw.saturating_sub(4),
        34,
        22,
        0x1B2B3F,
        172,
    )?;
    framebuffer::fill_circle_alpha(fx + fw.saturating_sub(24), fy + 18, 8, 0xE5646A, 222)?;
    framebuffer::draw_text(fx + 16, fy + 12, active_app_title(active_app), 0xF3F8FF)?;

    let body_y = fy + 46;
    framebuffer::fill_rounded_rect_alpha(
        fx + 10,
        body_y,
        fw.saturating_sub(20),
        fh.saturating_sub(58),
        14,
        0x0E1B2E,
        198,
    )?;

    match active_app {
        ActiveApp::Shell => {
            framebuffer::draw_text(fx + 24, body_y + 18, b"NETWORK HEARTBEAT", 0xD8E7FC)?;
            draw_kind_status(fx + 24, body_y + 36, shell_status)?;
            draw_metric_u64(
                fx + 24,
                body_y + 70,
                b"IPC QUEUE ",
                ipc_stats.queued_messages as u64,
                0xC5D8EE,
            )?;
            draw_metric_u64(
                fx + 24,
                body_y + 84,
                b"SPAWN ",
                lifecycle_stats.spawn_total,
                0xC5D8EE,
            )?;
            if shell_network_flip {
                framebuffer::draw_text(fx + 24, body_y + 98, b"NET: PING", 0x9AD5A1)?;
            } else {
                framebuffer::draw_text(fx + 24, body_y + 98, b"NET: OK", 0x9AD5A1)?;
            }
            framebuffer::fill_rounded_rect_alpha(fx + 20, fy + fh - 40, 132, 24, 8, 0x2A6CF0, 216)?;
            framebuffer::draw_text(fx + 34, fy + fh - 33, b"PING TOGGLE", 0xF4F9FF)?;
        }
        ActiveApp::Settings => {
            framebuffer::draw_text(fx + 24, body_y + 18, b"SYSTEM SETTINGS", 0xD8E7FC)?;
            draw_kind_status(fx + 24, body_y + 36, settings_status)?;
            draw_metric_u64(
                fx + 24,
                body_y + 70,
                b"FG PID ",
                lifecycle_stats.foreground_pid.0 as u64,
                0xC5D8EE,
            )?;
            framebuffer::draw_text(fx + 24, body_y + 84, b"WIFI: ON", 0xA9D4A8)?;
            if settings_airplane {
                framebuffer::draw_text(fx + 24, body_y + 98, b"AIRPLANE: ON", 0xF3C17A)?;
            } else {
                framebuffer::draw_text(fx + 24, body_y + 98, b"AIRPLANE: OFF", 0xF3C17A)?;
            }
            framebuffer::fill_rounded_rect_alpha(fx + 20, fy + fh - 40, 152, 24, 8, 0x4C84F2, 216)?;
            framebuffer::draw_text(fx + 32, fy + fh - 33, b"TOGGLE AIRPLANE", 0xF4F9FF)?;
        }
        ActiveApp::Files => {
            framebuffer::draw_text(fx + 24, body_y + 18, b"FILES", 0xD8E7FC)?;
            draw_kind_status(fx + 24, body_y + 36, files_status)?;
            draw_metric_u64(
                fx + 24,
                body_y + 70,
                b"RECORDS ",
                lifecycle_stats.record_count as u64,
                0xC5D8EE,
            )?;
            draw_file_row(fx + 24, body_y + 88, files_cursor == 0, b"OPENOS-RELEASE")?;
            draw_file_row(fx + 24, body_y + 106, files_cursor == 1, b"GESTURE-MAP")?;
            draw_file_row(fx + 24, body_y + 124, files_cursor == 2, b"LAUNCHER-HISTORY")?;
            framebuffer::fill_rounded_rect_alpha(fx + 20, fy + fh - 40, 116, 24, 8, 0x4281F2, 216)?;
            framebuffer::draw_text(fx + 35, fy + fh - 33, b"NEXT FILE", 0xF4F9FF)?;
            framebuffer::fill_rounded_rect_alpha(fx + 146, fy + fh - 40, 98, 24, 8, 0x2E4F7C, 212)?;
            framebuffer::draw_text(fx + 162, fy + fh - 33, b"OPEN", 0xF4F9FF)?;
        }
        ActiveApp::None => {}
    }

    Ok(())
}

fn draw_file_row(x: u32, y: u32, selected: bool, label: &[u8]) -> Result<(), FramebufferError> {
    if selected {
        framebuffer::fill_rounded_rect_alpha(x.saturating_sub(6), y.saturating_sub(3), 180, 16, 5, 0x4A6FA6, 148)?;
    }
    framebuffer::draw_text(x, y, label, 0xE7F0FD)
}

fn app_window_frame(width: u32, height: u32) -> Rect {
    let frame_w = clamp_u32(width.saturating_mul(46) / 100, 420, 640);
    let frame_h = clamp_u32(height.saturating_mul(52) / 100, 240, 420);
    let frame_x = width.saturating_sub(frame_w) / 2;
    let frame_y = clamp_u32(height.saturating_mul(18) / 100, 70, 180);
    target_frame(frame_x, frame_y, frame_w, frame_h)
}

fn active_app_title(app: ActiveApp) -> &'static [u8] {
    match app {
        ActiveApp::Shell => b"SHELL",
        ActiveApp::Settings => b"SETTINGS",
        ActiveApp::Files => b"FILES",
        ActiveApp::None => b"",
    }
}

fn draw_kind_status(x: u32, y: u32, status: lifecycle::KindStatus) -> Result<(), FramebufferError> {
    if !status.present {
        framebuffer::draw_text(x, y, b"LIFECYCLE: NOT LAUNCHED", 0xC7D5E8)?;
        return Ok(());
    }

    let (label, color) = if status.foreground {
        (b"LIFECYCLE: FOREGROUND" as &'static [u8], 0x9AD5A1)
    } else {
        match status.state {
            AppState::Foreground => (b"LIFECYCLE: FOREGROUND" as &'static [u8], 0x9AD5A1),
            AppState::Queued => (b"LIFECYCLE: QUEUED" as &'static [u8], 0xECD68B),
            AppState::Exited => (b"LIFECYCLE: EXITED" as &'static [u8], 0xEAA2A8),
        }
    };
    framebuffer::draw_text(x, y, label, color)?;
    draw_metric_u64(x, y + 14, b"PID ", status.pid.0 as u64, 0xC5D8EE)?;
    if status.state == AppState::Exited {
        draw_metric_i64(x + 84, y + 14, b"EXIT ", status.exit_status, 0xEAA2A8)?;
    }
    Ok(())
}

fn draw_metric_u64(
    x: u32,
    y: u32,
    prefix: &[u8],
    value: u64,
    color: u32,
) -> Result<(), FramebufferError> {
    let mut buf = [0u8; 48];
    let mut len = copy_prefix(&mut buf, prefix);
    len += encode_u64(value, &mut buf[len..]);
    framebuffer::draw_text(x, y, &buf[..len], color)
}

fn draw_metric_i64(
    x: u32,
    y: u32,
    prefix: &[u8],
    value: i64,
    color: u32,
) -> Result<(), FramebufferError> {
    let mut buf = [0u8; 48];
    let mut len = copy_prefix(&mut buf, prefix);
    len += encode_i64(value, &mut buf[len..]);
    framebuffer::draw_text(x, y, &buf[..len], color)
}

fn copy_prefix(dst: &mut [u8], prefix: &[u8]) -> usize {
    let count = min(dst.len(), prefix.len());
    dst[..count].copy_from_slice(&prefix[..count]);
    count
}

fn encode_u64(value: u64, out: &mut [u8]) -> usize {
    if out.is_empty() {
        return 0;
    }
    if value == 0 {
        out[0] = b'0';
        return 1;
    }

    let mut digits = [0u8; 20];
    let mut n = value;
    let mut idx = digits.len();
    while n != 0 && idx > 0 {
        idx -= 1;
        digits[idx] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    let count = min(out.len(), digits.len() - idx);
    out[..count].copy_from_slice(&digits[idx..idx + count]);
    count
}

fn encode_i64(value: i64, out: &mut [u8]) -> usize {
    if out.is_empty() {
        return 0;
    }

    if value < 0 {
        out[0] = b'-';
        1 + encode_u64(value.unsigned_abs(), &mut out[1..])
    } else {
        encode_u64(value as u64, out)
    }
}

fn build_home_targets(
    width: u32,
    height: u32,
    scene: SimpleScene,
    motion: MotionState,
) -> HomeTargets {
    let layout = compute_layout(width, height, scene.dock_height);
    let mut targets = HomeTargets::empty();
    let (
        active_app,
        clock_dx,
        clock_dy,
        match_dx,
        match_dy,
        weather_dx,
        weather_dy,
        dock_order,
    ) = unsafe {
        (
            UI_STATE.active_app,
            UI_STATE.widget_clock_dx,
            UI_STATE.widget_clock_dy,
            UI_STATE.widget_match_dx,
            UI_STATE.widget_match_dy,
            UI_STATE.widget_weather_dx,
            UI_STATE.widget_weather_dy,
            UI_STATE.dock_order,
        )
    };

    if active_app != ActiveApp::None {
        let app_frame = app_window_frame(width, height);
        let fx = app_frame.x.max(0) as u32;
        let fy = app_frame.y.max(0) as u32;
        let fw = app_frame.width;
        let fh = app_frame.height;

        push_target(
            &mut targets,
            TargetId::AppClose,
            target_frame(fx + fw.saturating_sub(32), fy + 10, 20, 20),
            TargetAction::CloseForegroundApp,
        );
        let primary_w = match active_app {
            ActiveApp::Shell => 132,
            ActiveApp::Settings => 152,
            ActiveApp::Files => 116,
            ActiveApp::None => 132,
        };
        push_target(
            &mut targets,
            TargetId::AppPrimary,
            target_frame(fx + 20, fy + fh.saturating_sub(40), primary_w, 24),
            TargetAction::ForegroundPrimary,
        );
        if active_app == ActiveApp::Files {
            push_target(
                &mut targets,
                TargetId::AppSecondary,
                target_frame(fx + 146, fy + fh.saturating_sub(40), 98, 24),
                TargetAction::ForegroundSecondary,
            );
        }
        return targets;
    }

    let dx = motion.content_dx;
    let dy = motion.content_dy;

    let clock_x = shift_u32(layout.left_margin, dx + clock_dx);
    let clock_y = shift_u32(layout.top_y, dy + clock_dy);
    push_target(
        &mut targets,
        TargetId::ClockWidget,
        target_frame(clock_x, clock_y, layout.widget_size, layout.widget_size),
        TargetAction::ToggleClock,
    );

    let match_x = shift_u32(
        layout.left_margin + layout.widget_size + layout.widget_gap,
        dx + match_dx,
    );
    let match_y = shift_u32(layout.top_y, dy + match_dy);
    push_target(
        &mut targets,
        TargetId::MatchWidget,
        target_frame(match_x, match_y, layout.widget_size, layout.widget_size),
        TargetAction::ToggleMatch,
    );

    let weather_x = shift_u32(layout.left_margin, dx + weather_dx);
    let weather_y = shift_u32(
        layout.top_y + layout.widget_size + layout.widget_gap,
        dy + weather_dy,
    );
    let weather_w = layout.widget_size.saturating_mul(2) + layout.widget_gap;
    push_target(
        &mut targets,
        TargetId::WeatherWidget,
        target_frame(weather_x, weather_y, weather_w, layout.weather_h),
        TargetAction::ToggleWeather,
    );

    let top_y = shift_u32(layout.top_y + 12, dy);
    let second_y = shift_u32(layout.top_y + 12 + layout.icon_size + layout.row_gap, dy);
    let third_y = shift_u32(
        layout.top_y
            + layout.widget_size
            + layout.widget_gap
            + layout.weather_h
            + clamp_u32(layout.widget_size / 3, 24, 44),
        dy,
    );

    let top_actions = [
        GestureAction::LaunchShell,
        GestureAction::LaunchShell,
        GestureAction::LaunchFiles,
        GestureAction::LaunchSettings,
    ];
    let second_actions = [
        GestureAction::LaunchSettings,
        GestureAction::LaunchFiles,
        GestureAction::LaunchFiles,
        GestureAction::LaunchShell,
    ];

    let mut i = 0usize;
    while i < top_actions.len() {
        let x = shift_u32(
            layout.icons_x0 + i as u32 * (layout.icon_size + layout.col_gap),
            dx,
        );
        push_target(
            &mut targets,
            TargetId::AppIcon(i),
            target_frame(x, top_y, layout.icon_size, layout.icon_size),
            TargetAction::Launch(top_actions[i]),
        );
        i += 1;
    }

    let mut j = 0usize;
    while j < second_actions.len() {
        let x = shift_u32(
            layout.icons_x0 + j as u32 * (layout.icon_size + layout.col_gap),
            dx,
        );
        push_target(
            &mut targets,
            TargetId::AppIcon(4 + j),
            target_frame(x, second_y, layout.icon_size, layout.icon_size),
            TargetAction::Launch(second_actions[j]),
        );
        j += 1;
    }

    let x0 = shift_u32(layout.left_margin + 10, dx);
    let x1 = shift_u32(
        layout.left_margin + 10 + layout.icon_size + clamp_u32(layout.col_gap / 2 + 14, 18, 36),
        dx,
    );
    push_target(
        &mut targets,
        TargetId::AppIcon(8),
        target_frame(x0, third_y, layout.icon_size, layout.icon_size),
        TargetAction::Launch(GestureAction::LaunchFiles),
    );
    push_target(
        &mut targets,
        TargetId::AppIcon(9),
        target_frame(x1, third_y, layout.icon_size, layout.icon_size),
        TargetAction::Launch(GestureAction::LaunchSettings),
    );

    let icon_size = clamp_u32(width / 22, 42, 52);
    let gap = clamp_u32(width / 118, 7, 10);
    let pad = 14u32;
    let separator_after = 5usize;
    let separator_w = 12u32;

    let icons_w = icon_size.saturating_mul(DOCK_ICON_COUNT as u32)
        + gap.saturating_mul((DOCK_ICON_COUNT - 1) as u32)
        + separator_w;
    let dock_w = icons_w + pad * 2;
    let dock_h = icon_size + 18;
    let dock_x = width.saturating_sub(dock_w) / 2;
    let dock_y = shift_u32(
        height.saturating_sub(max(scene.dock_height, 98)),
        -motion.dock_lift,
    );
    let y = dock_y + (dock_h.saturating_sub(icon_size)) / 2;

    let dock_actions = [
        GestureAction::LaunchFiles,
        GestureAction::LaunchShell,
        GestureAction::LaunchShell,
        GestureAction::LaunchFiles,
        GestureAction::LaunchFiles,
        GestureAction::LaunchShell,
        GestureAction::LaunchShell,
        GestureAction::LaunchSettings,
        GestureAction::LaunchFiles,
        GestureAction::LaunchFiles,
    ];

    let mut dock_x_cursor = dock_x + pad;
    let mut d = 0usize;
    while d < DOCK_ICON_COUNT {
        let slot = (dock_order[d] as usize).min(dock_actions.len().saturating_sub(1));
        push_target(
            &mut targets,
            TargetId::DockIcon(d),
            target_frame(dock_x_cursor, y, icon_size, icon_size),
            TargetAction::Launch(dock_actions[slot]),
        );
        dock_x_cursor += icon_size + gap;
        if d == separator_after {
            dock_x_cursor += separator_w;
        }
        d += 1;
    }

    targets
}

fn push_target(targets: &mut HomeTargets, id: TargetId, frame: Rect, action: TargetAction) {
    if targets.len >= targets.items.len() {
        return;
    }
    targets.items[targets.len] = HomeTarget { id, frame, action };
    targets.len += 1;
}

fn target_frame(x: u32, y: u32, width: u32, height: u32) -> Rect {
    Rect {
        x: x as i32,
        y: y as i32,
        width,
        height,
    }
}

fn hit_test_targets(targets: &HomeTargets, x: u32, y: u32) -> Option<usize> {
    let mut i = 0usize;
    while i < targets.len {
        if targets.items[i].frame.contains(x, y) {
            return Some(i);
        }
        i += 1;
    }
    None
}

unsafe fn apply_target_action(action: TargetAction) -> Option<GestureAction> {
    match action {
        TargetAction::ToggleClock => {
            UI_STATE.clock_digital = !UI_STATE.clock_digital;
            None
        }
        TargetAction::ToggleMatch => {
            UI_STATE.match_expanded = !UI_STATE.match_expanded;
            None
        }
        TargetAction::ToggleWeather => {
            UI_STATE.weather_fahrenheit = !UI_STATE.weather_fahrenheit;
            None
        }
        TargetAction::CloseForegroundApp => {
            UI_STATE.active_app = ActiveApp::None;
            None
        }
        TargetAction::ForegroundPrimary => {
            match UI_STATE.active_app {
                ActiveApp::Shell => {
                    UI_STATE.shell_network_flip = !UI_STATE.shell_network_flip;
                }
                ActiveApp::Settings => {
                    UI_STATE.settings_airplane = !UI_STATE.settings_airplane;
                }
                ActiveApp::Files => {
                    UI_STATE.files_cursor = (UI_STATE.files_cursor + 1) % 3;
                }
                ActiveApp::None => {}
            }
            None
        }
        TargetAction::ForegroundSecondary => {
            if UI_STATE.active_app == ActiveApp::Files {
                UI_STATE.files_cursor = 0;
            }
            None
        }
        TargetAction::Launch(action) => {
            UI_STATE.active_app = active_app_for_launch(action);
            Some(action)
        }
    }
}

fn drag_target_for_id(id: TargetId) -> DragTarget {
    match id {
        TargetId::ClockWidget => DragTarget::ClockWidget,
        TargetId::MatchWidget => DragTarget::MatchWidget,
        TargetId::WeatherWidget => DragTarget::WeatherWidget,
        TargetId::DockIcon(slot) => DragTarget::DockIcon(slot),
        _ => DragTarget::None,
    }
}

fn active_app_for_launch(action: GestureAction) -> ActiveApp {
    match action {
        GestureAction::LaunchShell => ActiveApp::Shell,
        GestureAction::LaunchSettings => ActiveApp::Settings,
        GestureAction::LaunchFiles => ActiveApp::Files,
        _ => ActiveApp::None,
    }
}

fn draw_hover_press_effects(targets: &HomeTargets) -> Result<(), FramebufferError> {
    let (hover_idx, pressed_idx, pointer_visible) = unsafe {
        (
            UI_STATE.hover_index,
            UI_STATE.pressed_index,
            UI_STATE.pointer_visible,
        )
    };
    if !pointer_visible || targets.len == 0 {
        return Ok(());
    }

    if hover_idx < targets.len {
        draw_target_effect(targets.items[hover_idx].frame, 0xFFFFFF, 42)?;
    }
    if pressed_idx < targets.len {
        draw_target_effect(targets.items[pressed_idx].frame, 0x8BB6FF, 76)?;
    }
    Ok(())
}

fn draw_target_effect(frame: Rect, color: u32, alpha: u8) -> Result<(), FramebufferError> {
    let x = frame.x.max(0) as u32;
    let y = frame.y.max(0) as u32;
    let w = frame.width;
    let h = frame.height;
    let radius = clamp_u32(min(w, h) / 4 + 3, 10, 22);

    framebuffer::fill_rounded_rect_alpha(
        x.saturating_sub(2),
        y.saturating_sub(2),
        w.saturating_add(4),
        h.saturating_add(4),
        radius,
        color,
        alpha,
    )
}

fn draw_focus_indicator(targets: &HomeTargets) -> Result<(), FramebufferError> {
    if targets.len == 0 {
        return Ok(());
    }

    let focus = unsafe {
        if UI_STATE.focus_index >= targets.len {
            UI_STATE.focus_index = 0;
        }
        UI_STATE.focus_index
    };
    let frame = targets.items[focus].frame;
    let x = frame.x.max(0) as u32;
    let y = frame.y.max(0) as u32;
    let w = frame.width;
    let h = frame.height;
    let radius = clamp_u32(min(w, h) / 4 + 4, 12, 24);

    framebuffer::fill_rounded_rect_alpha(
        x.saturating_sub(4),
        y.saturating_sub(4),
        w.saturating_add(8),
        h.saturating_add(8),
        radius,
        0xBFD9FF,
        74,
    )?;
    framebuffer::fill_rounded_rect_alpha(
        x.saturating_sub(2),
        y.saturating_sub(2),
        w.saturating_add(4),
        h.saturating_add(4),
        radius.saturating_sub(2),
        0xFFFFFF,
        64,
    )?;
    Ok(())
}

fn draw_pointer_cursor(width: u32, height: u32) -> Result<(), FramebufferError> {
    let (x, y, visible) = unsafe {
        if !UI_STATE.pointer_initialized {
            UI_STATE.pointer_x = width / 2;
            UI_STATE.pointer_y = height / 2;
            UI_STATE.pointer_initialized = true;
        }
        (
            UI_STATE.pointer_x,
            UI_STATE.pointer_y,
            UI_STATE.pointer_visible,
        )
    };

    if !visible {
        return Ok(());
    }

    let x = x.min(width.saturating_sub(1));
    let y = y.min(height.saturating_sub(1));

    framebuffer::fill_rect_alpha(x, y, 2, 14, 0x0E131A, 210)?;
    framebuffer::fill_rect_alpha(x, y, 9, 2, 0x0E131A, 210)?;
    framebuffer::fill_rect_alpha(x + 2, y + 2, 2, 9, 0xFFFFFF, 236)?;
    framebuffer::fill_rect_alpha(x + 2, y + 2, 7, 2, 0xFFFFFF, 236)?;
    framebuffer::fill_circle_alpha(x + 1, y + 1, 2, 0xFFFFFF, 236)?;
    Ok(())
}

fn draw_badge(x: u32, y: u32, count: u8) -> Result<(), FramebufferError> {
    let (text, text_x) = badge_text(count);
    framebuffer::fill_circle_alpha(x, y, 9, 0xF44A57, 230)?;
    framebuffer::draw_text(
        x.saturating_sub(text_x),
        y.saturating_sub(4),
        text,
        0xFFFFFF,
    )?;
    Ok(())
}

fn badge_text(count: u8) -> (&'static [u8], u32) {
    match count {
        0 => (b"", 0),
        1..=9 => {
            let bytes = match count {
                1 => b"1",
                2 => b"2",
                3 => b"3",
                4 => b"4",
                5 => b"5",
                6 => b"6",
                7 => b"7",
                8 => b"8",
                _ => b"9",
            };
            (bytes, 3)
        }
        10..=99 => {
            let bytes = match count {
                66 => b"66",
                _ => b"10",
            };
            (bytes, 7)
        }
        _ => (b"99", 7),
    }
}

fn draw_icon_symbol(kind: IconKind, x: u32, y: u32, size: u32) -> Result<(), FramebufferError> {
    let cx = x + size / 2;
    let cy = y + size / 2;

    match kind {
        IconKind::Messages => {
            framebuffer::fill_rounded_rect_alpha(
                cx.saturating_sub(10),
                cy.saturating_sub(8),
                20,
                14,
                5,
                0xFFFFFF,
                204,
            )?;
            framebuffer::fill_rect_alpha(cx.saturating_sub(3), cy + 7, 6, 3, 0xFFFFFF, 204)?;
        }
        IconKind::FaceTime => {
            framebuffer::fill_rounded_rect_alpha(
                cx.saturating_sub(12),
                cy.saturating_sub(8),
                16,
                14,
                4,
                0xFFFFFF,
                204,
            )?;
            framebuffer::fill_rounded_rect_alpha(
                cx + 4,
                cy.saturating_sub(5),
                8,
                8,
                3,
                0xFFFFFF,
                204,
            )?;
        }
        IconKind::Files => {
            framebuffer::fill_rounded_rect_alpha(
                cx.saturating_sub(11),
                cy.saturating_sub(9),
                22,
                18,
                4,
                0x2E92F0,
                200,
            )?;
            framebuffer::fill_rect_alpha(
                cx.saturating_sub(11),
                cy.saturating_sub(9),
                9,
                4,
                0x8AD0FF,
                204,
            )?;
        }
        IconKind::Reminders => {
            framebuffer::fill_rounded_rect_alpha(
                cx.saturating_sub(11),
                cy.saturating_sub(10),
                22,
                20,
                4,
                0xFFFFFF,
                222,
            )?;
            framebuffer::fill_circle_alpha(
                cx.saturating_sub(7),
                cy.saturating_sub(4),
                2,
                0x57A3F5,
                222,
            )?;
            framebuffer::fill_circle_alpha(cx.saturating_sub(7), cy + 2, 2, 0xF58E68, 222)?;
            framebuffer::fill_circle_alpha(cx.saturating_sub(7), cy + 8, 2, 0xF2C64F, 222)?;
            framebuffer::fill_rect_alpha(
                cx.saturating_sub(2),
                cy.saturating_sub(5),
                10,
                2,
                0xC4D2E3,
                210,
            )?;
            framebuffer::fill_rect_alpha(cx.saturating_sub(2), cy + 1, 10, 2, 0xC4D2E3, 210)?;
            framebuffer::fill_rect_alpha(cx.saturating_sub(2), cy + 7, 10, 2, 0xC4D2E3, 210)?;
        }
        IconKind::AppStore => {
            framebuffer::fill_rect_alpha(
                cx.saturating_sub(1),
                cy.saturating_sub(9),
                2,
                18,
                0xFFFFFF,
                214,
            )?;
            framebuffer::fill_rect_alpha(cx.saturating_sub(8), cy + 4, 16, 2, 0xFFFFFF, 214)?;
            framebuffer::fill_rect_alpha(
                cx.saturating_sub(7),
                cy.saturating_sub(5),
                2,
                10,
                0xFFFFFF,
                214,
            )?;
            framebuffer::fill_rect_alpha(cx + 5, cy.saturating_sub(5), 2, 10, 0xFFFFFF, 214)?;
        }
        IconKind::Navigation => {
            framebuffer::fill_rounded_rect_alpha(
                cx.saturating_sub(12),
                cy.saturating_sub(10),
                24,
                20,
                4,
                0x0A2D68,
                220,
            )?;
            let mut px = cx.saturating_sub(8);
            let mut py = cy.saturating_sub(6);
            let mut i = 0usize;
            while i < 8 {
                framebuffer::fill_circle_alpha(px, py, 2, mini_nav_color(i), 220)?;
                px += 6;
                if (i & 3) == 3 {
                    px = cx.saturating_sub(8);
                    py += 6;
                }
                i += 1;
            }
        }
        IconKind::Books => {
            framebuffer::fill_rounded_rect_alpha(
                cx.saturating_sub(11),
                cy.saturating_sub(10),
                9,
                20,
                3,
                0xFFF2D5,
                220,
            )?;
            framebuffer::fill_rounded_rect_alpha(
                cx + 2,
                cy.saturating_sub(10),
                9,
                20,
                3,
                0xFFF2D5,
                220,
            )?;
            framebuffer::fill_rect_alpha(cx, cy.saturating_sub(10), 2, 20, 0xDB8C31, 210)?;
        }
        IconKind::Podcasts => {
            framebuffer::fill_circle_alpha(cx, cy, 10, 0xF0D7FF, 210)?;
            framebuffer::fill_circle_alpha(cx, cy, 6, 0xB26AEF, 220)?;
            framebuffer::fill_circle_alpha(cx, cy, 2, 0xFFFFFF, 230)?;
            framebuffer::fill_rect_alpha(cx.saturating_sub(1), cy + 2, 2, 8, 0xFFFFFF, 220)?;
        }
        IconKind::Photos => {
            framebuffer::fill_circle_alpha(cx, cy.saturating_sub(8), 4, 0xFF6B7A, 220)?;
            framebuffer::fill_circle_alpha(cx + 7, cy.saturating_sub(4), 4, 0xFFA15D, 220)?;
            framebuffer::fill_circle_alpha(cx + 7, cy + 4, 4, 0xFFD45F, 220)?;
            framebuffer::fill_circle_alpha(cx, cy + 8, 4, 0x69D06B, 220)?;
            framebuffer::fill_circle_alpha(cx.saturating_sub(7), cy + 4, 4, 0x5AC7F5, 220)?;
            framebuffer::fill_circle_alpha(
                cx.saturating_sub(7),
                cy.saturating_sub(4),
                4,
                0x7486F2,
                220,
            )?;
            framebuffer::fill_circle_alpha(cx, cy, 3, 0xFFFFFF, 220)?;
        }
        IconKind::Settings => {
            framebuffer::fill_circle_alpha(cx, cy, 10, 0x6D727C, 222)?;
            framebuffer::fill_circle_alpha(cx, cy, 5, 0xBEC5CF, 222)?;
            framebuffer::fill_rect_alpha(
                cx.saturating_sub(1),
                cy.saturating_sub(14),
                2,
                4,
                0x9CA5B2,
                210,
            )?;
            framebuffer::fill_rect_alpha(cx.saturating_sub(1), cy + 10, 2, 4, 0x9CA5B2, 210)?;
            framebuffer::fill_rect_alpha(
                cx.saturating_sub(14),
                cy.saturating_sub(1),
                4,
                2,
                0x9CA5B2,
                210,
            )?;
            framebuffer::fill_rect_alpha(cx + 10, cy.saturating_sub(1), 4, 2, 0x9CA5B2, 210)?;
        }
        IconKind::Safari => {
            framebuffer::fill_circle_alpha(cx, cy, size / 4 + 2, 0xFFFFFF, 206)?;
            framebuffer::fill_circle_alpha(cx, cy, size / 4, 0xA8D2FF, 226)?;
            framebuffer::fill_rect_alpha(
                cx.saturating_sub(1),
                cy.saturating_sub(8),
                2,
                16,
                0x2A5D97,
                226,
            )?;
            framebuffer::fill_rect_alpha(
                cx.saturating_sub(8),
                cy.saturating_sub(1),
                16,
                2,
                0x2A5D97,
                226,
            )?;
            framebuffer::fill_rect_alpha(cx, cy.saturating_sub(7), 2, 8, 0xF05B63, 220)?;
            framebuffer::fill_rect_alpha(cx.saturating_sub(1), cy, 8, 2, 0xF05B63, 220)?;
        }
        IconKind::Music => {
            framebuffer::fill_rect_alpha(cx + 2, cy.saturating_sub(8), 3, 13, 0xFFFFFF, 206)?;
            framebuffer::fill_rect_alpha(
                cx.saturating_sub(5),
                cy.saturating_sub(8),
                10,
                3,
                0xFFFFFF,
                206,
            )?;
            framebuffer::fill_circle_alpha(cx.saturating_sub(3), cy + 5, 4, 0xFFFFFF, 206)?;
            framebuffer::fill_circle_alpha(cx + 4, cy + 3, 4, 0xFFFFFF, 206)?;
        }
        IconKind::Mail => {
            framebuffer::fill_rounded_rect_alpha(
                cx.saturating_sub(11),
                cy.saturating_sub(8),
                22,
                16,
                4,
                0xFFFFFF,
                206,
            )?;
            framebuffer::fill_rect_alpha(
                cx.saturating_sub(9),
                cy.saturating_sub(1),
                18,
                2,
                0x5A84B6,
                218,
            )?;
            framebuffer::fill_rect_alpha(
                cx.saturating_sub(9),
                cy.saturating_sub(7),
                2,
                7,
                0x5A84B6,
                206,
            )?;
            framebuffer::fill_rect_alpha(cx + 7, cy.saturating_sub(7), 2, 7, 0x5A84B6, 206)?;
        }
        IconKind::Camera => {
            framebuffer::fill_rounded_rect_alpha(
                cx.saturating_sub(12),
                cy.saturating_sub(8),
                24,
                16,
                4,
                0xDDE9F8,
                218,
            )?;
            framebuffer::fill_circle_alpha(cx, cy, 5, 0x5F85B7, 226)?;
            framebuffer::fill_circle_alpha(cx, cy, 2, 0xDCE8F8, 228)?;
            framebuffer::fill_rect_alpha(
                cx.saturating_sub(6),
                cy.saturating_sub(10),
                8,
                3,
                0xDDE9F8,
                218,
            )?;
        }
        IconKind::Notes => {
            framebuffer::fill_rounded_rect_alpha(
                cx.saturating_sub(11),
                cy.saturating_sub(10),
                22,
                20,
                4,
                0xFFFFFF,
                212,
            )?;
            framebuffer::fill_rounded_rect_alpha(
                cx.saturating_sub(11),
                cy.saturating_sub(10),
                22,
                6,
                4,
                0xF4D45E,
                226,
            )?;
            framebuffer::fill_rect_alpha(cx.saturating_sub(7), cy, 14, 2, 0xC7D5E6, 208)?;
            framebuffer::fill_rect_alpha(cx.saturating_sub(7), cy + 4, 10, 2, 0xC7D5E6, 208)?;
        }
        IconKind::Brush => {
            framebuffer::fill_rect_alpha(cx.saturating_sub(9), cy + 4, 18, 2, 0x9A6AF2, 212)?;
            framebuffer::fill_rect_alpha(cx.saturating_sub(6), cy + 1, 12, 2, 0xAC77F7, 212)?;
            framebuffer::fill_rect_alpha(
                cx.saturating_sub(3),
                cy.saturating_sub(2),
                8,
                2,
                0xC689FF,
                212,
            )?;
        }
        IconKind::Reddit => {
            framebuffer::fill_circle_alpha(cx, cy, 10, 0xFFFFFF, 220)?;
            framebuffer::fill_circle_alpha(cx.saturating_sub(3), cy, 1, 0xF06E3C, 226)?;
            framebuffer::fill_circle_alpha(cx + 3, cy, 1, 0xF06E3C, 226)?;
            framebuffer::fill_rect_alpha(cx.saturating_sub(3), cy + 4, 7, 2, 0xF06E3C, 220)?;
            framebuffer::fill_rect_alpha(cx + 5, cy.saturating_sub(8), 6, 2, 0xFFFFFF, 220)?;
            framebuffer::fill_circle_alpha(cx + 11, cy.saturating_sub(7), 2, 0xFFFFFF, 220)?;
        }
        IconKind::FolderGrid => {
            framebuffer::fill_rounded_rect_alpha(
                cx.saturating_sub(11),
                cy.saturating_sub(9),
                22,
                18,
                5,
                0xE9F0FB,
                212,
            )?;
            framebuffer::fill_rounded_rect_alpha(
                cx.saturating_sub(8),
                cy.saturating_sub(5),
                7,
                6,
                2,
                0x8EC8F8,
                210,
            )?;
            framebuffer::fill_rounded_rect_alpha(
                cx + 1,
                cy.saturating_sub(5),
                7,
                6,
                2,
                0xA4D9A7,
                210,
            )?;
            framebuffer::fill_rounded_rect_alpha(
                cx.saturating_sub(8),
                cy + 3,
                7,
                5,
                2,
                0xF3C67B,
                210,
            )?;
            framebuffer::fill_rounded_rect_alpha(cx + 1, cy + 3, 7, 5, 2, 0xBCA9F2, 210)?;
        }
    }
    Ok(())
}

fn mini_nav_color(index: usize) -> u32 {
    match index {
        0 => 0xF4CE61,
        1 => 0x9EC6F4,
        2 => 0x72BAF0,
        3 => 0xF17367,
        4 => 0xA2CDF7,
        5 => 0xA3D9AC,
        6 => 0xB48CF4,
        _ => 0xFFFFFF,
    }
}

fn dial_offset(index: u32, radius: u32) -> (i32, i32) {
    let r = radius as i32;
    match index {
        0 => (0, -r),
        1 => (r / 2, -(r * 86 / 100)),
        2 => ((r * 86 / 100), -r / 2),
        3 => (r, 0),
        4 => ((r * 86 / 100), r / 2),
        5 => (r / 2, r * 86 / 100),
        6 => (0, r),
        7 => (-r / 2, r * 86 / 100),
        8 => (-(r * 86 / 100), r / 2),
        9 => (-r, 0),
        10 => (-(r * 86 / 100), -r / 2),
        _ => (-r / 2, -(r * 86 / 100)),
    }
}

fn idle_motion(frame: u32) -> MotionState {
    MotionState {
        content_dx: triangle_wave(frame, 210, 2),
        content_dy: triangle_wave(frame.wrapping_add(59), 280, 1),
        dock_lift: triangle_wave(frame.wrapping_add(97), 190, 2),
    }
}

fn transition_motion(action: GestureAction, step: u32, steps: u32) -> MotionState {
    let steps = if steps == 0 { 1 } else { steps };
    let inv = steps.saturating_sub(step.min(steps)) as i32;
    let slide = inv.saturating_mul(34) / steps as i32;

    match action {
        GestureAction::Home => MotionState {
            content_dx: 0,
            content_dy: slide / 2,
            dock_lift: slide / 3,
        },
        GestureAction::AppSwitcherLeft => MotionState {
            content_dx: -slide,
            content_dy: 0,
            dock_lift: slide / 4,
        },
        GestureAction::AppSwitcherRight => MotionState {
            content_dx: slide,
            content_dy: 0,
            dock_lift: slide / 4,
        },
        GestureAction::ControlCenter => MotionState {
            content_dx: 0,
            content_dy: slide / 2,
            dock_lift: slide / 2,
        },
        GestureAction::NotificationCenter => MotionState {
            content_dx: 0,
            content_dy: -slide / 2,
            dock_lift: slide / 2,
        },
        GestureAction::LaunchShell | GestureAction::LaunchSettings | GestureAction::LaunchFiles => {
            MotionState {
                content_dx: 0,
                content_dy: 0,
                dock_lift: slide / 5,
            }
        }
    }
}

fn interpolate_scene(from: SimpleScene, to: SimpleScene, step: u32, steps: u32) -> SimpleScene {
    let steps = if steps == 0 { 1 } else { steps };
    let numer = step.min(steps);
    SimpleScene {
        top_color: interpolate_rgb(from.top_color, to.top_color, numer, steps),
        bottom_color: interpolate_rgb(from.bottom_color, to.bottom_color, numer, steps),
        dock_color: interpolate_rgb(from.dock_color, to.dock_color, numer, steps),
        dock_height: lerp_u32(from.dock_height, to.dock_height, numer, steps),
    }
}

fn lerp_u32(from: u32, to: u32, numer: u32, denom: u32) -> u32 {
    let denom = if denom == 0 { 1 } else { denom };
    let numer = numer.min(denom);
    let inv = denom.saturating_sub(numer);
    (from
        .saturating_mul(inv)
        .saturating_add(to.saturating_mul(numer))
        .saturating_add(denom / 2))
        / denom
}

fn interpolate_rgb(from: u32, to: u32, numer: u32, denom: u32) -> u32 {
    let denom = if denom == 0 { 1 } else { denom };
    let numer = numer.min(denom);
    let inv = denom.saturating_sub(numer);

    let fr = (from >> 16) & 0xFF;
    let fg = (from >> 8) & 0xFF;
    let fb = from & 0xFF;

    let tr = (to >> 16) & 0xFF;
    let tg = (to >> 8) & 0xFF;
    let tb = to & 0xFF;

    let r = (fr * inv + tr * numer + denom / 2) / denom;
    let g = (fg * inv + tg * numer + denom / 2) / denom;
    let b = (fb * inv + tb * numer + denom / 2) / denom;
    (r << 16) | (g << 8) | b
}

fn shift_u32(value: u32, delta: i32) -> u32 {
    if delta >= 0 {
        value.saturating_add(delta as u32)
    } else {
        value.saturating_sub((-delta) as u32)
    }
}

fn triangle_wave(frame: u32, period: u32, amplitude: i32) -> i32 {
    if period < 2 || amplitude <= 0 {
        return 0;
    }

    let period = period as i32;
    let half = period / 2;
    let mut t = (frame % period as u32) as i32;
    if t >= half {
        t = period - t;
    }
    (t * amplitude * 2 / half.max(1)) - amplitude
}

fn transition_spin() {
    let mut i = 0u32;
    while i < TRANSITION_SPIN {
        core::hint::spin_loop();
        i += 1;
    }
}

fn clamp_u32(value: u32, floor: u32, ceiling: u32) -> u32 {
    min(ceiling, max(floor, value))
}

fn clamp_i32(value: i32, floor: i32, ceiling: i32) -> i32 {
    min(ceiling, max(floor, value))
}

fn map_framebuffer_error(err: FramebufferError) -> PresentError {
    match err {
        FramebufferError::Unavailable | FramebufferError::UnsupportedFormat => {
            PresentError::FramebufferUnavailable
        }
    }
}
