//! The app's mark: the letters RP, wide and square-shouldered, drawn from
//! their outline at whatever size is asked for, in one colour, through the
//! glyph atlas ([`crate::paint::Painter::logo`]): the mark costs what a
//! letter costs. The app icon is this mark on a dark tile (the `icon` test
//! in `chat/snapshot.rs` draws it).

use ttf_parser::OutlineBuilder;

use crate::raster::{self, Bitmap};

/// The side of the square the outline is drawn on.
const BOX: f32 = 24.0;
/// Width and height of a letter.
const LETTER: (f32, f32) = (7.4, 9.4);
/// Space between the letters.
const SPACE: f32 = 1.6;
/// Thickness of a letter's strokes.
const STROKE: f32 = 1.95;
/// Height of a letter's bowl, strokes included.
const BOWL: f32 = 5.8;
/// Radius of the bowl's outer corners.
const ROUND: f32 = 2.3;

/// The mark's coverage on a square `px` pixels a side: a bitmap placed
/// relative to the square's bottom-left corner, y up, as a glyph's is to
/// its pen. `None` if it has no ink at that size.
#[must_use]
pub fn rasterize(px: f32) -> Option<Bitmap> {
    raster::rasterize_outline(px / BOX, |sink| {
        let left = (BOX - 2.0 * LETTER.0 - SPACE) * 0.5;
        letter(sink, left, true);
        letter(sink, left + LETTER.0 + SPACE, false);
    })
}

/// Draws a P whose left edge is at `x` into `sink`, or with `leg` an R: a
/// stem, a bowl with round shoulders, and the slit inside it.
fn letter(sink: &mut dyn OutlineBuilder, x: f32, leg: bool) {
    let (w, h) = LETTER;
    let top = (BOX - h) * 0.5;
    // Points are given from the letter's top-left corner, y down; the sink wants y up.
    let at = |dx: f32, dy: f32| (x + dx, BOX - top - dy);
    let line = |sink: &mut dyn OutlineBuilder, dx: f32, dy: f32| sink.line_to(at(dx, dy).0, at(dx, dy).1);
    // A quarter turn from `from` round `corner` to `to`.
    let turn = |sink: &mut dyn OutlineBuilder, from: (f32, f32), corner: (f32, f32), to: (f32, f32)| {
        let toward = |a: (f32, f32)| at(a.0 + (corner.0 - a.0) * 0.5523, a.1 + (corner.1 - a.1) * 0.5523);
        let ((c1, c2), end) = ((toward(from), toward(to)), at(to.0, to.1));
        sink.curve_to(c1.0, c1.1, c2.0, c2.1, end.0, end.1);
    };

    sink.move_to(at(0.0, 0.0).0, at(0.0, 0.0).1);
    line(sink, w - ROUND, 0.0);
    turn(sink, (w - ROUND, 0.0), (w, 0.0), (w, ROUND));
    line(sink, w, BOWL - ROUND);
    turn(sink, (w, BOWL - ROUND), (w, BOWL), (w - ROUND, BOWL));
    if leg {
        // The leg leaves the bowl's corner for the baseline, as thick as a stroke.
        let run = (h - BOWL).hypot(ROUND);
        let across = STROKE * run / (h - BOWL);
        line(sink, w, h);
        line(sink, w - across, h);
    }
    line(sink, STROKE, BOWL);
    line(sink, STROKE, h);
    line(sink, 0.0, h);
    sink.close();

    // The slit, wound the other way so it is cut out.
    let (right, bottom, inner) = (w - STROKE, BOWL - STROKE, 0.5);
    sink.move_to(at(STROKE, STROKE).0, at(STROKE, STROKE).1);
    line(sink, STROKE, bottom);
    line(sink, right - inner, bottom);
    turn(sink, (right - inner, bottom), (right, bottom), (right, bottom - inner));
    line(sink, right, STROKE + inner);
    turn(sink, (right, STROKE + inner), (right, STROKE), (right - inner, STROKE));
    sink.close();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mark_draws_inside_its_square() {
        for px in [16.0_f32, 22.0, 64.0, 256.0] {
            let bitmap = rasterize(px).expect("the mark has ink");
            let ink = bitmap.data.iter().map(|coverage| f32::from(*coverage) / 255.0).sum::<f32>() / (px * px);
            // Two letters of strokes: a fifth of the square, give or take, and the slits are cut out.
            assert!((0.12..0.3).contains(&ink), "inks {ink} of a {px} square");
            let (right, top) = (bitmap.xmin + bitmap.w as i32, bitmap.ymin + bitmap.h as i32);
            let side = px as i32;
            assert!(bitmap.xmin >= 0 && bitmap.ymin >= 0 && right <= side && top <= side, "leaves its square");
        }
    }
}
