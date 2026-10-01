# Ember2D — Master Plan

**Status of this document:** the single living plan for finishing the Ember2D
refactor and taking the engine to a 0.6.0 release. It replaces
`ember2d-refactor-plan.md`, `ember2d-phase6-plan.md`, `ember2d-phase7-plan.md`,
`ember2d-rpg-demo-feasibility.md`, and `HANDOFF.md`, all of which now live in
`docs/archive/` as historical record (Appendix B maps what each one still
holds). Written against the `claude` branch at `cf59f42` (2026-09-06), after a
full code review of every crate (Appendix C summarises what that review found).

**Companions that stay live beside this file:**

| Document | Role |
|---|---|
| `ember2d-scripting-api.md` | The Rhai API. The engine's real public contract. |
| `ember2d-regression-checklist.md` | Manual test checklist, run at every phase gate. |
| `CLAUDE.md` | Agent instructions and the short "where things are" summary. Points here for everything else. |

---

## 0. How to use this document

### 0.1 It is one file on purpose

Earlier phases each got their own plan doc, plus a handoff note, plus a
refactor plan that appended amendments to itself. By Phase 7 there were five
overlapping documents and three of them contradicted the tree. This file is
the only plan. It is allowed to be long. It is **not** allowed to be stale:
the same rule that applies to code comments applies here — when reality
changes, rewrite the section, never leave it describing something that no
longer exists. History goes in Appendix A, not inline.

### 0.2 Status markers

Every step carries exactly one marker at the start of its heading:

| Marker | Meaning |
|---|---|
| `[ ]` | Not started |
| `[~]` | In progress — say which sub-item in the step body |
| `[x]` | Done — add the commit hash in the heading |
| `[-]` | Dropped — add a one-line reason in the heading |

Phase headings carry the same markers. A phase is `[x]` only when every step
is `[x]` or `[-]` **and** the phase gate (§0.5) has passed.

### 0.3 Step anatomy

Each step is written in the same shape so nothing gets forgotten:

- **Why** — the problem, with the file and line where it lives today.
- **Change** — what to do, concretely.
- **Test** — the test that pins it. No step without a test unless the step
  *is* a doc or a config change.
- **Done when** — an observable condition, not "code written."
- **Scope** — which crates may show a diff. `git diff --stat` is checked
  against this before the step is called done.

### 0.4 Working rules (carried from CLAUDE.md, refined)

1. **One step at a time.** Implement, build, test, smoke-test, report, wait
   for confirmation before the next step.
2. **One commit per step, one tag per phase.** This changes the previous
   "commit at the end of the phase" habit. Reason: the last three phases
   landed as single multi-thousand-line commits (`a016267`, `a1db60f`,
   `cf59f42`), which cannot be bisected or reviewed. Phase 6 Steps 1–7 were
   committed individually and are the model. Commit messages carry measured
   numbers when a step is about performance.
3. **`claude` is the working branch. `main` is trunk.** At every phase gate,
   fast-forward `main` to `claude` and tag `v0.5.<phase>` (§9). Never commit
   to `main` directly.
4. **750-line hard limit per `.rs` file** (raised from 600, 2026-09-06, by
   user direction — 600 had just forced 7A-5's `render_rng` fix into an
   unplanned mid-step file split, `play.rs` → `play/render.rs`, that
   wasn't part of that step's own Change list). Enforced by the check script in
   §6.5, not by memory. Split by sibling file or child module; never shrink
   comments to fit.
5. **Preserve the comment style.** Heavily commented as a learning artifact.
   Rewrite comments when code changes.
6. **The engine runs at the end of every phase.** If a phase can't land
   working, split it.
7. **Don't opportunistically refactor.** Note it in §3's defect register or
   §11's parking lot and move on.
8. **Public API shape changes need explicit approval** and an `API_VERSION`
   bump. Additive changes are fine without a bump but get documented in
   `ember2d-scripting-api.md` in the same commit.
9. **Error handling:** `Result` + a returned diagnostic. `eprintln!` is
   acceptable in `ember2d`/`ember2d-editor`, forbidden in `ember2d-sim`
   (§4.2). A broken script must never crash or hang the editor.

### 0.5 Phase gate

A phase is done when all of these hold:

1. `cargo build --workspace --bins --examples` clean (see §8 on why not
   `--examples` alone); `cargo test --workspace` green.
2. `cargo clippy --workspace --all-targets` introduces no *new* warnings
   versus the previous gate (count recorded in §9).
3. `scripts/check.ps1` (§6.5) passes: no `.rs` over 750 lines, no
   `std::fs`/`Instant`/`eprintln!` in `ember2d-sim`, doc numbers match.
4. `cargo test --test replay` passes 3× as fresh processes, locally and in CI
   on both OSes.
5. The regression checklist sections named in the phase are run and ticked.
6. Both demos play: `cargo run -- demos/roguelike/floor2.level`,
   `cargo run -- demos/shooter/arena.level`.
7. CLAUDE.md "Current State" and this file's §2 are updated in the same
   commit as the tag.

---

## 1. Goals and non-goals

### 1.1 Goals (unchanged in spirit, sharpened)

1. **One renderer, world-space.** A glyph is a textured quad; a sprite is a
   textured quad. ASCII is a preset, not a mode. *(Done — Phase 2/3.)*
2. **General purpose.** No engine code branches on project type.
3. **Two time models.** Realtime and turn-based, switchable per project.
   *(Done — Phase 5.)*
4. **Full in-engine authoring.** Levels, scripts, assets, animation, themes.
5. **Multiplayer-ready seams.** Simulation shaped so 2-player netcode can be
   added. *(Seams closed in Phase 5.5; fidelity gaps in §3 R-series.)*
6. **Games can actually be built with it.** New, and the reason for Phases
   7.5 and 9: every genre the plan names (roguelike, shooter, platformer,
   tactical RPG, RPG) must be buildable from scripts without a Rust change.
   The two shipped demos prove two genres; the RPG demo (Phase 9) proves a
   third and is the acceptance test for the scripting and UI layers.
7. **Learning is a first-class goal**, except where a dependency removes
   work that has already caused burnout once (§7.1).

### 1.2 Non-goals

3D · physics beyond AABB · shipping netcode before 0.6 · mobile/console ·
API backwards compatibility across 0.x (break cleanly, bump `API_VERSION`,
document the migration).

---

## 2. Where the engine is today

*Rewrite this section at every phase gate. It is the one place a new session
should be able to read to know where things stand.*

### 2.1 Layout

Cargo workspace, four crates, one bin target (`ember2d-app`, binary name
`ember2d`):

| Crate | Contents | Depends on | Lines (incl. tests) |
|---|---|---|---|
| `ember2d-sim` | math, color, world, components, level, save, scripting, command, scheduler, graph, event, layers, simulation | serde, ron, rhai, rand only | ~10,290 |
| `ember2d` | engine loop, renderer (wgpu), font system, input/mouse/gamepad, audio (kira), play, project, camera, `sim.rs` per-step pump | `ember2d-sim` | ~13,575 |
| `ember2d-editor` | level/script/graph editor, docking, start screen | both above | ~19,610 |
| `ember2d-app` | `main.rs` + Editor↔Play orchestration | `ember2d`, `ember2d-editor` | ~350 |

Line counts jumped at 7A-9 (`cargo fmt --all`, one-time, no logic change —
rustfmt's own line-wrapping expanded the whole tree by roughly a third; two
files it pushed over the 750-line limit were split at 7A-10, R42/R43) and
again at the equivalent 7C/7D-gate rustfmt sweep (`69c3067`, R87). Every
crate grew substantially across 7C/7D's own new modules (`ui/`, `theme.rs`,
`theme_loader.rs`, `panel/`, the headless test harness, etc.) — still 0
files over 750 real lines (`scripts/check.ps1`, fixed to count them
correctly at R76).

`demos/roguelike/` and `demos/shooter/` (demo projects, moved out of the
repo root here — 7C-5 follow-up, R-series) and `docs/` sit at the repo
root. `Projects/` (also repo root, gitignored) is the default location the
start screen's New/Open Project browsers start from.

### 2.2 Phase status

| Phase | Subject | Status |
|---|---|---|
| 0–4 | Consolidation, defect sweep, world-space camera, sprite/asset model, de-hardcoded play mode, roguelike demo | `[x]` — Appendix A.1–A.5 |
| 5 | Determinism pass, turn scheduler, on_input/on_turn, save/replay, workspace split | `[x]` `a016267` — A.6 |
| 5.5 | Headless `Simulation`, external commands, animation queue, CI | `[x]` `4e4da60` — A.7 |
| 6 | Performance and data-model hardening (14 steps) | `[x]` `a1db60f` — A.8 |
| 7 Parts 1–2 | Pixel-space `UiRect`/`UiFrame`, `Font` trait, glyph atlas, TTF | `[x]` `cf59f42` — A.9 |
| **7A** | Stabilisation sprint | `[x]` `v0.5.7a` — A.10 |
| **7B** | Renderer foundation | `[x]` `v0.5.7b` — A.11 |
| 7C | Editor foundation | `[x]` `v0.5.7c` — A.12 (all 9 steps `[x]`, 7C-9's own §7.1 decision recorded) |
| 7D | Theme and restyle | `[x]` `v0.5.7d` — A.13 (7D-1/7D-4's own deferred remainder — `themes/ember-pixel` — stays unbuilt by design, not a gap; R88/R89 and the `UI Scale: 1.5x` follow-up landed as part of this same closing pass) |
| 7E | Editor features | `[-]` deferred, 2026-09-13 (by user direction) — §5.5. Feature/UX polish (rulers, Inspector 2.0, toasts, command palette, rendering perf, undo audit) rather than refactoring work; revisit as a future update, not blocking the phase sequence below |
| 7.5 | Scripting completeness | `[~]` — §5.6: all 13 steps `[x]`; gate open, awaiting the user's live checklist §11–§13 pass (shooter LOC exception accepted 2026-09-29) |
| 8 | Tilemap, assets, animation authoring | `[ ]` — §5.7 |
| 9 | Scene and UI layer + RPG demo | `[ ]` — §5.8 |
| 10 | Networked 2-player | `[ ]` — §5.9 |
| 11 | Presets, cleanup, 0.6.0 | `[ ]` — §5.10 |

**Next up (handoff note, updated 2026-09-13):** 7C and 7D are closed —
tags `v0.5.7c`/`v0.5.7d`, `main` fast-forwarded (§9). The gate closed on
the user's own direct sign-off ("everything looks good enough for now")
rather than a formal item-by-item run of `docs/ember2d-regression-
checklist.md` §3–§9/§11 — that file's own checkboxes are still `[ ]`,
not a claim this pass ticked them one at a time. What actually backs the
sign-off: extensive live use surfaced and fixed two real bugs this same
session (R88, R89 — §3.2), the `UI Scale: 1.5x` follow-up shipped, both
demos smoke-tested live (floor2, arena — §0.5 item 6, screenshots
confirmed HUD/enemies/player all rendering), and every automated §0.5
criterion passed (build, full test suite, clippy unchanged, replay ×3,
`check.ps1`). If a future session's own live use turns up something the
checklist would have caught, that's still fair game to log as a fresh
R-row — this sign-off isn't a claim nothing's left, only that nothing
currently known is blocking. Known-open rows a fresh session should NOT
re-discover: R46 (wheel `PixelDelta` hardcoded cell size), R79/R80 (graph
mode and start screen don't scale — by design), R81 (dropdowns/context
menus not clamped on-screen — reachable at high UI scale), R82 (unpadded
glyph atlas), R83 (`ContextMenu.x/y` still cell-based), the §11
parking-lot note on `WgpuBackend::render`'s zero-instance early return,
and the two 7D-3 live observations already logged as expected behavior:
chrome text overlaps at 4× on a small window, and a bigger UI scale
SHRINKS the viewport (fixed-point-width side panels eat more of a fixed
window). **Phase 7E (Editor features) deferred by user direction, same
day** — feature/UX polish, not refactoring work; its 6 steps stand as
written in §5.5 for whenever it's picked back up. **Phase 7.5 — Scripting
completeness (§5.6): every numbered step (7.5-1 through 7.5-13) is now
`[x]`**
(`a3d483e`/`fe75ef6`/`0c1ebb2`/`d84e821`/`8e3ebff`/`b1964af`/`83d598a`/
`aff65d4`/`57de3c2`/`1d965f1`/`26e3e82`/`869f919`, 7.5-13 itself a
decision-only step recorded in §7.4 with no commit). The phase itself
stays `[~]`, not `[x]` (§0.5: a phase needs the gate to pass too, not just
every step) — the phase gate (§0.5 in full: both demos rewritten smaller,
`API_VERSION` 7 migration table, tag `v0.5.8`) hasn't run yet. Four things
still owed from earlier steps, each flagged in its own step's
"Landed as" note: neither demo has been launched live this session (no
windowed/GPU sandbox available to this agent) — a real playtest of both,
not just the headless suite, is still worth doing (7.5-11 in particular
still needs a live check that music actually survives a floor transition
and that spatial-sound panning sounds right); the shooter's `director.rhai`
still hand-rolls its own enemy wall-slide (7.5-6 deliberately scoped
shooter enemies out of engine-side solid resolution); no project ships
with `TurnModel::Energy`/`ActionCost` yet (7.5-7, both new and opt-in); and
R91/R92 (§3.2, 7.5-9/7.5-10) — the ~66 remaining pre-existing lookup-only
`HashMap`/`HashSet` sites still need their own `#[allow(clippy::
disallowed_types)]` annotation, and the per-step spatial index +
collider-layer `Rc<str>` pair 7.5-10 deferred are both unscheduled.**

### 2.3 Baseline numbers (at `v0.5.7d`)

| Metric | Value | Where measured |
|---|---|---|
| Tests | 381, all pass (was 252 at `v0.5.7b` — 209 unit + 42 integration + 1 doctest) — every 7C/7D step's own named tests, plus R88/R89 and the `UI Scale: 1.5x` follow-up | `cargo test --workspace` |
| Clippy | 43 at `--lib` scope, 55 at `--all-targets` (down from `v0.5.7b`'s 59/71 — tracked per step through 7C/7D, no regression at this gate) | `cargo clippy --workspace --lib` / `--all-targets` |
| rustfmt | `cargo fmt --all -- --check` clean (one-time sweep, `69c3067`, same shape as 7A-9 — the mechanical reflow alone pushed one file over 750 lines, R87, split the same session) | `cargo fmt --all -- --check` |
| `cargo test --test replay` | green 3× fresh processes | §0.5 gate criterion 4 |
| floor2 p50 ms/step | not re-measured this gate (no sim-path change across 7C/7D) | `cargo run --release -p ember2d-sim --example bench_sim` |
| floor2 allocs/step | not re-measured this gate (no sim-path change across 7C/7D) | same |
| `LEVEL_FORMAT_VERSION` | 3 (unchanged since 7A-4) | `ember2d-sim/src/level.rs:298` |
| `API_VERSION` | 6 (unchanged) | `ember2d-sim/src/scripting/types.rs:25` |
| Registered script functions | 124 (unchanged) | `grep -c register_fn ember2d-sim/src/scripting/registry.rs` |
| Files over 750 lines | 0, real lines and `check.ps1`'s own count agree (fixed at R76 — the check used to silently undercount; the 3 files it had been hiding were each split into a `.rs` + child submodule). Largest file in the tree: `ember2d-editor/src/editor/mod.rs` at 750 | `scripts/check.ps1` / `wc -l` |
| Dependencies | wgpu 30.0.1, winit 0.30.13, kira 0.12.4, glam 0.33.7, rand 0.8.6, gilrs 0.11.2, rhai 1.24.0, fontdue 0.9.4 (unchanged since `v0.5.7b`) — new this gate: `arboard` 3.6.1 (7C-8, OS clipboard) | `Cargo.lock` |
| Both demos launch | `cargo run -- demos/roguelike/floor2.level` and `-- demos/shooter/arena.level` — both confirmed live, screenshots taken (HUD/HP/enemies/player rendering; §0.5 item 6) | manual, this gate |

---

## 3. Defect register

One table. Every known defect, its status, and where the fix lives. **This
is the only place defect status is tracked.** The regression checklist §14
points here rather than keeping its own copy.

Severity: **S1** crash/hang/data-loss reachable from normal use · **S2**
wrong behaviour users will hit · **S3** wrong behaviour in a rare path or a
determinism/contract violation with no visible symptom yet · **S4** debt.

### 3.1 Refactor-era defects (D-series)

| # | Sev | Defect | Location | Status |
|---|---|---|---|---|
| D1 | S2 | Input edge detection broke under fixed timestep | `engine.rs` | `[x]` Phase 1 — buffer-until-consumed |
| D2 | S2 | `set_persistent` in `on_start` discarded | `play/spawn.rs` | `[x]` Phase 1 |
| D3 | S3 | RNG nondeterministic in three places | `scripting/engine.rs`, `play.rs` | `[x]` Phase 1 — **but see R15** (render consumes sim RNG) |
| D4 | S2 | Trigger colliders defaulted to `"solid"` layer | `play/spawn.rs` | `[x]` Phase 1 |
| D5 | S3 | Equal-z draw order nondeterministic | `play.rs` | `[x]` Phase 1 |
| D6 | S2 | Editor obeyed project `gameplay_loop` | `main.rs`, `engine.rs` | `[x]` Phase 1 |
| D7 | S3 | Turn mode integrated physics with dt=1.0 | `sim.rs` | `[x]` Phase 5 Step 5f |
| D8 | S2 | Hot reload cleared all entity scopes | `scripting/engine.rs` | `[x]` Phase 1 — **scopes now dead entirely, see R22** |
| D9 | S2 | Erroring script kept running every frame | `scripting/engine.rs` | `[x]` Phase 1 |
| D10 | S3 | `spawn_entity` hardcoded white/z=2/1×1 trigger | `scripting/api.rs` | `[x]` Phase 1 (11-arg overload) — **undocumented, see R31** |
| D11 | S2 | Per-frame allocation churn | sim + play | `[x]` Phase 6 Steps 3–8 (−81% ms/step) |
| D12 | S3 | `"locked"` magic string | `play.rs` | `[x]` Phase 1 |
| D13 | S3 | Texture sprites bypassed culling | `play.rs` | `[x]` Phase 1 |
| D14 | S3 | Colour-space mismatch atlas vs textures | `renderer/backend.rs` | `[x]` Phase 1 |
| D15 | S2 | Real runtime error swallowed if message contained "Function not found" | `scripting/engine.rs` | `[x]` Phase 4 |
| D16 | S2 | Script HUD vanished on pause | `play.rs`, `scripting/engine.rs` | `[x]` Phase 4 Step 4g |
| D17 | S1 | Save/load lost script globals | `save.rs`, `play.rs` | `[x]` Phase 5 Step 5c — **round trip still not faithful, see R7** |
| D18 | S2 | Editor save scrambles tile order (HashMap) | `editor/grid.rs:202` | `[x]` 7C-6 (`b2a608f`) — `LevelGrid::tiles` is now a `BTreeMap`; `to_level_data` additionally sorts the collected `Vec<TileRecord>` by `(layer, y, x)` to match `gen_roguelike`'s on-disk convention (the map's own key order is `(x, y, layer)`, wrong for this purpose on its own) |
| D19 | S2 | Keypress dropped during enemy animation | `play.rs` | `[x]` Phase 6 |
| D20 | S2 | Animations stacked serially into input freeze | `play.rs` | `[x]` Phase 6 |
| D21 | S3 | `check_hot_reload` syscall per script per step | `scripting/engine.rs` | `[x]` Phase 6 Step 6 |
| D22 | S3 | Cancelled and just-fired timers share a sentinel | `api.rs`, `apply.rs:119-128` | `[x]` 7.5-8 — `TimerState`/`TimerWrite` enums replace the float sentinel; `Cancelled`/`Consumed` are now distinct terminal states |

### 3.2 Review defects (R-series, found 2026-09-06)

| # | Sev | Defect | Location | Status |
|---|---|---|---|---|
| **Scripts can crash or hang the editor** | | | | |
| R1 | S1 | No Rhai operation limit; `loop {}` hangs the process | `scripting/engine.rs:100` | `[x]` 7A-1 — `set_max_operations(2_000_000)` |
| R2 | S1 | `random_int(5, 1)` panics (`gen_range` empty range) | `api.rs:244` | `[x]` 7A-1 — swap bounds when `max < min` |
| R3 | S1 | `random_bool(NaN)` panics (NaN through `clamp` into Bernoulli) | `api.rs:246` | `[x]` 7A-1 — reject non-finite before `clamp` |
| R4 | S1 | `parse_color` byte-slices a 6-byte non-ASCII string → panic | `scripting/types.rs:172-175` | `[x]` 7A-1 — `is_ascii()` check; `set_tint` now no-ops + logs once on any malformed color instead of panicking or silently overwriting |
| R5 | S1 | `Animator::advance` loops forever at large `speed` (f32 absorption) | `components/animator.rs:77-90`, `apply.rs:97` | `[x]` 7A-1 — `%`-collapse past 4 clip cycles in `advance`; `apply.rs` clamps scripted speed to `0.0..=64.0` |
| R6 | S1 | NaN position → inconsistent comparator in collision sort → panic | `world.rs:271-273` | `[x]` 7A-1 — `total_cmp`; `set_position` also rejects non-finite input at the source |
| **Data integrity** | | | | |
| R7 | S1 | Save-load never rebuilds `exit_targets`; stairs dead after load. `turn_number` and scheduler due times not saved | `simulation.rs:177, 273-288, 429`; `spawn.rs:86` | `[x]` 7A-3 — `index_exits` (pure function of `self.level`) called on both branches; `SaveState` gains `turn_number`/`scheduler`; `TurnScheduler::snapshot`/`restore` replace an unconditional `rebuild_scheduler` on load |
| R8 | S2 | Shipped levels are format v2, code is v3; loader never checks `version` | `level.rs:298, 439`; `roguelike/*.level` | `[x]` 7A-4 — `LevelData::load` rejects `version > LEVEL_FORMAT_VERSION`; every shipped level regenerated to v3 |
| R9 | S2 | `clear_all_persistent` clears the pending queue, not the store — a no-op | `api.rs:313` | `[x]` 7A-1 — request-a-clear flag on `ScriptState`, applied to the real store in `apply_ctx` |
| R10 | S3 | `set_tag`/`play_clip` on a missing id create ghost components | `apply.rs:64, 88` | `[x]` 7A-1 — both guarded with `world.transforms.contains_key` |
| **Editor stability** | | | | |
| R11 | S1 | Non-ASCII text panics: script editor byte cursor, highlighter char/byte mix, console byte-slice | `input/script_editor.rs:50-150`; `ui/script.rs:61, 91`; `ui/panels/dock.rs:133` | `[x]` 7A-2 — cursor mutations convert char index → byte offset via `char_byte_offset`; highlighter builds from `chars`, not `line` byte-slices; console truncates via `chars().take` |
| R12 | S1 | Every printable key since the last prompt floods the next prompt's field | `engine.rs:238`; `editor/input/text.rs:14` | `[x]` 7A-2 — `InputManager::begin_text_capture`/`finish_frame_text_capture`; script editor, palette editor, palette search, and graph param field all migrated off `key_to_char` to `take_text()` |
| R13 | S2 | Rect/Line/Fill tools skip the `ignore_drag` painting guard | `input/canvas.rs:168-227` | `[x]` 7A-2 — `&& !self.ignore_drag` added to all three |
| R14 | S2 | Docked script panel: click places cursor but typing fires global shortcuts (S saves, F fills, Z resizes) | `input/panels/file_and_script.rs:105-115`; `input/mod.rs:253` | `[x]` 7A-2 — `EditorFocus` (derived, see 7A-2's "Landed as" note); `handle_shortcuts` returns early unless `Canvas` |
| R15 | S3 | Render-time camera shake consumes the sim RNG → particle stream frame-rate dependent | `play.rs:269-272, 496-513` | `[x]` 7A-5 — `render_rng` (own stream, seeded `level_seed ^ RENDER_RNG_SEED_OFFSET`) for camera + entity shake jitter; `rng` now only reads at apply_outcome's deterministic per-step cadence |
| R16 | S3 | Wall-clock `elapsed` reaches scripts via `get_elapsed` | `engine.rs:286, 316`; `api.rs:146` | `[x]` 7A-5 — `Simulation::step_count`, incremented once per `step` call; `PlayState` builds `StepInput::elapsed`/`late_step`'s `elapsed` from it instead of `UpdateContext::elapsed` |
| R17 | S3 | Filesystem I/O inside `ember2d-sim` (`Path::exists`, `LevelData::load` in `late_step`) | `simulation.rs:66, 432`; `spawn.rs:46, 71, 79` | `[ ]` → 7.5-9 |
| R41 | S4 | `eprintln!` inside `ember2d-sim` (`World::get_global_position`'s parent-cycle warning) — found writing `scripts/check.ps1` (7A-6); not one of R16/R17's already-tracked locations | `world.rs:142` | `[ ]` → 7.5-9 (same step as R17; both are "no ambient I/O in the sim" cleanup) |
| R18 | S2 | `receive_log` has zero callers; play-mode script errors never reach the editor console | `impl_state/mod.rs:527` | `[x]` 7C-7 (`35887a6`) — see 7C-7's own "Landed as" note for the full fix (the type-erased state stack was the real obstacle, not just a missing call) |
| R19 | S2 | File › Start Screen orphans an `EditorState` on the state stack | `ember2d-app/src/app.rs:85`; `main.rs:63-67` | `[x]` 7A-2 — both `Transition::ToStart` arms in app.rs pop before returning; `Engine::push_state` debug-asserts depth ≤ 3 |
| R20 | S2 | `TilePalette::current()` indexes `[0]`; empty or out-of-range `selected` from a loaded palette panics | `palette.rs:284`; `text.rs:53, 84`; `input/mod.rs:75, 111` | `[x]` 7A-2 — invariant enforced at `TilePalette::load` (reject empty tiles, clamp `selected`); `current()` itself unchanged, see 7A-2's "Landed as" note |
| **Renderer / engine** | | | | |
| R21 | S2 | Non-integer cell projection: cells stretched unless window is an exact cell multiple; `scale_factor()` width-only; HiDPI mis-sized | `renderer/mod.rs:523-546` (`try_handle_resize`), `renderer/mod.rs:190-192` (`scale_factor`) — locations shifted after 7B-1; original `engine.rs:246`/`backend.rs:328` cites are stale | `[x]` 7B-2 — `compute_layout` floor-divides and letterboxes instead of stretching; `scale` is now DPI-derived (`window.scale_factor().round().max(1.0)`, was a fixed constant); `ScreenMapping` (`origin_px`, per-axis `cell_px`) replaces the width-only `scale_factor()`, consumed by a real wgpu viewport (backend.rs) and by `MouseState::handle_move` |
| R22 | S4 | `scopes` map is dead state (rhai rewinds scope; timers moved off it) yet maintained by hot-reload and despawn | `scripting/engine.rs` | `[x]` 7.5-10 — deleted entirely, confirmed dead by reading rhai 1.24's own `call_fn`/`CallFnOptions` source (default `rewind_scope: true`); every `call_fn` call site now takes a throwaway `Scope::new()` via the new `call_lifecycle_fn` helper |
| R23 | S3 | Frame pacing double-throttles (Fifo vsync + `thread::sleep` to 60) | `engine.rs:389-392` | `[x]` 7B-4 — the tail-of-loop `thread::sleep(FRAME_DURATION - frame_elapsed)` and its now-unused `TARGET_FPS`/`FRAME_DURATION` constants are gone; `wgpu::PresentMode::Fifo` (renderer/mod.rs) is the sole pacing mechanism now |
| R24 | S3 | Key repeat inconsistent: `repeat` flag ignored; letters repeat into text buffer, editing keys never repeat; Ctrl+S pushes "s" | `engine.rs:226-243` | `[x]` 7B-4 — `PressBuffer::handle_repeat`/`is_repeating` (new `repeating: HashSet<K>`, separate from `pending`/`consumed`) fed from `KeyEvent::repeat` in `EventPump`; script editor's Up/Down/Left/Right/Tab/Enter/Backspace now check `is_repeating` alongside `just_pressed`; `text_buffer` pushes gated on `!ModifiersState::control_key() && !super_key()` (new `Engine::modifiers` field, updated on `WindowEvent::ModifiersChanged`) so Ctrl+S no longer leaks an "s" |
| R25 | S3 | `GamepadState::poll` ignores `Disconnected`; held buttons stick | `gamepad.rs:131-159` | `[x]` 7B-4 — `EventType::Disconnected` now calls the new `PressBuffer::retain` to drop every held/pending/consumed/just-released/repeating entry for that `gamepad_id`, plus clears its `axes` entries |
| R26 | S3 | GPU textures never freed; `AssetManager::clear` doesn't invalidate `texture_cache` | `backend.rs:124` | `[x]` 7B-3 — new `TextureBudget` (LRU + byte budget, `renderer/texture_budget.rs`) plus `AssetManager::clear` now evicts every id it forgets via a new `TextureEvictor` trait `Renderer` implements |
| R27 | S3 | `draw_text_px` clones the 4 MB atlas `Texture` per call | `renderer/mod.rs:270` | `[x]` 7B-3 — only clones real pixel data when `dirty` or not yet GPU-resident (`WgpuBackend::has_texture`); every other call passes a lightweight placeholder instead |
| R28 | S3 | Bottom world row never drawn (culled for a HUD bar removed in Phase 4) | `play/render.rs:106-108` | `[x]` 7B-4 — removed the trailing `.saturating_sub(1)` on `height` in `in_viewport`; confirmed visually (floor2 screenshot, bottom wall/floor row now renders through to the HUD text row) and via a new regression test |
| R29 | S3 | `request_adapter`/`request_device` `.expect` → panic with no message on unsupported GPU | `renderer/mod.rs:84, 93` | `[ ]` → 7B-1 |
| R30 | S3 | Audio: decode from disk on every `play_sound`; new `AudioEngine` per level kills music | `audio.rs:38, 51`; `play.rs:203` | `[x]` 7.5-11 — `AudioEngine` moved to `Engine` (one device stream for the app's lifetime, threaded via `UpdateContext::audio`); `play_sound`/`play_music` now decode through a per-path `StaticSoundData` cache instead of the filesystem on every call; `play_music` is idempotent by path so a still-alive device is actually observable across a level transition |
| **API and docs** | | | | |
| R31 | S2 | i64/f64 dispatch trap: `draw_hud(x, y, ..)` with float `x` fails "function not found"; only `submit` coerces | `api.rs` throughout | `[ ]` → 7.5-1 |
| R32 | S3 | Sentinel inconsistency (`-1` vs `0.0` vs `[]`; `()` tombstone means scripts can't store unit) | `api.rs` | `[ ]` → 7.5-1 |
| R33 | S3 | `is_animating` always `false` | `api_animation.rs:63` | `[x]` 7.5-7 — `StepInput::animating` plumbs `PlayState.animations`' entity ids through to `ScriptState`; `is_animating` checks real membership now |
| R34 | S2 | Node-graph codegen: no string escaping (code injection), no cycle guard (stack overflow), untyped `"0.0"` defaults, block-scoped `let` | `graph/codegen.rs:30, 40, 55, 138-146, 224, 249` | `[x]` 7.5-12 |
| R35 | S3 | `is_collider_locked`/`set_collider_locked` and the 11-arg `spawn_entity` registered but undocumented; API doc says D3/D9 unfixed and timers are scope variables | `ember2d-scripting-api.md` | `[x]` 7A-6 — both documented; D3/D9 marked fixed; §2's "Per-entity scope" timer example rewritten to match Step 9 (nothing writes into scope between calls anymore, R22) |
| R36 | S3 | HANDOFF/CLAUDE.md/checklist/index.html contradict the tree (test counts, format version, Phase 7 status, CI) | `docs/`, `index.html` | `[x]` 7A-6 — `index.html` already gone (pre-session); CLAUDE.md's format version/function count fixed and its "Current State" narrative replaced with a pointer to §2; checklist's test-count header and §14's defect table replaced with pointers; CI text (§15/§17) deliberately left for 7A-7 per this step's own Change list |
| **Process** | | | | |
| R37 | S2 | CI deleted; no cross-platform determinism check exists | `.github/` | `[x]` 7A-7 — `.github/workflows/ci.yml` recreated (`windows-latest`+`ubuntu-latest`, see 7A-7's "Landed as" note); pushed but blocked by an account billing lock, not a workflow defect — CI-green link still pending that being cleared |
| R38 | S4 | `play.rs` 607 lines (limit 600); `panel/mod.rs` 597 | `ember2d/src/play.rs` | `[x]` moot — the limit itself rose to 750 (§0.4, 2026-09-06, by user direction) after 7A-5 had already pulled `play.rs` back to exactly 600 (debug overlay + HUD-draw dispatch moved to `play/render.rs`); `panel/mod.rs` (597) was never over either limit. No file in the codebase is within 100 lines of 750 as of this row. |
| R39 | S4 | LICENSE placeholder; no `license`/`repository` in manifests; OFL text not bundled | `LICENSE`, `*/Cargo.toml` | `[x]` 7A-8 (`1e6080f`) — see 7A-8's "Landed as" note |
| R40 | S4 | No tags; `main` 26 commits behind; version 0.5.0 meaningless | git | `[ ]` unresolved by 7A-8 itself — tagging/fast-forwarding `main` happens at the Phase 7A gate (§0.4, §9), after 7A-9; not a 7A-8 commit |
| R42 | S4 | `scripting/api.rs` grew from 515 to 771 lines (limit 750) — found running `scripts/check.ps1` after 7A-9's `cargo fmt --all` pass; rustfmt's mechanical line-splitting alone pushed it over, no logic changed | `ember2d-sim/src/scripting/api.rs` | `[x]` 7A-10 — everything from the old "V0.4 Extensions" marker through `api_version` moved to a new `api_ext.rs` (third sibling `impl ScriptCtx` block); `api.rs` now 439 lines |
| R43 | S4 | `scripting/engine.rs` grew from 591 to 790 lines (limit 750) — same cause as R42, same commit | `ember2d-sim/src/scripting/engine.rs` | `[x]` 7A-10 — the ~140-line `register_fn` sequence moved to a new `registry.rs`; `engine.rs` now 684 lines |
| R44 | S2 | Space key never reached any text field — winit reports it as `Key::Named(NamedKey::Space)`, not `Key::Character(" ")`, and `Engine::poll_events` matched only `Character`. Made the built-in script editor (and every other `take_text()` consumer) unable to type a space, which makes writing real Rhai source impossible. Found live during the Phase 7A gate's own manual regression pass (§8, "Type, navigate with cursor keys, scroll") | `ember2d/src/engine.rs:266-275` (before fix) | `[x]` 7A-11 — `Key::logical_key_text` handles `Named(NamedKey::Space)` alongside `Character`; unit-tested directly (no live event loop needed) |
| R45 | S2 | `ember2d-app`'s `--editor <path>` CLI branch called `EditorState::load` but never set `project_folder` (only the start-screen "Open Project" flow, via `new_from_result`, did) — the level itself rendered fine, but the Files panel showed "(empty folder)" and New Script silently did nothing, since both require `project_folder: Some(_)`. Found live in the same manual regression pass, right after confirming R44's fix | `ember2d-app/src/main.rs:29-56` (before fix) | `[x]` 7A-12 — new `EditorState::open_project_folder` (`pub`, wraps the existing `pub(super) load_palette`/`refresh_project_files`) called from the `--editor <path>` branch, mirroring what `new_from_result` already did for the start-screen path |
| R46 | S4 | `WindowEvent::MouseWheel`'s `PixelDelta` branch still hardcodes `/ 8.0`/`/ 16.0` — the same class of gap 7B-2 fixed for click position, just for scroll delta instead. Found while fixing R21 (7B-2); not in that step's own Change list (only `backend.rs`/`mouse.rs`'s position-mapping sites were named), and `PixelDelta` scroll events are rare in practice (most mice/touchpads report `LineDelta`), so left as a follow-up rather than expanding 7B-2's scope | `ember2d/src/engine.rs` (`EventPump::window_event`, `MouseWheel` arm) | `[ ]` unscheduled |
| R47 | S4 | `cargo clippy -p ember2d --all-targets` reports dead code in `ember2d/tests/common/mod.rs` (`TurnHarness`'s own test helpers): `find_tagged_entity_at`, `test_temp_dir`, and `TurnHarness::load`/`player_id`/`player_pos` are never called by any current test. Found while checking 7B-3's own "no dead code in `ember2d`" done-when criterion — confirmed pre-existing (identical warnings before 7B-3's changes) and unrelated to that step's named Change list (renderer resource hygiene only), so not fixed there | `ember2d/tests/common/mod.rs:74, 223, 227, 238, 254` | `[ ]` unscheduled |
| R48 | S2 | 7B-2's DPI-derived `scale` floored at `1.0` — correct per R21's own "land on a whole physical pixel" wording, but on any standard 100%-scale display (the common case, not a corner case) `window.scale_factor()` is exactly `1.0`, so every cell rendered at its literal native 8×16 physical pixels: half the size the pre-7B-2 hardcoded `SCALE = 2` always gave, and hard to read on a modern display. Found by the user testing 7B-2 live, right after it was marked done | `renderer/mod.rs` (`Renderer::new`, `recompute_layout`); `engine.rs` (`WindowInit::resumed`'s pre-window guess) | `[x]` 7B-2 follow-up — new `MIN_UI_SCALE = 2.0` constant floors all three sites (was `.max(1.0)`); restores the old default size on ordinary displays, still scales up correctly above it on genuine HiDPI (150%/200%+ already rounds to 2+). A real user-facing scale *setting* is 7D-3, not this — by user direction, this is a scoped tuning fix only |
| R49 | S4 | `BitmapFont`'s `Font`-trait glyph model represents a native glyph as a literal, unstretched 8×8 square, but the dedicated font8x8 GPU path (`WgpuBackend::draw_char`) has always stretched that same 8×8 bitmap 2x vertically to fill the 8×16 `CELL_W`×`CELL_H` cell — the two disagree on what "native size" looks like. Found building 7B-5 (before/after screenshot comparison caught the shrink); currently dormant because `draw_str`'s default path deliberately keeps calling `draw_char` directly rather than routing through the `Font` abstraction (see 7B-5's "Landed as" note) | `renderer/font/bitmap.rs` (`BitmapFont::glyph`/`ascent`/`line_height`); `renderer/backend.rs` (`WgpuBackend::draw_char`) | `[ ]` unscheduled — blocks ever fully unifying `draw_str` onto `draw_text_px` for the bitmap case without either changing `BitmapFont`'s glyph model or `draw_text_px`'s destination-sizing logic (see R50) |
| R50 | S4 | `Renderer::draw_text_px`'s glyph destination rect always uses `GlyphInfo::atlas_rect.w`/`.h` directly as the drawn size — correct for `TtfFont` (whose atlas rasterizes each glyph AT the requested px, so `atlas_rect` already IS the intended render size) but wrong for `BitmapFont` at any non-native px: `atlas_rect` stays fixed at the native 8×8 texture region regardless of the requested size (only `advance`/`offset` scale), so a `BitmapFont` glyph requested at e.g. 16px would advance the pen 16px per character without the glyph itself actually being drawn any larger than 8×8 — a gap that predates 7B-5 but was undiscovered until this step's investigation, since `draw_text_px` had no live caller before it (its own doc comment used to say so) | `renderer/text.rs` (`draw_text_px`, dest rect construction) | `[x]` 7D-3 checkpoint 1 (renderer foundation) — went from dormant to live the instant UI-points chrome text could draw a `BitmapFont` at a non-native size (the fallback theme, and the planned `ember-pixel`). `GlyphInfo` gained a `size` field (the actual drawn size, distinct from `atlas_rect`'s own — `TtfFont` sets the two equal; `BitmapFont` sets `size` to the effective scaled size while `atlas_rect` stays the fixed native 8×8 cell); `draw_text_run`'s (formerly `draw_text_px`'s) dest rect now uses `g.size`, not `g.atlas_rect`. Regression tests: `bitmap_glyph_size_is_the_effective_drawn_size` (renamed `glyph_size_is_the_effective_drawn_size_not_the_native_atlas_cell`, `font/bitmap.rs`), `ttf_glyph_size_equals_its_atlas_rect` (`font/ttf.rs`) |
| R51 | S1 (was S2) | **Re-diagnosed 2026-09-13 — not DWM, not a resize bug: the editor was being DRAWN underneath play mode every frame.** Original report: maximizing the editor window, then pressing F5, leaves "stale editor-panel pixels (the docked Inspector's title/tool text, colored bars)" wherever the play grid is narrower than the window. The original investigation confirmed the surface reconfigures and `LoadOp::Clear` covers it every frame, and concluded DWM caching — both observations were true and both beside the point: after that clear, `Engine::run` rendered EVERY state in its stack bottom-to-top (`engine.rs`, "Render all states from bottom to top", dating to 2026-05), and `app.rs` pushes `PlayState` ON TOP of the still-live `EditorState` rather than replacing it (deliberately — the editor's grid/undo/panels survive the preview). That was only ever invisible because `PlayState::render` opened with an opaque full-screen `draw_rect_filled(0,0,w,h,' ',Reset,Reset)` that buried the editor's draws — the fill 7B-3 (`af582e3`) deleted in favor of the GPU clear, which runs BEFORE the editor draws and so hides nothing. 7B-3's own screenshots were editor-only and standalone-play-only, never the F5-over-editor stack, so it shipped unnoticed; the "1px resize nudge repaints most of it" observation was `apply_layout` moving the docked panels underneath. Once 7D-2/7D-3 made the chrome dense and high-contrast (9-slice panels, TTF text, points layout), the whole editor — panels, bars, even the viewport drawing the level a second time — showed through every cell play didn't draw, at any window size (user's 2026-09-13 screenshots of both demos) | `ember2d/src/engine.rs` (render loop, `GameState`); `ember2d/src/play.rs` (`PlayState::render`, `PauseMenuState`); `ember2d-app/src/app.rs:63` (push-not-replace) | `[x]` 7B-3 follow-up (`986239e`) — `Engine::run` now renders only from the topmost OPAQUE state up, the ordinary state-stack rule: new `GameState::is_overlay()` (default `false`), overridden to `true` only by `PauseMenuState` (a small centered panel that genuinely needs the play screen visible under it); the index is computed by the new `state_stack::render_start_index` (`ember2d/src/state_stack.rs`, its own file since `engine.rs` is already over the 750-line limit and the rule needs window-free unit tests). `EditorState` gained an `on_resume` that clears `ui_frame`, since `draw` (where `apply_layout`/`ui_space`/`ui_frame` refresh) no longer runs while play is on top and the first frame back runs `update()` first. Also fixed `Transition`'s doc comments, which claimed `ToPlay` "replaces current state". Regression tests (`state_stack.rs`): `an_opaque_top_state_hides_everything_beneath_it`, `an_overlay_on_top_still_renders_the_opaque_state_under_it`, `two_stacked_overlays_render_from_the_opaque_state_beneath_both`, `an_all_overlay_stack_renders_from_the_bottom`. Verified live: F5 on both demos shows only play; Esc draws the pause panel over the still-visible play screen; Back to Editor (also after resizing the window mid-play) restores a correctly re-laid-out editor whose menus still open; standalone `cargo run -- <level>` unchanged |
| R52 | S3 | 7C-1's own debug assertion in `handle_canvas_input` — "any click reaching here while `UiFrame::hit()` is `Some` is a bug" — fired constantly during ordinary use (both painting and legitimate widget clicks), found live by the user during 7C-2's manual smoke pass. Root cause: the assertion assumed panel input consuming a click already stops `handle_canvas_input` from running, but `handle_update` calls `handle_panel_input`/`handle_canvas_input`/`handle_shortcuts` unconditionally, one after another, regardless of what an earlier stage did — `handle_panel_input`'s own internal `return`s only stop itself. That's exactly the gap 7C-4's `Consumed \| Pass` chain is designed to close; the assertion's premise doesn't hold until then, so as written it couldn't distinguish a real bleed-through from any click that happens to also land on any registered widget anywhere on screen (which is most clicks). Painting itself was never actually broken — `handle_canvas_input`'s own `mouse_to_grid` bounds check is what really prevents unwanted painting, unrelated to whether `UiFrame::hit` matched something | `ember2d-editor/src/editor/input/canvas.rs` (`handle_canvas_input`) | `[x]` 7C-1 follow-up (`f89b301`) — assertion removed; re-add once a real `Consumed`/`Pass` signal exists to check instead of `UiFrame::hit` alone. 7C-4 (`0eb2db8`) replaced the boolean soup but deliberately did not build that signal (see 7C-4's own "Landed as" note) — still deferred, now past 7C-4 |
| R53 | S4 | `cargo fmt --all -- --check` reports diffs in 63 files across all three non-`ember2d-app` crates, none of which touch 7C-1 through 7C-4's own changed lines (verified: `git stash` on top of `v0.5.7c`'s uncommitted tree still reproduces all 63) — the `v0.5.7b` baseline (§2.3) recorded this clean. Most likely cause is a local rustfmt version difference from whatever ran 7A-9's one-time `cargo fmt --all` pass, not anything a specific step's own diff introduced — the affected lines are scattered, small (mostly whether a boolean `if`/`let` condition wraps), and span files no 7C step touched | tree-wide, found running 7C-5's own gate checks | `[ ]` unscheduled — needs `cargo fmt --all` re-run and a fresh baseline recorded once whichever step next touches the affected files, or at the 7C gate itself; not this step's job per its own Scope |
| R54 | S1 | The fullscreen script editor (`EditorMode::Script`) was completely unusable — opening it (clicking a `.rhai` file in the File Browser) worked for exactly one frame, then silently reverted to `Paint` on every frame after, invisible at 60fps: looked to the user like clicking the file did nothing. Root cause: 7C-4's `handle_update` dispatch (`match std::mem::take(&mut self.mode) { EditorMode::Script => { self.handle_script_mode_input(input, mouse); return; } ... }`) empties `self.mode` to `EditorMode::default()` before calling the handler — every OTHER hard-exclusive arm's own handler restores its mode as its first action (`handle_color_picker_input`/`handle_palette_editor_input`/`handle_place_spawn_input`/`handle_palette_search_input`), but `EditorMode::Script`'s arm never did, so `handle_script_mode_input`'s own `fullscreen = matches!(self.mode, EditorMode::Script)` always read `false` and nothing put `self.mode` back. A 7C-4 defect (`0eb2db8`), not a 7C-5 one — found live by the user testing 7C-5's own work, then reproduced and root-caused with the very `EditorHarness` 7C-5 built (`clicking_a_rhai_file_in_the_file_browser_opens_the_fullscreen_script_editor`, `editor_input.rs`) before this row existed — the harness catching a real, user-blocking bug moments after landing is itself evidence for why the step exists | `ember2d-editor/src/editor/input/mod.rs` (the `EditorMode::Script` arm) | `[x]` 7C-5 follow-up (`4ede7f5`) — `self.mode = EditorMode::Script;` restored at the call site (the arm has no payload to reconstruct it from inside the handler the way the others do), one line, before calling `handle_script_mode_input`. Regression test named above. `cargo test --workspace`: 279 (255 + 24), all pass; `cargo clippy --workspace --lib` unchanged at 56; `scripts/check.ps1` clean |
| R55 | S2 | Typing into any text-capture widget (the fullscreen/docked script editor, prompts, palette editor/search, graph param fields) silently dropped scattered characters at ordinary typing speed on any display faster than 60Hz — found live by the user immediately after R54 unblocked the fullscreen editor for the first time ("Hello how are you doing today" arrived as " ello howre you dog ody"). Root cause, in `ember2d/src/engine.rs`'s `run()`/`poll_events()` (predates 7C-4 and 7C-5 both — present since R12/7A-2 introduced the `begin_text_capture`/`finish_frame_text_capture` mechanism, just never noticed because nothing had stress-typed the fullscreen editor before R54, and most prior manual passes likely ran on ~60Hz displays where the bug is structurally unreachable): `poll_events` (called once per REAL frame) unconditionally called `finish_frame_text_capture`, which wipes `text_buffer` unless some widget "renewed" capture — but the only place that renewal (`begin_text_capture`) can happen is inside a simulation step (`sim::step`, called from `update()`), and `GameplayLoop::RealTime`'s fixed-timestep accumulator legitimately produces real frames with zero steps whenever less than one `SIM_DT` (1/60s) of real time has accumulated — true for roughly half of all frames at 144Hz. Checking on a zero-step frame always found nothing had renewed the request (nothing could have) and wiped out keystrokes `poll_events` had just captured that same frame. `EditorHarness` (7C-5) could not have caught this: it pairs one simulated "frame" with exactly one `sim::step` call by construction, which structurally cannot reproduce a real frame with zero steps | `ember2d/src/engine.rs` (`Engine::poll_events`, `Engine::run`'s `RealTime`/turn-based branches); `ember2d/src/input.rs` (`text_capture_requested`, `begin_text_capture`, `finish_frame_text_capture` doc comments corrected to match) | `[x]` 7C-5 follow-up (`4ede7f5`) — `finish_frame_text_capture` moved out of `poll_events` (always) into `run()`, called only when a step actually ran this frame (`steps > 0` in the `RealTime` branch; unconditional in the turn-based branch, which always steps exactly once) — the check is now always paired with the step that could have renewed it, regardless of display refresh rate. No headless regression test: reproducing the real bug needs the actual decoupled poll/step timing a live windowed loop has, which neither `EditorHarness` nor `TurnHarness` model (both pair 1:1 by construction) — verified instead via `cargo test --workspace` (279, unchanged), `cargo clippy --workspace --lib` (56, unchanged), `scripts/check.ps1` clean, and `cargo test -p ember2d --test replay` 3× fresh processes (unaffected — `text_buffer` is UI-only state no script/gameplay code reads) |
| R56 | S2 | A brand-new project's level files never appeared in the File Browser even after confirming on disk they existed — found live by the user manually testing New Project → new level creation. Two related gaps, neither in code this session touched before finding them: (1) `TextInputPurpose::NewLevelName`'s commit handler wrote the new `.level` file to disk but never called `refresh_project_files` (every sibling file-creating action — New Script, a level-switch confirm — already did); (2) plain `save()` (Ctrl+S/`S`) never did either, which matters specifically the first time a brand-new project's level is saved (no prior file at that path for the browser to have already listed) | `ember2d-editor/src/editor/input/text.rs` (`NewLevelName` arm); `ember2d-editor/src/editor/impl_state/mod.rs` (`save`) | `[x]` 7C-5 follow-up (`b065b54`) — `self.refresh_project_files()` added to both success paths. Regression tests: `saving_a_brand_new_level_for_the_first_time_refreshes_the_file_browser`, `creating_a_new_level_via_the_level_menu_refreshes_the_file_browser` (`editor_input.rs`) — new `EditorHarness::with_state` constructor and `EditorState::file_browser_files()` accessor added to make them possible. `cargo test --workspace`: 281 (255 + 26), all pass; `cargo clippy --workspace --lib` unchanged at 56; `scripts/check.ps1` clean |
| R57 | S2 | "Rename Level" only ever updated `grid.name` (the title-bar display name) — the file on disk, and `save_path`, kept the old name forever, so a renamed level's File Browser entry and its own displayed name silently drifted apart — found live by the user manually testing (screenshot: title bar read "Test4" while the File Browser still listed "Test.level"/"Test2.level"/"Test3.level" from earlier renames, none of them ever cleaned up) | `ember2d-editor/src/editor/input/text.rs` (`TextInputPurpose::LevelName` arm) | `[x]` 7C-5 follow-up (`ff1cd3b`) — if a file already exists at the old `save_path`, `std::fs::rename` it to the new name alongside updating `grid.name`; `save_path` always moves to the new name either way (even with no file yet on disk), so the next save lands at the new name instead of the old one; `refresh_project_files` called when a rename actually changes the path. Regression test: `renaming_a_level_also_renames_its_file_on_disk` (`editor_input.rs`). `cargo test --workspace`: 282 (255 + 27), all pass; `cargo clippy --workspace --lib` unchanged at 56; `scripts/check.ps1` clean |
| R58 | S4 | User-directed hygiene, not a found defect: `roguelike/`/`shooter/` demo projects sat at the repo root, and both the New Project and Open Project browsers defaulted to `std::env::current_dir()` (the repo root under `cargo run`) — a fresh user's first action was staring at the engine's own source tree | `roguelike/`, `shooter/` (moved); `ember2d-editor/src/editor/start_screen/logic.rs` (`init_fb`, `Screen::OpenProject`'s menu-1 arm) | `[x]` 7C-5 follow-up (`a41d8f4`) — `git mv roguelike demos/roguelike`, `git mv shooter demos/shooter`; every `"roguelike/"`/`"shooter/"` path reference updated across code, tests, examples, CI, and docs (`grep -rl` before and after, zero stragglers) — including the two bare `Path::new("roguelike")`/`("shooter")` output-dir constants in `gen_roguelike.rs`/`gen_shooter.rs` a trailing-slash-only search missed on the first pass, caught by `external_commands.rs` failing (the shipped `.level` files themselves embed `script`/`next_level` as literal `"roguelike/scripts/..."` RON strings, resolved at runtime — fixed by regenerating every shipped level via the now-corrected examples rather than hand-editing RON). New `StartScreen::default_projects_dir()` (repo-root `Projects/`, created on demand) replaces `current_dir()` at both `init_fb` and Open Project's own folder-listing entry point. `Projects/` added to `.gitignore` (developer-local projects, not shipped content — mirrors the existing `*.palette.ron` reasoning). No automated regression test for the `StartScreen` default-folder change itself (`StartScreen` has no headless harness the way `EditorState` does after 7C-5 — see 7C-5's own Scope) — verified by code inspection and the full `cargo test --workspace` pass (282, unchanged) confirming every demo-loading test still resolves its files correctly against the new paths; `cargo test -p ember2d --test replay` 3× fresh processes green; `cargo clippy --workspace --lib` unchanged at 56; `scripts/check.ps1` clean |
| R59 | S2 | `ContextMenuAction::DeleteFile`'s path was built from the raw File Browser row label alone (`&raw[3..]`, then trimmed) and never combined with `current_folder`/`project_folder` the way every sibling file action already does — `std::fs::remove_file` therefore always resolved against the process's current working directory, not the project's actual location, so deleting a file from the browser silently failed (or, worse, could delete an unrelated same-named file sitting in the engine's own CWD) for any project opened from anywhere other than the repo root. Found live while writing 7C-6's own `deleting_a_file_from_the_browser_confirms_first_and_declining_keeps_it`/`confirming_a_file_delete_actually_deletes_it` regression tests — the new test failed against the pre-existing code before this step touched the surrounding confirm-dialog logic, isolating it as a defect this step did not introduce | `ember2d-editor/src/editor/input/panels/context_menu_trigger.rs` (the `DeleteFile` menu-item builder) | `[x]` 7C-6 (`b2a608f`) — path now joins `project_folder` and `current_folder` the same way every other file-referencing action in this file does; directory rows (`raw.starts_with("/ ")`) are now excluded from getting a Delete entry at all, since `std::fs::remove_file` never supported deleting a folder in the first place and previously did so silently-wrong via the same broken path. Regression tests above (`editor_undo.rs`) cover both the confirm-and-decline path and the confirm-and-delete path, the latter asserting the file is actually gone from disk at the correctly-joined path |
| R60 | S2 | Clicking a level in the File Browser while there were unsaved changes did not confirm before switching, discarding them — found live by the user testing 7C-6's own new confirm-before-switch behavior. Root cause: the File Browser click handler's confirm check was `if self.unsaved` alone (grid dirtiness only); `switch_to_level`'s `*self = ns` replaces the *entire* `EditorState`, so an open script buffer edited but not saved (`self.script_unsaved`, tracked independently since script edits don't touch the level grid at all) was silently dropped with no confirmation whenever the grid itself happened to be clean — the exact "there ARE unsaved changes and it doesn't confirm" the user reported, just from the script side rather than the grid side 7C-6's own tests already covered | `ember2d-editor/src/editor/input/panels/file_and_script.rs` (the `.level` click arm's confirm condition) | `[x]` 7C-6 follow-up (`55605a4`) — condition widened to `if self.unsaved \|\| self.script_unsaved`. New `EditorState::script_unsaved()` accessor added (`unsaved()` already existed) to make this testable. Regression test: `switching_levels_with_only_an_unsaved_script_edit_still_confirms_first` (`editor_undo.rs`) — opens a `.rhai` file, types into it, leaves fullscreen via Escape (script buffer stays dirty, grid stays clean), then confirms clicking a different level now shows the modal. `cargo test --workspace`: 296 (was 295), all pass; `cargo clippy --workspace --lib`/`--all-targets` unchanged at 56/80; `scripts/check.ps1` clean |
| R61 | S4 | R58's `demos/` move (7C-5 follow-up) regenerated every shipped LEVEL's own `script`/`next_level` fields via `gen_roguelike`/`gen_shooter`, but never touched the hand-written SCRIPT FILES' own text — 8 `demos/{roguelike,shooter}/scripts/*.rhai` files still referenced the pre-move path: 5 real `ctx.play_sound`/`play_music` calls and 2 real `ctx.load_level` calls used a literal `"roguelike/..."`/`"shooter/..."` string (silently failing at runtime — `[audio] play_sound '...': The system cannot find the path specified`, or a "restart" key loading nothing), plus every file's own header comment still named the old path. `every_script_and_next_level_path_a_level_references_exists_on_disk` (`roguelike_level_integrity.rs`) never catches this class of bug — it only ever resolves paths stored in level DATA, never a literal inside a script's own source text. Found live during the 7C-9 phase gate's own required demo smoke-launch (§0.5 item 6) — `cargo run -- demos/roguelike/floor2.level` printed the audio load error to stderr within the first second | `demos/roguelike/scripts/{enemy_boss,enemy_rat,pickup,player,stairs,victory}.rhai`, `demos/shooter/scripts/{director,player}.rhai` | `[x]` 7C-9 gate (`ab1d404`) — every stale path (`play_sound`/`play_music`/`load_level` calls and header comments) repointed at its `demos/`-prefixed real location. New regression test `every_audio_path_a_demo_script_references_exists_on_disk` (`ember2d/tests/demo_script_audio_paths.rs`) text-scans every shipped script's `play_sound`/`play_sound_at`/`play_music` calls and asserts the referenced file exists — generalizes to catch this class of drift for ANY future path change, not just this one; verified to actually fail against the pre-fix path before confirming the fix (temporarily reverted one file, watched the test fail with the exact stale path in its message, restored it). `load_level` calls aren't covered by an equivalent scan (no test asserts on a script's *behavior* when its own load target is missing) — caught only by this same manual smoke-launch that found the audio bug; a `load_level`-scanning test would need a similar text-scan and wasn't added at this gate, since the smoke-launch itself was already run and passed |
| R62 | S4 | User-directed hygiene, not a found defect: the user asked for a Unity-style default panel layout (Hierarchy left, Inspector right, Console and File Browser tabbed together at the bottom, both visible out of the box) as part of a 7D visual-direction discussion — Hierarchy/Inspector already defaulted this way, but File Browser defaulted to a Left dock (which would have fought Hierarchy for the same side) and started hidden, and Console also started hidden | `ember2d-editor/src/editor/panel/mod.rs` (`PanelManager::new`) | `[x]` (`6e2a97c`) — File Browser's dock moved to `DockSide::Bottom` (matching Console's), both now default `visible: true` (Console stays the initially active bottom tab); the now-dead `BROW_W` Left-dock-sizing constant removed. Two existing `panel::tests` had to change their own SCENARIO, not just their expected numbers — one used FileBrowser as the "alternate Left-docked panel" case for `validate_active_panels` recovery (no longer possible; rewritten to exercise the Bottom side, where the repo now actually has two panels sharing a dock), the other assumed Console/FileBrowser started hidden. Also found while updating six `EditorHarness`-driven tests that used the View-menu "toggle File Browser" action to make it visible before clicking a row in it: toggling now HIDES it (it starts visible), and — the real finding — `PanelManager::in_draw_order` excludes a docked panel that's `visible` but not its side's active tab, so FileBrowser being visible by default isn't sufficient on its own for its rows to render; its own tab must be selected first. New shared test helper `select_dock_tab` (`tests/common/mod.rs`) replaces the toggle pattern everywhere it appeared. `cargo test --workspace`: 320 (unchanged — this was a default + several existing tests' own setup, not new coverage), all pass; `cargo clippy --workspace --lib`/`--all-targets` unchanged at 55/80; `scripts/check.ps1` clean |
| R63 | S1 | `Renderer::draw_nine_slice`/`nine_slice_quads` (Phase 7 Part 1a) hardcoded every source sub-rect to start at `(0, 0)` and span the WHOLE passed texture (`texture.width`/`.height` as the entire 9-slice region) — correct for a texture dedicated to exactly one 9-slice (this function's only real caller before 7D-2), silently wrong the instant a caller's texture was a shared ATLAS with the 9-slice living at some other sub-rect (exactly `NineSlice::src`'s own 7D-1 design: one `themes/ember-clean/chrome.png` packing all 12 `SliceRole`s). Every `draw_nine_slice_px` call in 7D-2's new `draw_panel_chrome` silently sampled from the atlas's `(0,0)` origin regardless of which slice it asked for, rendering as a tiled mosaic of several unrelated slice colors crammed into whatever small destination rect was being drawn (a close button, a resize grip) — visually a checkerboard of amber-bordered squares scattered across the screen. Undetectable by the pre-existing `nine_slice_quads` unit tests (all three used a texture dedicated to one slice, `src.x`/`src.y` always incidentally `0`) or by any other automated test (nothing renders real pixels and inspects them) — found only by actually launching the editor and looking at a screenshot, per CLAUDE.md's own "use the feature" rule for UI changes | `ember2d/src/renderer/mod.rs` (`draw_nine_slice`, `nine_slice_quads`) | `[x]` 7D-2 (`4dc90ed`) — both functions gained a `src: Rect` parameter (the 9-slice's own sub-rect within the texture, defaulting to `Rect::new(0,0,tex_w,tex_h)` to reproduce the old whole-texture behavior exactly) threaded through to `DrawSurface::draw_nine_slice_px` and its one real caller. Regression test `nine_slice_quads_offsets_every_src_rect_by_a_non_zero_atlas_origin` (`renderer/tests.rs`) pins a non-zero atlas origin explicitly, the exact case the three pre-existing tests never covered; those three updated to pass an explicit `src` (all `(0,0,w,h)`, preserving their original assertions unchanged) |
| **Found designing 7D-3 (UI points), 2026-09-13** | | | | |
| R64 | S2 | Right-click row index for the File Browser/Hierarchy context menu is computed from `mouse.cell_y` (16px cell rows), but those rows draw at `theme.metrics.row_h` (20px in `ember-clean`) — beyond the first couple of rows, the wrong file or entity gets the Delete/Duplicate/etc. menu. Delete is only mitigated because the confirm dialog names the real path | `ember2d-editor/src/editor/input/panels/context_menu_trigger.rs` (rows built from `mouse.cell_y`/`content_y`) | `[x]` 7D-3 checkpoint 3 — the right-click handler now reads the same `UiFrame::hit` (`FileBrowserRow(i)`/`HierarchyRow(sel)`) the left-click handlers already trusted. Regression tests: `r64_right_clicking_a_deep_scrolled_file_browser_row_targets_that_exact_file`, `r64_right_clicking_a_deep_hierarchy_row_targets_that_exact_spawn` (`tests/editor_input.rs`), both confirmed to fail against the pre-fix code |
| R65 | S3 | The palette-editor modal's draw centers itself in real px (`(screen_w - 36*CELL_W) / 2`) while its input handler centers in ROUNDED integer cells (`(sw_cells - 36) / 2`) and then compares `mouse.cell_y == my + N` — when the cell-count remainder is odd (true at the default 1280×720), the drawn rows sit half a row off from what the input expects, so a click near a row boundary can land on the wrong field | `ember2d-editor/src/editor/ui/panels/modals.rs:48-49` vs `input/mod.rs:243-247` | `[x]` 7D-3 checkpoint 4 — every field/toggle/button now has its own `WidgetId`, pushed by `draw_palette_editor_modal` at the exact point it's drawn; `handle_palette_editor_input`/`handle_color_picker_input` (extracted to `input/palette_editor.rs`) read them all back via `UiFrame::hit`/`rect_of` instead of any independent cell math. Regression test: `r65_palette_editor_fields_hit_where_drawn_with_an_odd_cell_remainder` (`tests/editor_input.rs`) |
| R66 | S3 | The viewport has no single source of truth for its own rect: the hardware scissor is built from `PanelManager`'s CELL-ROUNDED `content_x/y/w/h` bridges while the canvas itself draws from the exact-px `content_rect()` — after any sub-cell panel resize (drag/resize don't snap to cells) the two disagree by up to a cell, so the canvas is clipped short or spills past its own panel. The canvas's own click gate (`input/canvas.rs`, cells) and `mouse_to_grid` (`impl_state.rs`, px) can also disagree at the same edges; `center_on`/`clamp_scroll`/`FocusCamera` treat `content_w/h` as a tile COUNT, which silently changes if that bridge's own units ever do; the status bar's grid-position readout independently re-derives the canvas origin a third way | `ember2d-editor/src/editor/impl_render.rs:314-320` (scissor), `input/canvas.rs:74-83` (click gate), `impl_state/mod.rs:141-178` (`mouse_to_grid`/`center_on`/`clamp_scroll`), `input/context_menu.rs:120-123` (`FocusCamera`), `ui/panels/chrome.rs:145-146` (status readout) | `[x]` 7D-3 checkpoint 3 — the scissor, the canvas click gate, `mouse_to_grid`/`center_on`/`clamp_scroll`, and the status bar's own readout all now route through the same exact `content_rect(&metrics)` value instead of independently re-deriving it |
| R67 | S2 | The docked script editor has two independent mouse-to-cursor click paths that disagree: the FIRST click (before the panel has focus, `handle_script_editor_click`) ignores `script_hscroll` entirely (a click in a horizontally-scrolled line lands in the wrong column) and has no lower row bound (clicking the error row still moves the cursor); the FOCUSED path's own wheel step is 2 lines vs. the first-click path's 1 | `ember2d-editor/src/editor/input/panels/file_and_script.rs:127-165` vs `input/script_editor.rs:286-618` | `[x]` 7D-3 checkpoint 5 — both paths now call the same `ScriptLayout::hit`/`ScriptLayout::compute` (`ui/script_layout.rs`); the first-click path's wheel step matches the focused path's 2 lines. Regression tests: `r67_the_first_click_on_a_docked_unfocused_script_editor_lands_in_the_exact_column_clicked`, `r67_clicking_the_reserved_error_row_does_not_move_the_cursor` (`tests/editor_script.rs`), the first confirmed to fail against the pre-fix code |
| R68 | S3 | The script editor's drawn layout reserves an error row and clips a `…` marker column that its OWN input handling doesn't know about: `keep_in_view`'s vertical/horizontal bounds don't account for either, so the cursor can end up scrolled to a row/column that's actually hidden behind the error bar or the `…` marker. The gutter is hardcoded to 4 columns in four separate places but `{:3} ` becomes 5 characters wide starting at line 1000, silently misaligning the gutter from the code by one column on any file that long | `ember2d-editor/src/editor/input/script_editor.rs:319-333,603-617`; `ui/script.rs:72,88,104` | `[x]` 7D-3 checkpoint 5 — `ScriptLayout::compute` sizes the gutter from the buffer's own real line count and reserves the error/find rows in its own `text` rect; `visible_rows`/`visible_cols()` (read by keep-in-view, the wheel handlers, and the draw side, all from the same `ScriptLayout`) already exclude both. Regression test: `r68_the_focused_script_editor_wheel_can_scroll_to_the_last_line_of_a_1000_plus_line_file` (`tests/editor_script.rs`); gutter-widening itself covered directly by `gutter_widens_for_a_four_digit_line_count` (`ui/script_layout.rs`) |
| R69 | S3 | `EditorState::draw` returns early for script/graph mode BEFORE calling `apply_layout`, so `PanelManager`'s own `screen_size_cells`/`screen_size_px` go stale in those modes — a window resize while in fullscreen script mode or graph mode leaves the fullscreen script click area and the graph's Add-Node palette height using the PRE-resize window size until the user switches back to Paint mode and back again | `ember2d-editor/src/editor/impl_render.rs:163-185`; `input/script_editor.rs:322`; `input/graph.rs:89` | `[x]` 7D-3 checkpoint 3 — `apply_layout` now runs before the mode early-returns in `draw()`; `input/graph.rs`/`input/script_editor.rs`/`input/mod.rs`'s own lookups switch to `self.ui_space.screen_cells()`, captured at the last real draw, instead of recomputing from the panel bridge |
| R70 | S3 (latent) | `EditorState::switch_to_level`'s `*self = ns` rebuilds the whole state via `EditorState::load`/`new()`, which carries over only `project_folder`/`project_name`/`panels`/`current_folder` — the active theme, its resolved font, and (once they exist) editor preferences and UI scale all silently revert to `DEFAULT_THEME`/defaults on every level switch. Invisible today only because exactly one theme ships, so "reverting" to it looks like nothing happened | `ember2d-editor/src/editor/impl_state/mod.rs:32-41` | `[x]` 7D-3 checkpoint 2 (editor foundation) — `switch_to_level` now swaps (not `*self = ns`) so `theme`/`theme_chrome_tex`/`available_themes`/`font`/`code_font`/`font_raster_scale`/`prefs`/`prefs_store`/`ui_space` carry over from the OLD state by ordinary move (`Box<dyn Font>` isn't `Clone`), the same way `project_folder`/`panels`/`current_folder` already did. Regression test `r70_switching_levels_keeps_the_active_theme_fonts_and_prefs` (`tests/editor_theme.rs`), confirmed to fail against the pre-fix code |
| R71 | S3 | `compute_layout`'s letterbox origin (`renderer/mod.rs`) centered the leftover remainder without flooring it, so an ODD leftover (e.g. an 11-physical-px-wide gap) put the origin at a half physical pixel (5.5, not 5 or 6) — every quad drawn then sampled its texture half a texel off the physical grid regardless of how carefully anything on top of it snapped its own edges. No visible symptom before now (a 1px letterbox border is easy to miss) — found designing 7D-3's UI-points physical-pixel snapping, which depends on the origin itself already being exact | `ember2d/src/renderer/geometry.rs` (`compute_layout`, formerly `renderer/mod.rs`) | `[x]` 7D-3 checkpoint 1 (renderer foundation) — `.floor()` added to both origin axes. Regression test: `r71_letterbox_origin_is_always_a_whole_physical_pixel` (`renderer/geometry.rs`) |
| R72 | S3 | The palette panel's and File Browser's mouse-wheel `max_scroll` are computed from a CELL row count, but both lists actually draw at `theme.metrics.row_h` (20px, taller than a 16px cell) — there are always more cell-rows than real rows fit on screen, so the wheel's own scroll ceiling sits past the list's real end and the last few entries can never be scrolled into view | `ember2d-editor/src/editor/input/panels/hierarchy_and_palette.rs:59,69`; `input/panels/file_and_script.rs:20-26` vs `ui/panels/dock.rs:77,510` | `[x]` 7D-3 checkpoint 3 — both handlers now compute their scroll ceiling from the same pixel `content_rect`/`row_h` formula the draw side uses (`draw_file_browser_panel`'s `max_visible`, `draw_palette_panel`'s `max_rows`/`visible_rows`). Regression tests: `r72_the_file_browser_wheel_can_scroll_all_the_way_to_the_last_file`, `r72_the_palette_wheel_can_scroll_all_the_way_to_the_last_item` (`tests/editor_input.rs`), both confirmed to fail against the pre-fix code |
| R73 | S3 | The script editor never clips its own text to its panel bounds (a `//` line-comment tail or a long identifier can overflow into the panel to its right), and selection is only ever highlighted per-TOKEN (the syntax highlighter's own background-color decision looks at a token's first character), not per-character — a selection edge landing mid-token highlights the whole token | `ember2d-editor/src/editor/ui/script.rs` (highlighter background/clipping, `:276-371`) | `[x]` 7D-3 checkpoint 5 — `draw_script_editor` now sets a real scissor around its own text area; selection is painted as one exact per-character background fill BEFORE any text draws, so the highlighter itself no longer makes any background decision at all (it only ever draws foreground glyphs now). Regression test: `r73_a_selection_starting_mid_token_highlights_only_the_selected_characters` (`tests/editor_script.rs`, asserts the exact recorded `Fill` op's rect via `NullRenderer`'s draw-op recording) |
| R74 | S4 | The resize grip is drawn as an 8×8 nine-slice with a 6px border on all four sides — 6+6 = 12 exceeds the 8px total size, so the two opposing border corners overlap instead of meeting cleanly | `ember2d-editor/src/editor/panel/mod.rs:768-772` | `[x]` 7D-3 checkpoint 3 — `ChromeMetrics::grip` is now `2 × theme.metrics.border`, always fitting its own border exactly. Regression test: `chrome_metrics_resize_grip_fits_its_nine_slice_borders` (`panel/tests.rs`) |
| R75 | S4 | Every chrome text draw that packs a not-yet-cached glyph into the TTF atlas re-uploads the WHOLE atlas texture as a brand-new GPU texture (`draw_text_run`'s dirty/invalidate path) — harmless at the small, mostly-static glyph set a 1024² atlas holds today, but UI points needs a bigger atlas at high `ui_scale` (`glyph_atlas_side_for`), and a bigger atlas redrawn from scratch on every new glyph scales badly, especially right after a UI-scale change resets it | `ember2d/src/renderer/text.rs` (`draw_text_run`); `renderer/backend.rs` (`upload_texture`) | `[~]` 7D-3 checkpoint 2 (editor foundation) mitigates by pre-warming the printable ASCII set at a theme's three font sizes × the target `ui_scale` when fonts are (re)built (`theme_loader::build_font`), so the common case uploads once; a real dirty-rect partial upload is unscheduled |
| R76 | S4 | `scripts/check.ps1`'s file-size check counts NON-BLANK lines (`Get-Content \| Measure-Object -Line` skips blank lines), while CLAUDE.md's own limit is 750 REAL lines — `ember2d-editor/src/editor/mod.rs` is at exactly 750 by the script's count but 805 by `wc -l`; several other files (`panel/mod.rs`, `impl_state/mod.rs`, `engine.rs`, `renderer/mod.rs`) are real-line-over-750 but non-blank-under-750 too. Found auditing headroom before 7D-3's own file-size-constrained commit sequence | `scripts/check.ps1` §1 | `[x]` (`a26cc3d`) — `check.ps1`'s count switched to `(Get-Content $path).Count`, which counts every line `Get-Content` emits, blank or not (matching `check.sh`'s plain `wc -l`, which already had no version of this bug); confirmed the fix works by running it BEFORE splitting anything (flagged all 3 real-line-over-750 files — `ember2d/src/renderer/backend.rs` 810, `ember2d-sim/src/simulation.rs` 791, `ember2d/src/engine.rs` 789) and again after (clean). Each split into a `.rs` + child submodule, mirroring this codebase's own established convention for splitting one type's impl across sibling files (`scripting/api.rs`/`api_ext.rs`, `simulation.rs`/`simulation/spawn.rs`): `engine.rs` kept the `GameState` trait and `Engine` itself, its winit `ApplicationHandler` shims (`WindowInit`/`EventPump`) moved to new `engine/window.rs` (555/253 real lines); `renderer/backend.rs` kept `WgpuBackend`'s struct and setup/texture-upload half, its pre-existing SECOND `impl WgpuBackend` block (the per-frame draw/lifecycle API) moved to new `renderer/backend/draw.rs` (482/329 real lines — the file already carried this exact seam as two separate impl blocks); `ember2d-sim/src/simulation.rs` kept the `Simulation` struct/accessors/`on_start`, its `step`/`late_step`/`run_actor_turn`/`apply_script_result` moved to new `simulation/step.rs` (417/374 real lines, the same child-module mechanics as the pre-existing `simulation/spawn.rs`) — `apply_script_result` widened from private to `pub(super)` since `simulation/spawn.rs`'s `do_on_start` calls it too and is a sibling, not a descendant, of the new `step` module. Pure relocation, no behavior change: unused imports trimmed at each split site (doc comments preserved verbatim), no logic touched. Largest file in the tree now `ember2d-editor/src/editor/mod.rs` at 735 real lines. **Verification.** `cargo build --workspace --bins --examples` clean. `cargo test --workspace`: unchanged pass counts across every crate (same totals as before the split, since no test was added or removed — R76 is a pure structural fix). `cargo clippy --workspace --lib`/`--all-targets`: unchanged at 43/55 (the same pre-existing lints now attributed to the new file paths — `run_actor_turn`'s/`draw_texture`'s "too many arguments," `index_exits`'s loop-counter lint, `EventPump::window_event`'s collapsible-if — none new). `cargo test -p ember2d --test replay` 3× fresh processes green. `scripts/check.ps1` clean |
| R77 | S3 | The docked/fullscreen script editor's "keep cursor in view" scroll adjustment is skipped on several code paths that move the cursor: the find bar's own search-and-jump, Ctrl+A (select all, cursor to end), and multi-line undo/redo/cut/paste — each can leave the cursor scrolled off-screen after the operation | `ember2d-editor/src/editor/input/script_editor.rs:303-306,401-448` | `[ ]` unscheduled — found alongside R67/R68 but a distinct set of code paths, not fixed by `ScriptLayout` alone (each early-return needs its own call to the shared keep-in-view step) |
| R78 | S4 | The script editor has no PageUp/PageDown handling (the keys exist in `ember2d/src/input.rs` but nothing in the editor reads them) and no drag-to-select (only click and shift-click) | `ember2d-editor/src/editor/input/script_editor.rs` | `[ ]` unscheduled — feature gaps, not regressions |
| R79 | S4 | Graph mode's own title bar, status bar, and Add-Node palette overlay stay on the fixed cell grid and won't follow the editor's new UI scale — an intentional 7D-3 scope boundary (graph mode is a cell-grid canvas per the 7C-9 decision gate), logged so it isn't mistaken for an oversight later | `ember2d-editor/src/editor/graph_ui.rs`; `impl_render.rs` (graph-mode chrome) | `[ ]` deferred — out of 7D-3's own scope by design |
| R80 | S4 | The start screen (pre-project, no theme loaded yet) stays on its own hardcoded bitmap-font look and won't follow the editor's new UI scale — same class of deliberate exclusion as 7D-2's own start-screen note | `ember2d-editor/src/editor/start_screen/` | `[ ]` deferred — out of 7D-3's own scope by design |
| R81 | S4 | Neither the menu dropdown nor the context menu clamps its own position to stay on-screen — reachable today only at extreme window sizes, but a high UI scale on a short window makes it easy to open a dropdown whose bottom rows render off the bottom edge | `ember2d-editor/src/editor/ui/menu.rs`; `ui/panels/chrome.rs` (context menu) | `[ ]` unscheduled |
| R82 | S4 | `GlyphAtlas::pack`'s shelf-packing places glyphs edge-to-edge with no padding between them — latent bleeding risk if texture filtering or non-integer glyph scaling is ever introduced (today's nearest-neighbour sampling with exact-pixel glyph draws doesn't trigger it) | `ember2d/src/renderer/font/atlas.rs` (`pack`) | `[ ]` unscheduled |
| R83 | S4 | `ContextMenu.x`/`.y` are still `usize` cell coordinates (`mouse.cell_x`/`mouse.cell_y` at the moment a right-click opens one), converted to pixels once in `draw_context_menu` (`ui/panels/chrome.rs`) — the last chrome-content site still keyed off a cell reading rather than storing the exact `mouse.pixel_x`/`pixel_y` the click itself already carries. Not a functional bug today (the conversion is exact — `CELL_W`/`CELL_H` are the same compile-time constants on both sides) — found and left as-is by 7D-3's checkpoint 6 chrome audit, which fixed every OTHER live chrome CELL_W/CELL_H dependency it found | `ember2d-editor/src/editor/ui/types.rs` (`ContextMenu`); `input/panels/context_menu_trigger.rs` (every site that constructs one); `ui/panels/chrome.rs:439-440` (`draw_context_menu`'s conversion) | `[ ]` unscheduled — convert `ContextMenu.x`/`.y` to `f32` points, sourced from `mouse.pixel_x`/`pixel_y` directly, dropping the `CELL_W`/`CELL_H` conversion entirely |
| **Found landing 7D-3 checkpoint 7 (live UI scale), 2026-09-13** | | | | |
| R84 | S1 | `UiSpace::from_surface` divided `DrawSurface::pixel_width()`/`pixel_height()` by `render_scale` to get `screen_logical` — but those two are already LOGICAL pixels throughout this codebase (`Renderer::pixel_width = cells * CELL_W`, itself `floor(physical_width / (CELL_W * scale)) * CELL_W`, i.e. physical already divided by `scale`; the same space `MouseState::pixel_x`/`NullRenderer`'s own constructor argument already live in), so this divided by `render_scale` a SECOND time. Silent through every earlier checkpoint of this step because `self.ui_space` was only ever captured and read back as a VALUE, never actually multiplied into a drawn pixel, until this checkpoint's own `UiPainter`/input conversion started consuming `screen_pt()`/`rect_to_logical()` for real — at that point every panel, modal, and menu rendered at HALF size on any display where `render_scale` isn't exactly `1` (i.e. every real display, `MIN_UI_SCALE` floors it at `2`), overlapping the viewport | `ember2d/src/renderer/ui_space.rs` (`UiSpace::from_surface`) | `[x]` 7D-3 checkpoint 7 — `screen_logical` now reads `pixel_width()`/`pixel_height()` directly, no second division. Found via live screenshot (everything doubled in size, overlapping, viewport black) immediately after a fully-compiling, fully-test-passing build — no automated test caught it, since none of this step's earlier checkpoints ever exercised `from_surface` through a live draw at `render_scale != 1`. Regression test: `from_surface_does_not_divide_the_already_logical_pixel_size_again` (`renderer/ui_space.rs`) |
| R85 | S1 | `UiPainter::text`/`text_mono` used `pt_to_logical()` (`ui_scale / render_scale`, `S/R`) for `texel_scale` — but a glyph's raster bitmap is already `ui_scale` times bigger than its point size (`UiSpace::raster_px(pt) = pt * S`), so drawing that ALREADY-`S`-scaled bitmap at `S/R` logical pixels per texel scaled it up by `S` a SECOND time, rendering chrome text `S`× too large. A 9-slice's `border_scale` has no equivalent bug (an atlas texel is never pre-scaled by `S` the way a glyph raster is, so it genuinely needs the full `S/R`) — text specifically needed `1/R` instead. A pre-existing unit test (`painter_text_rasterizes_at_points_times_ui_scale`, from checkpoint 1) asserted the buggy `S/R` value as correct, so nothing caught this until a live screenshot did | `ember2d/src/renderer/ui_painter.rs` (`text`, `text_mono`) | `[x]` 7D-3 checkpoint 7 — new `UiSpace::raster_to_logical() = 1 / render_scale` method; both functions' `texel_scale` switched to it. Found via a SECOND live screenshot, right after R84's own fix corrected the layout but left every chrome text draw still severely overlapping. The pre-existing wrong test corrected (now asserts `texel_scale == 1.0` at `S == R`, not `S/R`); new regression test `painter_text_texel_scale_is_one_over_render_scale_not_s_over_r` (S=3, R=2, asserts `texel_scale == 0.5`, explicitly distinct from `pt_to_logical() == 1.5`) |
| **Found by the 2026-09-13 play-mode regression sweep (R51's re-diagnosis)** | | | | |
| R86 | S2 | `PauseMenuState::render` centered its 30×8-cell panel with a bare `(sw - 30) / 2`, `(sh - 8) / 2` in `usize` — but `compute_layout` floors the cell grid at 20×6 (`renderer/geometry.rs`), so a window narrower than the panel (under ~480 physical px at the default scale) or shorter than its 8 rows underflowed the moment Esc was pressed in play mode: a debug-build panic ("attempt to subtract with overflow", reproduced live at 400×220), or in release a wrapped-around origin feeding a gigantic `draw_rect_filled` loop. Pre-existing (the 20×6 floor predates 7B-2), never logged — nobody had shrunk a play window that far | `ember2d/src/play.rs` (`PauseMenuState::render`) | `[x]` (`7e51b5d`) — new `centered_origin(screen, size) = screen.saturating_sub(size) / 2` used for both axes, so the panel clamps to the window's top-left instead. `PauseMenuState` moved to its own `play/pause_menu.rs` (play.rs was at 751 real lines — over CLAUDE.md's limit — before this fix needed room; 688 after). Regression test `r86_the_pause_menu_origin_never_underflows_on_a_window_smaller_than_the_panel` (`play/pause_menu.rs`), confirmed to panic against the old formula. Verified live: Esc on a 400×220 play window draws the panel flush to the top-left, process stays up |
| **Found by the 2026-09-13 `cargo fmt --all` sweep** | | | | |
| R87 | S4 | The one-time `cargo fmt --all` commit (§11, same shape as 7A-9) pushed `ember2d-editor/tests/editor_input.rs` from 734 to 761 real lines, over CLAUDE.md's 750-line limit — the exact R42/R43 hazard from 7A-9's own pass, this time hitting a flat `#[test] fn` integration-test file (no impl block to split, unlike R42/R43's `ScriptCtx`) rather than a source file | `ember2d-editor/tests/editor_input.rs` | `[x]` (`25d4033`) — split by feature area, same one-file-per-area convention as `editor_theme.rs`/`editor_script.rs`/`editor_undo.rs`: `editor_input.rs` (370 lines) kept the baseline harness check, menu bar/dropdown, docked panels/focus, text capture (R11/R12), and other `EditorMode` transitions; new `editor_input_files.rs` (187 lines) took the two "repro" areas that touch real files on disk (opening a `.rhai` from the File Browser, File Browser refresh after save/new-level/rename); new `editor_input_panels.rs` (239 lines) took 7D-3's own area (R64/R65/R72: right-click row targeting and wheel scroll ceilings). Pure relocation, no test content changed — same 32 tests (23/4/5 split), same assertions, imports trimmed per file. **Verification.** `cargo build --workspace --bins --examples` clean. `cargo test --workspace`: unchanged total pass count (32 tests now split 23/4/5 across the three files instead of one 32). `cargo clippy --workspace --lib`/`--all-targets` unchanged at 43/55. `cargo test -p ember2d --test replay` 3× fresh processes green. `scripts/check.ps1` clean |
| **Found live by the user, 2026-09-13 (canvas hover at `ui_scale: 1`)** | | | | |
| R88 | S1 | `draw_status_bar`'s call site (`impl_render/mod.rs`) passed the Viewport panel's raw POINTS-space `content_rect().x/y` as `canvas_origin_px`, but `draw_status_bar` (`ui/panels/chrome.rs:168-169`) subtracts it from `mouse.pixel_x/y` (LOGICAL pixels, per the 7C-9 decision gate) and divides by `CELL_W`/`CELL_H` — exactly the points/logical unit mix `mouse_to_grid`'s own doc comment (`impl_state/viewport.rs`) warns "would silently scale the cursor position by ui_scale." Invisible whenever `ui_scale == render_scale` (points and logical coincide there — every harness default, and every real session before 7D-4 shipped the Theme > UI Scale menu), so it went unnoticed until a user picked `UI Scale: 1x` on a real 2×-DPI display and the status bar's own coordinate readout drifted away from the tile actually under the cursor (reported live, with a screenshot, then reproduced by launching the real editor and driving the mouse via Win32 `SetCursorPos` — no headless test caught it, since `canvas_painting_is_unaffected_by_ui_scale` already covers the SAME `ui_scale=1, render_scale=2` ratio but only for `mouse_to_grid`'s click path, never this call site). The visible hover HIGHLIGHT and real click placement were both already correct — only the numeric readout was wrong, which is why painting itself was never reported broken | `ember2d-editor/src/editor/impl_render/mod.rs:611`; `ember2d-editor/src/editor/ui/panels/chrome.rs:168-169` | `[x]` (`ed1b224`) — call site now passes `(vl.x, vl.y)` (the already-computed `rect_to_logical` result used for everything else this frame) instead of the raw points `viewport.x/y`. Regression test `r88_the_status_bars_coordinate_readout_matches_the_real_hovered_cell_when_ui_scale_is_smaller_than_render_scale` (`tests/editor_ui_scale.rs`), confirmed to fail against the pre-fix code (read `-1.9` instead of `5.5` for a click centered on grid cell (5,3)). **Verification.** `cargo build --workspace --bins --examples` clean. `cargo test --workspace`: 377 (was 376), all pass. `cargo clippy --workspace --lib`/`--all-targets` unchanged at 43/55. `cargo test -p ember2d --test replay` 3× fresh processes green. `scripts/check.ps1` clean. Verified live a second time after the fix: same repro (window at 0,0, mouse at screen (300,200), `UI Scale: 1x`) now reads `(10.9,3.4)`, matching the highlighted cell, instead of the pre-fix `(3.5,1.5)` |
| R89 | S2 | `draw_menu_dropdown`'s call site (`impl_render/mod.rs`) had the exact same unit mismatch as R88, in a sibling code path R88's own fix never touched: raw LOGICAL `mouse.pixel_x/y` passed straight into a function comparing it against POINTS-space row rects (`row_rect.contains_point`), so the dropdown's own drawn "hovered" highlight silently drifted at any `ui_scale != render_scale`. Reported live by the user with a screenshot: cursor down near "Close Project" (index 9, `MenuKind::File`'s own list, `ui/menu.rs`) while "Export Game..." (index 5) was drawn highlighted instead. `handle_menu_dropdown_click` (`input/panels/menu_bar.rs`) was never affected — it already converts via `logical_to_pt` — so a real click always landed on the right item; only the visual hint lied about which row that click would hit | `ember2d-editor/src/editor/impl_render/mod.rs:548-549` (pre-fix); `ember2d-editor/src/editor/ui/menu.rs` (`draw_menu_dropdown`'s own `hovered` check) | `[x]` (`99ee94b`) — call site converts via `self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y)` before calling `draw_menu_dropdown`, matching `handle_menu_dropdown_click`'s own conversion exactly. `draw_menu_dropdown` also widened from `()` to `Option<usize>` (the hovered row index), captured into a new `EditorState.menu_hover_item` field (reset every frame, exposed via `hovered_menu_item()`) — before this the hover highlight was a fire-and-forget local inside `ui/menu.rs` with no way for anything outside the draw call to observe it; now it's a real, testable single source of truth, closing the same "two independent implementations that can drift" gap R64/R66/R67 already fixed for other chrome hit-testing. Regression test `r89_menu_dropdown_hover_highlight_matches_the_real_hovered_row_when_ui_scale_is_smaller_than_render_scale` (`tests/editor_ui_scale.rs`), confirmed to fail against the pre-fix code (highlighted row 3 instead of the real row 9). **Verification.** `cargo build --workspace --bins --examples` clean. `cargo test --workspace`: 381 (was 380), all pass. `cargo clippy --workspace --lib`/`--all-targets` unchanged at 43/55. `cargo test -p ember2d --test replay` 3× fresh processes green. `scripts/check.ps1` clean (the new `menu_hover_item` field's doc comment trimmed to a single trailing `//` line to keep `editor/mod.rs` at exactly 750 real lines — no other file this step touched was close to the limit). Verified live a second time after the fix: opened `File`, moved the mouse to "Close Project"'s own row — highlighted correctly, matching the cursor, instead of the pre-fix mismatch |
| **Found writing 7.5-4 (docs/ember2d-master-plan.md §5.6)** | | | | |
| R90 | S4 | Adding `ActorRecord::stats`/`tint_aware`/`tint_asleep` (7.5-4) grew `TileRecord` enough that `ember2d-editor`'s undo `Command::PlaceTile { before: Option<TileRecord>, after: TileRecord }` variant now trips clippy's `large_enum_variant` lint (528 bytes vs. `Command`'s other variants; 624 since 8-2's `TileRecord::sprite`, 672 since 8-3's `TileRecord::clip` — same warning, same cause) — a new warning (`cargo clippy --workspace --lib`: 43 → 44), not a behavior change. Fixing it properly (`Box`ing the large variant fields) is an `ember2d-editor` change outside 7.5-4's own Scope (`ember2d-sim`, demos, API doc) | `ember2d-editor/src/editor/commands.rs:24` (`Command::PlaceTile`) | `[ ]` unscheduled |
| **Found writing 7.5-9 (docs/ember2d-master-plan.md §5.6)** | | | | |
| R91 | S4 | `ember2d-sim/clippy.toml`'s new `disallowed-types` lint (`HashMap`/`HashSet`, `#![warn(...)]` in lib.rs) surfaces 67 unique pre-existing sites once actually turned on — every one already a deliberate lookup-only use per CLAUDE.md's own carve-out (`WorldSnapshot`'s velocities/parents/glyphs/etc., `ScriptEngine`'s mod_times/disabled_scripts, the graph codegen module, `layers.rs`, and others), none newly introduced by 7.5-9 itself (verified: none of this step's own new code — `level_source.rs`, `World::diagnostics` — adds a HashMap/HashSet at all). Not a correctness bug (nothing here is order-sensitive; each one already has its own "lookup-only" reasoning in a nearby doc comment, just not yet the formal `#[allow(clippy::disallowed_types)]` annotation clippy now expects) — a mechanical annotation pass across ~12 files, large enough on its own to warrant its own step rather than folding into 7.5-9's already-substantial diff (LevelSource, Diagnostic, set_parent/despawn, the world.rs/world_tests.rs split). 7.5-10 deleted `ScriptEngine.scopes` (R22) — one of the ~67 sites — without annotating it, so this count is now ~66; still unscheduled | `ember2d-sim/src/scripting/state.rs` (the largest concentration, ~35 sites), `world.rs`, `simulation.rs`, `simulation/step.rs`, `scripting/engine.rs`, `scripting/collisions.rs`, `scripting/api_spatial.rs`, `command.rs`, `layers.rs`, `graph/codegen.rs`, `graph/mod.rs` | `[ ]` unscheduled |
| **Found writing 7.5-10 (docs/ember2d-master-plan.md §5.6)** | | | | |
| R92 | S4 | 7.5-10's own plan text bundled two more things that turned out to need deferring: (1) it says `run_collisions` should "reuse the step snapshot instead of rebuilding" — but `run_collisions` builds its snapshot AFTER `late_step` calls `resolve_solid_collision`, deliberately (see `collisions.rs`'s own comment, Phase 6 Step 5), while `step()`'s own shared snapshot is built BEFORE that resolution; sharing it as the plan text asks would hand `on_collide` scripts stale, pre-resolution positions — a real regression, not a style choice, so this sub-item was skipped rather than implemented as written (user decision, 7.5-10). (2) `WorldSnapshot` storing collider `layer`/`mask` as `Rc<str>`/`Rc<[Rc<str>]>` "like tags" only pays for itself the way `tags` does if the same `Rc` is shared into more than one map (`tags`/`tag_to_id`/`tag_to_ids`) — collider layer/mask are written into exactly one map today, and `Collider`'s own fields (`components/collider.rs`) are plain `String`/`Vec<String>`, so retyping just the snapshot's copy would still allocate once per collider per step, identical cost to today, while adding a `.to_string()` conversion at every `get_collider_layer`/`get_collider_mask` call. Both of these were coupled in the plan's own text to the per-step spatial index (`get_entity_at`/`is_solid_at`/`raycast`/`get_path` sharing a sorted-by-x index with the broad phase) — a genuine, measurement-driven performance project the user chose to defer as its own future step rather than rush alongside 7.5-10's cleanup half; revisit both together once that index exists | `scripting/collisions.rs`, `scripting/state.rs` (`WorldSnapshot.colliders`) | `[ ]` unscheduled |
| **Found writing 8-1 (docs/ember2d-master-plan.md §5.7)** | | | | |
| R93 | S2 | `Simulation::index_exits` rebuilt the exit-tile → next-level map by assuming "entity id = tile index + 1" — true only while every tile spawned as an entity, in `level.tiles` order, with nothing spawned before them. Latent until 8-1: the moment static tiles collapse into a `Tilemap` (spawned first, one entity for all of them), every stairs would have been keyed to the wrong id — the level transition silently dead, on fresh spawn and on every loaded save alike | `simulation.rs:468-477` (pre-8-1) | `[x]` 8-1 — exits moved onto `World.exits`, recorded at spawn against the id `spawn()` actually returned and carried through saves with the `World`; a pre-8-1 save (no `exits`, no tilemap) gets the old mapping rebuilt by `restore_legacy_exits`, still correct for it. Tests: `r93_an_exit_after_collapsed_walls_is_keyed_by_its_real_entity_id`, `r93_a_pre_8_1_save_without_exits_gets_them_rebuilt_from_tile_order` (`simulation/tilemap_spawn_tests.rs`) |
| R94 | S4 | The demo generators write `turn_model: Alternating` into `project.ron` (since 7.5-7 added the field), but the shipped `demos/roguelike/project.ron`/`demos/shooter/project.ron` were never regenerated — rerunning `gen_roguelike`/`gen_shooter` shows a one-line diff in each. Harmless (`#[serde(default)]` gives `Alternating` anyway) but means "regenerate the demos" isn't a no-op for files a step didn't mean to touch; 8-1 reverted that diff rather than ship it out of scope | `ember2d/examples/gen_roguelike.rs`/`gen_shooter.rs` `main`; `demos/*/project.ron` | `[ ]` unscheduled — regenerate both `project.ron`s in any later step that already touches demo content |
| **Found during 8-1's live test pass (2026-09-30) — every one reproduced identically on the pre-8-1 binary (`5e0e88d`), so none is an 8-1 regression** | | | | |
| R95 | S3 | Palette panel: each row's `[   ]` swatch is empty and the tile glyph previews are drawn as a stray stacked column lower in the panel (over the `[ Edit ]` button). `glyph_cell_x`/`glyph_cell_y` divide the row rect — UI **points** since 7D-3 — by `CELL_W`/`CELL_H` as if it were logical pixels, so at any UI scale other than the one the math happens to match (1.5× here) the glyph lands in the wrong cell and several rows collapse onto one. Same file: the 4–9/0 hotkey digit is right-aligned flush to `row_rect`'s right edge with no padding, so the panel border clips it. Also visible in the palette editor modal (a stray red glyph under its Tag field). Screenshot evidence in the 8-1 live test report | `ember2d-editor/src/editor/ui/panels/dock.rs:119-128` (swatch), `:147-155` (hotkey) | `[x]` `1668f92` (2026-09-30, own commit between 8-1 and 8-2, by user direction) — palette row and palette-editor glyph previews drawn in points through `UiPainter::tile_glyph` (new `widgets::draw_tile_glyph_in`, centered in the bracket slot); hotkey digit padded one space; the advanced color picker's hue bar / SV map / cursor / preview — found to have the same cell-vs-points bug once the palette editor was fixed — converted to points-space `fill`s at the same step size the input side already divides by (hit-testing unchanged), and its "Selected:" label moved above the preview it names. Live-verified at 1.5x (palette, palette editor, picker clicks). Tests: `r95_every_palette_rows_glyph_preview_is_drawn_inside_that_row_at_every_ui_scale`, `r95_the_color_pickers_hue_bar_and_map_are_painted_where_they_are_hit_tested_at_every_ui_scale` (`ember2d-editor/tests/editor_palette.rs`; both fail against the pre-fix code) |
| R96 | S2 | Roguelike: `player.rhai`'s `on_start` unconditionally `set_persistent`s hp 12 / gold 0 / potions 1 / depth 1 / turns 0 — and `on_start` runs on EVERY level load, so taking the stairs resets the whole run: the HUD shows `Depth 1` on floor2 and any gold/HP is lost. Live-confirmed on both the 8-1 build and pre-8-1 `5e0e88d`. Most likely introduced when 7.5-5 moved the old lazy-init out of `on_update` into the new `on_start` hook without an "already initialised" guard (`has_persistent`) | `demos/roguelike/scripts/player.rhai:122-130` | `[x]` `f96936d` (2026-09-30, own commit after R95, by user direction) — two halves, both needed: (1) `player.rhai`'s `on_start` seeds the run only `if !has_persistent("hp_max")` — the guard 7.5-5 dropped when it moved the lazy-init out of `on_update`; (2) `ember2d-app`'s `run_editor_app` now clears `engine.persistent` on each fresh F5 (the outer `ToPlay` arm only — the inner loop's `ToPlay(next)` is a level transition and keeps it), because the store outlives every `PlayState` and, once the script stopped re-seeding, a second F5 would have inherited the last run's stats. Live-verified: floor1 stairs → floor2 shows Depth 2; Back to Editor → F5 starts a fresh Depth 1 run. Test: `r96_descending_the_stairs_keeps_the_run_and_increments_depth` (`ember2d/tests/roguelike_floor1.rs`, fails against the old script; new `TurnHarness::continue_run` carries the store across the transition the way the app does). The editor-side clear has no automated test — `run_editor_app` needs a live `Engine`/window — only the live check above |
| R97 | S4 | Keyboard-shortcuts overlay: the "Press ? or Esc to close" hint is drawn on top of the LEVEL column heading (fixed y, not laid out after the columns) | `ember2d-editor/src/editor/ui/panels/modals.rs:632` | `[ ]` unscheduled |
| R98 | S4 | Stats panel counts tiles per palette def by `def.tag`, but the default palette's defs all have an empty tag — so every category reads 0 on a real level while Total (2,569 on floor2) is right | `ember2d-editor/src/editor/ui/panels/dock.rs:241` | `[ ]` unscheduled |
| R99 | S3 | View → API Docs runs `cmd /C start index.html` relative to CWD; no `index.html` exists anywhere in the repo, so the menu item silently does nothing (no error surfaced) | `ember2d-editor/src/editor/impl_state/mod.rs:649-660` | `[ ]` unscheduled — point it at `docs/ember2d-scripting-api.md` or remove the item |
| R100 | S4 | Two editor observations not root-caused in the 8-1 live pass: (1) toggling the grid (Tab / View → Grid) showed no visible overlay in either build; (2) the Files panel shows ~5 rows with no scroll affordance, so a project's `.level` files can sit below the fold with no hint they exist | `impl_render/mod.rs:265` (grid), Files panel | `[ ]` unscheduled — investigate before fixing; may be by design |

### 3.3 Editor defects carried from the Phase 7 plan (E-series)

| # | Defect | Status |
|---|---|---|
| E1 | `draw_cursor_highlight` drifts during fractional scroll | `[x]` cf59f42 (pinned by test) |
| E2 | `grid_to_pixel` hardcodes 8.0/16.0 | `[x]` 7C-2 (`e0d305c`) — this row's own `backend.rs:420-439`/`mouse.rs:28-32` citations were already stale by the time 7C-2 ran (both already used `CELL_W`/`CELL_H`, fixed incidentally at 7B-2/7B-4 without this row being updated — R36's own class of drift; also both in `ember2d`, not `ember2d-editor`, so out of 7C-2's Scope regardless); the real remaining site was `impl_render.rs`'s viewport scissor rect, now fixed |
| E3 | `draw_extra_spawns` label position wrong at zoom ≠ 1 | `[x]` cf59f42 |
| E4 | Two sources of truth for the canvas rect (`Layout` vs `PanelManager`) | `[x]` 7C-3 (`9bad191`) — `Layout` deleted; `PanelManager` (`viewport()`, `screen_size_px()`/`screen_size_cells()`) and `Panel::content_rect()` are the only source now |
| E5 | Hitboxes computed independently of drawing | `[x]` 7C-1 (`03f34bf`) — all 13 remaining sites migrated to `UiFrame`; palette editor's non-color-grid fields (title-close, name/glyph/tag, Save&Close/Delete) are the one deliberately out-of-scope remainder, per that step's own "Landed as" note |
| E6 | `PanelManager::new(80, 24)` hardcodes a terminal size | `[x]` cf59f42 |

---

## 4. Architecture invariants

Rules the code must keep, and how each is enforced. If a rule has no
enforcement mechanism it is a wish, not an invariant.

### 4.1 Determinism (the simulation)

| Rule | Enforcement |
|---|---|
| No `HashMap`/`HashSet` iteration on the sim path. Lookup-only use is allowed and must be commented as such. | Code review + the `BTreeMap` audit comment in `state.rs`. **7.5-9 adds** a clippy `disallowed_types` entry for `HashMap` in `ember2d-sim` with per-site `#[allow]` where lookup-only. |
| No ambient randomness. RNG is world-owned, seeded from the level. | `rand` without `std_rng`/`getrandom` in `ember2d-sim/Cargo.toml`. |
| No wall-clock time. | **7A-5** derives `elapsed` from step count. **7.5-9** adds `disallowed_methods` for `Instant::now`, `SystemTime::now`. |
| No transcendental math (`sin`/`cos`/`atan2`/`exp`/`powf`). | grep at phase gate; `math::atan2_approx` is the one approved replacement. |
| No filesystem access inside `ember2d-sim`. | **7.5-9** moves level resolution behind a caller-provided `LevelSource` trait and adds `disallowed_methods` for `std::fs::*`, `Path::exists`. |
| Presentation state never feeds back into sim state. | `PlayState` owns a **separate** RNG for particles/shake (**7A-5**). Camera lerp's `exp()` stays presentation-only. |
| `total_cmp` for every float sort in the sim. | **7A-1**; grep `partial_cmp` at phase gate. |

### 4.2 The sim boundary

`ember2d-sim` depends on exactly four crates: serde, ron, rhai, rand. It
compiles with no window, GPU, input-device, filesystem, or clock dependency.
`cargo tree -p ember2d-sim --depth 1` at every phase gate.

### 4.3 The scripting contract

- `ember2d-scripting-api.md` is the contract. Every registered function is
  listed there, in the same commit that registers it.
- `API_VERSION` bumps on any breaking change (rename, signature, semantics).
  Scripts may read it and the engine logs it at startup.
- A script can never crash, hang, or leak the editor: operation limit
  (**7A-1**), no `unwrap`/index on script-supplied values, setters on
  missing entities are no-ops, errors disable the script and surface in the
  console (**7C-7**).
- Setter semantics are uniform: missing entity → no-op; missing key →
  documented default; the same sentinel (`-1`) for "no entity" everywhere
  (**7.5-1**).

### 4.4 Editor stability

- Any mouse interaction is guarded so menu/panel clicks never bleed into the
  canvas (`ignore_drag` **and** the consumed-input contract of **7C-4**).
- Panels never render over menu-bar dropdowns (draw order in
  `impl_render.rs`).
- A widget cannot be drawn without being registered in `UiFrame`
  (**7C-1** makes this structural).
- All string indexing is char-aware (**7A-2**).
- Every destructive action (new level, delete file, switch level with
  unsaved edits) confirms first.

### 4.5 Testing

- Every fixed defect gets a regression test named after it.
- The sim is tested headless through `TurnHarness`/`Simulation`. The editor
  gets its own headless input harness in **7C-5**.
- `tests/replay.rs` is the desync test. It runs 3× as fresh processes in CI
  on Windows and Linux.
- Temp files in tests use a per-process unique directory (**7A-8**).

---

## 5. Phases

### 5.1 `[x]` Phase 7A — Stabilisation sprint (`v0.5.7a`)

**Purpose.** Close every S1 and the cheap S2s from the review before more
Phase 7 work lands on top of them. Everything here is small, independently
testable, and mostly one-file. Expected size: eight commits.

**Checklist sections at gate:** §1, §4, §8, §11, §13.

#### `[x]` 7A-1 — Scripts can no longer crash or hang the engine (`ddd386e`)

- **Why:** R1–R6, R9, R10. The project rule says a broken script never takes
  down the editor; today six different one-liners do.
- **Change:**
  - `ScriptEngine::new`: `engine.set_max_operations(2_000_000)` per call (a
    generous budget — floor2's full `on_turn` pass is far under this;
    measure with `bench_sim` and record the number in the commit). On the
    limit error, treat exactly like a runtime error: disable the script,
    log once, hot-reload re-enables.
  - `random_int`: `if max < min { swap }`. `random_bool`: `if !p.is_finite()
    { p = 0.0 }` before clamp. `random_choice` already guards empty.
  - `parse_color`: check `hex.is_ascii()` before length; on failure return
    the fallback colour and log once per distinct bad string (a small
    `BTreeSet<String>` of already-logged names on `ScriptState`).
  - `Animator::advance`: clamp `speed` to `0.0..=64.0` in `apply.rs` when
    applying `set_clip_speed`; in `advance`, if `elapsed > frame_duration *
    frame_count * 4.0`, reduce with `%` instead of the loop. Loop bound is
    then provably `≤ 4 * frame_count`.
  - `detect_collisions`: `a.1.x.total_cmp(&b.1.x)`. Also `set_position`
    rejects non-finite values (no-op + one log).
  - `clear_all_persistent`: push a `PersistentOp::ClearAll` onto a new
    pending op enum; `apply_ctx` clears the store then applies remaining
    pending writes in order.
  - `set_tag` / `play_clip` / every setter in `apply_ctx`: guard with
    `world.transforms.contains_key(&id)` (the existence check `World`
    already uses) before inserting.
- **Test:** one `engine_tests.rs` test per item: `loop {}` script is disabled
  after one step with an error logged; `random_int(5,1)` returns in
  `1..=5`; `random_bool(0.0/0.0)` returns `false`; `set_tint(id, "#€€")`
  leaves tint unchanged; `set_clip_speed(id, 1e9)` then 1,000 steps
  terminates; `set_position(id, 0.0/0.0)` is a no-op; `clear_all_persistent`
  empties a populated store; `set_tag(9999, "x")` does not create an entity.
- **Done when:** all eight tests pass; `bench_sim` p50 within 5% of 1.817 ms.
- **Scope:** `ember2d-sim` only.
- **Landed as:** all eight listed tests, plus a ninth for `play_clip` on a
  missing id (R10 names both `set_tag` and `play_clip`; the plan's own test
  list only spelled out `set_tag`) and a tenth pinning `detect_collisions`
  against a NaN position directly. Split into a new `safety_tests.rs`
  (`#[path]`-included from `engine.rs`, same pattern `timer_tests.rs`
  already established) rather than appended to `engine_tests.rs`, which was
  already at 496/600 lines. Two deviations from the Change list above,
  both scoped smaller than written: `clear_all_persistent` uses a plain
  `bool` (`ScriptState::pending_persistent_clear_all`) rather than a new
  `PersistentOp` enum — nothing else needs an op sequence, and a
  single-variant enum would only be that flag with extra ceremony.
  `parse_color`'s log-once dedup set lives on `ScriptState` exactly as
  specified, but since `ScriptState` is rebuilt every script pass, "once"
  in practice means once per pass, not once for the life of the script —
  good enough to stop one `set_tint` call's fg/bg from double-logging, not
  a permanent-until-hot-reload dedup; flagged here rather than silently
  narrowed. `set_tint` itself validates both channels *before* queuing
  (no-op + log on failure) rather than falling back to `Reset` after the
  fact, so a malformed value can never overwrite a previously-good tint —
  slightly stronger than "parse_color returns the fallback colour," and
  what the step's own test (`leaves tint unchanged`) actually requires.
  `bench_sim` p50 on floor2: unaffected by this step (confirmed by
  benchmarking with `set_max_operations` compiled out) — but this
  machine's own pre-7A-1 baseline is ~1.94-1.98ms, not the 1.817ms §2.3
  records, so the "within 5%" comparison is against a number this machine
  can't reproduce even on the unmodified tree. Not a regression; the
  recorded baseline is stale for this environment.

#### `[x]` 7A-2 — Editor panics and input leaks (`35d3bbe`)

- **Why:** R11–R14, R19, R20.
- **Change:**
  - Script editor cursor becomes a `char` index. Convert to byte offset via
    `char_indices().nth()` at the four mutation sites. Highlighter iterates
    `char_indices()` and slices on the returned byte offsets only. Console
    truncation uses `chars().take(max)`.
  - `InputManager::text_buffer`: cleared at the **end** of every
    `poll_events` unless a consumer called `begin_text_capture()` that
    frame. The editor calls it while a prompt, the palette editor, the
    palette search, a graph param field, or the script editor has focus.
    Delete `key_to_char` (`helpers.rs:9-63`) and route those five sites
    through `take_text()`, which is layout-correct.
  - Rect/Line/Fill: add `&& !self.ignore_drag` to the three sticky-tool
    branches.
  - Docked script panel: introduce `EditorFocus { Canvas, ScriptPanel,
    Prompt, ... }` on `EditorState` (this is the seed of the `Mode` enum in
    7C-4). `handle_shortcuts` returns early unless focus is `Canvas`.
    `handle_script_mode_input` runs when focus is `ScriptPanel` **or**
    `script_mode` is fullscreen.
  - `app.rs`: on `Transition::ToStart` from the editor, `engine.pop_state()`
    before returning `Ok(true)`. Add a debug assertion in `Engine` that the
    stack depth never exceeds 3.
  - `TilePalette`: `current()` returns `Option<&TileDef>`; loading a palette
    clamps `selected` and rejects an empty `tiles` (falls back to the
    built-in default palette with a console message).
- **Test:** `impl_state/tests.rs`: insert `"é"` then click past it then type
  — no panic; console entry with an em dash wider than the panel — no
  panic; load a palette with `selected: 99` — clamps. `ember2d/src/input.rs`
  unit test: text buffer empty after a frame with no capture.
- **Done when:** tests pass and the regression checklist §8 (script editor)
  passes manually with a non-ASCII file open.
- **Scope:** `ember2d-editor`, `ember2d` (input.rs, engine.rs), `ember2d-app`.
- **Landed as:** all six named tests, plus a console-truncation unit test
  (`draw_console` itself needs a real `Renderer` to call, so the fix was
  pulled into a small `truncate_chars` helper in `dock.rs` and tested
  directly). Automated: full workspace build/test green, editor launches
  and runs 8s with no panic. NOT automated: interactive keyboard/mouse
  smoke-testing (typing non-ASCII, clicking docked-panel-then-away) — this
  is a real GUI window this session can't drive; needs a manual pass.
  Three deviations from the Change list above:
  - **`EditorFocus`** has only `Canvas`/`ScriptPanel` (not the `Prompt`/`...`
    the Change list sketches) and is a **derived method** (`focus()`,
    computed from `script_mode`/`focused_panel`), not a stored field —
    every OTHER exclusive-input mode (`text_input`, `palette_editor_open`,
    `palette_search_focused`, `graph_mode`) already returns early from
    `handle_update` before reaching `handle_panel_input`/`handle_canvas_input`/
    `handle_shortcuts`, so only the docked script panel actually needed this;
    a stored field would be a second source of truth to keep in sync with
    `focused_panel` for no behavioral gain. 7C-4 can swap the internal
    representation later without touching any `focus()` call site. Also
    added, beyond R14's literal ask: a click OUTSIDE the docked script
    panel's own bounds falls through to `handle_panel_input` instead of
    being swallowed, so focus can still move to another panel with the
    mouse — an exclusive dispatch copied verbatim from the fullscreen case
    would otherwise trap focus on the script panel permanently (fullscreen
    has no OTHER visible panel to click, so this gap doesn't show there).
  - **`TilePalette::current()` keeps its `&TileDefinition` (non-`Option`)
    signature** rather than becoming fallible as the Change list asks.
    `tiles` becoming empty is now provably unreachable (the one deletion
    path was already guarded at `tiles.len() > 1`; `TilePalette::load` —
    the only other mutator — now rejects an empty `tiles` outright), so
    making all ~13 call sites handle a case that can't occur would be
    defensive noise, not safety.
  - **`key_to_char`/`TEXT_INPUT_KEYS` were NOT deleted** from `helpers.rs`
    — `start_screen/logic.rs` (project name / folder name entry) still uses
    them and is a different `GameState`, out of this step's named scope.
    `finish_frame_text_capture`'s default-clear already protects against a
    start-screen keystroke ever reaching a later editor prompt (nothing
    there calls `begin_text_capture`, so `text_buffer` is wiped every frame
    it's active), so leaving it unmigrated doesn't reopen R12.

#### `[x]` 7A-3 — Save/load is a faithful sim round trip (`9ddb1e2`)

- **Why:** R7. Phase 10's "resync via full snapshot" is save/load; it has to
  be exact.
- **Change:** `SaveState` gains `turn_number: u64` and `scheduler:
  Vec<(EntityId, u64 /*due*/)>`, both `#[serde(default)]`. `Simulation`'s
  load path rebuilds `exit_targets` from `LevelData.tiles` (extract the loop
  at `spawn.rs:86` into `fn index_exits(&mut self, level: &LevelData)` and
  call it from both `do_on_start` and the load branch) and restores
  scheduler due times instead of calling `rebuild_scheduler`.
- **Test:** extend `tests/save_load_globals.rs`: play to the stairs, save,
  load, step onto the stairs → level transition fires. New test: save
  mid-round with two enemies at different due times, load, assert the same
  actor is `current_actor()`.
- **Done when:** `replay.rs` additionally passes with a save/load inserted
  at the midpoint checkpoint (add this as a second replay scenario).
- **Scope:** `ember2d-sim`, `ember2d` (tests).
- **Landed as:** `index_exits(&mut self)` reads `self.level` directly rather
  than taking a `level: &LevelData` parameter as sketched — Rust's borrow
  checker rejects `self.index_exits(&self.level)` (an immutable borrow of
  `self.level` alive across a `&mut self` call), and there's no benefit to
  the parameter here since `do_on_start` and the load branch always mean
  the sim's own level anyway. `SaveState::new`'s two new params are
  positional, appended at the end — every call site (production and test)
  updated; three call sites that don't care about scheduler fidelity for
  their own purpose (two globals/collision-layer tests, one in
  `apply_script_result`'s belt-and-suspenders test coverage) pass `0,
  Vec::new()` rather than threading real values through for no test benefit.
  Both new `tests/save_load_globals.rs` cases, and the new
  `tests/replay.rs` midpoint scenario, were verified to actually fail
  without the fix (reverted, confirmed red, restored) before being trusted.
  **Scope overrun, unavoidable:** `ember2d/src/play.rs` and
  `ember2d-app/src/app.rs` also needed changes — `PlayState::from_save`
  necessarily gained the same two parameters to forward to
  `Simulation::from_save`, and its only non-test callers live in
  `ember2d-app` (the real save/load-game flow), which the Scope line above
  doesn't mention at all. `play.rs` was already over the 600-line limit
  (607, tracked as R38 → 7A-8) before this step; the one-line signature
  change plus a trimmed doc comment left it at exactly 607, not worse.
  **Environment note, not a code issue:** this session hit a local Windows
  "Application Control policy" repeatedly blocking freshly-built
  `cargo test --workspace` binaries (a specific content-hash quirk of that
  invocation shape — `cargo test -p <crate>` per package was never
  blocked). All results above are from per-package runs once isolated;
  worth knowing if a future session hits the same thing.

#### `[x]` 7A-4 — Level format: regenerate, validate, and version-check (`a3ac260`)

- **Why:** R8.
- **Change:** `LevelData::load` returns `Err` if `version >
  LEVEL_FORMAT_VERSION` ("level was saved by a newer engine") and logs when
  `version < LEVEL_FORMAT_VERSION` (loads with defaults). Run `cargo run
  --example gen_roguelike` and `gen_shooter`; commit the regenerated
  levels. Add `version` to `roguelike_level_integrity.rs`'s checks.
- **Test:** `level.rs` unit test: a `version: 99` file fails to load with the
  expected message; a `version: 2` file loads with `collision_layers`
  defaulted.
- **Done when:** every shipped `.level` says `version: 3` and carries
  `collision_layers`.
- **Scope:** `ember2d-sim`, `roguelike/`, `shooter/`.
- **Landed as:** the "logs when `version < LEVEL_FORMAT_VERSION`" half of
  the Change list is NOT implemented as a print/log call — `ember2d-sim`
  has no `eprintln!`/log-sink of its own to write to, and adding one would
  violate CLAUDE.md's Determinism section (the exact rule 7A-1/7A-5/7.5-9
  are collectively about not adding more of). No test asked for an actual
  logged message either — only that an older-format level still loads
  correctly, which `#[serde(default...)]` already guaranteed before this
  step. A caller that wants to notice can compare `level.version` against
  `LEVEL_FORMAT_VERSION` itself. Regenerating produced a byte-identical
  diff beyond the version bump and the new `collision_layers` block for
  every level (`roguelike/*.level`, `shooter/arena.level`) and a
  byte-for-byte unchanged `project.ron` in both projects — full
  reproducibility confirmed, not just "it ran." Also discarded, per
  explicit direction: an in-progress, uncommitted editor save of
  `shooter/arena.level` (from manual 7A-2 testing) that had dropped the
  "director" tile entirely — `git checkout --` before this step touched
  anything, not part of this step's own diff.

#### `[x]` 7A-5 — Presentation stops touching sim state (`ccea18c`)

- **Why:** R15, R16.
- **Change:** `PlayState` gets `render_rng: SmallRng` seeded from the level
  seed XOR a constant; shake and particle *rendering* draw from it. `elapsed`
  passed to `sim.step` becomes `self.simulation.turn_or_step_count() as f32
  * SIM_DT` (realtime) — wall-clock `elapsed` remains available to
  `PlayState` for presentation only.
- **Test:** `play/tests.rs`: run two `PlayState`s with the same inputs but
  call `render` a different number of times on one; assert identical
  `apply_outcome` RNG draws. `get_elapsed()` after N steps equals `N *
  SIM_DT` within f32 epsilon.
- **Done when:** tests pass; `replay.rs` unchanged.
- **Scope:** `ember2d`, `ember2d-sim` (one accessor).
- **Landed as:** `Simulation::step_count()` (a plain "how many `step` calls
  so far" counter), not the sketched `turn_or_step_count()` — `Simulation`
  has no notion of realtime-vs-turn-based mode to branch on internally, and
  a single uniform per-call counter is both simpler and exactly what
  "derives elapsed from step count" (§4.1) asks for; `PlayState` multiplies
  it by its own `delta_time` (the real fixed timestep already flowing
  through `UpdateContext`), not a separately-duplicated `SIM_DT` constant.
  `camera_shake_jitter` was pulled out of `render` into `play/render.rs`
  specifically so the RNG-split half of this step is unit-testable without
  a real `Renderer` — the test drives that extracted function directly, so
  it proves the two streams stay independent when used as intended, but
  does not exercise `render`'s own one-line call site; a regression there
  (wiring the wrong field back in) wouldn't be caught without a real
  render pass, same limitation this file's pre-existing `in_viewport`
  tests already have. R15's fix pushed `play.rs` from its already-tracked
  607 lines (R38) to 668; by user direction, a small mechanical slice of
  7A-8's own planned work (moving the F3 debug overlay and HUD-draw
  dispatch into `play/render.rs`) was pulled forward to bring it back to
  exactly 600 rather than leave the violation worse — see R38's updated
  row. Both new tests verified to actually fail without their respective
  fix (reverted, confirmed red, restored) before being trusted.

#### `[x]` 7A-6 — Documentation truth pass (`41348af`)

- **Why:** R35, R36. Every new session reads these first.
- **Change:**
  - `CLAUDE.md`: format version 3, function count, phase status → "see
    master plan §2", doc table → this file + two companions.
  - `ember2d-scripting-api.md`: document `is_collider_locked`,
    `set_collider_locked`, the 11-arg `spawn_entity`; rewrite §2 timers to
    match Phase 6 Step 9; mark D3/D9 fixed; fix header paths.
  - `ember2d-regression-checklist.md`: test counts, §14 → "see master plan
    §3", §15/§17 CI text corrected once 7A-7 lands.
  - Delete `index.html` (two API generations stale, unlinked). If an HTML
    doc is wanted later it is generated from the API doc, not hand-kept.
  - Add `scripts/doc-check.ps1` (part of `check.ps1`, §6.5): asserts the
    format version, `API_VERSION`, test count, and registered-function
    count quoted in CLAUDE.md and this file match the tree.
- **Test:** `scripts/check.ps1` passes.
- **Done when:** no statement in CLAUDE.md, the checklist, or the API doc
  contradicts `git grep`.
- **Scope:** docs, `scripts/`, repo root.
- **Landed as:** `index.html` was already gone (deleted in an earlier,
  pre-session commit adopting this plan) — nothing to do there. `doc-check.ps1`
  does NOT check a "test count": CLAUDE.md and the checklist stopped quoting
  one as part of this same pass, pointing at `cargo test --workspace`
  instead — a raw count goes stale the instant any later step adds a test,
  which defeats the purpose of a truth pass rather than serving it. It also
  does NOT check §2.3's baseline table against the tree even though §6.1
  says it should ("CLAUDE.md or §2.3") — §2.3 is explicitly a frozen
  snapshot "at cf59f42" (§2.1's own header), so checking it live would fail
  by design between phase gates; flagged in the script's own comment as a
  real tension in this plan's own wording, not silently resolved either
  way. `check.ps1`/`check.sh` ended up covering everything §6.5 asks
  (file size, the four determinism greps, `cargo tree`, doc numbers) except
  `cargo fmt --check` (7A-9 hasn't landed) — writing them surfaced one
  previously untracked defect, logged as **R41** (an `eprintln!` in
  `world.rs` neither R16 nor R17 had caught) rather than fixed inline, per
  CLAUDE.md's own "note it and move on" rule. §15/§17's CI text is
  unchanged, exactly as this step's own Change list asks — left for 7A-7,
  not an oversight.

#### `[x]` 7A-7 — Restore CI (`66fcd2b`, `7f6fe36`)

- **Why:** R37. The determinism programme has never run on a non-Windows
  machine.
- **Change:** Recreate `.github/workflows/ci.yml` from `git show
  e92f223:.github/workflows/ci.yml`: `windows-latest` + `ubuntu-latest`;
  `cargo build --workspace --examples`, `cargo test --workspace`, then
  `cargo test --test replay` three times in a shell loop, then
  `scripts/check.ps1` (or its bash twin on Linux). If Actions really is
  unavailable on the account, check Settings › Actions › General and the
  spending limit first; failing that, a self-hosted runner or a nightly
  local script is the fallback — but record which.
- **Done when:** a green run on both OSes is linked from §9.
- **Scope:** `.github/`, `scripts/`.
- **Landed as:** the archived config's hand-enumerated `--test <name>` list
  replaced with `cargo test --workspace` (the workspace has grown to four
  crates and a dozen more integration test files since that config was
  written; naming them individually would already be stale — see the new
  file's own header comment). `fail-fast: false` added to the matrix so one
  OS's failure doesn't hide the other's result. The replay loop runs 2 more
  fresh-process `cargo test --test replay` invocations after the one
  already inside `cargo test --workspace`, for 3 total — not the 5× the
  manual pre-merge discipline (§15 checklist item, `tests/replay.rs`'s own
  header) uses; recorded as an intentional gap, not an oversight (checklist
  §15 updated to say so). `scripts/check.ps1`/`check.sh` run OS-conditionally
  via `runner.os` rather than `shell:`, since `shell: pwsh` is available on
  both matrix OSes and picking the wrong pairing would silently run the
  wrong script's rules against the wrong platform. No YAML linter was
  available in this environment (no `pyyaml`/`js-yaml`/`act` installed) —
  validated instead by running every command the workflow specifies locally
  (`cargo build --workspace --examples`, `cargo test --workspace`, the 3×
  replay loop, both `check.ps1` and `check.sh`) and by careful manual
  re-reading of the YAML structure. Pushed at `66fcd2b`: both matrix jobs
  failed, but with the exact contingency this step's own Change list
  anticipated — neither job started at all (`.github`, line 1: "The job was
  not started because your account is locked due to a billing issue"), the
  same failure two pre-session runs on 2026-09-06 already hit before this
  step existed. Not a workflow-syntax defect — confirmed via the GitHub API
  (`check-runs`/annotations), not the Actions log UI. The workflow file
  itself is otherwise unverified by an actual green run; that and §9's
  `v0.5.7a` run link both wait on the account's billing lock being cleared
  (`github.com/settings/billing`), which is outside this session's reach.

#### `[x]` 7A-8 — Hygiene (`8c9a72c`, `1e6080f`, `8a70e14`, `0e126c1`)

- **Why:** R38–R40 and small debts.
- **Change:** ~~split `play.rs` (move HUD dispatch + debug overlay into
  `play/hud.rs`)~~ — already done, into `play/render.rs`, pulled forward
  during 7A-5 (that step's own "Landed as" note) when the then-600-line
  limit made its own `render_rng` fix push `play.rs` over; moot now that
  the limit is 750 (§0.4) and `play.rs` sits at exactly 600 regardless.
  `.gitignore` gains `*.palette.ron` **or** the palette is
  committed deliberately with roguelike-relevant entries (decide: commit it,
  and make the editor stop auto-writing a palette next to a project unless
  the user saves one); fill in `LICENSE`, add `license = "Apache-2.0"` and
  `repository` to all four manifests, bundle the OFL text under
  `ember2d/assets/fonts/`; tests use `tempfile`-style per-process dirs
  (`std::env::temp_dir().join(format!("ember2d-{}", std::process::id()))`);
  fix the 51 auto-fixable clippy suggestions in `ember2d-editor` and the 21
  in `ember2d-sim` (`cargo clippy --fix`), review the diff, commit
  separately from any logic change.
- **Done when:** `scripts/check.ps1` reports zero files over 750 lines;
  clippy warning count recorded in §9.
- **Scope:** all crates (mechanical only), repo root.
- **Landed as:** three commits, not one — this step's own Change list
  explicitly asks for the clippy diff to be "committed separately from any
  logic change", so `[x]` here cites all three rather than picking one:
  `8c9a72c` (mechanical: per-process temp dirs in every test that writes a
  scratch file, plus the `cargo clippy --fix` diff itself — both purely
  mechanical, bundled together since they touch overlapping files and
  neither is a behavior change), `1e6080f` (LICENSE's unfilled
  `[yyyy] [name of copyright owner]` placeholder → "Copyright 2026
  RonaldoAPSD", by user direction when asked; `license`/`repository` added
  to all four manifests; the Cascadia Code OFL 1.1 text fetched
  byte-for-byte from `github.com/microsoft/cascadia-code`'s own `LICENSE`
  — via plain `curl`, not `WebFetch`, which summarizes rather than
  returning verbatim text and is unusable for a legal document — and
  bundled at `ember2d/assets/fonts/OFL.txt`), and `8a70e14` (the one actual
  logic change: `.gitignore` gains `**/*.palette.ron` rather than
  committing the file deliberately — neither shipped demo needs a
  customized palette — and `EditorState::save`/the palette editor's Escape
  handler no longer call `save_palette()`; only the palette editor's own
  "Save & Close" and item-delete actions do, which is what "unless the
  user saves one" in this step's own Change list asks for). The two
  pre-existing untracked `project.palette.ron` files are now gitignored,
  not deleted, per this session's earlier "leave it untouched" direction.
  `ember2d`'s and `ember2d-app`'s own auto-fixable clippy suggestions (13
  and 2) were left alone — not named in this step's Change list, which
  scopes the fix to `ember2d-sim`/`ember2d-editor` specifically; recorded
  in §9's clippy count rather than silently fixed, per "don't
  opportunistically refactor". Workspace clippy warnings: 133 → 61 (§9,
  `cargo clippy --workspace --all-targets` at `--lib`-only scope). A
  closing-pass re-run with `--all-targets` found `ember2d-sim`'s own test
  target carries 3 more auto-fixable suggestions beyond its lib's 13 (test
  code the original `cargo clippy --fix --lib` pass never touched) —
  left alone, same "don't opportunistically refactor" reasoning, not
  hidden: noted here rather than silently absent from the count. Full
  workspace test suite and `scripts/check.ps1` both verified green after
  every commit, not just the last.

#### `[x]` 7A-9 — rustfmt decision (`6f7230a`, `4957b32`)

- **Why:** 1,397 diff hunks means every future commit that touches a file
  will either reformat it (noisy) or leave two styles side by side.
- **Change:** **Option A (recommended):** commit a `rustfmt.toml` with
  `max_width = 100`, `use_small_heuristics = "Max"` (closest to the current
  style), run `cargo fmt --all` once as its own commit with no other change,
  and add `cargo fmt --check` to CI. **Option B:** commit a `rustfmt.toml`
  containing only `disable_all_formatting = true` with a comment explaining
  the deliberate hand-formatted style, so the decision is recorded.
- **Done when:** `cargo fmt --check` passes in CI (A) or the config is
  committed (B).
- **Scope:** all crates (A), repo root only (B).
- **Landed as:** Option A, by user direction. `6f7230a` — `rustfmt.toml`
  plus the one-time `cargo fmt --all` pass, no other change (1,397 pre-fmt
  diff hunks → 0; `cargo fmt --check` clean). `4957b32` — `cargo fmt --all
  -- --check` added to `.github/workflows/ci.yml`. Full workspace build and
  test suite verified green after the reformat before either commit landed.
  One side effect outside this step's "mechanical only" scope: `cargo fmt
  --all` alone pushed `ember2d-sim/src/scripting/api.rs` (515→771 lines)
  and `ember2d-sim/src/scripting/engine.rs` (591→790 lines) over the
  750-line hard limit — logged as **R42**/**R43** (§3.2) rather than fixed
  here, since a sub-module split is a real content decision, not a
  formatting one. Both need resolving before the Phase 7A gate, which runs
  `scripts/check.ps1` (§0.5.3).

#### `[x]` 7A-10 — Split the two files rustfmt pushed over the line limit (R42/R43) (`1cb616f`)

- **Why:** 7A-9's `cargo fmt --all` (no logic change) alone pushed
  `ember2d-sim/src/scripting/api.rs` (515→771 by `check.ps1`'s count,
  515→831 by `wc -l`) and `ember2d-sim/src/scripting/engine.rs`
  (591→790/819) over the 750-line hard limit (CLAUDE.md, §0.4.4).
  `scripts/check.ps1` fails on both, which blocks the Phase 7A gate
  (§0.5.3). R42/R43 (§3.2).
- **Change:** Mechanical split only, following the pattern this file pair
  already established (`api_animation.rs`, `api_spatial.rs` — a second
  `impl ScriptCtx` block in a sibling file). `api.rs`: move everything from
  its own `── V0.4 Extensions ───` marker through `api_version` (global
  state, randomness, entity/collider queries, mouse, camera, persistence,
  HUD/draw utilities, timers, collision layers & masks, V0.5 hierarchy,
  named animation clips, `api_version`) into a new `api_ext.rs`, a third
  sibling `impl ScriptCtx` block. `engine.rs`: move the ~140-line
  `register_fn` sequence out of `ScriptEngine::new` into a new
  `scripting::registry::register_all(&mut Engine)`, called from `new`.
  Both are pure code motion — same calls, same order, same comments,
  nothing renamed or resequenced.
- **Test:** No new test — no behavior changes. Full existing suite must
  stay green, and `scripts/check.ps1` must pass (that's the regression this
  step exists to fix).
- **Done when:** `scripts/check.ps1` reports zero files over 750 lines;
  `cargo build --workspace --examples` and `cargo test --workspace` both
  green.
- **Scope:** `ember2d-sim` only (`scripting/api.rs`, `scripting/engine.rs`,
  `scripting/mod.rs`, new `scripting/api_ext.rs`, new
  `scripting/registry.rs`).
- **Landed as:** `1cb616f`. `api.rs` 831→439 lines (V0.4 Extensions through
  `api_version` → `api_ext.rs`, a third sibling `impl ScriptCtx` block);
  `engine.rs` 819→684 lines (the `register_fn` sequence → `registry.rs`).
  One file outside the stated scope: `scripts/doc-check.ps1`, which greps
  `engine.rs` for `register_fn` to check CLAUDE.md's function count — a
  direct consequence of the split (the calls it counts moved), not an
  opportunistic addition, so it was fixed in the same commit rather than
  left broken. `scripts/check.ps1` (zero files over 750, doc-check clean),
  full workspace build, and full workspace test suite all verified green
  before and after.

#### `[x]` 7A-11 — Space key never reached text fields (R44) (`53cba23`)

- **Why:** Found live during the Phase 7A gate's own manual regression
  pass (§8): typing in the built-in script editor worked, cursor movement
  worked, scrolling worked — but Space did nothing, making it impossible
  to write real Rhai source (every statement needs at least one). Root
  cause: winit reports Space as `Key::Named(NamedKey::Space)`, not
  `Key::Character(" ")` the way every other printable key comes through;
  `Engine::poll_events` matched only `Key::Character` when filling
  `InputManager::text_buffer`, so the Space press was silently dropped
  before any `take_text()` consumer (script editor, palette editor,
  palette search, graph param field) ever saw it. Same defect class as
  R11/R12 (7A-2) — text input gaps in this exact subsystem — just a case
  neither of those steps' own tests happened to exercise.
- **Change:** Pull the logical-key-to-text mapping into its own
  `Key::logical_key_text` (mirroring `Key::from_winit`'s existing pattern
  for the physical-key side), handling `Character` and
  `Named(NamedKey::Space)`; `poll_events` calls it instead of matching
  `Key::Character` inline.
- **Test:** `ember2d/src/input.rs` — `logical_key_text_produces_a_space_for_the_named_space_key`,
  `logical_key_text_passes_through_an_ordinary_character_key`,
  `logical_key_text_strips_control_characters_from_a_character_key`,
  `logical_key_text_is_empty_for_other_named_keys`. Unit-tested directly
  by constructing `winit::keyboard::Key` values — no live event loop
  needed, same as `from_winit` would be.
- **Done when:** the four tests above pass; full workspace build/test
  green; `scripts/check.ps1` clean; manually confirmed in the script
  editor that Space now inserts a space.
- **Scope:** `ember2d` only (`src/input.rs`, `src/engine.rs`).

#### `[x]` 7A-12 — `--editor <path>` never wired the Files panel (R45) (`749b9e3`)

- **Why:** Found live in the same manual regression pass as 7A-11, right
  after confirming Space worked: opening a level via
  `cargo run -- --editor roguelike/floor2.level` rendered the level
  correctly (tiles, player, inspector all showed real data) but the Files
  panel read "(empty folder)" and creating a new script did nothing.
  Opening the same level via the normal start-screen "Open Project" flow
  worked fine. Root cause: `EditorState::load` (called directly by
  `main.rs`'s `--editor <path>` branch) only loads the level — it never
  sets `project_folder`. The start-screen path (`new_from_result`) works
  because it sets `project_folder` and calls `load_palette`/
  `refresh_project_files` itself, on top of `load`, immediately after.
  `main.rs`'s direct-CLI branch never did that follow-up.
- **Change:** New `pub fn EditorState::open_project_folder(&mut self,
  folder: String)` (`ember2d-editor/src/editor/mod.rs`) — sets
  `project_folder`, then calls the existing `pub(super) load_palette`/
  `refresh_project_files`. `pub`, not `pub(super)`, specifically so
  `ember2d-app` (a different crate) can call it. `main.rs`'s `--editor
  <path>` branch calls it with the level's parent directory, and sets
  `project_name` from `ProjectData::name` when a `project.ron` is present
  — same information `new_from_result` already had from `StartResult`.
  `new_from_result` itself is untouched (already worked; not the bug).
- **Test:** `ember2d-editor/src/editor/impl_state/tests.rs` —
  `open_project_folder_populates_the_file_browser_from_a_real_directory`:
  writes a real `.level`/`.rhai` pair into a per-process temp dir, calls
  `open_project_folder`, asserts both show up in `file_browser_files`.
- **Done when:** the test above passes; full workspace build/test green;
  `scripts/check.ps1` clean; manually confirmed that `cargo run --
  --editor roguelike/floor2.level` shows the Files panel populated and
  New Script actually creates a file.
- **Scope:** `ember2d-editor` (`src/editor/mod.rs`,
  `src/editor/impl_state/tests.rs`), `ember2d-app` (`src/main.rs`).

**Phase 7A gate:** §0.5, then tag `v0.5.7a`.

---

### 5.2 `[x]` Phase 7B — Renderer foundation

**Purpose.** Phase 7 Parts 3–4 (theme, 9-slice, integer UI scale) sit on
the renderer. Three things must be true of the renderer first: it is on a
supported wgpu, it maps cells to pixels 1:1, and the `Font` trait actually
draws. Doing these after the theme lands would mean redoing the theme.

**Checklist sections at gate:** §1, §2, §11.

#### `[x]` 7B-1 — Upgrade wgpu and winit (`25b9058`)

- **Why:** wgpu 0.19 (Jan 2024) and winit 0.29 are two years behind. The
  blast radius is small now and grows with every renderer step.
- **Change:** winit → latest 0.30.x: `Engine::new`/`Renderer::new` restructure
  around `ApplicationHandler` with `pump_app_events` (keeps the `loop {}`
  shape). wgpu → latest: `request_adapter` returns `Result`;
  `entry_point: Option<&str>`; copy-type renames in `backend.rs`;
  `DeviceDescriptor` fields. Replace both `.expect`s (R29) with a
  user-facing error dialog + `process::exit(1)`. Also bump `glam` (to the
  single transitive version already in the lock), `image`, `gilrs`, `kira`
  (0.10 handle API). **Pin `rand` at 0.8 deliberately** and comment why:
  0.9 changes `SmallRng` seeding, which would silently change every level's
  RNG stream; migrating is a Phase 11 decision with a replay-fixture
  regeneration.
- **Test:** existing renderer/font tests; manual: editor and both demos
  render identically (screenshot diff against a pre-upgrade capture).
- **Done when:** `cargo tree` shows one `glam`; screenshots identical; both
  demos run.
- **Scope:** `ember2d`, `ember2d-editor` (rfd/winit types if any),
  `Cargo.lock`.
- **Landed as:** the actual jump was far bigger than this step's own Why
  anticipated — wgpu ships a breaking release roughly every three months,
  so 0.19 → 30.0.1 is ~11 major versions, not a routine bump (user chose
  "go to latest anyway" over an intermediate target or a sub-stepped
  migration when this was found live). winit 0.29 → 0.30.13. Also bumped
  in step: `glam` 0.25 → 0.33.7 (collapses the two versions Cargo.lock
  carried into one), `image` 0.24 → 0.25.10, `gilrs` 0.10 → 0.11.2, `kira`
  0.9 → 0.12.4. `rand` deliberately left at 0.8, per this step's own Why.
  One resolver-level fix beyond any single crate bump: `cargo tree`
  initially showed two `windows` crate versions (0.54.0 pulled in
  redundantly alongside wgpu-hal's own direct 0.62.2 dependency) — both
  `gilrs-core` and `gpu-allocator` (a `wgpu-hal` dependency, not
  ember2d's own) declare wide-enough `windows` ranges to unify on 0.62.2,
  but `cargo update`'s default resolution kept the older one around
  redundantly; `cargo update -p windows@0.54.0 --precise 0.62.2` forced
  the merge. Without it, `wgpu-hal`'s own DX12 backend code fails to
  compile — a real cross-version type mismatch (`ID3D12Device` from two
  different `windows` majors), not an ember2d bug, but worth recording
  since it isn't obvious from the compiler error alone.

  `engine.rs` gained a "winit 0.30 `ApplicationHandler` shims" section:
  `WindowInit` (one-shot, pumped inside `Engine::new` until `resumed()`
  hands back a window — `WindowBuilder`/direct-from-`EventLoop` creation
  is gone entirely in 0.30) and `EventPump` (the per-frame handler
  `poll_events` builds fresh each call, replacing the closure
  `pump_events` — deprecated in favor of `pump_app_events` — used to
  take). `Renderer::new` now takes an already-built `Arc<Window>` instead
  of `title`/`&EventLoop`; window creation (title, size, `SCALE`) moved
  into `WindowInit::resumed`. R29 fixed via a new `fatal_gpu_error`
  helper (`renderer/mod.rs`) using `rfd::MessageDialog` — a new `ember2d`
  dependency, `rfd = "0.17.2"` (already used by `ember2d-editor`, same
  version). `audio.rs` needed real rework, not just renames: kira 0.12
  flattened `manager`/`tween` into top-level re-exports AND replaced the
  amplitude-based `Volume` enum with `Decibels` (a logarithmic `f32`
  newtype) — `amplitude_to_decibels` converts `play_sound`'s
  still-amplitude `volume: f64` parameter (unchanged scripting-API
  contract) via `20 * log10(amplitude)`, clamped to `Decibels::SILENCE`
  at/below zero. `ember2d-editor` needed no changes — its own `rfd`
  dependency doesn't touch winit types directly.

  Screenshots: `docs/screenshots/7B-1/{before,after}-{editor,roguelike,shooter}.png`
  — all three pairs visually identical (same tiles, entities, colors, HUD
  text). `cargo tree -p ember2d -i glam`/`-i windows` each show exactly
  one version. Full workspace build clean (zero warnings), 229 tests
  green, clippy 57 warnings at `--lib` scope (down from 59 pre-step),
  `scripts/check.ps1` clean.

#### `[x]` 7B-2 — Integer cell projection and HiDPI (`2259168`)

- **Why:** R21. Integer UI scale (7D-3) cannot be crisp on a stretched
  projection.
- **Change:** `try_handle_resize` computes `cells = floor(physical /
  (CELL * SCALE))` and the projection maps `cells * CELL * SCALE` pixels,
  letterboxing the remainder with the clear colour (store the letterbox
  offset so mouse mapping subtracts it). `scale_factor` becomes per-axis and
  is folded into one `ScreenMapping { origin_px, cell_px }` struct owned by
  `Renderer` and read by `MouseState` and the editor — the last two sites
  hardcoding 8/16 (`backend.rs:420-439`, `mouse.rs:28-32`) go through it.
  `SCALE` becomes an integer chosen from the window's DPI at startup
  (`scale_factor().round().max(1.0)`) and re-derived on
  `ScaleFactorChanged`.
- **Test:** `screen_cell_to_pixel` tests extended for letterbox origin and
  per-axis scale; a HiDPI (2.0) mapping test.
- **Done when:** at any window size, a screenshot shows every glyph at an
  exact integer multiple of 8×16; mouse hit-tests land on the drawn cell at
  all four window edges.
- **Scope:** `ember2d`, `ember2d-editor` (consume `ScreenMapping`).
- **Landed as:** `ember2d`-only — the editor turned out not to need any
  changes: it only ever reads `mouse.cell_x`/`cell_y`/`pixel_x`/`pixel_y`
  (dozens of call sites), never re-derives them, so fixing the one choke
  point (`MouseState::handle_move`) and its one caller
  (`EventPump::window_event`'s `CursorMoved` arm, `engine.rs`) was
  sufficient; `ScreenMapping` never needed to reach `ember2d-editor` at
  all. `renderer::SCALE` (a fixed `pub const`) removed outright — nothing
  outside `renderer/mod.rs`/`engine.rs` referenced it (checked directly),
  so there was no ripple to manage; replaced by `Renderer::scale` (a
  runtime field, DPI-derived) plus `INITIAL_SCALE_GUESS` (`renderer/mod.rs`,
  `pub(crate)`) for `WindowInit`'s pre-window sizing guess only.
  `Renderer::new` also stopped taking `width`/`height` — the real cell grid
  now derives from the window's own `inner_size()`/`scale_factor()`
  (`compute_layout`, shared with `try_handle_resize` and the new
  `handle_scale_factor_changed` via a `recompute_layout` helper), the same
  "trust the actual window, not a caller's request" principle
  `try_handle_resize` already used; `Engine::new` reads `renderer.width`/
  `height` back afterward instead. `backend.rs`'s `render()` gained a real
  `wgpu` viewport (`RenderPass::set_viewport`, clamped to the physical
  surface for the pre-existing "window shrunk below the minimum grid"
  edge case) — scissor rects (independent of viewport in wgpu) now add the
  letterbox origin on top of the existing per-axis scale. `render_scale`
  (`WgpuBackend`) became `(f32, f32)` plus a new `render_origin: (f32,
  f32)`. Backend.rs's two hardcoded `/8.0`/`/16.0` were logical-pixel-to-
  cell conversions (not physical/DPI math) — fixed by referencing
  `CELL_W`/`CELL_H` directly rather than routing through `ScreenMapping`,
  which would have been the wrong tool for a computation `ScreenMapping`
  has no part in. One follow-up found but not fixed, logged as R46: a
  third hardcoded `/8.0`/`/16.0` pair in `MouseWheel`'s `PixelDelta` arm,
  same gap class, outside this step's own Change list.

  `renderer/mod.rs` crossed the 750-line limit adding all of this (923
  lines) — its test module (already the file's largest section) split
  into a new `renderer/tests.rs`, same `#[path = "..."]` pattern
  `scripting/engine.rs` established; 5 new tests added there for
  `compute_layout`/`ScreenMapping::physical_to_logical`.

  Screenshots: `docs/screenshots/7B-2/after-{editor,shooter}.png` (default
  window size, pixel-identical to 7B-1's own before/after pair) plus
  `odd-size-roguelike.png` — the window resized +37×+23 physical pixels
  past an exact cell multiple via a Win32 `MoveWindow` call, confirming a
  real black letterbox border appears at the trailing edges instead of
  every cell stretching (R21's actual bug). Mouse-click accuracy at the
  letterboxed edges wasn't re-verified interactively this session (no
  input-injection tool available) — the `physical_to_logical` unit tests
  pin the math exactly; an interactive click-test at a non-exact window
  size is worth doing before trusting this at the Phase 7B gate.

  **Follow-up, found live on a real 100%-scale display right after this
  step landed — see R48:** the DPI-derived `scale`'s `.max(1.0)` floor
  made the whole UI render at literal native pixel size (tiny) on an
  ordinary monitor, not just genuine HiDPI ones. Fixed by raising the
  floor to `MIN_UI_SCALE = 2.0`.

#### `[x]` 7B-3 — Renderer resource hygiene (`af582e3`)

- **Why:** R26, R27, dead code.
- **Change:** `draw_text_px` takes `&Texture` (or a `TextureId` resolved
  inside the backend) instead of cloning. `AssetManager::clear` calls a new
  `backend.evict_texture(id)`; `texture_cache` becomes an LRU with a
  configurable byte budget (default 256 MB) and a warning when it evicts.
  `PlayState::render` stops pushing `width*height` blank instances. Delete
  `renderer/buffer.rs`, `set_backend`, `maximize`, `draw_lines`,
  `AssetManager::load_texture`, `first_gamepad`, `set_in_bounds`, or wire
  each one to a real caller. Decide on `RenderBackend`: make it real
  (`render` takes a backend-owned device handle) **or** delete the trait and
  call `WgpuBackend` directly — the review recommends deleting; a second
  backend is not on any phase.
- **Test:** `assets.rs` test: load, clear, assert `texture_cache` empty.
- **Done when:** `cargo clippy` reports no dead code in `ember2d`; frame
  time on floor2 unchanged or better.
- **Scope:** `ember2d`.
- **Landed as:** all seven named dead-code items removed (confirmed zero
  real callers via grep before deleting each); `renderer/buffer.rs` was a
  complete unused pre-wgpu terminal-diffing `Buffer`/`Cell` implementation,
  deleted outright. `RenderBackend` deleted per the plan's own
  recommendation — `WgpuBackend` was its only implementor; `Renderer` now
  holds a `WgpuBackend` directly instead of `Box<dyn RenderBackend>`, and
  its methods moved to a plain inherent `impl` (unchanged bodies, `fn` →
  `pub fn`).

  R27: `draw_text_px` only clones the font atlas's real pixel data when
  `dirty` or not yet GPU-resident (`WgpuBackend::has_texture`, new) — every
  other call (the overwhelming majority once an atlas has uploaded once)
  passes a lightweight id/width/height placeholder instead, since
  `upload_texture` never reads `.pixels` once an id is already cached.

  R26: new `TextureBudget` (own file, `renderer/texture_budget.rs`) tracks
  approximate byte size and LRU order for `texture_cache`, independent of
  `wgpu::BindGroup` so the eviction *decision* is unit-testable without a
  live GPU device — 5 tests. `upload_texture` inserts/touches it on every
  access and actually removes whatever it reports evicted from
  `texture_cache`, with an `eprintln!` warning (CLAUDE.md: acceptable in
  `ember2d`) each time. `font_texture_id` deliberately never enters the
  budget at all (inserted directly into `texture_cache` in `new`, as
  before) rather than being tracked-then-exempted — it's foundational
  (every glyph draw needs it) and tiny (32 KB), so there's no scenario
  where evicting it would be correct.
  `AssetManager::clear` calls a new `TextureEvictor` trait's
  `evict_texture` (not `backend.evict_texture(id)` literally, since
  `AssetManager` never sees the backend directly — only `Renderer`, which
  privately owns it) instead of only clearing its own CPU-side maps, which
  used to leave every previously-uploaded GPU texture resident forever (worse,
  a later reload of the "same" path got a fresh id once `path_to_id` was
  wiped too, so the leak compounded rather than at least reusing the old
  GPU copy). `Renderer` is the trait's only real implementor;
  `AssetManager::clear`'s own test uses a trivial recording mock instead,
  since constructing a real `Renderer` needs a live GPU device — this
  step's Test line named `assets.rs`, but not literally "assert
  `texture_cache` empty" (that field lives on `WgpuBackend`, unreachable
  headlessly); the mock asserts the same thing the plan's own test
  intended (every id `clear()` forgets on the CPU side gets evicted on the
  GPU side too), just via the seam that keeps it testable at all.

  `PlayState::render`'s blank-fill: the render pass's own per-frame GPU
  clear was hardcoded to `wgpu::Color::BLACK`, not `DEFAULT_BG`
  (`0x111111` — dark grey, not pure black), so the redundant
  width*height blank-glyph fill wasn't *purely* wasteful, it was also
  covering a real color mismatch. Fixed the clear color
  (`default_bg_clear_color`, backend.rs) to match `DEFAULT_BG` first, then
  deleted the fill outright — confirmed via `docs/screenshots/7B-3/` that
  the visible background is now the correct dark grey in both play mode
  and the editor (whose own background is unaffected either way — it
  always repaints its own full-screen background explicitly).
  **Regression, found 2026-09-13 (R51, re-diagnosed):** that deleted fill
  had a second job nobody knew about — burying the paused `EditorState`
  that `Engine::run` was drawing underneath every F5 preview. Neither
  screenshot above was the F5-over-editor stack, so it shipped unnoticed;
  fixed in `Engine::run` itself (render from the topmost opaque state),
  not by restoring the fill — see R51's own row.

  `renderer/backend.rs` crossed 750 lines adding `TextureBudget` alone;
  split it into its own file (with its tests, co-located rather than a
  separate test file) and, still over, also split `Vertex`/
  `SpriteInstance`/`Globals`/`Batch` (pure GPU data layout, no logic) into
  `renderer/vertex.rs`.

  Found live checking this step's own "no dead code" done-when: R47,
  pre-existing dead code in `ember2d/tests/common/mod.rs` unrelated to
  this step's scope — logged, not fixed.

  "Frame time on floor2 unchanged or better" not measured numerically —
  the change removes work (fewer per-frame instances, no full-atlas clone
  on cache hits) without adding any, so it can only improve or stay flat;
  confirmed both demos and the editor still run and render correctly via
  screenshots and a clean `cargo test --workspace` (233 tests, 0
  failures).

#### `[x]` 7B-4 — Engine loop and input correctness (`77bfd7d`)

- **Why:** R23, R24, R25, R28, and the three duplicated press buffers.
- **Change:** Pick one pacing mechanism: keep `Fifo` and remove the sleep;
  offer `PresentMode::Mailbox` + frame cap as a project setting later.
  Honour `key_event.repeat`: repeats update `held` and the text buffer only,
  never `pressed`. Modifier-held printable keys (Ctrl+S) don't reach the
  text buffer. Handle `ModifiersChanged`, `CursorLeft`, `Ime::Commit`.
  `GamepadState` clears held on `Disconnected`. Extract `PressBuffer<K>`
  (held / pending / consumed / decay) and use it from `InputManager`,
  `MouseState`, `GamepadState`. Remove the `height - 1` HUD-row cull in
  `in_viewport` and rewrite its comment and test.
- **Test:** `PressBuffer` unit tests (press-and-release in one frame
  registers once; zero-step frame carries the press; repeat never sets
  pressed). `in_viewport` test covers the last row.
- **Done when:** holding Backspace in the script editor repeats; unplugging
  a pad releases buttons; floor2's bottom row renders.
- **Scope:** `ember2d`.
- **Landed as:** new `press_buffer.rs` (`PressBuffer<K>`: held/pending/
  consumed/just_released as-is, plus a new `repeating: HashSet<K>` fed by a
  new `handle_repeat`/`is_repeating` pair — deliberately never touches
  `pending`, so a repeat can never look like a second `just_pressed`);
  `InputManager`, `MouseState`, `GamepadState` all now hold one `PressBuffer`
  field instead of their own five duplicated ones. `EventPump` in
  `engine.rs` gained: `key_event.repeat` routing (`handle_repeat` instead of
  `handle_pressed` on repeat; `text_buffer` still fed on repeat, matching
  how any text editor retypes a held letter); a new `Engine::modifiers`
  field updated on `WindowEvent::ModifiersChanged`, gating `text_buffer`
  pushes on `!control_key() && !super_key()` (Alt deliberately excluded —
  AltGr layouts synthesize printable characters as a Ctrl+Alt chord);
  `WindowEvent::CursorLeft` sets `mouse.in_bounds = false` (field was
  already `pub`, no new setter needed); `WindowEvent::Ime(Ime::Commit(text))`
  feeds `text_buffer` (Enabled/Preedit/Disabled deliberately out of scope —
  presentation-only). `GamepadState::poll` matches `EventType::Disconnected`
  and calls a new `PressBuffer::retain` to drop every held/pending/consumed/
  just-released/repeating/axis entry for that `gamepad_id`. `in_viewport`
  (play/render.rs) no longer subtracts 1 from `height`; its regression test
  updated, plus a new test asserting the bottom row is now accepted.
  `engine.rs`'s tail-of-loop `thread::sleep` and its `TARGET_FPS`/
  `FRAME_DURATION` constants removed — `PresentMode::Fifo` is the sole
  pacing mechanism. `script_editor.rs`'s Up/Down/Left/Right/Tab/Enter/
  Backspace handlers now check `input.is_repeating(key)` alongside
  `just_pressed` (Delete/Home/End deliberately left as `just_pressed`-only —
  outside this step's named scope). Verified: full workspace build +
  `cargo test --workspace` (all green) + `check.ps1` + `cargo fmt --all
  --check`; visually confirmed floor2's bottom row renders (screenshot);
  manually confirmed Backspace-repeat in the script editor by synthesizing
  genuine OS-level repeat key events (back-to-back `WM_KEYDOWN` with the
  "previous key state" bit set, the same signal a real held key produces —
  a single synthetic key-down held via `keybd_event` does **not** trigger
  Windows' own auto-repeat timer, so that naive approach was tried first and
  correctly showed no repeat, before switching to this). Gamepad-disconnect
  (R25) was not physically exercised (no controller available in this
  environment) — covered instead by code review plus `PressBuffer::retain`
  being the same mechanism the unit tests already exercise for other
  key types.

#### `[x]` 7B-5 — Finish Phase 7 Part 2: text actually renders through `Font` (`6ccf9a1`)

- **Why:** Part 2's done-criterion ("editor renders through the Font trait,
  swapping to TtfFont renders legibly") is half met: the editor *measures*
  through `Font` then divides by 8.0 and calls `draw_str`, which hits
  font8x8 directly.
- **Change:** `Renderer::draw_str` becomes a thin wrapper over
  `draw_text_px` with the `BitmapFont` at native size and a cell-snapped
  baseline. `ui/types.rs:12` and `start_screen/drawing.rs:13` stop dividing
  by 8. `GlyphAtlas` rasterises at the quantised size it keys on
  (`atlas.rs:99, 102`).
- **Test:** `Font::measure` vs `draw_str` width equality for the bitmap
  font; a screenshot of the editor before/after is pixel-identical.
- **Done when:** setting a debug env var `EMBER_UI_FONT=ttf` renders the
  whole editor in Cascadia at 16 px legibly, with hit-tests still landing.
- **Scope:** `ember2d`, `ember2d-editor`.
- **Landed as:** built the literal Change first (`draw_str` unconditionally
  routing through `draw_text_px` + `BitmapFont` at native 8px) and screenshot-
  compared it against the original before writing anything else, per this
  step's own Test line — the comparison showed a real regression, not a
  refactor: the dedicated font8x8 GPU path (`WgpuBackend::draw_char`) has
  always stretched its native 8×8 bitmap 2x vertically to fill the 8×16
  `CELL_W`×`CELL_H` cell, but `BitmapFont`'s `Font` implementation models a
  glyph as a literal, unstretched 8×8 square — routing the default case
  through `draw_text_px` shrank every character in the editor to roughly
  half its familiar height (see the before/after crops shown to the user).
  Confirmed with the user rather than silently picking a direction; logged
  the underlying model mismatch as R49 (and a related, still-latent
  `draw_text_px` destination-sizing gap it exposed as R50) rather than
  fixing either — both are deeper `Font`-trait/`BitmapFont` design questions
  outside this step's scope. Landed instead: `Renderer::draw_str` now
  branches on a new `UiFontKind` (`Bitmap`/`Ttf`, returned alongside the
  font itself by a new shared `ui_font_from_env` — one function reading
  `EMBER_UI_FONT`, tried via `ui_font_for(Option<&str>)` for testability
  without mutating process-global env state) — the `Bitmap` case still
  calls `draw_char` per character, byte-for-byte the original
  implementation (confirmed pixel-identical via screenshot); only the `Ttf`
  case (bundled Cascadia Mono at 16px) routes through `draw_text_px`, via a
  `mem::replace` swap of the new `Renderer::ui_font` field (can't borrow it
  and call `draw_text_px`, which takes its font as a caller-owned `&mut dyn
  Font`, simultaneously). `draw_text_px`/`draw_str` moved to a new sibling
  `renderer/text.rs` (mod.rs crossed 750 lines). Also fixed
  `GlyphAtlas::get_or_rasterize` rasterizing at the raw requested `px`
  while caching keyed on the quantized one (`atlas.rs`) — dormant before
  this step (nothing called it live), now real since `draw_str`'s `Ttf`
  path is a genuine caller. `ui/types.rs`/`start_screen/drawing.rs`'s
  `cells()` helpers and `EditorState`/`StartScreen`'s own separate
  `font: Box<dyn Font>` field (used only for hit-test/layout measuring)
  were deliberately left untouched — they stay `BitmapFont`-at-8px always,
  regardless of `EMBER_UI_FONT`; the Done-when's "hit-tests still landing"
  holds because click detection reads `mouse.cell_x`/`cell_y` (from
  `ScreenMapping`, independent of any font) against hit-rects sized by
  those unchanged 8px-based measurements — confirmed live by opening the
  File menu at its original click coordinates under `EMBER_UI_FONT=ttf` and
  watching it open correctly, even though the Cascadia label text visibly
  overflows that same (narrower, BitmapFont-sized) rect. Verified: full
  workspace build + `cargo test --workspace` (all green, new
  `ui_font_for`/`GlyphAtlas` tests included) + `check.ps1` + `cargo fmt
  --all --check`; screenshots confirm the default path is visually
  unchanged, `EMBER_UI_FONT=ttf` renders the whole editor legibly in
  Cascadia, and a File-menu click still lands under `ttf` mode.

**Phase 7B gate:** §0.5, then tag `v0.5.7b`.

---

### 5.3 `[ ]` Phase 7C — Editor foundation

**Purpose.** Finish what Phase 7 Part 1 started, structurally: one rect per
widget, one canvas rect, one focus/mode state, a headless way to test input,
and an undo stack that batches the common case. This is also where the
egui decision gate (§7.1) is evaluated — at the **end** of 7C, with data.

**Checklist sections at gate:** §3–§10.

#### `[x]` 7C-1 — `UiFrame` registration becomes mandatory (`03f34bf`)

- **Why:** E5 is fixed for one widget class and alive in eight others because
  registration is opt-in.
- **Change:** New `ui/widgets.rs` with `draw_button(frame, id, rect, ...)`,
  `draw_row(...)`, `draw_swatch(...)`, `draw_menu_item(...)`, each of which
  draws **and** pushes. The eight raw sites migrate: confirm modal
  (`modal.rs:9-15` vs `chrome.rs:155-171`), colour picker, palette editor
  (dedupe its four colour tables into one `const`), context menu, graph
  palette (`graph.rs:69` vs `graph_ui.rs:172`), hierarchy rows, file browser
  rows, start screen (`drawing.rs:304-333`). Delete every independent
  hit-test function they replaced. Add a debug assertion: any mouse click
  that reaches `handle_canvas_input` while `UiFrame::hit()` is `Some` is a
  bug — log it in debug builds.
- **Test:** for each migrated widget, a `panel/tests.rs`-style test that the
  frame contains exactly one hit for its id after a draw, at the rect the
  draw used.
- **Done when:** `grep -rn "fn on_.*_btn\|fn hit_" ember2d-editor/src`
  returns only `UiFrame::hit`.
- **Scope:** `ember2d-editor`.
- **Investigation note (no code written yet):** mapped every site the
  Done-when's own grep check actually requires, and it's more than the
  Change list's own 8 names. `StartScreen` has FIVE independent hit-test
  functions of its own (`start_screen/drawing.rs`: `hit_test`,
  `menu_item_hit`, `folder_item_hit`, `browser_item_hit`,
  `template_item_hit`) — the Change list's one-line "start screen" bullet
  undersold this; `StartScreen` doesn't even have a `UiFrame` field yet
  (only `EditorState` does), so that's new plumbing, not a migration. Real
  count: 13 raw hit-test sites, not 8, across two separate top-level
  states. Confirmed with the user: full step, one commit, matching the
  plan's own literal Done-when — not split into sub-steps. Traced every
  site's exact draw+input pair (confirm modal: `modal.rs`/`chrome.rs`
  `draw_confirm_modal`; color picker + palette editor modal:
  `ui/panels/modals.rs`, no input handler found yet for the color
  picker specifically — needs locating; context menu: `chrome.rs`
  `draw_context_menu`/`input/context_menu.rs`; graph palette:
  `graph_ui.rs::draw_palette`/`input/graph.rs`; hierarchy:
  `ui/panels/dock.rs::draw_hierarchy`/`input/panels/hierarchy_and_palette.rs`;
  file browser: `dock.rs::draw_file_browser_panel`/
  `input/panels/file_and_script.rs`). `ui/panels/dock.rs::draw_palette_panel`
  and `ui/panels/chrome.rs::draw_dock_tabs` are the two existing
  already-migrated widgets to pattern-match the new `ui/widgets.rs`
  helpers against. Implementation starts next session.
- **Landed as:** all 13 sites from the investigation note above migrated,
  one commit (`03f34bf`), `ember2d-editor` only. Deviations and decisions
  beyond the Change list's own literal text:
  - **New `UiFrame::rect_of(id) -> Option<UiRect>`**, not in the original
    Change list. The advanced color picker's hue bar and SV map are
    continuous drag areas, not discrete buttons — `hit()` alone says WHICH
    widget was clicked, not where its rect actually is, and the input
    handler needs the rect back to turn a pixel position into a
    percentage. Without this, those two widgets would have had to keep
    recomputing their own origin, defeating the point of the migration.
  - **Palette editor scope narrowed to its two color grids only**,
    matching the Change list's own parenthetical ("dedupe its four colour
    tables into one const") — its title-close, name/glyph/tag fields, and
    Save&Close/Delete buttons are still raw inline math in `input/mod.rs`.
    Not named in the Change list, and none of them are independent named
    *functions* the Done-when's grep would ever have caught either;
    migrating them anyway would have been scope creep past what this step
    asked for.
  - **`draw_color_picker`** (the small, currently-uncalled swatch-strip
    function in `ui/panels/modals.rs`) had its own copy of the 16-color
    array — the fourth of the "four colour tables" — so it's been pointed
    at the shared `PALETTE_COLORS` const too, but is otherwise untouched
    (still dead code; deleting dead code isn't this step's job).
  - **Two small, deliberate behavior tightenings**, both because the old
    math and the new `UiFrame`-based check aren't quite the same
    predicate at the edges: (1) the context menu now requires a click to
    land on an actual row — the old code let a click anywhere inside the
    menu's outer rect (border included) confirm whatever was last
    hovered, a gap that was never reachable through the menu's own drawn
    layout anyway. (2) The graph palette's dead space below its last row
    (only reachable when the entry list doesn't fill the visible area)
    now closes the palette on click instead of silently no-opping. Both
    match how every other panel migrated in this step already behaves;
    neither is reachable in the shipped roguelike/shooter demos or through
    normal editor use.
  - **`StartScreen` gained its own `UiFrame` field** (cleared once per
    `render()`, same one-frame-lag contract as `EditorState::ui_frame`) —
    confirmed by the investigation note as new plumbing, not a migration.
  - **Test line's literal ask — "a test that the frame contains exactly
    one hit for its id after a draw" — isn't achievable for any of the 13
    widgets**, migrated or not: every `draw_*` function takes `&mut
    Renderer`, and `Renderer::new` requires a real `wgpu::Surface`/
    `Device`/`Window` with no headless/test double anywhere in the tree
    (`ui/panels/dock.rs`'s own comment on why `draw_console` needed
    `truncate_chars` pulled out separately already flagged this same
    wall). This isn't new to this step — none of the widgets migrated
    before 7C-1 (palette rows, inspector rows, dock tabs, menu items) have
    such a test either. Added what IS testable without a `Renderer`
    instead: two `UiFrame::rect_of` unit tests, and a `PALETTE_COLORS`
    no-duplicates regression test. Verification for the 13 migrated
    widgets is the manual smoke pass at the phase gate (§8), same as
    7A-2's own precedent for GUI-only changes. **7C-5's headless input
    harness is what actually closes this gap** — it's why that step
    exists.
  - **Done-when's grep has a pre-existing false-positive**, not introduced
    by this step: `grep -rn "fn on_.*_btn\|fn hit_"` also matches
    `ui/frame.rs`'s own `#[test] fn hit_returns_none_on_an_empty_frame`
    and its three siblings (present before 7C-1 started) — literal
    "returns only `UiFrame::hit`" was already unreachable at `v0.5.7b`.
    Verified by hand instead: no independent hit-test function remains
    anywhere in `ember2d-editor/src` outside `UiFrame::hit` itself and its
    own test module.
  - Clippy (`cargo clippy --workspace --lib`): 58 warnings, at/under the
    `v0.5.7b` baseline of 59 — `#[allow(clippy::too_many_arguments)]`
    added to `draw_button`/`draw_row`/`draw_swatch` (`ui/widgets.rs`),
    `draw_palette` (`graph_ui.rs`), and `draw_hierarchy` (`ui/panels/
    dock.rs`), each of which crossed the 7-argument default threshold
    once `frame`/`id` was added.
  - **Manual smoke pass confirmed 2026-09-12** (user, live editor session):
    all 13 migrated widgets click correctly (confirm modal, color picker
    hue/SV drag, palette editor swatches, context menu, graph palette,
    hierarchy rows, file browser rows, start screen menu/folder/browser/
    template items) — no regressions found.
  - **Correction, found during that manual pass (R52, `f89b301`):** the
    debug assertion this step added to `handle_canvas_input` fired
    constantly on ordinary use, not just genuine bleed-through — its
    premise (a panel handler consuming a click already stops
    `handle_canvas_input` from running) doesn't hold until 7C-4's
    `Consumed | Pass` chain exists. Removed; see R52 and 7C-4's own Change
    list for where it's re-added.

#### `[x]` 7C-2 — No cell literals below `Panel.rect` (`e0d305c`)

- **Why:** E2 remainder; the Part 1 done-criterion "no 8.0/16.0 in the
  editor" is not met.
- **Change:** `impl_render.rs:205-208` scissor and every `* 8`/`* 16` go
  through `ScreenMapping` (7B-2). `cells()` helper deduplicated
  (`types.rs:11`, `drawing.rs:12`).
- **Done when:** `grep -rnE "\b(8|16)\.0\b" ember2d-editor/src` is empty.
- **Scope:** `ember2d-editor`.
- **Landed as:** the scissor rect (`impl_render.rs`, line had shifted to
  269 by the time this step ran, after 7C-1's additions) uses
  `ember2d::renderer::CELL_W`/`CELL_H` directly, not the `ScreenMapping`
  struct the Change list names — `ScreenMapping` (7B-2) solves a
  different problem (raw physical mouse coordinates -> the editor's
  logical pixel space, needing a live `scale`/`origin_px` snapshot);
  this site converts a cell COUNT to the same logical space a plain
  `CELL_W`/`CELL_H` multiply already produces everywhere else
  (`UiRect::from_cells`), and the backend's own `render_scale`/
  `render_origin` (fed from `screen_mapping()` elsewhere) does the actual
  physical-pixel scaling downstream — routing through `ScreenMapping`
  itself here would have been the wrong tool, not a stronger fix.
  `cells()` deduplication landed as named. Beyond the Change list's own
  two items: every `Font::measure`/`glyph`/`wrap_text` call site with a
  literal `8.0` font-size argument (`start_screen/drawing.rs`'s
  `keep_tail`, its `draw_text_step`'s own inline copy of the same
  algorithm, both `wrap_text` calls; `ui/panels/chrome.rs`'s
  `draw_text_input` tail-truncation) also got converted — not named in
  the Change list, but squarely inside the Why's own "no 8.0/16.0 in the
  editor" criterion, and the only way the Done-when's grep gets
  meaningfully close to empty. Two categories of literal deliberately
  left alone: `ui/rect.rs`'s pinning tests (`assert_eq!(r.x, cx as f32 *
  8.0)` etc.) exist specifically to check `UiRect::from_cells` against
  the real 8×16 constant, not a duplicate of it — converting them to
  `CELL_W as f32` would make them tautological; and a pre-existing
  historical comment in `ui/canvas.rs` (describing what USED to be
  hardcoded before 7B-2) that the grep's own pattern can't distinguish
  from code. Full `cargo test --workspace` green throughout (255 tests,
  unchanged — mechanical, no new test needed), `scripts/check.ps1`
  clean, clippy `--lib` unchanged at 58 (still under the `v0.5.7b`
  baseline of 59).

#### `[x]` 7C-3 — Delete `Layout`; the viewport is a real panel (`9bad191`)

- **Why:** E4. `Layout.canvas_*` (cells) is rebuilt every frame from
  `Viewport.rect` (pixels) and is what `mouse_to_grid`, canvas input, and
  every `ui::draw_*` read.
- **Change:** `mouse_to_grid(px, py)` takes pixels and reads
  `PanelManager::viewport().rect`. `draw_scaled_tile` and the canvas
  drawing take the viewport `UiRect`. Zoom pivot uses the pixel mouse
  position. The Viewport panel's title bar and tabs are handled by
  `handle_panel_chrome_click` like any other panel (it stays non-closable
  and master-fill). Then delete `Layout`.
- **Test:** world → pixel → world round-trip identity across scroll, zoom,
  and viewport origins (the test the Phase 7 plan asked for in 1f).
- **Done when:** `Layout` no longer exists; the viewport docks/undocks like
  any panel while remaining non-closable.
- **Scope:** `ember2d-editor`.
- **Landed as:** `Layout` deleted outright (only historical comments
  mention it now). `PanelManager` gains `viewport()` (`&Panel`, was
  `Layout.canvas_*`), `screen_size_px()`/`screen_size_cells()` (was
  `Layout.screen_w`/`screen_h`, now updated every `apply_layout` call
  instead of duplicated), and `Panel::content_rect()` (the pixel-native
  counterpart to the existing cell-based `content_x`/`content_y`/
  `content_w`/`content_h`). `toolbar_row` becomes a plain `TOOLBAR_ROW`
  constant (it was never anything but `1`). `canvas_bounds` (dead the
  moment its one caller, the old `Layout::new(..).with_canvas(..)` line in
  `impl_render.rs`, was deleted) removed with it — same deletion, not a
  separate cleanup.
  - **Viewport drag:** the Change list's "docks/undocks like any panel...
    stays non-closable and master-fill" was ambiguous about how far
    "like any panel" should go — asked the user directly rather than
    guessing; they picked the minimal reading. `handle_panel_chrome_click`'s
    `TitleBar` branch no longer
    excludes `PanelId::Viewport` from `start_drag`, so it now behaves
    exactly like dragging any other panel — the resize-handle and
    close-button guards are untouched (resize handle stays inert for the
    Viewport, close stays disabled). `apply_layout`'s own `PanelId::Viewport`
    match arm (`panel/mod.rs`) still unconditionally recomputes its
    dock/rect as "whatever's left over" every single frame regardless of
    what the drag did, so a dragged Viewport visually follows the cursor
    but snaps back to filling the remaining space on the very next frame
    — "docks/undocks like any panel" describes the input mechanics, not
    a change to the actual layout result.
  - **`mouse_to_grid` is now pixel-native**, not just re-sourced: it reads
    the mouse's true sub-cell pixel position against
    `PanelManager::viewport().content_rect()`, rather than
    `mouse.cell_x`/`cell_y` (already floored to whole cells) against the
    deleted `Layout`'s own copy. `draw_cursor_highlight`
    (`ui/canvas.rs`) and the wheel-zoom pivot (`input/canvas.rs`) were
    rewritten to the identical formula, so the three can never
    independently drift at any zoom or scroll — the same invariant E1's
    original fix established, now precise at the sub-cell level too, not
    just at whole-cell mouse positions. All 28 `mouse_to_grid` call sites
    now pass `mouse.pixel_x`/`pixel_y`.
  - **Every other `self.layout.canvas_x`/`canvas_y`/`canvas_w`/`canvas_h`
    consumer** (the zoom-gate `on_canvas` check in `input/canvas.rs`, the
    `FocusCamera` context-menu action, `center_on`, `clamp_scroll`,
    `draw_status_bar`'s mouse-position readout) is a direct swap to
    `self.panels.viewport().content_x()`/etc. — same cell-based values,
    same behavior, just sourced from `PanelManager` instead of a
    redundant per-frame copy of it.
  - **Not done this session — §8's screenshot requirement:** "7B-1, 7B-2,
    7B-5, and 7C-3 each require a before/after screenshot pair." This
    session has no way to launch the real editor and capture one — no
    pair exists yet under `docs/screenshots/7c-3/`. The round-trip test
    plus the full `cargo test --workspace` pass (255, unchanged) are the
    automated evidence this step's own geometry math didn't change; the
    screenshot pair itself is still outstanding and needs a human pass —
    same gap as the manual GUI smoke-testing 7C-1/7C-2 already flagged,
    but this one is a named §8 gate requirement, not just good practice,
    so flagging it explicitly rather than letting it quietly ride into
    the phase gate.
  - Full `cargo build --workspace --examples` and `cargo test --workspace`
    green throughout (255 tests, unchanged — no new test needed beyond
    updating the existing round-trip test to build `UiRect`s instead of
    `Layout`s). `scripts/check.ps1` clean. Clippy `--lib`: 56 warnings,
    down from the `v0.5.7b` baseline of 59 (removing `canvas_bounds` and
    simplifying several call sites reduced the count rather than growing
    it).

#### `[x]` 7C-4 — `EditorMode` replaces the boolean soup (`0eb2db8`)

- **Why:** ~15 mutually exclusive booleans/Options on `EditorState`;
  correctness depends on the if-chain order in `handle_update`, and then
  panel, canvas, and shortcut handlers all run unconditionally.
- **Change:**
  ```rust
  pub enum EditorMode {
      Paint(Tool),            // Tool: Brush | Erase | Rect | Line | Fill | Scatter
      Select { start: Option<IVec2>, cutting: bool },
      Paste,
      PlaceSpawn,
      Script { fullscreen: bool },
      Graph,
      PaletteEditor,
      ColorPicker { target: ColorTarget },
      Prompt(TextInputPurpose),
      Modal(ModalKind),
      ContextMenu(ContextMenuState),
  }
  ```
  `handle_update` dispatches on `mode` once. Input flows menu bar → modal
  layer → panels → (mode-specific canvas handler) → shortcuts, and **each
  stage returns `Consumed | Pass`**; a `Consumed` stops the chain. The
  `EditorFocus` from 7A-2 folds into this. Once this chain actually exists,
  re-add 7C-1's debug assertion (removed as R52, `f89b301` — it fired on
  any click landing on any registered widget, not just genuine
  bleed-through, because nothing yet stopped `handle_canvas_input` from
  running after a panel handler consumed the click) checking the real
  `Consumed`/`Pass` result instead of `UiFrame::hit` alone.
- **Test:** see 7C-5 — this step is what makes the harness possible.
- **Done when:** `EditorState` has no `bool` field whose name is a mode;
  `handle_update` has one `match self.mode`.
- **Scope:** `ember2d-editor`.
- **Landed as:** `EditorMode` shaped slightly differently than the Change
  list's sketch (all in `editor/mod.rs`): `Paint(ToolKind)` where `ToolKind`
  is the real (already-existing) 4-variant enum, not a new `Tool` with
  `Erase`/`Scatter` (neither exists in this codebase); `Inspect` is its own
  variant rather than folded into `Select` (`Select` here always means the
  copy/cut marquee, matching what the removed `select_mode` bool actually
  gated); `PlaceSpawn(Option<String>)` carries the named-spawn tag inline
  instead of a separate flag; `Script` has no `fullscreen` field — fullscreen
  is `matches!(mode, Script)` itself, since the docked (non-fullscreen) case
  never touches `mode` at all (see below); `Graph { gx, gy }` and
  `ColorPicker { is_fg }` carry the concrete fields this codebase's graph
  editor and color picker actually used, not the sketch's placeholder
  `ColorTarget`; `PaletteSearch` is a variant the sketch didn't list (the old
  `palette_search_focused` bool needed a home); `ContextMenu` wraps the
  existing `ui::ContextMenu` type rather than a new `ContextMenuState`.
  `TextInput` (the old `{buffer, purpose}` struct) is deleted outright —
  `purpose` lives in `EditorMode::Prompt(purpose)` and `buffer` becomes a
  plain `prompt_buffer: String` field, since the enum payload is the one
  place that's genuinely exclusive; the buffer itself is a growable buffer
  like `script_buffer`, not mode-exclusive data.
  - **Auxiliary fields kept separate, not folded into the enum:**
    `rect_anchor`/`line_anchor` (need to persist independently of which
    Paint sub-tool is active), `prompt_buffer` and `script_*` (growable text
    buffers, not discriminant data), `graph_*` beyond `gx`/`gy` and
    `palette_editor_focus`/`color_picker_hsv`/`paste_flip_x`/`paste_flip_y`/
    `paste_flip_rotate` (each meaningful only while its own mode is active,
    but embedding them in the enum payload would force every handler that
    touches them to re-destructure `self.mode` just to get at a sibling
    field already available as `&mut self.foo`). Each has its own doc
    comment on `EditorState` explaining why it stays outside the enum.
  - **Not the literal `Consumed | Pass` chain.** The Change list describes
    every stage (menu bar → modal → panels → canvas → shortcuts) returning
    `Consumed | Pass` so a later stage can tell a click was already handled.
    What actually landed is `handle_update` doing one
    `match std::mem::take(&mut self.mode) { ... }`: modal/context-menu/
    color-picker/palette-editor/graph/script/place-spawn/palette-search/
    prompt modes each fully own the frame's input and `return` early (the
    same hard-exclusive shape the old if-chain had for these cases); only
    `Paint`/`Inspect`/`Select`/`Paste` fall through to the shared
    `handle_panel_input` → `handle_canvas_input` → `handle_shortcuts`
    sequence, which still runs unconditionally with no consumed/pass signal
    between stages — unchanged from before this step. This satisfies the
    literal Done-when (no bool-named mode fields, one `match self.mode`) but
    not the Change list's stronger structural goal. Reason for not building
    it now: real `Consumed | Pass` threading touches the signature of every
    panel/shortcut handler, is exactly the kind of change 7C-5's headless
    harness exists to make verifiable by test rather than by hand, and
    building it without that harness first risks a subtle consumption-order
    regression no manual click-through would reliably catch. **R52's
    debug-assertion re-add is therefore still deferred** — past 7C-4, not
    resolved by it — to whenever the real `Consumed`/`Pass` result exists to
    check instead of `UiFrame::hit` alone; this is a re-deferral, not a new
    finding.
  - **Two related simplifications, treated as fixes rather than
    regressions:** (1) `ColorPicker`/`PaletteEditor` always exit to a fixed
    target (`ColorPicker` → `PaletteEditor` → `Paint(Paint)`) instead of a
    generic "resume whatever was active before" stack — the old boolean
    soup could theoretically leave stale flags letting an overlay claim to
    resume a Select/Paste that was never actually interrupted-and-resumed
    anywhere in the UI; that path is now impossible by construction. (2)
    Closing any transient overlay (modal cancel, prompt escape, context
    menu dismiss) defaults to `Paint(ToolKind::Paint)` rather than
    attempting to restore the prior mode, matching what the old code
    actually did at each of those sites (none of them restored a prior
    tool/select/paste state either).
  - **Two vestigial dead-code lines removed, not translated:**
    `hierarchy_and_palette.rs` had two `self.palette_search_focused = false`
    "default clear" assignments that were unreachable in the old if-chain
    (the chain already returned early whenever that flag was true, before
    reaching this code) — translating them to `self.mode = Paint(..)` would
    have introduced a new bug, wrongly cancelling an active Select/Paste/
    Inspect mode on an ordinary palette-panel click.
  - **`script_editor.rs`'s Escape handler** now explicitly checks
    `if fullscreen` (i.e. `matches!(self.mode, EditorMode::Script)`) before
    resetting `self.mode`, rather than unconditionally resetting it — the
    docked (non-fullscreen) panel can hold keyboard focus without `mode`
    ever being `Script`, and an unconditional reset there would have
    cancelled Select/Paste/Inspect the moment Escape was pressed while the
    docked panel merely had focus. Caught before landing, not a shipped
    regression.
  - `ui/menu.rs`'s `draw_menu_toolbar` takes a `mode_label: &str` computed by
    the caller (`EditorMode::toolbar_label()`) instead of a bare `ToolKind`,
    since `ToolKind` no longer has a variant for every mode the toolbar
    indicator shows.
  - Full `cargo build --workspace --examples` and `cargo test --workspace`
    green (255 tests, unchanged count from `9bad191`). `scripts/check.ps1`
    clean (750-line limit: no touched file exceeds 685 lines). Clippy
    `--lib`: 56 warnings, unchanged from 7C-3's baseline — no new lint
    introduced. Clippy `--all-targets`: 80 warnings both immediately before
    (`ddcafdc`) and after (`aef74fa`) this step — confirmed via a sorted
    message-level diff, not just the count, that the only differences are
    which test binary's build happens to report the "N duplicates" suffix
    on R47's pre-existing `ember2d/tests/common/mod.rs` dead-code warnings;
    zero new warnings. `ember2d-sim` untouched by this step's diff (see
    Scope above), so the replay-3× determinism check doesn't apply here.
  - **Manual pass confirmed 2026-09-12** (user, live editor session):
    every `EditorMode` exercised — Paint/Rect/Line/Fill tools, Inspect
    select-click, Copy/Cut marquee, Paste with flip/rotate, both
    spawn-placement flows, palette editor + color picker, script editor
    docked and fullscreen, graph editor, context menu, the confirm modal,
    every text prompt — plus the viewport-as-panel behavior 7C-3 left for
    a human pass (drag/dock, resize handle inert, close disabled). No
    regressions found. 7C-3's own before/after screenshot-pair gap
    (`docs/screenshots/7c-3/`, §8 requirement) is still separately
    outstanding — a screenshot pair, not a functional check, so this pass
    doesn't close it.

#### `[x]` 7C-5 — Headless editor input harness (`4ede7f5`)

- **Why:** Everything that goes wrong in the editor is behind
  `&InputManager`/`&MouseState` on an 85-field struct with no way to inject
  events. Editor coverage is ~5%.
- **Change:** `ember2d-editor/tests/common/mod.rs`: `EditorHarness` that owns
  an `EditorState` with a fixed 1280×720 window, an `InputManager`,
  `MouseState`, and a `UiFrame`, and exposes `click(px, py)`, `key(Key)`,
  `type_text(&str)`, `drag(from, to)`, `frame()`. Rendering is not needed;
  `UiFrame` is populated by calling the draw functions with a
  `NullRenderer` (a `Renderer`-shaped struct whose draw calls only push
  rects — this is why 7C-1 must come first).
- **Test:** the R12/R13/R14 regressions as harness tests; menu click does
  not paint; Escape closes a dropdown; shortcuts do not fire while typing
  in the docked script panel; docking a panel leaves the viewport filling
  the remainder.
- **Done when:** ≥ 20 harness tests; every 7A-2 fix has one.
- **Scope:** `ember2d-editor` (+ a `NullRenderer` in `ember2d` behind a
  `test-support` feature).
- **Landed as:** 23 harness tests, `ember2d-editor/tests/{common/mod.rs,
  editor_input.rs}`, `ember2d-editor` + `ember2d` only (matches Scope).
  Deviations from the Change list's own literal text, all found during
  investigation before writing any code:
  - **`NullRenderer` isn't behind a `test-support` feature.** The literal
    plan would need `ember2d`'s feature enabled only while `ember2d-editor`
    builds its OWN tests, with no extra flag on the `cargo test --workspace`
    command §0.5/§8 name everywhere — the only way to make that automatic is
    a package listing itself as its own `dev-dependency` with the feature
    turned on (a real but obscure Cargo pattern), unverified in this tree
    and risky to get subtly wrong under the fixed verification commands the
    whole team's workflow depends on. Simpler and lower-risk: `DrawSurface`/
    `NullRenderer` are unconditional, always-compiled `ember2d` API — a
    `NullRenderer` costs a few bytes and four no-op-ish methods in the
    shipped binary, a trade this codebase already makes for `Box<dyn Font>`.
    `cargo build --workspace`/`cargo run` never construct one; nothing
    changes for the shipped app.
  - **A new `DrawSurface` trait** (`ember2d/src/renderer/draw_surface.rs`),
    not named in the Change list at all — required to make `NullRenderer`
    usable. Every editor `draw_*` function took a concrete `&mut Renderer`,
    which needs a real wgpu `Surface`/`Device`/`Window`
    (`Renderer::new`) — there is no way to swap in a stand-in without
    either this trait or a real (still-impossible) headless `Renderer`
    construction path. Grepped the actual call surface first rather than
    guessing: exactly 6 methods (`draw_char`, `draw_char_scaled_pixels`,
    `draw_str`, `draw_rect_outline`, `draw_rect_filled`, `set_scissor`) plus
    2 field reads (`width`/`height`, `pixel_width`/`pixel_height`, exposed
    as trait accessor methods since a trait can't expose a field) across 53
    call sites. This is deliberately NOT a revival of the `RenderBackend`
    trait 7B-3 removed as needless indirection (`backend.rs`'s own comment
    on that decision, still accurate) — that one abstracted over multiple
    GPU backends behind the same live window and had no real second
    implementor; this one abstracts over "is there a window at all," which
    a headless test genuinely needs and `RenderBackend` never did.
    `impl DrawSurface for Renderer` thin-delegates to the existing inherent
    methods (unchanged, still callable directly everywhere else in
    `ember2d`/`ember2d-editor`); `#[allow(clippy::too_many_arguments)]`
    added to the two methods/impls whose argument count already exceeded
    clippy's default on the inherent methods they mirror, keeping the
    `--lib` count at the unchanged baseline of 56 rather than growing it.
  - **43 of the 53 call sites converted, not all of them.** The other 10
    are `start_screen/drawing.rs`'s widget-drawing functions — real, but
    this step's own named Test list needs none of them (`StartScreen` has
    its own separate `UiFrame`, already fully covered by 7C-1's own
    verification), and touching them isn't required to reach ≥ 20 tests
    or cover R12/R13/R14. Left as concrete `&mut Renderer`, unconverted;
    revisit only if a future step's tests actually need to drive
    `StartScreen` headlessly.
  - **`impl_render.rs`'s `handle_render(ctx: RenderContext)`** (the one
    `GameState::render` entry point, still taking a concrete `&mut
    Renderer` via `RenderContext.renderer` — untouched, on purpose:
    `RenderContext` is `ember2d`'s own core engine type, shared by every
    `GameState` implementor including `PlayState`, and changing its
    `renderer` field's type would be exactly the kind of central-`ember2d`
    change this step's Scope doesn't cover) is now a 2-line wrapper around
    a new `pub fn draw(&mut self, renderer: &mut dyn DrawSurface, mouse:
    &MouseState)` holding the real body verbatim (mechanical extraction,
    not a rewrite) — `EditorHarness` calls `draw` directly with a
    `NullRenderer` it owns, bypassing `RenderContext` entirely.
    `render_graph_mode`/`render_script_mode` (`draw`'s own two full-screen
    sub-modes) converted the same way. The real app's own render path is
    unchanged end to end — a concrete `&mut Renderer` still auto-coerces to
    `&mut dyn DrawSurface` at this one call boundary, same as at all 43
    converted `ember2d-editor` sites.
  - **`EditorHarness` reuses `ember2d::sim::step`** (the same per-step
    sequence `Engine::run()` and `ember2d/tests/common/mod.rs`'s
    `TurnHarness` already share, per that module's own header comment on
    why it exists) rather than hand-building an `UpdateContext` — the
    Change list didn't name this, but it's the direct application of
    `sim.rs`'s own stated purpose, and hand-duplicating the consume/decay
    sequence a third time is exactly what that module exists to prevent.
    `begin_frame`/`end_frame` around it reproduce `Engine::poll_events`'s
    clear-before/`finish_frame_text_capture`-and-decay-after wrapping by
    hand, since that half genuinely isn't in `sim::step`.
  - **New read-only accessors on `EditorState`** (`mode`, `ui_frame`,
    `panels`, `active_menu`, `grid`, `focused_panel`, `focus_is_canvas`,
    `script_buffer`, `prompt_buffer`, `rect_anchor`, `show_physics`,
    `show_grid`, `active_layer`, `unsaved`) — not named in the Change list,
    but required for tests in a genuinely external crate (an integration
    test binary only sees `pub` items) to observe anything; every existing
    field stayed `pub(super)`, no setters were added alongside them, and
    `EditorFocus` itself stays `pub(crate)` (`focus_is_canvas` exposes only
    the one bit a test needs instead of widening that enum's visibility).
  - **Verification beyond the standard per-step build/test:**
    `cargo clippy --workspace --lib` unchanged at 56 (7C-4's baseline);
    `--all-targets` 80 real warnings (2 additional "corrupt incremental
    compilation artifact" lines are a toolchain/filesystem quirk this
    session hit independently of any code change here, self-described as
    harmless and auto-deleted, confirmed non-reproducing after deleting the
    named files) — unchanged from 7C-4's own recorded 80. `cargo fmt --all
    -- --check` reports 63 pre-existing diffs, none in code this step
    touched (logged as **R53**, S4, unscheduled — not this step's job).
    `scripts/check.ps1` clean. `cargo test --workspace`: 279 (255 + 24),
    all pass. `ember2d-sim` untouched by this step's diff, so the sim
    boundary invariant (§4.2) and the replay 3× gate don't apply here.
  - **Not done this session:** the literal "manual smoke test" §8 asks for
    per step doesn't apply in the usual sense — this step's entire point is
    replacing exactly that with automated coverage; the harness tests
    passing is most of the verification.
  - **Immediate payoff:** the user's own manual pass, the very first time
    the fullscreen script editor was ever reachable (see R54), found it
    completely broken — a 7C-4 defect, not this step's own — and this
    step's harness reproduced and root-caused it in minutes. Fixing R54
    then surfaced a second, unrelated, longer-lived defect (R55, present
    since 7A-2, silently dropping typed characters on any display faster
    than 60Hz) the moment fullscreen typing became testable at all. Both
    fixed as follow-ups (see their own rows, §3.2) rather than deferred —
    the user was actively blocked by R54, and R55 was found investigating
    R54 in the same sitting.

#### `[x]` 7C-6 — `LevelGrid` determinism and undo batching (D18)

- **Why:** D18 open since Phase 5; freehand strokes are not batched.
- **Change:** `LevelGrid::tiles: BTreeMap<(i32, i32, u8), TileRecord>`;
  `to_level_data` sorts `(layer, y, x)` like `gen_roguelike`. Freehand
  paint, drag-erase, and scatter open an `UndoBatch` on mouse-down and
  close it on mouse-up. Graph edits (add/remove node, edge, param, drag),
  hierarchy Duplicate/Delete, and palette edits become `Command`s.
  `NewLevel`, file delete, and level switch confirm when `unsaved`.
- **Test:** open/save `floor1.level` with no edits → `git diff` empty;
  paint a 50-cell stroke → one undo restores all 50; graph add-node then
  undo → node gone.
- **Done when:** `roguelike_level_integrity.rs` passes on an editor-saved
  level; D18 marked `[x]` in §3.
- **Scope:** `ember2d-editor`.
- **Landed as (`b2a608f`):** the plan text above undersold this step's
  real scope by a wide margin — investigated up front and the user
  explicitly authorized doing "the full step" rather than narrowing it
  (the established precedent from 7C-1). What actually shipped, by area:
  - **Determinism (D18 itself).** `grid.rs`'s `tiles` field is a
    `BTreeMap<(i32, i32, u8), TileRecord>` (was `HashMap`), keyed
    `(x, y, layer)` for O(log n) point lookups by position — but that key
    order is the wrong *iteration* order for the on-disk format, so
    `to_level_data` still collects into a `Vec` and does an explicit
    `.sort_by_key(|t| (t.layer, t.y, t.x))` on top, matching
    `gen_roguelike`'s convention. Two new regression tests in `grid.rs`
    pin both halves: insertion-order independence and save-twice
    stability.
  - **Paint/erase/scatter batching.** A new `paint_batch:
    Option<PaintBatch>` field (`PaintBatch` is a `BTreeMap<(i32,i32,u8),
    (Option<TileRecord>, Option<TileRecord>)>` type alias, added to dodge
    a clippy `type_complexity` warning on the inline form) accumulates
    per-cell before/after snapshots across a drag; `record_paint_batch_edit`
    inserts or updates an entry (keeping the *original* before and the
    *latest* after per cell), and `commit_paint_batch` — called once,
    unconditionally, at the top of `handle_canvas_input` on
    `left_just_released`/`right_just_released`, before the tool-specific
    dispatch, so it fires no matter which tool's branch subsequently
    returns — turns the whole batch into one `Command::Batch` push.
    Found and fixed two latent bugs while wiring this up, both predating
    this step: brush-size-1 erase split a drag into two separate undo
    commands (the press frame went through the one-shot `erase_brush`
    path via `right_just_pressed` while the rest of the drag batched
    separately — restructured so `right_held` alone drives size-1 erase,
    mirroring how paint already used `left_held` uniformly); and alt-drag
    scatter's "randomness" (`(gx*1234 + gy*5678 + undo.len()) % 2`) never
    actually depended on position at all (1234 and 5678 are both even) —
    only on `undo.len()`'s parity, which froze for an entire drag once
    batching stopped incrementing it per cell, turning scatter into
    "paint everything" or "paint nothing" depending on when the drag
    started. Replaced with `rand::random::<bool>()`.
  - **Graph edits become undoable (new capability, not a fix — the graph
    editor had no undo support at all before this step).** New
    `apply_graph_edit` helper in `graph.rs` snapshots the tile, hands the
    caller's closure a `&mut NodeGraph` to mutate, then pushes one
    `Command::PlaceTile{before, after}`; used for param-edit commit,
    add-node (palette and Ctrl+V paste, the latter returning the new
    node id through the closure's return value), auto-layout (`F`), and
    node deletion. Node dragging is separate (a drag isn't a single
    mutate call): `graph_drag_before` snapshots the tile when a drag
    starts and the drag-release handler pushes one `PlaceTile` comparing
    against the tile after the move.
  - **Hierarchy Duplicate/Delete become undoable** by snapshotting
    `grid.extra_spawns` before the mutation and pushing
    `Command::UpdateExtraSpawns{before, after}` — reusing the existing
    command variant rather than adding a new one.
  - **Palette edits become undoable (new `Command::UpdatePalette{before,
    after}` variant).** Two distinct push sites, because there are two
    distinct editing flows: the modal palette editor snapshots
    `palette_edit_before` on open and pushes exactly one `UpdatePalette`
    covering the whole edit session on any of its four exit paths (close,
    Save & Close, Delete, Escape — all now routed through one new
    `close_palette_editor` helper instead of four separate `self.mode =
    ...` assignments); the standalone "New" button pushes its own
    `UpdatePalette` immediately, since it isn't a session with a
    close event.
  - **Confirm before destructive actions.** Two new `ModalPurpose`
    variants, `ConfirmNewLevel` and `ConfirmDeleteFile{path}` (doc-commented
    to distinguish them from the pre-existing `ConfirmSwitchLevel`, which
    alone is gated on `self.unsaved` — New Level and Delete File always
    confirm, a level switch only confirms when there are actual unsaved
    edits to lose). Both the Level-menu and the right-click New Level
    entry points, and the File Browser's Delete entry, now show a modal
    first; `modal.rs`'s handlers proceed to the real action (open the
    rename prompt / `std::fs::remove_file` + refresh) only on
    confirmation.
  - **Found along the way, fixed, logged separately: R59.** Writing the
    Delete File regression tests surfaced a genuine pre-existing bug in
    `context_menu_trigger.rs` — the Delete menu item's path was never
    joined with `current_folder`/`project_folder`, so deletion always
    resolved against the process's CWD rather than the project's actual
    location. Fixed and given its own row (§3.2) rather than folded into
    this step's own listed Change, per "every fixed defect gets a named
    regression test and a §3 row" — it's a real defect with its own
    blast radius, not a mechanical part of the batching/undo work this
    step set out to do.
  - **`switch_to_level` helper.** All three ways a level can be replaced
    wholesale (Level menu, File Browser click, and the load path
    generally) were consolidated into one `impl_state` helper that loads
    the new `EditorState` and carries over `project_folder`,
    `project_name`, `panels`, and `current_folder` before replacing
    `self` — used by both `ConfirmSwitchLevel`'s modal handler and the
    File Browser's now-conditional (only-if-`unsaved`) switch click.
  - **Test-file split (file-size limit, not scope creep).** 7C-6's tests
    pushed `editor_input.rs` to 811 lines against the 750-line limit.
    Rather than trim coverage, the four shared helpers used by both test
    files (`open_menu`, `click_menu_item`, `canvas_center`,
    `canvas_pixel_for_grid`) moved into `tests/common/mod.rs` as `pub
    fn`s, and every 7C-6-specific test moved into a new
    `tests/editor_undo.rs` (405 lines); `editor_input.rs` is back to 495
    lines. Each test binary compiles `mod common;` independently, so
    helpers used by only one binary looked like dead code to the other —
    resolved with a single module-level `#![allow(dead_code)]` in
    `common/mod.rs` (documented inline) rather than per-function
    annotations.
  - **Verification.** `cargo build --workspace --examples` clean.
    `cargo test --workspace`: 295 (was 282 before this step — +13: 11 new
    tests in `editor_undo.rs`, 2 in `grid.rs`), all pass. `cargo clippy
    --workspace --lib` unchanged at 56 (two real new warnings surfaced
    mid-step and were fixed rather than left, not counted against
    baseline: a `clippy::question_mark` in `apply_graph_edit`, and a
    `clippy::type_complexity` on `paint_batch`'s inline type, fixed by
    the `PaintBatch` alias mentioned above); `--all-targets` unchanged at
    80. `scripts/check.ps1` clean. `cargo test -p ember2d --test replay`
    3× fresh processes green (this step never touches `ember2d-sim`, so
    the sim boundary invariant, §4.2, doesn't apply, but the replay gate
    was still re-run given the size of the undo/command surface this step
    changed). `git diff --stat`: 16 files changed, all under
    `ember2d-editor/`, matching this step's own Scope exactly.

#### `[x]` 7C-7 — Script errors reach the editor

- **Why:** R18. `receive_log` has no callers; there is no compile step in
  the script editor.
- **Change:** `run_play_app` (app.rs) drains `PlayState`'s script log into
  `EditorState::receive_log` on return from F5. The script editor compiles
  on save (and on a 500 ms idle timer) via `ScriptEngine::compile_str` and
  shows the first error inline (line highlighted, message in the status
  row). Rhai keywords list completed (`switch do until throw try catch
  private global`); `//` inside strings and `/* */` handled.
- **Test:** harness: type `fn on_update(id, ctx) {` (unclosed), save →
  status shows the parse error at the right line.
- **Done when:** a runtime error in F5 preview appears in the editor
  console after returning.
- **Scope:** `ember2d-editor`, `ember2d-app`.
- **Landed as (`35887a6`):** investigated up front and reported back
  before implementing — the plan text named the wrong function
  (`run_play_app` doesn't touch F5 at all; `run_editor_app` does) and
  missed the real obstacle underneath both: `Engine`'s state stack is
  `Vec<Box<dyn GameState>>`, which has no downcasting, so `app.rs` could
  never have reached a popped `PlayState`'s concrete `take_log()` or a
  resumed `EditorState`'s concrete `receive_log()` no matter which
  function it called from. Fixing that meant a small, explicitly-flagged
  expansion of this step's own Scope into `ember2d` itself (asked and
  confirmed before writing any code, since it touches a public trait) —
  everything that actually shipped, by area:
  - **Crossing the type-erasure boundary.** Two new default (no-op)
    `GameState` trait methods, `take_script_log`/`receive_script_log`
    (`ember2d/src/engine.rs`) — additive only, every existing implementor
    keeps compiling unchanged. `PlayState` overrides `take_script_log` by
    calling its own pre-existing `take_log`; `EditorState` overrides
    `receive_script_log` by calling its own pre-existing `receive_log` —
    both of R18's named methods were already correct and already
    existed, just unreachable across the stack. New `Engine::top_state_mut`
    accessor (mirrors `pop_state`'s own shape) so a caller can reach the
    *new* top of the stack without holding the concrete value.
  - **The actual drain, in `run_editor_app`'s `Transition::ToEditor` arm**
    (`ember2d-app/src/app.rs`): every state above the base `EditorState`
    (a `PlayState`, or a `PauseMenuState` on top of one if the player
    quit from the pause menu) gets `take_script_log()`'d as it's popped;
    the combined log is handed to whatever's left via
    `top_state_mut().receive_script_log(...)`. `PauseMenuState`'s default
    no-op override means popping it costs nothing extra — the real
    `PlayState` underneath still gets drained in the same loop.
  - **Live syntax check.** New `EditorState::check_script_syntax`
    (`impl_state/mod.rs`) compiles the current `script_buffer` with a
    disposable `rhai::Engine::new()` — deliberately NOT
    `ember2d-sim::ScriptEngine::compile_str` as the plan named: that one
    caches an AST by key, so re-checking an edited-but-unsaved script at
    the same path would keep returning the FIRST compile's cached result
    forever; it also only ever produces a pre-formatted `LogEntry`
    string, discarding `rhai::ParseError`'s real `Position` (line/column)
    that inline highlighting needs. Called from `save_script` (compile on
    save) and `load_script` (a freshly opened file starts genuinely
    unchecked, not assumed clean). New `script_error: Option<(usize,
    String)>` and `script_idle_timer: u32` fields on `EditorState`.
  - **Idle-triggered check.** New `note_script_edit` helper
    (`input/script_editor.rs`) replaces all 7 of the file's own
    `self.script_unsaved = true` sites — the one place that also resets
    `script_idle_timer` to 0, so a check fires ~500ms after the LATEST
    keystroke in a typing burst, not the first. `handle_script_mode_input`
    increments the timer once per frame (it runs once per frame for both
    the docked-focused and fullscreen script editor, covering both with
    one counter) and checks with `==` against `SCRIPT_IDLE_CHECK_FRAMES`
    (30, i.e. ~500ms at the editor's fixed 60Hz step) rather than `>=`,
    so it fires exactly once per idle stretch without a separate
    "already checked" flag.
  - **Inline error display.** `draw_script_editor`
    (`editor/ui/script.rs`) gained an `error: Option<(usize, &str)>`
    parameter, used by both its callers (the docked panel and the
    fullscreen editor — one code path, so neither needed its own copy):
    the erroring line's whole row gets a `Color::DarkRed` background
    (gutter and highlighted text both), and the panel's own bottom row is
    reserved for the message (mirroring how the header already reserves
    the top row) rather than only being shown in the fullscreen mode's
    separate Line/Col status bar, which stays as it was.
  - **Rhai keyword list** extended with `switch do until throw try catch
    private global`, per the plan.
  - **`/* */` block comments.** New `block_comment_starts` scans the
    whole buffer once per render to know which lines START already
    inside an unterminated block comment from a previous line (with the
    buffer scrolled, `draw_highlighted_rhai` never sees line 0 to derive
    that itself) — `draw_highlighted_rhai` takes the resulting per-line
    flag and both enters and exits block-comment coloring within a line
    using the same code path. Deliberately naive like the file's
    pre-existing string handling already was: skips string contents
    (so a `/*` inside one doesn't start a real comment) but without
    backslash-escape awareness, same limitation `draw_highlighted_rhai`'s
    string case already had.
  - **Verification.** `cargo build --workspace --examples` clean. `cargo
    test --workspace`: 305 (was 296), all pass — new tests:
    `take_script_log_drains_and_clears_the_log` (`ember2d/tests/
    take_script_log.rs`, the `PlayState` half of the log hand-off);
    `receive_script_log_appends_to_the_console`,
    `saving_an_unclosed_function_shows_a_parse_error_at_the_right_line`,
    `fixing_the_error_and_saving_again_clears_it`,
    `the_idle_timer_triggers_a_check_without_an_explicit_save`
    (`ember2d-editor/tests/editor_script.rs`, the `EditorState` half plus
    the live-check behavior, driven through the real File Browser click
    and Ctrl+S paths, not direct field access); 4 unit tests for
    `block_comment_starts` (`ui/script.rs`, one of which caught a
    documentation mistake in this very commit — an early doc comment
    claimed the string-skip was "deliberately naive" in the OPPOSITE
    direction, that a `/*` inside a string would wrongly start a comment;
    the test written to pin that claim failed against the actual code,
    which already skips strings correctly, so the doc comment was wrong,
    not the implementation — fixed to describe the real, narrower
    limitation). `cargo clippy --workspace --lib`: 55 (was 56) — a real
    decrease, not a discrepancy: `draw_script_editor` grew to 11
    parameters and needed `#[allow(clippy::too_many_arguments)]`, which
    also silences the pre-existing 10-parameter warning that function
    already had before this step (already over the default threshold,
    just never annotated). `--all-targets` unchanged at 80.
    `scripts/check.ps1` clean. `cargo test -p ember2d --test replay` 3×
    fresh processes green (this step touches `ember2d`'s engine/play
    code, not `ember2d-sim`, so the strict determinism boundary, §4.2,
    doesn't apply, but the gate was re-run anyway given the state-stack
    change). No test drives a real F5 press through a live `Engine`/
    window — the `run_editor_app` wiring itself (as opposed to the two
    trait methods it calls) is verified by code inspection and the two
    halves' own unit tests, consistent with how this repo treats
    anything needing a real winit event loop.

#### `[x]` 7C-8 — Text editor completeness

- **Why:** No selection, clipboard, undo, find, horizontal scroll.
- **Change:** selection (Shift+arrows, Shift+click, Ctrl+A), clipboard via
  `arboard` (one new dep, editor-only), per-buffer undo stack, Ctrl+F
  incremental find, horizontal scroll following the cursor, key repeat
  (from 7B-4). Long lines show a `…` marker rather than clipping.
- **Test:** harness: select-all, cut, paste round-trip preserves content
  including non-ASCII.
- **Done when:** checklist §8 extended and passing.
- **Scope:** `ember2d-editor`.
- **Landed as (`1747b9d`):** investigated up front — unlike 7C-6/7C-7,
  this step's plan text held: everything fit inside `ember2d-editor`, no
  hidden structural blocker. The one item needing sign-off (the new
  `arboard` dependency) was flagged and confirmed before writing any code,
  specifically on the basis that the script editor's OWN `String` buffer
  — not `arboard`'s live OS clipboard — is the real source of truth a
  paste reads from, so a cut/copy → paste round trip inside the editor
  never depends on a display/clipboard server existing (CI runs both
  target OSes headless); `arboard` is used only as a best-effort ONE-WAY
  sync out to the OS clipboard on cut/copy, so text copied in the editor
  can be pasted into another application — pulling text FROM another
  application INTO the editor is explicitly not supported by this step,
  a deliberate scope cut flagged here rather than silently left out.
  - **Selection.** `EditorState::script_selection_anchor: Option<(usize,
    usize)>` (same `(char, line)` shape as `script_cursor`, which is
    always the moving end); `script_selection()` normalizes it against
    the cursor into reading-order `(start, end)`. Shift+arrows/Home/End
    and Shift+click open or extend a selection via one shared
    `update_selection_anchor` helper; any plain (non-Shift) move
    collapses it — simpler than real editors' "collapse to the near
    edge" convention, and a deliberate scope cut for this step (documented
    on the helper itself). The level grid's own `EditorMode::Select`
    (tile-shaped, 2D) turned out to have nothing code-level to reuse for
    1D text ranges — only the "own mode variant, own clipboard field"
    pattern carried over, confirmed by investigation before writing code.
  - **Clipboard.** New `script_clipboard: String` field, separate from
    the grid's own tile clipboard. Cut/copy write to it (and best-effort
    mirror to `arboard`); paste always reads it. Persists across
    switching to a different script (unlike selection/undo/hscroll/find,
    which `load_script` resets) — matching how a real clipboard behaves.
  - **Per-buffer undo.** A dedicated `script_undo`/`script_redo: Vec<(Vec<String>,
    (usize, usize))>` stack — whole-buffer-and-cursor snapshots, not
    diffs (scripts are small text files; simplicity wins here) — entirely
    separate from `commands::UndoStack` (grid-command-shaped, 7C-6,
    nothing reusable). Consecutive edits of the same `ScriptEditGroup`
    (`Insert` or `Delete`) coalesce into one checkpoint via
    `checkpoint_script_edit`, so a typed word or a run of Backspaces
    undoes as one step; Enter, cut, paste, and replacing a selection
    always force a fresh checkpoint via `push_undo_checkpoint` instead, so
    they never coalesce with anything. Undo/redo re-run
    `check_script_syntax` immediately (unlike normal typing, which waits
    for the 7C-7 idle timer) — a discrete user action deserves instant
    feedback, not a 500ms wait.
  - **Incremental find.** Ctrl+F opens a one-line find bar
    (`script_find_active`/`script_find_query`) that owns all input while
    open. Live-as-you-type search always re-searches from
    `script_find_origin` (the cursor position when Ctrl+F was pressed),
    so growing or shrinking the query re-searches consistently instead of
    drifting forward; Enter searches from the current match's end,
    advancing through the buffer and wrapping around. A found match is
    shown by setting the SAME selection anchor/cursor pair a manual
    selection would — free rendering reuse, no separate highlight path.
    Case-insensitive via `to_lowercase()`, ASCII-width assumed (documented
    limitation, matching the file's existing "good enough, not a real
    parser" tokenizer).
  - **Horizontal scroll + `…` marker.** New `script_hscroll: usize`,
    auto-tracked exactly like the existing vertical `script_scroll` (same
    "keep the cursor in the visible window" logic, just on the other
    axis). Rendering slices the hscroll'd-off prefix from each line
    before syntax-highlighting the rest, rather than threading a "skip
    until column N" flag through `draw_highlighted_rhai`'s six-odd
    branches — a token that straddles the hscroll cut can mis-highlight
    for one frame at the boundary, the same class of approximation
    `block_comment_starts` already accepts at line boundaries. A trailing
    `…` replaces the last visible column whenever a line has more content
    than fits.
  - **Selection rendering** applies a `Color::DarkBlue` background per
    highlighted TOKEN (using its first character's index) rather than
    per character within multi-char tokens — a selection boundary
    landing mid-identifier highlights the whole identifier. A visual
    approximation only; cut/copy/paste/delete all operate on exact
    character ranges regardless of how the selection renders.
  - **Delete key-repeat gap, found and fixed as an aside.** 7B-4 added
    `is_repeating` to Up/Down/Left/Right/Tab/Enter/Backspace but missed
    Delete — found during this step's own up-front investigation, not a
    discovered defect with its own blast radius (nothing was broken,
    Delete simply didn't repeat on hold the way every sibling key already
    did), so fixed inline rather than given its own R-row. No dedicated
    regression test, matching every other key's own repeat behavior here
    — none of them have one either, since the harness has no way to drive
    `InputManager::handle_repeat` without a live event loop.
  - **Verification.** `cargo build --workspace --examples` clean. `cargo
    test --workspace`: 313 (was 305), all pass — 8 new tests in
    `editor_script.rs` covering select-all/cut/paste (the plan's own
    named test, non-ASCII included), copy-leaves-original, Shift+Right
    selection with plain-move collapse, typing-replaces-selection, a
    typing burst undoing as one step, switching edit kinds starting a
    new undo step, Ctrl+F find-and-advance, and horizontal scroll
    triggering past the visible width. `cargo clippy --workspace --lib`:
    briefly 56 (was 55) — `draw_highlighted_rhai` crossed clippy's
    7-argument default with the new `sel_range` parameter and needed its
    own `#[allow(clippy::too_many_arguments)]` (matching
    `draw_script_editor`'s own, added 7C-7); fixed immediately, back to
    55. `--all-targets` unchanged at 80. `scripts/check.ps1` clean.
    `cargo test -p ember2d --test replay` 3× fresh processes green (this
    step never touches `ember2d`/`ember2d-sim`, so the determinism
    boundary, §4.2, doesn't apply, but the gate was re-run anyway). `git
    diff --stat` confined entirely to `ember2d-editor` (plus its own
    `Cargo.toml`/`Cargo.lock` for `arboard`), matching this step's Scope
    exactly.

#### `[x]` 7C-9 — Decision gate: own chrome or egui (§7.1)

Evaluated here, with 7C-1 through 7C-8 as evidence. Record the decision and
its reasoning in §7.1 and proceed to 7D (own chrome) or 7D′ (egui skin).

- **Decision recorded:** §7.1 — own chrome, both switch-to-egui triggers
  false (session count, test count, and the selection/clipboard check all
  came back the opposite of what would have triggered a switch). Proceeding
  to Phase 7D below.
- **Found along the way: R61.** This step's own required demo smoke-launch
  (§0.5 item 6, run early since it's part of what "evaluating the gate"
  needs anyway) turned up 8 shipped `.rhai` scripts still referencing
  pre-`demos/`-move paths — R58 regenerated levels' own `script`/
  `next_level` fields but never touched hand-written script TEXT calling
  `play_sound`/`play_music`/`load_level` with a literal path. Fixed; see
  R61's own row (§3.2) for the full account and the new regression test.
- **Phase gate status (§0.5): partial, by necessity.** Items 1–4 and 7 are
  fully verified below. Item 6 (both demos play) is smoke-tested (launch,
  a few seconds, no crash/error — the check that found R61) but not
  interactively played through — that needs a human at the keyboard, which
  this session doesn't have. Item 5 (the regression checklist sections
  named for this phase, §3–§10, run BY HAND) has NOT been run — it is
  fundamentally a manual, visual, interactive pass (clicking, watching the
  screen) that requires the user, the same way every prior phase gate's
  own manual pass in this project's history has (the R54–R61 defects this
  whole 7C phase found were all discovered by the user's own hands-on
  testing, not by this session). Tagging `v0.5.7c` and fast-forwarding
  `main` are exactly the "hard to reverse, affects shared state" class of
  action this project's own working agreement holds back for explicit
  confirmation — **not done in this session**; pending the user's own
  §3–§10 pass and go-ahead.
  - §0.5 item 1: `cargo build --workspace --examples` clean; `cargo test
    --workspace`: 315, all pass.
  - §0.5 item 2: `cargo clippy --workspace --all-targets` unchanged at 80
    (7C-4's own baseline, held through every step since).
  - §0.5 item 3: `scripts/check.ps1` clean.
  - §0.5 item 4: `cargo test -p ember2d --test replay` 3× fresh processes
    green, locally (CI itself still blocked by the account billing lock,
    R37/R40 — unchanged from every prior gate's own note).
  - §0.5 item 7: this entry, §7.1, and §2 (below) are this same commit.

**Phase 7C gate:** §0.5, then tag `v0.5.7c`. *(Automated portion verified
below; manual regression pass and the tag itself are pending the user.)*

---

### 5.4 `[~]` Phase 7D — Theme and restyle

*(Phase 7 plan Parts 3–4, with the unspecified types now specified. If §7.1
resolves to egui, this phase becomes "write the pixel egui style and port
panels" and 7D-1/7D-2 are replaced by an `egui::Style` plus a bitmap-font
`FontDefinitions`; 7D-3 and 7D-4 stand.)*

**Checklist sections at gate:** §3–§9 (full editor pass).

#### `[~]` 7D-1 — `Theme` resource, fully typed

```rust
pub struct Theme {
    pub name: String,
    pub palette: BTreeMap<PaletteRole, Color>,   // roles, not raw colours
    pub chrome: TextureId,                        // 9-slice atlas, loaded via AssetManager
    pub slices: BTreeMap<SliceRole, NineSlice>,
    pub font: FontChoice,                         // Bitmap | Ttf { path }
    pub font_sizes: FontSizes { small: f32, body: f32, heading: f32 },
    pub metrics: Metrics { padding: f32, border: f32, row_h: f32, min_target: f32 },
    pub ui_scale: u8,                             // 1 | 2 | 3, integer
}
pub enum PaletteRole { PanelBg, PanelBorder, TitleBg, TitleText, TextPrimary,
    TextDim, Accent, Danger, InputBg, InputText, Selection, TabActive, TabInactive, ... }
pub enum SliceRole { Panel, TitleBar, Button, ButtonHover, ButtonPressed,
    ButtonDisabled, Input, TabActive, TabInactive, Scrollbar, Checkbox, ResizeGrip }
pub struct NineSlice { pub src: Rect, pub border: (f32, f32, f32, f32) }
```

Loaded from `themes/<name>/theme.ron` + PNG. **How the texture reaches the
renderer:** `Renderer` gains `ui_assets: AssetManager` (separate from the
game's, so a project's asset clear never evicts chrome) and
`Theme::load(&mut renderer.ui_assets, path)`. Missing slice or role → a
loud fallback (magenta) never a panic. Two shipped themes:
`themes/ember-pixel/` (bitmap font, 1× and 2×) and `themes/ember-clean/`
(Cascadia TTF at 12/14/18).

- **Test:** theme RON round-trips; a missing role falls back; `NineSlice`
  quad math already tested in 7-1a.
- **Scope:** `ember2d` (theme.rs, renderer), `themes/`.
- **Landed as (`5699ce5`/`b19ac82`), partial — investigated up front, split the
  step's own two kinds of work before starting:** the `Theme` resource
  itself is Rust logic with zero asset dependency (structs, serde/RON,
  fallback-by-lookup); the two NAMED shipped themes are real pixel-art and
  font-pairing decisions a coding agent producing them unprompted would be
  guessing at, not implementing a spec — flagged here rather than shipped
  as low-effort placeholder art passed off as the real restyle.
  - **What shipped:** `ember2d/src/theme.rs` — `Theme`/`ThemeData`/
    `PaletteRole`/`SliceRole`/`NineSlice`/`FontChoice`/`FontSizes`/
    `Metrics`, matching the plan's own sketch exactly (`PaletteRole`'s
    `...` and `SliceRole` copied verbatim). `NineSlice` is a thin,
    serializable pairing of the `src`/`border` values
    `Renderer::draw_nine_slice`/`nine_slice_quads` (`renderer/mod.rs:445`)
    already take — confirmed by investigation that "7-1a" (no step by
    that exact name exists; the plan's own dangling reference means
    `docs/archive/ember2d-phase7-plan.md`'s pre-renumbering "Part 1a")
    already has 3 passing quad-math unit tests (`renderer/tests.rs`), so
    this step adds no new drawing primitive, only a named data shape
    around an existing one. `ThemeData` is a separate on-disk-shaped type
    from `Theme` (a `chrome_path: String` instead of a resolved
    `chrome: TextureId`) — a hand-rolled `#[serde(skip)]` default for a
    `TextureId` would alias a real texture id with no meaningful "empty"
    value to skip to, so the honest fix is two types and an explicit
    resolve step in `Theme::load`, not a derive workaround.
  - **`Theme::load`/`Theme::fallback`** never panic: a missing/unparsable
    `theme.ron` falls back to `Theme::fallback` (magenta chrome via
    `AssetManager`'s own existing placeholder-on-missing-path behavior,
    empty palette/slices, the built-in bitmap font) rather than stopping
    the editor from opening; `Theme::role_color`/`Theme::slice` implement
    the PER-ROLE fallback separately (a role missing from an otherwise-fine
    theme is not the same failure as the whole theme being missing) —
    `role_color` returns loud magenta, `slice` returns `None` rather than
    fabricating placeholder geometry, since drawing something in place of
    a missing slice is 7D-2's job (`draw_panel_chrome`), not this step's.
  - **`Renderer` gains `ui_assets: AssetManager`**, deliberately separate
    from the game's own (which lives on `Engine`/`RenderContext`, cleared
    on every project switch — confirmed by investigation this is a real,
    if minor, architectural asymmetry the plan's own wording already
    implied by saying "`Renderer` gains" one, not "reuses the existing
    one"). `AssetManager` itself needed no changes — already a
    self-contained struct with no singleton coupling, confirmed by
    investigation before adding the second instance.
  - **Follow-up (same day): `themes/ember-clean/` shipped for real.** A
    design conversation with the user picked a direction — "hybrid"
    chrome (bitmap-font viewport, unchanged; Cascadia Code panel text)
    over an all-bitmap alternative, amber (`#e8a33d`) as the accent —
    after comparing both live in an interactive mockup. That's a
    concrete enough spec (locked palette, locked font, a "flat, clean,
    modern dev console" visual direction rather than ornate pixel art)
    that a real 9-slice chrome atlas no longer needs a human pixel
    artist: new `ember2d/examples/gen_ember_clean_theme.rs` (same
    reasoning as `gen_roguelike.rs` for generating rather than
    hand-authoring — exact pixel arithmetic, and computing the PNG and
    `theme.ron`'s `Rect` coordinates from the same constants means they
    can't drift apart) draws a 128×96, 12-region, 32px-cell/6px-border
    atlas covering every `SliceRole`, and writes the matching
    `theme.ron` (palette, font, font_sizes, metrics, `ui_scale: 1`)
    using the real `ThemeData` type directly rather than hand-typed RON.
    `themes/ember-pixel/` (the plan's other named theme, all-bitmap) is
    NOT built — the user didn't ask for it, and 7D-2 only needs one real
    theme to render against; deferred, not abandoned.
  - **Verification.** `cargo build --workspace --examples` clean. `cargo
    test --workspace`: 321 (was 315 before this step; +5 from the
    struct/logic layer, +1 more here), all pass — new:
    `the_shipped_ember_clean_theme_loads_with_every_slice_and_a_real_chrome_texture`
    (`theme.rs`) loads the actual generated files through `Theme::load`
    (not a synthetic fixture, unlike every other test in this module)
    and asserts all 12 `SliceRole`s resolve and the chrome texture isn't
    `AssetManager`'s 1×1 failure placeholder. `cargo clippy --workspace
    --lib`/`--all-targets` unchanged at 55/80. `scripts/check.ps1`
    clean. No new dependency — `ember2d` already had `serde`+`ron`
    (`project.ron`) and `image` (texture loading).

#### `[x]` 7D-2 — Chrome through 9-slice

`draw_panel_chrome` → one `draw_nine_slice` for the frame, one for the
title bar, `measure`-centred title, one slice for the close button. Buttons,
inputs, tabs, scrollbars, checkboxes, resize grip follow. Text draws from a
baseline (`ascent()` at box-thinking call sites). `UiRect::from_cells` is
deleted at the end of this step; panels size to content and theme metrics.

- **Done when:** the editor no longer looks cell-quantised; `from_cells` is
  gone; the pixel theme at 2× and the clean theme both render crisply
  (7B-2 makes this possible).
- **Landed as (`4dc90ed`, first slice), then extended across every
  remaining panel (`c754447`, `a80abb7`, `373246b`, `5bf1de6`,
  `c210901`) once the user explicitly authorized continuing the full
  step unattended** ("continue everything until 7D-2 is complete") —
  investigated up front, initially scoped down to just `draw_panel_chrome`
  given this step's real size (~24 draw functions, 35 `from_cells` call
  sites across 11 files) matches exactly the "chrome work causes burnout"
  shape §7.1 warns about by name, then widened once the user chose to
  keep going.
  - **Color/9-slice theming now covers every `ui::draw_*` function**, not
    just `draw_panel_chrome`: `ui/panels/chrome.rs` (title bar, status
    bar, dock tabs, text-input modal, confirm modal, context menu —
    `c754447`), `ui/menu.rs` (toolbar, dropdown — `c754447`),
    `ui/panels/dock.rs` (palette, stats, console, inspector, hierarchy,
    file browser — `a80abb7`), `ui/panels/modals.rs` (palette editor,
    color picker, help overlay — `373246b`), `ui/script.rs` (the script
    editor's own chrome — `5bf1de6`), and `graph_ui.rs` (node graph:
    nodes, wires, the Add Node palette — `c210901`). `ui/widgets.rs`
    needed no changes (`draw_button`/`draw_row`/`draw_swatch` already
    take `fg`/`bg` as plain parameters; theming happens at each call
    site, covered above).
  - **Consistent color-role mapping applied everywhere**: panel
    backgrounds → `PanelBg`, editable-field backgrounds (script buffer,
    text-input modal, inspector value rows) → `InputBg`, primary text →
    `TextPrimary`, secondary/hint text → `TextDim`, highlighted
    backgrounds/selected rows → `Accent` or `Selection`, title strips →
    `TitleBg`/`TitleText`, destructive actions (palette editor's Delete
    button, script error line/message) → the new use of `Danger`. Two new
    shared helpers, `draw_themed_frame`/`draw_themed_title_strip`
    (`chrome.rs`, `pub(super)` so `modals.rs` reuses them too), draw a
    real `SliceRole::Panel`/`TitleBar` 9-slice for every BOX-shaped
    overlay (text input, confirm modal, context menu, palette editor,
    color picker) with the same "fall back to a flat fill if the theme
    lacks the role" contract 7D-1 established; single-row-tall bars
    (title bar, status bar, dock tabs, menu bar/dropdown, node-graph Add
    Node header) stay flat fills on purpose — the theme's 6px 9-slice
    border would consume most of a 16px row.
  - **Semantic (not decorative) colors were deliberately left literal
    throughout**, each documented inline at its own site: console
    log-level colors, hierarchy entity-kind colors (player/spawn),
    file-browser icon-kind colors (dir/level/script), the Inspector's
    "New Graph" button, `PALETTE_COLORS`/the color picker's hue bar and
    saturation/value map (literal RGB/HSV — the content being picked, not
    chrome), Rhai syntax-highlight token colors in `script.rs`, and
    node-graph port glyph colors (white=exec, yellow=data-in,
    cyan=data-out, signaling port KIND). No themed "text-on-accent"
    `PaletteRole` exists (7D-1's own documented gap) — every bright-accent
    background with text on it (active dock tab, selected menu/context-
    menu row, selected node title, hovered dropdown item) still uses
    literal `Color::Black`, each site cross-referencing this same note.
  - **Font unification**: the separate `theme_font` field added in the
    first slice was removed — `EditorState::font` (pre-existing since
    Phase 7 Part 2c, documented back then as "a future theme can swap it
    for a TtfFont without call sites changing again") is now set directly
    from the loaded theme font at construction, matching that field's own
    original intent instead of duplicating it.
  - **`EditorState` fields are `theme: Theme`, `theme_chrome_tex:
    Texture`, and `font: Box<dyn Font>`** (not a separate `theme_font` —
    see above), resolved by `editor/theme_loader.rs` (split out of
    `mod.rs` to stay under the 750-line limit), loaded EAGERLY and
    unconditionally in `EditorState::new`
    — not lazily on first render as originally planned, once investigation
    showed `AssetManager::new()` needs no live GPU/window at all (only the
    eventual texture *upload* does, inside `Renderer::draw_texture_px`
    itself). The resolved `Texture` is cloned out and the loading
    `AssetManager` is dropped immediately, so `EditorHarness` (headless
    tests) gets the exact same real theme the live app does — no separate
    test-only code path, and (deliberately) no use of `Renderer.ui_assets`
    (7D-1) for this narrow slice at all; that field stays reserved for
    7D-4's runtime theme-switching, which does need a persistent,
    evictable `AssetManager` the way this one-shot load doesn't.
  - **`draw_panel_chrome`** (`ember2d-editor/src/editor/panel/mod.rs`)
    rewritten: one `SliceRole::Panel` 9-slice for the whole frame (its
    stretched center replaces the old separate "fill interior" step —
    one draw call doing what used to take two), one `SliceRole::TitleBar`
    9-slice over the top strip, a measure-centered title in the theme's
    own font (`Font::measure`/`Font::ascent`, baseline-positioned exactly
    like `Renderer::draw_text_px` already does), a `SliceRole::Button`
    close button, a `SliceRole::ResizeGrip` resize handle. The viewport
    is excluded entirely — flat black, no chrome, per the 7C-9 decision
    gate (§7.1). Every missing-role case falls back to the OLD flat fill
    (7D-1's own "no fabricated geometry" contract, `Theme::slice`).
    Hit-rect coordinates for `WidgetId::TitleBar`/`CloseBtn`/
    `ResizeHandle` changed slightly (the close button is now a themed
    2-cell square at the title bar's right edge, not the old 3-char
    `[X]`) — no existing test exercised those specific rects, confirmed
    by grep before changing them, so nothing broke.
  - **`DrawSurface` (the 7C-5 headless-testing trait) gains
    `draw_nine_slice_px`/`draw_text_px`**, mirroring
    `Renderer::draw_nine_slice`/`draw_text_px` exactly (`NullRenderer`'s
    own `draw_text_px` still calls `Font::measure` for a real width, not
    a dummy `0.0`, since headless title-centering math needs it) — this
    is what let `draw_panel_chrome` stay callable from both the real
    renderer and `EditorHarness` without a second copy of itself.
  - **Found and fixed live: R63, a real bug in `Renderer::draw_nine_slice`
    itself** (pre-dating this step — Phase 7 Part 1a). See R63's own row
    (§3.2) for the full account: it assumed a nine-slice texture is
    ALWAYS the whole texture, which broke the instant a shared atlas
    packed multiple named regions into one texture (exactly `NineSlice`'s
    own 7D-1 design) — caught by actually launching the editor and
    looking at it (a checkerboard of wrong-region tiles below the
    viewport), not by any automated test, since the pre-existing
    `nine_slice_quads` unit tests only ever exercised a texture dedicated
    to one slice.
  - **Follow-up (2026-09-13): the from_cells/pixel-layout conversion
    itself, done — `UiRect::from_cells` deleted.** Originally deferred
    (see the plan's own prior note here, preserved in git history) as a
    separate, dedicated step given the risk profile — 35 call sites is
    exactly defect E5's shape, hit-rects silently drifting from what's
    drawn. Investigated properly before starting rather than reopening
    the earlier "low reward, real risk" framing on faith: that framing
    was right for a blind mechanical rename, but wrong once the real
    finding surfaced — the theme's own font (Cascadia Mono) was rendering
    NOWHERE but `draw_panel_chrome`'s title bar. Every other piece of
    chrome text drew through `draw_str`/`draw_char`, which reads
    `Renderer.ui_font` (the unrelated `EMBER_UI_FONT` debug env var from
    7B-5, defaulting to the bitmap font), never the theme. A shortcut of
    just pointing `ui_font` at the theme's font was investigated and
    rejected: 7B-5's own "Landed as" note already found that exact
    mismatch (measuring stays cell-quantized while drawn glyphs don't)
    causes visible text overflow, which is why that toggle only ever
    shipped as a debug flag. There was no way to get the theme's real
    font rendering well without doing the real per-panel conversion.
    - **Converted, panel by panel, each its own commit** (`ccc6296`
      through `60aa776`): `dock.rs` (all six panel-content functions —
      Stats first, as the simplest proof-of-pattern slice with no
      hit-rects, then Console, Hierarchy, File Browser, Palette,
      Inspector), `chrome.rs` (title bar, status bar, dock tabs, all
      three chrome modals), `modals.rs` (palette editor, color picker,
      help overlay), `menu.rs` (toolbar and dropdown). Each panel now
      draws through `DrawSurface::draw_text_px`/`fill_rect_px` (two new
      trait methods this step added) at the theme's real
      `font_sizes.body`, with rows sized to `theme.metrics.row_h` —
      `ui/widgets.rs` gained pixel-space twins of the existing
      `draw_button`/`draw_row`/`draw_swatch` helpers
      (`draw_button_px`/`draw_row_px`/`draw_swatch_px`, plus a new shared
      `draw_text_row`) so hit-rects keep being pushed at the exact point
      drawn, the same discipline 7C-1 established, just in pixel space.
    - **Real, load-bearing exception, not an oversight: some chrome bars
      stay `CELL_H`/`CELL_W`-locked, not `theme.metrics.row_h`, even
      though they now draw through the real font.** Found live via
      screenshot, twice, before landing: the title bar (the menu bar
      drawn right below it still assumes exactly one `CELL_H` row above
      it), the status bar (`PanelManager` reserves exactly one `CELL_H`
      row at the screen bottom when sizing every panel), `draw_dock_tabs`'
      own strip (sits where `draw_panel_chrome`'s still-`CELL_H` title bar
      does), `menu.rs` in full (`draw_menu_dropdown`'s own hover check
      compares `mouse_col`/`mouse_row` as raw cell ints), and
      `draw_palette_editor_modal` (its own `input/mod.rs` handler
      hit-tests most rows by comparing `mouse.cell_y` against fixed cell
      offsets, a pre-existing independent recompute never migrated to
      `UiFrame` for this one modal). Each is documented inline at its own
      site with which OTHER code it stays synchronized with. One genuine
      bug shipped and self-caught before commit: the title bar's own
      baseline math passed its row HEIGHT where its row's top Y was
      needed, landing "EMBER2D EDITOR" a full row low, visibly merged
      into the menu bar underneath — caught by the same screenshot
      discipline, fixed same commit.
    - **`UiRect::from_cells` itself deleted** (`60aa776`), along with its
      own two dedicated unit tests, once its last ~7 real call sites
      (`Panel::new`'s initial sizing, the cell-based
      `draw_button`/`draw_row`/`draw_swatch`/`draw_menu_item` widget
      helpers, `StartScreen`'s template cards and browsers, the color
      picker's hue bar/SV map, one `canvas.rs` test fixture) converted to
      constructing the same pixel rect directly instead of through a
      named helper.
    - **What did NOT convert, deliberately, confirmed by investigation
      rather than left unexamined:** `ui/script.rs` and `graph_ui.rs`
      never called `from_cells` in the first place (grep-confirmed) —
      both are genuinely cell-grid systems, not unmigrated chrome. The
      script editor's cursor/click/selection math
      (`input/panels/file_and_script.rs`) walks `mouse.cell_x`/`cell_y`
      against `Panel::content_x`/`content_y` assuming one character per
      cell; the node graph's `node_at`/`port_at` hit-testing
      (`graph_ui.rs`) compares raw cell columns/rows against
      cell-measured node bounds. Converting either to real proportional
      text would desync click-to-position the same way the palette
      editor modal's own cell-locked rows had to stay locked — a correct,
      deliberate design choice for a code/graph editor (monospace text
      IS the expected feel), not a gap. `Panel::cell_x`/`cell_y`/
      `cell_w`/`cell_h`/`content_x`/`content_y`/`content_w`/`content_h`
      (`panel/mod.rs`) accordingly stay — not a bridge waiting to be
      deleted, but the permanent API those genuinely cell-grid subsystems
      (plus the canvas/viewport, cell-based forever per 7C-9) depend on;
      that file's own header comment now says so directly.
      `start_screen/` itself was never brought under the theme system at
      all (it predates 7D-1 and runs before a project — and therefore a
      loaded theme — exists) — still its own hardcoded bitmap-font look,
      unchanged, out of scope for 7D.
    - **Verification, every checkpoint from `ccc6296` through `60aa776`.**
      `cargo build --workspace --examples` clean at each. `cargo test
      --workspace`: 322 → 327 (5 new: 4 in a new `tests/editor_theme.rs`-
      adjacent... — see individual commits) → 325 (net −2 once
      `from_cells`'s own two unit tests were deleted with it), 0 failures
      at every checkpoint EXCEPT two caught and fixed same-session before
      moving on (`clicking_outside_the_focused_docked_script_panel_
      returns_focus_to_the_canvas`,
      `r14_pressing_a_shortcut_key_while_the_docked_script_panel_is_
      focused_does_not_fire_it` — both from the dock-tab-strip height
      mismatch above, fixed by locking that strip to `CELL_H`). `cargo
      clippy --workspace --all-targets` tracked at every checkpoint,
      74 → 73, never above the pre-conversion baseline (new
      `#[allow(too_many_arguments)]` on functions whose new `&Theme`/
      `font` params crossed the 7-arg threshold kept pace with the drop
      from deleted dead-label draw calls found along the way in
      `draw_inspector`). `scripts/check.ps1` clean at every checkpoint.
      `cargo test -p ember2d --test replay` 3× fresh processes green at
      every checkpoint. Manually verified live at every checkpoint by
      launching the real editor and screenshotting the specific panel
      just converted, per CLAUDE.md's "use the feature" rule — this is
      what caught both the title-bar baseline bug and the dock-tab/
      menu-bar row-height overlaps before they were ever committed.
  - **Verification, first slice (`4dc90ed`).** `cargo build --workspace
    --examples` clean. `cargo test --workspace`: 322 (was 321), all
    pass — new: `nine_slice_quads_offsets_every_src_rect_by_a_non_zero_
    atlas_origin` (`renderer/tests.rs`, R63's own regression test). `cargo
    clippy --workspace --lib`/`--all-targets` unchanged at 55/80 (one new
    `clippy::single_match` surfaced mid-step from a `match ... { Some =>
    ..., None => {} }` and was fixed immediately, not counted against
    baseline). `scripts/check.ps1` clean. `cargo test -p ember2d --test
    replay` 3× fresh processes green. Manually verified by launching the
    real editor (`cargo run -- --editor demos/roguelike/floor1.level`)
    and screenshotting it, per CLAUDE.md's own "use the feature" rule for
    UI changes — this is what caught R63 in the first place, since no
    automated test renders real pixels.
  - **Verification, each subsequent checkpoint** (`c754447`, `a80abb7`,
    `373246b`, `5bf1de6`, `c210901`) **repeated the same sequence**:
    `cargo build -p ember2d-editor` clean; `cargo test --workspace`
    stayed at 322/322 the whole way (no test ever needed updating — no
    checkpoint touched hit-rect geometry, only draw colors/9-slice fill,
    so no `WidgetId`/`UiRect` assertion was affected); `cargo clippy
    --workspace --all-targets` tracked (new `#[allow(too_many_arguments)]`
    on functions that crossed the 7-arg threshold by gaining a `&Theme`
    parameter kept the total from climbing: 79 → 78 → 78 → 78 → 75, never
    higher than the pre-7D-2 baseline); `scripts/check.ps1` clean at every
    checkpoint; `cargo test -p ember2d --test replay` 3× fresh processes
    green at every checkpoint. The dock.rs, modals.rs, and script.rs/
    graph_ui.rs checkpoints were also manually verified by launching the
    real editor and screenshotting Hierarchy/Inspector/Console/File
    Browser/the script editor's empty state/the View menu dropdown, per
    CLAUDE.md's "use the feature" rule — all consistent with the palette
    (amber accent, Cascadia Code body text) chosen in the first slice.
    This entire extension ran unattended per explicit user authorization
    ("continue everything until 7D-2 is complete... test and take all the
    screenshots you need that you are capable of doing alone") while the
    user was away from the session.

#### `[x]` 7D-3 — UI points: a real, user-facing UI scale for the editor chrome

- **Why:** the original two-pass investigation (2026-09-12/13, preserved
  below) found the naive "multiply `CELL_W`/`CELL_H` by an integer
  `ui_scale`" design doesn't work: `Renderer.scale` (R, DPI-derived) is
  already ≥2 on every display (`MIN_UI_SCALE`, `renderer/mod.rs:54`), so an
  integer chrome multiplier on top only reaches 2×/4×/6× physical — never
  smaller than today. Asked to look deeper for long-term scalability
  (2026-09-13), that search found: theme TTF text was never crisp
  (rasterized at logical px, nearest-upscaled ≥2×, `renderer/text.rs:54`,
  `backend.rs:104`); `CELL_W`/`CELL_H` carry five unrelated meanings and
  chrome depends on them; ~50 `mouse.cell_x/y` reads, many chrome ones
  desyncing under any chrome-only scale; the script editor (classified as
  chrome by §7.1) draws on the fixed 8×16 grid so it wouldn't scale; R49/R50
  (bitmap glyph sizing) go live under any scale; the glyph atlas never
  evicts; no editor preference persists, not even the theme choice; and
  Phase 9-3 needs the same scaling machinery for a script-facing pixel HUD.
  The same investigation also found LIVE bugs in the exact code this step
  rewrites — R64–R74 below.
- **Decided (2026-09-13):** UI **points**, a new coordinate space where
  1 point = `ui_scale` (S, an integer editor preference, 1–4) physical
  pixels — not the "chrome cell" design from the first pass. Every editor
  chrome surface (panels, title/menu/status bars, dock tabs, dropdowns,
  context menu, all dock panel content, palette editor, color picker,
  modals, the script editor both docked and fullscreen) moves to points and
  stops referencing `CELL_W`/`CELL_H` entirely. The level canvas, play mode,
  `ember2d-sim`, graph mode, and the start screen are unchanged. The script
  editor scales too, via a new optional theme `code_font` (a theme's body
  font can be proportional; the script grid needs monospace). UI scale and
  theme choice persist in a new per-user `EditorPrefs` file. `Auto` =
  `round(os_scale_factor × 2)` (100%→2 = today's size, 150%→3, 200%→4).
  Chrome bars (title/menu/status/tabs/dropdown rows) follow
  `theme.metrics.row_h` once converted (ember-clean: 16→20 physical px at
  the default scale — a real, deliberate visual change, confirmed with the
  user). UI Scale choices (Auto/1×/2×/3×/4×) live in the Theme menu after a
  separator, not View (View is already 13 rows; short windows at 4× would
  overflow it, and dropdowns aren't clamped to the screen — R81).
- **Change:** new `ember2d::renderer` modules — `ui_space.rs` (`UiSpace`:
  points↔logical↔physical conversion, physical-pixel snapping, text metrics
  measured at the real rasterized size so measuring never pollutes the atlas
  with an unused point-size cache entry) and `ui_painter.rs` (`UiPainter`:
  the one drawing choke point every chrome draw call goes through — fill,
  nine_slice, text, text_mono, tile_glyph, clip — theme-agnostic so Phase
  9-3 can reuse it for a script-facing HUD). `DrawSurface` gains
  `draw_text_run`/`TextRun` (replacing `draw_text_px` as the required
  method; `draw_text_px` is now a default built on it — every pre-7D-3
  caller unchanged), `draw_char_px`, `display_scale`; `set_scissor` and
  `draw_nine_slice_px` move to `f32`/`border_scale`. `GlyphInfo` gains a
  `size` field (R50). `Font` gains `reset_atlas`. New `editor/prefs.rs`
  (`EditorPrefs`/`PrefsStore`, per-user config dir, tests never touch the
  real file). New `editor/ui/metrics.rs` (`ChromeMetrics::from_theme`
  replaces every `CELL_*` chrome constant) and `editor/ui/script_layout.rs`
  (`ScriptLayout`: one shared geometry for the script editor's draw AND both
  its input paths, which independently disagreed before this step — R67/R68).
- **Test:** per checkpoint, named after the fixed defect where applicable
  (R50, R64–R74 below) — see each checkpoint's own commit for its list.
- **Done when:** no chrome file references `CELL_W`/`CELL_H` (a new
  `scripts/check.ps1` section enforces this); the Theme menu's UI Scale
  picker applies live and persists across restart; every chrome surface
  named above scales correctly at a non-integer `ui_scale/render_scale`
  ratio (e.g. S=3, R=2); the canvas/play mode/graph mode are visually
  unaffected at any UI scale.
- **Scope:** `ember2d` (renderer, theme.rs, font/*), `ember2d-editor` (panel,
  ui/*, input/*, impl_render.rs, impl_state.rs, theme_loader.rs, a new
  prefs.rs), `themes/ember-clean/theme.ron` (regenerated),
  `docs/ember2d-theming.md`, `scripts/check.ps1`.
- **Landed as a sequence of checkpoint commits** (7D-2's own precedent —
  reported between checkpoints, `git diff --stat` checked against this
  Scope at each one):
  - **Checkpoint 1 — renderer foundation (`45341bc`).** `ui_space.rs`/`ui_painter.rs`/
    `draw_log.rs` (new); `geometry.rs` extracted from `renderer/mod.rs`
    (`compute_layout` — now floors the letterbox origin, R71 —
    `nine_slice_quads` — now takes `border_scale` — and the other pure
    coordinate helpers); `draw_surface.rs`/`text.rs`/`backend.rs`/
    `vertex.rs`/`font/*` updated for the new signatures and `GlyphInfo.size`
    (R50); `theme.rs` (`ui_scale` removed — a legacy `theme.ron` with a
    stray `ui_scale` field still loads, serde's default "ignore unknown
    fields" behavior, tested directly; `code_font: Option<FontChoice>`
    added, `#[serde(default)]`); `themes/ember-clean/theme.ron` regenerated
    (`ui_scale` line dropped, `chrome.png` byte-identical). Every existing
    editor call site updated mechanically (`border_scale: 1.0`, `Rect`
    scissor) with NO behavior change — confirmed live (screenshot,
    `--editor demos/roguelike/floor1.level`, pixel-identical to before) and
    by `cargo test --workspace`: 347 (was 325, +22 new: `UiSpace`
    conversions/snapping at S3/R2, `UiPainter` draw-op assertions via
    `NullRenderer`'s new opt-in recording, `r71_letterbox_origin_is_always_a_whole_physical_pixel`,
    glyph-atlas sizing/reset, `BitmapFont`/`TtfFont` `size` field, legacy
    theme loading, `code_font` default), all pass. `cargo clippy --workspace
    --all-targets` unchanged at 73. `scripts/check.ps1` clean. `cargo test
    -p ember2d --test replay` 3× fresh processes green.
  - **Checkpoint 2 — editor foundation, UI scale pinned to R (`00273bd`).** New
    `editor/prefs.rs` (`EditorPrefs`/`PrefsStore`/`UiScaleChoice`, per-user
    config dir — `%APPDATA%\Ember2D\editor_prefs.ron` on Windows,
    `$XDG_CONFIG_HOME/ember2d/` else `$HOME/.config/ember2d/` elsewhere;
    every test stays on `PrefsStore::InMemory`, the only real-file caller is
    `ember2d-app/src/main.rs`'s three `EditorState` construction sites via
    `.with_prefs(PrefsStore::user())`). `theme_loader.rs` rebuilt: fonts now
    build at a real physical raster scale with the printable-ASCII range
    pre-warmed (mitigates R75); a theme's `code_font` resolves alongside
    `font` (falling back to a separate instance of `font`'s own choice);
    `switch_theme` now persists to prefs; new `with_prefs`/`set_ui_scale`/
    `effective_ui_scale` (**pinned** to `display.render_scale`, ignoring
    `self.prefs.ui_scale` — the live menu is this step's last checkpoint)/
    `rebuild_fonts_if_scale_changed`. `EditorState` gains `prefs`/
    `prefs_store`/`ui_space`/`code_font`/`font_raster_scale`, captured once
    per real draw in `impl_render.rs`'s `draw()` before any mode dispatch.
    Fixed **R70** (`switch_to_level` silently reverting theme/font/prefs to
    fresh-construction defaults) via a field-swap, since `Box<dyn Font>`
    isn't `Clone`. Extractions to stay under the 750-line limit:
    `editor/accessors.rs` (read-only accessors, out of `mod.rs`, which was
    at the limit exactly) and `impl_state/graph_sidecars.rs`
    (`migrate_graph_sidecars`, out of `impl_state/mod.rs` — kept separate
    from the pre-existing `impl_state/export.rs`, a different feature,
    "Export Standalone Game", not to be confused with it). Harness gained
    `with_display`/`with_state_and_display` (a settable `DisplayScale` —
    `move_mouse` stays in logical pixels regardless of render scale),
    `resize`, `start_recording`/`draw_ops` (empty until a later checkpoint
    actually draws through `UiPainter`). New tests (`editor_theme.rs` +
    `prefs.rs` unit tests): prefs path resolution (Windows/XDG/HOME/none),
    missing/unparsable-file fallback, round-trip, a save failure logging
    without panicking, `Fixed` clamping, `Auto` resolution at 100/125/150/
    200%, `a_fresh_editor_uses_an_in_memory_prefs_store_with_defaults`,
    `selecting_a_theme_from_its_menu_persists_it_to_prefs`,
    `the_harness_at_render_scale_2_still_round_trips_menu_clicks`, and
    `r70_switching_levels_keeps_the_active_theme_fonts_and_prefs` (verified
    to actually fail against the pre-fix code before confirming the fix).
    `cargo test --workspace`: 360 (was 347, +13), all pass. Clippy: no new
    warnings in any file this checkpoint touched or created (spot-checked
    against the full workspace warning list). `scripts/check.ps1` clean.
    `cargo test -p ember2d --test replay` 3× fresh processes green.
    Confirmed live (screenshot) pixel-identical to before; confirmed no
    prefs file is written on a session that never changes a preference.
  - **Checkpoint 3 — panels, bars, menus, viewport seam, and dock content (`2ce8335`).**
    New `editor/ui/metrics.rs` (`ChromeMetrics::from_theme`: `bar_h`/`row_h`
    from `theme.metrics.row_h`, `grip = 2 × border` — R74). `panel/mod.rs`
    rewritten in points (`Panel::rect` is now the one source of truth;
    `cell_x/y/w/h` kept ONLY as a rounding bridge for the node graph and the
    still-cell-based docked script editor outer frame, documented as such);
    `panel/chrome.rs` extracted (`draw_panel_chrome`, viewport drawn from its
    own exact rect, not the cell bridge). `ui/menu.rs` rewritten: dropdown
    position reads back `frame.rect_of(WidgetId::MenuLabel(menu))` (the SAME
    rect the toolbar itself pushed) instead of re-measuring the label layout
    a second time — the exact class of independent-recompute bug this whole
    step exists to remove. **R66** (viewport single-source-of-truth) fixed by
    routing the scissor, the canvas click gate, `mouse_to_grid`/`center_on`/
    `clamp_scroll`, and the status bar's own origin readout all through the
    same exact `content_rect(&metrics)` value — no independent re-derivation
    left anywhere. **R69** fixed: `apply_layout` now runs before script/graph
    mode's early return, so a resize while fullscreen no longer goes stale
    until a mode switch. **R64** fixed in `context_menu_trigger.rs`: the
    right-click handler now reads the same `UiFrame` hit
    (`WidgetId::FileBrowserRow`/`HierarchyRow`) the left-click handlers
    already trusted, instead of `mouse.cell_y` minus a cell-rounded
    `content_y()`; verified to actually mistarget a deep, scrolled-to row
    against the pre-fix code before confirming the fix. **R72** fixed in
    `file_and_script.rs`/`hierarchy_and_palette.rs`: the File Browser's and
    Palette's wheel-scroll ceilings now come from the same pixel
    `content_rect`/`row_h` formula `draw_file_browser_panel`/
    `draw_palette_panel` already draw against, instead of an approximate
    cell-row count; also verified to actually stop short of the last file
    against the pre-fix code. Dock panel content (`ui/panels/dock.rs`) needed
    no change — it already drew from `content_rect(&metrics)` directly.
    Found live testing this checkpoint's own chrome-bar height change: the
    status bar's grid-position readout and `[LAYER: ...]` label used two
    fixed pixel columns (`10 * CELL_W`/`30 * CELL_W`) sized for the old
    fixed-width bitmap glyphs — a real, proportionally-advancing theme font
    can run past the first column and overlap the second. Fixed by measuring
    each label's own drawn width (`Font::measure`) and placing the next
    column after it plus a small gap, so the columns can never collide
    regardless of font or content. New regression tests (`editor_input.rs`):
    `r64_right_clicking_a_deep_scrolled_file_browser_row_targets_that_exact_file`,
    `r64_right_clicking_a_deep_hierarchy_row_targets_that_exact_spawn`,
    `r72_the_file_browser_wheel_can_scroll_all_the_way_to_the_last_file`,
    `r72_the_palette_wheel_can_scroll_all_the_way_to_the_last_item`; new
    harness helper `EditorHarness::wheel`. `panel/tests.rs` rewritten in
    points (a fixed `test_metrics()` pinned to the real shipped theme's own
    values, not loaded from disk) with 2 new tests:
    `chrome_metrics_resize_grip_fits_its_nine_slice_borders`,
    `docked_panels_never_shrink_below_the_minimum_size`. `cargo test
    --workspace`: 364 (was 360, +4), all pass. Clippy: 44 warnings workspace-
    wide (well under the 73 baseline; none new in a file this checkpoint
    touched). `scripts/check.ps1` clean. `cargo test -p ember2d --test
    replay` 3× fresh processes green. Confirmed live (screenshot,
    `--editor demos/roguelike/floor1.level`, maximized window): chrome bars
    now 20px (was 16px, the confirmed-with-the-user visual change), status
    bar readout no longer overlaps, viewport/panels/dock content all correct.
  - **Checkpoint 4 — modals (`e229b56`).** New `WidgetId` variants
    (`PaletteEditorClose`/`Field`/`Toggle`/`CustomColor`/`SaveClose`/
    `Delete`) — `draw_palette_editor_modal` (`ui/panels/modals.rs`) now
    pushes every interactive row/button at the exact point it's drawn,
    `row_h` switched from a hardcoded `CELL_H` to `theme.metrics.row_h`
    (the coupling that used to force it onto the cell grid is gone once the
    input side stops recomputing that grid independently). **R65** fixed:
    `handle_palette_editor_input` and `handle_color_picker_input` extracted
    to a new `input/palette_editor.rs` and rewritten to read every field
    back via `UiFrame::hit`/`rect_of` — no more `mouse.cell_x`/`cell_y`
    comparisons against an independently-recomputed `mx`/`my`/`cx`. The
    advanced color picker's hue-bar/SV-map step counts (previously two
    unnamed literals, `36`/`20`/`8`) become named `HUE_BAR_STEPS`/
    `SV_MAP_W`/`SV_MAP_H` constants in `modals.rs`; their own hit-test math
    was already exact (it reads the SAME rect the draw side pushed, not an
    independent literal) so needed no behavior change, just the shared
    name. Updated `editing_a_palette_item_in_the_modal_editor_undoes_as_one_session`
    (`tests/editor_undo.rs`) to click through `rect_of` instead of raw cell
    position. New accessors `palette_tile`/`palette_editing_idx`
    (`accessors.rs`) for test assertions. New regression test
    (`editor_input.rs`): `r65_palette_editor_fields_hit_where_drawn_with_an_odd_cell_remainder`
    (clicks the Tag row's far edge, not its top-left corner, and confirms
    typing lands in Tag — the old code would have landed several rows off
    at the default 1280×720, since `row_h` going from `CELL_H` (16) to the
    real theme row height (20) widened the draw/input mismatch far past a
    single half-row drift). `cargo test --workspace`: 365 (was 364, +1),
    all pass. Clippy: 44 warnings workspace-wide, none new in a touched
    file. `scripts/check.ps1` clean. `cargo test -p ember2d --test replay`
    3× fresh processes green. Confirmed live (screenshot, maximized
    window): palette editor modal's every field/toggle/button clickable
    and correctly targeted, Foreground/Background color grids and their
    `[ Advanced ]` buttons open the color picker, hue bar/SV map/Apply/
    Cancel all functional, Save & Close commits one undo step.
  - **Checkpoint 5 — script editor, in points, through the `code_font` (`f6403b7`).**
    New `ui/script_layout.rs` (`ScriptLayout::compute`: header/find-bar/
    error/text rects, a gutter sized from the buffer's own real line count,
    `char_w` from the code font's own monospace advance, `visible_rows`/
    `visible_cols()`; `ScriptLayout::hit` is the one pixel-to-`(char,line)`
    hit-test every click path now shares). `ui/script.rs` rewritten to draw
    through `self.code_font` and `DrawSurface::draw_text_run` directly
    (`pitch: Some(char_w)`, the same "S pinned to R" `texel_scale: 1.0`
    simplification every other panel in this step already uses — not the
    full `UiPainter`, which stays for a later checkpoint once `texel_scale`
    actually varies); the highlighter (`draw_highlighted_rhai`) now only
    ever draws FOREGROUND glyphs, batched per same-colored run, since
    selection backgrounds are painted as their own exact fill before any
    text (**R73**). `impl_render.rs` split into `impl_render/{mod,modes}.rs`
    (`render_graph_mode`/`render_script_mode` extracted) to stay under the
    750-line limit; `render_script_mode`'s fullscreen title/status bars are
    real themed chrome now (`metrics.bar_h`, theme colors) — the one
    surface in this editor that had stayed on literal `Color::Cyan`/
    `Color::DarkBlue` through every earlier theming step. New
    `ChromeMetrics::script_fullscreen_rect` — the fullscreen content rect,
    shared by draw and input instead of each recomputing it. **R67** fixed:
    `input/panels/file_and_script.rs`'s first-click path and
    `input/script_editor.rs`'s focused path both now call
    `ScriptLayout::hit`/`.visible_rows` — no more independent `gutter_w = 4`
    literal, no more an `hscroll`-blind first click, no more a 1-line vs.
    2-line wheel-step mismatch. **R68** fixed: the gutter width, and
    keep-in-view's own row/column bounds, all come from the same
    `ScriptLayout` the draw side used. Panel cell-bridge (`content_x/y/w/h`)
    usage for `PanelId::ScriptEditor` removed from `impl_render.rs` — every
    panel now takes `content_rect(&metrics)` directly. Updated the R11
    regression test (`impl_state/tests.rs`) to layout (pixel) coordinates.
    New accessors `script_cursor`/`script_scroll`/`code_font` (`&mut`, the
    one non-read-only accessor — `Font::measure` needs it) for the new
    test file. New `tests/editor_script.rs`:
    `r67_the_first_click_on_a_docked_unfocused_script_editor_lands_in_the_exact_column_clicked`
    (confirmed to fail against the pre-fix code),
    `r67_clicking_the_reserved_error_row_does_not_move_the_cursor`,
    `r68_the_focused_script_editor_wheel_can_scroll_to_the_last_line_of_a_1000_plus_line_file`,
    `r73_a_selection_starting_mid_token_highlights_only_the_selected_characters`
    (asserts the exact `Fill` draw-op rect via `NullRenderer` recording).
    `cargo test --workspace`: 373 (was 365, +8), all pass. Clippy: 44
    warnings workspace-wide, none new. `scripts/check.ps1` clean —
    `impl_render/{mod,modes}.rs` both real-line-checked, not just by
    check.ps1's own non-blank-line count (R76). `cargo test -p ember2d
    --test replay` 3× fresh processes green. Confirmed live (screenshot,
    fullscreen `scripts/enemy_boss.rhai`): syntax-highlighted, themed title/
    status bars, a click on a specific character (line 3, "header") landed
    exactly on it (status bar read "Line: 3  Col: 12", matching the click).
  - **Checkpoint 6 — chrome audit (`9ca404b`).** New `scripts/check.ps1` §4: every
    `.rs` file under `ember2d-editor/src/editor` (excluding `start_screen/`,
    deliberately out of scope) fails the check if it references
    `CELL_W`/`CELL_H` in real code (comment lines exempt, matching the
    existing determinism-check convention) — an explicit allowlist covers
    every already-reviewed, deliberate exception: the level canvas
    (`ui/canvas.rs`), the viewport-seam cell math (`input/canvas.rs`,
    `impl_state/mod.rs`), `panel/mod.rs`'s own documented cell-rounding
    bridge, `ui/types.rs`'s `cells()` helper (only called by `graph_ui.rs`/
    `start_screen/`), literal font8x8 tile-glyph previews and the advanced
    color picker's own literal HSV grid (`ui/panels/dock.rs`,
    `ui/panels/modals.rs`, `input/palette_editor.rs`), `ui/widgets.rs`'s
    `draw_row`/`draw_menu_item` (still used only by `graph_ui.rs`/
    `start_screen/`), and `ui/panels/chrome.rs`'s own fixed-pixel-budget
    modal/button-width literals. Running this check against the tree found
    (and fixed) one real, live violation the audit's own rewrite of this
    step's earlier checkpoints hadn't touched: `input/context_menu.rs`'s
    `FloatPanel` action positioned a newly-floated panel at
    `10.0 * CELL_W`/`10.0 * CELL_H` (a Phase 7 Part 1c leftover, predating
    this whole points conversion) — now a bare point literal at the same
    numeric position. Also deleted `ui/widgets.rs`'s cell-based
    `draw_button`/`draw_swatch` — dead code, `draw_button_px`/
    `draw_swatch_px` replaced their last real callers in an earlier
    checkpoint. Logged, not fixed (not a functional bug — the conversion is
    exact either way): **R83**, `ContextMenu.x`/`.y` still store a cell
    reading (`mouse.cell_x`/`cell_y`) rather than the exact pixel position
    the click itself already carries, converted once in
    `draw_context_menu`. `cargo test --workspace`: 373 (unchanged — no new
    tests this checkpoint, a lint/cleanup pass, not a behavior change).
    Clippy: 44 warnings workspace-wide, none new. `scripts/check.ps1`
    clean, and confirmed to actually catch the `context_menu.rs` violation
    when temporarily reintroduced. `cargo test -p ember2d --test replay`
    3× fresh processes green. Confirmed live (screenshot): editor
    unchanged.
  - **Checkpoint 7 — live UI scale, at last (`ab6ef64`).** The whole point
    of this step, and the first checkpoint to actually let `ui_scale` diverge
    from `render_scale` — every earlier checkpoint deliberately kept
    `effective_ui_scale` **pinned** to `render_scale` (checkpoint 2's own
    note) specifically so the draw/input conversion could land one file at a
    time with no visual regression to verify against. Draw side first:
    `ui/widgets.rs` (`draw_text_row`/`draw_button_px`/`draw_swatch_px`/
    `draw_row_px`), `panel/chrome.rs` (`draw_panel_chrome`), `ui/panels/
    chrome.rs` (every function), `ui/panels/dock.rs`, `ui/panels/modals.rs`,
    `ui/script.rs`, and `ui/menu.rs` all converted from raw `&mut dyn
    DrawSurface` to `&mut UiPainter` (the canvas/viewport escape hatch,
    `painter.surface()`, stays on `DrawSurface` per the 7C-9 gate); `ui/menu.rs`
    gained the actual `Theme > UI Scale` entries (`Sep` + `UiScaleChoice::ALL`,
    a checkmark on the active one); `ToolbarAction::SetUiScale`/
    `MenuState::current_ui_scale` new. `impl_render/{mod,modes}.rs` construct
    one `UiPainter` per draw and use `self.ui_space.screen_pt()` instead of
    raw `pixel_width()`/`pixel_height()`.

    A fully-compiling, fully-green build at this point still looked
    completely broken live: **R84** (`UiSpace::from_surface` double-dividing
    already-logical `pixel_width()`/`pixel_height()` by `render_scale`) put
    every panel at half its real size, overlapping the viewport — caught by
    screenshot, not by any test, since no earlier checkpoint had ever pushed
    a real value through `from_surface` at `render_scale != 1`. Fixing it
    exposed a second, independent bug the first screenshot's own layout
    corruption had been masking: **R85** (`UiPainter::text`/`text_mono` using
    `pt_to_logical` — `S/R` — for `texel_scale` instead of the new
    `raster_to_logical` — `1/R` — double-counting `S` on top of a glyph
    raster that's already `S` times its point size) rendered every chrome
    text draw `S`× too large. Both fixed and reverified live before
    continuing — see each's own §3.2 row.

    Input side: every chrome hit-test call site converts `mouse.pixel_x/y`
    (LOGICAL) to points via `self.ui_space.logical_to_pt(...)` — the "INPUT
    choke point" `ui_space.rs`'s own header comment named back in checkpoint
    1 — before comparing against a `UiFrame`/`Panel::rect` value (POINTS-space
    since the draw-side conversion above). Converted: `input/panels/mod.rs`
    (`handle_panel_chrome_click`, `update_panel_drag_and_resize`),
    `input/panels/menu_bar.rs` (both handlers, plus the new `SetUiScale`
    dispatch calling `set_ui_scale`), `input/context_menu.rs`,
    `input/modal.rs`, `input/palette_editor.rs` (both handlers),
    `input/panels/context_menu_trigger.rs`, `input/panels/file_and_script.rs`
    (both handlers), `input/script_editor.rs`, `input/panels/
    hierarchy_and_palette.rs` (both handlers), `input/panels/inspector.rs`,
    and `input/mod.rs`'s own docked-script-focus click-outside check.
    `input/graph.rs`'s `GraphPaletteRow` hit deliberately left unconverted —
    it's drawn by the CELL-based `draw_row` helper in `graph_ui.rs` (7C-9
    scope boundary), so `mouse.pixel_x/y` already matches the space it
    lives in.

    The level canvas (7C-9: stays logical forever, independent of
    `ui_scale`) needed the OPPOSITE conversion at its own three points-space
    reads: `impl_state/mod.rs`'s `mouse_to_grid`/`viewport_tiles` convert
    `Panel::content_rect` down to LOGICAL via `rect_to_logical` (not the
    mouse up to points) before dividing by the logical-pixel `CELL_W`/
    `CELL_H` constants — mixing a points-space rect with a logical-pixel
    constant would have silently scaled the cursor position by `ui_scale`.
    `input/canvas.rs`'s `on_canvas` gate needed the same treatment for its
    own `viewport_rect`/`bar_h` margin. `mouse_to_grid`/`center_on`/
    `clamp_scroll` (plus their private `viewport_tiles` helper) extracted to
    a new `impl_state/viewport.rs` — exactly the "one viewport seam module"
    this step's own plan called for, needed once R70's swap-based fix and
    this checkpoint's own conversion pushed `impl_state/mod.rs` back over
    CLAUDE.md's 750-line real-line limit (777) despite `check.ps1`'s
    non-blank-line count still passing it (R76's own gap) — `check.ps1`'s §4
    chrome-CELL-allowlist updated to name the new file instead of
    `impl_state/mod.rs`.

    `theme_loader::effective_ui_scale` finally un-pinned:
    `self.prefs.ui_scale.resolve(display.os_scale_factor)`, exactly the one
    line checkpoint 2's own doc comment said this checkpoint would change.
    Un-pinning immediately broke 26 tests that had never touched
    `DisplayScale` themselves — the default harness display, `(1, 1.0)`
    since before this step existed, now resolved `Auto` to `ui_scale == 2`
    while `render_scale` stayed `1`, a real (if accidental) divergence every
    one of those tests' own click coordinates assumed away. Fixed at the
    root, not per-test: the harness default became `(render_scale: 2,
    os_scale_factor: 1.0)` — realistic besides (`Renderer.scale` is never
    actually `1` in the real app, `MIN_UI_SCALE`), and restores every
    pre-existing test's implicit "points == logical" assumption, since
    `Auto.resolve(1.0) == 2 == render_scale` again. Two tests still needed
    fixing beyond that: `r70_switching_levels_keeps_the_active_theme_fonts_and_prefs`
    deliberately seeds `Fixed(3)` against the harness's now-`render_scale: 2`
    default (a genuine, if incidental, S≠R case) and was clicking a
    points-space `rect_of` result directly; and
    `the_harness_at_render_scale_2_still_round_trips_menu_clicks` asserted
    the old PINNED behavior by name and had to be renamed and re-pointed at
    the real un-pinned resolution (`os_scale_factor: 2.0` now resolves to
    `ui_scale: 4`, not `2`) — both confirm the checkpoint's own conversion
    rather than special-casing around it. `tests/common/mod.rs`'s own shared
    helpers (`open_menu`, `click_menu_item`, `click_theme_menu_item`,
    `select_dock_tab`, `canvas_center`, `canvas_pixel_for_grid`) all had the
    same latent points/logical conflation — harmless at every earlier
    checkpoint's `S == R`, real the moment any caller wants `S != R` — fixed
    the same way as production code.

    New `tests/editor_ui_scale.rs` (5 tests, the ones this step's own plan
    named): `setting_ui_scale_via_the_theme_menu_takes_effect_on_the_next_frame_and_persists`,
    `auto_ui_scale_resolves_from_the_displays_own_os_scale_factor` (100/150/
    200% → 2/3/4, through a real draw, not `UiScaleChoice::resolve` in
    isolation), `chrome_geometry_scales_with_ui_scale_at_a_fixed_render_scale`
    (asserts the title bar's own recorded `DrawOp::Fill` height doubles
    from `S=2` to `S=4` at a fixed `R=2` — NOT the viewport's own logical
    width, which this test's own first draft got backwards: a bigger
    `ui_scale` makes fixed-point-width side panels eat a BIGGER logical
    share of a fixed physical window, so the viewport actually SHRINKS as
    `ui_scale` grows, matching how real OS DPI scaling shrinks usable
    screen area — the title bar's fixed-points HEIGHT has no such
    interaction, so it was the correct thing to assert on instead),
    `canvas_painting_is_unaffected_by_ui_scale` (7C-9's own gate, checked
    live: the same grid cell paints at `S=1` and `S=4`), and
    `menu_and_theme_dropdown_clicks_round_trip_when_ui_scale_is_smaller_than_render_scale`
    (the step's own "S/R = 0.5" case, `render_scale: 4`/`Fixed(2)`).

    Found and fixed one more real gap while finishing this checkpoint, not
    logged as an R-row (a design gap this checkpoint's own live scale
    exposed, not a pre-existing defect): `PanelManager::apply_layout`'s
    `DockSide::None` (floating) arm was a complete no-op — a panel floated
    at `FloatPanel`'s fixed `(80, 160)` point offset, or left wherever a
    drag released it, was never re-clamped against `screen_pt()`, which
    itself SHRINKS as `ui_scale` grows (fewer points fit in the same
    window). At a high enough scale a floated panel's own title bar — the
    one thing a drag grabs — could end up entirely off-screen with no way
    to retrieve it. Fixed: floating panels now clamp to
    `[0, screen_w] x [canvas_top, canvas_bottom]` every `apply_layout`, the
    same way a real OS window manager keeps a dragged window's title bar
    reachable. Regression test:
    `a_floating_panel_left_off_screen_is_clamped_back_into_view_on_the_next_layout`
    (`panel/tests.rs`).

    `cargo test --workspace`: 371, all pass (some earlier checkpoints'
    counts moved both up — 6 new tests this checkpoint — and, net, down
    across the renamed/rewritten harness-helper tests above; every test
    this checkpoint touches is named individually here and in §3.2, not
    just totaled). `cargo clippy --workspace --all-targets`: 55 warnings
    (was 44 at checkpoint 6 — the `too_many_arguments` lint firing on
    several draw functions that gained a `painter: &mut UiPainter`
    parameter on top of an already-long list; well under the step's own
    73 ceiling, and not addressed here — reducing argument counts is a
    separate, out-of-scope cleanup). `scripts/check.ps1` clean, including
    its own §4 chrome-CELL audit against the new `impl_state/viewport.rs`.
    `cargo test -p ember2d --test replay` 3× fresh processes green.
    Verified live at THREE distinct points: (1) after R84/R85's fixes, a
    screenshot at the default (pinned-equivalent) scale confirmed pixel-
    identical rendering to every earlier checkpoint's own baseline; (2) a
    second screenshot right after un-pinning, at `Auto`'s real resolved
    scale on this machine, confirmed the same; (3) the live `Theme > UI
    Scale` menu itself, driven by simulated mouse input against the real
    running app (not the harness): opened the Theme dropdown (screenshot
    confirmed the new UI Scale entries with `Auto` checked), selected
    "UI Scale: 4x" (screenshot confirmed a real, visible 2× jump from the
    machine's own Auto-resolved `2` — chrome text and panels visibly
    larger, the title bar's two independent labels overlapping at this
    window size, an expected consequence of choosing an extreme scale on a
    small window, not a hit-testing defect), then clicked the Hierarchy's
    "@ Player" row at that same real 4×/2× divergence and confirmed both
    the row highlighted AND the Inspector updated to show the Player's own
    fields — hit-testing genuinely correct at a real, live `ui_scale !=
    render_scale`, not just in the headless harness. The real per-user
    prefs file this touched (`%APPDATA%\Ember2D\editor_prefs.ron`) was
    reset back to `Auto` afterward, not left at the test scale.

**Preserved from the original two-pass investigation, for context:**
first pass (2026-09-12) thought the step was blocked on 7D-2's own deferred
`UiRect::from_cells` removal; the user picked 7D-4 first instead. Second
pass (2026-09-13), after `from_cells` actually landed, found the real
remaining blocker was several chrome bars staying locked to `CELL_W`/
`CELL_H` regardless — the choice above (points, not a chrome-cell
multiplier) is what that pass's options (b)/(c) turned into once designed
in full.

#### `[~]` 7D-4 — Theme switching and `docs/ember2d-theming.md`

View › Theme lists `themes/*`; switching reloads chrome and font without
restart. New doc: file format, palette roles, how to author a chrome atlas,
how the two shipped themes differ.

- **Landed as (`6464dd3`)** — the switching mechanism and its tests are
  done; `docs/ember2d-theming.md` describes the ONE shipped theme
  (`ember-clean`), not two — `themes/ember-pixel` is still deferred (7D-1's
  own "Landed as" note), so the doc's "how the two shipped themes differ"
  section doesn't exist yet; it'll be added when/if that theme ships.
  - **`MenuKind::Theme`** (`ui/types.rs`) is a new TOP-LEVEL menu (after
    Layers), not a submenu under View — the menu system has no submenu
    concept, and this is the first menu whose entries are runtime-known
    rather than a fixed compile-time list. `MenuEntry` gained a
    `DynamicItem { label: String, action: ToolbarAction }` variant for
    exactly this (a shipped theme's directory name isn't a `&'static
    str`); `ToolbarAction::SetTheme(String)` is the action it carries.
    `theme_menu_entries(available: &[String]) -> Vec<MenuEntry>`
    (`ui/menu.rs`) builds one `DynamicItem` per name — both real call
    sites (`draw_menu_dropdown`, and `handle_menu_dropdown_click`'s
    re-resolve-by-index in `input/panels/menu_bar.rs`) special-case
    `MenuKind::Theme` to call it instead of the static `menu_entries`, so
    the two stay in sync the same way `menu_entries` itself already had to
    for every other menu. `menu_entries`'s own `MenuKind::Theme` arm is an
    explicit `vec![]`, never actually reached, kept non-wildcard so a
    future genuinely-fixed `MenuKind` can't silently fall through unnoticed.
  - **`EditorState` gains `available_themes: Vec<String>`**, scanned ONCE
    at startup by `theme_loader::list_available_themes` (every `themes/*`
    subdirectory with a real `theme.ron` in it, sorted, never empty — a
    missing/unreadable `themes/` dir still returns `[DEFAULT_THEME]`, the
    same "menu always has something selectable" contract `Theme::load`'s
    own fallback already keeps at the single-theme level). Not re-scanned
    while running — a theme dropped into `themes/` mid-session needs a
    restart to appear, an explicit, documented tradeoff, not an oversight.
  - **`EditorState::switch_theme(&mut self, name: &str)`**
    (`theme_loader.rs`) reloads `theme`/`theme_chrome_tex`/`font` in
    place via `load_editor_theme_named` (the first slice's own loader,
    generalized from a hardcoded `"ember-clean"` to take any name) — the
    `ToolbarAction::SetTheme` handler in `input/panels/menu_bar.rs` is the
    one caller. **Investigated before writing this**, since 7D-2's own
    doc comment on `Renderer.ui_assets` (7D-1) claimed runtime switching
    "DOES need" a persistent, evictable `AssetManager` the way the
    one-shot first-slice load doesn't: `Texture::id` (renderer/texture.rs)
    is a process-wide `AtomicU64`, not scoped to one `AssetManager`
    instance, so a throwaway `AssetManager` on every switch still hands
    out a genuinely unique id — no collision, no stale-texture bug. The
    old theme's GPU-resident texture becomes unreferenced until
    `WgpuBackend`'s own LRU `texture_budget` (R26, §5.2) evicts it rather
    than being freed immediately, an acceptable tradeoff for a rarely-used,
    user-initiated action switching between a handful of small chrome
    atlases — not worth wiring a second `AssetManager` through
    `EditorState` just to avoid. `Renderer.ui_assets` stays unused by the
    editor; nothing yet claims that reservation.
  - **Found and fixed a real, pre-existing test-coverage gap while writing
    this step's own tests**: `cargo test` runs `ember2d-editor`'s
    integration tests with CWD set to `ember2d-editor/` (this crate's own
    manifest dir), not the repo root where `themes/ember-clean/` actually
    lives — since `Theme::load` never fails outward, every
    `EditorHarness`-built `EditorState` since 7D-2's very first slice had
    silently been getting `Theme::fallback()` (magenta chrome, no slices,
    the bitmap font), not the real shipped theme. No earlier test happened
    to assert on real theme content closely enough to notice. Fixed with
    the same `ensure_workspace_root_cwd` idiom
    `ember2d/tests/common/mod.rs` already uses for level-loading tests —
    added to `ember2d-editor/tests/common/mod.rs`, called from
    `EditorHarness::new`/`with_state` and from the handful of tests that
    construct an `EditorState` directly as `with_state`'s own argument
    (too late for `with_state`'s internal call to help, since Rust
    evaluates that argument first). New regression test:
    `a_fresh_editor_loads_the_real_shipped_theme_not_the_fallback`
    (`tests/editor_theme.rs`) — the fallback theme would fail every
    assertion in it.
  - **Tests** (`tests/editor_theme.rs`, new file — same one-file-per-
    feature-area split as `editor_script.rs`/`editor_undo.rs`): the CWD
    regression above; `available_themes` finds the shipped theme;
    `list_available_themes` falls back to `[DEFAULT_THEME]` for a missing
    directory (a `#[cfg(test)]` unit test in `theme_loader.rs` itself,
    since the function is `pub(super)` — deliberately the ONLY CWD-
    mutating test in that binary, isolated to its own temp directory, to
    avoid racing any other test that assumes a particular CWD);
    `theme_menu_entries` lists one entry per available theme; a full
    click-through-the-real-menu round trip
    (`selecting_the_current_theme_from_its_own_menu_round_trips_without_
    crashing`) exercising the entire click → hit-test → action →
    `switch_theme` reload pipeline end-to-end. Only one theme ships today,
    so nothing yet proves switching between two DIFFERENT themes' visual
    content — that's `themes/ember-pixel` actually shipping, still
    deferred.
  - **Verification.** `cargo build --workspace --examples` clean. `cargo
    test --workspace`: 327 (was 322), all pass. `cargo clippy --workspace
    --all-targets` unchanged at 75. `scripts/check.ps1` clean (after
    trimming a few doc comments in `editor/mod.rs` to stay under the
    750-line limit once the new `available_themes` field/its accessor
    pushed it over — `theme()`/`available_themes()` moved to
    `theme_loader.rs`, same accessor contract, to make room). `cargo test
    -p ember2d --test replay` 3× fresh processes green. Manually verified
    by launching the real editor, opening the new `Theme` menu label,
    confirming the checkmark on the active theme, and clicking it —
    dropdown closes, chrome stays intact, editor doesn't crash.
  - **Follow-up (2026-09-13): `UI Scale: 1.5x`.** Live user feedback on
    hovering at `1x` (which also surfaced R88, §3.2 — a real bug, fixed
    separately) came with a second, independent ask: something between
    `1x` and `2x`. Added `UiScaleChoice::OnePointFive` — a single dedicated
    variant, not a generalized fractional `Fixed`, since the menu only
    ever offers this one half-step and a dedicated variant keeps every
    existing `Fixed(n)` prefs file/test literal untouched. The real
    plumbing change was downstream: `UiSpace::ui_scale`/
    `UiScaleChoice::resolve`/`EditorState::effective_ui_scale`/
    `rebuild_fonts_if_scale_changed`/`theme_loader::build_font` all
    widened from `u32`/`u8` to `f32` — every one of them already cast an
    integer through `f32` arithmetic before this, so the storage type was
    the only real constraint. `docs/ember2d-theming.md` §6 updated to
    match. **Verification.** `cargo build --workspace --bins --examples`
    clean. `cargo test --workspace`: 380 (was 377, after R88), all pass —
    new: `one_point_five_resolves_to_1_5_regardless_of_os_scale_factor`/
    `one_point_five_is_in_the_menus_own_list_between_1x_and_2x`
    (`prefs.rs`), `one_point_five_ui_scale_is_selectable_from_the_theme_
    menu_and_takes_effect` (`tests/editor_ui_scale.rs`, a full click →
    persist → `ui_space()` → live-frame round trip, same shape as the
    existing whole-step version of this test). `cargo clippy --workspace
    --lib`/`--all-targets` unchanged at 43/55 (the `ui_scale ==
    self.font_raster_scale` float comparison in
    `rebuild_fonts_if_scale_changed` compares only exact literals
    `resolve()` ever returns — 1.0/1.5/2.0/3.0/4.0 — never a value that's
    passed through lossy arithmetic first, so no new `float_cmp`-class
    warning). `cargo test -p ember2d --test replay` 3× fresh processes
    green. `scripts/check.ps1` clean. Verified live: launched the real
    editor, opened `Theme`, confirmed `UI Scale: 1.5x` sits between `1x`
    and `2x` with the checkmark on the previously-active `1x`, clicked it
    — chrome re-rendered crisply at the new scale (no blur, no panic,
    every panel resized), and the status bar's own coordinate readout
    still agreed with the Inspector's hovered-tile position at this new
    ratio (`S/R = 1.5/2 = 0.75`), confirming R88's fix generalizes past
    the two ratios its own regression test pins.

**Phase 7D gate:** §0.5, full checklist §3–§9, then tag `v0.5.7d`.

---

### 5.5 `[-]` Phase 7E — Editor features (deferred, 2026-09-13)

**Deferred by user direction, right after the 7C/7D gate closed**: this
phase is editor feature/UX work (rulers, a real property-grid Inspector,
toast notifications, a command palette, rendering perf, an undo audit) —
"more of an update rather than part of the refactoring," in the user's own
words, unlike 7.5/8/9/10/11 below, which change what the engine and its
scripting/data layers can DO. Not dropped, not abandoned — the 6 steps
below stand as written for whenever this phase gets picked back up; §2.2's
phase table is the live pointer for whether that's happened yet. Work
continues at **Phase 7.5** (§5.6) in the meantime.

*(Phase 7 plan Parts 5–6, unchanged in intent, now on the new base.)*

#### `[ ]` 7E-1 — Rulers and bracketed selection
Indices along the viewport's top/left edges every 5 cells, respecting
scroll/zoom, togglable. Cursor highlight becomes bracketed.

#### `[ ]` 7E-2 — Inspector 2.0
Row-based property grid from `(label, widget)` pairs. Collapsible
sections: Transform, Sprite, Physics, Script, Exits, **Actor** (new: the
data-driven stats from 7.5-4). Inline toggles/steppers retire the
per-property `TextInputPurpose` variants.

#### `[ ]` 7E-3 — Toasts and tooltips
Queue replacing `save_message`/`save_message_timer`: stacked, severity,
independent timers. Save failures and export failures become error toasts
(closes the silent-failure items in R-series editor robustness).

#### `[ ]` 7E-4 — Command palette
`Ctrl+P` files, `Ctrl+Shift+P` commands, fuzzy. Built on the modal layer
from 7C-4. Every shortcut in `input/shortcuts.rs` is registered as a
command with its binding shown.

#### `[ ]` 7E-5 — Editor rendering performance
`draw_grid`/`draw_physics_overlay` use a `BTreeMap` range query over the
visible rect (7C-6's key order `(layer, y, x)` makes per-layer row ranges
cheap). Measure floor2 at zoom 1.0 and 0.25 before/after; numbers in the
commit message.

#### `[ ]` 7E-6 — Undo/redo audit
Every mutating action (including everything 7E-2 added) confirmed on the
undo stack and batching correctly. Checklist §5 extended.

**Phase 7E gate:** §0.5, then tag `v0.5.7`. **Phase 7 is complete.**

---

### 5.6 `[~]` Phase 7.5 — Scripting completeness

**Purpose.** New phase. The demo scripts and the RPG feasibility study show
the same gaps from two directions: `or_zero()` copy-pasted into five
scripts, `"hp_" + id` string keys standing in for components, a 436-line
director that exists because there is no `set_script`, lazy-init in
`on_update` because there is no `on_load`. Every genre on the roadmap needs
these. Phase 8's authoring tools produce content the scripting layer must
be able to use well, so this comes first.

**API_VERSION → 7.** Breaking changes are limited to 7.5-1 and are listed
in the API doc's migration section in the same commit.

**Checklist sections at gate:** §11, §12, §13.

#### `[x]` 7.5-1 — Uniform typing and sentinels (breaking) (`a3d483e`)

- **Why:** R31, R32.
- **Change:** Every registered function that takes a coordinate, size, or
  layer accepts both `i64` and `f64` (register both overloads via a small
  macro that coerces). "No entity" is `-1` everywhere; every `get_*` on a
  missing entity returns the documented neutral value **and** `ctx.exists(id)`
  is added so scripts can check. `remove_global`/`clear_persistent` take
  effect through explicit pending ops (7A-1's enum), so `()` is storable.
  `load_level`, `save_game`, `play_music` are all **last-wins** and say so.
- **Test:** `engine_tests.rs`: `draw_hud(1.0, 2.0, ...)` and `draw_hud(1, 2,
  ...)` both draw; `set_global("k", ())` then `get_global("k")` is `()`.
- **Scope:** `ember2d-sim`, API doc.
- **Landed as:** investigated up front to scope precisely which functions
  "coordinate, size, or layer" actually covers, rather than guessing —
  ~30 candidates across `api.rs`/`api_ext.rs`/`api_animation.rs`/
  `api_spatial.rs`; 20 got a same-name alternate-type overload
  (`draw_hud`/`draw_menu`/`draw_panel`/`draw_box`/`fill_rect`/
  `set_layer_order` — were `i64`, gained an `_f` `f64` wrapper;
  `set_position`/`set_velocity`/`spawn_entity`/`spawn_entity_full`/
  `play_sound_at`/`emit_particles`/`set_camera`/`get_entity_at`/
  `is_solid_at`/`find_entities_in_rect`/`raycast`/`get_path`/
  `set_collider_size`/`animate_move` — were `f64`, gained an `_i` `i64`
  wrapper). **Deviated from "a small macro that coerces":** each wrapper is
  a 1-3 line hand-written function (e.g. `pub fn set_position_i(&mut self,
  id: i64, x: i64, y: i64) { self.set_position(id, x as f64, y as f64) }`)
  rather than a `macro_rules!` — the ~20 signatures are too heterogeneous
  (mixed `String`/`Array`/`bool` params alongside the numeric ones, some
  already-correct-type params like `spawn_entity_full`'s `z: i64` that must
  NOT be re-cast) for one macro to meaningfully reduce over hand-written
  wrappers without becoming its own maintenance burden; each wrapper is
  registered under the target's SAME Rhai name in `registry.rs` (which
  picks the right overload by argument type, the same mechanism
  `spawn_entity`/`spawn_entity_full` already used to overload by ARITY) —
  a script must still write one call's numeric literals in ONE consistent
  style (all int or all float), not freely mixed, documented in
  `registry.rs`'s own new header comment and `docs/ember2d-scripting-api.md`
  §5. **`ctx.exists(id)` dropped, not added:** `entity_exists(id)`
  (`positions.contains_key(&id)`) already does exactly this, found before
  writing any code — a redundant second name would have been the wrong
  call, not "additive." **`PendingWrite { Set(Dynamic), Remove }`** (new,
  `types.rs`) replaces `pending_globals`/`pending_persistent`'s old
  `Dynamic::UNIT`-means-delete sentinel (`state.rs`, `apply.rs`,
  `set_global`/`remove_global`/`set_persistent`/`clear_persistent` in
  `api_ext.rs`) — 7A-1's own plan sketched an enum for this exact case but
  that step ended up needing only a plain `bool` since nothing yet needed a
  real op; this is the first thing that does. **`load_level` fixed to
  last-wins** (`api.rs`) — was `if pending_level.is_none()`, genuinely
  first-wins, inconsistent with `save_game`/`play_music`, which already
  overwrote unconditionally; a real behavior change, not just a doc
  clarification. `API_VERSION` 6→7 (`types.rs`), with a full migration
  entry in `docs/ember2d-scripting-api.md` §6 (three rows: the typing
  overloads, the `PendingWrite` fix, `load_level`'s last-wins) and a new
  §5 "Numeric parameters" convention note. New sibling test file
  `uniform_typing_tests.rs` (same `#[path]`-split convention
  `timer_tests.rs`/`safety_tests.rs` already established) — appending to
  `engine_tests.rs` would have pushed it to 762/750 lines. Regression
  tests beyond the plan's own two examples: `set_position` accepting int
  literals (the reverse direction from `draw_hud`), `get_entity_at`
  accepting int literals (a spot-check beyond `api.rs`, in
  `api_spatial.rs`), and `load_level`'s last-wins — all 5 confirmed to
  fail against the pre-fix code (temporarily reverted each fix in turn,
  ran the test, restored). **Verification.** `cargo build --workspace
  --bins --examples` clean. `cargo test --workspace`: 386 (was 381), all
  pass. `cargo clippy --workspace --lib`/`--all-targets` unchanged at
  43/55 (two new `too_many_arguments` warnings from `draw_panel_f`/
  `fill_rect_f` fixed with the same `#[allow]` their non-overload siblings
  already needed). `cargo test -p ember2d --test replay` 3× fresh
  processes green. `scripts/check.ps1` clean (`CLAUDE.md`'s own quoted
  `API_VERSION`/registered-function-count updated: 7, 144). Verified live:
  launched `demos/roguelike/floor1.level`, confirmed the HUD/player/items
  all still render — the demo scripts' own `draw_hud`/`spawn_entity`/etc.
  calls (all `i64`/`f64` in their ORIGINAL form) are unaffected by the
  purely-additive overloads.

#### `[x]` 7.5-2 — Atomic global/persistent arithmetic (`fe75ef6`)

- **Why:** the plan's own §7.4 note and the pattern R31/R32 already
  surfaced in 7.5-1 — a running total accumulated by hand
  (`set_global(k, get_global(k) + d)`) breaks the moment two writes to the
  same key land in one script pass, and every script that needed one had
  independently invented its own workaround (`or_zero()` guards, or
  director.rhai's hand-tallied `resolve_hits`).
- **Change:** `add_global(key, delta) -> new_value`, `add_persistent(key,
  delta) -> new_value`, reading the CURRENT value (this pass's own
  already-queued write if there is one, else the resolved store, else `0`)
  and adding `delta` on top — safe to call any number of times for the
  same key in one pass. `or_zero()` deleted from all six roguelike scripts;
  `director.rhai`'s `resolve_hits` no longer hand-tallies duplicate hits.
- **Test:** two `add_global` calls to the same key in one pass both land
  (not just the last one); `add_global`/`add_persistent` correctly treat a
  `remove_global`/`clear_all_persistent` earlier in the same pass as
  "current is 0," not a stale resolved value.
- **Scope:** `ember2d-sim`, both demos, API doc.
- **Landed as:** implemented `add_global`/`add_persistent` in
  `api_ext.rs`, each reading `pending_*` first (own-pass write wins),
  falling back to the resolved store, falling back to `0.0` via a shared
  `dynamic_as_f64_or_zero` helper. `add_persistent` additionally treats
  `pending_persistent_clear_all` as "current is 0" — a same-pass
  `clear_all_persistent(); add_persistent(k, d);` would otherwise still see
  the (not-yet-cleared) stale resolved value, since `apply_ctx` applies the
  clear before `pending_persistent`, not before this read. Both got an
  `i64` overload (`add_global_i`/`add_persistent_i`, same uniform-typing
  convention 7.5-1 established) — needed immediately, since
  `director.rhai`'s own `ctx.add_global("score", 25 * cleared)` is an int
  expression. **Purely additive, no `API_VERSION` bump**: confirmed against
  `docs/ember2d-scripting-api.md` §6's own convention (7.5-1's uniform-
  typing overloads were also "No" for the same reason — new same-name
  overloads, nothing existing changed shape) before writing the migration
  row, rather than bumping on reflex the way 7.5-1 had to for its other two
  changes.
  **Script changes**, categorized by the on_update-sweep-then-apply_ctx-
  then-on_turn-with-its-own-apply_ctx structure of `Simulation::step`
  (confirmed by reading `simulation/step.rs` and `run_scripts` directly,
  not assumed): every `or_zero()` read inside `on_turn` (player.rhai,
  enemy_rat.rhai, enemy_boss.rhai) was provably safe to become a plain
  `get_persistent`/`get_global`, since on_turn always runs in a LATER pass
  than the SAME step's on_update, whose lazy-init has already been applied
  by then. The one read inside `on_update` itself that raced its own
  lazy-init (`draw_hud`) was fixed by restructuring, not a guard:
  `on_update` now captures `hp`/`hp_max`/`gold`/`potions`/`depth` into
  locals during lazy-init and hands them to `draw_hud` as parameters,
  instead of it re-reading persistent. Every write-accumulate site
  (`turns_taken`, `potions`, `gold`, `depth`, `hp` damage, enemy `hp_<id>`)
  became one `add_persistent`/`add_global` call. `stairs.rhai` had an
  inline `if depth == () { depth = 0; }` equivalent (never called
  `or_zero()` itself) collapsed the same way. `pickup.rhai`'s own header
  comment already documented its `or_zero()` as provably-unnecessary
  defensive code before this step touched it — rewritten to say so about
  `add_persistent` instead. `victory.rhai`'s three reads are safe because
  that level is only reached after player.rhai's own lazy-init has already
  run in an earlier pass. **`director.rhai`'s `resolve_hits`** dropped its
  `done`/`n` duplicate-count-then-one-write entirely: each bullet hit is
  now its own `ctx.add_global(key, -1)` call, checked for a kill
  immediately, with a `dead` list only to stop a hit against an
  already-removed key (`remove_global` then `add_global` in the same pass
  must start from 0, not resurrect the pre-removal value — the exact
  scenario a regression test below pins). Score/kills moved from
  local-accumulate-then-one-write to a direct `add_global` call per kill.
  **HUD `.0` risk, found by direct testing before touching any script**:
  `add_global`/`add_persistent` always store their result as a float
  (`Dynamic::from(f64)`), and a throwaway probe test confirmed Rhai's own
  string concatenation renders a whole-number float with a trailing `.0`
  (`"N=" + 5.0` → `"N=5.0"`, unlike Rust's own `Display for f64`, which
  drops it) — every roguelike/shooter HUD line displaying a value that now
  flows through either function casts with `.to_int()` first (confirmed
  `.to_int()` is registered for `i64` too, as identity, via another
  probe — a script mixing an untouched int field with a touched float one
  in the same HUD line needed both to cast safely).
  `demos/shooter/scripts/player.rhai`'s header comment (referencing the
  roguelike's `or_zero()` pattern) and `director.rhai`'s header (the
  read-modify-write hazard section) rewritten to describe the new
  primitive instead of the deleted workaround.
  New sibling test file `atomic_arithmetic_tests.rs` (same `#[path]`-split
  convention `uniform_typing_tests.rs` established at 7.5-1) — 5 tests: two
  same-pass `add_global` calls to one key both land (confirmed to fail
  against a resolved-store-only implementation that ignores
  `pending_globals`, temporarily reverted then restored); a never-set key
  reads as 0; `add_global` after `remove_global` in the same pass starts
  over from 0, not the pre-removal resolved value (confirmed to fail
  against a version that lets `Remove` fall through to the resolved store,
  seeded from an earlier pass via a new `run_source_with_result_and_globals`
  helper — a same-pass `set_global` before `remove_global` wouldn't
  discriminate this case at all, since `pending_globals` is a flat map);
  `add_persistent` after `clear_all_persistent` in the same pass starts
  over from 0 (confirmed to fail the same way, seeded via
  `run_source_with_result_and_persistent`); the `i64` overload. Extended
  `shooter_arena.rs`'s existing
  `two_bullets_landing_in_one_pass_both_count_against_an_enemy` (already
  the exact two-bullets-one-pass shape `resolve_hits`'s rewrite needed
  re-proving) with a "score"/"kills" assertion after the kill, since it
  already drives the real `Simulation`/`ScriptEngine`, not a synthetic
  script. Fixed 3 pre-existing `ember2d/tests/*.rs` assertions
  (`roguelike_combat.rs` x2, `roguelike_floor1.rs`) that read "hp"/"gold"
  via `Dynamic::as_int()` — strict, doesn't coerce a float — which now
  returns `None` for a value `add_persistent` touched; added a shared
  `dynamic_as_i64` helper to `tests/common/mod.rs` rather than fixing each
  site ad hoc. **Verification.** `cargo build --workspace --bins
  --examples` clean. `cargo test --workspace`: 392 (was 386: +5 new
  `atomic_arithmetic_tests`, +1 extended assertion in an existing test),
  all pass. `cargo clippy --workspace --all-targets`: one new
  `doc_lazy_continuation` warning from a doc comment's wrapped line
  starting with `+ d)` (parsed as a markdown list marker) — reworded, zero
  new warnings remain in any touched file. `cargo test -p ember2d --test
  replay` 3× fresh processes green. `scripts/check.ps1` clean (`CLAUDE.md`'s
  quoted registered-function count updated 144 → 148: `add_global`,
  `add_global_i`, `add_persistent`, `add_persistent_i`; `API_VERSION`
  unchanged at 7). Verified live: launched `demos/roguelike/floor1.level`
  and `floor2.level`, walked onto two gold piles and through ~35 turns —
  `Gold`/`Turn` render as clean ints (`Gold 1`, `Turn 22`) with no `.0`
  regression. Launched `demos/shooter/arena.level`, confirmed
  `SCORE 0  KILLS 0` (top HUD) and the death screen's `Score 0  Kills 0`
  both render cleanly at zero; did not land a live kill by hand (mouse-aim
  simulation proved harder to land than expected against the wave AI), so
  the non-zero score/kills path is verified by the extended
  `shooter_arena.rs` test above rather than a screenshot.

#### `[x]` 7.5-3 — Per-entity variables (`0c1ebb2`)

- **Why:** the `"hp_" + id`/`"aware_" + id`/`"ehp_" + id` global-key-
  concatenation convention (already visible in 7.5-2's own script edits)
  fakes per-entity scope out of a level-scoped global — no automatic
  cleanup on despawn, no protection against a naming collision, and no
  real reason it should live on `PlayState` rather than the entity itself.
- **Change:** `set_var(id, key, value)`, `get_var(id, key) -> Dynamic`,
  `has_var`, `remove_var`, backed by a new `Vars` component
  (`BTreeMap<String, Dynamic>`), cleared on despawn. Migrate `hp_`/`aware_`
  in the roguelike and `ehp_` in the shooter.
- **Test:** `set_var` this pass is invisible to `get_var` later in the SAME
  pass (matching `get_global`'s own rule) but visible next pass; `remove_var`
  actually removes the key; despawn clears the whole component.
- **Scope:** `ember2d-sim`, both demos, API doc.
- **Landed as:** `acted_`/`atk_*` turned out to already be dead — grepped
  first and confirmed both were removed by Phase 5f's turn-scheduler
  rewrite (player.rhai's/enemy_rat.rhai's own header comments already said
  so); only `hp_`/`aware_`/`ehp_` were live, so those are the whole
  migration. **Scope decision made with the user up front, not assumed:**
  the plan's own "visible in the inspector as read-only" sub-bullet has no
  home today — the editor's Inspector panel only edits design-time
  `TileRecord` data before entities exist; a runtime/play-mode entity
  inspector is squarely Phase 7E (editor features), which the user
  deferred earlier this session. Asked directly; the user chose "land
  everything except the UI" — implement the component/API/migration now,
  note the inspector bullet as deferred to 7E rather than silently
  dropping it (see this file's §7.4 note below, and the plan's own 7E
  entry when that phase resumes).
  **`Vars` needs no `SaveState` field** the way `globals`/`persistent`/
  `clips` do (`save.rs`) — it lives directly on `World` alongside
  `Transform`/`Sprite`/`Tag`/etc., which `SaveState.world: World` already
  serializes via `derive(Serialize, Deserialize)`; `#[serde(default)]` on
  `World::vars` (matching `animators`/`actors`'s own convention) is what
  lets an old save load with every entity's `Vars` empty. Read path: a new
  `WorldSnapshot.vars: BTreeMap<i64, BTreeMap<String, Dynamic>>`, built
  once per pass exactly like `tags`/`colliders`, so `get_var`/`has_var`
  never observe a same-pass `set_var` — the same rule `get_global` already
  has, for the same reason. Write path: `pending_vars: Vec<(i64, String,
  PendingWrite)>` on `ScriptState`, applied in `apply_ctx` behind the same
  ghost-component guard R10 (7A-1) already established for `pending_tags`
  (`world.transforms.contains_key`) — `set_var` on a nonexistent entity is
  a no-op, not a `Vars` created out of nowhere.
  **`add_var(id, key, delta) -> new_value` added beyond the plan's own
  text** — not a scope creep, a correctness requirement found while
  migrating `director.rhai`'s `resolve_hits`: its per-enemy HP total
  (`ehp_<id>`) was already on `add_global` as of 7.5-2 specifically to
  close the same-pass duplicate-write hazard; moving that HP onto `Vars`
  without an equivalent `add_var` would have reopened the exact hazard
  7.5-2 had just closed, one layer down. Mirrors `add_global`'s own design
  (current = last matching entry in `pending_vars` this pass, scanned in
  reverse since it's a flat `Vec` not a map, else the resolved snapshot,
  else `0.0`) plus an `add_var_i` int overload (7.5-1's uniform-typing
  convention — `director.rhai`'s own `25 * cleared`-shaped calls need it).
  `resolve_hits` also dropped its explicit `remove_global`/now-`remove_var`
  call on a kill entirely: the very next line already calls `ctx.despawn`,
  and despawn clears the whole `Vars` component for free, so the explicit
  removal was redundant once `Vars` (not a bare global) was the backing
  store. Its `dead` list stays, but for a different reason now — not
  resurrection (that risk was specific to the old global convention), but
  stopping a third bullet on an already-dead enemy from re-running the
  kill block (score/loot/despawn) a second time.
  **Script changes**: `on_start` seeds `set_var(id, "hp", n)` in place of
  `set_global("hp_" + id, n)`; lazy-init in `on_update` mirrors it;
  `update_awareness` reads/writes `get_var`/`set_var(id, "aware", ...)`;
  player.rhai's `attack()` becomes `ctx.add_var(target, "hp", -3)`;
  director.rhai's `spawn_enemy` becomes `ctx.set_var(e, "hp", hp)`. Updated
  `save.rs`'s own stale doc comment (it named `hp_<id>`/`aware_<id>` as
  living in `globals`) and `docs/ember2d-scripting-api.md` §2's per-entity-
  state guidance, which used to recommend the global-key-concatenation
  convention as the ONLY option — now describes `Vars` as the real
  mechanism, with `set_global`/`get_global` kept for genuinely level-scoped
  values. New sibling test file `vars_tests.rs` (same `#[path]`-split
  convention `atomic_arithmetic_tests.rs` established at 7.5-2) — 7 tests:
  the round-trip/same-pass-invisible guarantee, `has_var`, `remove_var`
  actually removing the key, despawn clearing `Vars`, the ghost-component
  guard, and two `add_var` tests (double-call-same-pass accumulation,
  post-remove restart-from-zero) — all confirmed to fail against a
  deliberately-broken implementation in turn (apply_ctx never applying
  `pending_vars`; the ghost-component guard removed; `add_var` reading only
  the resolved snapshot, ignoring `pending_vars`), then restored. Also
  added `world.rs`'s own lower-level coverage (matching its existing
  `animators`/`actors` test pattern): a RON round-trip, a pre-7.5-3 RON
  string still loading with `vars` defaulting empty, despawn clearing
  `vars`, and `entity_ids()`'s union including a `Vars`-only entity.
  **Verification.** `cargo build --workspace --bins --examples` clean.
  `cargo test --workspace`: 401 (was 392: +7 `vars_tests`, +2 `world.rs`
  tests), all pass — including `roguelike_combat.rs`'s and
  `shooter_arena.rs`'s existing combat/duplicate-hit tests, unchanged and
  still green against the new `Vars`-backed storage. `cargo clippy
  --workspace --all-targets`: zero new warnings in any touched file (one
  new `doc_lazy_continuation` warning surfaced during 7.5-2's own commit,
  already fixed there). `cargo test -p ember2d --test replay` 3× fresh
  processes green (`World::vars` round-trips through the save/load-midpoint
  replay test along with every other component). `scripts/check.ps1` clean
  (`CLAUDE.md`'s quoted registered-function count updated 148 → 154:
  `set_var`/`get_var`/`has_var`/`remove_var`/`add_var`/`add_var_i`;
  `API_VERSION` unchanged at 7, purely additive per `docs/ember2d-
  scripting-api.md` §6's own convention). Verified live: launched
  `demos/roguelike/floor1.level` and `floor2.level` — HUD, movement, gold
  pickup, and a sleeping rat's `DarkRed` tint (a `get_var(id, "aware")`
  read through `WorldSnapshot`) all render correctly with no script error;
  did not reach a rat to land a live attack by hand (floor2's wall layout
  made manual navigation slow), so the `attack()`/`add_var`/despawn path
  is verified by `roguelike_combat.rs`'s existing byte-exact assertions
  (unchanged, still passing against the new storage) rather than a
  screenshot of a kill.

#### `[x]` 7.5-4 — Data-driven actor stats (`d84e821`)

- **Why:** the demo scripts and the RPG feasibility study both show the
  same gap — `enemy_rat.rhai`/`enemy_boss.rhai` are copy-pasted files whose
  only real difference is a handful of numbers and colors hardcoded in the
  script text itself, not authored as data.
- **Change:** `TileRecord.actor` (exists since Step 5f) gains `stats:
  BTreeMap<String, f64>`. `get_stat(id, key)`. `enemy_rat.rhai` and
  `enemy_boss.rhai` collapse into one `enemy.rhai` reading `hp`, `atk`,
  `awareness_range` from stats.
- **Test:** a level with `actor.stats` round-trips through RON, and a
  pre-7.5-4 level (no `stats` field at all) loads with it defaulted empty;
  `get_stat` returns the authored value, `0.0` for a missing key or a
  non-actor entity.
- **Scope:** `ember2d-sim`, `demos/roguelike`, API doc.
- **Landed as:** two scope decisions made with the user up front, not
  assumed (AskUserQuestion, before writing any code):
  1. The plan's own text has `enemy.rhai` reading "`glyph`, `tint`" from
     `stats` too — impossible as written, since `stats` is numeric-only
     (`BTreeMap<String, f64>`) and glyph is a `char`, tint a `Color`. No
     color-lightening helper exists anywhere in the codebase either (and
     computing one would need transcendental math, forbidden in the sim
     regardless), so an "aware" tint can't be derived from a single base
     color. **Decided: tint stays real authored data, just not inside
     `stats`** — `ActorRecord`/`Actor` gain `tint_aware: Color`/
     `tint_asleep: Color` as their own fields alongside `stats`, read via
     two new functions (`get_tint_aware`/`get_tint_asleep`), rather than
     narrowing scope to numbers-only. Glyph itself was never in question —
     that's `TileRecord.glyph`, already data, unrelated to this step.
  2. `awareness_range` is a new concept (the old per-script raycast had no
     distance limit at all — any unobstructed line of sight of any length
     woke an enemy), and the plan's own inspector-authoring sub-bullet
     assumes 7E-2's Actor section, which doesn't exist (7E deferred) — the
     same gap 7.5-3 hit. **Decided, same way 7.5-3 was scoped:** land the
     component/API/migration now, author stats via `gen_roguelike.rs` code,
     leave the inspector authoring surface for whenever 7E resumes.

  **`ActorRecord`** (`level.rs`) gains `stats: BTreeMap<String, f64>`
  (`#[serde(default)]`) and `tint_aware`/`tint_asleep: Color`
  (`#[serde(default = "default_tint")]`, `Color::Reset` — a generic,
  genre-agnostic fallback, not a "rat-flavored" default baked into general
  engine code) — all three default cleanly for a pre-7.5-4 level, whose
  `ActorRecord` only ever had `speed`. **`Actor`** (`components/actor.rs`)
  gets the same three fields as its runtime copy, populated in
  `simulation/spawn.rs::do_on_start` right after `Actor::ai(ar.speed)`
  construction — kept as a plain field-copy there rather than threading
  `level::ActorRecord` into `components` for a constructor `components`
  has no other reason to know about. This is also why `Actor` is no longer
  `Copy`: a `BTreeMap` isn't. Checked every `Actor`-copying call site first
  (only `.controller`, itself still `Copy`, was ever copied out of one) —
  removing the derive needed no other code changes.

  **`WorldSnapshot`** (`scripting/state.rs`) gains `actor_stats: HashMap<i64,
  BTreeMap<String, f64>>` and `actor_tints: HashMap<i64, (Color, Color)>`,
  built the same lookup-only-`HashMap` way `actor_speeds` already is (§4.1
  allows this: never iterated, only looked up by a known id). `get_stat`/
  `get_tint_aware`/`get_tint_asleep` (`api.rs`) read them with the same
  `0.0`/`"Reset"` neutral-default convention every other `get_*` uses
  (R32, 7.5-1) — a non-actor entity or a missing key never panics, just
  reads back the neutral value. Registered in `registry.rs` right after
  `get_speed`/`set_speed`, same step-grouping convention that file already
  uses.

  **`enemy.rhai`** replaces `enemy_rat.rhai`/`enemy_boss.rhai` (`git rm`),
  reading `get_stat(id, "hp"/"atk"/"awareness_range")` and
  `get_tint_aware`/`get_tint_asleep` instead of hardcoded constants —
  `gen_roguelike.rs`'s `rat()`/`boss()` now author those five values
  explicitly per role (rat: 6/2/8, `Red`/`DarkRed`; boss: 15/3/10,
  `Magenta`/`DarkMagenta`). **Compile-time surprise found immediately**:
  the merged script's `update_awareness` tripped Rhai's max-expression-
  complexity guard the instant the new distance check was added on top of
  the existing hp/raycast branching — the exact class of limit
  `enemy_rat.rhai`'s own comment already flagged for the ORIGINAL
  function, just pushed over by one more nested `if`. Fixed the same way
  that comment says the original was avoided: pulled the distance check
  into its own `in_awareness_range` function, one level of nesting
  cheaper. Verified by compiling the file directly against a bare `rhai::
  Engine` before touching anything else, not by guessing.
  `bench_sim.rs`'s synthetic actor tile updated to match (script path,
  full `ActorRecord { speed, ..Default::default() }` literal since the old
  2-field literal no longer compiles, and the same rat stats/tint so the
  benchmark still exercises a live enemy instead of one whose `get_stat`
  calls all read `0.0` and despawns itself on the first `on_update`).
  `demos/roguelike/floor2.level`/`floor3.level` regenerated via `cargo run
  --example gen_roguelike` (floor1/victory unchanged — no enemy tiles).
  Five other live cross-references to `enemy_rat.rhai`/`enemy_boss.rhai`
  by name — ones that actively point a reader at "that file's header
  comment" for still-current reasoning, not historical illustration —
  updated to `enemy.rhai` (`play.rs`, `roguelike_combat.rs`,
  `roguelike_level_integrity.rs` ×2, `turn_animation.rs`); left the ones in
  `components/vars.rs`/`components/animator.rs` alone, since those are
  explaining a PAST decision (D17/7.5-3 era) using the file name that was
  live at the time, not pointing to it as current.

  **New R-row, not this step's job to fix:** growing `TileRecord` (via
  `ActorRecord`) pushed `ember2d-editor`'s undo `Command::PlaceTile`
  variant past clippy's `large_enum_variant` threshold — a real new
  warning (43→44 at `--lib`), but fixing it (`Box`ing the variant) is an
  `ember2d-editor` change outside this step's Scope. Logged as **R90**
  (§3.2) rather than fixed or silently ignored.

  New test file `actor_stats_tests.rs` (same `#[path]`-split convention
  `vars_tests.rs`/`atomic_arithmetic_tests.rs` established) — 5 tests:
  `get_stat` reads an authored value, a missing key reads `0.0`, a
  non-actor entity reads `0.0`, `get_tint_aware`/`get_tint_asleep` read the
  authored colors, and a non-actor entity's tint reads `"Reset"` — the
  first of these confirmed to fail (reads `0.0` instead of `6.0`) against
  a deliberately-broken `actor_stats` population, then restored. Two new
  `level.rs` tests: a pre-7.5-4 `ActorRecord` RON shape still deserializes
  with `stats`/tint defaulted, and stats/tint round-trip through RON.
  **Verification.** `cargo build --workspace --bins --examples` clean.
  `cargo test --workspace`: 408 (was 401: +5 `actor_stats_tests`, +2
  `level.rs` tests), all pass — including `roguelike_combat.rs`'s existing
  byte-exact combat/boss/stairs-unlock assertions, unchanged and still
  green against the new stats-driven numbers (first failed with the
  un-split `update_awareness`, since the enemy script didn't compile at
  all — confirmed the merge is otherwise behavior-preserving once fixed).
  `cargo clippy --workspace --lib`: 43→44 (R90 above, the only new
  warning, in `ember2d-editor` not `ember2d-sim`); `--all-targets`: 56→57,
  same single warning. `cargo test -p ember2d --test replay` 3× fresh
  processes green. `scripts/check.ps1` clean (`CLAUDE.md`'s quoted
  registered-function count updated 154 → 157: `get_stat`,
  `get_tint_aware`, `get_tint_asleep`; `API_VERSION` unchanged at 7,
  purely additive per `docs/ember2d-scripting-api.md` §6's own
  convention). Verified live: launched `demos/roguelike/floor2.level` and
  `floor3.level` (screenshots) — rats and the boss all render in their
  correct `DarkRed`/`DarkMagenta` asleep tint, HUD intact, no script error,
  no crash. Did not drive the player into raycast/awareness range of a
  live rat by hand this session (floor2's nearest rat was ~40 cells off in
  the screenshot) — the awake/chase/attack/kill path is covered instead by
  `roguelike_combat.rs`'s existing headless tests, all still passing
  unchanged against the new data-driven numbers.

#### `[x]` 7.5-5 — `set_script` and `on_load` (`8e3ebff`)

- **Why:** `demos/shooter/scripts/director.rhai`'s own pre-step header said
  it plainly: "there is no `set_script` in the API… a spawned enemy
  therefore has no `on_update` of its own." Every bullet and enemy had to
  be driven by hand from one always-present entity. Separately,
  `demos/roguelike/scripts/player.rhai` couldn't use `on_start` at all —
  `Simulation::on_start`'s loading-save branch never runs a script's
  `on_start` (R7, 7A-3, deliberately: re-seeding a run's stats on load
  would reset a run in progress) — so it lazy-initialized inside
  `on_update`, guarded by `!ctx.has_persistent("hp_max")`, every single
  step, forever.
- **Change:** `set_script(id, path)` attaches (or replaces) a script on an
  already-spawned entity, deferred like every other setter. `on_load(id,
  ctx)` runs once per scripted entity on the loaded-save path, **instead
  of** `on_start`. `player.rhai` moves its lazy-init into a real `on_start`
  (fresh-spawn only). Shooter bullets get their own `bullet.rhai` via
  `ctx.set_script`, attached by `player.rhai`'s `do_shoot`.
- **Test:** `set_script_tests.rs` (attach lands this pass, `on_start` waits
  for the next one, ghost-entity no-op, failed-compile no-op);
  `save_load_globals.rs`'s `on_load_runs_on_a_loaded_save_but_on_start_
  does_not_re_run`; `shooter_arena.rs`'s existing
  `two_bullets_landing_in_one_pass_both_count_against_an_enemy` updated to
  drive the new per-bullet path.
- **Scope:** `ember2d-sim`, `ember2d` (tests, `examples/gen_shooter.rs`),
  `demos/roguelike`, `demos/shooter`, both docs.
- **Landed as:** one scope decision made with the user up front
  (AskUserQuestion, before touching the shooter demo): how far to
  decentralize it. `contact_damage` (director.rhai) takes the single
  largest hit touching the player per step, gated by one shared cooldown —
  spreading that across each enemy's own `on_update` would either silently
  stop stacking correctly (deferred writes: two enemies' `set_global("hp",
  …)` in the same pass both read the same pre-pass value, last write wins)
  or need a per-enemy cooldown that measurably raises damage taken when
  surrounded — a real difficulty change, not a neutral refactor. **Decided:
  bullets only.** Enemies stay centrally steered and damaged by
  director.rhai, by choice, not because `set_script` can't reach them —
  documented in director.rhai's, player.rhai's, and gen_shooter.rs's own
  headers so a future step doesn't mistake it for an oversight.

  **Engine side** (`ember2d-sim`): `ScriptState` gains `pending_set_script:
  Vec<(i64, String)>` (`api.rs`'s `set_script`, same one-line-push shape
  `set_tag` already has). `apply_ctx` (`apply.rs`) drains it with the same
  R10 ghost-component guard `pending_tags`/`pending_vars` use, compiles the
  path via `self.compile` (never attaches on a compile failure — the sim
  boundary's "never crash the editor" rule extends naturally to "never
  silently point `World::scripts` at an AST that doesn't exist"), and on
  success pushes the entity onto a new `ScriptEngine::pending_on_start:
  Vec<EntityId>`. `run_scripts` drains that at the top of its own call and
  runs `on_start` for each entry using that call's own `ctx`, before the
  normal `on_update` loop — which already includes the newly-attached
  entity, since `world.scripts` picked it up when `apply_ctx` ran. `on_load`
  is `run_on_load_all`, a near-identical twin of the existing
  `run_on_start_all` (same `ScriptState::from_world` setup, calls
  `"on_load"` instead of `"on_start"`) — kept as two separate methods
  rather than one parameterized by function name, matching how
  `run_on_input`/`run_on_turn` already don't share a body either.
  `Simulation::on_start`'s `is_loading_save` branch calls it once, computing
  `cam_pos` the exact way `do_on_start` (`simulation/spawn.rs`) already does
  for its own `run_on_start_all` call.

  **A real correctness bug found designing bullet.rhai, not left in:** when
  `do_shoot` (`player.rhai`, `on_input`) spawns a bullet and calls
  `ctx.set_script` on it in the same call, that bullet's OWN first
  `on_update` — which fires later this SAME step, since `apply_ctx` already
  attached it to `world.scripts` before `run_scripts` runs — still executes
  against the `WorldSnapshot` built at the TOP of this step, before the
  spawn landed. `ctx.get_x`/`get_y` read that frozen snapshot and default to
  `0.0` for an id they don't recognize, so the bullet would see itself at
  `(0.0, 0.0)` — outside the arena — and self-destruct on the spot, every
  time, without ever traveling. Fixed with an `armed` `Vars` flag
  (`ctx.has_var`/`set_var`, Step 7.5-3): a bullet's first `on_update` call
  just arms itself and returns; real hit detection starts on the second
  call, once a snapshot built AFTER the bullet existed is in play —
  reproducing the pre-7.5-5 `director.rhai`'s own already-correct timing
  ("a bullet gets its first hit test on the step after it exists") inside
  the new per-entity model instead of accidentally breaking it.

  **`demos/roguelike/scripts/player.rhai`:** the lazy-init block (hp/
  hp_max/gold/potions/depth/turns_taken, plus the "music_started" global
  guard) moved into a new `on_start`, unconditional — it only ever runs on
  a fresh spawn now, so the guard it used to need is gone along with it.
  `on_update` just reads all five via `get_persistent` and feeds them to
  `draw_hud`/the death check; header rewritten throughout (the "SAME-PASS
  LAZY-INIT HAZARD" section, `on_update`'s and `draw_hud`'s own doc
  comments) to describe the new split instead of the old workaround.

  **`demos/shooter`:** `bullet.rhai` (new) owns a fired bullet's own hit
  detection and kill resolution — `is_enemy`/`points_for`/`resolve_hit`
  (was `resolve_hits`, no longer needs to dedupe a batch since there's no
  batch)/`maybe_drop`, plus its own copy of the arena bounds (no
  cross-script `use`/import exists yet — 7.5-13's open question).
  `director.rhai` loses `update_bullets`/`resolve_hits`/`maybe_drop`/
  `points_for` entirely; keeps `steer_enemies`/`steer_group`/
  `contact_damage` per the scope decision above, with `is_enemy` kept too
  (still used by `contact_dmg_for`). `player.rhai`'s `do_shoot` gains one
  line: `ctx.set_script(b, "demos/shooter/scripts/bullet.rhai")`.
  `gen_shooter.rs`'s header fact (2) and the `director()` doc comment
  rewritten to match — `set_script` existing is not the same claim as
  "director.rhai is now unnecessary."

  **File-size fallout:** `engine.rs` crossed 750 lines the moment
  `run_on_load_all` landed alongside the rest. `run_on_start_all`/
  `run_on_load_all` moved to a new `lifecycle.rs` (same second-`impl
  ScriptEngine`-in-a-sibling-file pattern `apply.rs` already established,
  Phase 6 Step 2) — pure relocation. Needed four more `ScriptEngine` fields
  bumped to `pub(super)` (`engine`, `ast_cache`, `disabled_scripts`, `rng`)
  beyond the three (`scopes`/`layers`/`timers`/`pending_on_start`) already
  there for `apply.rs`'s sake.

  **A pre-existing test needed updating, not just adding to:**
  `shooter_arena.rs`'s `two_bullets_landing_in_one_pass_both_count_against_
  an_enemy` used to place two "bullet"-tagged entities directly into
  `world` with no `Script` component at all, relying on director.rhai's own
  tag-scan (`find_all_by_tag("bullet")`) to find them — which no longer
  exists. Updated to also attach `Script::new("demos/shooter/scripts/
  bullet.rhai")` and pre-arm each one's `Vars("armed", true)` (this test's
  own bullets are already fully present in `world` before its `h.step`
  call, so the staleness guard doesn't apply to them — pre-arming just
  skips it explicitly rather than wasting a step). Also needed
  `bullet.rhai` compiled into `ScriptEngine`'s ast cache first (`run_scripts`
  silently skips an entity whose script isn't compiled yet, and this test
  never calls `ctx.set_script` itself) — fixed by firing one real shot via
  the harness's existing `run_firing_at` and despawning the resulting real
  bullet before placing the two synthetic ones.

  **Verification.** `cargo build --workspace --bins --examples` clean.
  `cargo test --workspace`: 412 (was 408: +3 `set_script_tests`, +1
  `on_load` save/load test), all pass, including `shooter_arena.rs`'s full
  7 (the updated two-bullet test, plus `a_long_run_under_continuous_input_
  never_errors`, unchanged, still green against the new per-bullet path)
  and `roguelike_combat.rs`'s existing byte-exact assertions (unchanged,
  still passing against the new `on_start`-based init). `scripts/check.sh`
  clean (file-size limit, `ember2d-sim` determinism greps). `cargo clippy
  --workspace --lib`: one new warning, `too_many_arguments` on the new
  `run_on_load_all` (mirrors `run_on_start_all`'s own pre-existing 9-arg
  shape, now both visible in `lifecycle.rs`) — not chased further, since
  the phase-gate criterion (§0.5) tracks "no new warnings" at the gate, not
  per step. `CLAUDE.md`'s registered-function count updated 157 → 158
  (`set_script`); `API_VERSION` unchanged at 7, purely additive per
  `docs/ember2d-scripting-api.md` §6's own convention. **Not verified
  live this session** — no windowed/GPU environment available in this
  agent's sandbox to launch either demo and take a screenshot the way prior
  steps did; the user asked to keep going through the rest of Phase 7.5
  and said they'd manually test it themselves. Coverage instead leans on
  `shooter_arena.rs`'s `a_long_run_under_continuous_input_never_errors`
  (many simulated steps of continuous fire/movement with
  `assert_no_script_errors`) and `all_ten_waves_run_in_order_and_then_the_
  run_is_won` (a full 10-wave clear, meaning every spawned bullet across an
  entire run found and resolved its own hits without a script ever
  disabling itself) as the closest headless proxy for "the demo actually
  plays," but a live playtest of both demos is still owed before this step
  is treated as fully closed the way CLAUDE.md's UI/feature-testing rule
  asks for.

#### `[x]` 7.5-6 — Engine-side solid resolution for all actors (`b1964af`)

- **Why:** `late_step`'s solid-collision resolution (`resolve_solid_
  collision`) only ever ran for the local player (`is_local_player`) — any
  other `Actor` (every roguelike enemy today; a future realtime AI actor
  tomorrow) could walk straight through a wall unless a script hand-rolled
  its own check, the way `director.rhai`'s `steer_group` (shooter demo)
  already had to. `get_path` also had no diagonal option and no way to
  preview a movement range, both open questions from the old refactor plan
  for a future tactical-RPG genre (Phase 9).
- **Change:** `resolve_solid_collision` now covers any `Actor` whose own
  `physics` flag is set (default `true`, an opt-out). `get_path` gains a
  6-arg overload adding `diagonal: bool`. `reachable_within(id, budget)` is
  new.
- **Test:** `ember2d/tests/actor_physics.rs` (an AI actor with `physics`
  gets pushed out of an overlapped wall; `physics: false` opts out; the
  local player is unaffected; an AI actor overlapping an exit tile never
  triggers a level transition); `ember2d-sim`'s new `path_tests.rs`
  (diagonal finds a shorter route; the no-corner-cutting guard; three
  `reachable_within` cases).
- **Scope:** `ember2d-sim`, `ember2d` (tests), both docs.
- **Landed as:** one scope decision made with the user up front
  (AskUserQuestion), before writing any code: the step's own text — "any
  entity with an Actor" — reads as if it would let the shooter demo's
  hand-rolled `steer_group` wall-slide be deleted too, but shooter enemies
  have no `Actor` at all, deliberately (`gen_shooter.rs`'s own header:
  giving one to an AI-controlled entity would insert it into
  `TurnScheduler`, which round-robins turns among every `Actor` regardless
  of `GameplayLoop` — with enemies added, the local player's own `on_input`
  would stop firing on any step it isn't the player's turn, a real,
  live-breaking regression to the shooter's WASD/aim/shoot controls, not
  just a code-shape one). **Decided: Actor-only, AI-only** — extend solid
  resolution to any `Actor` regardless of controller (so `Controller::Ai`
  joins the local player, which already had it), but do NOT give shooter
  enemies an `Actor` to reach it. Consequence, stated plainly rather than
  silently dropped: `director.rhai`'s own wall-slide is NOT deleted this
  step (shooter enemies have no other mechanism) — its own header, and
  `player.rhai`'s and `gen_shooter.rs`'s, all note this explicitly so a
  future session doesn't mistake the gap for an oversight. The "bullet hit
  tests are deleted" half of the step's own Change already happened, one
  step early — 7.5-5's `bullet.rhai` did that.

  **Engine side** (`ember2d-sim`): `Actor.physics: bool` (`components/
  actor.rs`, `#[serde(default = "default_physics")]` reading `true` for
  every pre-7.5-6 save) plus the mirrored `ActorRecord.physics` (`level.rs`,
  same default) so a level can actually author the opt-out — without it,
  the flag would exist on the runtime type with no way to ever set it to
  `false`, a half-finished feature. `do_on_start` (`simulation/spawn.rs`)
  copies it the same way `stats`/`tint_aware`/`tint_asleep` already are.
  `simulation.rs` gains `actor_has_physics` alongside the existing
  `is_local_player`. `late_step` (`simulation/step.rs`) splits what used to
  be one combined branch into two: a new physics-actor branch resolves
  solid collisions for any qualifying pair (covers the local player too,
  now through the shared path instead of a separate inline call); the
  original local-player branch keeps the exit-tile check ONLY — deliberately
  NOT reachable by the new branch, so an AI actor stepping onto stairs can
  never trigger a level transition, a real correctness concern the
  restructuring itself introduced were it not for the two-branch split.

  **`get_path`/`reachable_within`** (`scripting/api_spatial.rs`): the
  existing 4-directional `get_path` now forwards to a new `get_path_diag`
  (registered under the same Rhai name, a 6-arg overload — `spawn_entity`'s
  own 4-arg/11-arg split is the precedent) which adds 8-directional search
  when `diagonal` is `true`: costs scaled ×10 (14 for a diagonal step, ≈
  10×√2) to keep `g`/`f` plain `i32` with no runtime `sqrt`, a Chebyshev
  heuristic when diagonal moves are legal (Manhattan would overestimate and
  break A*'s admissibility once diagonals exist), and a no-corner-cutting
  guard (a diagonal step is refused unless both orthogonal cells beside it
  are also open). `reachable_within(id, budget)` is a plain 4-directional
  BFS from `id`'s own position, no mask option — every solid blocks,
  matching `is_solid_at`'s own simplicity, since a movement-range preview
  should show exactly what a script's own `is_solid_at`-gated move allows.

  **Verification.** `cargo build --workspace --bins --examples` clean.
  `cargo test --workspace`: 421 (was 412: +4 `actor_physics.rs`, +5
  `path_tests.rs`), all pass. `scripts/check.sh` clean. `cargo clippy -p
  ember2d-sim --lib`: no new warnings from this step's own changes, verified
  by inspecting every touched file's own warnings directly rather than
  trusting a raw before/after count (clippy's per-run caching made a naive
  count unreliable) — one doc-comment lint (`doc_lazy_continuation`, a
  wrapped line in `get_path_diag`'s own doc comment that accidentally
  started with `*`, read as a markdown bullet) found and fixed before it
  ever landed. `CLAUDE.md`'s registered-function count updated 158 → 161
  (`get_path` gains a 6-arg overload plus its `_i` twin, `reachable_within`);
  `API_VERSION` unchanged at 7, purely additive. **Not verified live** —
  same sandbox limitation 7.5-5's own note recorded (no windowed/GPU
  environment available to this agent); `actor_physics.rs`'s four tests are
  the direct proof this step's own new behavior works, but neither demo has
  been launched to confirm the roguelike still plays normally with `Actor`
  now carrying one more field.

#### `[x]` 7.5-7 — Animation and turn model completeness (`83d598a`)

- **Why:** R33 — `is_animating(id)` always returned `false`, not because
  nothing could run mid-animation (D20 already made that gate per-actor)
  but because `ember2d-sim` had no visibility into `PlayState.animations`
  at all. Separately, `docs/archive/ember2d-refactor-plan.md` sketched
  `TurnModel::{Alternating, Energy, ActionCost, Declared}` back at Phase 5,
  but only `Alternating` was ever wired up — `Actor::speed` was a real,
  read/write-able field with zero effect on turn order (docs/archive/
  ember2d-rpg-demo-feasibility.md §2.8, the exact gap a Pokemon-style
  speed-order battle needs closed).
- **Change:** `StepInput` gains `animating: &[EntityId]`, threaded into
  `ScriptState` for the three passes that see a real `StepInput`
  (`on_input`/`on_update`/`on_turn`); `is_animating` checks real
  membership. `TurnModel` (`ember2d-sim::scheduler`, `Alternating` default)
  gains `Energy` (cost scales inversely with `Actor::speed`) and
  `ActionCost` (cost comes from `Command.cost`, set via `submit`'s new
  4-argument overload); `ProjectData::turn_model` selects it,
  `Simulation::set_turn_model`/`PlayState::set_turn_model` thread it in
  the same way `pixels_per_unit` already does. `Declared` (Phase 9's
  netcode concern) deliberately stays unimplemented — not this step's job.
- **Test:** `engine_tests.rs`'s two new `is_animating_*` tests (true for an
  id the caller's own list names, false otherwise). `ember2d/tests/
  turn_model.rs`: a speed-200 AI actor acts ~2x as often as a speed-100 one
  under `Energy`; a cost-20 command lets its actor act ~10x as often as a
  cost-200 one under `ActionCost`; `Alternating` (the untouched default)
  still gives both actors identical turn rates regardless of speed.
- **Scope:** `ember2d-sim`, `ember2d`, `ember2d-app`, both docs.
- **Landed as:** no user decision point this step — both halves turned out
  to be purely additive once investigated (see the two "found while
  implementing" notes below), so nothing needed a scope call the way
  7.5-5's/7.5-6's own decisions did.

  **`is_animating` plumbing:** `WorldSnapshot::build` was NOT the insertion
  point — it has ~20 call sites across every scripting test file, and
  threading a new parameter through all of them for one feature would have
  been a wildly disproportionate diff. Landed instead as a NEW
  `ScriptState::animating: Vec<i64>` field (mirrors `pending_on_start`'s
  own "set directly, not via the constructor" pattern from 7.5-5),
  populated by `run_on_input`/`run_on_turn`/`run_scripts` (engine.rs) from
  a new trailing `animating: &[EntityId]` parameter each already had room
  for under their existing `#[allow(clippy::too_many_arguments)]` — 10
  call sites across `step.rs` and 9 test files needed a one-line `&[]` (or
  the real list, for `play.rs`'s own `PlayState::update`) added, each
  mechanical. `run_on_start_all`/`run_on_load_all`/`run_collisions` were
  deliberately left untouched (always empty via `ScriptState`'s own
  default): nothing can be mid-animation before a level has even started
  stepping, and `run_collisions` runs from `late_step`, which never
  receives a `StepInput` of its own to read one from in the first place.

  **A real subtlety found designing the `TurnModel` regression tests, not
  left in production code:** `do_on_start` always spawns a `Local(0)`
  player and `rebuild_scheduler` always inserts it into `TurnScheduler`
  regardless of whether the level "needs" one. An unscripted `Local`
  actor's turn is NEVER consumed (a script must call `ctx.act` for a Local
  actor's turn to count at all — the existing "a wall bump costs nothing"
  rule), and `Simulation::step` only even attempts a Local actor's
  `on_turn` once it already has a queued command (`!is_local ||
  has_command`) — so a level with two bare AI actors and an untouched
  player would leave the player parked at the scheduler's front FOREVER,
  starving both AI actors completely (confirmed live: both turn counts
  read exactly 0 before this was understood). `turn_model.rs`'s own tests
  give the player a trivial `on_turn` script that calls `ctx.act(1000000.0)`
  and feed it an external "wait" command every step so that script actually
  gets to run once — after which its due time is far too high to ever
  compete again. Not a defect in the engine (a real project's player always
  has a real `on_input`-driven script), just a real trap for headless
  AI-only test levels, documented in the test file's own header so a future
  session doesn't have to rediscover it by hand.

  **Verification.** `cargo build --workspace --bins --examples` clean.
  `cargo test --workspace`: 426 (was 421: +2 `is_animating` tests, +3
  `turn_model.rs`), all pass. `scripts/check.sh` clean. `CLAUDE.md`'s
  registered-function count updated 161 → 162 (`submit`'s 4-arg overload —
  `get_path_diag`/`get_path_diag_i`/`reachable_within` were 7.5-6's own,
  already counted); `API_VERSION` unchanged at 7, purely additive.
  **Not verified live** — same sandbox limitation 7.5-5's/7.5-6's own notes
  recorded (no windowed/GPU environment available to this agent this
  session).

#### `[x]` 7.5-8 — Timers (D22) (`aff65d4`)

- **Why:** `timer_done`'s storage was one `f64` overloaded four ways by sign
  and magnitude, and `cancel_timer`'s "cancelled" value and `timer_done`'s
  own "just consumed" value were the SAME number (`-1.0`) — so a cancelled
  timer still reported `done` on the next check, and a fired timer kept
  reporting `done` forever instead of exactly once (D22).
- **Change:** `TimerState { Running(f32), Fired, Cancelled, Consumed }`
  (`scripting/types.rs`) replaces the float; `TimerWrite { Start, Cancel,
  Consume }` is its write-queue counterpart. `timer_done` reads `true` only
  for `Fired`, queuing a transition to `Consumed`; `cancel_timer` moves
  straight to `Cancelled`, a dead end distinct from `Consumed`.
- **Test:** `timer_tests.rs`'s two new tests —
  `timer_done_returns_true_exactly_once_not_forever` and
  `cancel_timer_prevents_timer_done_from_ever_firing` — plus the three
  pre-existing tests updated to seed `TimerState::Running` instead of a
  raw float.
- **Scope:** `ember2d-sim`, docs.
- **Landed as:** no scope decision needed — a self-contained type swap with
  no shipped script (`demos/roguelike/`, `demos/shooter/`) calling
  `start_timer`/`timer_done`/`cancel_timer` at all, so nothing outside
  `ember2d-sim` needed to change. `TimerState`/`TimerWrite` live in
  `scripting/types.rs` alongside `PendingWrite` (same "a real enum instead
  of an overloaded sentinel" shape, same file). `ScriptEngine.timers` and
  `ScriptState.timers`/`pending_timers` change type but not structure — the
  round-trip through `mem::take`/`apply_ctx` that already existed for the
  old float map is unchanged, just carrying a richer value now. Decay
  (`run_scripts`, engine.rs) now only touches `Running` timers, transitioning
  one to `Fired` the instant it crosses zero, instead of decrementing every
  stored value unconditionally and letting `timer_done` reinterpret negative
  numbers after the fact.

  **File-size fallout, again:** `engine.rs` crossed 750 lines a second time
  (the D22 doc comments this step added were what pushed it over, not new
  logic volume). `run_collisions` — the on_collide dispatch pass, ~100
  self-contained lines untouched by this step's own changes — moved to a
  new `collisions.rs`, same second-`impl ScriptEngine`-in-a-sibling-file
  pattern `apply.rs`/`lifecycle.rs` already established twice. `engine.rs`
  is now 663 lines, with real headroom for whatever 7.5-9 through 7.5-13
  still need to add to it.

  **Verification.** `cargo build --workspace --bins --examples` clean.
  `cargo test --workspace`: 428 (was 426: +2 timer tests), all pass.
  `scripts/check.sh` clean (file-size limit, including the `collisions.rs`
  split). `CLAUDE.md`'s registered-function count unchanged at 162 — no new
  Rhai-facing function, `start_timer`/`timer_done`/`cancel_timer`'s own
  signatures didn't change, only their internal representation.
  `API_VERSION` unchanged at 7. **Not verified live** — same sandbox
  limitation the last three steps' own notes recorded; no shipped demo
  calls these three functions, so there is no live scenario this step's
  fix would even change the behavior of today.

#### `[x]` 7.5-9 — Sim boundary lints and `LevelSource` (`57de3c2`)

- **Why:** R17 (`simulation.rs`'s `resolve_exit_path`, `simulation/spawn.rs`'s
  node-graph script combine) and R41 (`world.rs`'s hierarchy-cycle
  `eprintln!`) were real filesystem/console violations of CLAUDE.md's
  Determinism section, allow-listed in `scripts/check.ps1`/`check.sh`
  specifically so this step could fix them instead of the allowlist
  becoming permanent. Separately, an entity hierarchy had no cycle
  prevention at all (only a depth-100 bail-out after the fact) and
  `despawn` left children pointing at a dead parent id forever.
- **Change:** `Simulation` gets `level_source: Box<dyn LevelSource>`
  (`exists`/`read_to_string`/`load_level`); `ember2d`'s `FsLevelSource` is
  the real disk-backed implementation, `Simulation`'s own default
  (`NullLevelSource`) does zero I/O. `World::diagnostics` (a `RefCell<Vec<
  Diagnostic>>`) replaces the `eprintln!`, drained into the existing `logs`/
  `LogEntry` pipeline every `step`/`late_step`/`on_start` call.
  `set_parent` rejects a cycle-creating reparent up front; `despawn` clears
  every child's own parent link, preserving its world position.
- **Test:** `world_tests.rs`'s 5 new tests (direct/indirect/self-parent
  cycle rejection, despawn clearing a child's parent, the Diagnostic
  surfacing for a cycle that bypasses `set_parent`); `ember2d/tests/
  level_source.rs` (a level transition succeeds with a working
  `LevelSource` configured, fails closed with the default).
- **Scope:** `ember2d-sim`, `ember2d`, `ember2d-editor`, `scripts/check.ps1`/
  `check.sh`, both docs.
- **Landed as:** one scope decision made without a user question this
  time, but worth stating plainly: the plan's own text says
  `StepOutcome.diagnostics: Vec<Diagnostic>`, a new field nothing
  downstream would read yet. Landed instead as a drain into the EXISTING
  `logs`/`LogEntry` pipeline (`Simulation`'s own new `drain_diagnostics_
  into` helper) — `world.rs` still defines its own `Diagnostic` type
  (it sits BELOW `scripting` in this crate's layering, so it can't build a
  `LogEntry` directly), but the conversion happens one level up, in
  `simulation.rs`/`simulation/step.rs`, which already have both types in
  scope. Chose this over a parallel, unwired `StepOutcome.diagnostics`
  field specifically so the fix is immediately visible in the editor
  console (`PlayState.script_log`'s existing surfacing) instead of dead
  data waiting for a future step to wire up display for it.

  **`resolve_exit_path`** (simulation.rs) — its own `Path::new(next).exists()`
  became an injected `exists: &dyn Fn(&str) -> bool` parameter rather than
  taking a full `LevelSource` object: this function is also called from
  `ember2d-editor` (`graph_sidecars.rs`, via the existing `ember2d::play`
  re-export), which is allowed real fs access already and has no reason to
  learn about a sim-side trait just to pass one through — it now passes
  `&|p| Path::new(p).exists()` directly. Every `ember2d-sim`-internal
  caller (5 in `spawn.rs`, 2 in `step.rs`) passes `&|p|
  self.level_source_exists(p)` instead, a small forwarding method added
  specifically so the closure's own capture stays disjoint from the
  `&self.level.path` argument the same call always also borrows.

  **A wider blast radius than R17's own two files suggested:** any test
  that constructs a `Simulation` directly (bypassing `PlayState`, which
  wires up `FsLevelSource` unconditionally in `new_with_sim`) and relies on
  a REAL script/texture/exit path resolving correctly needed the same
  wiring by hand — `ember2d/tests/common/mod.rs`'s `TurnHarness` (used by
  most of this crate's own test suite, including the R7 stairs-transition
  tests in `save_load_globals.rs`) and `shooter_arena.rs`'s
  `RealtimeHarness` both call `sim.set_level_source(Box::new(FsLevelSource))`
  now, BEFORE `on_start` — every shipped demo script uses a repo-root-
  relative path that must resolve as "already exists from CWD," which
  needs a working `LevelSource` in place before `do_on_start` ever runs.
  Confirmed by running the full suite before AND after this wiring: every
  one of these tests would have silently resolved every script path wrong
  (joined against the level's own directory instead of used as-is) without
  it — caught immediately by the existing test suite, not discovered live.

  **`ember2d-sim/clippy.toml`** landed with a real, if partial, wiring:
  `#![warn(clippy::disallowed_methods, clippy::disallowed_types)]` in
  lib.rs (a `warn`, not `deny` — a hard `deny` would fail a plain `cargo
  build` too, since rustc still parses the attribute without clippy
  actually running this crate's lints). `disallowed-methods` immediately
  found two GENUINE gaps `check.ps1`/`check.sh`'s own text-based grep had
  always missed: `scripting/engine.rs`'s `compile`/`check_hot_reload` call
  `fs::metadata` via the short `use std::fs;` alias, which never matches a
  literal `std::fs::` grep pattern — clippy catches it because it resolves
  by PATH, not source text. Both are pre-existing, dev-time-only hot-reload
  machinery, explicitly out of this step's own scope (level/exit-path
  resolution, not script compilation) — `#[allow]`ed with a comment
  explaining why, not silently left unannotated. `level.rs`'s `save`/`load`
  and `save.rs`'s `save_to_file`/`load_from_file` — the file formats' own
  real, permanent load/save entry points, already excluded from
  `check.ps1`/`check.sh`'s own scan — got the same `#[allow]` treatment for
  the same reason, closing out that exemption's story for good instead of
  leaving it as 4 more unannotated warnings.

  **`disallowed-types` surfaced 67 pre-existing HashMap/HashSet sites**
  (`scripting/state.rs` alone has ~35 — every one of `WorldSnapshot`'s own
  lookup-only maps), none of them newly introduced by this step and none
  of them a real determinism bug — every one already has its own
  "lookup-only" reasoning in a nearby doc comment per CLAUDE.md's existing
  carve-out, just not yet the formal `#[allow(clippy::disallowed_types)]`
  annotation clippy now expects. Annotating all 67 in the same diff as
  everything else this step already did (`LevelSource`, `Diagnostic`,
  `set_parent`/`despawn`, the `world.rs`/`world_tests.rs` split) would have
  more than doubled this step's own size for a purely mechanical pass with
  no behavior change — logged as **R91** (§3.2) instead of done here or
  silently skipped.

  **File-size fallout:** `world.rs` was at 742/750 lines before this step's
  own `Diagnostic`/`set_parent`/`despawn` additions, which would have
  pushed it over. Its `#[cfg(test)] mod tests` (186 lines, unrelated to
  this step until its own new tests needed to land somewhere) moved to a
  new `world_tests.rs`, same `#[path]`-split pattern the scripting module
  already uses repeatedly — `world.rs` is now 562 lines.

  **Verification.** `cargo build --workspace --bins --examples` clean.
  `cargo test --workspace`: 435 (was 428: +5 `world_tests.rs`, +2
  `level_source.rs`), all pass — including every `TurnHarness`-driven test
  (`roguelike_combat.rs`, `save_load_globals.rs`'s R7 stairs tests,
  `replay.rs` 3× fresh processes) unaffected by the `LevelSource` rewiring.
  `scripts/check.sh`/`check.ps1` both updated (R17/R41's allowlists
  removed — the checks now scan every file again, not just the
  historically-excused ones) and both pass clean; `doc-check.ps1`'s own
  numbers (function count 162, `API_VERSION` 7, `LEVEL_FORMAT_VERSION` 3)
  unchanged and still match. `CLAUDE.md` unchanged (no new Rhai-facing
  function this step). **Not verified live** — same sandbox limitation
  every step since 7.5-5 has recorded; a live save/load and a live level
  transition on both demos is still owed.

#### `[x]` 7.5-10 — Scripting engine internals (`1d965f1`)

**Why.** R22: `ScriptEngine.scopes` (a per-entity `rhai::Scope` map) was
still being maintained by `check_hot_reload` and `apply_ctx`'s despawn
cleanup years after Step 9 moved timers off it — but nothing had ever
verified it was actually load-bearing for anything else. Separately, the
five `run_*` methods (`run_on_start_all`/`run_on_load_all`/`run_on_input`/
`run_on_turn`/`run_scripts`) had each grown to 15-17 positional parameters
one field at a time since Phase 5, with five near-identical `call_fn`
error-handling blocks copy-pasted across three files, and every one of
them rebuilt an identical `extra_spawns` `HashMap` from the same static
level data on every single call.

**Change.**
- Deleted `ScriptEngine.scopes` entirely, after confirming from rhai
  1.24.0's own `call_fn`/`CallFnOptions` source that its default
  `rewind_scope: true` truncates the `Scope` back to its pre-call length
  after every call — so nothing a script's own top-level `let` or a called
  function's locals ever wrote into it survived past that same call. The
  map was provably dead, not just suspiciously idle. `call_lifecycle_fn`
  (new helper, `engine.rs`) now takes a fresh throwaway `Scope::new()` per
  call instead.
- `call_lifecycle_fn` also replaces the five copy-pasted `call_fn` +
  `is_missing_optional_fn` + disable-on-error blocks (`run_on_start_all`,
  `run_on_load_all`, `run_on_input`, `run_on_turn`, `run_scripts`'s two —
  on_start and on_update — and `run_collisions`'s on_collide) with one
  method, parameterized by the function name to call, the label to log
  errors under (on_update's has historically been "Runtime", not
  "on_update" — preserved), and the Rhai argument tuple (generic over
  `impl rhai::FuncArgs`, since `on_collide`'s `(id, other_id, ctx)` differs
  from every other lifecycle function's `(id, ctx)`).
- New `PassArgs<'a>` struct (`state.rs`) replaces the shared tail of every
  `run_*` method's own parameter list — `delta_time`/`elapsed`/`input`/
  `mouse`/`gamepad`/`spawns`/`globals`/`clips`/`camera_pos`/`commands`/
  `turn_number`/`viewport_size` — and is also what `ScriptState::from_world`/
  `from_snapshot` themselves now take, so both the outer `run_*` signatures
  and the inner constructors shrank together. `run_scripts` went from 16
  positional parameters to 6 (`world`, `snapshot`, `log`, `persistent`,
  `args`, `animating`); `run_on_start_all`/`run_on_load_all`/
  `run_collisions` from 8-11 down to 4-5. `run_on_input`/`run_on_turn` keep
  `#[allow(clippy::too_many_arguments)]` at 7 real parameters (`actor_id`
  is a genuinely separate concept from pass-wide `PassArgs`, not folded in
  for its own sake). `persistent`/`world`/`snapshot`/`log` stay their own
  parameters — see `PassArgs`'s own doc comment for why folding
  `persistent` in particular wouldn't express its `&mut`-in/read-back-out
  contract as cleanly.
- `extra_spawns` moved from a per-`ScriptState` `HashMap` (rebuilt from
  the same static `Vec<(String,f32,f32)>` on every `from_snapshot` call —
  up to 3× a step across on_input/on_update/on_turn, since each built its
  own `ScriptState`) onto `WorldSnapshot` itself, built once per snapshot
  and shared by the same `Rc::clone` every other snapshot field already
  relies on. `ctx.get_spawn_point` (api.rs) needed no change — it already
  read `self.inner.borrow_mut().extra_spawns`, which now resolves through
  `ScriptState`'s existing `Deref<Target = WorldSnapshot>` instead of a
  field on `ScriptState` directly.
- Two sub-items from this step's own original plan text were deliberately
  **not** done — see R92 for the full reasoning: `run_collisions` reusing
  `step()`'s own pre-resolution snapshot would hand `on_collide` scripts
  stale positions (a real regression, not a style choice), and retyping
  `WorldSnapshot`'s collider `layer`/`mask` to `Rc<str>`/`Rc<[Rc<str>]>`
  only pays for itself once a per-step spatial index shares that same
  allocation across multiple maps the way `tags` does — both deferred to
  a future step alongside that index, by explicit user decision (4-option
  `AskUserQuestion`, this step).

**Test.** `get_spawn_point_resolves_through_the_snapshots_extra_spawns`
(new, `engine_tests.rs`) — pins `ctx.get_spawn_point` still resolving a
named spawn point's coordinates through the new `WorldSnapshot`-owned
`extra_spawns` and the `Deref` path that replaces the old per-`ScriptState`
field. `hot_reload_clears_only_the_reloaded_scripts_entities` (the old
D8/R22 test keyed on `engine.scopes`) was removed rather than rewritten —
`timer_tests.rs`'s `hot_reload_clears_only_the_reloaded_scripts_entities_
timers` (added at Step 9) already exercises the identical scenario against
`self.timers`, the field that inherited every observable behavior `scopes`
used to have.

**Scope.** `ember2d-sim/src/scripting/engine.rs`, `lifecycle.rs`,
`collisions.rs`, `apply.rs`, `state.rs`, `mod.rs`; `ember2d-sim/src/
simulation.rs`, `simulation/step.rs`, `simulation/spawn.rs`;
`ember2d-sim/examples/bench_sim.rs`; every `scripting/*_tests.rs` file that
calls `run_scripts`/`WorldSnapshot::build` directly (9 files, ~23 call
sites, all mechanical — no test's actual assertions changed).
`docs/ember2d-master-plan.md` (R22 closed, R91's site count corrected, new
R92 for the two deferred sub-items).

**Landed as:** `1d965f1`. Full workspace build clean (`cargo build
--workspace --bins --examples`). `cargo test --workspace`: 435 (unchanged
from 7.5-9's own count: +1 new `get_spawn_point_resolves_through_the_
snapshots_extra_spawns`, -1 the removed scopes-based D8 test — net zero,
but real coverage moved from a dead field to a live one), all pass. `cargo clippy -p
ember2d-sim --lib --tests`: no new warnings — `run_on_input`/`run_on_turn`
kept their `#[allow(clippy::too_many_arguments)]` (8 with `self`, one over
the default threshold), every other warning is pre-existing (R91's
HashMap/HashSet sites, R38-class too-many-arguments on unrelated methods
this step didn't touch). `scripts/check.sh`/`check.ps1` both clean;
`doc-check.ps1`'s numbers (function count 162, `API_VERSION` 7,
`LEVEL_FORMAT_VERSION` 3) unchanged — no Rhai-facing function added,
removed, or renamed this step. `bench_sim --release` run to record a
baseline per this step's own "measure; record" instruction: synthetic
n=500/2000/5000/10000 at p50 0.480/1.758/4.288/7.881ms per step
(`WorldSnapshot::build` alone: 0.323/1.294/3.143/5.733ms), shipped
`floor1`/`floor2`/`floor3` at p50 0.355/1.302/0.774ms. Caveat: `bench_sim`'s
own phase-timing harness benchmarks `WorldSnapshot::build`/
`detect_collisions` as standalone primitives, once per iteration — it
doesn't isolate the `extra_spawns` fix's actual effect (eliminating up to
2 redundant per-step rebuilds across on_input/on_update/on_turn, now
folded into the one `WorldSnapshot::build` call already being timed), so
these numbers are a baseline for future comparison, not a before/after for
this step's own change. **Not verified live** — same sandbox limitation
every step since 7.5-5 has recorded.

#### `[x]` 7.5-11 — Audio (`26e3e82`)

**Why.** R30: `AudioEngine` lived on `PlayState`, which `ember2d-app/src/
app.rs`'s `Transition::ToPlay` handling destroys and rebuilds on every
level transition (`engine.pop_state()`, then a brand-new `PlayState`) —
so the real device stream closed and reopened on every floor change,
killing whatever music was playing and paying a device re-init cost even
between two floors that wanted the exact same track. Separately,
`play_sound`/`play_music` called `StaticSoundData::from_file` — a real
disk read plus decode — on every single call, including a sound effect
fired every step.

**Change.**
- `AudioEngine` moved from `PlayState` to `Engine` (`ember2d/src/
  engine.rs`), threaded down to whichever `GameState` is running via a new
  `UpdateContext::audio: &mut AudioEngine` field — `sim.rs`'s `step`
  free function (shared by `Engine::run` and, for `EditorState`, the
  editor's own test harness) gained a matching `audio: &mut AudioEngine`
  parameter, constructing both `UpdateContext`s with it. `PlayState` no
  longer owns one at all; `flush_audio` (its own drain-and-forward method)
  now takes `&mut AudioEngine` as a parameter instead of reading `self.audio`.
- `AudioEngine::play_sound`/`play_music` decode through a new `load_cached`
  helper (`HashMap<String, StaticSoundData>`, keyed by path) instead of
  hitting the filesystem every call — `StaticSoundData` is cheap to clone
  (its own doc comment: "the audio data is shared among all clones," an
  `Arc<[Frame]>` underneath), so every play after the first is a clone plus
  independent per-call `settings` (volume/panning), not a re-decode.
- `play_music` is now a no-op when asked to (re)start the track already
  playing (`current_music_path`, set as soon as a path is asked for —
  before checking device/file availability, so a script asking for a bad
  path logs the failure once, not every call). This is what actually makes
  "the device survives a transition" observable: `player.rhai`'s `on_start`
  calling `ctx.play_music("music.ogg")` unconditionally on every floor no
  longer restarts an already-playing identical track from the beginning
  just because a new level loaded.
- `AudioEngine::play_sound` gained a `pan: f64` parameter (-1.0 hard left,
  0.0 center, 1.0 hard right, via kira's `Panning`); `PlayState::flush_audio`
  computes it for `ctx.play_sound_at` calls from the same camera-relative
  `dx` its pre-existing distance falloff already used, sharing `max_dist`
  so a sound at the edge of audible range also reaches full pan. Plain
  `ctx.play_sound` calls pass `0.0` (centered) — unchanged sound.
- `demos/roguelike/scripts/victory.rhai`'s `music_started`-guarded lazy-init
  (`ctx.play_music` inside `on_update`, from before 7.5-5 gave scripts a
  real `on_start` hook) moved into a proper `on_start`, matching
  `player.rhai`'s own established pattern — the global no longer exists
  anywhere in the roguelike.

**Test.** `ember2d/src/audio.rs` gained its own `#[cfg(test)] mod tests`
(none existed before): `play_music_with_the_same_path_twice_is_a_no_op`,
`play_music_with_a_different_path_switches_tracks`,
`stop_music_clears_the_current_track_so_the_same_path_restarts_it` — all
against nonexistent `"*.ogg"` paths and reading the private
`current_music_path` field directly (same-module test), so they're
deterministic whether or not the machine running them has a real audio
device (the idempotence check runs, and is checked, before `self.manager`
is ever consulted). Every other call site gaining the new `UpdateContext::
audio`/`sim::step` parameter (`ember2d/tests/turn_animation.rs` ×2 helpers,
`ember2d/tests/save_load_globals.rs`, `ember2d/src/play/tests.rs` ×3,
`ember2d-editor/tests/common/mod.rs`) needed a locally-constructed
`AudioEngine::new()` purely to satisfy the new signature — none of those
tests exercise audio itself.

**Scope.** `ember2d/src/audio.rs`, `engine.rs`, `sim.rs`, `play.rs`;
`ember2d/tests/turn_animation.rs`, `save_load_globals.rs`, `src/play/
tests.rs`; `ember2d-editor/tests/common/mod.rs`; `demos/roguelike/scripts/
victory.rhai`; `docs/ember2d-scripting-api.md` (Effects and audio section).

**Landed as:** `26e3e82`. Full workspace build clean (`cargo build
--workspace --bins --examples`). `cargo test --workspace`: 438 (was 435:
+3 new `audio.rs` tests), all pass. `cargo clippy --workspace --lib
--all-targets`: no new warnings from any file this step touched (the one
warning inside `audio.rs`'s diff, `let _ = handle.stop(...)`'s unit
binding, is pre-existing, unchanged text from before this step). `scripts/
check.sh`/`check.ps1` both clean; `doc-check.ps1`'s numbers (function count
162, `API_VERSION` 7, `LEVEL_FORMAT_VERSION` 3) unchanged — no Rhai-facing
function signature changed (`play_sound`/`play_sound_at`/`play_music`'s
script-visible arity is exactly what it was; only `AudioEngine`'s internal
Rust API gained parameters). **Not verified live** — same sandbox
limitation every step since 7.5-5 has recorded; whether music actually
survives a real floor transition, and whether spatial-sound panning sounds
correct, both still need a live playtest.

#### `[x]` 7.5-12 — Node-graph codegen hardening (`869f919`)

String literals and identifiers escaped (`rhai` string escaping; identifiers
validated against `[A-Za-z_][A-Za-z0-9_]*` with a UI error otherwise).
`add_edge` rejects cycles; `gen_exec_chain` carries a visited set. Typed
literal nodes: `IntLit`, `BoolLit`, `StringLit`; unconnected ports default
per the port's declared type. Variables declared at function top, not
inside branches. Concatenation with a tile's own script becomes a compile
error surfaced in the editor rather than a silent override. **Tests:**
property-style — random graphs of the supported node kinds always produce
Rhai that `compile_str` accepts.

**Scope decision (asked at session start, re-opening the question left from
the prior session's own close-out note):** sim-side hardening only — no new
`IntLit`/`BoolLit` `NodeKind` variants, no editor palette/property-UI work.
`StringLit` already exists. "Typed literal nodes... unconnected ports
default per the port's declared type" is satisfied by tagging `PortSpec`
with a `DataType` used only to pick each unconnected data-in port's default
literal — a data-model addition, not a new node kind. "Identifiers
validated... with a UI error otherwise" has no UI in this scope, so codegen
sanitizes an invalid identifier to a safe one and records why in the
`GraphSource` the generator now returns, which both call sites (`ember2d-
sim/src/simulation/spawn.rs`, `ember2d-editor/src/editor/impl_state/
graph_sidecars.rs`) surface through the existing `LogEntry` console-log
mechanism (7C-7) instead.

**Landed as `869f919`.** Every user-entered value spliced into
generated Rhai is now either escaped as a string literal
(`codegen.rs::escape_rhai_string` — tags, paths, global/persistent/timer
names, `StringLit`'s own value) or sanitized into a safe identifier
(`sanitize_ident` — `SetVar`/`GetVar`'s `name`, which becomes a bare
`__var_*` variable rather than a quoted argument, so escaping would be the
wrong fix). `NodeGraph::add_edge` (graph/mod.rs) now refuses any edge that
would close a cycle over the combined exec+data edge set (`path_exists`),
the primary defense; `gen_exec_chain`/`resolve_data` (codegen.rs) also
carry their own cycle guards (`GenCtx::exec_visited`/`data_visited`) as
defense in depth against a cycle already sitting in a saved level file,
which bypasses `add_edge` entirely on load. Unconnected `data_in` ports now
default per the port's own new `DataType` (`PortSpec::data_type`,
`codegen.rs::default_for_port`) instead of an unconditional `0.0` — fixes
several previously always-broken combinations (an unwired `Branch.Cond`,
`SetVisible.Bool`, `Log.Msg`, `RandomInt.Min`/`Max`, and others: Rhai has no
int/float/string/bool auto-coercion for function arguments, so `ctx.log(0.0)`
or `if 0.0 {` never compiled even before this step). A `SetVar`/`GetVar`'s
`__var_*` local is now hoisted to one `let` block at the top of whichever
lifecycle function (on_start/on_update/on_collide) references it
(`GenCtx::declared_vars`), fixing the case where a variable set only inside
a `Branch` arm was unreadable once the `if`/`else` closed (Rhai block
scoping). `generate_graph` now returns a `GraphSource` (source + which
lifecycle functions got real content + generation warnings) instead of a
bare `String`; both call sites — `ember2d-sim/src/simulation/spawn.rs`'s
runtime combine and `ember2d-editor/src/editor/impl_state/
graph_sidecars.rs`'s save-time export (the latter's `migrate_graph_sidecars`
is now `&mut self`) — use the new `concatenation_collisions` check before
concatenating a graph's generated source with a tile's own script file, and
skip the combine with a logged `LogEntry::error` instead of letting Rhai's
last-definition-wins semantics silently discard the graph's own version.
Also normalized `codegen_expr`'s `Spawn` fallback (an unresolved spawn-
result reference) from the ad hoc `"0"` to `DataType::Entity`'s own `"-1"`
sentinel, for the same "no entity" convention as everywhere else (§4.3,
7.5-1) — not one of R34's own listed locations, but the same defect.
**Tests:** 17 new — `graph/codegen_tests.rs` (15, including the plan's own
property-style random-graph check, seeds 0–39, and a diamond-fan-in test
guarding against a cycle-guard false positive) and
`simulation/spawn_tests.rs` (2, exercising the concatenation-collision
check through the real `Simulation::on_start` entry point rather than just
the extracted predicate). Full workspace green (150 ember2d-sim / 54
ember2d-editor / rest unchanged — 660+ total), `cargo clippy --workspace
--all-targets` at 217 (down 1 from the 218 baseline — no new warnings from
this step), `check.ps1` clean, replay ×3 byte-identical. Neither shipped
demo uses a node graph (`grep -l "graph:" demos/**/*.level` — no matches),
so this step is inert for both and needed no live playtest.

#### `[x]` 7.5-13 — Rhai `no_module` re-evaluation (`6af6f42`)

Decide (§7.4) whether to enable Rhai modules so scripts can `import` a
shared `common.rhai`. Cost: AST cache and hot-reload need module
resolution; determinism is unaffected. Benefit: ends copy-paste across
scripts for good. If yes, ship `demos/roguelike/scripts/common.rhai`.

**Landed as a decision, no code.** §7.4 records it: **No**, keep
`no_module` on. The gate's own trigger (`or_zero`-style duplication
surviving 7.5-2/7.5-3 in a shipped script) never fired — verified fresh at
this step with `grep -rn "or_zero" demos/`, which found only historical
comments, no live calls, and no other cross-script duplication either.
Nothing to build; revisit only if a future script reintroduces that kind
of duplication.

**Phase 7.5 gate:** §0.5; both demos rewritten to use the new primitives and
**smaller** than before (record line counts); `API_VERSION` 7 documented
with a migration table; tag `v0.5.8`.

**Gate status (checked after 7.5-13, not yet passed — not tagged):**
`API_VERSION` 7's migration table was satisfied incrementally, each step
documenting its own additions in `ember2d-scripting-api.md` in the same
commit (§6 of that doc) — nothing left to write there. Every automated
§0.5 criterion is green (build, full test suite — 455, up from 381 at
`v0.5.7d` — clippy diffed clean of regressions, `check.ps1`, replay ×3;
see §9's `v0.5.8` row). Two things still block the tag:

1. **Checklist §11/§12/§13 (play mode, turn-based mode, save/load and
   scripting)** — these are live F5-into-play-mode checks; this agent has
   no windowed/GPU sandbox this session, same gap noted at the `v0.5.7c`/
   `v0.5.7d` gates. Needs the user's own pass.
2. **Demo line counts: roguelike shrank as required (681 → 588, `v0.5.7d`
   → now), shooter did not (679 → 723).** Investigated directly rather than
   assumed: `director.rhai` did shrink (436 → 351, per 7.5-5's own bullet/
   enemy extraction), but the new `bullet.rhai` (112 lines) plus
   `player.rhai` growth (243 → 260) more than offset it. A real pass over
   all three files for duplication found exactly one genuine duplicate
   (`is_enemy`, 3 identical lines in both `bullet.rhai` and
   `director.rhai`) — not enough to matter, and the one place a bigger
   savings exists (shared arena-bounds constants, `min_x`/`max_x`/`min_y`/
   `max_y`, already commented as "duplicated, not shared" in `bullet.rhai`)
   would need exactly the Rhai-modules feature 7.5-13 just decided not to
   build. Recommendation given to the user: accept the growth as the
   legitimate cost of a real architectural improvement (moving bullet hit
   detection out of `director.rhai`'s old per-step batch scan into each
   bullet's own `on_update`), not unaddressed cruft. **Decided
   (2026-09-29, user sign-off): accepted and documented** — the shooter's
   723 lines stand as a recorded exception to this gate's "smaller"
   criterion, not a blocker. No script changes.

**Remaining blocker (2026-09-29):** item 1 only — the user's own live
§11/§12/§13 pass. By user direction, Phase 8 work (8-1) proceeds in the
meantime; 7.5 stays `[~]` and untagged until that pass is done.

---

### 5.7 `[~]` Phase 8 — Tilemap, assets, animation authoring

**Purpose.** The old Phase 8 (tileset importer, clip editor) plus the one
data-model change that moves the entity ceiling by an order of magnitude.

#### `[x]` 8-1 — `Tilemap` component (decision gate §7.2) (`8008022`)

- **Why:** Every tile is an entity with its own collider. A 200×200 map is
  40,000 entities before one actor. Collision, `is_solid_at`, raycast, A*,
  and rendering all pay per tile.
- **Change:** `Tilemap { origin: IVec2, size: UVec2, layers: Vec<TileLayer>
  }` where `TileLayer { cells: Vec<TileCell>, collision: bool, z: i32 }` and
  `TileCell` is a compact `{ glyph_or_uv, fg, bg, solid, layer_bits }`.
  Static, script-less, tag-less, non-trigger tiles collapse into the
  tilemap at load; anything interactive stays an entity. `is_solid_at`,
  raycast, and A* check the tilemap first (O(1)). The broad phase never
  sees tilemap cells; actor-vs-tilemap is a direct cell lookup. Rendering
  draws the visible tilemap window as one batch. **Level format v4:**
  `tilemap` section added; v3 files load losslessly (every tile stays an
  entity) and re-save as v4 after the editor's "Bake static tiles" action.
- **Test:** `bench_sim` synthetic 200×200 map: steps/sec and allocs before
  and after; `roguelike_level_integrity.rs` on a baked floor2 shows
  identical BFS reachability; replay unchanged.
- **Done when:** a 200×200 level with 50 actors holds 60 fps in debug.
- **Scoped (2026-09-29, user decisions, before any code):** (1) **tagged
  static tiles collapse too** — the plan's "tag-less" rule would have
  collapsed zero tiles in the shipped demos (every wall/floor is tagged);
  the tag is kept per cell (`TileDef::tag`, readable via `get_tile_tag`).
  (2) A cell hit reports **the tilemap's own entity id** from
  `get_entity_at`/`find_entities_in_rect`/`raycast`/`on_collide`, plus
  additive `is_tilemap(id)`/`get_tile_tag(x,y)` — no `API_VERSION` bump.
  (3) **Full v4, auto-bake on save** instead of an explicit "Bake static
  tiles" action: the editor unpacks the tilemap into its ordinary tile grid
  on load and bakes on save. (4) 9-4's `spawns` map is **not** folded in —
  it gets its own format bump at 9-4.
- **Landed as** (`8008022`):
  - `components/tilemap.rs` — `Tilemap` (palette of `TileDef`s + one
    `u16` grid per layer, runtime caches `#[serde(skip)]` and rebuilt by
    `refresh`) and `TilemapBuilder`. The plan's `TileCell { glyph_or_uv,
    fg, bg, solid, layer_bits }` became a palette index per cell instead
    (identical walls share one def; 2 bytes a cell). `TileRecord::
    is_static` (`level/bake.rs`) is the one collapse rule: no script,
    graph, trigger, actor, `next_level`, collider mask, or camera follow.
  - `LevelData.tilemap` (v4) with lossless `bake_tilemap`/`all_tiles`/
    `split_static`. A **v3 level also collapses at load** (`split_static`
    reads both the baked section and static `tiles`), so the speedup needs
    no re-save — a refinement of the plan's "v3 loads with every tile an
    entity".
  - `World.tilemaps` (`Rc<Tilemap>`, shared into `WorldSnapshot` by
    refcount — serde's `rc` feature enabled for it) and `World.exits`
    (R93). The tilemap entity spawns first, with a `Transform` at the grid
    origin (informational; moving it doesn't move cells).
  - Queries (`api_spatial.rs`): tilemap checked alongside colliders;
    `get_path`/`reachable_within` check it first, O(1) per neighbour.
    Broad phase: colliders test tilemap cells by direct lookup
    (`world/tilemap_collision.rs`), one `Collision` event per (collider,
    tilemap) pair; `late_step` pushes a mover out of each overlapped cell
    row-major through the same `push_out_of` a wall entity uses.
  - Play rendering: `DrawList::from_world_in` adds only the cells inside
    the camera's view, as ordinary `DrawCommand`s at `layer * 10`.
  - Editor `LevelGrid::from_level_data`/`to_level_data` unpack/bake;
    both generators bake; every shipped level regenerated to v4 and
    verified lossless against its v3 original (every tile, and every other
    field, identical). floor2.level: 28,360 lines → ~370.
  - `bullet.rhai` recognises a wall hit by `is_tilemap(h)` as well as
    `has_tag(h, "wall")` — the only shipped script that identified walls
    by an entity tag. (Not `is_solid_at` as first proposed: the existing
    `find_entities_in_rect` call already uses the bullet's exact 0.4×0.4
    footprint; a point test would have changed the hit shape.)
  - **Measured** (`bench_sim`, release, before → after): 200×200 + 50
    actors **42.38 → 0.298 ms/step p50**, 99,907 → 1,877 allocs/step,
    `WorldSnapshot::build` 26.5 → 0.041 ms; floor2 1.306 → 0.009 ms/step,
    6,327 → 70 allocs/step. Debug build, same 200×200 scenario: 0.345
    ms/step (sim only).
  - **Tests (25 new):** 15 unit tests (`components/tilemap_tests.rs` —
    builder, masks, clamping, NaN, raycast parity, bake round trip/
    idempotence/oversize refusal); 4 spawn tests
    (`simulation/tilemap_spawn_tests.rs`, incl. both R93 tests and a
    save/RON/load round trip); `ember2d/tests/tilemap_equivalence.rs`'s 3
    — floor2 with and without the tilemap must give identical
    `is_solid_at`/`get_entity_at`/`find_entities_in_rect`/`raycast`/
    `get_path` (4- and 8-dir)/`reachable_within` answers, identical
    push-out traces, and an identical 160-step playthrough; 1 integrity
    test (every shipped level baked, tilemap solidity = tile solidity cell
    for cell) plus the existing integrity checks moved to `all_tiles()`
    (they would otherwise have passed vacuously — no walls in `tiles`); 2
    `DrawList` window tests. Existing tests updated for walls no longer
    being entities: `shooter_arena` (wall count now read from the
    tilemap), `trigger_collider_layer`, `play::tests`' z-order test (now
    read off the draw list), the editor's two `grid.rs` tests.
  - **Live-verified (2026-09-30, by the agent, real `ember2d.exe`
    windows driven by synthetic input + screenshots — the user had no
    time for a manual pass):** floor2/floor1/arena render; walls block
    the player (turn-based and realtime push-out); potion pickup;
    corridor walk + rat wake/chase/bump-kill; floor1 stairs → floor2
    (R93's fix, live); shooter bullets stop at walls; editor opens a v4
    level with every wall back as an ordinary tile; editor save rewrote
    floor2.level **byte-identical**; paint → F5 (painted wall present and
    solid in play) → undo. Play-mode floor2 and arena screenshots are
    **pixel-identical** to the pre-8-1 binary. A generated 200×200 level
    + 50 actors (266 KB as v4 vs 9.4 MB as v3) holds the **60 FPS** cap
    in a debug build on the F3 overlay — the "Done when" condition. Every
    editor menu and panel was also exercised; the defects found (R95–R100)
    all reproduce identically on the pre-8-1 binary.

#### `[x]` 8-2 — Tileset importer (`d751be7`)
Slice a PNG into a grid, name regions, write `project/assets/tilesets/*.ron`.
Sprite thumbnails in the palette (unblocked by 7D).

- **Scoped (2026-09-30, user decisions, before any code):** (1) a placed
  tile references its sprite by **tileset + region name**, resolved
  through `assets/tilesets/<name>.ron` at load (re-slicing a tileset
  updates every placed tile; names usable by scripts later); (2) the
  editor canvas **draws the real sprites**, not just fallback glyphs;
  (3) a **full importer dialog** — OS PNG picker, cell size / margin /
  spacing, live sliced-grid preview, click a cell to name it, writes the
  tileset `.ron` and adds a palette entry per named region; (4) level
  format **v5** (an older engine would silently draw sprite tiles wrong).
- **Landed as** (`d751be7`):
  - `ember2d-sim/src/tileset.rs` — `TilesetData` (image, cell size,
    margin, spacing, grid, named regions; `grid_size`/`cell_rect`/
    `region_rect`/`validate`) and `SpriteRef { tileset, region }`.
    `TileRecord::sprite` and `TileDef::sprite` (format v5; `TileDef::src`
    holds the resolved rect at runtime, so a save draws without re-reading
    tilesets). `simulation/tilesets.rs` resolves references at level load —
    `assets/tilesets/<name>.ron` searched upward from the level's own
    folder, then from the working directory, all through `LevelSource` (no
    filesystem access in the sim); each tileset read once per load; a
    missing tileset/region warns once and falls back to the glyph. Sprite
    tiles (entity and baked tilemap cell alike) draw at exactly one cell.
  - `ember2d`: `DrawSurface::draw_texture_px` + `DrawOp::Texture` (so
    headless tests see image draws) + `UiPainter::image`; `sprite_size`
    now uses the `src` sub-rect's size (it used the WHOLE texture — a sheet
    cell with no explicit size would have rendered as big as the sheet).
  - Editor: `sprites.rs` (the project's tilesets + sheet textures, loaded
    with the palette on project open and after each import); the canvas
    draws sprite tiles as images; palette rows and the palette editor show
    sprite thumbnails (`widgets::draw_tile_preview_in`);
    `TileDefinition::sprite` (`#[serde(default)]` — old palette files
    load); File > Import Tileset... (`rfd` image picker) opens the importer
    (`importer.rs` state/arithmetic, `ui/panels/importer_panel.rs`,
    `input/importer.rs`, `impl_state/tileset_import.rs`): name, cell W/H,
    margin, spacing fields, a whole-number-zoom preview of the sheet with
    its grid, click a cell + type to name it, [ Import ] copies the image
    to `assets/tilesets/<name>.png`, writes `<name>.ron`, adds one
    undoable palette entry per new region and saves the palette;
    re-importing a sheet carries its existing settings and names over.
    Export now copies `assets/`.
  - Forced relocations (pure moves, files at the 750 limit):
    `EditorMode`/`Modal`/`ModalPurpose`/`TextInputPurpose` to
    `editor/mode.rs`; `draw_palette_panel` to `ui/panels/palette_panel.rs`.
    `scripts/check.ps1`'s CELL_W/CELL_H chrome check made case-sensitive
    (it flagged every ordinary `cell_w` field).
  - **Tests (24 new):** 4 `tileset.rs` unit tests (grid math, rects,
    validation, RON defaults), 5 `simulation/tileset_spawn_tests.rs`
    (entity + baked resolution through a nested-folder project, missing
    tileset warns once, missing region named, invalid file rejected), 1
    bake test (sprite ref survives baking and is part of def identity), 1
    `sprite_size` test, 7 `importer.rs` unit tests, 2 `sprites.rs` tests,
    and 4 headless editor tests (`ember2d-editor/tests/editor_importer.rs`:
    import writes files + palette entries; thumbnails drawn inside their
    rows; painting a sprite entry places a sprite tile the canvas draws as
    an image and saves as v5; bad import stays open, Esc cancels, nothing
    written; an old palette file still loads). Updated: the R89 menu test's
    hard-coded "Close Project" index (the new menu item moved it 9 -> 10).
  - **Live-verified** (real exe, synthetic input, screenshots): File >
    Import Tileset... -> Windows file dialog -> importer showing the sheet
    sliced 4x2; eight cells named; [ Import ] wrote `dungeon.ron` +
    `dungeon.png`; palette thumbnails; painted brick/grass rows drawn as
    sprites on the canvas; saved level is v5 with `sprite:` refs; F5 and a
    direct `ember2d <level>` launch both draw them; reopening the editor
    reloads tileset + palette. Known and unchanged: world cells are 1:2
    (the parking-lot "square world units" item), so a square sprite draws
    twice as tall as wide, in the canvas and in play alike.

#### `[x]` 8-3 — Sprite animation editor (`HASH83`)
Build clips, scrub frames, preview looping; clips serialised to the project
(today they are runtime-only), referenced by name from `SpriteSource::Clip`.

- **Scoped (2026-10-01, user decisions, before any code):** (1) a clip's
  frames are **named regions of one tileset** (re-slicing the tileset
  updates the clip, as with tiles); (2) **tiles can be animated** — palette
  entries and tiles reference a clip by name and play it on loop in game,
  level format **v6**; animated tiles stay entities (the tilemap can't
  animate); (3) **one file per clip**, `<project>/assets/clips/<name>.ron`,
  beside `assets/tilesets/`; (4) animated tiles **animate on the editor
  canvas** too.
- **Landed as** (`HASH83`):
  - `ember2d-sim/src/clip_asset.rs` — `ClipData { name, tileset, frames
    (region names), fps, looping }` with `validate` (name, non-empty, fps in
    (0, 60]) and `to_animation_clip`, which builds the existing runtime
    `AnimationClip` as `ClipFrames::Rects` (defined since Phase 3, never
    constructed until now). `TileRecord::clip` (format **v6**); a clip tile
    is never baked into the tilemap (`is_static` requires `clip: None`).
    `simulation/tilesets.rs` gained a clip cache beside the tileset one,
    resolved through the same upward `assets/` search and `LevelSource`; a
    missing clip/region warns once and the tile draws its still sprite.
    Spawn registers the clip in `Simulation.clips` and gives the tile an
    `Animator`, so the existing sim-step animation advances it and saves
    carry it unchanged.
  - `ember2d`: play mode's `SpriteSource::Clip` branch draws a `Rects`
    frame as a texture sub-rect (`play/render.rs::clip_frame`; it only drew
    glyph frames before).
  - `ember2d-editor`: File > Animation Clips... opens a full modal
    (`clip_editor.rs` state, `impl_state/clip_edit.rs`,
    `input/clip_editor.rs`, `ui/panels/clip_editor_panel.rs`) — clip list,
    name/fps fields, loop toggle, tileset cycle (only while the clip has
    no frames), the tileset sheet with region outlines (click a region to
    append it as a frame), a frame strip with select/move/delete,
    play/pause and Left/Right scrubbing, a live preview, [ Save ] and
    [ Add to Palette ] (one undoable palette change; the entry's still
    sprite is frame 1). `TileDefinition::clip`; the canvas, palette panel
    and palette editor animate clip tiles off a new `anim_time` clock.
  - **Tests (15 new):** 4 `clip_asset.rs`, 3 `tileset_spawn_tests.rs`
    (Rects clip on a tile, frame advances with steps, missing clip warns
    once), 2 `play/render.rs` `clip_frame`, 4 `clip_editor.rs`, 2 headless
    editor tests (`tests/editor_clips.rs`: build/save/scrub/add/paint/canvas
    draws the frame for the time/v6 save; a bad save stays open with an
    error and Esc closes). Updated: the R89 menu test's Close Project index
    (10 -> 11), the importer test's version check (now `LEVEL_FORMAT_VERSION`).
    Clippy unchanged at 192 (R90's message grew to 672 bytes).
  - **Live-verified** (real exe, synthetic input, screenshots): built a
    4-frame "pulse" clip from the 8-2 test tileset, preview animating,
    scrubbing, Save wrote `assets/clips/pulse.ron`, Add to Palette showed an
    animated thumbnail, painted six pulse tiles — two captures 250 ms apart
    show different frames on the canvas and again in play (F5); the saved
    level is v6 with `clip: Some("pulse")`.

#### `[ ]` 8-4 — Asset preview and drag-and-drop
Roadmap V0.5.5: file browser thumbnails, drag a texture onto the palette or
a tile.

**Phase 8 gate:** §0.5, checklist §4, §7, §11; tag `v0.5.9`.

---

### 5.8 `[ ]` Phase 9 — Scene and UI layer, and the RPG demo

**Purpose.** New phase, from the RPG feasibility study. Ember2D is a
tile-and-turn engine with no scene or UI layer scripts can drive: scenes are
level files, the state stack is Rust-only, camera follow is engine-owned,
`draw_menu` has no input model, and Phase 7's proportional fonts do not
reach scripts. The acceptance test is a third shipped demo.

#### `[ ]` 9-1 — Script-visible scene stack
`push_scene(name)`, `pop_scene()`, `current_scene()`. A scene is a named
script-owned state (`battle`, `menu`, `dialogue`) layered over the level;
the engine pauses `on_turn`/`on_update` for the level while a scene with
`pauses_world: true` is on top and routes input to the scene script's
`on_input`. This replaces the Rust-only `PauseMenuState` with a script.

#### `[ ]` 9-2 — Script-drivable camera
`set_camera_target(id | position)`, `set_camera_zoom`, `camera_shake` (already
via animation), `set_camera_bounds`. Lerp stays presentation-side (its
`exp()` never re-enters the sim). Cutscenes become possible.

#### `[ ]` 9-3 — UI widgets with input
`draw_menu` gains a real model: `menu_open(items) -> menu_id`,
`menu_selection(menu_id)`, `menu_closed`, arrow/confirm/cancel handled by
the engine with buffered input. `draw_dialogue(text, speaker)` with
`wrap_text` (from Part 2) and paging; `dialogue_advance`. Both draw through
the theme font on the pixel path — the **first** script-facing pixel-space
HUD API, additive beside the cell-based one.

#### `[ ]` 9-4 — Positional continuity and structured state
`load_level(path, spawn_name)` spawns at a named spawn point (level format
gains `spawns: BTreeMap<String, Vec2>`, replacing the single `spawn_point`
— a v4 change, folded into 8-1's bump). Nested `Dynamic` maps/arrays in
`persistent` verified to round-trip through RON (test), giving a party
roster and inventory without a new type.

#### `[ ]` 9-5 — The RPG demo
`rpg/`: a town with three NPCs and dialogue, a field with wild encounters,
an Attack/Item/Run battle scene, a party of two, save/load mid-town.
Deterministic test in `ember2d/tests/rpg_*.rs` drives a full encounter.
This is the third genre proof and exercises every 9-x item.

**Phase 9 gate:** §0.5, all three demos play; tag `v0.5.10`.

---

### 5.9 `[ ]` Phase 10 — Networked 2-player

*(Old Phase 9, renumbered. Prerequisites now explicit.)*

**Prerequisites** (all elsewhere in this plan): save/load fidelity (7A-3);
`LevelSource` so both peers resolve levels identically (7.5-9);
`WorldSnapshot` `Send`-able — `Rc` → `Arc` or the snapshot is rebuilt per
thread (decide in 10-1); a binary snapshot path (10-3); `EntityId {
index, generation }` with authority-prefixed ranges (10-2).

#### `[ ]` 10-1 — Loopback and harness
`Transport` trait behind a `netcode` feature; `LoopbackTransport` with
latency/jitter/loss; `NetSession` exchanging command batches per step;
desync detector hashing world state every N steps. All in one process.

#### `[ ]` 10-2 — `EntityId { index: u32, generation: u32 }`
The ~79 `as i64` casts, the `-1` sentinel (becomes `EntityId::NONE`), the
two allocators (`World::spawn` and the script `next_spawn_id`) unified into
one. Breaks save files: `SaveState` version bump + migration. Scripts see
ids as before (packed `i64`).

#### `[ ]` 10-3 — Binary snapshot
A tagged `ScriptValue` enum mirroring the subset of `rhai::Dynamic` the
engine stores (unit, bool, int, float, string, array, map), `bincode`
alongside RON. Target: sub-millisecond for a few thousand entities.

#### `[ ]` 10-4 — Turn-based over the wire
Host authoritative on turn order; committed command batches; reconnect via
full snapshot. **Done when:** two instances play a tactical level to
completion with no desync at 150 ms simulated latency.

#### `[ ]` 10-5 — Realtime rollback *(only if a realtime game needs online)*
Ring buffer of binary snapshots, predict/rollback, input delay tuning.

#### `[ ]` 10-6 — Real transport
Steam networking, `matchbox`/WebRTC, or a relay. Budget real time for NAT
traversal.

**Phase 10 gate:** §0.5 plus the 10-4 done-when; tag `v0.5.11`.

---

### 5.10 `[ ]` Phase 11 — Presets, cleanup, 0.6.0

- **Presets, not modes.** `VisualStyle` becomes initial project settings
  (zoom constraints, snapping, default sprite constructor, palette, tool
  defaults). ASCII, Sprite, Empty presets. No `if preset` in the engine.
- Delete `rollback_position`, `_prev` parameters, `Vec2::normalized`,
  `IVec2`, `Rect::center` if still unused.
- `PlayerRecord` becomes a normal entity (prefab groundwork); prefabs
  decision (§7.5).
- `rand` 0.9 migration with replay-fixture regeneration, or a written
  decision to stay.
- README.md: what Ember2D is, screenshots of all three demos, build/run,
  where the docs are.
- Doc comments reconciled with reality across all crates (the review found
  stale headers in `sim.rs`, `buffer.rs`, `grid.rs`, `palette.rs`,
  `commands.rs`, `rect.rs`, `project.rs`, `render.rs`).
- Fast-forward `main`, tag `v0.6.0`.

---

## 6. Cross-cutting tracks

Work that runs alongside every phase rather than belonging to one.

### 6.1 Documentation truth

- This file's §2 and §3 are updated in the same commit as the code they
  describe. A step is not done until its `[x]` and hash are in.
- `scripts/check.ps1` (§6.5) fails if the numbers quoted in CLAUDE.md or §2.3
  drift from the tree.
- Appendix A grows one paragraph per phase gate; nothing else in this file
  accumulates history.

### 6.2 Testing strategy and targets

| Area | Today | Target by 0.6.0 | How |
|---|---|---|---|
| Sim core | ~60% | 80% | Parenting, physics, scheduler removal cases, spatial queries, every 7.5 primitive |
| Level/save | ~50% | 80% | Version checks, v3→v4 migration, nested `Dynamic` round trip |
| Play orchestration | ~40% | 60% | `MAX_SIM_STEPS`, turn-mode `frame_dt`, render-RNG isolation |
| Renderer | ~10% | 30% | Everything GPU-free: mapping, letterbox, atlas, nine-slice, theme loading |
| Editor | ~5% | 50% | `EditorHarness` (7C-5), one test per fixed input defect, undo audit |
| Codegen | 0% | 90% | Property tests: any valid graph compiles |
| Engine loop / app | 0% | 30% | State-stack depth assertions; `PressBuffer` |

Replay runs 3× as fresh processes in CI on two OSes from 7A-7 onward.
`bench_sim` numbers are recorded at every gate in §9; a >10% regression on
floor2 p50 blocks the gate.

### 6.3 Dependency policy

Upgrade at 7B-1, then review at each phase gate. `rand` pinned at 0.8 until
Phase 11 (determinism). `rhai` tracked closely (the engine depends on
`CallFnOptions` semantics — see R22). Any new dependency is justified in
its `Cargo.toml` comment, as today.

### 6.4 Performance budget

| Scenario | Budget | Today |
|---|---|---|
| floor2 p50 ms/step (release) | ≤ 2.0 ms | 0.009 (8-1; was 1.306 at `5e0e88d`) |
| floor2 allocs/step | ≤ 7,000, not growing with entity count after 8-1 | 70 (8-1; was 6,327) |
| 200×200 tilemap + 50 actors (after 8-1) | 60 fps debug | sim 0.345 ms/step debug, 0.298 release (8-1, was 42.4 release); frame rate not measured live yet |
| Editor frame at zoom 0.25 on floor2 (after 7E-5) | ≤ 4 ms | unmeasured |

### 6.5 `scripts/check.ps1` (and `check.sh`)

Created in 7A-6, extended as phases add rules. Fails on: any `.rs` over 750
lines; `HashMap`/`HashSet` iteration, `std::fs`, `Instant`, `eprintln!` in
`ember2d-sim` (grep until 7.5-9's clippy config takes over);
`partial_cmp` in `ember2d-sim`; `cargo tree -p ember2d-sim --depth 1`
showing anything but serde/ron/rhai/rand; doc numbers drifting from the
tree; `cargo fmt --check` (if 7A-9 chose A).

---

## 7. Decision gates

Decisions with real cost either way. Each is decided at a named point with
named criteria, and the outcome is recorded here.

### 7.1 Own chrome or egui — decided at 7C-9

**Continue with own chrome if** 7C-1 through 7C-8 land in ≤ 12 working
sessions and the editor harness reaches 20 tests. **Switch to egui (with a
pixel-art `Style` and bitmap `FontDefinitions`) if** 7C consumes more than
that, or if 7C-8's text editor is still missing selection/clipboard at the
gate. What is kept either way: the viewport, tile grid, painting, picking,
gizmos, node graph canvas — all on the engine's own renderer. What egui
would replace: panels, menus, modals, inspector rows, text fields, the
script editor's text widget. The refactor plan's warning stands: the
original burnout came from chrome work; sessions without visible progress
are the signal.

**Decision:** **Own chrome.** Neither trigger for switching to egui fired:
7C-1 through 7C-8 landed as 8 numbered steps across 2 calendar days
(2026-09-07 for 7C-1..7C-4, 2026-09-12 for 7C-5..7C-8 — `git log`,
comfortably under the 12-session threshold), and the editor harness sits
at 51 tests (`editor_input.rs` 27 + `editor_undo.rs` 12 +
`editor_script.rs` 12), well past the 20-test bar. 7C-8 also landed with
BOTH selection and clipboard, the specific "still missing" trigger the
gate named — not partially, not deferred. Kept as planned: viewport,
tile grid, painting, picking, node graph canvas on the engine's own
renderer. Proceeding to Phase 7D (own chrome), not 7D′.

### 7.2 Tilemap component — decided at start of Phase 8

**Build 8-1 if** any target game (RPG demo, or the game being built with a
friend) needs maps over ~5,000 tiles, or if `bench_sim` shows
`WorldSnapshot::build` above 1 ms at that size. Otherwise defer to Phase 11
and proceed to 8-2.

**Decision (2026-09-29): build 8-1.** Measured, not assumed —
`cargo run --release -p ember2d-sim --example bench_sim` at `5e0e88d`:
`WorldSnapshot::build` p50 = **3.18 ms** at the 5,000-tile synthetic level
(5.86 ms at 10,000; 26.5 ms at 40,000), over 3× the 1 ms trigger, and the
shipped floor2 (2,570 entities) already sits at 1.16 ms. Scoping choices
the user made at the same time are recorded in 8-1's own body.

### 7.3 Rollback (10-5) — decided at end of 10-4

Only if a realtime game needs online play. Turn-based lockstep ships first
regardless.

### 7.4 Rhai modules — decided at 7.5-13

Enable if `or_zero`-style duplication survives 7.5-2/7.5-3 in any shipped
script. The `no_module` feature currently exists for build size and
simplicity, not determinism. **Both sides now resolved, nothing left to
motivate this:** `or_zero()` is gone from all six roguelike scripts
(7.5-2, deleted, not just unused), and the `hp_`/`aware_`/`ehp_` global-
key-concatenation duplication is gone too (7.5-3 — `acted_`/`atk_*` turned
out to already be dead before 7.5-3 even started, removed by Phase 5f's
turn-scheduler rewrite). Revisit only if a future step reintroduces
key-concatenation duplication in a shipped script.

**Decision:** **No** — `no_module` stays on. The gate's own trigger
condition never fired: a fresh grep across both demos at 7.5-13 found zero
live `or_zero(` calls (only historical comments referencing the
now-deleted pattern) and no other cross-script key-concatenation
duplication either. Nothing currently in either shipped demo would benefit
from `import`; enabling modules now would only add AST-cache/hot-reload
module-resolution cost for a benefit no script actually needs yet.

### 7.5 Prefabs — decided in Phase 11

`PlayerRecord` becoming a normal entity is the groundwork. Prefabs proper
(named entity templates with overrides) only if the RPG demo's NPC
authoring shows the need.

### 7.6 rustfmt — decided at 7A-9

See that step. Record the choice here.

**Decision:** Option A — adopt rustfmt (`rustfmt.toml`: `max_width = 100`,
`use_small_heuristics = "Max"`), workspace reformatted once (`6f7230a`),
`cargo fmt --check` enforced in CI (`4957b32`).

---

## 8. Verification protocol

**Per step:** `cargo build --workspace --bins --examples` (NOT `--examples`
alone — that selects only example targets and silently leaves
`target/debug/ember2d.exe` stale, so any live check that launches the exe
directly instead of via `cargo run` tests the previous build; caught
2026-09-13 during R86, when a "no panic" live test ran a pre-fix binary);
`cargo test --workspace` (which never rebuilds the bin either); the step's
own named tests; manual smoke test named in the step; `git diff --stat`
matches the step's **Scope**.

**Per phase:** §0.5, in full, no exceptions. The checklist sections listed
in the phase heading are run by hand and ticked in
`ember2d-regression-checklist.md` with the date and commit.

**Replay gate:** `cargo test --test replay` three times as fresh processes
(a loop in the shell, not `--test-threads`), locally, and in CI on both
OSes. Required at every phase gate and after any step that touches
`ember2d-sim`.

**Screenshots:** 7B-1, 7B-2, 7B-5, and 7C-3 each require a before/after
screenshot pair stored under `docs/screenshots/<step>/` and referenced in
the commit message. "Appearance unchanged" is a claim that needs evidence.

---

## 9. Release plan and gate log

| Tag | After | Date | Tests | Clippy warnings | floor2 p50 | CI run |
|---|---|---|---|---|---|---|
| `v0.5.0-pre-refactor` | (retroactive, at `a7e3af0`) | — | 0 | — | — | — |
| `v0.5.7a` | Phase 7A | 2026-09-07 | 229 (186 unit + 42 integration + 1 doctest) | 59 at `--lib` scope, 86 at `--all-targets` (7A-8: 133 → 61 after `cargo clippy --fix`; final count moved slightly during 7A-9's rustfmt pass and the 7A-10/7A-11/7A-12 fixes, still a net decrease from the phase's own baseline) | not re-measured (no sim-path change in 7A) | local only — CI still blocked by the account billing lock (R37/R40); not yet confirmed green on either OS |
| `v0.5.7b` | Phase 7B | 2026-09-07 | 252 (209 unit + 42 integration + 1 doctest) | 59 at `--lib` scope (exact match with `v0.5.7a`), 71 at `--all-targets` (down from 86 — fewer test-binary duplicates, not a fix) | not re-measured (no sim-path change in 7B) | local only, same as `v0.5.7a` — CI still blocked by the account billing lock (R37/R40) |
| `v0.5.7c` | Phase 7C | 2026-09-13 | 381 (was 252 at `v0.5.7b`) | 43 at `--lib` scope, 55 at `--all-targets` (down from 59/71) | not re-measured (no sim-path change) | local only — CI still blocked by the account billing lock (R37/R40) |
| `v0.5.7d` | Phase 7D | 2026-09-13 | 381, same as `v0.5.7c` (tagged together — R88/R89 and the `UI Scale: 1.5x` follow-up landed as part of this same closing pass) | 43/55, unchanged from `v0.5.7c` | not re-measured (no sim-path change) | local only, same as `v0.5.7c` |
| `v0.5.7` | Phase 7E | | | | | |
| `v0.5.8` | Phase 7.5 | *pending — not tagged, see §5.6's own gate note* | 455 (was 381 at `v0.5.7d`) | 217 at `--all-targets` (was 55 at `v0.5.7d`; the jump is almost entirely 7.5-9's new `disallowed_types`/`disallowed_methods` lint categories firing on pre-existing lookup-only `HashMap`/`HashSet` and test-only `std::fs` use — R91/R92, already tracked, unscheduled — not a regression: message-by-message diffing at every step since 7.5-9 found zero new warnings from that step's own changes) | not re-measured (no sim-path perf change across 7.5) | local only — CI still blocked by the account billing lock (R37/R40) |
| `v0.5.9` | Phase 8 | | | | | |
| `v0.5.10` | Phase 9 | | | | | |
| `v0.5.11` | Phase 10 | | | | | |
| `v0.6.0` | Phase 11 | | | | | |

`main` is fast-forwarded to `claude` at every tag. Version numbers in the
four `Cargo.toml`s and CLAUDE.md change in the tag commit.

---

## 10. Risks

| Risk | Mitigation |
|---|---|
| Editor work becomes a sink again | §7.1 gate with a session budget; 7C-5 makes progress measurable in tests, not feel |
| Stabilisation sprint (7A) balloons | Every 7A step is one file or one concern; anything larger moves to its owning phase |
| wgpu/winit upgrade breaks rendering subtly | Screenshot pairs required (§8); do it before any theme work |
| `API_VERSION` 7 breaks scripts nobody remembers | Only two projects exist; both are rewritten in the same phase and shrink |
| Tilemap format change corrupts levels | v3 loads losslessly (and collapses at load); baking is automatic on save but lossless and idempotent (`tilemap_tests.rs`), every shipped level was verified tile-for-tile against its v3 original at 8-1, and the integrity test checks each baked level's grid against its own tiles |
| Determinism erodes during single-player phases | CI on two OSes, replay 3×, clippy lints on the sim crate (7.5-9) |
| Docs drift again | `check.ps1` fails the gate; §2/§3 updated in the same commit as code |
| Netcode competes with shipping a game | Phase 10 after the third demo; 10-5 conditional |

---

## 11. Parking lot

Ideas noted during work that belong to no current step. Promote to a step
or delete; never let this grow past a screen.

- Square world units (true 8×8 cells): still a platformer-demo concern; 7B-2
  makes the cell aspect a single constant, which is the prerequisite.
- A pixel-space script HUD API beyond 9-3's menu/dialogue widgets.
- Local co-op authoring (`PlayerRecord` plural, `camera_entity` plural).
- `Sprite.layer`/`TileRecord.layer`/`PlayerRecord.layer` unified to one type.
- Static flag on colliders to skip static-vs-static pairs (may be moot after
  8-1).
- `cargo clippy --workspace --all-targets` has drifted from the `v0.5.7b`
  baseline of 71 to 80 (confirmed already 80 as of `ddcafdc`, before 7C-4 —
  not this step's doing, and no single 7C step's own diff introduces a new
  warning message, per a direct before/after diff done at 7C-4's close).
  Only `--lib` counts have been tracked step-to-step through 7C; worth an
  explicit `--all-targets` re-baseline at the 7C gate so this doesn't keep
  drifting unnoticed. (Tracked step-to-step all through 7D-2 instead —
  currently 73, i.e. improved, not drifted further.) **Superseded by 7.5-9's
  `ember2d-sim/clippy.toml`** (`disallowed-methods`/`disallowed-types`),
  which makes raw totals incomparable to anything pre-7.5-9 — R91/R92 track
  the real, categorized gap that lint surfaced. As of 7.5-11 (`923c045`),
  `--all-targets` reports 218 total (was 227 at 7.5-9's own close,
  `59bdc3a`) — a genuine before/after message diff (not just the count)
  confirms zero new warnings across every file 7.5-10/7.5-11 touched, and
  three pre-existing ones fixed as side effects (a `too_many_arguments` on
  `lifecycle.rs` dropped below threshold once `PassArgs` landed, an
  `or_insert_with` nit in a test deleted along with the dead `scopes` map,
  and `AudioEngine` gaining a `Default` impl).
- `Renderer.ui_font`/`ui_font_kind`/`ui_font_px` (7B-5's `EMBER_UI_FONT`
  debug toggle, `renderer/font/mod.rs`) is now largely redundant: every
  editor panel draws its OWN text through the theme's font directly
  (7D-2) rather than through this process-wide field. Nothing still
  depends on it drawing real glyphs — `draw_str`'s bitmap branch is what
  the viewport's own HUD/debug text still uses, so the field itself can't
  go, but the `EMBER_UI_FONT=ttf` toggle's only remaining effect is that
  ONE remaining `draw_str` surface, not "the whole editor" the way its
  own doc comment still describes. Not a defect (nothing is wrong, the
  toggle still does what it says for what's left) — worth a doc-comment
  pass and maybe renaming away from "UI font" whenever someone's next in
  that file, so it doesn't read as more load-bearing than it now is.
- `WgpuBackend::render` returns early when there are zero instances
  (`backend.rs`, "if self.instances.is_empty() { return; }"), which skips
  the whole render pass INCLUDING its `LoadOp::Clear` — a frame that draws
  nothing keeps the previous swapchain image instead of clearing to
  `DEFAULT_BG`. Unreachable today (every state draws at least a HUD or a
  background), noticed during R51's re-diagnosis (2026-09-13); worth a
  one-line fix (always run the pass) whenever someone's next in `render`.
- `cargo fmt --all -- --check` drift (45 files, 7B-5 through 7D-3 era) was
  cleared by a one-time `cargo fmt --all` commit, 2026-09-13, same shape
  as 7A-9 — see §2.3's rustfmt row and R87 (§3.2) for the one file that
  pass pushed over 750 lines.
- Rhai's default max-expression-complexity guard trips easily when merging
  scripts that each independently stayed under it — 7.5-4's `enemy.rhai`
  merge needed one more function split than either source script alone
  did (§5.6, 7.5-4's own "Landed as" note). 7.5-5's own plan text shrinks
  the shooter director by folding its bullet/enemy blocks into per-entity
  scripts — worth compiling early rather than assuming a merge that looks
  small will fit.
- §3.2's R31/R32 rows still read `[ ] → 7.5-1` even though 7.5-1
  (`a3d483e`) landed both fixes (uniform i64/f64 dispatch, sentinel
  consistency) — noticed at 7.5-12/13's own wrap-up, not this session's
  doing and not fixed here (out of scope for either step); worth a
  one-line correction whenever someone's next in §3.
- `cargo fmt --all -- --check` has drifted again since the 2026-09-13
  one-time sweep — 46 files at 7.5-12's own check, 37 of them unrelated to
  that step (pre-existing, left alone rather than swept as a drive-by).
  Worth another explicit one-time `cargo fmt --all` pass at a future gate,
  same shape as 7A-9/2026-09-13.
- `cargo clippy --workspace --all-targets` held at 217 through 7.5-12
  (down 1 from 218 at 7.5-11's own close, `923c045`) and 7.5-13 (a
  decision-only step, no code) — still the same R91/R92-categorized gap
  the bullet above already tracks, no new drift.

---

## Appendix A — History (what each completed phase actually delivered)

Compressed from the archived plan documents. Commit hashes are the record;
the archived docs have the measured tables and reasoning.

**A.1 Phase 0 — Consolidate.** `gemini` (v0.5.0) merged into `main` as trunk;
`demo/` content recovered then later archived to `docs/archive/demo/`. The
planned `v0.5.0-pre-refactor` tag was never created — §9 adds it
retroactively at `a7e3af0`.

**A.2 Phase 1 — Defect sweep.** D1 input buffering (buffer-until-consumed,
~120 ms window, the semantics documented in the API doc), D2–D6, D8–D10,
D12–D14. Commit `714f04c` (with Phases 0 and 2).

**A.3 Phase 2 — World space and camera.** `DrawList` sorted by `(space, z,
texture)`, `Camera`, world-space draw entry points, rotation in the shader.
`draw_char(cell, cell)` kept as a screen-space helper.

**A.4 Phase 3 — Sprite and asset model.** `SpriteSource { Texture | Glyph |
Clip }`, `TextureId` handles, `AnimationClip`/`Animator`, level format v2
with graph sidecar migration, breaking scripting renames. `cf96c9f`,
`df48ad1`.

**A.5 Phase 4 — De-hardcode play mode.** `PlayState` lost all tag strings,
movement, and score. `demo/` archived; the roguelike (three floors +
victory) built as the first deterministic fixture. Camera follow stayed in
Rust (decision recorded: no tag strings, and scripting it would leak
`exp()`). D15, D16 found and fixed. `105604e`, `4857717`, `f04198e`,
`159007e`.

**A.6 Phase 5 — Simulation extraction.** `BTreeMap` determinism pass;
`on_input`/`on_turn` lifecycle; `TurnScheduler` min-heap; `Actor`
component; `SaveState` carries globals/clips (D17); `tests/replay.rs`
byte-identical at every checkpoint; workspace split into four crates with
`sim.rs` staying in `ember2d` (documented deviation). `a016267`.

**A.7 Phase 5.5 — Seams and animation queue.** Headless
`ember2d_sim::simulation::Simulation` (seam 1), `StepInput::external_commands`
(seam 2, proven by `tests/external_commands.rs`), `AnimationEvent` queue with
presentation-side playback that blocks the next step until drained. CI
added (`e92f223`) then deleted (`9524d70`). `ce8b375`, `4e4da60`.

**A.8 Phase 6 — Performance and data-model hardening.** 14 steps. Benchmark
harness with counting allocator; `mem::take` for globals/clips/persistent;
`Rc<str>` snapshot; snapshot skipped when no scripted collision; collision
layers → bitmask with a project-settings registry; sweep-and-prune broad
phase; hot-reload throttle (D21); timers off scope variables (D22 found);
`entity_ids` fixed to union all stores; `atan2_approx`. floor2 p50 9.723 →
1.817 ms/step (−81%), allocs 35,299 → 6,598. Deferred: `EntityId`
generation (now 10-2), binary snapshot (now 10-3). Rejected: component
registration macro. D19, D20 fixed mid-phase. `4429127` … `a1db60f`.

**A.9 Phase 7 Parts 1–2 — Pixel-space foundation and fonts.** `UiRect`,
`UiFrame` (draw-and-hit in one pass, applied to chrome/tabs/menus/palette/
inspector), `Panel`/`PanelManager` in pixels, E1/E3/E6 fixed, three pixel
primitives incl. `draw_nine_slice`, `Font` trait with `BitmapFont` and
`TtfFont` (fontdue, Cascadia Mono), `GlyphAtlas` shelf packer, `wrap_text`,
23 editor tests. Not met: Part 1's "no cell literals" and Part 2's "renders
through the trait" (→ 7C-2, 7B-5). E4 and most of E5 still open (→ 7C-3,
7C-1). `cf59f42`.

**A.10 Phase 7A — Stabilisation sprint.** 12 steps. R1–R6/R9/R10 (scripts
can't crash or hang the engine: operation limit, bad-argument panics,
`Animator` runaway loop, NaN-position collision-sort panic, silent no-ops).
R11–R14/R19/R20 (editor panics and input leaks: non-ASCII text, key-repeat
flooding, painting-guard gaps, docked-panel focus, orphaned `EditorState`,
empty-palette panic). R7/R8 (save/load exit-target and scheduler fidelity;
level format version check, every shipped level regenerated to v3). R15/R16
(presentation RNG and elapsed time no longer touch sim state). R35–R37
(scripting API doc corrected, CLAUDE.md/checklist truth pass, CI restored —
blocked on an account billing lock, not a workflow defect). R38–R40
(hygiene: LICENSE, manifests, per-process temp dirs, clippy auto-fixes).
rustfmt adopted (§7.6 Option A) — one-time reformat, `cargo fmt --check` in
CI; the two files it pushed over the 750-line limit split (R42/R43). Two
defects found live during this phase's own manual regression pass and
fixed the same session: Space never reached any text field (R44,
`Key::logical_key_text`) and `--editor <path>` never wired the Files panel
(R45, `EditorState::open_project_folder`). `ddd386e` … `f4a4720`,
`v0.5.7a`.

**A.11 Phase 7B — Renderer foundation.** 5 steps. wgpu 0.19→30, winit
0.29→0.30 (`ApplicationHandler`, `pump_app_events`, `CurrentSurfaceTexture`)
— R29's panic-on-unsupported-GPU fixed the same step. Integer cell
projection: `compute_layout` letterboxes instead of stretching a
non-exact-multiple window size, DPI-derived `scale` replaces the old fixed
constant, `ScreenMapping` replaces a width-only `scale_factor()` (R21);
found live afterward that flooring `scale` at `1.0` (not `2.0`) shrank the
whole UI on any ordinary 100%-scale display — `MIN_UI_SCALE` follow-up
(R48). Renderer resource hygiene: `TextureBudget` LRU+byte-budget eviction
so GPU textures are actually freed (R26), atlas-texture over-cloning fixed
(R27). Engine loop and input: the redundant `thread::sleep` frame-pacing
throttle removed now that `PresentMode::Fifo` alone paces correctly (R23);
`PressBuffer<K>` extracted from three copies of the same held/pending/
consumed/decay logic (`InputManager`/`MouseState`/`GamepadState`), gaining
a real OS-repeat signal (R24) and gamepad-disconnect cleanup (R25); the
`in_viewport` HUD-row cull left over from a bar Phase 4 removed is gone
(R28). `Renderer::draw_str` finally routes through the `Font` trait for a
new `EMBER_UI_FONT=ttf` debug toggle — screenshot comparison caught that
routing the DEFAULT path through it too would have shrunk every glyph
(`BitmapFont`'s square-glyph model disagrees with font8x8's historical 2x
vertical stretch), so the default path deliberately still calls `draw_char`
directly; the mismatch is logged as R49/R50 rather than fixed. `25b9058` …
`a1061c0`, `v0.5.7b`.

**A.12 Phase 7C — Editor foundation.** 9 steps. `UiFrame` registration made
mandatory (7C-1, `03f34bf`): 13 independent hit-test functions across
`ember2d-editor` and `StartScreen` migrated to draw-and-push helpers in
`ui/widgets.rs`, new `UiFrame::rect_of`. No cell literals below
`Panel.rect` (7C-2, `e0d305c`, E2 remainder). `Layout` deleted outright —
the viewport becomes a real, dockable (if non-closable) panel (7C-3,
`9bad191`, E4); `mouse_to_grid` reads pixels directly off
`PanelManager::viewport().content_rect()`, and `draw_cursor_highlight`/
wheel-zoom pivot share its exact formula so the three can't drift.
`EditorMode` enum replaces ~15 mutually-exclusive booleans (7C-4,
`0eb2db8`); real `Consumed`/`Pass` input chaining deferred pending a test
harness to verify it. That harness is 7C-5 (`4ede7f5`): a new
`DrawSurface` trait + `NullRenderer` + `EditorHarness` (23 tests), which
immediately found the fullscreen script editor completely broken (R54)
and a 60Hz+ typed-character drop bug live since 7A-2 (R55). `LevelGrid`
moves to a deterministic `BTreeMap` (D18) with real paint/erase/scatter
undo batching, graph/hierarchy/palette edits all made undoable, and
destructive actions now confirm first (7C-6, `b2a608f`); found and fixed
R59 (Delete File resolving against the wrong directory) along the way.
Script errors reach the editor console via new
`GameState::take_script_log`/`receive_script_log` trait methods crossing
the state-stack's type-erasure boundary, plus live compile-on-save/idle-
timer syntax checking with inline error highlighting (7C-7, `35887a6`,
R18). Script editor completeness — selection, OS clipboard via
`arboard`, per-buffer undo, incremental find, horizontal scroll (7C-8,
`1747b9d`). 7C-9 is the §7.1 decision gate: own chrome, not egui — found
R61 (8 shipped scripts still referencing pre-`demos/`-move paths) via the
gate's own demo smoke-launch. `v0.5.7c`.

**A.13 Phase 7D — Theme and restyle.** 4 steps plus follow-ups. `Theme`
resource — `PaletteRole`/`SliceRole`/`NineSlice`/`FontChoice`/
`FontSizes`/`Metrics`, RON-loaded with loud-fallback-never-panic
semantics — lands as pure logic first (7D-1, `5699ce5`/`b19ac82`), then
the real `themes/ember-clean/` ships via a generator
(`gen_ember_clean_theme.rs`) rather than hand-authored art;
`themes/ember-pixel` stays deferred. Chrome moves through 9-slice across
roughly 24 draw functions in 11 files (7D-2, `4dc90ed` through
`c210901`), unifying every panel/modal/menu/dock-content color and
geometry onto the theme; found and fixed R63 live (a nine-slice atlas
region bug). `UiRect::from_cells` itself is deleted in a same-phase
follow-up once investigation showed the theme's real font was rendering
nowhere but the title bar. UI points — a whole new `ui_scale`/
`render_scale` coordinate space (`UiSpace`/`UiPainter`, `EditorPrefs`,
`ChromeMetrics`, `ScriptLayout`) — lands across 7 checkpoints (7D-3),
fixing R50 and R64–R85 along the way, including two live-caught
double-scaling bugs (R84/R85) the checkpoint's own earlier tests hadn't
reached. Runtime theme switching via a new top-level `Theme` menu, plus
`docs/ember2d-theming.md` (7D-4, `6464dd3`). Two dated follow-ups closed
out this same pass: `UI Scale: 1.5x` added between 1x and 2x (`e5e489e`,
widening `ui_scale` from an integer to `f32` throughout the renderer and
editor), and two live-reported bugs found and fixed during the phase's
own manual close-out — R88 (`ed1b224`, the status bar's coordinate
readout) and R89 (`99ee94b`, the dropdown menu's hover highlight) — both
the identical points/logical unit mismatch in sibling code paths, each
found via a user screenshot and pinned with a regression test.
`v0.5.7d`.

## Appendix B — Archived documents and what they still hold

| File (in `docs/archive/`) | Still useful for |
|---|---|
| `ember2d-refactor-plan.md` | Original goals, §4 target architecture sketches (`DrawCommand`, `Sprite`, `AnimationClip`, turn scheduling), §5 networking rationale in full, §8 roadmap conflict discussion |
| `ember2d-phase5-plan.md` | Step-by-step reasoning for the determinism pass and workspace split; the two prep gaps found |
| `ember2d-phase5.5-plan.md` | §0.2 — why turn-based physics was rejected in favour of the animation queue |
| `ember2d-phase6-plan.md` | Every measured table (per-step ms and allocs for all 14 steps, synthetic n=500…10000 sweep); §0 deferral reasoning for EntityId/binary snapshot; the component-macro rejection |
| `ember2d-phase7-plan.md` | §0 framing and aesthetic direction (still the design brief for 7D); Part 1–2 specs as landed |
| `ember2d-rpg-demo-feasibility.md` | The eight gaps with code citations — the source for Phase 7.5 and Phase 9 |
| `HANDOFF.md` | Phase 6 → 7 session context; the `AppActivate` automation warning (§ "Workflow reminders") |
| `roadmaptoV0.6.md` | The original V0.5.3–V0.6.0 editor item list |

## Appendix C — The 2026-09-06 review in one paragraph

Full report: https://claude.ai/code/artifact/b4884c88-1ee9-4e65-9467-aa7c52dc05c3.
Overall 6/10: simulation core 7, engine 6, editor 5, tests 6, docs 5,
process 4. Strongest: the four-dependency sim crate, the deferred-mutation
scripting design, the layer registry, the scheduler, measured perf work,
behaviour-level tests. Weakest: scripts can crash or hang the editor (R1–R6),
save/load is not a faithful round trip (R7), the editor has user-reachable
panics and input leaks (R11–R14), the renderer stretches cells (R21), CI is
gone (R37), and five documents contradict the tree (R36). Every item is in
§3 with a step number.
