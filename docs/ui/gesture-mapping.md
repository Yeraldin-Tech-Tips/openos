# Non-touch Gesture Mapping

Default keyboard mapping for iPadOS-like actions on laptops:

- `Alt + Up`: Home
- `Alt + Left`: App switcher previous app
- `Alt + Right`: App switcher next app
- `Alt + Down`: Control Center
- `Alt + Shift + Down`: Notification Center

Keyboard home-screen interactions (implemented):

- `Left/Up`: previous focus target
- `Right/Down`: next focus target
- `Tab` / `Shift+Tab`: next/previous focus target
- `Enter` / `Space`: activate focused widget/icon/control
- `Esc`: Home

Mouse interactions (implemented on PS/2 input path):

- Mouse move: moves UI cursor
- Left click: activates hovered widget/icon/control
- Press-hold + move on clock/match/weather widgets: drag widget placement prototype
- Press-hold + move on dock icons: horizontal dock reorder prototype
- Hover and press states render translucent feedback on interactive targets

Foreground app panel interactions (implemented):

- Clicking Shell/Settings/Files app icons opens an in-UI foreground panel
- Foreground panel controls include close, primary action, and a Files secondary action
- Foreground panel displays live kernel-derived status
- Per-app lifecycle state (`foreground`, `queued`, `exited`, `not launched`)
- PID and exit code when available
- Lifecycle totals (`spawn_total`, `record_count`, `foreground_pid`)
- IPC queue depth

Mouse/trackpad mapping (planned, not yet implemented):

- Right-edge drag: Control Center reveal
- Top-edge drag: Notification Center reveal
- Three-finger horizontal swipe: app switch
