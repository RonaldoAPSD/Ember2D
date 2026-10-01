// editor/clip_editor.rs — the animation clip editor's in-progress state:
// which clip is being built, its frames (tileset region names), frame rate,
// looping, the selected (scrubbed) frame, and the live preview's clock.
//
// Step 8-3 (docs/ember2d-master-plan.md §5.7). Same split as the 8-2
// importer: this file is the pure part (unit-tested on its own);
// `ui/panels/clip_editor_panel.rs` draws it, `input/clip_editor.rs` drives
// it, `impl_state/clip_edit.rs` writes `assets/clips/<name>.ron` and adds
// palette entries. Lives on `EditorState::clip_editor` while
// `EditorMode::ClipEditor` is active.

use ember2d_sim::clip_asset::{ClipData, MAX_FPS};

/// Which text field has keyboard focus. `Hash` for `WidgetId` payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClipField {
    Name,
    Fps,
}

pub struct ClipEditor {
    pub name: String,
    /// The tileset frames are picked from — `None` only when the project
    /// has no tilesets at all (the dialog then says so).
    pub tileset: Option<String>,
    /// Region names, in playback order.
    pub frames: Vec<String>,
    /// Typed text; `to_clip` parses it.
    pub fps: String,
    pub looping: bool,
    /// The frame the strip has selected — what the preview shows while
    /// paused (scrubbing), and what move/delete act on.
    pub selected: Option<usize>,
    pub playing: bool,
    /// Seconds of preview playback, advanced by `tick` while `playing`.
    pub preview_t: f32,
    pub focus: Option<ClipField>,
    /// The last failed save's reason, or a confirmation after a good one.
    pub status: Option<(bool, String)>,
}

impl ClipEditor {
    /// A new, empty clip drawing from `tileset`.
    pub fn new(tileset: Option<String>) -> Self {
        ClipEditor {
            name: "new_clip".to_string(),
            tileset,
            frames: Vec::new(),
            fps: "8".to_string(),
            looping: true,
            selected: None,
            playing: true,
            preview_t: 0.0,
            focus: None,
            status: None,
        }
    }

    /// Open an existing clip for editing.
    pub fn open(clip: &ClipData) -> Self {
        ClipEditor {
            name: clip.name.clone(),
            tileset: Some(clip.tileset.clone()),
            frames: clip.frames.clone(),
            // `{}` prints 8.0 as "8" — keep whole-number rates tidy.
            fps: format!("{}", clip.fps),
            looping: clip.looping,
            selected: None,
            playing: true,
            preview_t: 0.0,
            focus: None,
            status: None,
        }
    }

    /// The typed frame rate, if it's a usable one.
    pub fn fps_value(&self) -> Option<f32> {
        self.fps.trim().parse::<f32>().ok().filter(|f| f.is_finite() && *f > 0.0 && *f <= MAX_FPS)
    }

    /// Advance the preview clock (only while playing).
    pub fn tick(&mut self, dt: f32) {
        if self.playing {
            // Wrapped so a dialog left open for hours never loses precision.
            self.preview_t = (self.preview_t + dt.max(0.0)) % 3600.0;
        }
    }

    /// The frame the preview shows right now: the playing frame, or —
    /// paused — the selected one (the scrubber), or the first.
    pub fn preview_frame(&self) -> Option<usize> {
        if self.frames.is_empty() {
            return None;
        }
        if !self.playing {
            return Some(self.selected.unwrap_or(0).min(self.frames.len() - 1));
        }
        let fps = self.fps_value().unwrap_or(1.0);
        let i = (self.preview_t * fps).floor() as usize;
        Some(if self.looping { i % self.frames.len() } else { i.min(self.frames.len() - 1) })
    }

    pub fn toggle_play(&mut self) {
        self.playing = !self.playing;
        if self.playing {
            self.preview_t = 0.0;
        }
    }

    /// Append `region` as a frame and select it.
    pub fn add_frame(&mut self, region: &str) {
        self.frames.push(region.to_string());
        self.selected = Some(self.frames.len() - 1);
    }

    /// Select frame `i` and pause on it — clicking the strip scrubs.
    pub fn select(&mut self, i: usize) {
        if i < self.frames.len() {
            self.selected = Some(i);
            self.playing = false;
        }
    }

    /// Step the selection left/right (wrapping), pausing on it.
    pub fn step(&mut self, delta: i32) {
        if self.frames.is_empty() {
            return;
        }
        let n = self.frames.len() as i32;
        let cur = self.selected.map(|s| s as i32).unwrap_or(0);
        self.select(((cur + delta) % n + n) as usize % n as usize);
    }

    /// Move the selected frame one place earlier (`-1`) or later (`+1`).
    pub fn move_selected(&mut self, delta: i32) {
        let Some(i) = self.selected else { return };
        let j = i as i32 + delta;
        if j >= 0 && (j as usize) < self.frames.len() {
            self.frames.swap(i, j as usize);
            self.selected = Some(j as usize);
        }
    }

    pub fn remove_selected(&mut self) {
        let Some(i) = self.selected else { return };
        if i < self.frames.len() {
            self.frames.remove(i);
            self.selected =
                if self.frames.is_empty() { None } else { Some(i.min(self.frames.len() - 1)) };
        }
    }

    pub fn type_char(&mut self, ch: char) {
        match self.focus {
            Some(ClipField::Name)
                if (ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
                    && self.name.len() < 40 =>
            {
                self.name.push(ch)
            }
            Some(ClipField::Fps) if (ch.is_ascii_digit() || ch == '.') && self.fps.len() < 6 => {
                self.fps.push(ch)
            }
            _ => {}
        }
    }

    pub fn backspace(&mut self) {
        match self.focus {
            Some(ClipField::Name) => {
                self.name.pop();
            }
            Some(ClipField::Fps) => {
                self.fps.pop();
            }
            None => {}
        }
    }

    /// The clip this would save, or why it can't be saved.
    pub fn to_clip(&self) -> Result<ClipData, String> {
        let tileset =
            self.tileset.clone().ok_or("import a tileset first (File > Import Tileset...)")?;
        let fps = self.fps_value().ok_or_else(|| {
            format!("frames per second must be a number above 0, at most {MAX_FPS}")
        })?;
        let clip = ClipData {
            name: self.name.clone(),
            tileset,
            frames: self.frames.clone(),
            fps,
            looping: self.looping,
        };
        clip.validate()?;
        Ok(clip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three() -> ClipEditor {
        let mut e = ClipEditor::new(Some("dungeon".into()));
        for r in ["a", "b", "c"] {
            e.add_frame(r);
        }
        e
    }

    #[test]
    fn the_preview_plays_at_the_typed_rate_and_scrubs_when_paused() {
        let mut e = three();
        e.fps = "4".into();
        e.preview_t = 0.0;
        e.tick(0.6); // 0.6 s at 4 fps = frame 2
        assert_eq!(e.preview_frame(), Some(2));
        e.tick(0.3); // 0.9 s -> frame 3 -> wraps to 0
        assert_eq!(e.preview_frame(), Some(0));
        e.select(1);
        assert!(!e.playing, "picking a frame pauses on it");
        e.tick(10.0);
        assert_eq!(e.preview_frame(), Some(1), "paused, the clock doesn't move the preview");
        e.looping = false;
        e.toggle_play();
        e.tick(10.0);
        assert_eq!(e.preview_frame(), Some(2), "a one-shot holds its last frame");
    }

    #[test]
    fn frames_can_be_reordered_removed_and_stepped() {
        let mut e = three();
        e.select(0);
        e.move_selected(1);
        assert_eq!(e.frames, vec!["b", "a", "c"]);
        assert_eq!(e.selected, Some(1));
        e.move_selected(5);
        assert_eq!(e.frames, vec!["b", "a", "c"], "out-of-range moves do nothing");
        e.step(-2);
        assert_eq!(e.selected, Some(2), "stepping wraps");
        e.remove_selected();
        assert_eq!((e.frames.len(), e.selected), (2, Some(1)));
        e.remove_selected();
        e.remove_selected();
        assert_eq!((e.frames.len(), e.selected), (0, None));
    }

    #[test]
    fn to_clip_validates_and_round_trips_through_open() {
        let mut e = three();
        e.name = "torch".into();
        e.fps = "12".into();
        let c = e.to_clip().unwrap();
        assert_eq!((c.fps, c.frames.len()), (12.0, 3));
        let reopened = ClipEditor::open(&c);
        assert_eq!((reopened.fps.as_str(), reopened.frames.clone()), ("12", c.frames.clone()));
        e.fps = "0".into();
        assert!(e.to_clip().is_err());
        e.fps = "8".into();
        e.frames.clear();
        assert!(e.to_clip().unwrap_err().contains("no frames"));
        assert!(ClipEditor::new(None).to_clip().unwrap_err().contains("tileset"));
    }

    #[test]
    fn typing_keeps_names_and_rates_well_formed() {
        let mut e = three();
        e.focus = Some(ClipField::Fps);
        e.fps.clear();
        for c in "1x2.5".chars() {
            e.type_char(c);
        }
        assert_eq!(e.fps, "12.5");
        e.focus = Some(ClipField::Name);
        e.name.clear();
        for c in "big torch!".chars() {
            e.type_char(c);
        }
        assert_eq!(e.name, "bigtorch");
    }
}
