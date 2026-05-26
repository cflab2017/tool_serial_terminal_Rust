//! Procedural CNTerminal icon: dark rounded square with an amber `>_` prompt.
//!
//! Used in two places:
//!   * Window/taskbar icon via `eframe::ViewportBuilder::with_icon` (uses RGBA).
//!   * .exe file icon via `embed-resource` in build.rs (uses a multi-size ICO).
//!
//! Drawing is fully procedural so no external image assets are required —
//! both `main.rs` and `build.rs` consume this same file via `#[path]`.

#![allow(dead_code)] // some helpers are only used by build.rs or main.rs

// ---------- Palette (mirrors the app's amber CRT theme) ----------
const BG: [u8; 4] = [0x0b, 0x0a, 0x08, 0xff];
const AMBER: [u8; 4] = [0xff, 0xb0, 0x00, 0xff];

// ---------- Public API ----------

/// Render the icon to a `size x size` RGBA8 buffer (row-major, top-down).
pub fn render_rgba(size: u32) -> Vec<u8> {
    let s = size as i32;
    let mut buf = vec![0u8; (size * size * 4) as usize];

    // Layout
    let pad = (s / 32).max(1);
    let r = (s * 6) / 32;
    let inner_x = pad;
    let inner_y = pad;
    let inner_w = s - pad * 2;
    let inner_h = s - pad * 2;

    // Filled rounded-rect background
    for y in 0..s {
        for x in 0..s {
            if inside_rounded(x, y, inner_x, inner_y, inner_w, inner_h, r) {
                put(&mut buf, size, x, y, BG);
            }
        }
    }

    // Amber border
    let bw = (s / 16).max(1);
    for y in 0..s {
        for x in 0..s {
            let outer = inside_rounded(x, y, inner_x, inner_y, inner_w, inner_h, r);
            let hole = inside_rounded(
                x,
                y,
                inner_x + bw,
                inner_y + bw,
                inner_w - bw * 2,
                inner_h - bw * 2,
                (r - bw).max(0),
            );
            if outer && !hole {
                put(&mut buf, size, x, y, AMBER);
            }
        }
    }

    // ">" chevron — drawn as two thick lines forming a V on its side
    let cx = s / 2 - s / 8;
    let cy = s / 2;
    let chev_w = s / 3;
    let chev_h = (s * 2) / 5;
    let thickness = (s / 10).max(2);
    let p1 = (cx - chev_w / 2, cy - chev_h / 2);
    let p2 = (cx + chev_w / 2, cy);
    let p3 = (cx - chev_w / 2, cy + chev_h / 2);
    thick_line(&mut buf, size, p1.0, p1.1, p2.0, p2.1, thickness, AMBER);
    thick_line(&mut buf, size, p2.0, p2.1, p3.0, p3.1, thickness, AMBER);

    // "_" cursor underscore to the right of the chevron
    let ux = s / 2 + s / 12;
    let uw = s / 4;
    let uh = (s / 12).max(2);
    let uy = cy + chev_h / 2 - uh;
    for y in uy..uy + uh {
        for x in ux..ux + uw {
            put(&mut buf, size, x, y, AMBER);
        }
    }

    buf
}

/// Encode a multi-size Windows ICO file containing all requested sizes.
/// Each image is a 32-bit BMP (BI_RGB) with an empty AND mask — the alpha
/// channel does the transparency work on every Windows version we care about.
pub fn build_ico(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<(u32, Vec<u8>)> = sizes
        .iter()
        .map(|&s| (s, encode_bmp_for_ico(s, s, &render_rgba(s))))
        .collect();

    let count = images.len() as u16;
    let header_len: u32 = 6 + 16 * count as u32;
    let total: usize = header_len as usize
        + images.iter().map(|(_, b)| b.len()).sum::<usize>();

    let mut out: Vec<u8> = Vec::with_capacity(total);
    // ICONDIR
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type = 1 (ICO)
    out.extend_from_slice(&count.to_le_bytes());

    // ICONDIRENTRY x count
    let mut offset = header_len;
    for (s, img) in &images {
        let w_b: u8 = if *s >= 256 { 0 } else { *s as u8 };
        let h_b: u8 = if *s >= 256 { 0 } else { *s as u8 };
        out.push(w_b);
        out.push(h_b);
        out.push(0); // color count
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bit count
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for (_, img) in &images {
        out.extend_from_slice(img);
    }
    out
}

// ---------- Internals ----------

fn put(buf: &mut [u8], w: u32, x: i32, y: i32, c: [u8; 4]) {
    if x < 0 || y < 0 {
        return;
    }
    let xu = x as u32;
    let yu = y as u32;
    if xu >= w || yu >= w {
        return;
    }
    let idx = ((yu * w + xu) * 4) as usize;
    buf[idx..idx + 4].copy_from_slice(&c);
}

fn inside_rounded(x: i32, y: i32, rx: i32, ry: i32, rw: i32, rh: i32, r: i32) -> bool {
    if x < rx || y < ry || x >= rx + rw || y >= ry + rh {
        return false;
    }
    if r <= 0 {
        return true;
    }
    let dx = if x < rx + r {
        rx + r - x
    } else if x >= rx + rw - r {
        x - (rx + rw - r - 1)
    } else {
        0
    };
    let dy = if y < ry + r {
        ry + r - y
    } else if y >= ry + rh - r {
        y - (ry + rh - r - 1)
    } else {
        0
    };
    dx * dx + dy * dy <= r * r
}

fn thick_line(
    buf: &mut [u8],
    w: u32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    thickness: i32,
    c: [u8; 4],
) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut x = x0;
    let mut y = y0;
    let r = thickness / 2;
    loop {
        // disk brush
        for j in -r..=r {
            for i in -r..=r {
                if i * i + j * j <= r * r {
                    put(buf, w, x + i, y + j, c);
                }
            }
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

fn encode_bmp_for_ico(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let row_bytes = (width * 4) as usize;
    let pixel_bytes = row_bytes * height as usize;
    // 1bpp AND mask, rows padded to 4 bytes
    let mask_row_bytes = ((width + 31) / 32 * 4) as usize;
    let mask_bytes = mask_row_bytes * height as usize;

    let mut out = Vec::with_capacity(40 + pixel_bytes + mask_bytes);

    // BITMAPINFOHEADER (40 bytes). ICO requires biHeight to be 2 * actual.
    out.extend_from_slice(&40u32.to_le_bytes()); // biSize
    out.extend_from_slice(&(width as i32).to_le_bytes()); // biWidth
    out.extend_from_slice(&((height * 2) as i32).to_le_bytes()); // biHeight
    out.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    out.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    out.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    out.extend_from_slice(&(pixel_bytes as u32).to_le_bytes()); // biSizeImage
    out.extend_from_slice(&0i32.to_le_bytes()); // biXPelsPerMeter
    out.extend_from_slice(&0i32.to_le_bytes()); // biYPelsPerMeter
    out.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
    out.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant

    // XOR image: 32-bit BGRA, bottom-up
    for y in (0..height as i32).rev() {
        for x in 0..width as i32 {
            let idx = ((y * width as i32 + x) * 4) as usize;
            let r = rgba[idx];
            let g = rgba[idx + 1];
            let b = rgba[idx + 2];
            let a = rgba[idx + 3];
            out.push(b);
            out.push(g);
            out.push(r);
            out.push(a);
        }
    }

    // AND mask: 1bpp. All zero = "use the XOR image's alpha for transparency",
    // which Windows handles correctly for 32bpp icons.
    out.resize(out.len() + mask_bytes, 0);

    out
}
