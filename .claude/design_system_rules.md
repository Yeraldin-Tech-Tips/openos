# OpenOS Design System Rules

## Overview
OpenOS is a from-scratch x86_64 operating system with an iOS/iPadOS-inspired shell. The UI is rendered directly to the framebuffer using Rust, with no web technologies involved. This document defines the design tokens, patterns, and conventions for maintaining visual consistency across the system.

## Technology Stack
- **Language**: Rust (no_std environment)
- **Rendering**: Direct framebuffer manipulation via `kernel/src/ui/framebuffer.rs`
- **Compositor**: `kernel/src/ui/compositor.rs` handles scene composition and presentation
- **Syscalls**: `gfx_submit_scene()` and `gfx_present()` for rendering
- **Apps**: Located in `userspace/app-*-payload/` and `userspace/apps/`

---

## 1. Color Palette

### System Scene Colors (Dark Theme)
These colors define the gradient backgrounds for system-level UI states:

```rust
// Default compositor scenes (kernel/src/ui/compositor.rs)
DEFAULT_SCENE:              top: 0x040A20, bottom: 0x07163A, dock: 0x1E2D3F
HOME_SCENE:                 top: 0x040A20, bottom: 0x07183F, dock: 0x1F2F42
APP_SWITCHER_LEFT_SCENE:    top: 0x03091A, bottom: 0x061432, dock: 0x1B2838
APP_SWITCHER_RIGHT_SCENE:   top: 0x040A1D, bottom: 0x07163A, dock: 0x1D2B3B
CONTROL_CENTER_SCENE:       top: 0x020714, bottom: 0x06112A, dock: 0x182535
NOTIFICATION_SCENE:         top: 0x020714, bottom: 0x06122D, dock: 0x182637
```

### App-Specific Colors (Light Theme)
Apps define their own gradient scenes using lighter, pastel tones:

```rust
// Shell app workspaces (userspace/app-shell-payload/src/main.rs)
Workspace 0 (blue):         top: 0xDCEBFF, bottom: 0xB7CCF2, dock: 0xE8F2FF
Workspace 1 (light blue):   top: 0xD5F1FF, bottom: 0xA8D5F1, dock: 0xE4F6FF
Workspace 2 (purple-blue):  top: 0xE2ECFF, bottom: 0xB9CAF0, dock: 0xECF3FF

// Settings app (userspace/app-settings-payload/src/main.rs)
Settings (mint green):      top: 0xE5FFF4, bottom: 0xC7F2E1, dock: 0xEEFFF7

// Files app (userspace/app-files-payload/src/main.rs)
Files (warm beige):         top: 0xFFF5E6, bottom: 0xF2DFC5, dock: 0xFFF8EE
```

### UI Element Colors

```rust
// Status bar
STATUS_TEXT:        0xF2F6FF  // Light blue-white
BATTERY_GREEN:      0x7CE877  // Battery fill
WIFI_INDICATOR:     0xF2F6FF  // WiFi bars

// Text hierarchy
PRIMARY_TEXT:       0xFFFFFF  // Pure white for primary content
SECONDARY_TEXT:     0xEEF4FF  // Slightly dimmed for labels
TERTIARY_TEXT:      0xC5D6EC  // Muted for timestamps/metadata
QUATERNARY_TEXT:    0xCAD9ED  // Dimmed for secondary info

// Wallpaper accents (compositor.rs:517-569)
WALLPAPER_BASE:     0x050A16  // Dark base overlay
BLUE_PRIMARY:       0x1B5BE3  // Main blue sweep
BLUE_SECONDARY:     0x2D83F5  // Lighter blue
DARK_CURTAIN:       0x05070D  // Dark overlay
GREEN_ACCENT:       0x6AD246  // Green lower-right shape
SEAM_BLEND:         0x1B4EAE  // Blue-green overlap
```

### App Icon Background Colors

```rust
// Primary apps (compositor.rs:746-810)
MESSAGES:           0x4FCA71  // Green
FACETIME:           0x57C96A  // Green
FILES:              0xF2F6FC  // Light gray/white
REMINDERS:          0xF5F7FC  // Light gray/white
APP_STORE:          0x5BA7F7  // Blue
NAVIGATION:         0x102D69  // Dark blue
BOOKS:              0xF3A744  // Orange
PODCASTS:           0x9C4BDD  // Purple
PHOTOS:             0xF6F8FC  // Light gray/white
SETTINGS:           0xD0D5DD  // Gray

// Dock apps (compositor.rs:945-1006)
SAFARI:             0x67AEF8  // Blue
MUSIC:              0xF26382  // Pink/red
MAIL:               0x6BAEF8  // Blue
CAMERA:             0x87B4EC  // Light blue
NOTES:              0xF4E27C  // Yellow
BRUSH:              0x171726  // Dark gray/black
REDDIT:             0xF06E3C  // Orange
FOLDER_GRID:        0xBED4F2  // Light blue

// Badge
BADGE_RED:          0xF44A57  // Notification badge
```

### Color Usage Guidelines

1. **Scene Backgrounds**: Always use vertical gradients (top → bottom)
2. **App Themes**: Each app should have a unique color palette that distinguishes it
3. **Dock Color**: Should match the scene's dock_color for visual continuity
4. **Alpha Blending**: Use alpha values (0-255) for layering and translucency
5. **Contrast**: Ensure text has sufficient contrast against backgrounds (light text on dark, dark text on light)

---

## 2. Layout System

### Dock Specifications

```rust
// Dock dimensions (compositor.rs:896-1027)
dock_height: 102-110px (typically 104-108px)
icon_size: clamp(width / 22, 42, 52)  // Responsive
icon_gap: clamp(width / 118, 7, 10)
padding: 14px
separator_width: 12px (after 5th icon)
separator_position: After 5th icon
icon_count: 10

// Dock positioning
dock_y: height - max(scene.dock_height, 98)
```

### Home Screen Layout

```rust
// Widget layout (compositor.rs:470-515)
left_margin: clamp(width / 10, 42, 118)
top_y: clamp(height / 11, 52, 84)
widget_size: clamp(icon_size + 58, 118, 150)
widget_gap: clamp(width / 58, 12, 22)
weather_height: clamp(widget_size - 4, 108, 150)

// App grid (compositor.rs:733-861)
icon_size: clamp(width / 19, 56, 66)
col_gap: clamp(width / 24, 22, 46)
row_gap: clamp(height / 10, 52, 82)
icons_x0: left_margin + (widget_size * 2) + widget_gap + clamp(width / 18, 30, 62)

// Layout structure
- Clock widget (top-left)
- Match widget (top-center)
- Weather widget (below clock + match, spans both)
- App grid (right side, 4 columns)
- Third row apps (bottom-left, 2 icons)
```

### Responsive Sizing

All dimensions use clamped calculations for different screen sizes:

```rust
clamp_u32(value, floor, ceiling)
// Example: icon_size = clamp(width / 19, 56, 66)
// On 1920px wide: 1920/19 = 101 → clamped to 66px
// On 800px wide: 800/19 = 42 → clamped to 56px
```

### Status Bar

```rust
// Status bar (compositor.rs:572-588)
height: ~30px (text at y=12)
time_x: 14px
battery_x: width - 22px
wifi_x: width - 92px
percent_x: width - 62px
text_color: 0xF2F6FF
```

---

## 3. Component Patterns

### Rounded Rectangles

All card-like UI elements use rounded corners:

```rust
// Border radius calculations
widget_radius: clamp(size / 7, 18, 24)
icon_radius: clamp(size / 4, 10, 18)
dock_radius: 24px (fixed)

// Usage pattern
framebuffer::fill_rounded_rect_alpha(x, y, width, height, radius, color, alpha)
```

### Shadows

Shadows are created using offset, darker rectangles with alpha:

```rust
// Shadow pattern (used everywhere)
framebuffer::fill_rounded_rect_alpha(x + 4, y + 8, w, h, radius, 0x060B16, 136)  // Shadow
framebuffer::fill_rounded_rect_alpha(x, y, w, h, radius, main_color, 234)       // Main
```

### Glossy Effect

Many UI elements have a white/light overlay on the top half:

```rust
// Gloss overlay (icons, widgets)
framebuffer::fill_rounded_rect_alpha(
    x + 2,
    y + 2,
    size - 4,
    size / 2,              // Top half only
    radius - 2,
    0xFFFFFF,
    22-62                  // Low alpha for subtle shine
)
```

### Badges

```rust
// Badge specification (compositor.rs:1309-1347)
radius: 9px (circle)
background: 0xF44A57 (red)
text: white (0xFFFFFF)
alpha: 230
position: top-right of icon (x = icon_x + size - 6, y = icon_y + 8)
text_offset: Single digit: -3px, Double digit: -7px
```

### Widgets

#### Clock Widget
```rust
// Clock widget (compositor.rs:624-659)
size: widget_size (118-150px)
background: 0x0D1B43
modes: analog (default) or digital (toggleable)
// Analog: white circle with gray hands
// Digital: "09:48" + "THU AUG 11"
```

#### Match/Sports Widget
```rust
// Match widget (compositor.rs:661-687)
size: widget_size
background: 0x0A0C12
elements: Team icons, score, match info
expandable: Shows full score vs. next match time
```

#### Weather Widget
```rust
// Weather widget (compositor.rs:689-731)
width: (widget_size * 2) + widget_gap
height: weather_h (108-150px)
background: 0x081848
elements: Location, temperature, forecast
toggleable: Celsius ↔ Fahrenheit
```

### App Icons

```rust
// Icon structure (compositor.rs:863-894)
size: Responsive (42-66px)
background: App-specific color
shadow: +2px y-offset, dark color
gloss: Top half, white overlay, low alpha
symbol: Drawn using icon-specific shapes
label: Below icon, +10px, centered

// Icon symbol guidelines
- Use simple geometric shapes (circles, rectangles, rounded rects)
- Layer elements with alpha for depth
- Match iOS/iPadOS design language
- Keep symbols recognizable at small sizes
```

---

## 4. Rendering & Graphics API

### Scene Submission Pattern

All apps follow this pattern for rendering:

```rust
// 1. Submit scene parameters
let submit = gfx_submit_scene(top_color, bottom_color, dock_color, dock_height);

// 2. Present to screen
if submit.code == 0 {
    let present = gfx_present();
}
```

### Syscalls Used

```rust
// Graphics
gfx_submit_scene(top: u32, bottom: u32, dock: u32, height: u32) -> SyscallResult
gfx_present() -> SyscallResult

// Input
input_subscribe(enabled: bool) -> SyscallResult
input_read() -> SyscallResult

// IPC (for Control Center, Notifications)
ipc_send(header: &UiMessageHeader, payload: &[u8]) -> SyscallResult
```

### Framebuffer Primitives

Available drawing functions in kernel/src/ui/framebuffer.rs:

```rust
fill_vertical_gradient(top_color: u32, bottom_color: u32)
fill_rect_alpha(x: u32, y: u32, w: u32, h: u32, color: u32, alpha: u8)
fill_rounded_rect(x: u32, y: u32, w: u32, h: u32, radius: u32, color: u32)
fill_rounded_rect_alpha(x: u32, y: u32, w: u32, h: u32, radius: u32, color: u32, alpha: u8)
fill_circle_alpha(cx: u32, cy: u32, radius: u32, color: u32, alpha: u8)
draw_text(x: u32, y: u32, text: &[u8], color: u32)
```

### Alpha Blending Guidelines

```rust
// Alpha values guide
0-50:     Very subtle effects, overlays
50-100:   Moderate transparency, gloss effects
100-150:  Semi-transparent, dock background
150-200:  Mostly opaque, shadows
200-255:  Nearly/fully opaque, solid UI elements
```

---

## 5. Gesture System

### Gesture Constants

```rust
// Input gesture actions (shared/abi/src/input.rs referenced in apps)
GESTURE_HOME:                   0
GESTURE_APP_SWITCHER_LEFT:      1
GESTURE_APP_SWITCHER_RIGHT:     2
GESTURE_CONTROL_CENTER:         3
GESTURE_NOTIFICATION_CENTER:    4
```

### Gesture Handling Pattern

```rust
// Standard gesture loop
loop {
    let input = input_read();
    if input.code == 0 {
        match input.value {
            GESTURE_HOME => { /* Handle home */ }
            GESTURE_APP_SWITCHER_LEFT => { /* Handle left swipe */ }
            // ...
        }
    } else if input.code == -11 {
        // Idle, no input (EAGAIN equivalent)
    }
}
```

---

## 6. Animation & Transitions

### Transition System

```rust
// Transition parameters (compositor.rs:94-95)
TRANSITION_STEPS: 7
TRANSITION_SPIN: 50_000  // Delay between steps

// Motion state
struct MotionState {
    content_dx: i32,    // Horizontal content offset
    content_dy: i32,    // Vertical content offset
    dock_lift: i32,     // Dock animation offset
}
```

### Scene Interpolation

Color transitions use linear RGB interpolation:

```rust
fn interpolate_scene(from: SimpleScene, to: SimpleScene, step: u32, steps: u32)
// Interpolates: top_color, bottom_color, dock_color, dock_height
// Over 7 steps for smooth transitions
```

### Gesture Animations

```rust
// Home gesture: Content slides down, dock lifts
content_dy: slide / 2
dock_lift: slide / 3

// App Switcher Left: Content slides left
content_dx: -slide

// App Switcher Right: Content slides right
content_dx: slide

// Control Center: Content slides down, dock lifts more
content_dy: slide / 2
dock_lift: slide / 2
```

### Idle Animations

```rust
// Subtle parallax effect when idle (compositor.rs:1742-1748)
idle_motion(frame: u32) -> MotionState
// Uses triangle_wave() for smooth back-and-forth motion
// content_dx oscillates over 210 frames
// content_dy oscillates over 280 frames
// dock_lift oscillates over 190 frames
```

---

## 7. IPC & UI Channels

### UI Message System

```rust
// IPC channels (shared/abi/src/ipc.rs)
enum UiChannel {
    ShellLifecycle = 1,
    NotificationCenter = 2,
    ControlCenter = 3,
    AppLaunch = 4,
}

enum UiMessageKind {
    LaunchApp = 0x01,
    CloseApp = 0x02,
    PublishNotification = 0x03,
    ToggleControl = 0x04,
}

struct UiMessageHeader {
    channel: UiChannel,
    kind: UiMessageKind,
    payload_len: u16,
}
```

### Sending IPC Messages

```rust
// Example: Toggle Control Center
let header = UiMessageHeader {
    channel: UiChannel::ControlCenter,
    kind: UiMessageKind::ToggleControl,
    payload_len: payload.len() as u16,
};
ipc_send(&header, payload);
```

---

## 8. Typography

### Text Rendering

OpenOS uses a simple bitmap font system:

```rust
// Text drawing
framebuffer::draw_text(x: u32, y: u32, text: &[u8], color: u32)

// Characteristics
- Fixed-width characters
- Each character: ~8px wide
- Line height: ~14px
- ASCII only (byte strings)
```

### Text Positioning

```rust
// Centering text
let text_width = text.len() * 8;
let centered_x = container_x + (container_width / 2) - (text_width / 2);
```

### Text Colors by Context

```rust
// Status bar
0xF2F6FF  // Primary status text

// Widget content
0xFFFFFF  // Primary text (time, temperature)
0xEEF4FF  // Labels (location, app names)
0xC5D6EC  // Secondary info (date)
0xCAD9ED  // Tertiary info (forecast)
0xB9CDE6  // Muted info (match details)
```

---

## 9. File Organization

### App Structure

```
userspace/
├── app-shell-payload/src/main.rs       # Shell/launcher app
├── app-settings-payload/src/main.rs    # Settings app
├── app-files-payload/src/main.rs       # Files app
├── apps/
│   ├── settings/src/main.rs            # Settings executable
│   ├── files/src/main.rs               # Files executable
│   └── terminal-lite/src/main.rs       # Terminal app
└── syscall/src/lib.rs                  # Syscall wrappers
```

### Kernel UI Components

```
kernel/src/ui/
├── compositor.rs    # Scene composition, rendering, gesture handling
└── framebuffer.rs   # Low-level drawing primitives
```

### Shared ABI

```
shared/abi/src/
├── ipc.rs              # IPC message types
├── input.rs            # Gesture definitions
├── syscalls.rs         # Syscall numbers/structures
└── app_manifest.rs     # App metadata
```

---

## 10. App Development Guidelines

### Creating a New App

1. **Choose a color palette** that distinguishes your app
   ```rust
   // Unique gradient for your app
   top_color: 0xXXXXXX,
   bottom_color: 0xXXXXXX,
   dock_color: 0xXXXXXX,
   dock_height: 108-110,
   ```

2. **Follow the render pattern**
   ```rust
   fn render_scene() {
       let submit = gfx_submit_scene(top, bottom, dock, height);
       if submit.code == 0 {
           gfx_present();
       }
   }
   ```

3. **Handle input gracefully**
   ```rust
   - Subscribe to input
   - Check input.code == 0 for valid gestures
   - Check input.code == -11 for idle/no input
   - Exit cleanly on GESTURE_HOME or timeout
   ```

4. **Clean up resources**
   ```rust
   // Unmap memory
   vm_unmap(addr, size);

   // Exit with status
   proc_exit(status_code);
   ```

### App Color Selection

Choose colors that:
- Distinguish your app visually from others
- Use soft, pastel tones for light themes (like Settings, Files)
- Use dark, saturated tones for dark themes (compositor scenes)
- Maintain readability (light text on dark, dark text on light)
- Match the iOS/iPadOS aesthetic (soft gradients, refined colors)

### Performance Considerations

```rust
// Minimize rendering calls
- Only call gfx_present() when scene changes
- Batch drawing operations
- Use alpha blending sparingly (expensive)
- Avoid complex shapes in tight loops

// Idle handling
- Use pause instruction during idle waits
- Set reasonable timeout thresholds
- Clean up before exit
```

---

## 11. Code Style & Conventions

### Naming Conventions

```rust
// Colors: SCREAMING_SNAKE_CASE with descriptive names
const BLUE_PRIMARY: u32 = 0x1B5BE3;
const SETTINGS_BG: u32 = 0xE5FFF4;

// Dimensions: snake_case with units in name
let icon_size = 64;
let dock_height = 108;

// Functions: snake_case, descriptive
fn render_scene() { }
fn draw_app_icon() { }
```

### Color Format

Always use hex RGB (0xRRGGBB):

```rust
✓ 0x1B5BE3    // Good: Hex RGB
✗ 1800675     // Bad: Decimal
✗ #1B5BE3     // Bad: CSS-style (not valid Rust)
```

### Layout Calculations

Use clamping for responsive design:

```rust
// Pattern
clamp_u32(base_calculation, min_value, max_value)

// Example
let icon_size = clamp_u32(width / 19, 56, 66);
```

---

## 12. Testing & Validation

### Build & Test

```bash
# Build entire project
./tools/image/build.sh

# Create bootable USB image
./tools/image/make_usb_image.sh

# Test in QEMU
./tools/image/run_qemu.sh

# Parallel validation
./tools/dev/parallel-lanes.sh
```

### Visual QA Checklist

- [ ] Colors match design system palette
- [ ] Gradients render smoothly (top → bottom)
- [ ] Text is readable with sufficient contrast
- [ ] Icons have proper shadows and gloss effects
- [ ] Dock height and position are correct
- [ ] Transitions are smooth (7 steps)
- [ ] Gestures trigger correct scene changes
- [ ] App exits cleanly on HOME gesture

---

## 13. Common Patterns

### App Boilerplate

```rust
#![no_std]
#![no_main]

use openos_syscall::{fs_write, gfx_present, gfx_submit_scene, proc_exit};

#[no_mangle]
pub extern "sysv64" fn _start(_task_id: u64) -> ! {
    let _ = fs_write(1, b"[app-name] started\r\n");

    render_scene();

    // App-specific logic here

    let _ = proc_exit(exit_code);
    halt_forever();
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    halt_forever()
}

fn render_scene() {
    let submit = gfx_submit_scene(TOP_COLOR, BOTTOM_COLOR, DOCK_COLOR, 108);
    if submit.code == 0 {
        let _ = gfx_present();
    }
}

fn halt_forever() -> ! {
    loop {
        core::hint::spin_loop();
        unsafe {
            core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
        }
    }
}
```

### Widget Drawing Pattern

```rust
fn draw_widget(x: u32, y: u32, size: u32) -> Result<(), FramebufferError> {
    let radius = clamp_u32(size / 7, 18, 24);

    // Shadow
    framebuffer::fill_rounded_rect_alpha(x + 4, y + 8, size, size, radius, 0x060B16, 136)?;

    // Background
    framebuffer::fill_rounded_rect_alpha(x, y, size, size, radius, BG_COLOR, 234)?;

    // Gloss
    framebuffer::fill_rounded_rect_alpha(
        x + 2,
        y + 2,
        size - 4,
        size / 2,
        radius - 2,
        0xFFFFFF,
        22
    )?;

    // Content here...

    Ok(())
}
```

---

## 14. Future Figma Integration

When integrating Figma designs into OpenOS:

1. **Extract Colors**: Convert Figma hex colors to Rust `0xRRGGBB` format
2. **Measure Dimensions**: Note sizes, gaps, radii in Figma design
3. **Apply Responsive Math**: Use `clamp_u32()` for different screen sizes
4. **Match Layering**: Replicate Figma layer order in render calls
5. **Preserve Alpha**: Match Figma opacity to alpha values (0-255)
6. **Test on Hardware**: Verify colors/sizing on actual framebuffer

### Figma → OpenOS Mapping

| Figma Property | OpenOS Equivalent |
|----------------|-------------------|
| Fill Color (#RRGGBB) | `0xRRGGBB` |
| Opacity (0-100%) | `alpha (0-255)` |
| Corner Radius | `radius` parameter |
| Drop Shadow | Offset rect with alpha |
| Linear Gradient | `fill_vertical_gradient()` |
| Text Layer | `draw_text()` |

---

## Resources

- **Kernel Compositor**: `kernel/src/ui/compositor.rs` - Main UI rendering logic
- **Framebuffer API**: `kernel/src/ui/framebuffer.rs` - Drawing primitives
- **App Examples**: `userspace/app-*-payload/src/main.rs` - Reference implementations
- **Syscall Wrappers**: `userspace/syscall/src/lib.rs` - API for apps
- **ABI Definitions**: `shared/abi/src/*.rs` - System interfaces

---

*Last updated: 2026-02-14*
