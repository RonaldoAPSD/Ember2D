// renderer/draw_log.rs — DrawOp: an opt-in record of what `NullRenderer`
// was asked to draw (7D-3, docs/ember2d-master-plan.md §5.4). Exists
// because none of this project's other headless testing (`EditorHarness`,
// `TurnHarness`) renders real pixels — `UiFrame` hit-testing verifies WHERE
// a widget would be clickable, but nothing before this could verify what a
// draw call actually computed (a fill's exact snapped rect, a glyph's real
// rasterization size, a 9-slice's border scale). UI-points snapping is
// exactly the kind of fine-grained numeric contract that needs that: "every
// physical pixel a chrome quad's edge lands on is a whole number" isn't
// visible in a `UiFrame` rect, only in what got handed to `DrawSurface`.
//
// All positions/sizes recorded here are in LOGICAL pixels — `DrawSurface`'s
// own native unit — exactly what a real `Renderer` would receive from the
// same call, so a test asserting on these ops is asserting on the same
// values a live GPU draw would actually get.

use ember2d_sim::math::{Rect, Vec2};

#[derive(Clone, Debug, PartialEq)]
pub enum DrawOp {
    Fill(Rect),
    NineSlice {
        dest: Rect,
        src: Rect,
        border: (f32, f32, f32, f32),
        border_scale: f32,
    },
    /// `raster_px`/`texel_scale`/`pitch` mirror `TextRun`'s own fields
    /// exactly — see that struct's doc comment (`draw_surface.rs`) for what
    /// each means; recorded here rather than the drawn glyph quads
    /// themselves, since the CONTRACT this step's tests need to verify is
    /// "what size did this rasterize at / what texel scale did it draw
    /// with," not the exact pixels of a bitmap or TTF glyph.
    Text {
        text: String,
        origin: Vec2,
        raster_px: f32,
        texel_scale: f32,
        pitch: Option<f32>,
    },
    Scissor(Option<Rect>),
    Char {
        pos: Vec2,
        ch: char,
        scale: f32,
    },
}
