// level.rs — Level file format: load and save levels as human-readable RON files.
//
// ── WHY RON? ─────────────────────────────────────────────────────────────────
//
// RON (Rusty Object Notation) is a data format designed specifically for Rust.
// It looks like Rust source code — enums, structs, tuples — so a `.level` file
// is readable and editable by hand without needing a separate tool.
//
// Compared to JSON:
//   ✓  Supports Rust enums as bare names:  `fg: Yellow`  not  `"fg": "Yellow"`
//   ✓  Trailing commas allowed             (JSON: syntax error)
//   ✓  Comments allowed with //            (JSON: not supported)
//   ✗  Less universal tooling than JSON
//
// Example `.level` file on disk:
//
//   LevelData(
//       name: "My Level",
//       width: 80,
//       height: 24,
//       spawn_point: (5.0, 5.0),
//       tiles: [
//           TileRecord(x: 2, y: 2, glyph: '#', fg: Grey, bg: Reset,
//                      solid: true, trigger: false, tag: "wall"),
//           TileRecord(x: 3, y: 2, glyph: '.', fg: DarkGrey, bg: Reset,
//                      solid: false, trigger: false, tag: "floor"),
//       ],
//   )
//
// ── HOW SERIALIZATION WORKS ───────────────────────────────────────────────────
//
// serde is Rust's universal serialization framework.
// Adding `#[derive(Serialize, Deserialize)]` to a struct or enum tells the
// Rust compiler to auto-generate all the code needed to convert it to and from
// any data format that serde supports (JSON, RON, TOML, CBOR, …).
//
// We use `ron::ser::to_string_pretty` to convert a LevelData value into a RON
// string, then write that string to a file.
// We use `ron::de::from_str` to parse a RON string back into a LevelData value.
//
// Because Color (in renderer/color.rs) also derives Serialize + Deserialize,
// it serializes to its variant name: `Yellow`, `Reset`, etc. — not a number.

use std::fs;
use serde::{Deserialize, Serialize};
use crate::color::Color;

// ── TileRecord ────────────────────────────────────────────────────────────────

/// One tile entry in a saved level file.
///
/// Stores everything the editor placed at a particular (x, y) cell:
/// the visual appearance (glyph, fg, bg) and the gameplay properties
/// (solid for wall collision, trigger for item/zone detection, tag for logic).
///
/// `#[derive(Serialize, Deserialize)]` generates the RON/JSON/… encoding
/// automatically — no hand-written parsing code required.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileRecord {
    /// Column in character-cell space (0 = leftmost column).
    pub x: i32,

    /// Row in character-cell space (0 = topmost row).
    pub y: i32,

    /// The ASCII character displayed at this cell (e.g. '#', '.', '*').
    pub glyph: char,

    /// Foreground (text/glyph) color.
    pub fg: Color,

    /// Background color. Use `Color::Reset` for the engine default.
    pub bg: Color,

    /// If true, the physics system treats this tile as a solid wall.
    /// Moving entities that overlap it are pushed back.
    #[serde(default)]
    pub solid: bool,

    /// If true, this tile fires a Collision event when overlapped,
    /// but does NOT block movement (e.g. a collectible item or a zone).
    #[serde(default)]
    pub trigger: bool,

    /// Optional semantic label used by game logic (e.g. "wall", "floor", "item").
    /// Empty string means no tag.
    #[serde(default)]
    pub tag: String,

    /// The draw layer (0=Background, 1=Main, 2=Foreground).
    #[serde(default = "default_layer")]
    pub layer: u8,

    /// Optional path to a .rhai script file run every frame for this entity.
    ///
    /// `#[serde(default)]` means existing .level files that don't have this
    /// field will deserialize it as `None` rather than failing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,

    /// Optional collision layer name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub collider_layer: String,

    /// Optional collision mask: layers this tile should interact with.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collider_mask: Vec<String>,

    /// If true, the play-mode camera centers on this entity's position.
    /// Only one entity should have this set at a time; the first one wins.
    #[serde(default, skip_serializing_if = "is_false")]
    pub camera_follow: bool,

    /// If set, walking into this trigger in play mode loads this level file path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_level: Option<String>,

    /// Blueprint-style node graph — editor-authoring state only, never a
    /// runtime concern. Since Step 3d (`LEVEL_FORMAT_VERSION` 1),
    /// `EditorState::save`'s sidecar migration generates this graph's Rhai
    /// once, combines it with whatever `script` already pointed to, writes
    /// the result to a `.rhai` file beside the level, repoints `script` at
    /// it, and drops this field before the `TileRecord` is serialized — a
    /// saved `.level` file never carries a `graph`. It survives here only
    /// for the live in-memory grid the node-graph editor keeps working on,
    /// and for a still-unsaved level played via F5 preview (see
    /// `play/spawn.rs::do_on_start`'s own runtime combine, kept for that
    /// case).
    ///
    /// `NodeGraph` itself lives in the crate-root `graph` module, not under
    /// `editor` (Step 5a, docs/ember2d-phase5-plan.md) — despite being
    /// editor-authoring data, both `level.rs` (here) and `play/spawn.rs`
    /// need the type and `generate_graph` without depending on the editor to
    /// get them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<crate::graph::NodeGraph>,

    /// Optional path to a texture file (for Sprites2D mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<String>,

    /// Makes this tile's entity eligible to take turns under
    /// `TurnScheduler` (Step 5f, docs/ember2d-phase5-plan.md;
    /// `LEVEL_FORMAT_VERSION` 1 → 2). The player needs no such field — it's
    /// implicitly `Controller::Local(0)` at speed 100, added unconditionally
    /// in `play/spawn.rs` regardless of what any tile authors. Only
    /// `Controller::Ai` is ever authored this way; `Local`/`Remote` are
    /// assigned by the engine, never by level data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<ActorRecord>,
}

fn default_layer() -> u8 { 1 }
fn is_false(b: &bool) -> bool { !*b }

// ── ActorRecord ───────────────────────────────────────────────────────────────

/// See `TileRecord::actor`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorRecord {
    #[serde(default = "default_actor_speed")]
    pub speed: u32,
}

fn default_actor_speed() -> u32 { 100 }

impl Default for ActorRecord {
    fn default() -> Self {
        ActorRecord { speed: default_actor_speed() }
    }
}

// ── PlayerRecord ──────────────────────────────────────────────────────────────

/// Properties of the player entity. Stored in the level file and editable
/// from the editor's hierarchy panel, independently of tile records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerRecord {
    #[serde(default = "default_player_glyph")]
    pub glyph: char,
    #[serde(default = "default_player_fg")]
    pub fg: Color,
    #[serde(default = "default_player_bg")]
    pub bg: Color,
    #[serde(default)]
    pub solid: bool,
    #[serde(default)]
    pub trigger: bool,
    #[serde(default = "default_player_tag")]
    pub tag: String,
    #[serde(default = "bool_true")]
    pub camera_follow: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    #[serde(default)]
    pub collider_layer: String,
    #[serde(default)]
    pub collider_mask: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<String>,

    /// Player hitbox size in world units. Phase 4
    /// (docs/ember2d-refactor-plan.md): this used to be a `Collider::new(0.75,
    /// 0.75)` hardcoded in `play/spawn.rs`, regardless of what a project's
    /// player actually needed. `#[serde(default = "default_player_collider_size")]`
    /// preserves that exact 0.75 value for every level saved before these
    /// fields existed.
    #[serde(default = "default_player_collider_size")]
    pub collider_w: f32,
    #[serde(default = "default_player_collider_size")]
    pub collider_h: f32,

    /// Player draw order. Step 4g: this used to be a hardcoded `Z_PLAYER: i32
    /// = 15` constant in `play.rs`, same shape as `collider_w`/`collider_h`
    /// above — `#[serde(default = "default_player_layer")]` preserves that
    /// exact 15 value for every level saved before this field existed.
    #[serde(default = "default_player_layer")]
    pub layer: i32,
}

fn default_player_glyph() -> char { '@' }
fn default_player_fg()    -> Color { Color::Green }
fn default_player_bg()    -> Color { Color::Reset }
fn default_player_tag()   -> String { "player".to_string() }
fn bool_true()            -> bool { true }
fn default_player_collider_size() -> f32 { 0.75 }
fn default_player_layer() -> i32 { 15 }

impl Default for PlayerRecord {
    fn default() -> Self {
        PlayerRecord {
            glyph:          '@',
            fg:             Color::Green,
            bg:             Color::Reset,
            solid:          false,
            trigger:        false,
            tag:            "player".to_string(),
            camera_follow:  true,
            script:         None,
            collider_layer: String::new(),
            collider_mask:  Vec::new(),
            texture:        None,
            collider_w:     default_player_collider_size(),
            collider_h:     default_player_collider_size(),
            layer:          default_player_layer(),
        }
    }
}

impl TileRecord {
    /// Construct a TileRecord with all fields explicit.
    pub fn new(
        x: i32, y: i32,
        layer: u8,
        glyph: char,
        fg: Color, bg: Color,
        solid: bool, trigger: bool,
        tag: impl Into<String>,
    ) -> Self {
        TileRecord { 
            x, y, layer, glyph, fg, bg, solid, trigger, 
            tag: tag.into(), 
            script: None, 
            collider_layer: String::new(),
            collider_mask: Vec::new(),
            camera_follow: false,
            next_level: None,
            graph: None,
            texture: None,
            actor: None,
        }
    }
}

// ── LevelData ─────────────────────────────────────────────────────────────────

/// The complete, serializable description of a level.
///
/// The editor produces one of these when you press S. The preview/play mode
/// consumes one to build the World (ECS) that the player runs around in.
///
/// All fields are public so game code can read them without ceremony.
/// Level file format version. Bumped whenever the on-disk shape or the
/// meaning of an existing field changes in a way a loader needs to know
/// about. Version 1 is Phase 3 Step 3d: the node-graph → sidecar-script
/// migration (see `TileRecord::graph`'s doc comment) — nothing else about
/// the format changed, so there is no migration logic yet, only the field
/// itself for future steps to key off of.
/// Version 2 is Phase 5 Step 5f: `TileRecord::actor` (docs/ember2d-phase5-plan.md).
/// Both are purely additive, `#[serde(default)]` fields — an old level still
/// loads with `actor: None` everywhere, which plays gracefully (nothing but
/// the player, itself always eligible, ever takes a turn) rather than hanging.
pub const LEVEL_FORMAT_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelData {
    /// Level file format version — see `LEVEL_FORMAT_VERSION`.
    /// `#[serde(default)]` means every `.level` file saved before this
    /// field existed loads as version 0, which needs no migration since
    /// nothing else about the format has changed shape yet.
    #[serde(default)]
    pub version: u32,

    /// Human-readable level name (shown in the editor title bar).
    pub name: String,

    /// Level width in character columns (must match the editor viewport).
    pub width: usize,

    /// Level height in character rows (must match the editor viewport).
    pub height: usize,

    /// Where the player entity spawns when the level is played.
    /// Stored as (column, row) in character-cell space.
    pub spawn_point: (f32, f32),

    /// Named entity spawn points placed with Shift+P in the editor.
    /// Each entry is (name, x, y). Game code can look these up by name
    /// to spawn enemies, NPCs, or other scripted entities at level start.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_spawns: Vec<(String, f32, f32)>,

    /// All tiles placed in this level, in the order the editor placed them.
    /// The order does not matter for rendering — the renderer sorts by z_order.
    pub tiles: Vec<TileRecord>,

    /// Player entity properties (glyph, camera, script, etc.).
    /// Uses Default if missing so old .level files still load.
    #[serde(default)]
    pub player: PlayerRecord,

    /// The file path this level was loaded from, used to resolve relative
    /// `next_level` exit paths at runtime. Not written to disk.
    #[serde(skip, default)]
    pub path: String,

    /// Seeds every RNG stream play mode uses for this level — script
    /// `random_*` calls, particle velocity/life, and camera shake jitter.
    ///
    /// Defect D3: these used to be reseeded from OS entropy on every run
    /// (scripts) or reallocated from entropy on every single call (particles,
    /// shake), so nothing was reproducible. Chosen once, here, and reused
    /// for the life of the level: replaying the same level with the same
    /// inputs now produces the same "random" outcomes every time — required
    /// for automated tests later (Phase 5) and for netcode (§5) after that.
    /// `#[serde(default)]` means old .level files without this field load as
    /// seed 0, which is still fully deterministic — just not level-unique.
    #[serde(default)]
    pub seed: u64,
}

impl LevelData {
    /// Create an empty level with no tiles and the spawn point at (1, 1).
    ///
    /// `width` and `height` should match the editor's canvas size so that
    /// the grid overlay lines up correctly.
    ///
    /// The seed is picked once here, from OS entropy, and then written to
    /// disk with everything else — a fresh level gets its own distinct
    /// "randomness", but from that point on it's fixed and reproducible
    /// (defect D3), not re-rolled on every launch.
    pub fn empty(width: usize, height: usize) -> Self {
        LevelData {
            version:      LEVEL_FORMAT_VERSION,
            name:         String::from("Untitled"),
            width,
            height,
            spawn_point:  (1.0, 1.0),
            extra_spawns: Vec::new(),
            tiles:        Vec::new(),
            player:       PlayerRecord::default(),
            path:         String::new(),
            seed:         rand::random(),
        }
    }

    // ── Persistence ───────────────────────────────────────────────────────────

    /// Serialize this LevelData to a pretty-printed RON string and write it to
    /// `path` on disk.
    ///
    /// Returns an error if:
    ///   - RON serialization fails (shouldn't happen with valid data)
    ///   - The file cannot be created or written (permissions, bad path, etc.)
    ///
    /// `Box<dyn std::error::Error>` lets us return either a ron error or an
    /// io::Error without choosing a single error type up front.
    pub fn save(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        // PrettyConfig controls the RON output formatting.
        // depth_limit controls indentation depth; new_line controls line endings.
        let config = ron::ser::PrettyConfig::new()
            .depth_limit(4)
            .new_line("\n".to_string());

        // Serialize self to a RON string.
        let ron_string = ron::ser::to_string_pretty(self, config)?;

        // Write the string to the file, creating it if it doesn't exist.
        fs::write(path, ron_string)?;

        Ok(())
    }

    /// Read a `.level` file from disk and deserialize it back into a LevelData.
    ///
    /// Returns an error if:
    ///   - The file doesn't exist or can't be read
    ///   - The RON content is malformed or doesn't match LevelData's structure
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        // Read the entire file into a String.
        let content = fs::read_to_string(path)?;

        // Parse the RON string. The type annotation tells ron what Rust type
        // to deserialize into — it uses LevelData's Deserialize impl.
        let mut level: LevelData = ron::de::from_str(&content)?;
        level.path = path.to_string();

        Ok(level)
    }
}
