// editor/ui/panels/inspector.rs — the Inspector panel: what it shows for a
// tile or the player, as a list of rows, and drawing that list.
//
// Step 9-6 (docs/ember2d-master-plan.md §5.8): moved out of `dock.rs` and
// rebuilt as a ROW LIST (`inspector_rows`) — a focused slice of the
// deferred 7E-2 "Inspector 2.0". It used to be a fixed layout of row
// numbers (`INSP_GLYPH_OFF`, `INSP_TAG_OFF`, ...) that simply stopped at
// the panel's bottom edge; the fields this step adds (colours, sprite,
// clip, the whole Actor section, the player's collider) don't fit that, so
// the rows are now data, the panel scrolls (`scroll`, the mouse wheel), and
// the input side asks `inspector_rows` for the same list to clamp its
// scroll — draw and input can't disagree about what a row is.
//
// Every clickable row still registers its own hit rect in `UiFrame` at the
// exact place it's drawn (Phase 7 Part 1d), so a row scrolled out of view
// has no live hitbox.

use super::super::frame::{InspectorField, UiFrame, WidgetId};
use super::super::rect::UiRect;
use super::super::widgets::draw_text_row;
use ember2d::renderer::{color::Color, Font, UiPainter};
use ember2d::theme::{PaletteRole, Theme};
use ember2d_sim::level::{PlayerRecord, TileRecord};
use ember2d_sim::math::Rect;
use ember2d_sim::scripting::color_to_name;

/// What the Inspector is showing.
#[derive(Clone, Copy)]
pub enum InspSubject<'a> {
    Tile(&'a TileRecord),
    Player(&'a PlayerRecord),
}

/// How a row draws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RowStyle {
    /// A value row: primary text on the input background.
    Value,
    /// An unset value (`(none)`): dim text on the input background.
    Unset,
    /// A value in the accent colour (layer, mask, exit).
    Accent,
    /// The glyph row, drawn in the subject's own foreground colour.
    Glyph(Color),
    /// A button: black text on this background.
    Button(Color),
}

/// One Inspector row.
#[derive(Debug, Clone, PartialEq)]
pub enum InspRow {
    /// A dashed separator.
    Sep,
    /// Plain dim text — a section label, or information.
    Label(String),
    /// A clickable row editing `field`.
    Field { text: String, field: InspectorField, style: RowStyle },
}

fn field(text: impl Into<String>, field: InspectorField, style: RowStyle) -> InspRow {
    InspRow::Field { text: text.into(), field, style }
}

/// `"  name  value"`, dim when the value is unset.
fn value_row(name: &str, value: Option<String>, f: InspectorField) -> InspRow {
    match value {
        Some(v) => field(format!("  {name}  {v}"), f, RowStyle::Value),
        None => field(format!("  {name}  (none)"), f, RowStyle::Unset),
    }
}

fn check(on: bool, label: &str, f: InspectorField) -> InspRow {
    field(format!(" [{}] {label}", if on { 'x' } else { ' ' }), f, RowStyle::Value)
}

/// A stat value as typed: `6`, not `6.0`.
pub fn format_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// A path shown by its file name only.
fn short_path(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

/// The rows the Inspector shows for `subject`, top to bottom.
pub fn inspector_rows(subject: &InspSubject) -> Vec<InspRow> {
    use InspectorField::*;
    let mut rows = Vec::new();
    match subject {
        InspSubject::Tile(t) => {
            rows.push(field(format!("  '{}' glyph", t.glyph), Glyph, RowStyle::Glyph(t.fg)));
            rows.push(value_row("fg", Some(color_to_name(t.fg)), Fg));
            rows.push(value_row("bg", Some(color_to_name(t.bg)), Bg));
            let sprite = t.sprite.as_ref().map(|s| format!("{}:{}", s.tileset, s.region));
            rows.push(value_row("sprite", sprite, Sprite));
            rows.push(value_row("clip", t.clip.clone(), Clip));
            rows.push(InspRow::Sep);
            rows.push(field(" Tag:", Tag, RowStyle::Unset));
            let tag = (!t.tag.is_empty()).then(|| t.tag.clone());
            rows.push(field(format!("  {}", tag.as_deref().unwrap_or("(none)")), Tag, {
                if tag.is_some() {
                    RowStyle::Value
                } else {
                    RowStyle::Unset
                }
            }));
            rows.push(InspRow::Sep);
            rows.push(check(t.solid, "Solid", Solid));
            rows.push(check(t.trigger, "Trigger", Trigger));
            rows.push(check(t.camera_follow, "Camera follow", CameraFollow));
            rows.push(InspRow::Sep);
            rows.push(value_row("script", t.script.as_deref().map(short_path), Script));
            match &t.next_level {
                Some(p) => rows.push(field(format!("  exit >{p}"), Exit, RowStyle::Accent)),
                None => rows.push(field("  (no exit)", Exit, RowStyle::Unset)),
            }
            rows.push(InspRow::Sep);
            match &t.graph {
                Some(g) => {
                    rows.push(field("  [Edit Graph]", GraphBtn, RowStyle::Button(Color::Reset)));
                    rows.push(InspRow::Label(format!(
                        "  {} nodes  {} edges",
                        g.nodes.len(),
                        g.edges.len()
                    )));
                }
                // A distinct "create" action, not decorative chrome — stays
                // literal green rather than a theme role.
                None => {
                    rows.push(field("  [New Graph]", GraphBtn, RowStyle::Button(Color::DarkGreen)))
                }
            }
            rows.push(InspRow::Sep);
            collider_rows(&mut rows, &t.collider_layer, &t.collider_mask);
            rows.push(InspRow::Sep);
            actor_rows(&mut rows, t);
        }
        InspSubject::Player(p) => {
            rows.push(field(format!("  '{}' glyph", p.glyph), Glyph, RowStyle::Glyph(p.fg)));
            rows.push(value_row("fg", Some(color_to_name(p.fg)), Fg));
            rows.push(value_row("bg", Some(color_to_name(p.bg)), Bg));
            rows.push(InspRow::Sep);
            rows.push(field(" Tag:", Tag, RowStyle::Unset));
            rows.push(field(format!("  {}", p.tag), Tag, RowStyle::Value));
            rows.push(InspRow::Sep);
            rows.push(check(p.solid, "Solid", Solid));
            rows.push(check(p.trigger, "Trigger", Trigger));
            rows.push(check(p.camera_follow, "Camera follow", CameraFollow));
            rows.push(InspRow::Sep);
            rows.push(value_row("script", p.script.as_deref().map(short_path), Script));
            rows.push(InspRow::Sep);
            let size = format!("{} x {}", format_number(p.collider_w as f64), {
                format_number(p.collider_h as f64)
            });
            rows.push(value_row("collider", Some(size), ColliderSize));
            collider_rows(&mut rows, &p.collider_layer, &p.collider_mask);
        }
    }
    rows
}

fn collider_rows(rows: &mut Vec<InspRow>, layer: &str, mask: &[String]) {
    let layer = if layer.is_empty() { "(any)".to_string() } else { layer.to_string() };
    rows.push(field(format!("  layer  {layer}"), InspectorField::Layer, RowStyle::Accent));
    let mask = if mask.is_empty() { "(all layers)".to_string() } else { mask.join(",") };
    rows.push(field(format!("  mask  {mask}"), InspectorField::Mask, RowStyle::Accent));
}

/// Step 9-6: the Actor section — what makes a tile take turns (7.5-3) and
/// carry its role's numbers (7.5-4). Authorable only in code before this.
fn actor_rows(rows: &mut Vec<InspRow>, t: &TileRecord) {
    use InspectorField::*;
    rows.push(check(t.actor.is_some(), "Actor (takes turns)", ActorToggle));
    let Some(a) = &t.actor else { return };
    rows.push(value_row("speed", Some(a.speed.to_string()), ActorSpeed));
    rows.push(check(a.physics, "physics (walls block it)", ActorPhysics));
    rows.push(value_row("tint aware", Some(color_to_name(a.tint_aware)), TintAware));
    rows.push(value_row("tint asleep", Some(color_to_name(a.tint_asleep)), TintAsleep));
    rows.push(InspRow::Label("  stats:".to_string()));
    for (i, (k, v)) in a.stats.iter().enumerate() {
        let i = u16::try_from(i).unwrap_or(u16::MAX);
        rows.push(field(format!("    {k} = {}", format_number(*v)), ActorStat(i), RowStyle::Value));
    }
    rows.push(field("    + add stat", ActorStatAdd, RowStyle::Unset));
}

/// How far the rows can scroll: everything past what fits under the two
/// header rows (the mode tag and the position).
pub fn max_inspector_scroll(row_count: usize, content_h: f32, row_h: f32) -> usize {
    let fits = ((content_h / row_h).floor() as usize).saturating_sub(2);
    row_count.saturating_sub(fits)
}

/// Draws the Inspector: the mode tag, the position, then `subject`'s rows
/// from `scroll` down, registering every visible clickable row in `frame`.
#[allow(clippy::too_many_arguments)]
pub fn draw_inspector(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    subject: Option<InspSubject>,
    pos: Option<(i32, i32)>,
    mode_tag: &str,
    content: Rect,
    frame: &mut UiFrame,
    scroll: usize,
) {
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let input_bg = theme.role_color(PaletteRole::InputBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let dim = theme.role_color(PaletteRole::TextDim);
    let accent = theme.role_color(PaletteRole::Accent);
    let row_h = theme.metrics.row_h;
    let text_px = theme.font_sizes.body;
    let max_rows = ((content.h / row_h).floor() as usize).max(1);
    let row_rect = |i: usize| Rect::new(content.x, content.y + i as f32 * row_h, content.w, row_h);

    painter.fill(content, panel_bg);
    draw_text_row(painter, font, mode_tag, row_rect(0), text_px, Color::Black, accent);

    let Some(subject) = subject else {
        let hint = if pos.is_some() { " (empty cell)" } else { " hover a tile" };
        draw_text_row(painter, font, hint, row_rect(2), text_px, dim, panel_bg);
        return;
    };
    if let Some((gx, gy)) = pos {
        let at = format!(" ({gx},{gy})");
        draw_text_row(painter, font, &at, row_rect(1), text_px, accent, panel_bg);
    }

    let sep: String =
        "-".repeat((content.w / painter.measure(font, "-", text_px).max(1.0)) as usize);
    let rows = inspector_rows(&subject);
    let scroll = scroll.min(max_inspector_scroll(rows.len(), content.h, row_h));
    let visible = max_rows.saturating_sub(2);
    let more_below = rows.len() > scroll + visible;
    // When rows continue below, the last visible row says so instead.
    let shown = if more_below { visible.saturating_sub(1) } else { visible };
    for (i, row) in rows.iter().skip(scroll).take(shown).enumerate() {
        let r = row_rect(2 + i);
        match row {
            InspRow::Sep => {
                draw_text_row(painter, font, &sep, r, text_px, dim, panel_bg);
            }
            InspRow::Label(t) => {
                draw_text_row(painter, font, t, r, text_px, dim, panel_bg);
            }
            InspRow::Field { text, field, style } => {
                let (fg, bg) = match *style {
                    RowStyle::Value => (text_fg, input_bg),
                    RowStyle::Unset => (dim, input_bg),
                    RowStyle::Accent => (accent, input_bg),
                    RowStyle::Glyph(c) => (c, input_bg),
                    RowStyle::Button(Color::Reset) => (Color::Black, accent),
                    RowStyle::Button(c) => (Color::Black, c),
                };
                draw_text_row(painter, font, text, r, text_px, fg, bg);
                frame.push(WidgetId::InspectorRow(*field), UiRect::new(r.x, r.y, r.w, r.h));
            }
        }
    }
    if more_below {
        let r = row_rect(2 + shown);
        draw_text_row(painter, font, "  v more (scroll)", r, text_px, dim, panel_bg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ember2d_sim::level::ActorRecord;

    fn fields(rows: &[InspRow]) -> Vec<InspectorField> {
        rows.iter()
            .filter_map(|r| match r {
                InspRow::Field { field, .. } => Some(*field),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_tile_without_an_actor_offers_to_make_it_one() {
        let t = TileRecord::new(1, 1, 1, '#', Color::Grey, Color::Reset, true, false, "wall");
        let f = fields(&inspector_rows(&InspSubject::Tile(&t)));
        for want in [
            InspectorField::Fg,
            InspectorField::Bg,
            InspectorField::Sprite,
            InspectorField::Clip,
            InspectorField::ActorToggle,
        ] {
            assert!(f.contains(&want), "{want:?} missing from {f:?}");
        }
        assert!(!f.contains(&InspectorField::ActorSpeed), "no actor, no actor fields");
    }

    #[test]
    fn an_actor_tile_lists_its_stats_in_key_order_and_an_add_row() {
        let mut t = TileRecord::new(1, 1, 1, 'r', Color::Red, Color::Reset, true, false, "enemy");
        let mut a = ActorRecord::default();
        a.stats.insert("hp".into(), 6.0);
        a.stats.insert("atk".into(), 1.5);
        t.actor = Some(a);
        let rows = inspector_rows(&InspSubject::Tile(&t));
        let texts: Vec<String> = rows
            .iter()
            .filter_map(|r| match r {
                InspRow::Field { text, field: InspectorField::ActorStat(_), .. } => {
                    Some(text.trim().to_string())
                }
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["atk = 1.5", "hp = 6"]);
        let f = fields(&rows);
        assert!(f.contains(&InspectorField::ActorStat(1)));
        assert!(f.contains(&InspectorField::ActorStatAdd));
        assert!(f.contains(&InspectorField::TintAsleep));
    }

    #[test]
    fn the_player_shows_its_collider_size_and_no_tile_only_fields() {
        let p = PlayerRecord::default();
        let f = fields(&inspector_rows(&InspSubject::Player(&p)));
        assert!(f.contains(&InspectorField::ColliderSize));
        assert!(f.contains(&InspectorField::Fg));
        assert!(!f.contains(&InspectorField::Exit) && !f.contains(&InspectorField::ActorToggle));
    }

    #[test]
    fn scrolling_stops_once_the_last_row_is_visible() {
        // 30 rows, a panel 12 rows tall: 10 fit under the 2 header rows.
        assert_eq!(max_inspector_scroll(30, 240.0, 20.0), 20);
        assert_eq!(max_inspector_scroll(5, 240.0, 20.0), 0);
    }
}
