use crate::boot::FramebufferInfo;
use core::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramebufferError {
    Unavailable,
    UnsupportedFormat,
}

static FRAMEBUFFER_ACTIVE: AtomicBool = AtomicBool::new(false);
static mut ACTIVE_FRAMEBUFFER: FramebufferInfo = FramebufferInfo {
    base: core::ptr::null_mut(),
    size: 0,
    width: 0,
    height: 0,
    stride: 0,
    bytes_per_pixel: 0,
};

pub fn install(fb: FramebufferInfo) {
    unsafe {
        ACTIVE_FRAMEBUFFER = fb;
    }
    FRAMEBUFFER_ACTIVE.store(true, Ordering::Release);
}

pub fn has_active_framebuffer() -> bool {
    FRAMEBUFFER_ACTIVE.load(Ordering::Acquire)
}

pub fn fill_solid(color: u32) -> Result<(), FramebufferError> {
    with_framebuffer_mut(|fb| {
        let pixels = fb.size / fb.bytes_per_pixel as usize;
        let ptr = fb.base as *mut u32;
        for i in 0..pixels {
            unsafe { ptr.add(i).write_volatile(color) };
        }
    })
}

pub fn fill_vertical_gradient(top_color: u32, bottom_color: u32) -> Result<(), FramebufferError> {
    with_framebuffer_mut(|fb| {
        let width = fb.width as usize;
        let height = fb.height as usize;
        let stride = fb.stride as usize;
        let ptr = fb.base as *mut u32;

        if height == 0 || width == 0 {
            return;
        }

        let tr = ((top_color >> 16) & 0xFF) as u64;
        let tg = ((top_color >> 8) & 0xFF) as u64;
        let tb = (top_color & 0xFF) as u64;

        let br = ((bottom_color >> 16) & 0xFF) as u64;
        let bg = ((bottom_color >> 8) & 0xFF) as u64;
        let bb = (bottom_color & 0xFF) as u64;

        let denom = if height > 1 { (height - 1) as u64 } else { 1 };

        for y in 0..height {
            let y_u64 = y as u64;
            let inv = denom - y_u64;
            let r = ((tr * inv) + (br * y_u64)) / denom;
            let g = ((tg * inv) + (bg * y_u64)) / denom;
            let b = ((tb * inv) + (bb * y_u64)) / denom;
            let color = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);

            let row_base = y * stride;
            for x in 0..width {
                unsafe { ptr.add(row_base + x).write_volatile(color) };
            }
        }
    })
}

pub fn fill_bottom_strip(color: u32, height: u32) -> Result<(), FramebufferError> {
    with_framebuffer_mut(|fb| {
        let width = fb.width as usize;
        let fb_height = fb.height as usize;
        let stride = fb.stride as usize;
        let ptr = fb.base as *mut u32;
        let strip_height = core::cmp::min(height as usize, fb_height);
        let start_y = fb_height.saturating_sub(strip_height);

        for y in start_y..fb_height {
            let row_base = y * stride;
            for x in 0..width {
                unsafe { ptr.add(row_base + x).write_volatile(color) };
            }
        }
    })
}

pub fn dimensions() -> Result<(u32, u32), FramebufferError> {
    let fb = validated_framebuffer()?;
    Ok((fb.width, fb.height))
}

pub fn fill_rect(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    color: u32,
) -> Result<(), FramebufferError> {
    if width == 0 || height == 0 {
        return Ok(());
    }

    with_framebuffer_mut(|fb| {
        let fb_width = fb.width as usize;
        let fb_height = fb.height as usize;
        let stride = fb.stride as usize;
        let ptr = fb.base as *mut u32;

        let start_x = (x as usize).min(fb_width);
        let start_y = (y as usize).min(fb_height);
        let end_x = (x.saturating_add(width) as usize).min(fb_width);
        let end_y = (y.saturating_add(height) as usize).min(fb_height);

        if start_x >= end_x || start_y >= end_y {
            return;
        }

        let mut row = start_y;
        while row < end_y {
            let row_base = row * stride;
            let mut col = start_x;
            while col < end_x {
                unsafe { ptr.add(row_base + col).write_volatile(color) };
                col += 1;
            }
            row += 1;
        }
    })
}

pub fn fill_rect_alpha(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    color: u32,
    alpha: u8,
) -> Result<(), FramebufferError> {
    if width == 0 || height == 0 || alpha == 0 {
        return Ok(());
    }
    if alpha == 0xFF {
        return fill_rect(x, y, width, height, color);
    }

    with_framebuffer_mut(|fb| {
        let fb_width = fb.width as usize;
        let fb_height = fb.height as usize;
        let stride = fb.stride as usize;
        let ptr = fb.base as *mut u32;

        let start_x = (x as usize).min(fb_width);
        let start_y = (y as usize).min(fb_height);
        let end_x = (x.saturating_add(width) as usize).min(fb_width);
        let end_y = (y.saturating_add(height) as usize).min(fb_height);

        if start_x >= end_x || start_y >= end_y {
            return;
        }

        let mut row = start_y;
        while row < end_y {
            let row_base = row * stride;
            let mut col = start_x;
            while col < end_x {
                let pixel_ptr = unsafe { ptr.add(row_base + col) };
                let dst = unsafe { pixel_ptr.read_volatile() };
                unsafe { pixel_ptr.write_volatile(blend_rgb(dst, color, alpha)) };
                col += 1;
            }
            row += 1;
        }
    })
}

pub fn fill_vertical_gradient_rect(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    top_color: u32,
    bottom_color: u32,
) -> Result<(), FramebufferError> {
    if width == 0 || height == 0 {
        return Ok(());
    }

    with_framebuffer_mut(|fb| {
        let fb_width = fb.width as usize;
        let fb_height = fb.height as usize;
        let stride = fb.stride as usize;
        let ptr = fb.base as *mut u32;

        let start_x = (x as usize).min(fb_width);
        let start_y = (y as usize).min(fb_height);
        let end_x = (x.saturating_add(width) as usize).min(fb_width);
        let end_y = (y.saturating_add(height) as usize).min(fb_height);

        if start_x >= end_x || start_y >= end_y {
            return;
        }

        let rel_h = (end_y - start_y) as u32;
        let denom = if rel_h > 1 { rel_h - 1 } else { 1 };
        let mut row = start_y;
        while row < end_y {
            let t = (row - start_y) as u32;
            let color = interpolate_rgb(top_color, bottom_color, t, denom);
            let row_base = row * stride;
            let mut col = start_x;
            while col < end_x {
                unsafe { ptr.add(row_base + col).write_volatile(color) };
                col += 1;
            }
            row += 1;
        }
    })
}

pub fn fill_rounded_rect(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    radius: u32,
    color: u32,
) -> Result<(), FramebufferError> {
    if width == 0 || height == 0 {
        return Ok(());
    }

    let max_radius = core::cmp::min(width / 2, height / 2);
    let radius = core::cmp::min(radius, max_radius);
    if radius == 0 {
        return fill_rect(x, y, width, height, color);
    }

    with_framebuffer_mut(|fb| {
        let fb_width = fb.width as usize;
        let fb_height = fb.height as usize;
        let stride = fb.stride as usize;
        let ptr = fb.base as *mut u32;

        let start_x = (x as usize).min(fb_width);
        let start_y = (y as usize).min(fb_height);
        let end_x = (x.saturating_add(width) as usize).min(fb_width);
        let end_y = (y.saturating_add(height) as usize).min(fb_height);

        if start_x >= end_x || start_y >= end_y {
            return;
        }

        let rel_w = end_x.saturating_sub(start_x) as u32;
        let rel_h = end_y.saturating_sub(start_y) as u32;
        let mut py = 0u32;
        while py < rel_h {
            let row_base = (start_y + py as usize) * stride;
            let mut px = 0u32;
            while px < rel_w {
                if rounded_rect_contains(px, py, rel_w, rel_h, radius) {
                    unsafe {
                        ptr.add(row_base + start_x + px as usize)
                            .write_volatile(color)
                    };
                }
                px += 1;
            }
            py += 1;
        }
    })
}

pub fn fill_rounded_rect_alpha(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    radius: u32,
    color: u32,
    alpha: u8,
) -> Result<(), FramebufferError> {
    if width == 0 || height == 0 || alpha == 0 {
        return Ok(());
    }
    if alpha == 0xFF {
        return fill_rounded_rect(x, y, width, height, radius, color);
    }

    let max_radius = core::cmp::min(width / 2, height / 2);
    let radius = core::cmp::min(radius, max_radius);
    if radius == 0 {
        return fill_rect_alpha(x, y, width, height, color, alpha);
    }

    with_framebuffer_mut(|fb| {
        let fb_width = fb.width as usize;
        let fb_height = fb.height as usize;
        let stride = fb.stride as usize;
        let ptr = fb.base as *mut u32;

        let start_x = (x as usize).min(fb_width);
        let start_y = (y as usize).min(fb_height);
        let end_x = (x.saturating_add(width) as usize).min(fb_width);
        let end_y = (y.saturating_add(height) as usize).min(fb_height);

        if start_x >= end_x || start_y >= end_y {
            return;
        }

        let rel_w = end_x.saturating_sub(start_x) as u32;
        let rel_h = end_y.saturating_sub(start_y) as u32;
        let mut py = 0u32;
        while py < rel_h {
            let row_base = (start_y + py as usize) * stride;
            let mut px = 0u32;
            while px < rel_w {
                if rounded_rect_contains(px, py, rel_w, rel_h, radius) {
                    let pixel_ptr = unsafe { ptr.add(row_base + start_x + px as usize) };
                    let dst = unsafe { pixel_ptr.read_volatile() };
                    unsafe { pixel_ptr.write_volatile(blend_rgb(dst, color, alpha)) };
                }
                px += 1;
            }
            py += 1;
        }
    })
}

pub fn fill_circle(cx: u32, cy: u32, radius: u32, color: u32) -> Result<(), FramebufferError> {
    if radius == 0 {
        return Ok(());
    }

    with_framebuffer_mut(|fb| {
        let fb_width = fb.width as usize;
        let fb_height = fb.height as usize;
        let stride = fb.stride as usize;
        let ptr = fb.base as *mut u32;

        let start_x = cx.saturating_sub(radius) as usize;
        let start_y = cy.saturating_sub(radius) as usize;
        let end_x = (cx.saturating_add(radius).saturating_add(1) as usize).min(fb_width);
        let end_y = (cy.saturating_add(radius).saturating_add(1) as usize).min(fb_height);

        let rr = (radius as i64) * (radius as i64);
        let mut y = start_y;
        while y < end_y {
            let dy = y as i64 - cy as i64;
            let row_base = y * stride;
            let mut x = start_x;
            while x < end_x {
                let dx = x as i64 - cx as i64;
                if dx * dx + dy * dy <= rr {
                    unsafe { ptr.add(row_base + x).write_volatile(color) };
                }
                x += 1;
            }
            y += 1;
        }
    })
}

pub fn fill_circle_alpha(
    cx: u32,
    cy: u32,
    radius: u32,
    color: u32,
    alpha: u8,
) -> Result<(), FramebufferError> {
    if radius == 0 || alpha == 0 {
        return Ok(());
    }
    if alpha == 0xFF {
        return fill_circle(cx, cy, radius, color);
    }

    with_framebuffer_mut(|fb| {
        let fb_width = fb.width as usize;
        let fb_height = fb.height as usize;
        let stride = fb.stride as usize;
        let ptr = fb.base as *mut u32;

        let start_x = cx.saturating_sub(radius) as usize;
        let start_y = cy.saturating_sub(radius) as usize;
        let end_x = (cx.saturating_add(radius).saturating_add(1) as usize).min(fb_width);
        let end_y = (cy.saturating_add(radius).saturating_add(1) as usize).min(fb_height);

        let rr = (radius as i64) * (radius as i64);
        let mut py = start_y;
        while py < end_y {
            let dy = py as i64 - cy as i64;
            let row_base = py * stride;
            let mut px = start_x;
            while px < end_x {
                let dx = px as i64 - cx as i64;
                if dx * dx + dy * dy <= rr {
                    let pixel_ptr = unsafe { ptr.add(row_base + px) };
                    let dst = unsafe { pixel_ptr.read_volatile() };
                    unsafe { pixel_ptr.write_volatile(blend_rgb(dst, color, alpha)) };
                }
                px += 1;
            }
            py += 1;
        }
    })
}

pub fn draw_text(x: u32, y: u32, text: &[u8], color: u32) -> Result<(), FramebufferError> {
    with_framebuffer_mut(|fb| {
        let mut cursor_x = x;
        let mut i = 0usize;
        while i < text.len() {
            draw_glyph_to_fb(fb, cursor_x, y, text[i], color);
            cursor_x = cursor_x.saturating_add(8);
            i += 1;
        }
    })
}

fn rounded_rect_contains(px: u32, py: u32, width: u32, height: u32, radius: u32) -> bool {
    if radius == 0 {
        return true;
    }

    let right = width.saturating_sub(1);
    let bottom = height.saturating_sub(1);
    let r = radius.saturating_sub(1) as i64;
    let rr = r * r;

    if px < radius && py < radius {
        let dx = r - px as i64;
        let dy = r - py as i64;
        return dx * dx + dy * dy <= rr;
    }

    if px >= width.saturating_sub(radius) && py < radius {
        let dx = px as i64 - (right as i64 - r);
        let dy = r - py as i64;
        return dx * dx + dy * dy <= rr;
    }

    if px < radius && py >= height.saturating_sub(radius) {
        let dx = r - px as i64;
        let dy = py as i64 - (bottom as i64 - r);
        return dx * dx + dy * dy <= rr;
    }

    if px >= width.saturating_sub(radius) && py >= height.saturating_sub(radius) {
        let dx = px as i64 - (right as i64 - r);
        let dy = py as i64 - (bottom as i64 - r);
        return dx * dx + dy * dy <= rr;
    }

    true
}

fn with_framebuffer_mut<F>(op: F) -> Result<(), FramebufferError>
where
    F: FnOnce(FramebufferInfo),
{
    let fb = validated_framebuffer()?;

    op(fb);
    Ok(())
}

fn validated_framebuffer() -> Result<FramebufferInfo, FramebufferError> {
    if !FRAMEBUFFER_ACTIVE.load(Ordering::Acquire) {
        return Err(FramebufferError::Unavailable);
    }

    let fb = unsafe { ACTIVE_FRAMEBUFFER };
    if fb.base.is_null() || fb.bytes_per_pixel != 4 {
        return Err(FramebufferError::UnsupportedFormat);
    }
    Ok(fb)
}

pub struct FrameBufferConsole {
    fb: FramebufferInfo,
    cursor_x: u32,
    cursor_y: u32,
    fg: u32,
}

impl FrameBufferConsole {
    pub unsafe fn new(fb: FramebufferInfo) -> Self {
        Self {
            fb,
            cursor_x: 8,
            cursor_y: 8,
            fg: 0xE2E8F0,
        }
    }

    pub fn clear(&mut self, color: u32) {
        let pixels = self.fb.size / self.fb.bytes_per_pixel as usize;
        let ptr = self.fb.base as *mut u32;
        for i in 0..pixels {
            unsafe { ptr.add(i).write_volatile(color) };
        }
    }

    pub fn write_line(&mut self, text: &str) {
        for b in text.bytes() {
            self.draw_glyph(b);
            self.cursor_x += 8;
        }
        self.cursor_x = 8;
        self.cursor_y += 12;
    }

    fn draw_glyph(&self, byte: u8) {
        draw_glyph_to_fb(self.fb, self.cursor_x, self.cursor_y, byte, self.fg);
    }

    fn put_pixel(&self, x: u32, y: u32, color: u32) {
        put_pixel_fb(self.fb, x, y, color);
    }
}

fn draw_glyph_to_fb(fb: FramebufferInfo, x: u32, y: u32, byte: u8, color: u32) {
    let glyph = simple_glyph(byte);
    let mut row = 0usize;
    while row < glyph.len() {
        let bits = glyph[row];
        let mut col = 0u32;
        while col < 5 {
            if (bits >> (4 - col)) & 1 == 1 {
                put_pixel_fb(
                    fb,
                    x.saturating_add(1 + col),
                    y.saturating_add(1 + row as u32),
                    color,
                );
            }
            col += 1;
        }
        row += 1;
    }
}

fn put_pixel_fb(fb: FramebufferInfo, x: u32, y: u32, color: u32) {
    if x >= fb.width || y >= fb.height {
        return;
    }
    let idx = (y * fb.stride + x) as usize;
    let ptr = fb.base as *mut u32;
    unsafe { ptr.add(idx).write_volatile(color) };
}

fn simple_glyph(byte: u8) -> [u8; 7] {
    let ascii = if byte >= b'a' && byte <= b'z' {
        byte - (b'a' - b'A')
    } else {
        byte
    };

    match ascii {
        b'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        b'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        b'C' => [0x0F, 0x10, 0x10, 0x10, 0x10, 0x10, 0x0F],
        b'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        b'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        b'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        b'G' => [0x0F, 0x10, 0x10, 0x17, 0x11, 0x11, 0x0F],
        b'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        b'I' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1F],
        b'J' => [0x01, 0x01, 0x01, 0x01, 0x11, 0x11, 0x0E],
        b'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        b'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        b'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        b'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        b'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        b'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        b'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        b'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        b'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        b'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        b'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        b'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        b'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0A],
        b'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        b'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        b'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        b'0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        b'1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        b'2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        b'3' => [0x1E, 0x01, 0x01, 0x0E, 0x01, 0x01, 0x1E],
        b'4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        b'5' => [0x1F, 0x10, 0x10, 0x1E, 0x01, 0x01, 0x1E],
        b'6' => [0x0E, 0x10, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        b'7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        b'8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        b'9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x01, 0x0E],
        b'!' => [0x04, 0x04, 0x04, 0x04, 0x04, 0x00, 0x04],
        b'?' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
        b'.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x06],
        b',' => [0x00, 0x00, 0x00, 0x00, 0x06, 0x06, 0x0C],
        b':' => [0x00, 0x06, 0x06, 0x00, 0x06, 0x06, 0x00],
        b';' => [0x00, 0x06, 0x06, 0x00, 0x06, 0x06, 0x0C],
        b'-' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
        b'_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1F],
        b'+' => [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00],
        b'/' => [0x01, 0x02, 0x02, 0x04, 0x08, 0x08, 0x10],
        b'\\' => [0x10, 0x08, 0x08, 0x04, 0x02, 0x02, 0x01],
        b'(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        b')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        b'[' => [0x0E, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0E],
        b']' => [0x0E, 0x02, 0x02, 0x02, 0x02, 0x02, 0x0E],
        b'\'' => [0x04, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00],
        b'"' => [0x0A, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00],
        b'#' => [0x0A, 0x0A, 0x1F, 0x0A, 0x1F, 0x0A, 0x0A],
        b'%' => [0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03],
        b'&' => [0x0C, 0x12, 0x14, 0x08, 0x15, 0x12, 0x0D],
        b'*' => [0x00, 0x15, 0x0E, 0x1F, 0x0E, 0x15, 0x00],
        b'=' => [0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00, 0x00],
        b'<' => [0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02],
        b'>' => [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08],
        b'|' => [0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        b' ' => [0x00; 7],
        _ => [0x1F, 0x11, 0x15, 0x11, 0x15, 0x11, 0x1F],
    }
}

fn interpolate_rgb(top: u32, bottom: u32, numer: u32, denom: u32) -> u32 {
    let denom = if denom == 0 { 1 } else { denom };
    let inv = denom.saturating_sub(numer.min(denom));
    let numer = numer.min(denom);

    let tr = (top >> 16) & 0xFF;
    let tg = (top >> 8) & 0xFF;
    let tb = top & 0xFF;

    let br = (bottom >> 16) & 0xFF;
    let bg = (bottom >> 8) & 0xFF;
    let bb = bottom & 0xFF;

    let r = (tr * inv + br * numer + denom / 2) / denom;
    let g = (tg * inv + bg * numer + denom / 2) / denom;
    let b = (tb * inv + bb * numer + denom / 2) / denom;
    (r << 16) | (g << 8) | b
}

fn blend_rgb(dst: u32, src: u32, alpha: u8) -> u32 {
    let a = alpha as u32;
    let inv = 255u32.saturating_sub(a);

    let dr = (dst >> 16) & 0xFF;
    let dg = (dst >> 8) & 0xFF;
    let db = dst & 0xFF;

    let sr = (src >> 16) & 0xFF;
    let sg = (src >> 8) & 0xFF;
    let sb = src & 0xFF;

    let r = (sr * a + dr * inv + 127) / 255;
    let g = (sg * a + dg * inv + 127) / 255;
    let b = (sb * a + db * inv + 127) / 255;
    (r << 16) | (g << 8) | b
}
