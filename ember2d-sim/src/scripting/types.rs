// scripting/types.rs — Log types, HUD drawing, and internal scripting types.

use std::collections::BTreeMap;
use crate::command::Command;
use crate::color::Color;
use crate::world::EntityId;

/// The scripting API's breaking-change generation, returned by
/// `ctx.api_version()`. See `docs/ember2d-scripting-api.md` §6's changelog
/// table: v1 is the pre-refactor baseline; v2 is Phase 2 (camera zoom added
/// — no scripted control yet); v3 is Phase 3's breaking renames
/// (`set_color`→`set_tint`, `set_z_order`→`set_layer_order`,
/// `set_animation` removed in favor of the clip API); v4 is Step 4g —
/// `get_mouse_world_y` stops subtracting a HUD row now that `HUD_TOP_ROWS`
/// is 0 (an earlier version of this comment claimed that shipped already in
/// v2; it hadn't — `HUD_TOP_ROWS` only got centralized into one constant
/// then, never actually zeroed); v5 is Phase 5 Step 5e
/// (docs/ember2d-phase5-plan.md) — the `on_input` lifecycle plus
/// `ctx.submit`/`command_action`/`command_param`. Bump this alongside the
/// next "Yes" row in that table.
/// v6 is Step 5f (docs/ember2d-phase5-plan.md): the `on_turn` lifecycle,
/// `ctx.act`/`get_turn_number`/`get_speed`/`set_speed`, and
/// `ctx.trigger_turn` removed outright (the turn scheduler replaces it —
/// see `ScriptUpdateResult`'s field doc comments below).
pub const API_VERSION: i64 = 6;

// ── Console log types (used by editor console panel) ─────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum LogLevel { Error, Warning, Info }

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: LogLevel,
    pub text:  String,
}

impl LogEntry {
    pub fn error(text: impl Into<String>) -> Self { LogEntry { level: LogLevel::Error,   text: text.into() } }
    pub fn warn(text:  impl Into<String>) -> Self { LogEntry { level: LogLevel::Warning, text: text.into() } }
    pub fn info(text:  impl Into<String>) -> Self { LogEntry { level: LogLevel::Info,    text: text.into() } }
}

pub struct ScriptUpdateResult {
    pub pending_level:   Option<String>,
    pub pending_save:    Option<String>,
    pub pending_load:    Option<String>,
    pub globals:         BTreeMap<String, rhai::Dynamic>,
    pub clips:           BTreeMap<String, crate::components::AnimationClip>,
    pub persistent:      BTreeMap<String, rhai::Dynamic>,
    pub camera_override: Option<crate::math::Vec2>,
    pub shake_state:     Option<ShakeState>,
    pub clear_hud:       bool,
    pub particles:       Vec<ParticleRequest>,
    /// Commands `ctx.submit()` queued this pass, keyed by actor id (Step
    /// 5e, docs/ember2d-phase5-plan.md) — `on_input`'s own result feeds
    /// this into the subsequent `on_update` pass's `ScriptState.commands`,
    /// which is what `ctx.command_action()`/`command_param()` read. Unlike
    /// `globals`, this does *not* accumulate across passes — each
    /// `on_input` pass's commands fully replace whatever was here before,
    /// since a command means "what this actor wants to do this step," not
    /// persistent state.
    pub commands:        BTreeMap<i64, Command>,
    /// `Some(cost)` if `ctx.act(cost)` was called this pass — Step 5f
    /// (docs/ember2d-phase5-plan.md). Meaningful only for an `on_turn`
    /// pass: `PlayState::run_actor_turn` (play.rs) reads it to decide both
    /// *whether* this step consumed a turn (an AI actor's turn always
    /// counts even without calling `act`, so a sleeping monster can't wedge
    /// the scheduler — but a `Local` actor's turn counts only if `act` was
    /// called, which is what lets a rejected action like a wall bump cost
    /// nothing) and, when it did, how much energy to charge
    /// `TurnScheduler::advance`. Replaces the removed `trigger_turn` field
    /// — see `docs/ember2d-scripting-api.md`'s changelog for why `ctx.act`
    /// took over `ctx.trigger_turn`'s old job.
    pub act_cost:        Option<f64>,
    /// Entities `ctx.despawn()` queued this pass, so a caller that tracks
    /// per-entity state outside `World` (`PlayState::scheduler`) can clean
    /// up too — added alongside the turn scheduler (Step 5f), since a
    /// despawned actor left in `TurnScheduler`'s queue would otherwise
    /// cycle a dead turn slot forever (harmless, but a leak).
    pub despawned:       Vec<EntityId>,
}

/// Camera shake request/state — `ctx.shake_camera` queues one, PlayState reads
/// it back out through `ScriptUpdateResult`. Moved here from `play.rs` in
/// Step 5a (docs/ember2d-phase5-plan.md): this is what a script asked for, not
/// something `PlayState` itself defines the shape of — `play.rs` re-exports it
/// (`pub use crate::scripting::ShakeState;`) so existing call sites there are
/// unaffected.
#[derive(Clone, Copy)]
pub struct ShakeState {
    pub intensity: f32,
    pub duration:  f32,
}

// ── HudDraw ───────────────────────────────────────────────────────────────────

pub enum HudDraw {
    Text { x: usize, y: usize, text: String, fg: Color, bg: Color },
    Box  { x: usize, y: usize, w: usize, h: usize, fg: Color, bg: Color },
    Fill { x: usize, y: usize, w: usize, h: usize, ch: char, fg: Color, bg: Color },
    Menu { x: usize, y: usize, w: usize, options: Vec<String>, selected: usize, fg: Color, bg: Color, sel_fg: Color, sel_bg: Color },
    Panel { x: usize, y: usize, w: usize, h: usize, title: String, fg: Color, bg: Color },
}

// ── SpawnRequest ──────────────────────────────────────────────────────────────

pub(super) struct SpawnRequest {
    pub id:    EntityId,
    pub glyph: char,
    pub x:     f32,
    pub y:     f32,
    pub tag:   String,
    /// Defect D10: these used to be hardcoded (white, z=2, 1x1 non-solid
    /// trigger, no layer) in `apply_ctx` regardless of what the script asked
    /// for. `spawn_entity`'s default overload still fills in these exact
    /// same values, so existing scripts see no behavior change; the new
    /// extended overload lets a script set them at spawn time instead of
    /// having to `set_tint`/`set_layer_order`/etc. the id on some later frame.
    pub fg:    Color,
    pub bg:    Color,
    pub z:     i32,
    pub solid: bool,
    pub w:     f32,
    pub h:     f32,
    pub layer: String,
}

// ── ParticleRequest ───────────────────────────────────────────────────────────

pub struct ParticleRequest {
    pub x:     f32,
    pub y:     f32,
    pub glyph: char,
    pub fg:    Color,
}

// ── Color name ↔ Color enum ───────────────────────────────────────────────────

/// Step 3e: `parse_color` now also reads explicit `"#RRGGBB"` hex values —
/// `color_to_name` already emits that exact format for `Color::Rgb`, so a
/// value round-tripped out through `get_color`/etc. and back in through
/// `set_tint`/etc. survives unchanged. Purely additive: every existing
/// named-color script keeps working exactly as before.
pub fn parse_color(name: &str) -> Color {
    let trimmed = name.trim();
    if let Some(hex) = trimmed.strip_prefix('#') {
        if hex.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            ) {
                return Color::Rgb(r, g, b);
            }
        }
        eprintln!("[script] malformed hex color '{}', defaulting to Reset", trimmed);
        return Color::Reset;
    }

    match trimmed {
        "Black"       => Color::Black,
        "DarkRed"     => Color::DarkRed,
        "DarkGreen"   => Color::DarkGreen,
        "DarkYellow"  => Color::DarkYellow,
        "DarkBlue"    => Color::DarkBlue,
        "DarkMagenta" => Color::DarkMagenta,
        "DarkCyan"    => Color::DarkCyan,
        "Grey"|"Gray" => Color::Grey,
        "DarkGrey"|"DarkGray" => Color::DarkGrey,
        "Red"         => Color::Red,
        "Green"       => Color::Green,
        "Yellow"      => Color::Yellow,
        "Blue"        => Color::Blue,
        "Magenta"     => Color::Magenta,
        "Cyan"        => Color::Cyan,
        "White"       => Color::White,
        "Reset"       => Color::Reset,
        _ => {
            eprintln!("[script] unknown color '{}', defaulting to Reset", trimmed);
            Color::Reset
        }
    }
}

pub fn color_to_name(color: Color) -> String {
    match color {
        Color::Black       => "Black".to_string(),
        Color::DarkRed     => "DarkRed".to_string(),
        Color::DarkGreen   => "DarkGreen".to_string(),
        Color::DarkYellow  => "DarkYellow".to_string(),
        Color::DarkBlue    => "DarkBlue".to_string(),
        Color::DarkMagenta => "DarkMagenta".to_string(),
        Color::DarkCyan    => "DarkCyan".to_string(),
        Color::Grey        => "Grey".to_string(),
        Color::DarkGrey    => "DarkGrey".to_string(),
        Color::Red         => "Red".to_string(),
        Color::Green       => "Green".to_string(),
        Color::Yellow      => "Yellow".to_string(),
        Color::Blue        => "Blue".to_string(),
        Color::Magenta     => "Magenta".to_string(),
        Color::Cyan        => "Cyan".to_string(),
        Color::White       => "White".to_string(),
        Color::Reset       => "Reset".to_string(),
        Color::Rgb(r, g, b) => format!("#{:02X}{:02X}{:02X}", r, g, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Tests: Step 3e hex color support (ember2d-scripting-api.md §3) ─────────

    #[test]
    fn parse_color_still_reads_every_named_color_unchanged() {
        assert_eq!(parse_color("Red"), Color::Red);
        assert_eq!(parse_color("Reset"), Color::Reset);
        assert_eq!(parse_color("DarkGray"), Color::DarkGrey, "the Gray/Grey American-spelling alias must still work");
    }

    #[test]
    fn parse_color_reads_explicit_hex_values() {
        assert_eq!(parse_color("#FF8000"), Color::Rgb(0xFF, 0x80, 0x00));
        assert_eq!(parse_color("#00ff00"), Color::Rgb(0, 255, 0), "hex digits should be case-insensitive");
        assert_eq!(parse_color("  #112233  "), Color::Rgb(0x11, 0x22, 0x33), "surrounding whitespace must still be trimmed");
    }

    #[test]
    fn parse_color_round_trips_through_color_to_name() {
        let original = Color::Rgb(0x4A, 0x90, 0xE2);
        assert_eq!(parse_color(&color_to_name(original)), original, "a tint read back out via get_color/color_to_name and back in via set_tint must survive unchanged");
    }

    #[test]
    fn parse_color_falls_back_to_reset_on_malformed_hex() {
        assert_eq!(parse_color("#ZZZZZZ"), Color::Reset);
        assert_eq!(parse_color("#FFF"), Color::Reset, "only 6-digit RRGGBB is accepted, not the 3-digit shorthand");
    }
}
