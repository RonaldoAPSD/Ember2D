# Archived — the original `demo/` project

This is the six-level demo used throughout Phases 0–3 of
`docs/ember2d-refactor-plan.md`. It was retired during Phase 4 rather than
carried forward: an audit found only two of its six levels
(`level1.level`, `polish.level`) had any player script attached at all, so
Phase 4's plan to move player movement into "the player's script" would
have left four levels — including the `level2` → `level3` progression —
with a player unable to move. Rhai is built here with the `no_module`
feature, so there is no way to share one movement controller across scripts
without duplicating it into every level's player script by hand.

Phase 4's replacement is a small turn-based roguelike at `roguelike/` —
see `docs/HANDOFF.md` and `docs/ember2d-refactor-plan.md` §7 Phase 4 for
the reasoning and the staged plan.

**This archive is kept for reference, not as a working project.** In
particular:

- `scripts/*.rhai` reference `demo/audio/*.ogg` by path. That audio was
  moved (not copied) to `roguelike/audio/` when this project was archived,
  so those paths are stale by design — copying would have added ~1.5 MB of
  duplicate audio for a project that isn't run anymore. `play_sound`/
  `play_music` log a warning and no-op on a missing file; they don't crash.
- `scripts/patrol.rhai`, `chaser.rhai`, and `ai_test.rhai` are useful
  reference for the roguelike's own enemy AI (patrol behavior, chase
  behavior, raycast/pathfinding usage) — that's the main reason this is an
  archive and not a deletion.
- Nothing under this directory is loaded by the engine, referenced by any
  test, or covered by `docs/ember2d-regression-checklist.md` going forward.
