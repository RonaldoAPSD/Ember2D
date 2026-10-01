// scripting/tiles.rs — scripts placing static tiles: declare a tile once
// (`tile_def`), then stamp it into the level's tilemap (`tile_set`,
// `tile_fill`), clear cells (`tile_clear`, `tile_clear_rect`), size the
// grid (`tilemap_resize`), and read back what a cell holds (`get_tile`).
//
// Step 9.5-1 (docs/ember2d-master-plan.md §5.8.5), for the roguelike's
// generated floors. Before it, a script could only make a wall by spawning
// an entity — exactly the one-entity-per-tile cost Step 8-1's `Tilemap`
// removed (a 80x45 floor is 3,600 cells). These write cells instead.
//
// Like every other write, a call only QUEUES a `TileOp`, in call order.
// Applying one needs things the script engine doesn't have — the layer
// registry (a solid cell's collision bits) and, for a sprite tile, the
// tileset file — so the ops travel back in `ScriptUpdateResult::tile_ops`
// and `Simulation` applies them (simulation/tiles.rs), after the pass, the
// way 9-7's sprite requests go. Consequences a script can see:
//   - collision (physics, `move_toward`'s solid checks later in the same
//     step) sees the new cells at once;
//   - a script's own reads (`is_solid_at`, `get_tile`, `get_tile_tag`,
//     pathfinding) see them from the NEXT step, like any deferred write —
//     the pass reads the snapshot taken before it ran.
//
// Which tilemap: the level's own — the first one `World` holds (a level's
// static tiles are baked into exactly one). A level with no static tiles
// gets one the size of the level the first time a script places a tile.

use crate::components::{TileDef, TileStamp};
use crate::tileset::SpriteRef;

use super::api::ScriptCtx;
use super::types::try_parse_color;

/// One tile request from a script, in call order.
#[derive(Debug, Clone, PartialEq)]
pub enum TileOp {
    /// Declare (or redeclare) a named tile. Cells already placed keep the
    /// look they were placed with; later placements use the new one.
    Def(String, TileStamp),
    /// Replace the tilemap with an empty `w`×`h` grid at (0, 0).
    Resize(i64, i64),
    /// Every cell of the rect (`w`/`h` cells from `x`, `y`): place tile
    /// `name` (`Some`) or clear (`None`). `layer: None` means the tile's
    /// own layer when placing, every layer when clearing.
    Rect { x: i64, y: i64, w: i64, h: i64, layer: Option<u8>, name: Option<String> },
    /// A request refused at call time; the reason is logged when applied.
    Bad(String),
}

/// The keys `tile_def` understands. A static tile can't do anything an
/// entity would have to (a trigger, an animation, a script), so those keys
/// are refused by name rather than silently ignored.
const KNOWN_KEYS: &[&str] =
    &["glyph", "fg", "bg", "solid", "tag", "layer", "sprite", "collider_layer"];
const ENTITY_ONLY: &[&str] = &["trigger", "clip", "script", "exit", "next_level", "actor"];

/// `tile_def`'s map, read into a stamp — or why it can't be one.
fn parse_def(name: &str, spec: &rhai::Map) -> Result<TileStamp, String> {
    if name.is_empty() {
        return Err("tile_def: a tile needs a name".into());
    }
    for key in spec.keys() {
        let key = key.as_str();
        if ENTITY_ONLY.contains(&key) {
            return Err(format!(
                "tile_def(\"{name}\"): '{key}' needs an entity — spawn one instead (a tile is static)"
            ));
        }
        if !KNOWN_KEYS.contains(&key) {
            return Err(format!(
                "tile_def(\"{name}\"): unknown key '{key}' (known: {})",
                KNOWN_KEYS.join(", ")
            ));
        }
    }
    let text = |key: &str| spec.get(key).map(|v| v.to_string());
    let colour = |key: &str, default| match text(key) {
        None => Ok(default),
        Some(c) => try_parse_color(&c).ok_or(format!("tile_def(\"{name}\"): '{c}' isn't a colour")),
    };
    let solid = match spec.get("solid") {
        None => false,
        Some(v) => {
            v.as_bool().map_err(|_| format!("tile_def(\"{name}\"): solid must be true or false"))?
        }
    };
    let layer = match spec.get("layer") {
        None => 0,
        Some(v) => {
            let n = v
                .as_int()
                .map_err(|_| format!("tile_def(\"{name}\"): layer must be a whole number"))?;
            u8::try_from(n)
                .map_err(|_| format!("tile_def(\"{name}\"): layer {n} is outside 0..255"))?
        }
    };
    let sprite = match text("sprite") {
        None => None,
        Some(s) => match s.split_once(':') {
            Some((set, region)) if !set.is_empty() && !region.is_empty() => {
                Some(SpriteRef::new(set.to_string(), region.to_string()))
            }
            _ => {
                return Err(format!(
                    "tile_def(\"{name}\"): sprite is \"tileset:region\", not \"{s}\""
                ))
            }
        },
    };
    let def = TileDef {
        glyph: text("glyph").and_then(|g| g.chars().next()).unwrap_or(' '),
        // A sprite tile draws its art as-is; a glyph tile in plain white
        // unless told otherwise (`try_parse_color` knows "White").
        fg: colour("fg", crate::color::Color::White)?,
        bg: colour("bg", crate::color::Color::Reset)?,
        solid,
        tag: text("tag").unwrap_or_default(),
        collider_layer: text("collider_layer").unwrap_or_default(),
        texture: None,
        sprite,
        src: None,
        name: name.to_string(),
    };
    Ok(TileStamp { def, layer })
}

/// A script layer number, or why it isn't one.
fn layer_of(call: &str, layer: i64) -> Result<u8, String> {
    u8::try_from(layer).map_err(|_| format!("{call}: layer {layer} is outside 0..255"))
}

/// A float coordinate as a cell, the way every spatial query floors one; a
/// NaN or infinite value becomes a cell far outside any grid (ignored).
fn cell(v: f64) -> i64 {
    if v.is_finite() {
        v.floor().clamp(i64::MIN as f64, i64::MAX as f64) as i64
    } else {
        i64::MIN
    }
}

impl ScriptCtx {
    fn push_tile(&mut self, op: TileOp) {
        self.inner.borrow_mut().tile_ops.push(op);
    }

    /// Declare tile `name`: `#{ glyph, fg, bg, solid, tag, layer, sprite,
    /// collider_layer }`, every key optional (a space, white on nothing,
    /// walkable, layer 0). `sprite: "tileset:region"` draws a project
    /// tileset region instead of the glyph.
    pub fn tile_def(&mut self, name: String, spec: rhai::Map) {
        let op = match parse_def(&name, &spec) {
            Ok(stamp) => TileOp::Def(name, stamp),
            Err(why) => TileOp::Bad(why),
        };
        self.push_tile(op);
    }

    /// Empty the level's tilemap and make it `w`×`h` cells from (0, 0).
    pub fn tilemap_resize(&mut self, w: i64, h: i64) {
        self.push_tile(TileOp::Resize(w, h));
    }
    fn tilemap_resize_f(&mut self, w: f64, h: f64) {
        self.tilemap_resize(cell(w), cell(h));
    }

    /// Place tile `name` at (x, y), on its own layer.
    pub fn tile_set(&mut self, x: i64, y: i64, name: String) {
        self.push_tile(TileOp::Rect { x, y, w: 1, h: 1, layer: None, name: Some(name) });
    }
    fn tile_set_f(&mut self, x: f64, y: f64, name: String) {
        self.tile_set(cell(x), cell(y), name);
    }

    /// Place tile `name` at (x, y) on `layer`.
    pub fn tile_set_layer(&mut self, x: i64, y: i64, layer: i64, name: String) {
        self.tile_fill_layer(x, y, 1, 1, layer, name);
    }
    fn tile_set_layer_f(&mut self, x: f64, y: f64, layer: f64, name: String) {
        self.tile_set_layer(cell(x), cell(y), cell(layer), name);
    }
    // Float coordinates with a whole-number layer — the natural call with a
    // position from `get_x`/`get_y` (floats) and a layer literal (an int):
    // Rhai matches overloads by exact types, so without this
    // `tile_set_layer(ctx.get_x(id), ctx.get_y(id), 1, "x")` doesn't resolve.
    // Same for `tile_clear_layer` and `get_tile` below.
    fn tile_set_layer_fi(&mut self, x: f64, y: f64, layer: i64, name: String) {
        self.tile_set_layer(cell(x), cell(y), layer, name);
    }

    /// Place tile `name` in every cell of the `w`×`h` rect at (x, y).
    pub fn tile_fill(&mut self, x: i64, y: i64, w: i64, h: i64, name: String) {
        self.push_tile(TileOp::Rect { x, y, w, h, layer: None, name: Some(name) });
    }
    fn tile_fill_f(&mut self, x: f64, y: f64, w: f64, h: f64, name: String) {
        self.tile_fill(cell(x), cell(y), cell(w), cell(h), name);
    }

    /// `tile_fill` on `layer`.
    pub fn tile_fill_layer(&mut self, x: i64, y: i64, w: i64, h: i64, layer: i64, name: String) {
        let op = match layer_of("tile_fill_layer", layer) {
            Ok(l) => TileOp::Rect { x, y, w, h, layer: Some(l), name: Some(name) },
            Err(why) => TileOp::Bad(why),
        };
        self.push_tile(op);
    }
    fn tile_fill_layer_f(&mut self, x: f64, y: f64, w: f64, h: f64, layer: f64, name: String) {
        self.tile_fill_layer(cell(x), cell(y), cell(w), cell(h), cell(layer), name);
    }

    /// Clear (x, y) on every layer.
    pub fn tile_clear(&mut self, x: i64, y: i64) {
        self.tile_clear_rect(x, y, 1, 1);
    }
    fn tile_clear_f(&mut self, x: f64, y: f64) {
        self.tile_clear(cell(x), cell(y));
    }

    /// Clear (x, y) on `layer` only.
    pub fn tile_clear_layer(&mut self, x: i64, y: i64, layer: i64) {
        self.tile_clear_rect_layer(x, y, 1, 1, layer);
    }
    fn tile_clear_layer_f(&mut self, x: f64, y: f64, layer: f64) {
        self.tile_clear_layer(cell(x), cell(y), cell(layer));
    }
    fn tile_clear_layer_fi(&mut self, x: f64, y: f64, layer: i64) {
        self.tile_clear_layer(cell(x), cell(y), layer);
    }

    /// Clear every cell of the `w`×`h` rect at (x, y), on every layer.
    pub fn tile_clear_rect(&mut self, x: i64, y: i64, w: i64, h: i64) {
        self.push_tile(TileOp::Rect { x, y, w, h, layer: None, name: None });
    }
    fn tile_clear_rect_f(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.tile_clear_rect(cell(x), cell(y), cell(w), cell(h));
    }

    /// `tile_clear_rect` on `layer` only.
    pub fn tile_clear_rect_layer(&mut self, x: i64, y: i64, w: i64, h: i64, layer: i64) {
        let op = match layer_of("tile_clear_rect_layer", layer) {
            Ok(l) => TileOp::Rect { x, y, w, h, layer: Some(l), name: None },
            Err(why) => TileOp::Bad(why),
        };
        self.push_tile(op);
    }
    fn tile_clear_rect_layer_f(&mut self, x: f64, y: f64, w: f64, h: f64, layer: f64) {
        self.tile_clear_rect_layer(cell(x), cell(y), cell(w), cell(h), cell(layer));
    }

    /// The name of the tile on `layer` at (x, y), as `tile_def` gave it —
    /// `""` for an empty cell or a tile the editor painted (those have no
    /// name; `get_tile_tag` reads their tag). Sees the map as it was at the
    /// start of this pass.
    pub fn get_tile(&mut self, x: i64, y: i64, layer: i64) -> String {
        let (Ok(x), Ok(y), Ok(layer)) = (i32::try_from(x), i32::try_from(y), u8::try_from(layer))
        else {
            return String::new();
        };
        let s = self.inner.borrow();
        s.tilemaps
            .values()
            .map(|m| m.name_at(layer, x, y))
            .find(|n| !n.is_empty())
            .unwrap_or("")
            .to_string()
    }
    fn get_tile_f(&mut self, x: f64, y: f64, layer: f64) -> String {
        self.get_tile(cell(x), cell(y), cell(layer))
    }
    fn get_tile_fi(&mut self, x: f64, y: f64, layer: i64) -> String {
        self.get_tile(cell(x), cell(y), layer)
    }
}

pub(super) fn register(engine: &mut rhai::Engine) {
    engine.register_fn("tile_def", ScriptCtx::tile_def);
    engine.register_fn("tilemap_resize", ScriptCtx::tilemap_resize);
    engine.register_fn("tilemap_resize", ScriptCtx::tilemap_resize_f);
    engine.register_fn("tile_set", ScriptCtx::tile_set);
    engine.register_fn("tile_set", ScriptCtx::tile_set_f);
    engine.register_fn("tile_set_layer", ScriptCtx::tile_set_layer);
    engine.register_fn("tile_set_layer", ScriptCtx::tile_set_layer_f);
    engine.register_fn("tile_set_layer", ScriptCtx::tile_set_layer_fi);
    engine.register_fn("tile_fill", ScriptCtx::tile_fill);
    engine.register_fn("tile_fill", ScriptCtx::tile_fill_f);
    engine.register_fn("tile_fill_layer", ScriptCtx::tile_fill_layer);
    engine.register_fn("tile_fill_layer", ScriptCtx::tile_fill_layer_f);
    engine.register_fn("tile_clear", ScriptCtx::tile_clear);
    engine.register_fn("tile_clear", ScriptCtx::tile_clear_f);
    engine.register_fn("tile_clear_layer", ScriptCtx::tile_clear_layer);
    engine.register_fn("tile_clear_layer", ScriptCtx::tile_clear_layer_f);
    engine.register_fn("tile_clear_layer", ScriptCtx::tile_clear_layer_fi);
    engine.register_fn("tile_clear_rect", ScriptCtx::tile_clear_rect);
    engine.register_fn("tile_clear_rect", ScriptCtx::tile_clear_rect_f);
    engine.register_fn("tile_clear_rect_layer", ScriptCtx::tile_clear_rect_layer);
    engine.register_fn("tile_clear_rect_layer", ScriptCtx::tile_clear_rect_layer_f);
    engine.register_fn("get_tile", ScriptCtx::get_tile);
    engine.register_fn("get_tile", ScriptCtx::get_tile_f);
    engine.register_fn("get_tile", ScriptCtx::get_tile_fi);
}
