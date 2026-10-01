// level/spawns.rs — a level's named spawn points: the player's default
// start, the points a level transition can enter at, and migrating a pre-v7
// file's two separate spawn fields into the one map.
//
// Step 9-4 (docs/ember2d-master-plan.md §5.8). Before it a level had one
// `spawn_point` (where the player starts) and, beside it, the editor's
// `extra_spawns` list (named points scripts could look up). An RPG needs
// the two to be the same thing: walking out of a house's door has to put
// the player at the house's door on the town map, not at the town's single
// start. So every point is now a name in `LevelData::spawns`, the player's
// default is the one named `"player"`, and a transition names the one to
// enter at (`LevelData::entry_spawn`).
//
// Names are unique (it's a map). The editor still keeps its own
// player-spawn + named-list shape (`LevelGrid`, ember2d-editor), so
// converting its list in can meet a duplicate name; `unique_name` keeps
// both rather than letting the second silently replace the first.

use serde::{Deserialize, Deserializer};

use super::LevelData;

/// The `spawns` entry the player starts at when a level is entered without
/// naming one.
pub const PLAYER_SPAWN: &str = "player";

/// Where the player starts on a level that somehow has no `"player"` entry
/// — what `LevelData::empty` has always used.
const FALLBACK_SPAWN: (f32, f32) = (1.0, 1.0);

impl LevelData {
    /// The player's default start: `spawns["player"]`.
    pub fn player_spawn(&self) -> (f32, f32) {
        self.spawns.get(PLAYER_SPAWN).copied().unwrap_or(FALLBACK_SPAWN)
    }

    /// Moves the player's default start.
    pub fn set_player_spawn(&mut self, at: (f32, f32)) {
        self.spawns.insert(PLAYER_SPAWN.to_string(), at);
    }

    /// Adds a named spawn point, renaming it (`"door"` → `"door_2"`, …) if
    /// the name is already taken. Returns the name it got.
    pub fn add_spawn(&mut self, name: &str, at: (f32, f32)) -> String {
        let name = unique_name(self, name);
        self.spawns.insert(name.clone(), at);
        name
    }

    /// Where the player starts on THIS visit: the spawn the transition into
    /// the level named (`entry_spawn`) if the level has it, else
    /// `player_spawn`. The `bool` is false when a name was asked for and the
    /// level doesn't have it — `do_on_start` warns about that.
    pub fn entry_point(&self) -> ((f32, f32), bool) {
        match self.entry_spawn.as_deref() {
            None => (self.player_spawn(), true),
            Some(name) => match self.spawns.get(name) {
                Some(&at) => (at, true),
                None => (self.player_spawn(), false),
            },
        }
    }

    /// Folds a pre-v7 file's `spawn_point` and `extra_spawns` into
    /// `spawns` (the player's under `"player"`, every other under its own
    /// name, a clash renamed by `add_spawn`). Called by `load`; a no-op on
    /// a v7 file, which has neither.
    pub(super) fn migrate_spawns(&mut self) {
        if let Some(at) = self.legacy_spawn_point.take() {
            self.set_player_spawn(at);
        }
        for (name, x, y) in std::mem::take(&mut self.legacy_extra_spawns) {
            self.add_spawn(&name, (x, y));
        }
    }
}

fn unique_name(level: &LevelData, name: &str) -> String {
    if !level.spawns.contains_key(name) {
        return name.to_string();
    }
    (2..).map(|n| format!("{name}_{n}")).find(|n| !level.spawns.contains_key(n)).unwrap_or_default()
}

/// Reads a pre-v7 file's bare `spawn_point: (x, y)` into the `Option` that
/// `LevelData::legacy_spawn_point` holds (absent → `None` via `default`).
pub(super) fn legacy_point<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<(f32, f32)>, D::Error> {
    <(f32, f32)>::deserialize(d).map(Some)
}

/// Splits an exit or `load_level` target `"path#spawn"` into the path and
/// the spawn name. No `#`, or an empty name after it, means no name.
pub fn split_spawn_target(target: &str) -> (&str, Option<&str>) {
    match target.rsplit_once('#') {
        Some((path, name)) if !name.is_empty() => (path, Some(name)),
        Some((path, _)) => (path, None),
        None => (target, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pre_v7_level_migrates_its_spawn_point_and_named_spawns() {
        let mut level: LevelData = ron::de::from_str(
            "(version: 6, name: \"x\", width: 4, height: 4, spawn_point: (2.0, 3.0), \
             extra_spawns: [(\"door\", 1.0, 1.0), (\"player\", 9.0, 9.0), (\"door\", 5.0, 5.0)], \
             tiles: [])",
        )
        .expect("a v6 level parses");
        level.migrate_spawns();
        assert_eq!(level.player_spawn(), (2.0, 3.0));
        assert_eq!(level.spawns.get("door"), Some(&(1.0, 1.0)));
        assert_eq!(level.spawns.get("door_2"), Some(&(5.0, 5.0)), "a duplicate keeps its point");
        assert_eq!(level.spawns.get("player_2"), Some(&(9.0, 9.0)), "so does a clash with player");
        assert_eq!(level.spawns.len(), 4);
    }

    #[test]
    fn a_v7_level_writes_spawns_and_no_legacy_fields() {
        let mut level = LevelData::empty(4, 4);
        level.add_spawn("door", (2.0, 2.0));
        let text = ron::ser::to_string(&level).unwrap();
        assert!(text.contains("spawns:"), "{text}");
        assert!(!text.contains("spawn_point") && !text.contains("extra_spawns"), "{text}");
        let back: LevelData = ron::de::from_str(&text).unwrap();
        assert_eq!(back.spawns, level.spawns);
    }

    #[test]
    fn the_entry_point_follows_entry_spawn_and_falls_back_to_player() {
        let mut level = LevelData::empty(4, 4);
        level.set_player_spawn((1.0, 2.0));
        level.add_spawn("door", (3.0, 3.0));
        assert_eq!(level.entry_point(), ((1.0, 2.0), true));
        level.entry_spawn = Some("door".into());
        assert_eq!(level.entry_point(), ((3.0, 3.0), true));
        level.entry_spawn = Some("nowhere".into());
        assert_eq!(level.entry_point(), ((1.0, 2.0), false));
    }

    #[test]
    fn a_target_splits_into_path_and_spawn_name() {
        assert_eq!(split_spawn_target("town.level#inn_door"), ("town.level", Some("inn_door")));
        assert_eq!(split_spawn_target("town.level"), ("town.level", None));
        assert_eq!(split_spawn_target("town.level#"), ("town.level", None));
    }
}
