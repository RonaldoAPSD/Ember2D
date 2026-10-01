// ui.rs — the state behind script menus and dialogue boxes: which are open,
// what's highlighted, which page is showing, and how the keyboard moves
// them.
//
// ── WHY (Step 9-3, docs/ember2d-master-plan.md §5.8) ─────────────────────────
//
// Before 9-3, `draw_menu(...)` only DREW a list with one row highlighted —
// every script that wanted a working menu kept its own selection index,
// read the arrow keys itself, and redrew the list every frame (the old
// built-in pause menu did exactly that). Dialogue had nothing at all.
//
// A menu or dialogue box is now engine-owned state, here, in the
// simulation: a script opens one (`menu_open`, `draw_dialogue`), the engine
// moves the highlight / turns the page / confirms / cancels from the step's
// buffered input BEFORE any script runs, and scripts poll the result
// (`menu_selection`, `menu_closed`, `dialogue_done`). The keys a widget uses
// are taken out of that step's input, so a player doesn't walk while a menu
// is open. Being simulation state, it's deterministic, replays, and saves.
//
// Drawing is presentation: `ember2d::play` draws whatever is open here
// with a real TTF font on the pixel path. Because the simulation can't
// measure a font, dialogue is wrapped and paged here by CHARACTER count —
// `wrap_text` — with a budget chosen so the bundled monospace UI font fits
// one character per cell (`ember2d/src/play/ui_draw.rs`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::command::InputSnapshot;

/// Lines of text one dialogue page shows.
pub const DIALOGUE_LINES: usize = 3;
/// Closed menus kept readable (`menu_selection`/`menu_closed`) until a
/// script forgets them with `menu_close`; past this many, the oldest go.
pub const MAX_CLOSED_MENUS: usize = 32;

/// Keys a widget reads.
const UP: [&str; 2] = ["up", "w"];
const DOWN: [&str; 2] = ["down", "s"];
const CONFIRM: [&str; 2] = ["enter", "space"];
const CANCEL: &str = "escape";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MenuState {
    Open,
    Confirmed,
    Cancelled,
}

/// One menu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Menu {
    pub items: Vec<String>,
    pub selected: usize,
    pub state: MenuState,
    pub title: String,
    /// Top-left cell; `None` centres it on that axis.
    pub x: Option<i64>,
    pub y: Option<i64>,
    /// Width in cells; `None` fits the longest item or title.
    pub width: Option<i64>,
    /// Escape cancels it.
    pub cancelable: bool,
    /// The entity whose script opened it — popping that entity's scene
    /// closes it (`close_owned_by`).
    #[serde(default)]
    pub owner: i64,
}

/// The dialogue box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dialogue {
    pub id: i64,
    pub speaker: String,
    /// The text as given — `draw_dialogue` with the same text and speaker
    /// while this is still showing does nothing.
    pub text: String,
    /// Wrapped lines, `DIALOGUE_LINES` per page.
    pub pages: Vec<Vec<String>>,
    pub page: usize,
    pub done: bool,
    /// The entity whose script opened it — see `Menu::owner`.
    #[serde(default)]
    pub owner: i64,
}

/// Every open (and recently closed) widget.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UiModel {
    pub menus: BTreeMap<i64, Menu>,
    pub dialogue: Option<Dialogue>,
    /// The next id `menu_open`/`draw_dialogue` hands out.
    pub next_id: i64,
}

/// A widget request a script queued this pass.
#[derive(Debug, Clone, PartialEq)]
pub enum UiOp {
    OpenMenu(i64, Menu),
    CloseMenu(i64),
    OpenDialogue(Dialogue),
    AdvanceDialogue,
    CloseDialogue,
}

impl UiModel {
    /// The open dialogue, if one is showing.
    pub fn open_dialogue(&self) -> Option<&Dialogue> {
        self.dialogue.as_ref().filter(|d| !d.done)
    }

    /// The menu that has the keyboard: the newest open one.
    pub fn active_menu(&self) -> Option<(i64, &Menu)> {
        self.menus.iter().rev().find(|(_, m)| m.state == MenuState::Open).map(|(&id, m)| (id, m))
    }

    /// Open menus, oldest first (the drawing order).
    pub fn open_menus(&self) -> impl Iterator<Item = (i64, &Menu)> {
        self.menus.iter().filter(|(_, m)| m.state == MenuState::Open).map(|(&id, m)| (id, m))
    }

    /// The highlighted row of an open menu, the chosen row of a confirmed
    /// one, `-1` for a cancelled or unknown one.
    pub fn menu_selection(&self, id: i64) -> i64 {
        match self.menus.get(&id) {
            Some(m) if m.state != MenuState::Cancelled => m.selected as i64,
            _ => -1,
        }
    }

    /// True once confirmed or cancelled (and for an unknown id).
    pub fn menu_closed(&self, id: i64) -> bool {
        self.menus.get(&id).map(|m| m.state != MenuState::Open).unwrap_or(true)
    }

    /// True once dialogue `id` was read to the end, closed, or replaced.
    pub fn dialogue_done(&self, id: i64) -> bool {
        match &self.dialogue {
            Some(d) if d.id == id => d.done,
            _ => true,
        }
    }

    /// Closes every menu and the dialogue that entity `owner`'s script
    /// opened — a popped scene's widgets go with it.
    pub fn close_owned_by(&mut self, owner: i64) {
        self.menus.retain(|_, m| m.owner != owner);
        if let Some(d) = self.dialogue.as_mut().filter(|d| d.owner == owner) {
            d.done = true;
        }
    }

    fn advance_dialogue(&mut self) {
        if let Some(d) = self.dialogue.as_mut().filter(|d| !d.done) {
            if d.page + 1 < d.pages.len() {
                d.page += 1;
            } else {
                d.done = true;
            }
        }
    }

    /// Applies one pass's requests, in order.
    pub fn apply(&mut self, ops: Vec<UiOp>) {
        for op in ops {
            match op {
                UiOp::OpenMenu(id, menu) => {
                    self.menus.insert(id, menu);
                    self.next_id = self.next_id.max(id + 1);
                }
                UiOp::CloseMenu(id) => {
                    self.menus.remove(&id);
                }
                UiOp::OpenDialogue(d) => {
                    self.next_id = self.next_id.max(d.id + 1);
                    self.dialogue = Some(d);
                }
                UiOp::AdvanceDialogue => self.advance_dialogue(),
                UiOp::CloseDialogue => {
                    if let Some(d) = self.dialogue.as_mut() {
                        d.done = true;
                    }
                }
            }
        }
        self.trim_closed();
    }

    fn trim_closed(&mut self) {
        let closed: Vec<i64> = self
            .menus
            .iter()
            .filter(|(_, m)| m.state != MenuState::Open)
            .map(|(&id, _)| id)
            .collect();
        for id in closed.iter().take(closed.len().saturating_sub(MAX_CLOSED_MENUS)) {
            self.menus.remove(id);
        }
    }

    /// The keyboard's turn at the open widgets, at the start of a step: the
    /// dialogue box first (it's modal), else the newest open menu. Returns
    /// the step's input with the keys a widget used removed (pressed and
    /// held), or `None` when no widget is open and nothing was taken.
    pub fn handle_input(&mut self, input: &InputSnapshot) -> Option<InputSnapshot> {
        let pressed = |keys: &[&str]| keys.iter().any(|k| input.pressed.contains(*k));
        let consumed: &[&str] = if self.open_dialogue().is_some() {
            if pressed(&CONFIRM) {
                self.advance_dialogue();
            }
            &["enter", "space", "up", "w", "down", "s", "escape"]
        } else if let Some((id, menu)) = self.active_menu() {
            let n = menu.items.len().max(1);
            let mut m = menu.clone();
            if pressed(&UP) {
                m.selected = (m.selected + n - 1) % n;
            }
            if pressed(&DOWN) {
                m.selected = (m.selected + 1) % n;
            }
            if pressed(&CONFIRM) && !m.items.is_empty() {
                m.state = MenuState::Confirmed;
            } else if pressed(&[CANCEL]) && m.cancelable {
                m.state = MenuState::Cancelled;
            }
            self.menus.insert(id, m);
            &["enter", "space", "up", "w", "down", "s", "escape"]
        } else {
            return None;
        };
        let mut filtered = input.clone();
        for k in consumed {
            filtered.pressed.remove(*k);
            filtered.held.remove(*k);
        }
        Some(filtered)
    }
}

/// `text` word-wrapped to at most `width` characters per line: breaks at
/// spaces, honours `\n`, and hard-breaks a single word longer than a line
/// (so every line really fits). Always at least one line.
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ').filter(|w| !w.is_empty()) {
            let mut word: Vec<char> = word.chars().collect();
            while word.len() > width {
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                lines.push(word.drain(..width).collect());
            }
            let word: String = word.into_iter().collect();
            let fits = line.chars().count() + usize::from(!line.is_empty()) + word.chars().count();
            if !line.is_empty() && fits > width {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(&word);
        }
        lines.push(line);
    }
    lines
}

/// `text` wrapped to `width` and cut into pages of `DIALOGUE_LINES`.
pub fn paginate(text: &str, width: usize) -> Vec<Vec<String>> {
    let lines = wrap_text(text, width);
    lines.chunks(DIALOGUE_LINES).map(|c| c.to_vec()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn keys(k: &[&str]) -> InputSnapshot {
        let s: BTreeSet<String> = k.iter().map(|x| x.to_string()).collect();
        InputSnapshot { held: s.clone(), pressed: s }
    }

    fn menu(items: &[&str]) -> Menu {
        Menu {
            items: items.iter().map(|s| s.to_string()).collect(),
            selected: 0,
            state: MenuState::Open,
            title: String::new(),
            x: None,
            y: None,
            width: None,
            cancelable: true,
            owner: 0,
        }
    }

    #[test]
    fn a_popped_scenes_widgets_close_with_it() {
        let mut ui = UiModel::default();
        let mut mine = menu(&["a"]);
        mine.owner = 7;
        ui.apply(vec![UiOp::OpenMenu(0, menu(&["b"])), UiOp::OpenMenu(1, mine)]);
        ui.close_owned_by(7);
        assert!(ui.menus.contains_key(&0) && !ui.menus.contains_key(&1));
    }

    #[test]
    fn wrap_breaks_at_spaces_honours_newlines_and_splits_long_words() {
        assert_eq!(wrap_text("the quick brown fox", 9), vec!["the quick", "brown fox"]);
        assert_eq!(wrap_text("a\nb", 10), vec!["a", "b"]);
        assert_eq!(wrap_text("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap_text("", 5), vec![""]);
        for line in wrap_text("lorem ipsum dolor sit amet, consectetur", 7) {
            assert!(line.chars().count() <= 7, "{line}");
        }
    }

    #[test]
    fn pages_hold_three_lines() {
        let p = paginate("one two three four five six seven", 5);
        assert_eq!(p.len(), 3);
        assert!(p.iter().all(|page| page.len() <= DIALOGUE_LINES));
    }

    #[test]
    fn arrows_move_the_newest_menu_and_wrap_around_and_the_keys_are_taken() {
        let mut ui = UiModel::default();
        ui.apply(vec![
            UiOp::OpenMenu(0, menu(&["a", "b"])),
            UiOp::OpenMenu(1, menu(&["x", "y", "z"])),
        ]);
        let left = ui.handle_input(&keys(&["up", "d"])).expect("a menu took input");
        assert_eq!(ui.menu_selection(1), 2, "up from the top wraps to the bottom");
        assert_eq!(ui.menu_selection(0), 0, "only the newest menu moves");
        assert!(!left.pressed.contains("up") && left.pressed.contains("d"));
    }

    #[test]
    fn confirm_and_cancel_close_a_menu() {
        let mut ui = UiModel::default();
        ui.apply(vec![UiOp::OpenMenu(5, menu(&["a", "b"]))]);
        ui.handle_input(&keys(&["down"]));
        ui.handle_input(&keys(&["enter"]));
        assert!(ui.menu_closed(5));
        assert_eq!(ui.menu_selection(5), 1);

        let mut ui = UiModel::default();
        ui.apply(vec![UiOp::OpenMenu(5, menu(&["a"]))]);
        ui.handle_input(&keys(&["escape"]));
        assert!(ui.menu_closed(5));
        assert_eq!(ui.menu_selection(5), -1, "cancelled");

        let mut m = menu(&["a"]);
        m.cancelable = false;
        let mut ui = UiModel::default();
        ui.apply(vec![UiOp::OpenMenu(5, m)]);
        ui.handle_input(&keys(&["escape"]));
        assert!(!ui.menu_closed(5), "a menu that can't be cancelled ignores Escape");
    }

    #[test]
    fn dialogue_pages_turn_on_confirm_and_it_closes_after_the_last() {
        let mut ui = UiModel::default();
        let d = Dialogue {
            id: 3,
            speaker: "Old man".into(),
            text: "t".into(),
            pages: vec![vec!["one".into()], vec!["two".into()]],
            page: 0,
            done: false,
            owner: 0,
        };
        ui.apply(vec![UiOp::OpenDialogue(d), UiOp::OpenMenu(4, menu(&["a"]))]);
        ui.handle_input(&keys(&["enter"]));
        assert_eq!(ui.open_dialogue().map(|d| d.page), Some(1));
        assert!(!ui.menu_closed(4), "the dialogue had the keyboard, not the menu");
        ui.handle_input(&keys(&["space"]));
        assert!(ui.dialogue_done(3));
        assert!(ui.open_dialogue().is_none());
    }

    #[test]
    fn no_widget_leaves_input_alone() {
        let mut ui = UiModel::default();
        assert!(ui.handle_input(&keys(&["enter"])).is_none());
    }

    #[test]
    fn only_the_newest_closed_menus_are_kept() {
        let mut ui = UiModel::default();
        for id in 0..(MAX_CLOSED_MENUS as i64 + 5) {
            let mut m = menu(&["a"]);
            m.state = MenuState::Confirmed;
            ui.apply(vec![UiOp::OpenMenu(id, m)]);
        }
        assert_eq!(ui.menus.len(), MAX_CLOSED_MENUS);
        assert!(!ui.menus.contains_key(&0));
        assert!(ui.menu_closed(0), "a forgotten menu still reads as closed");
    }
}
