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
    /// Visual events `ctx.animate_move`/`animate_flash`/`animate_shake`
    /// queued this pass (Phase 5.5 Part 3, docs/ember2d-phase5.5-plan.md).
    /// `ember2d::play::PlayState` is the only consumer — grid state has
    /// already resolved by the time this is emitted (a script calls
    /// `ctx.set_position` for the real move same as always); this is purely
    /// "here's what to show while the player catches up," played back over
    /// real frames, never fed back into simulation state.
    pub animations:      Vec<AnimationEvent>,
}

/// One visual event a resolved action emitted. Durations are REAL SECONDS,
/// with no relationship to simulation time — a 100-cost turn may animate
/// for 0.1s or 1.0s with identical game consequences, since the state this
/// describes has already resolved in the sim. Keeping durations separate
/// from simulation cost is what would let a future "fast-forward
/// animations" setting exist without touching balance (see
/// `docs/ember2d-scripting-api.md` §7's existing note on this for the
/// turn-cost/animation-duration split).
#[derive(Debug, Clone)]
pub enum AnimationEvent {
    Move  { entity: EntityId, from: crate::math::Vec2, to: crate::math::Vec2, duration: f32 },
    Flash { entity: EntityId, color: Color, duration: f32 },
    Shake { entity: EntityId, duration: f32 },
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
///
/// `None` on anything unrecognized — the fallible half of `parse_color`
/// below, and what lets `set_tint` (api.rs) tell "genuinely asked for
/// Reset" apart from "gave me garbage."
///
/// R4 (7A-1): `hex.len() == 6` used to run first and slice `hex[0..2]` etc.
/// by byte index — a `#` followed by a multi-byte-per-character non-ASCII
/// string (e.g. two 3-byte characters) can total exactly 6 *bytes* while
/// having no byte boundary at index 2/4, which panics ("byte index N is not
/// a char boundary"). `hex.is_ascii()` first rules that out: every ASCII
/// byte is one character, so a subsequent byte-index slice can never land
/// mid-character.
pub(super) fn try_parse_color(name: &str) -> Option<Color> {
    let trimmed = name.trim();
    if let Some(hex) = trimmed.strip_prefix('#') {
        if hex.is_ascii() && hex.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            ) {
                return Some(Color::Rgb(r, g, b));
            }
        }
        return None;
    }

    match trimmed {
        "Black"       => Some(Color::Black),
        "DarkRed"     => Some(Color::DarkRed),
        "DarkGreen"   => Some(Color::DarkGreen),
        "DarkYellow"  => Some(Color::DarkYellow),
        "DarkBlue"    => Some(Color::DarkBlue),
        "DarkMagenta" => Some(Color::DarkMagenta),
        "DarkCyan"    => Some(Color::DarkCyan),
        "Grey"|"Gray" => Some(Color::Grey),
        "DarkGrey"|"DarkGray" => Some(Color::DarkGrey),
        "Red"         => Some(Color::Red),
        "Green"       => Some(Color::Green),
        "Yellow"      => Some(Color::Yellow),
        "Blue"        => Some(Color::Blue),
        "Magenta"     => Some(Color::Magenta),
        "Cyan"        => Some(Color::Cyan),
        "White"       => Some(Color::White),
        "Reset"       => Some(Color::Reset),
        _ => None,
    }
}

/// Infallible wrapper over `try_parse_color` for every call site that
/// doesn't need to distinguish "asked for Reset" from "gave me garbage" —
/// draws and spawns, where there's no previous value a no-op would need to
/// preserve (unlike `set_tint`, which uses `try_parse_color` directly).
/// R4 (7A-1): previously `eprintln!`'d on a bad value — forbidden in this
/// crate (CLAUDE.md's Determinism section) and, worse, unbounded (a script
/// re-drawing a bad color every frame would flood stderr forever). Callers
/// that can usefully report the bad string once (`set_tint`) do so through
/// `ScriptState::log_bad_color_once` instead; the rest just get the same
/// silent `Reset` fallback as always.
pub fn parse_color(name: &str) -> Color {
    try_parse_color(name).unwrap_or(Color::Reset)
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

    // ── Test: R4 (7A-1, docs/ember2d-master-plan.md §5.1) ──────────────────────

    #[test]
    fn parse_color_does_not_panic_on_a_non_ascii_hex_string_of_the_right_byte_length() {
        // "€" is 3 bytes in UTF-8, so "€€" is 6 bytes (matching the old
        // `hex.len() == 6` check) but only 2 characters — `&hex[0..2]` used
        // to land mid-character and panic ("byte index is not a char
        // boundary"). `hex.is_ascii()` rules this out before any slicing.
        assert_eq!(parse_color("#€€"), Color::Reset);
        assert_eq!(try_parse_color("#€€"), None);
    }
}
