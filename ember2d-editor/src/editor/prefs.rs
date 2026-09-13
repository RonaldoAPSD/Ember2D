// editor/prefs.rs — EditorPrefs/PrefsStore: persisted, per-user editor
// preferences (7D-3, docs/ember2d-master-plan.md §5.4) — today just the UI
// scale choice and the active theme name, the two things a "restart and
// everything reset" editor was missing before this step (R70, §3).
//
// `PrefsStore` is the injectable seam that keeps every test off the real
// user config file: `EditorState::new`/`load`/`new_from_result` always
// start on `PrefsStore::InMemory(EditorPrefs::default())`, and the ONLY
// caller of `PrefsStore::user()` (the real file-backed constructor) is
// `ember2d-app/src/app.rs`'s `run_editor_app`, called once at process
// startup — no test, headless or otherwise, ever constructs an
// `EditorState` through that path. A test that wants to exercise the
// FILE-backed store constructs `PrefsStore::File(some_temp_path)` directly.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The editor's UI-scale preference (7D-3, docs/ember2d-master-plan.md
/// §5.4) — `Auto` follows the display's own DPI reading (`resolve`),
/// `Fixed(n)` pins it regardless of DPI. Physical pixels per UI point,
/// matching `ember2d::renderer::UiSpace`'s own `ui_scale`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiScaleChoice {
    #[default]
    Auto,
    Fixed(u8),
}

impl UiScaleChoice {
    /// Every choice the `Theme` menu's UI Scale entries list, in display
    /// order (7D-3's own last checkpoint wires these to a live menu).
    pub const ALL: [UiScaleChoice; 5] = [
        UiScaleChoice::Auto,
        UiScaleChoice::Fixed(1),
        UiScaleChoice::Fixed(2),
        UiScaleChoice::Fixed(3),
        UiScaleChoice::Fixed(4),
    ];

    /// Clamps a `Fixed` value to `1..=4` — the range the `Theme` menu ever
    /// offers — so a hand-edited or corrupt prefs file can never persist
    /// (or resolve to) an out-of-range scale. `Auto` is always already
    /// valid, nothing to clamp.
    pub fn sanitized(self) -> Self {
        match self {
            UiScaleChoice::Auto => UiScaleChoice::Auto,
            UiScaleChoice::Fixed(n) => UiScaleChoice::Fixed(n.clamp(1, 4)),
        }
    }

    /// The real UI scale to use this frame, given the display's raw OS
    /// scale factor (`ember2d::renderer::DisplayScale::os_scale_factor`).
    /// `Auto` = `round(os_scale_factor * 2)`, clamped to `[1, 8]` — chosen
    /// (2026-09-13, with the user) so 100% OS scaling resolves to `2`, the
    /// editor's existing default look (`MIN_UI_SCALE`, `renderer/mod.rs`),
    /// 150% resolves to `3`, and 200% resolves to `4`; the `[1, 8]` clamp
    /// is a defensive ceiling for an unusually high real-world OS scale
    /// factor, well above anything `ALL`'s own `Fixed` range offers, so
    /// `Auto` can never silently produce something wilder than a user
    /// could pick directly. `Fixed(n)` ignores the display's own scale
    /// factor entirely — that's the point of pinning it.
    pub fn resolve(self, os_scale_factor: f32) -> u32 {
        match self {
            UiScaleChoice::Auto => (((os_scale_factor * 2.0).round()) as i32).clamp(1, 8) as u32,
            UiScaleChoice::Fixed(n) => n.clamp(1, 4) as u32,
        }
    }

    /// The `Theme` menu's own label for this choice (this step's last
    /// checkpoint is the one live caller; kept here now so the label logic
    /// has exactly one home once that menu exists).
    pub fn menu_label(self) -> String {
        match self {
            UiScaleChoice::Auto => "UI Scale: Auto".to_string(),
            UiScaleChoice::Fixed(n) => format!("UI Scale: {n}x"),
        }
    }
}

/// Persisted editor preferences (7D-3, docs/ember2d-master-plan.md §5.4).
/// `#[serde(default)]` at the struct level (needs `EditorPrefs: Default`,
/// below) means a prefs file written by an EARLIER version of this struct
/// (missing a field a later version adds) still loads, the same
/// forward-compatibility contract `ThemeData`'s own `#[serde(default)]`
/// fields keep.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorPrefs {
    pub ui_scale: UiScaleChoice,
    pub theme: String,
}

impl Default for EditorPrefs {
    fn default() -> Self {
        EditorPrefs {
            ui_scale: UiScaleChoice::default(),
            theme: super::theme_loader::DEFAULT_THEME.to_string(),
        }
    }
}

/// Where `EditorPrefs` persists to. `InMemory` never touches disk — every
/// `save` just replaces the held value, which is what lets a test observe
/// a save happened without ever creating a file (`EditorState::new`'s own
/// default, and every test's). `File` is the real, disk-backed store —
/// `PrefsStore::user()` is the only constructor that produces one outside
/// a test that explicitly asks for it.
pub enum PrefsStore {
    InMemory(EditorPrefs),
    File(PathBuf),
}

impl PrefsStore {
    /// The real, per-user prefs store — `PrefsStore::File` at
    /// `user_prefs_path()`, or `PrefsStore::InMemory` with defaults if this
    /// platform/environment offers nowhere sensible to put one (missing
    /// `HOME`/`APPDATA`, e.g. some CI sandboxes) — a resolvable STORE, even
    /// one that can't actually persist, is better than `EditorState`
    /// construction failing outright over a preferences file nobody has
    /// asked to read or write yet.
    pub fn user() -> Self {
        match user_prefs_path() {
            Some(path) => PrefsStore::File(path),
            None => PrefsStore::InMemory(EditorPrefs::default()),
        }
    }

    /// Never fails outward: a missing file (the common case — first run,
    /// or a platform with nowhere to put one) silently returns
    /// `EditorPrefs::default()`; an EXISTING file that fails to parse logs
    /// once (`eprintln!`, allowed in `ember2d-editor` — CLAUDE.md) and also
    /// falls back to defaults, since a preferences file is advisory, never
    /// something worth blocking the editor opening over.
    pub fn load(&self) -> EditorPrefs {
        match self {
            PrefsStore::InMemory(prefs) => prefs.clone(),
            PrefsStore::File(path) => match std::fs::read_to_string(path) {
                Ok(text) => ron::from_str::<EditorPrefs>(&text).unwrap_or_else(|e| {
                    eprintln!(
                        "[prefs] '{}' failed to parse ({e}) — using defaults",
                        path.display()
                    );
                    EditorPrefs::default()
                }),
                Err(_) => EditorPrefs::default(),
            },
        }
    }

    /// Never fails outward — a directory that can't be created or a write
    /// that fails (a read-only filesystem, a permissions error) logs once
    /// and the preference simply doesn't persist this session, rather than
    /// panicking or propagating an error nothing downstream is set up to
    /// handle (every call site here is a fire-and-forget "the user changed
    /// a setting," not a user-initiated Save action with its own error UI).
    pub fn save(&mut self, prefs: &EditorPrefs) {
        match self {
            PrefsStore::InMemory(stored) => *stored = prefs.clone(),
            PrefsStore::File(path) => {
                if let Some(parent) = path.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        eprintln!(
                            "[prefs] couldn't create '{}' ({e}) — not saved",
                            parent.display()
                        );
                        return;
                    }
                }
                match ron::ser::to_string_pretty(prefs, ron::ser::PrettyConfig::default()) {
                    Ok(text) => {
                        if let Err(e) = std::fs::write(path.as_path(), text) {
                            eprintln!(
                                "[prefs] couldn't write '{}' ({e}) — not saved",
                                path.display()
                            );
                        }
                    }
                    Err(e) => eprintln!("[prefs] couldn't serialize preferences ({e}) — not saved"),
                }
            }
        }
    }
}

/// `user_prefs_path`'s own logic, with the environment lookup injected
/// (7D-3, master plan §5.4) — so tests can exercise every branch
/// (Windows/`APPDATA`, XDG, `HOME` fallback, "nowhere to put one") without
/// mutating real process-wide environment variables, which `cargo test`'s
/// parallel execution makes racy. `windows` mirrors `cfg!(windows)` at the
/// one real call site, injected the same way for the same reason.
pub fn prefs_path_from(env: impl Fn(&str) -> Option<String>, windows: bool) -> Option<PathBuf> {
    if windows {
        let appdata = env("APPDATA")?;
        if appdata.trim().is_empty() {
            return None;
        }
        return Some(PathBuf::from(appdata).join("Ember2D").join("editor_prefs.ron"));
    }
    if let Some(xdg) = env("XDG_CONFIG_HOME") {
        let candidate = PathBuf::from(&xdg);
        if !xdg.trim().is_empty() && candidate.is_absolute() {
            return Some(candidate.join("ember2d").join("editor_prefs.ron"));
        }
    }
    let home = env("HOME")?;
    if home.trim().is_empty() {
        return None;
    }
    Some(PathBuf::from(home).join(".config").join("ember2d").join("editor_prefs.ron"))
}

/// `%APPDATA%\Ember2D\editor_prefs.ron` on Windows; `$XDG_CONFIG_HOME/ember2d/editor_prefs.ron`
/// (only when set to a non-empty absolute path) else `$HOME/.config/ember2d/editor_prefs.ron`
/// elsewhere; `None` if the relevant variable is unset/empty. The only
/// non-test caller is `PrefsStore::user()`.
pub fn user_prefs_path() -> Option<PathBuf> {
    prefs_path_from(|key| std::env::var(key).ok(), cfg!(windows))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |key| pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_string())
    }

    #[test]
    fn prefs_path_uses_appdata_on_windows() {
        let path = prefs_path_from(env_of(&[("APPDATA", r"C:\Users\rin\AppData\Roaming")]), true);
        assert_eq!(
            path,
            Some(PathBuf::from(r"C:\Users\rin\AppData\Roaming\Ember2D\editor_prefs.ron"))
        );
    }

    #[test]
    fn prefs_path_prefers_xdg_config_home_then_home_dot_config() {
        let via_xdg = prefs_path_from(
            env_of(&[("XDG_CONFIG_HOME", "/home/rin/.config"), ("HOME", "/home/rin")]),
            false,
        );
        assert_eq!(via_xdg, Some(PathBuf::from("/home/rin/.config/ember2d/editor_prefs.ron")));

        let via_home_only = prefs_path_from(env_of(&[("HOME", "/home/rin")]), false);
        assert_eq!(
            via_home_only,
            Some(PathBuf::from("/home/rin/.config/ember2d/editor_prefs.ron"))
        );

        // An empty/relative XDG_CONFIG_HOME must not be trusted — falls
        // through to HOME instead of producing a bogus relative path.
        let via_bad_xdg =
            prefs_path_from(env_of(&[("XDG_CONFIG_HOME", ""), ("HOME", "/home/rin")]), false);
        assert_eq!(via_bad_xdg, Some(PathBuf::from("/home/rin/.config/ember2d/editor_prefs.ron")));
    }

    #[test]
    fn prefs_path_is_none_without_any_home() {
        assert_eq!(prefs_path_from(env_of(&[]), false), None);
        assert_eq!(prefs_path_from(env_of(&[]), true), None);
    }

    #[test]
    fn a_missing_prefs_file_loads_defaults() {
        let dir =
            std::env::temp_dir().join(format!("ember2d-prefs-test-missing-{}", std::process::id()));
        let store = PrefsStore::File(dir.join("editor_prefs.ron"));
        assert_eq!(store.load(), EditorPrefs::default());
    }

    #[test]
    fn an_unparsable_prefs_file_loads_defaults() {
        let dir =
            std::env::temp_dir().join(format!("ember2d-prefs-test-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir must be creatable");
        let path = dir.join("editor_prefs.ron");
        std::fs::write(&path, "not valid ron at all {{{").expect("write garbage");

        let store = PrefsStore::File(path);
        assert_eq!(store.load(), EditorPrefs::default());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prefs_round_trip_through_a_temp_file() {
        let dir = std::env::temp_dir()
            .join(format!("ember2d-prefs-test-roundtrip-{}", std::process::id()));
        let path = dir.join("editor_prefs.ron");
        let mut store = PrefsStore::File(path.clone());

        let prefs =
            EditorPrefs { ui_scale: UiScaleChoice::Fixed(3), theme: "ember-clean".to_string() };
        store.save(&prefs);
        assert_eq!(store.load(), prefs);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_save_failure_is_reported_not_fatal() {
        // A regular FILE where a directory component is expected: the
        // subsequent `create_dir_all` must fail, and `save` must swallow
        // that (logging, not panicking) rather than propagating it.
        let blocker =
            std::env::temp_dir().join(format!("ember2d-prefs-test-blocker-{}", std::process::id()));
        std::fs::write(&blocker, b"not a directory").expect("write blocker file");
        let bad_path = blocker.join("subdir").join("editor_prefs.ron");

        let mut store = PrefsStore::File(bad_path);
        store.save(&EditorPrefs::default()); // must not panic

        let _ = std::fs::remove_file(&blocker);
    }

    #[test]
    fn fixed_ui_scale_is_clamped_to_1_through_4() {
        assert_eq!(UiScaleChoice::Fixed(0).sanitized(), UiScaleChoice::Fixed(1));
        assert_eq!(UiScaleChoice::Fixed(9).sanitized(), UiScaleChoice::Fixed(4));
        assert_eq!(UiScaleChoice::Fixed(2).sanitized(), UiScaleChoice::Fixed(2));
        assert_eq!(UiScaleChoice::Auto.sanitized(), UiScaleChoice::Auto);
    }

    #[test]
    fn auto_ui_scale_resolves_100_to_2_125_to_3_150_to_3_200_to_4() {
        assert_eq!(UiScaleChoice::Auto.resolve(1.0), 2);
        assert_eq!(UiScaleChoice::Auto.resolve(1.25), 3);
        assert_eq!(UiScaleChoice::Auto.resolve(1.5), 3);
        assert_eq!(UiScaleChoice::Auto.resolve(2.0), 4);
    }
}
