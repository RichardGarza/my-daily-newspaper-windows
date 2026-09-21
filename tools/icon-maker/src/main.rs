//! icon-maker <name-or-initials | - > <out-dir>     ("-" reads ICON_MAKER_NAME)
//!
//! Draws the app icon - two blackletter initials on a newsprint tile - and
//! writes everything the app bundle needs:
//!   32x32.png  128x128.png  128x128@2x.png  icon.png (1024)  icon.icns  icon.ico
//!
//! "Sam" -> SD, "Priya" -> PD, "JD" -> JD, nothing usable -> MD (My Daily).
//! Every size is drawn from the outlines, not scaled down from a big bitmap.

use std::fs;
use std::path::Path;

use ab_glyph::{point, Font, FontRef, Glyph, PxScale, ScaleFont};

const FONT: &[u8] = include_bytes!("../Chomsky.otf");
const PAPER: [f32; 3] = [246.0, 243.0, 236.0];
const INK: [f32; 3] = [20.0, 20.0, 20.0];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        eprintln!("usage: icon-maker <name-or-initials> <out-dir>");
        std::process::exit(2);
    }
    let font = FontRef::try_from_slice(FONT).expect("bundled font");
    // "-" means "the name is in ICON_MAKER_NAME": Windows PowerShell drops
    // empty arguments and mangles some characters on the way to a program.
    let name = if args[0] == "-" { std::env::var("ICON_MAKER_NAME").unwrap_or_default() } else { args[0].clone() };
    let letters = initials(&name, &font);
    let out = Path::new(&args[1]);
    fs::create_dir_all(out).expect("create out dir");

    let render = |size: u32| encode_png(&draw(&font, &letters, size), size);

    for (name, size) in [("32x32.png", 32), ("128x128.png", 128), ("128x128@2x.png", 256), ("icon.png", 1024)] {
        fs::write(out.join(name), render(size)).expect("write png");
    }

    // .icns is a list of PNGs with four-letter tags.
    let mut body: Vec<u8> = Vec::new();
    for (tag, size) in [
        (b"icp4", 16u32),
        (b"icp5", 32),
        (b"ic11", 32),
        (b"ic12", 64),
        (b"ic07", 128),
        (b"ic13", 256),
        (b"ic08", 256),
        (b"ic14", 512),
        (b"ic09", 512),
        (b"ic10", 1024),
    ] {
        let png = render(size);
        body.extend_from_slice(tag);
        body.extend_from_slice(&((png.len() as u32 + 8).to_be_bytes()));
        body.extend_from_slice(&png);
    }
    let mut icns = Vec::with_capacity(body.len() + 8);
    icns.extend_from_slice(b"icns");
    icns.extend_from_slice(&((body.len() as u32 + 8).to_be_bytes()));
    icns.extend_from_slice(&body);
    fs::write(out.join("icon.icns"), icns).expect("write icns");

    // .ico (Windows): a directory of PNGs.
    let sizes = [16u32, 24, 32, 48, 64, 256];
    let pngs: Vec<Vec<u8>> = sizes.iter().map(|s| render(*s)).collect();
    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (size, png) in sizes.iter().zip(&pngs) {
        let edge = if *size >= 256 { 0u8 } else { *size as u8 };
        ico.extend_from_slice(&[edge, edge, 0, 0, 1, 0, 32, 0]);
        ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for png in &pngs {
        ico.extend_from_slice(png);
    }
    fs::write(out.join("icon.ico"), ico).expect("write ico");

    println!("{letters}");
}

/// First letter of the name + D for Daily. Two capitals typed on purpose
/// ("JD") are taken as they are. Letters the font can't draw fall back to MD.
fn initials(input: &str, font: &FontRef) -> String {
    let drawable = |c: char| c.is_alphabetic() && font.glyph_id(c).0 != 0;
    let word = input.trim();
    let letters: Vec<char> = word.chars().filter(|c| !c.is_whitespace()).collect();
    if letters.len() == 2 && letters.iter().all(|c| c.is_uppercase() && drawable(*c)) {
        return letters.iter().collect();
    }
    match word.chars().find(|c| c.is_alphabetic()).and_then(|c| c.to_uppercase().next()) {
        Some(c) if drawable(c) => format!("{c}D"),
        _ => "MD".to_string(),
    }
}

/// RGBA, straight alpha.
fn draw(font: &FontRef, letters: &str, size: u32) -> Vec<u8> {
    let s = size as f32;
    let n = size as usize;

    // The tile: macOS icon grid - 824/1024 square, continuous-ish corners.
    let inset = s * 100.0 / 1024.0;
    let radius = s * 185.0 / 1024.0;
    let half = s / 2.0 - inset;

    // Ink coverage for the letters.
    let mut ink = vec![0f32; n * n];
    let (max_w, max_h) = (s * 0.66, s * 0.40);
    let probe = 1000.0f32;
    let (w0, top0, bottom0, _) = measure(font, letters, probe);
    let h0 = bottom0 - top0;
    let px = probe * (max_w / w0).min(max_h / h0);
    let (w, top, bottom, glyphs) = measure(font, letters, px);
    let ox = (s - w) / 2.0;
    let oy = (s - (bottom - top)) / 2.0 - top;
    for mut g in glyphs {
        g.position = point(g.position.x + ox, g.position.y + oy);
        if let Some(outline) = font.outline_glyph(g) {
            let b = outline.px_bounds();
            let (gw, gh) = (b.width().ceil() as usize + 1, b.height().ceil() as usize + 1);
            let mut cov = vec![0f32; gw * gh];
            outline.draw(|x, y, c| {
                let (x, y) = (x as usize, y as usize);
                if x < gw && y < gh {
                    cov[y * gw + x] = c;
                }
            });
            // The rasterizer can leave a ghost hairline down the first column
            // of a glyph's box. Real edges have ink beside them; ghosts don't.
            if gw > 4 {
                for y in 0..gh {
                    for x in 0..3 {
                        let left = if x == 0 { 0.0 } else { cov[y * gw + x - 1] };
                        if left < 0.02 && cov[y * gw + x + 1] < 0.02 {
                            cov[y * gw + x] = 0.0;
                        }
                    }
                }
            }
            for y in 0..gh {
                for x in 0..gw {
                    let c = cov[y * gw + x];
                    let (px, py) = (b.min.x as i32 + x as i32, b.min.y as i32 + y as i32);
                    if c > 0.0 && px >= 0 && py >= 0 && (px as usize) < n && (py as usize) < n {
                        let i = py as usize * n + px as usize;
                        ink[i] = (ink[i] + c).min(1.0);
                    }
                }
            }
        }
    }

    let mut rgba = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            // distance to the rounded square, for a soft one-pixel edge
            let dx = ((x as f32 + 0.5 - s / 2.0).abs() - (half - radius)).max(0.0);
            let dy = ((y as f32 + 0.5 - s / 2.0).abs() - (half - radius)).max(0.0);
            let dist = (dx * dx + dy * dy).sqrt() - radius;
            let tile = (0.5 - dist).clamp(0.0, 1.0);
            let k = ink[y * n + x];
            let i = (y * n + x) * 4;
            for c in 0..3 {
                rgba[i + c] = (PAPER[c] * (1.0 - k) + INK[c] * k).round() as u8;
            }
            rgba[i + 3] = (tile * 255.0).round() as u8;
        }
    }
    rgba
}

/// Lay the letters on a baseline at y = 0: (width, top, bottom, glyphs).
fn measure(font: &FontRef, letters: &str, px: f32) -> (f32, f32, f32, Vec<Glyph>) {
    let scaled = font.as_scaled(PxScale::from(px));
    let mut glyphs = Vec::new();
    let mut pen = 0.0f32;
    let mut prev = None;
    for c in letters.chars() {
        let id = scaled.glyph_id(c);
        if let Some(p) = prev {
            pen += scaled.kern(p, id);
        }
        glyphs.push(id.with_scale_and_position(px, point(pen, 0.0)));
        pen += scaled.h_advance(id) + px * 0.035;
        prev = Some(id);
    }
    let (mut left, mut right, mut top, mut bottom) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for g in &glyphs {
        if let Some(o) = font.outline_glyph(g.clone()) {
            let b = o.px_bounds();
            left = left.min(b.min.x);
            right = right.max(b.max.x);
            top = top.min(b.min.y);
            bottom = bottom.max(b.max.y);
        }
    }
    for g in &mut glyphs {
        g.position.x -= left;
    }
    (right - left, top, bottom, glyphs)
}

fn encode_png(rgba: &[u8], size: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, size, size);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("png header");
        w.write_image_data(rgba).expect("png data");
    }
    out
}
