// editor/input/inspector_edit.rs — the Inspector fields Step 9-6 added:
// colours, sprite and clip on a tile; the Actor section; the player's
// colours and collider size.
//
// Step 9-6 (docs/ember2d-master-plan.md §5.8). Before it, an enemy's
// `ActorRecord` (stats, tints, speed), a placed tile's own colours and the
// player's collider could only be set by a Rust level generator — the
// editor showed nothing and kept whatever the file had. Every field here
// goes through ONE prompt purpose (`TextInputPurpose::Inspector { target,
// field }`) and one commit function (`commit_inspector_edit`), instead of
// a dozen more single-use variants like `TileTag`/`PlayerTag`. Every
// change is one undo step: a tile through `Command::Batch` (the same
// before/after cell record painting uses), the player through
// `Command::UpdatePlayer`.

use super::super::commands::Command;
use super::super::ui::InspectorField;
use super::super::{EditorMode, EditorState, InspTarget, TextInputPurpose};
use ember2d::renderer::color::Color;
use ember2d_sim::level::{ActorRecord, PlayerRecord, TileRecord};
use ember2d_sim::scripting::{color_to_name, parse_color, LogEntry};
use ember2d_sim::tileset::SpriteRef;

/// What the prompt says for each field.
pub(in crate::editor) fn inspector_prompt_label(field: InspectorField) -> &'static str {
    use InspectorField::*;
    match field {
        Fg => "Foreground colour (a name like Red, or #RRGGBB)",
        Bg => "Background colour (a name, #RRGGBB, or Reset)",
        Sprite => "Sprite as tileset:region (empty clears it)",
        Clip => "Animation clip name (empty clears it)",
        ColliderSize => "Collider size as width,height (e.g. 0.75,0.75)",
        ActorSpeed => "Actor speed (100 = normal; higher acts more often)",
        TintAware => "Tint once aware (a colour, or Reset for none)",
        TintAsleep => "Tint while asleep (a colour, or Reset for none)",
        ActorStat(_) => "Stat as name=value (empty value removes it)",
        ActorStatAdd => "New stat as name=value (e.g. hp=6)",
        _ => "Value",
    }
}

/// A typed colour: a name `parse_color` knows, or `#RRGGBB`. `None` for
/// anything it doesn't recognise (`parse_color` itself would quietly turn
/// that into `Reset`).
pub(in crate::editor) fn parse_colour_input(text: &str) -> Option<Color> {
    let t = text.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("reset") || t.eq_ignore_ascii_case("none") {
        return Some(Color::Reset);
    }
    let c = parse_color(t);
    (c != Color::Reset).then_some(c)
}

/// `name=value`, `name = value` or `name value`. `Some((name, None))`
/// means "remove": a name with no value.
pub(in crate::editor) fn parse_stat_input(text: &str) -> Option<(String, Option<f64>)> {
    let t = text.trim();
    let (name, value) = match t.split_once('=') {
        Some((n, v)) => (n.trim(), v.trim()),
        None => match t.split_once(char::is_whitespace) {
            Some((n, v)) => (n.trim(), v.trim()),
            None => (t, ""),
        },
    };
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    if value.is_empty() {
        return Some((name.to_string(), None));
    }
    value.parse::<f64>().ok().filter(|v| v.is_finite()).map(|v| (name.to_string(), Some(v)))
}

/// `w,h`, `w x h` or `w h`, both positive.
pub(in crate::editor) fn parse_size_input(text: &str) -> Option<(f32, f32)> {
    let parts: Vec<&str> =
        text.split([',', 'x', 'X', ' ']).map(str::trim).filter(|s| !s.is_empty()).collect();
    let [w, h] = parts.as_slice() else { return None };
    let (w, h) = (w.parse::<f32>().ok()?, h.parse::<f32>().ok()?);
    (w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0).then_some((w, h))
}

/// `tileset:region` (or `tileset/region`).
pub(in crate::editor) fn parse_sprite_input(text: &str) -> Option<SpriteRef> {
    let (set, region) = text.trim().split_once([':', '/'])?;
    let (set, region) = (set.trim(), region.trim());
    (!set.is_empty() && !region.is_empty()).then(|| SpriteRef::new(set, region))
}

fn nth_stat(actor: &ActorRecord, i: u16) -> Option<(String, f64)> {
    actor.stats.iter().nth(i as usize).map(|(k, v)| (k.clone(), *v))
}

impl EditorState {
    /// A click on one of Step 9-6's rows: toggles flip at once, everything
    /// else opens its prompt pre-filled with the current value. `false`
    /// for a field this doesn't handle (the older ones, handled where they
    /// always were).
    pub(in crate::editor) fn inspector_click_new_field(
        &mut self,
        target: InspTarget,
        field: InspectorField,
    ) -> bool {
        use InspectorField::*;
        match (target, field) {
            (InspTarget::Tile { gx, gy }, ActorToggle) => {
                self.edit_tile(gx, gy, |t| {
                    t.actor = if t.actor.is_some() { None } else { Some(ActorRecord::default()) };
                });
                true
            }
            (InspTarget::Tile { gx, gy }, ActorPhysics) => {
                self.edit_tile(gx, gy, |t| {
                    if let Some(a) = t.actor.as_mut() {
                        a.physics = !a.physics;
                    }
                });
                true
            }
            (
                _,
                Fg | Bg | Sprite | Clip | ColliderSize | ActorSpeed | TintAware | TintAsleep
                | ActorStat(_) | ActorStatAdd,
            ) => {
                let Some(current) = self.inspector_prefill(target, field) else { return true };
                self.prompt_buffer = current;
                self.mode = EditorMode::Prompt(TextInputPurpose::Inspector { target, field });
                true
            }
            _ => false,
        }
    }

    /// The prompt's starting text — the field's current value. `None` if
    /// the target is gone (or the field doesn't apply to it).
    fn inspector_prefill(&self, target: InspTarget, field: InspectorField) -> Option<String> {
        use InspectorField::*;
        match target {
            InspTarget::Player => {
                let p = &self.grid.player;
                Some(match field {
                    Fg => color_to_name(p.fg),
                    Bg => color_to_name(p.bg),
                    ColliderSize => format!("{},{}", p.collider_w, p.collider_h),
                    _ => return None,
                })
            }
            InspTarget::Tile { gx, gy } => {
                let t = self.grid.get(gx, gy, self.active_layer)?;
                let actor = t.actor.as_ref();
                Some(match field {
                    Fg => color_to_name(t.fg),
                    Bg => color_to_name(t.bg),
                    Sprite => {
                        t.sprite.as_ref().map(|s| format!("{}:{}", s.tileset, s.region))?
                            .to_string()
                    }
                    Clip => t.clip.clone().unwrap_or_default(),
                    ActorSpeed => actor?.speed.to_string(),
                    TintAware => color_to_name(actor?.tint_aware),
                    TintAsleep => color_to_name(actor?.tint_asleep),
                    ActorStat(i) => {
                        let (k, v) = nth_stat(actor?, i)?;
                        format!("{k}={}", super::super::ui::format_number(v))
                    }
                    ActorStatAdd => String::new(),
                    _ => return None,
                })
            }
        }
        .or_else(|| (field == Sprite).then(String::new))
    }

    /// Applies a typed value. A value that doesn't parse changes nothing
    /// and says why in the console.
    pub(in crate::editor) fn commit_inspector_edit(
        &mut self,
        target: InspTarget,
        field: InspectorField,
        buffer: String,
    ) {
        use InspectorField::*;
        let bad = |s: &mut Self, what: &str| {
            s.console_log.push(LogEntry::warn(format!("Inspector: '{buffer}' isn't {what}")));
        };
        match (target, field) {
            (InspTarget::Player, Fg | Bg) => match parse_colour_input(&buffer) {
                Some(c) => self.edit_player(|p| if field == Fg { p.fg = c } else { p.bg = c }),
                None => bad(self, "a colour"),
            },
            (InspTarget::Player, ColliderSize) => match parse_size_input(&buffer) {
                Some((w, h)) => self.edit_player(|p| {
                    p.collider_w = w;
                    p.collider_h = h;
                }),
                None => bad(self, "a size like 0.75,0.75"),
            },
            (InspTarget::Tile { gx, gy }, Fg | Bg | TintAware | TintAsleep) => {
                match parse_colour_input(&buffer) {
                    Some(c) => self.edit_tile(gx, gy, |t| match field {
                        Fg => t.fg = c,
                        Bg => t.bg = c,
                        TintAware => t.actor.iter_mut().for_each(|a| a.tint_aware = c),
                        _ => t.actor.iter_mut().for_each(|a| a.tint_asleep = c),
                    }),
                    None => bad(self, "a colour"),
                }
            }
            (InspTarget::Tile { gx, gy }, Sprite) => {
                let sprite = if buffer.trim().is_empty() {
                    None
                } else {
                    match parse_sprite_input(&buffer) {
                        Some(s) => Some(s),
                        None => return bad(self, "tileset:region"),
                    }
                };
                if let Some(s) = &sprite {
                    if self.sprites.resolve(s).is_none() {
                        self.console_log.push(LogEntry::warn(format!(
                            "Inspector: no region '{}' in tileset '{}' yet — the tile keeps its glyph until there is",
                            s.region, s.tileset
                        )));
                    }
                }
                self.edit_tile(gx, gy, |t| t.sprite = sprite);
            }
            (InspTarget::Tile { gx, gy }, Clip) => {
                let clip = buffer.trim();
                let clip = (!clip.is_empty()).then(|| clip.to_string());
                self.edit_tile(gx, gy, |t| t.clip = clip);
            }
            (InspTarget::Tile { gx, gy }, ActorSpeed) => match buffer.trim().parse::<u32>() {
                Ok(v) if v > 0 => {
                    self.edit_tile(gx, gy, |t| t.actor.iter_mut().for_each(|a| a.speed = v))
                }
                _ => bad(self, "a whole number above 0"),
            },
            (InspTarget::Tile { gx, gy }, ActorStat(_) | ActorStatAdd) => {
                let Some((name, value)) = parse_stat_input(&buffer) else {
                    return bad(self, "name=value");
                };
                // Editing stat `i` may rename it: drop the old key first.
                let old = match field {
                    ActorStat(i) => self
                        .grid
                        .get(gx, gy, self.active_layer)
                        .and_then(|t| t.actor.as_ref())
                        .and_then(|a| nth_stat(a, i))
                        .map(|(k, _)| k),
                    _ => None,
                };
                self.edit_tile(gx, gy, |t| {
                    if let Some(a) = t.actor.as_mut() {
                        if let Some(old) = &old {
                            a.stats.remove(old);
                        }
                        match value {
                            Some(v) => {
                                a.stats.insert(name.clone(), v);
                            }
                            None => {
                                a.stats.remove(&name);
                            }
                        }
                    }
                });
            }
            _ => {}
        }
    }

    /// One undoable change to the tile at `(gx, gy)` on the active layer.
    /// A change that changes nothing records nothing.
    fn edit_tile(&mut self, gx: i32, gy: i32, change: impl FnOnce(&mut TileRecord)) {
        let layer = self.active_layer;
        let Some(before) = self.grid.get(gx, gy, layer).cloned() else { return };
        let mut after = before.clone();
        change(&mut after);
        if format!("{before:?}") == format!("{after:?}") {
            return;
        }
        self.undo.push(Command::Batch {
            cells: vec![(gx, gy, layer, Some(before), Some(after.clone()))],
        });
        self.grid.place(gx, gy, layer, after);
        self.unsaved = true;
    }

    /// One undoable change to the player record.
    fn edit_player(&mut self, change: impl FnOnce(&mut PlayerRecord)) {
        let before = self.grid.player.clone();
        let mut after = before.clone();
        change(&mut after);
        self.undo.push(Command::UpdatePlayer { before, after: after.clone() });
        self.grid.player = after;
        self.unsaved = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_take_names_hex_and_reset_but_not_nonsense() {
        assert_eq!(parse_colour_input("Red"), Some(Color::Red));
        assert_eq!(parse_colour_input("#ff8800"), Some(Color::Rgb(255, 136, 0)));
        assert_eq!(parse_colour_input("reset"), Some(Color::Reset));
        assert_eq!(parse_colour_input(""), Some(Color::Reset));
        assert_eq!(parse_colour_input("blurple"), None);
    }

    #[test]
    fn stats_take_several_spellings_and_an_empty_value_removes() {
        assert_eq!(parse_stat_input("hp=6"), Some(("hp".into(), Some(6.0))));
        assert_eq!(parse_stat_input(" atk = 1.5 "), Some(("atk".into(), Some(1.5))));
        assert_eq!(parse_stat_input("range 4"), Some(("range".into(), Some(4.0))));
        assert_eq!(parse_stat_input("hp="), Some(("hp".into(), None)));
        assert_eq!(parse_stat_input("hp=lots"), None);
        assert_eq!(parse_stat_input("bad name=1"), None);
        assert_eq!(parse_stat_input("=3"), None);
    }

    #[test]
    fn sizes_and_sprites_parse_their_forms() {
        assert_eq!(parse_size_input("0.7,0.7"), Some((0.7, 0.7)));
        assert_eq!(parse_size_input("1 x 2"), Some((1.0, 2.0)));
        assert_eq!(parse_size_input("0,1"), None);
        assert_eq!(parse_size_input("1"), None);
        assert_eq!(parse_sprite_input("dungeon:brick"), Some(SpriteRef::new("dungeon", "brick")));
        assert_eq!(parse_sprite_input("town/roof"), Some(SpriteRef::new("town", "roof")));
        assert_eq!(parse_sprite_input("nocolon"), None);
    }
}
