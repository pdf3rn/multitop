//! A grid of character cells that can be shown in a terminal or saved as a PNG picture.

use std::io::Write;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Col {
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    Gray,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Style {
    pub fg: Col,
    pub bg: Col,
    pub bold: bool,
}

impl Style {
    pub const fn new(fg: Col, bg: Col, bold: bool) -> Style {
        Style { fg, bg, bold }
    }
}

pub const DEFAULT: Style = Style::new(Col::Default, Col::Default, false);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub st: Style,
}

#[derive(Clone)]
pub struct Buf {
    pub w: usize,
    pub h: usize,
    pub cells: Vec<Cell>,
}

impl Buf {
    pub fn new(w: usize, h: usize) -> Buf {
        Buf { w, h, cells: vec![Cell { ch: ' ', st: DEFAULT }; w * h] }
    }

    pub fn set(&mut self, x: usize, y: usize, ch: char, st: Style) {
        if x < self.w && y < self.h {
            self.cells[y * self.w + x] = Cell { ch, st };
        }
    }

    /// Writes text starting at x, clipped at `limit` (exclusive). Returns the next x.
    pub fn put_clip(&mut self, x: usize, y: usize, s: &str, st: Style, limit: usize) -> usize {
        let mut cx = x;
        for ch in s.chars() {
            if cx >= limit.min(self.w) {
                break;
            }
            self.set(cx, y, ch, st);
            cx += 1;
        }
        cx
    }

    pub fn put(&mut self, x: usize, y: usize, s: &str, st: Style) -> usize {
        let w = self.w;
        self.put_clip(x, y, s, st, w)
    }

    pub fn fill(&mut self, x0: usize, y: usize, x1: usize, st: Style) {
        for x in x0..x1.min(self.w) {
            self.set(x, y, ' ', st);
        }
    }
}

/// Monochrome scheme (-C): colours dropped, coloured backgrounds become reverse video.
pub fn mono(st: Style) -> Style {
    if st.bg != Col::Default && st.bg != Col::Black {
        Style::new(Col::Black, Col::White, false)
    } else {
        Style::new(Col::Default, Col::Default, st.bold)
    }
}

fn term_color(c: Col, bold: bool) -> crossterm::style::Color {
    use crossterm::style::Color as C;
    match c {
        Col::Default => C::Reset,
        Col::Black => C::Black,
        Col::Red => C::DarkRed,
        Col::Green => C::DarkGreen,
        Col::Yellow => C::DarkYellow,
        Col::Blue => C::DarkBlue,
        Col::Magenta => C::DarkMagenta,
        Col::Cyan => C::DarkCyan,
        Col::White => if bold { C::White } else { C::Grey },
        Col::Gray => C::DarkGrey,
    }
}

/// Sends the rows that changed since `prev` to the terminal.
pub fn flush(out: &mut impl Write, buf: &Buf, prev: Option<&Buf>, monochrome: bool) -> std::io::Result<()> {
    use crossterm::{cursor, queue, style};
    let full = prev.map(|p| p.w != buf.w || p.h != buf.h).unwrap_or(true);
    queue!(out, crossterm::terminal::BeginSynchronizedUpdate)?;
    let mut last: Option<Style> = None;
    for y in 0..buf.h {
        let row = &buf.cells[y * buf.w..(y + 1) * buf.w];
        if !full {
            let p = prev.unwrap();
            if &p.cells[y * p.w..(y + 1) * p.w] == row {
                continue;
            }
        }
        queue!(out, cursor::MoveTo(0, y as u16))?;
        let mut text = String::new();
        for cell in row {
            let st = if monochrome { mono(cell.st) } else { cell.st };
            if last != Some(st) {
                if !text.is_empty() {
                    queue!(out, style::Print(&text))?;
                    text.clear();
                }
                queue!(out, style::SetAttribute(style::Attribute::Reset))?;
                if st.bold {
                    queue!(out, style::SetAttribute(style::Attribute::Bold))?;
                }
                queue!(out, style::SetForegroundColor(term_color(st.fg, st.bold)))?;
                queue!(out, style::SetBackgroundColor(term_color(st.bg, false)))?;
                last = Some(st);
            }
            text.push(cell.ch);
        }
        if !text.is_empty() {
            queue!(out, style::Print(&text))?;
        }
    }
    queue!(out, style::SetAttribute(style::Attribute::Reset), crossterm::terminal::EndSynchronizedUpdate)?;
    out.flush()
}

// ---------------------------------------------------------------- picture ----

const CW: usize = 8;
const CH: usize = 16;

fn rgb(c: Col, bold: bool, is_bg: bool) -> [u8; 3] {
    match c {
        Col::Default => if is_bg { [0x10, 0x10, 0x10] } else if bold { [0xff, 0xff, 0xff] } else { [0xd0, 0xd0, 0xd0] },
        Col::Black => [0x00, 0x00, 0x00],
        Col::Red => [0xcd, 0x00, 0x00],
        Col::Green => [0x00, 0xcd, 0x00],
        Col::Yellow => [0xcd, 0xcd, 0x00],
        Col::Blue => if bold { [0x5c, 0x5c, 0xff] } else { [0x00, 0x00, 0xee] },
        Col::Magenta => [0xcd, 0x00, 0xcd],
        Col::Cyan => [0x00, 0xcd, 0xcd],
        Col::White => if bold { [0xff, 0xff, 0xff] } else { [0xe5, 0xe5, 0xe5] },
        Col::Gray => [0x7f, 0x7f, 0x7f],
    }
}

/// 8x16 bitmap of a character: each byte is a row, bit 0 = leftmost pixel.
fn glyph(ch: char) -> [u8; 16] {
    use font8x8::UnicodeFonts;
    let mut out = [0u8; 16];
    // Box drawing and triangles are drawn by hand so lines join between rows.
    match ch {
        '─' => { out[7] = 0xff; out[8] = 0xff; return out; }
        '│' => { for r in out.iter_mut() { *r = 0x18; } return out; }
        '├' => { for r in out.iter_mut() { *r = 0x18; } out[7] = 0xf8; out[8] = 0xf8; return out; }
        '└' => { for r in out.iter_mut().take(9) { *r = 0x18; } out[7] = 0xf8; out[8] = 0xf8; return out; }
        '┌' => { for r in out.iter_mut().skip(7) { *r = 0x18; } out[7] = 0xf8; out[8] = 0xf8; return out; }
        '△' | '▽' => {
            let rows: [u8; 7] = [0x08, 0x14, 0x14, 0x22, 0x22, 0x41, 0x7f];
            for (i, r) in rows.iter().enumerate() {
                let row = if ch == '△' { 4 + i } else { 10 - i };
                out[row] = *r;
            }
            return out;
        }
        _ => {}
    }
    let g = font8x8::BASIC_FONTS
        .get(ch)
        .or_else(|| font8x8::LATIN_FONTS.get(ch))
        .or_else(|| font8x8::BOX_FONTS.get(ch))
        .or_else(|| font8x8::BLOCK_FONTS.get(ch))
        .or_else(|| font8x8::GREEK_FONTS.get(ch))
        .unwrap_or_else(|| font8x8::BASIC_FONTS.get('?').unwrap());
    for (i, row) in g.iter().enumerate() {
        out[i * 2] = *row;
        out[i * 2 + 1] = *row;
    }
    out
}

pub fn save_png(buf: &Buf, path: &std::path::Path, monochrome: bool) -> Result<(), String> {
    let (pw, ph) = (buf.w * CW, buf.h * CH);
    let mut px = vec![0u8; pw * ph * 3];
    for y in 0..buf.h {
        for x in 0..buf.w {
            let cell = buf.cells[y * buf.w + x];
            let st = if monochrome { mono(cell.st) } else { cell.st };
            let fg = rgb(st.fg, st.bold, false);
            let bg = rgb(st.bg, false, true);
            let g = glyph(cell.ch);
            for gy in 0..CH {
                let mut bits = g[gy] as u16;
                if st.bold {
                    bits |= bits << 1;
                }
                for gx in 0..CW {
                    let on = bits & (1 << gx) != 0;
                    let c = if on { fg } else { bg };
                    let i = ((y * CH + gy) * pw + x * CW + gx) * 3;
                    px[i..i + 3].copy_from_slice(&c);
                }
            }
        }
    }
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
    }
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), pw as u32, ph as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(&px).map_err(|e| e.to_string())?;
    Ok(())
}
