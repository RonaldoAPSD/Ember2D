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

1. `cargo build --workspace --examples` clean; `cargo test --workspace` green.
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

| Crate | Contents | Depends on | Lines |
|---|---|---|---|
| `ember2d-sim` | math, color, world, components, level, save, scripting, command, scheduler, graph, event, layers, simulation | serde, ron, rhai, rand only | ~10,240 |
| `ember2d` | engine loop, renderer (wgpu), font system, input/mouse/gamepad, audio (kira), play, project, camera, `sim.rs` per-step pump | `ember2d-sim` | ~10,880 incl. tests |
| `ember2d-editor` | level/script/graph editor, docking, start screen | both above | ~11,910 |
| `ember2d-app` | `main.rs` + Editor↔Play orchestration | `ember2d`, `ember2d-editor` | ~325 |

Line counts jumped at 7A-9 (`cargo fmt --all`, one-time, no logic change —
rustfmt's own line-wrapping expanded the whole tree by roughly a third; two
files it pushed over the 750-line limit were split at 7A-10, R42/R43).
`ember2d` grew the most at 7B (new `press_buffer.rs`, `renderer/text.rs`,
the `font/` module's `ui_font_from_env`) — still 0 files over 750 lines.

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
| 7C | Editor foundation | `[ ]` — §5.3 |
| 7D | Theme and restyle | `[ ]` — §5.4 |
| 7E | Editor features | `[ ]` — §5.5 |
| 7.5 | Scripting completeness | `[ ]` — §5.6 |
| 8 | Tilemap, assets, animation authoring | `[ ]` — §5.7 |
| 9 | Scene and UI layer + RPG demo | `[ ]` — §5.8 |
| 10 | Networked 2-player | `[ ]` — §5.9 |
| 11 | Presets, cleanup, 0.6.0 | `[ ]` — §5.10 |

### 2.3 Baseline numbers (at `v0.5.7b`)

| Metric | Value | Where measured |
|---|---|---|
| Tests | 209 unit + 42 integration + 1 doctest = 252, all pass | `cargo test --workspace` |
| Clippy | 0 errors, 59 warnings at `--lib` scope (unchanged from `v0.5.7a`; fewer at `--all-targets`) | `cargo clippy --workspace --lib` / `--all-targets` |
| rustfmt | applied; `cargo fmt --all -- --check` clean | `cargo fmt --all -- --check` |
| `cargo test --test replay` | green 3× fresh processes | §0.5 gate criterion 4 |
| floor2 p50 ms/step | not re-measured this gate (no sim-path change in 7B) | `cargo run --release -p ember2d-sim --example bench_sim` |
| floor2 allocs/step | not re-measured this gate (no sim-path change in 7B) | same |
| `LEVEL_FORMAT_VERSION` | 3 (unchanged since 7A-4) | `ember2d-sim/src/level.rs:298` |
| `API_VERSION` | 6 (unchanged) | `ember2d-sim/src/scripting/types.rs:25` |
| Registered script functions | 124 (unchanged) | `grep -c register_fn ember2d-sim/src/scripting/registry.rs` |
| Files over 750 lines | 0 | `scripts/check.ps1` |
| Dependencies | wgpu 30.0.1, winit 0.30.13, kira 0.12.4, glam 0.33.7, rand 0.8.6, gilrs 0.11.2, rhai 1.24.0, fontdue 0.9.4 — wgpu/winit/glam/kira/gilrs all upgraded at 7B-1 (were 0.19.4/0.29.15/0.25/0.9.6/0.10 at `v0.5.7a`) | `Cargo.lock` |

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
| D22 | S3 | Cancelled and just-fired timers share a sentinel | `api.rs`, `apply.rs:119-128` | `[ ]` → **7.5-8** |

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
| R22 | S4 | `scopes` map is dead state (rhai rewinds scope; timers moved off it) yet maintained by hot-reload and despawn | `scripting/engine.rs` | `[ ]` → 7.5-10 |
| R23 | S3 | Frame pacing double-throttles (Fifo vsync + `thread::sleep` to 60) | `engine.rs:389-392` | `[x]` 7B-4 — the tail-of-loop `thread::sleep(FRAME_DURATION - frame_elapsed)` and its now-unused `TARGET_FPS`/`FRAME_DURATION` constants are gone; `wgpu::PresentMode::Fifo` (renderer/mod.rs) is the sole pacing mechanism now |
| R24 | S3 | Key repeat inconsistent: `repeat` flag ignored; letters repeat into text buffer, editing keys never repeat; Ctrl+S pushes "s" | `engine.rs:226-243` | `[x]` 7B-4 — `PressBuffer::handle_repeat`/`is_repeating` (new `repeating: HashSet<K>`, separate from `pending`/`consumed`) fed from `KeyEvent::repeat` in `EventPump`; script editor's Up/Down/Left/Right/Tab/Enter/Backspace now check `is_repeating` alongside `just_pressed`; `text_buffer` pushes gated on `!ModifiersState::control_key() && !super_key()` (new `Engine::modifiers` field, updated on `WindowEvent::ModifiersChanged`) so Ctrl+S no longer leaks an "s" |
| R25 | S3 | `GamepadState::poll` ignores `Disconnected`; held buttons stick | `gamepad.rs:131-159` | `[x]` 7B-4 — `EventType::Disconnected` now calls the new `PressBuffer::retain` to drop every held/pending/consumed/just-released/repeating entry for that `gamepad_id`, plus clears its `axes` entries |
| R26 | S3 | GPU textures never freed; `AssetManager::clear` doesn't invalidate `texture_cache` | `backend.rs:124` | `[x]` 7B-3 — new `TextureBudget` (LRU + byte budget, `renderer/texture_budget.rs`) plus `AssetManager::clear` now evicts every id it forgets via a new `TextureEvictor` trait `Renderer` implements |
| R27 | S3 | `draw_text_px` clones the 4 MB atlas `Texture` per call | `renderer/mod.rs:270` | `[x]` 7B-3 — only clones real pixel data when `dirty` or not yet GPU-resident (`WgpuBackend::has_texture`); every other call passes a lightweight placeholder instead |
| R28 | S3 | Bottom world row never drawn (culled for a HUD bar removed in Phase 4) | `play/render.rs:106-108` | `[x]` 7B-4 — removed the trailing `.saturating_sub(1)` on `height` in `in_viewport`; confirmed visually (floor2 screenshot, bottom wall/floor row now renders through to the HUD text row) and via a new regression test |
| R29 | S3 | `request_adapter`/`request_device` `.expect` → panic with no message on unsupported GPU | `renderer/mod.rs:84, 93` | `[ ]` → 7B-1 |
| R30 | S3 | Audio: decode from disk on every `play_sound`; new `AudioEngine` per level kills music | `audio.rs:38, 51`; `play.rs:203` | `[ ]` → 7.5-11 |
| **API and docs** | | | | |
| R31 | S2 | i64/f64 dispatch trap: `draw_hud(x, y, ..)` with float `x` fails "function not found"; only `submit` coerces | `api.rs` throughout | `[ ]` → 7.5-1 |
| R32 | S3 | Sentinel inconsistency (`-1` vs `0.0` vs `[]`; `()` tombstone means scripts can't store unit) | `api.rs` | `[ ]` → 7.5-1 |
| R33 | S3 | `is_animating` always `false` | `api_animation.rs:63` | `[ ]` → 7.5-7 |
| R34 | S2 | Node-graph codegen: no string escaping (code injection), no cycle guard (stack overflow), untyped `"0.0"` defaults, block-scoped `let` | `graph/codegen.rs:30, 40, 55, 138-146, 224, 249` | `[ ]` → 7.5-12 |
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
| R50 | S4 | `Renderer::draw_text_px`'s glyph destination rect always uses `GlyphInfo::atlas_rect.w`/`.h` directly as the drawn size — correct for `TtfFont` (whose atlas rasterizes each glyph AT the requested px, so `atlas_rect` already IS the intended render size) but wrong for `BitmapFont` at any non-native px: `atlas_rect` stays fixed at the native 8×8 texture region regardless of the requested size (only `advance`/`offset` scale), so a `BitmapFont` glyph requested at e.g. 16px would advance the pen 16px per character without the glyph itself actually being drawn any larger than 8×8 — a gap that predates 7B-5 but was undiscovered until this step's investigation, since `draw_text_px` had no live caller before it (its own doc comment used to say so) | `renderer/text.rs` (`draw_text_px`, dest rect construction) | `[ ]` unscheduled — not hit by 7B-5's own usage: `draw_str`'s new `draw_text_px` call is reached only when `ui_font_kind` is `Ttf`, so it never passes a `BitmapFont` in the first place (see R49, 7B-5's "Landed as" note) |
| R51 | S2 | Maximizing (or otherwise growing) the editor window, then pressing F5 to play, leaves stale editor-panel pixels (the docked Inspector's title/tool text, colored bars) visibly stuck on screen wherever the play view's own grid is narrower than the resized window — found live by the user right after the 7B gate. Confirmed this is NOT an app-side "forgot to clear" bug: instrumented `Renderer::try_handle_resize` and verified the surface *is* reconfigured to the correct new size and the render pass's `LoadOp::Clear` covers the whole surface every frame (dozens of frames observed, well into active play — e.g. "Turn 20" in the user's own screenshot). A manual 1px-out-1px-back resize nudge afterward repainted *most* (not all) of the stale area, which points at a Windows DWM compositor caching artifact tied to wgpu's flip-model swapchain presentation during a resize/maximize transition, not application logic — a known-tricky class of issue on Windows with wgpu/winit, not something to guess-fix without real investigation | `ember2d/src/renderer/mod.rs` (`try_handle_resize`, `recompute_layout`); Windows DWM/wgpu swapchain presentation, not yet isolated to a specific engine file | `[ ]` unscheduled — needs focused investigation (try forcing a window repaint/invalidate after resize, a different present mode, or comparing against known wgpu/winit issue reports) before attempting a fix; reproduce with: maximize the editor window, then F5 |
| R52 | S3 | 7C-1's own debug assertion in `handle_canvas_input` — "any click reaching here while `UiFrame::hit()` is `Some` is a bug" — fired constantly during ordinary use (both painting and legitimate widget clicks), found live by the user during 7C-2's manual smoke pass. Root cause: the assertion assumed panel input consuming a click already stops `handle_canvas_input` from running, but `handle_update` calls `handle_panel_input`/`handle_canvas_input`/`handle_shortcuts` unconditionally, one after another, regardless of what an earlier stage did — `handle_panel_input`'s own internal `return`s only stop itself. That's exactly the gap 7C-4's `Consumed \| Pass` chain is designed to close; the assertion's premise doesn't hold until then, so as written it couldn't distinguish a real bleed-through from any click that happens to also land on any registered widget anywhere on screen (which is most clicks). Painting itself was never actually broken — `handle_canvas_input`'s own `mouse_to_grid` bounds check is what really prevents unwanted painting, unrelated to whether `UiFrame::hit` matched something | `ember2d-editor/src/editor/input/canvas.rs` (`handle_canvas_input`) | `[x]` 7C-1 follow-up (`f89b301`) — assertion removed; re-add once a real `Consumed`/`Pass` signal exists to check instead of `UiFrame::hit` alone. 7C-4 (`0eb2db8`) replaced the boolean soup but deliberately did not build that signal (see 7C-4's own "Landed as" note) — still deferred, now past 7C-4 |
| R53 | S4 | `cargo fmt --all -- --check` reports diffs in 63 files across all three non-`ember2d-app` crates, none of which touch 7C-1 through 7C-4's own changed lines (verified: `git stash` on top of `v0.5.7c`'s uncommitted tree still reproduces all 63) — the `v0.5.7b` baseline (§2.3) recorded this clean. Most likely cause is a local rustfmt version difference from whatever ran 7A-9's one-time `cargo fmt --all` pass, not anything a specific step's own diff introduced — the affected lines are scattered, small (mostly whether a boolean `if`/`let` condition wraps), and span files no 7C step touched | tree-wide, found running 7C-5's own gate checks | `[ ]` unscheduled — needs `cargo fmt --all` re-run and a fresh baseline recorded once whichever step next touches the affected files, or at the 7C gate itself; not this step's job per its own Scope |
| R54 | S1 | The fullscreen script editor (`EditorMode::Script`) was completely unusable — opening it (clicking a `.rhai` file in the File Browser) worked for exactly one frame, then silently reverted to `Paint` on every frame after, invisible at 60fps: looked to the user like clicking the file did nothing. Root cause: 7C-4's `handle_update` dispatch (`match std::mem::take(&mut self.mode) { EditorMode::Script => { self.handle_script_mode_input(input, mouse); return; } ... }`) empties `self.mode` to `EditorMode::default()` before calling the handler — every OTHER hard-exclusive arm's own handler restores its mode as its first action (`handle_color_picker_input`/`handle_palette_editor_input`/`handle_place_spawn_input`/`handle_palette_search_input`), but `EditorMode::Script`'s arm never did, so `handle_script_mode_input`'s own `fullscreen = matches!(self.mode, EditorMode::Script)` always read `false` and nothing put `self.mode` back. A 7C-4 defect (`0eb2db8`), not a 7C-5 one — found live by the user testing 7C-5's own work, then reproduced and root-caused with the very `EditorHarness` 7C-5 built (`clicking_a_rhai_file_in_the_file_browser_opens_the_fullscreen_script_editor`, `editor_input.rs`) before this row existed — the harness catching a real, user-blocking bug moments after landing is itself evidence for why the step exists | `ember2d-editor/src/editor/input/mod.rs` (the `EditorMode::Script` arm) | `[x]` 7C-5 follow-up (`4ede7f5`) — `self.mode = EditorMode::Script;` restored at the call site (the arm has no payload to reconstruct it from inside the handler the way the others do), one line, before calling `handle_script_mode_input`. Regression test named above. `cargo test --workspace`: 279 (255 + 24), all pass; `cargo clippy --workspace --lib` unchanged at 56; `scripts/check.ps1` clean |
| R55 | S2 | Typing into any text-capture widget (the fullscreen/docked script editor, prompts, palette editor/search, graph param fields) silently dropped scattered characters at ordinary typing speed on any display faster than 60Hz — found live by the user immediately after R54 unblocked the fullscreen editor for the first time ("Hello how are you doing today" arrived as " ello howre you dog ody"). Root cause, in `ember2d/src/engine.rs`'s `run()`/`poll_events()` (predates 7C-4 and 7C-5 both — present since R12/7A-2 introduced the `begin_text_capture`/`finish_frame_text_capture` mechanism, just never noticed because nothing had stress-typed the fullscreen editor before R54, and most prior manual passes likely ran on ~60Hz displays where the bug is structurally unreachable): `poll_events` (called once per REAL frame) unconditionally called `finish_frame_text_capture`, which wipes `text_buffer` unless some widget "renewed" capture — but the only place that renewal (`begin_text_capture`) can happen is inside a simulation step (`sim::step`, called from `update()`), and `GameplayLoop::RealTime`'s fixed-timestep accumulator legitimately produces real frames with zero steps whenever less than one `SIM_DT` (1/60s) of real time has accumulated — true for roughly half of all frames at 144Hz. Checking on a zero-step frame always found nothing had renewed the request (nothing could have) and wiped out keystrokes `poll_events` had just captured that same frame. `EditorHarness` (7C-5) could not have caught this: it pairs one simulated "frame" with exactly one `sim::step` call by construction, which structurally cannot reproduce a real frame with zero steps | `ember2d/src/engine.rs` (`Engine::poll_events`, `Engine::run`'s `RealTime`/turn-based branches); `ember2d/src/input.rs` (`text_capture_requested`, `begin_text_capture`, `finish_frame_text_capture` doc comments corrected to match) | `[x]` 7C-5 follow-up (`4ede7f5`) — `finish_frame_text_capture` moved out of `poll_events` (always) into `run()`, called only when a step actually ran this frame (`steps > 0` in the `RealTime` branch; unconditional in the turn-based branch, which always steps exactly once) — the check is now always paired with the step that could have renewed it, regardless of display refresh rate. No headless regression test: reproducing the real bug needs the actual decoupled poll/step timing a live windowed loop has, which neither `EditorHarness` nor `TurnHarness` model (both pair 1:1 by construction) — verified instead via `cargo test --workspace` (279, unchanged), `cargo clippy --workspace --lib` (56, unchanged), `scripts/check.ps1` clean, and `cargo test -p ember2d --test replay` 3× fresh processes (unaffected — `text_buffer` is UI-only state no script/gameplay code reads) |
| R56 | S2 | A brand-new project's level files never appeared in the File Browser even after confirming on disk they existed — found live by the user manually testing New Project → new level creation. Two related gaps, neither in code this session touched before finding them: (1) `TextInputPurpose::NewLevelName`'s commit handler wrote the new `.level` file to disk but never called `refresh_project_files` (every sibling file-creating action — New Script, a level-switch confirm — already did); (2) plain `save()` (Ctrl+S/`S`) never did either, which matters specifically the first time a brand-new project's level is saved (no prior file at that path for the browser to have already listed) | `ember2d-editor/src/editor/input/text.rs` (`NewLevelName` arm); `ember2d-editor/src/editor/impl_state/mod.rs` (`save`) | `[x]` 7C-5 follow-up (`b065b54`) — `self.refresh_project_files()` added to both success paths. Regression tests: `saving_a_brand_new_level_for_the_first_time_refreshes_the_file_browser`, `creating_a_new_level_via_the_level_menu_refreshes_the_file_browser` (`editor_input.rs`) — new `EditorHarness::with_state` constructor and `EditorState::file_browser_files()` accessor added to make them possible. `cargo test --workspace`: 281 (255 + 26), all pass; `cargo clippy --workspace --lib` unchanged at 56; `scripts/check.ps1` clean |
| R57 | S2 | "Rename Level" only ever updated `grid.name` (the title-bar display name) — the file on disk, and `save_path`, kept the old name forever, so a renamed level's File Browser entry and its own displayed name silently drifted apart — found live by the user manually testing (screenshot: title bar read "Test4" while the File Browser still listed "Test.level"/"Test2.level"/"Test3.level" from earlier renames, none of them ever cleaned up) | `ember2d-editor/src/editor/input/text.rs` (`TextInputPurpose::LevelName` arm) | `[x]` 7C-5 follow-up (`ff1cd3b`) — if a file already exists at the old `save_path`, `std::fs::rename` it to the new name alongside updating `grid.name`; `save_path` always moves to the new name either way (even with no file yet on disk), so the next save lands at the new name instead of the old one; `refresh_project_files` called when a rename actually changes the path. Regression test: `renaming_a_level_also_renames_its_file_on_disk` (`editor_input.rs`). `cargo test --workspace`: 282 (255 + 27), all pass; `cargo clippy --workspace --lib` unchanged at 56; `scripts/check.ps1` clean |
| R58 | S4 | User-directed hygiene, not a found defect: `roguelike/`/`shooter/` demo projects sat at the repo root, and both the New Project and Open Project browsers defaulted to `std::env::current_dir()` (the repo root under `cargo run`) — a fresh user's first action was staring at the engine's own source tree | `roguelike/`, `shooter/` (moved); `ember2d-editor/src/editor/start_screen/logic.rs` (`init_fb`, `Screen::OpenProject`'s menu-1 arm) | `[x]` 7C-5 follow-up (`a41d8f4`) — `git mv roguelike demos/roguelike`, `git mv shooter demos/shooter`; every `"roguelike/"`/`"shooter/"` path reference updated across code, tests, examples, CI, and docs (`grep -rl` before and after, zero stragglers) — including the two bare `Path::new("roguelike")`/`("shooter")` output-dir constants in `gen_roguelike.rs`/`gen_shooter.rs` a trailing-slash-only search missed on the first pass, caught by `external_commands.rs` failing (the shipped `.level` files themselves embed `script`/`next_level` as literal `"roguelike/scripts/..."` RON strings, resolved at runtime — fixed by regenerating every shipped level via the now-corrected examples rather than hand-editing RON). New `StartScreen::default_projects_dir()` (repo-root `Projects/`, created on demand) replaces `current_dir()` at both `init_fb` and Open Project's own folder-listing entry point. `Projects/` added to `.gitignore` (developer-local projects, not shipped content — mirrors the existing `*.palette.ron` reasoning). No automated regression test for the `StartScreen` default-folder change itself (`StartScreen` has no headless harness the way `EditorState` does after 7C-5 — see 7C-5's own Scope) — verified by code inspection and the full `cargo test --workspace` pass (282, unchanged) confirming every demo-loading test still resolves its files correctly against the new paths; `cargo test -p ember2d --test replay` 3× fresh processes green; `cargo clippy --workspace --lib` unchanged at 56; `scripts/check.ps1` clean |
| R59 | S2 | `ContextMenuAction::DeleteFile`'s path was built from the raw File Browser row label alone (`&raw[3..]`, then trimmed) and never combined with `current_folder`/`project_folder` the way every sibling file action already does — `std::fs::remove_file` therefore always resolved against the process's current working directory, not the project's actual location, so deleting a file from the browser silently failed (or, worse, could delete an unrelated same-named file sitting in the engine's own CWD) for any project opened from anywhere other than the repo root. Found live while writing 7C-6's own `deleting_a_file_from_the_browser_confirms_first_and_declining_keeps_it`/`confirming_a_file_delete_actually_deletes_it` regression tests — the new test failed against the pre-existing code before this step touched the surrounding confirm-dialog logic, isolating it as a defect this step did not introduce | `ember2d-editor/src/editor/input/panels/context_menu_trigger.rs` (the `DeleteFile` menu-item builder) | `[x]` 7C-6 (`b2a608f`) — path now joins `project_folder` and `current_folder` the same way every other file-referencing action in this file does; directory rows (`raw.starts_with("/ ")`) are now excluded from getting a Delete entry at all, since `std::fs::remove_file` never supported deleting a folder in the first place and previously did so silently-wrong via the same broken path. Regression tests above (`editor_undo.rs`) cover both the confirm-and-decline path and the confirm-and-delete path, the latter asserting the file is actually gone from disk at the correctly-joined path |
| R60 | S2 | Clicking a level in the File Browser while there were unsaved changes did not confirm before switching, discarding them — found live by the user testing 7C-6's own new confirm-before-switch behavior. Root cause: the File Browser click handler's confirm check was `if self.unsaved` alone (grid dirtiness only); `switch_to_level`'s `*self = ns` replaces the *entire* `EditorState`, so an open script buffer edited but not saved (`self.script_unsaved`, tracked independently since script edits don't touch the level grid at all) was silently dropped with no confirmation whenever the grid itself happened to be clean — the exact "there ARE unsaved changes and it doesn't confirm" the user reported, just from the script side rather than the grid side 7C-6's own tests already covered | `ember2d-editor/src/editor/input/panels/file_and_script.rs` (the `.level` click arm's confirm condition) | `[x]` 7C-6 follow-up (`55605a4`) — condition widened to `if self.unsaved \|\| self.script_unsaved`. New `EditorState::script_unsaved()` accessor added (`unsaved()` already existed) to make this testable. Regression test: `switching_levels_with_only_an_unsaved_script_edit_still_confirms_first` (`editor_undo.rs`) — opens a `.rhai` file, types into it, leaves fullscreen via Escape (script buffer stays dirty, grid stays clean), then confirms clicking a different level now shows the modal. `cargo test --workspace`: 296 (was 295), all pass; `cargo clippy --workspace --lib`/`--all-targets` unchanged at 56/80; `scripts/check.ps1` clean |

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

#### `[ ]` 7C-8 — Text editor completeness

- **Why:** No selection, clipboard, undo, find, horizontal scroll.
- **Change:** selection (Shift+arrows, Shift+click, Ctrl+A), clipboard via
  `arboard` (one new dep, editor-only), per-buffer undo stack, Ctrl+F
  incremental find, horizontal scroll following the cursor, key repeat
  (from 7B-4). Long lines show a `…` marker rather than clipping.
- **Test:** harness: select-all, cut, paste round-trip preserves content
  including non-ASCII.
- **Done when:** checklist §8 extended and passing.
- **Scope:** `ember2d-editor`.

#### `[ ]` 7C-9 — Decision gate: own chrome or egui (§7.1)

Evaluated here, with 7C-1 through 7C-8 as evidence. Record the decision and
its reasoning in §7.1 and proceed to 7D (own chrome) or 7D′ (egui skin).

**Phase 7C gate:** §0.5, then tag `v0.5.7c`.

---

### 5.4 `[ ]` Phase 7D — Theme and restyle

*(Phase 7 plan Parts 3–4, with the unspecified types now specified. If §7.1
resolves to egui, this phase becomes "write the pixel egui style and port
panels" and 7D-1/7D-2 are replaced by an `egui::Style` plus a bitmap-font
`FontDefinitions`; 7D-3 and 7D-4 stand.)*

**Checklist sections at gate:** §3–§9 (full editor pass).

#### `[ ]` 7D-1 — `Theme` resource, fully typed

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

#### `[ ]` 7D-2 — Chrome through 9-slice

`draw_panel_chrome` → one `draw_nine_slice` for the frame, one for the
title bar, `measure`-centred title, one slice for the close button. Buttons,
inputs, tabs, scrollbars, checkboxes, resize grip follow. Text draws from a
baseline (`ascent()` at box-thinking call sites). `UiRect::from_cells` is
deleted at the end of this step; panels size to content and theme metrics.

- **Done when:** the editor no longer looks cell-quantised; `from_cells` is
  gone; the pixel theme at 2× and the clean theme both render crisply
  (7B-2 makes this possible).

#### `[ ]` 7D-3 — Integer UI scale, separate from canvas zoom

`Theme.ui_scale` multiplies chrome and font sizes; canvas zoom is
independent. Nearest-neighbour for bitmap/9-slice; TTF rasterises at the
scaled size. Runtime switch via View › UI Scale.

#### `[ ]` 7D-4 — Theme switching and `docs/ember2d-theming.md`

View › Theme lists `themes/*`; switching reloads chrome and font without
restart. New doc: file format, palette roles, how to author a chrome atlas,
how the two shipped themes differ.

**Phase 7D gate:** §0.5, full checklist §3–§9, then tag `v0.5.7d`.

---

### 5.5 `[ ]` Phase 7E — Editor features

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

### 5.6 `[ ]` Phase 7.5 — Scripting completeness

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

#### `[ ]` 7.5-1 — Uniform typing and sentinels (breaking)

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

#### `[ ]` 7.5-2 — Atomic global/persistent arithmetic

`add_global(key, delta) -> new_value`, `add_persistent(key, delta) ->
new_value`, applied at `apply_ctx` time against the **current** value
(after earlier pending writes in the same pass). Delete `or_zero()` from
all six scripts and `director.rhai`'s duplicate-tally.

#### `[ ]` 7.5-3 — Per-entity variables

`set_var(id, key, value)`, `get_var(id, key) -> Dynamic`, `has_var`,
`remove_var`, backed by a new `Vars` component (`BTreeMap<String, Dynamic>`,
serialised in `SaveState`, cleared on despawn, visible in the inspector as
read-only). Migrate `hp_`, `aware_`, `acted_`, `atk_*` keys in the
roguelike and `ehp_` in the shooter.

#### `[ ]` 7.5-4 — Data-driven actor stats

`TileRecord.actor` (exists since Step 5f) gains `stats: BTreeMap<String,
f64>` authored in the inspector (7E-2's Actor section). `get_stat(id,
key)`. `enemy_rat.rhai` and `enemy_boss.rhai` collapse into one
`enemy.rhai` reading `hp`, `atk`, `awareness_range`, `glyph`, `tint` from
stats. This is the first real test of "content is data, behaviour is one
script per role."

#### `[ ]` 7.5-5 — `set_script` and `on_load`

`set_script(id, path)` attaches a script to a spawned entity (compiles via
the AST cache; the entity's `on_start` runs at the next step boundary).
`on_load(id, ctx)` lifecycle hook runs for every scripted entity after a
save is loaded, **instead of** `on_start`, so scripts can re-derive
presentation-only state without resetting gameplay state. `player.rhai`
loses its `on_update` lazy-init. The shooter director shrinks by the bullet
and enemy blocks that become per-entity scripts.

#### `[ ]` 7.5-6 — Engine-side solid resolution for all actors

`resolve_solid_collision` and wall sliding apply to any entity with an
`Actor` (not only `Controller::Local`); a `physics: bool` on `Actor`
opts out. The director's hand-rolled wall-slide and bullet hit tests are
deleted. `get_path` gains `diagonal: bool` and a `reachable_within(id,
budget)` query (tactical RPG need from the old plan's open question 4).

#### `[ ]` 7.5-7 — Animation and turn model completeness

`is_animating(id)` returns the truth (whether `PlayState`'s queue holds an
animation for `id`; plumbed via `StepInput`). `TurnModel::Energy` and
`ActionCost` wired: `Actor.speed` becomes live, `Command.cost` is honoured,
project setting selects the model. Regression test: a speed-200 actor acts
twice per speed-100 actor's turn.

#### `[ ]` 7.5-8 — Timers (D22)

Distinct `TimerState { Running(f32), Fired, Cancelled, Consumed }` replaces
the float sentinel. `timer_done` returns `true` exactly once. Test in
`timer_tests.rs`.

#### `[ ]` 7.5-9 — Sim boundary lints and `LevelSource`

`ember2d-sim/clippy.toml` with `disallowed-methods` for `std::fs::*`,
`Path::exists`, `Instant::now`, `SystemTime::now`, `eprintln`, and
`disallowed-types` for `HashMap`/`HashSet` (allow-listed per lookup-only
site). `Simulation` gets `level_source: Box<dyn LevelSource>` (`fn
load(&self, path) -> Result<LevelData>`, `fn exists`); `ember2d` supplies
the filesystem implementation, tests supply an in-memory one. Diagnostics
that were `eprintln!` become `StepOutcome.diagnostics: Vec<Diagnostic>`.
`get_global_position`'s cycle bail-out becomes a `Diagnostic` and
`set_parent` rejects cycles up front. `despawn` clears children's `parent`.

#### `[ ]` 7.5-10 — Scripting engine internals

Delete the dead `scopes` map (R22). `PassArgs` struct replaces the 11–16
positional arguments on the five `run_*` methods; the five copy-pasted
`call_fn` error blocks become one helper. `WorldSnapshot` stores collider
`layer`/`mask` as `Rc<str>`/`Rc<[Rc<str>]>` like tags. `from_snapshot` stops
rebuilding `extra_spawns`. `run_collisions` reuses the step snapshot instead
of rebuilding. Spatial queries (`get_entity_at`, `is_solid_at`, `raycast`,
`get_path`) use a per-step sorted-by-x index shared with the broad phase.
Measure with `bench_sim`; record.

#### `[ ]` 7.5-11 — Audio

`AudioEngine` moves from `PlayState` to `Engine` (one device stream for the
app's lifetime; music survives level transitions). Decoded `StaticSoundData`
cache keyed by path. `play_sound_at` gains stereo panning from the camera's
horizontal offset (presentation-only). `music_started` global in the
roguelike is deleted.

#### `[ ]` 7.5-12 — Node-graph codegen hardening

String literals and identifiers escaped (`rhai` string escaping; identifiers
validated against `[A-Za-z_][A-Za-z0-9_]*` with a UI error otherwise).
`add_edge` rejects cycles; `gen_exec_chain` carries a visited set. Typed
literal nodes: `IntLit`, `BoolLit`, `StringLit`; unconnected ports default
per the port's declared type. Variables declared at function top, not
inside branches. Concatenation with a tile's own script becomes a compile
error surfaced in the editor rather than a silent override. **Tests:**
property-style — random graphs of the supported node kinds always produce
Rhai that `compile_str` accepts.

#### `[ ]` 7.5-13 — Rhai `no_module` re-evaluation

Decide (§7.4) whether to enable Rhai modules so scripts can `import` a
shared `common.rhai`. Cost: AST cache and hot-reload need module
resolution; determinism is unaffected. Benefit: ends copy-paste across
scripts for good. If yes, ship `demos/roguelike/scripts/common.rhai`.

**Phase 7.5 gate:** §0.5; both demos rewritten to use the new primitives and
**smaller** than before (record line counts); `API_VERSION` 7 documented
with a migration table; tag `v0.5.8`.

---

### 5.7 `[ ]` Phase 8 — Tilemap, assets, animation authoring

**Purpose.** The old Phase 8 (tileset importer, clip editor) plus the one
data-model change that moves the entity ceiling by an order of magnitude.

#### `[ ]` 8-1 — `Tilemap` component (decision gate §7.2)

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

#### `[ ]` 8-2 — Tileset importer
Slice a PNG into a grid, name regions, write `project/assets/tilesets/*.ron`.
Sprite thumbnails in the palette (unblocked by 7D).

#### `[ ]` 8-3 — Sprite animation editor
Build clips, scrub frames, preview looping; clips serialised to the project
(today they are runtime-only), referenced by name from `SpriteSource::Clip`.

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
| floor2 p50 ms/step (release) | ≤ 2.0 ms | 1.817 |
| floor2 allocs/step | ≤ 7,000, not growing with entity count after 8-1 | 6,598 |
| 200×200 tilemap + 50 actors (after 8-1) | 60 fps debug | n/a |
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

**Decision:** *(pending)*

### 7.2 Tilemap component — decided at start of Phase 8

**Build 8-1 if** any target game (RPG demo, or the game being built with a
friend) needs maps over ~5,000 tiles, or if `bench_sim` shows
`WorldSnapshot::build` above 1 ms at that size. Otherwise defer to Phase 11
and proceed to 8-2.

**Decision:** *(pending)*

### 7.3 Rollback (10-5) — decided at end of 10-4

Only if a realtime game needs online play. Turn-based lockstep ships first
regardless.

### 7.4 Rhai modules — decided at 7.5-13

Enable if `or_zero`-style duplication survives 7.5-2/7.5-3 in any shipped
script. The `no_module` feature currently exists for build size and
simplicity, not determinism.

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

**Per step:** `cargo build --workspace --examples`; `cargo test --workspace`;
the step's own named tests; manual smoke test named in the step;
`git diff --stat` matches the step's **Scope**.

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
| `v0.5.7c` | Phase 7C | | | | | |
| `v0.5.7d` | Phase 7D | | | | | |
| `v0.5.7` | Phase 7E | | | | | |
| `v0.5.8` | Phase 7.5 | | | | | |
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
| Tilemap format change corrupts levels | v3 loads losslessly; baking is an explicit editor action; integrity test covers a baked level |
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
  drifting unnoticed.

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
