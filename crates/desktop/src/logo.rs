//! The app's mark: the letters RP in a wide, square-shouldered face, drawn
//! from their outline at whatever size is asked for, in one colour,
//! through the glyph atlas ([`crate::paint::Painter::logo`]): the mark
//! costs what a letter costs. The app icon is this mark on a dark tile (the
//! `icon` test in `chat/snapshot.rs` draws it).
//!
//! The letters are measured in letter heights: a stem and bowl of even
//! strokes, square on top, the bowl's lower right corner rounded, and the
//! R's leg sweeping out from beside its stem.

use ttf_parser::OutlineBuilder;

use crate::raster::{self, Bitmap};

/// The side of the square the outline is drawn on.
const BOX: f32 = 24.0;
/// How tall the letters stand on that square.
const HEIGHT: f32 = 7.6;
/// Thickness of every stroke.
const STROKE: f32 = 0.2;
/// Height of a bowl, its strokes included.
const BOWL: f32 = 0.65;
/// Radius of a bowl's rounded lower right corner.
const ROUND: f32 = 0.2;
/// Width of the R's bowl, and of the whole R to the end of its leg.
const R: (f32, f32) = (0.91, 1.067);
/// Where the R's leg leaves the bowl, beside the stem, and how thick it runs.
const LEG: (f32, f32) = (0.33, 0.26);
/// Width of the P.
const P: f32 = 0.99;
/// Space between the letters.
const SPACE: f32 = 0.075;
/// How far right of centre the letters sit, in letter heights: the R
/// carries more ink than the P, whose right side is open under its bowl,
/// so centring the box alone leaves them looking pushed left.
const NUDGE: f32 = 0.06;

/// The mark's coverage on a square `px` pixels a side: a bitmap placed
/// relative to the square's bottom-left corner, y up, as a glyph's is to
/// its pen. `None` if it has no ink at that size.
#[must_use]
pub fn rasterize(px: f32) -> Option<Bitmap> {
    raster::rasterize_outline(px / BOX, draw)
}

/// Draws the mark into `sink`, in the square's units, y up.
fn draw(sink: &mut dyn OutlineBuilder) {
    let width = R.1 + SPACE + P;
    let left = (BOX - width * HEIGHT) * 0.5 + NUDGE * HEIGHT;
    let top = (BOX - HEIGHT) * 0.5;
    // A letter's corner `(x, y)`, in letter heights from its top left, y
    // down, as the sink takes it.
    let at = |origin: f32, (x, y): (f32, f32)| (left + (origin + x) * HEIGHT, BOX - top - y * HEIGHT);
    for (origin, bowl, leg) in [(0.0, R.0, true), (R.1 + SPACE, P, false)] {
        let at = |point| at(origin, point);
        // The stem.
        shape(sink, &[at((0.0, 0.0)), at((STROKE, 0.0)), at((STROKE, 1.0)), at((0.0, 1.0))]);
        // The bowl, its lower right corner turning a quarter circle.
        let (corner, below) = (at((bowl, BOWL - ROUND)), at((bowl - ROUND, BOWL)));
        let k = 0.5523 * ROUND * HEIGHT;
        let start = at((0.0, 0.0));
        sink.move_to(start.0, start.1);
        let right = at((bowl, 0.0));
        sink.line_to(right.0, right.1);
        sink.line_to(corner.0, corner.1);
        sink.curve_to(corner.0, corner.1 - k, below.0 + k, below.1, below.0, below.1);
        let end = at((0.0, BOWL));
        sink.line_to(end.0, end.1);
        sink.close();
        // The slit inside it, wound the other way so it is cut out.
        let (inner, floor) = (bowl - STROKE, BOWL - STROKE);
        shape(sink, &[at((STROKE, STROKE)), at((STROKE, floor)), at((inner, floor)), at((inner, STROKE))]);
        if leg {
            // From the floor of the slit, beside the stem, out to the R's full width.
            let (from, thick) = LEG;
            let floor = BOWL - STROKE;
            shape(sink, &[at((from, floor)), at((from + thick, floor)), at((R.1, 1.0)), at((R.1 - thick, 1.0))]);
        }
    }
}

/// A closed polygon through `points`, in order.
fn shape(sink: &mut dyn OutlineBuilder, points: &[(f32, f32)]) {
    let Some((first, rest)) = points.split_first() else { return };
    sink.move_to(first.0, first.1);
    for point in rest {
        sink.line_to(point.0, point.1);
    }
    sink.close();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mark_draws_inside_its_square_with_even_margins() {
        for px in [16.0_f32, 22.0, 64.0, 256.0] {
            let bitmap = rasterize(px).expect("the mark has ink");
            let ink = bitmap.data.iter().map(|coverage| f32::from(*coverage) / 255.0).sum::<f32>() / (px * px);
            assert!((0.06..0.2).contains(&ink), "inks {ink} of a {px} square");
            let (right, top) = (bitmap.xmin + bitmap.w as i32, bitmap.ymin + bitmap.h as i32);
            let side = px as i32;
            assert!(bitmap.xmin >= 0 && bitmap.ymin >= 0 && right <= side && top <= side, "leaves its square");
        }
        // At full size: as much room above as below, and the ink's weight
        // within a pixel or two of the middle.
        let bitmap = rasterize(256.0).expect("the mark has ink");
        let below = bitmap.ymin;
        let above = 256 - (bitmap.ymin + bitmap.h as i32);
        assert!((above - below).abs() <= 1, "{above} above, {below} below");
        let (mut mass, mut moment) = (0.0, 0.0);
        for (i, coverage) in bitmap.data.iter().enumerate() {
            let coverage = f32::from(*coverage);
            mass += coverage;
            moment += coverage * (bitmap.xmin as f32 + (i % bitmap.w as usize) as f32 + 0.5);
        }
        let centre = moment / mass;
        assert!((centre - 128.0).abs() < 3.0, "the ink's weight sits at {centre}");
    }
}
