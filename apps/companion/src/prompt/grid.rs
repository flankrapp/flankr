//! The attachment grid above the message card's typed box
//! (docs/design/20261113_01_ADD_IMAGES_PLAN.md, decisions 10 and 11): up
//! to four 64 pt tiles in one row, or two rows of two when the inner width
//! is under 4 × 64 + 3 × 8 pt; 8 pt gaps; each tile its number in a pill
//! at its bottom left and an ✕ in a 24 pt target at its top right.
//!
//! A PNG tile shows its thumbnail ([`Thumb`]), a JPEG, WebP or GIF tile
//! its type in capitals on the input colour, until libmorph decodes
//! those. The upload's progress is a 2 pt ring in the accent while it
//! runs, with ✓ when the daemon holds the image and ! when it failed.

use std::sync::Arc;

use libcontrols::{DrawContext, Rect};
use libmsdf::drawlist::DrawList;

use super::PromptKind;
use crate::agent::{AgentHit, fill, line_h};
use crate::theme::theme;

/// A tile's side, the gap between tiles, and the ✕'s target, in points.
pub const TILE: f32 = 64.0;
pub const GAP: f32 = 8.0;
pub const CLOSE: f32 = 24.0;
/// The thumbnail's cells on a side: nearest neighbour into this many,
/// drawn as filled squares in the draw list.
pub const THUMB_CELLS: u32 = 32;
/// The number's and the type label's size, in points.
const LABEL_PT: f32 = 12.0;

/// A thumbnail: [`THUMB_CELLS`] × [`THUMB_CELLS`] RGBA8 cells, row by row,
/// the image fitted inside with its aspect kept; cells outside it are
/// clear.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thumb {
    pub cells: Vec<[u8; 4]>,
}

impl Thumb {
    /// `width` × `height` RGBA8 `pixels` by nearest neighbour.
    pub fn from_rgba(width: u32, height: u32, pixels: &[u8]) -> Option<Thumb> {
        if width == 0 || height == 0 || pixels.len() < (width * height * 4) as usize {
            return None;
        }
        let n = THUMB_CELLS;
        let long = width.max(height) as f32;
        let (fw, fh) = (
            (width as f32 / long * n as f32).round().max(1.0) as u32,
            (height as f32 / long * n as f32).round().max(1.0) as u32,
        );
        let (ox, oy) = ((n - fw) / 2, (n - fh) / 2);
        let mut cells = vec![[0u8; 4]; (n * n) as usize];
        for y in 0..fh {
            let sy = ((y as f32 + 0.5) * height as f32 / fh as f32) as u32;
            for x in 0..fw {
                let sx = ((x as f32 + 0.5) * width as f32 / fw as f32) as u32;
                let at = ((sy.min(height - 1) * width + sx.min(width - 1)) * 4) as usize;
                cells[((oy + y) * n + ox + x) as usize] =
                    [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]];
            }
        }
        Some(Thumb { cells })
    }
}

/// One attached image, as its tile shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    /// Its number, 1-4: the `[image N]` token.
    pub n: u8,
    /// Its type in capitals: PNG, JPG, WEBP or GIF.
    pub label: &'static str,
    pub thumb: Option<Arc<Thumb>>,
    /// The upload's share written, 0 to 1.
    pub progress: f32,
    pub ready: bool,
    pub failed: bool,
}

impl Tile {
    /// The type label of a MIME type.
    pub fn label_of(mime: &str) -> &'static str {
        match mime {
            "image/png" => "PNG",
            "image/jpeg" => "JPG",
            "image/webp" => "WEBP",
            "image/gif" => "GIF",
            _ => "IMG",
        }
    }
}

/// The grid as laid out: each tile with its square.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Grid {
    kind: Option<PromptKind>,
    tiles: Vec<(Tile, Rect)>,
    scale: f32,
}

impl Grid {
    /// Whether four tiles fit in one row of `w` at scale `s`.
    fn one_row(w: f32, s: f32) -> bool {
        w >= (4.0 * TILE + 3.0 * GAP) * s
    }

    /// The tiles per row in `w` at scale `s`.
    fn per_row(w: f32, s: f32) -> usize {
        if Self::one_row(w, s) { 4 } else { 2 }
    }

    /// The grid's height for `count` tiles in `w` at scale `s`: nothing
    /// for none.
    pub fn height(count: usize, w: f32, s: f32) -> f32 {
        if count == 0 {
            return 0.0;
        }
        let rows = count.div_ceil(Self::per_row(w, s)) as f32;
        (rows * TILE + (rows - 1.0) * GAP) * s
    }

    /// `tiles` laid out with the grid's top left at `(x, y)`, `w` wide.
    pub fn lay_out(kind: PromptKind, tiles: &[Tile], x: f32, y: f32, w: f32, s: f32) -> Grid {
        let per = Self::per_row(w, s);
        let (side, gap) = (TILE * s, GAP * s);
        Grid {
            kind: Some(kind),
            tiles: tiles
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let (col, row) = ((i % per) as f32, (i / per) as f32);
                    let r = Rect::new(x + col * (side + gap), y + row * (side + gap), side, side);
                    (t.clone(), r)
                })
                .collect(),
            scale: s,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// The tiles' squares, in order.
    pub fn rects(&self) -> Vec<Rect> {
        self.tiles.iter().map(|(_, r)| *r).collect()
    }

    fn close_of(r: Rect, s: f32) -> Rect {
        let c = CLOSE * s;
        Rect::new(r.x + r.w - c, r.y, c, c)
    }

    fn moved(r: Rect, dy: f32) -> Rect {
        Rect::new(r.x, r.y + dy, r.w, r.h)
    }

    /// The ✕ at `(x, y)`, the grid moved down by `dy`.
    pub fn hit(&self, x: f32, y: f32, dy: f32) -> Option<AgentHit> {
        let kind = self.kind?;
        self.tiles
            .iter()
            .find(|(_, r)| Self::moved(Self::close_of(*r, self.scale), dy).contains([x, y]))
            .map(|(t, _)| AgentHit::PromptDetach(kind, t.n))
    }

    /// Where `hit`'s ✕ is, moved down by `dy`.
    pub fn rect_of(&self, hit: AgentHit, dy: f32) -> Option<Rect> {
        let AgentHit::PromptDetach(kind, n) = hit else {
            return None;
        };
        if Some(kind) != self.kind {
            return None;
        }
        self.tiles
            .iter()
            .find(|(t, _)| t.n == n)
            .map(|(_, r)| Self::moved(Self::close_of(*r, self.scale), dy))
    }

    /// Draws the tiles moved down by `dy`.
    pub fn draw(&self, list: &mut DrawList, ctx: &DrawContext, dy: f32) {
        let s = self.scale;
        for (t, r) in &self.tiles {
            let r = Self::moved(*r, dy);
            fill(list, r, theme().input, 6.0 * s);
            let px = LABEL_PT * s;
            match &t.thumb {
                Some(thumb) => draw_thumb(list, r, thumb),
                None => {
                    let w = ctx.text_advance(t.label, px);
                    ctx.draw_text(
                        list,
                        t.label,
                        [r.x + (r.w - w) * 0.5, r.y + (r.h - line_h(px)) * 0.5],
                        px,
                        theme().muted,
                    );
                }
            }
            // The number in a pill at the bottom left.
            let num = t.n.to_string();
            let nw = ctx.text_advance(&num, px);
            let pill = Rect::new(
                r.x + 3.0 * s,
                r.y + r.h - line_h(px) - 3.0 * s,
                nw + 8.0 * s,
                line_h(px),
            );
            fill(list, pill, theme().bg, pill.h * 0.5);
            ctx.draw_text(list, &num, [pill.x + 4.0 * s, pill.y], px, theme().fg);
            // The ✕ at the top right, on a disc so it reads over pixels.
            let c = Self::close_of(r, s);
            let [cx, cy] = c.center();
            let d = 18.0 * s;
            fill(
                list,
                Rect::new(cx - d * 0.5, cy - d * 0.5, d, d),
                theme().bg,
                d * 0.5,
            );
            let half = 4.5 * s;
            let th = 1.5 * s;
            list.push_line(
                [cx - half, cy - half],
                [cx + half, cy + half],
                th,
                theme().fg,
            );
            list.push_line(
                [cx - half, cy + half],
                [cx + half, cy - half],
                th,
                theme().fg,
            );
            // The upload: a ring while it runs, ✓ or ! when it ends.
            let [mx, my] = r.center();
            if t.failed {
                badge(list, [mx, my], s, theme().red);
                list.push_line([mx, my - 6.0 * s], [mx, my + 2.0 * s], 2.0 * s, theme().bg);
                fill(
                    list,
                    Rect::new(mx - 1.25 * s, my + 4.5 * s, 2.5 * s, 2.5 * s),
                    theme().bg,
                    1.25 * s,
                );
            } else if t.ready {
                badge(list, [mx, my], s, theme().green);
                list.push_line(
                    [mx - 5.0 * s, my],
                    [mx - 1.5 * s, my + 4.0 * s],
                    2.0 * s,
                    theme().bg,
                );
                list.push_line(
                    [mx - 1.5 * s, my + 4.0 * s],
                    [mx + 5.5 * s, my - 4.5 * s],
                    2.0 * s,
                    theme().bg,
                );
            } else {
                ring(
                    list,
                    [mx, my],
                    12.0 * s,
                    2.0 * s,
                    t.progress,
                    theme().accent,
                );
            }
        }
    }
}

/// A filled disc behind ✓ or !.
fn badge(list: &mut DrawList, c: [f32; 2], s: f32, color: [f32; 4]) {
    let d = 22.0 * s;
    fill(
        list,
        Rect::new(c[0] - d * 0.5, c[1] - d * 0.5, d, d),
        color,
        d * 0.5,
    );
}

/// A progress ring of radius `r` and thickness `t` from the top,
/// clockwise, `share` of the way round; a faint full ring under it.
fn ring(list: &mut DrawList, c: [f32; 2], r: f32, t: f32, share: f32, color: [f32; 4]) {
    const SEGMENTS: usize = 32;
    let mut faint = color;
    faint[3] *= 0.3;
    let point = |i: usize| {
        let a = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        [c[0] + r * a.cos(), c[1] + r * a.sin()]
    };
    let done = (share.clamp(0.0, 1.0) * SEGMENTS as f32).round() as usize;
    for i in 0..SEGMENTS {
        let col = if i < done { color } else { faint };
        list.push_line(point(i), point(i + 1), t, col);
    }
}

/// The thumbnail's cells as filled squares, its fitted part only.
fn draw_thumb(list: &mut DrawList, r: Rect, thumb: &Thumb) {
    let n = THUMB_CELLS as usize;
    let cell = r.w / n as f32;
    for (i, px) in thumb.cells.iter().enumerate() {
        if px[3] == 0 {
            continue;
        }
        let (x, y) = ((i % n) as f32, (i / n) as f32);
        let color = [
            f32::from(px[0]) / 255.0,
            f32::from(px[1]) / 255.0,
            f32::from(px[2]) / 255.0,
            f32::from(px[3]) / 255.0,
        ];
        // A hair over a cell, so no seams show between them.
        fill(
            list,
            Rect::new(r.x + x * cell, r.y + y * cell, cell + 0.5, cell + 0.5),
            color,
            0.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(n: u8) -> Tile {
        Tile {
            n,
            label: "JPG",
            thumb: None,
            progress: 0.5,
            ready: false,
            failed: false,
        }
    }

    #[test]
    fn four_tiles_go_in_one_row_at_a_phones_width_and_two_by_two_when_narrow() {
        let s = 3.0;
        let tiles: Vec<Tile> = (1..=4).map(tile).collect();
        // A phone's card: 360 pt inside, over 280.
        let g = Grid::lay_out(PromptKind::Message, &tiles, 10.0, 100.0, 360.0 * s, s);
        let r = g.rects();
        assert!(r.iter().all(|r| r.y == 100.0 && r.w == 64.0 * s));
        assert_eq!(r[1].x - r[0].x, 72.0 * s);
        assert_eq!(Grid::height(4, 360.0 * s, s), 64.0 * s);
        // Under 4 × 64 + 3 × 8 = 280 pt: two rows of two.
        let g = Grid::lay_out(PromptKind::Message, &tiles, 10.0, 100.0, 279.0 * s, s);
        let r = g.rects();
        assert_eq!((r[1].y, r[2].y, r[2].x), (100.0, 100.0 + 72.0 * s, 10.0));
        assert_eq!(Grid::height(4, 279.0 * s, s), 136.0 * s);
        assert_eq!(Grid::height(2, 279.0 * s, s), 64.0 * s);
        assert_eq!(Grid::height(0, 360.0 * s, s), 0.0);
    }

    #[test]
    fn the_close_target_is_at_a_tiles_top_right() {
        let s = 2.0;
        let g = Grid::lay_out(PromptKind::Message, &[tile(2)], 0.0, 0.0, 800.0, s);
        let c = g
            .rect_of(AgentHit::PromptDetach(PromptKind::Message, 2), 0.0)
            .unwrap();
        assert_eq!(c, Rect::new(128.0 - 48.0, 0.0, 48.0, 48.0));
        assert_eq!(
            g.hit(c.x + 5.0, c.y + 5.0, 0.0),
            Some(AgentHit::PromptDetach(PromptKind::Message, 2))
        );
        // The tile's middle takes no tap.
        assert_eq!(g.hit(20.0, 100.0, 0.0), None);
        // Moved with an overlay's slide.
        assert_eq!(
            g.hit(c.x + 5.0, c.y + 5.0 + 100.0, 100.0),
            g.hit(c.x + 5.0, c.y + 5.0, 0.0)
        );
    }

    #[test]
    fn a_wide_image_is_fitted_into_the_thumbnails_middle_rows() {
        // 4 × 2: red on the left half, blue on the right.
        let mut px = Vec::new();
        for _ in 0..2 {
            for x in 0..4 {
                px.extend_from_slice(if x < 2 {
                    &[255, 0, 0, 255]
                } else {
                    &[0, 0, 255, 255]
                });
            }
        }
        let t = Thumb::from_rgba(4, 2, &px).unwrap();
        let n = THUMB_CELLS as usize;
        assert_eq!(t.cells.len(), n * n);
        // Half the height: the top quarter is clear, the middle filled.
        assert_eq!(t.cells[0][3], 0);
        assert_eq!(t.cells[n * (n / 2)], [255, 0, 0, 255]);
        assert_eq!(t.cells[n * (n / 2) + n - 1], [0, 0, 255, 255]);
        assert!(Thumb::from_rgba(0, 2, &px).is_none());
    }
}
