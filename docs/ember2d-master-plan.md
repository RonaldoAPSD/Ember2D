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
6. Both demos play: `cargo run -- roguelike/floor2.level`,
   `cargo run -- shooter/arena.level`.
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
| `ember2d-sim` | math, color, world, components, level, save, scripting, command, scheduler, graph, event, layers, simulation | serde, ron, rhai, rand only | ~6,900 |
| `ember2d` | engine loop, renderer (wgpu), font system, input/mouse/gamepad, audio (kira), play, project, camera, `sim.rs` per-step pump | `ember2d-sim` | ~7,300 incl. tests |
| `ember2d-editor` | level/script/graph editor, docking, start screen | both above | ~8,700 |
| `ember2d-app` | `main.rs` + Editor↔Play orchestration | `ember2d`, `ember2d-editor` | ~225 |

`roguelike/` and `shooter/` (demo projects) and `docs/` sit at the repo root.

### 2.2 Phase status

| Phase | Subject | Status |
|---|---|---|
| 0–4 | Consolidation, defect sweep, world-space camera, sprite/asset model, de-hardcoded play mode, roguelike demo | `[x]` — Appendix A.1–A.5 |
| 5 | Determinism pass, turn scheduler, on_input/on_turn, save/replay, workspace split | `[x]` `a016267` — A.6 |
| 5.5 | Headless `Simulation`, external commands, animation queue, CI | `[x]` `4e4da60` — A.7 |
| 6 | Performance and data-model hardening (14 steps) | `[x]` `a1db60f` — A.8 |
| 7 Parts 1–2 | Pixel-space `UiRect`/`UiFrame`, `Font` trait, glyph atlas, TTF | `[x]` `cf59f42` — A.9 |
| **7A** | Stabilisation sprint | `[ ]` — §5.1 |
| 7B | Renderer foundation | `[ ]` — §5.2 |
| 7C | Editor foundation | `[ ]` — §5.3 |
| 7D | Theme and restyle | `[ ]` — §5.4 |
| 7E | Editor features | `[ ]` — §5.5 |
| 7.5 | Scripting completeness | `[ ]` — §5.6 |
| 8 | Tilemap, assets, animation authoring | `[ ]` — §5.7 |
| 9 | Scene and UI layer + RPG demo | `[ ]` — §5.8 |
| 10 | Networked 2-player | `[ ]` — §5.9 |
| 11 | Presets, cleanup, 0.6.0 | `[ ]` — §5.10 |

### 2.3 Baseline numbers (at `cf59f42`)

| Metric | Value | Where measured |
|---|---|---|
| Tests | 158 unit + 38 integration, all pass | `cargo test --workspace` |
| Clippy | 0 errors, ~130 warnings | `cargo clippy --workspace --all-targets` |
| rustfmt | not applied (1,397 diff hunks) | `cargo fmt --check` |
| floor2 p50 ms/step | 1.817 ms (release) | `cargo run --release -p ember2d-sim --example bench_sim` |
| floor2 allocs/step | 6,598 | same |
| `LEVEL_FORMAT_VERSION` | 3 (shipped levels regenerated to v3 as of 7A-4) | `ember2d-sim/src/level.rs:298` |
| `API_VERSION` | 6 | `ember2d-sim/src/scripting/types.rs:25` |
| Registered script functions | 123 | `grep -c register_fn` |
| Files over 600 lines | 1 (`ember2d/src/play.rs`, 607) | `scripts/check.ps1` once it exists |
| Dependencies | wgpu 0.19.4, winit 0.29.15, kira 0.9.6, glam 0.25, rand 0.8, gilrs 0.10, rhai 1.24, fontdue 0.9 | `Cargo.lock` |

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
| D18 | S2 | Editor save scrambles tile order (HashMap) | `editor/grid.rs:202` | `[ ]` → **7C-6** |
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
| R18 | S2 | `receive_log` has zero callers; play-mode script errors never reach the editor console | `impl_state/mod.rs:527` | `[ ]` → 7C-7 |
| R19 | S2 | File › Start Screen orphans an `EditorState` on the state stack | `ember2d-app/src/app.rs:85`; `main.rs:63-67` | `[x]` 7A-2 — both `Transition::ToStart` arms in app.rs pop before returning; `Engine::push_state` debug-asserts depth ≤ 3 |
| R20 | S2 | `TilePalette::current()` indexes `[0]`; empty or out-of-range `selected` from a loaded palette panics | `palette.rs:284`; `text.rs:53, 84`; `input/mod.rs:75, 111` | `[x]` 7A-2 — invariant enforced at `TilePalette::load` (reject empty tiles, clamp `selected`); `current()` itself unchanged, see 7A-2's "Landed as" note |
| **Renderer / engine** | | | | |
| R21 | S2 | Non-integer cell projection: cells stretched unless window is an exact cell multiple; `scale_factor()` width-only; HiDPI mis-sized | `renderer/mod.rs:381-382`; `backend.rs:328`; `engine.rs:246` | `[ ]` → 7B-2 |
| R22 | S4 | `scopes` map is dead state (rhai rewinds scope; timers moved off it) yet maintained by hot-reload and despawn | `scripting/engine.rs` | `[ ]` → 7.5-10 |
| R23 | S3 | Frame pacing double-throttles (Fifo vsync + `thread::sleep` to 60) | `engine.rs:389-392` | `[ ]` → 7B-4 |
| R24 | S3 | Key repeat inconsistent: `repeat` flag ignored; letters repeat into text buffer, editing keys never repeat; Ctrl+S pushes "s" | `engine.rs:226-243` | `[ ]` → 7B-4 |
| R25 | S3 | `GamepadState::poll` ignores `Disconnected`; held buttons stick | `gamepad.rs:131-159` | `[ ]` → 7B-4 |
| R26 | S3 | GPU textures never freed; `AssetManager::clear` doesn't invalidate `texture_cache` | `backend.rs:124` | `[ ]` → 7B-3 |
| R27 | S3 | `draw_text_px` clones the 4 MB atlas `Texture` per call | `renderer/mod.rs:270` | `[ ]` → 7B-3 |
| R28 | S3 | Bottom world row never drawn (culled for a HUD bar removed in Phase 4) | `play/render.rs:106-108` | `[ ]` → 7B-4 |
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
| R39 | S4 | LICENSE placeholder; no `license`/`repository` in manifests; OFL text not bundled | `LICENSE`, `*/Cargo.toml` | `[ ]` → 7A-8 |
| R40 | S4 | No tags; `main` 26 commits behind; version 0.5.0 meaningless | git | `[ ]` → 7A-8, §9 |

### 3.3 Editor defects carried from the Phase 7 plan (E-series)

| # | Defect | Status |
|---|---|---|
| E1 | `draw_cursor_highlight` drifts during fractional scroll | `[x]` cf59f42 (pinned by test) |
| E2 | `grid_to_pixel` hardcodes 8.0/16.0 | `[~]` `CELL_W/H` exported; literals remain in `impl_render.rs:205-208`, `backend.rs:420-439`, `mouse.rs:28-32` → 7C-2 |
| E3 | `draw_extra_spawns` label position wrong at zoom ≠ 1 | `[x]` cf59f42 |
| E4 | Two sources of truth for the canvas rect (`Layout` vs `PanelManager`) | `[ ]` **not resolved** — `Layout.canvas_*` still rebuilt each frame from `Viewport.rect` → 7C-3 |
| E5 | Hitboxes computed independently of drawing | `[~]` fixed for panel chrome/tabs/menus/palette/inspector; **open** for confirm modal, colour picker, palette editor, context menu, graph palette, hierarchy rows, file browser rows, start screen → 7C-1 |
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

### 5.1 `[ ]` Phase 7A — Stabilisation sprint

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

#### `[x]` 7A-7 — Restore CI

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

#### `[ ]` 7A-8 — Hygiene

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

#### `[ ]` 7A-9 — rustfmt decision

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

**Phase 7A gate:** §0.5, then tag `v0.5.7a`.

---

### 5.2 `[ ]` Phase 7B — Renderer foundation

**Purpose.** Phase 7 Parts 3–4 (theme, 9-slice, integer UI scale) sit on
the renderer. Three things must be true of the renderer first: it is on a
supported wgpu, it maps cells to pixels 1:1, and the `Font` trait actually
draws. Doing these after the theme lands would mean redoing the theme.

**Checklist sections at gate:** §1, §2, §11.

#### `[ ]` 7B-1 — Upgrade wgpu and winit

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

#### `[ ]` 7B-2 — Integer cell projection and HiDPI

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

#### `[ ]` 7B-3 — Renderer resource hygiene

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

#### `[ ]` 7B-4 — Engine loop and input correctness

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

#### `[ ]` 7B-5 — Finish Phase 7 Part 2: text actually renders through `Font`

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

**Phase 7B gate:** §0.5, then tag `v0.5.7b`.

---

### 5.3 `[ ]` Phase 7C — Editor foundation

**Purpose.** Finish what Phase 7 Part 1 started, structurally: one rect per
widget, one canvas rect, one focus/mode state, a headless way to test input,
and an undo stack that batches the common case. This is also where the
egui decision gate (§7.1) is evaluated — at the **end** of 7C, with data.

**Checklist sections at gate:** §3–§10.

#### `[ ]` 7C-1 — `UiFrame` registration becomes mandatory

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

#### `[ ]` 7C-2 — No cell literals below `Panel.rect`

- **Why:** E2 remainder; the Part 1 done-criterion "no 8.0/16.0 in the
  editor" is not met.
- **Change:** `impl_render.rs:205-208` scissor and every `* 8`/`* 16` go
  through `ScreenMapping` (7B-2). `cells()` helper deduplicated
  (`types.rs:11`, `drawing.rs:12`).
- **Done when:** `grep -rnE "\b(8|16)\.0\b" ember2d-editor/src` is empty.
- **Scope:** `ember2d-editor`.

#### `[ ]` 7C-3 — Delete `Layout`; the viewport is a real panel

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

#### `[ ]` 7C-4 — `EditorMode` replaces the boolean soup

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
  `EditorFocus` from 7A-2 folds into this.
- **Test:** see 7C-5 — this step is what makes the harness possible.
- **Done when:** `EditorState` has no `bool` field whose name is a mode;
  `handle_update` has one `match self.mode`.
- **Scope:** `ember2d-editor`.

#### `[ ]` 7C-5 — Headless editor input harness

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

#### `[ ]` 7C-6 — `LevelGrid` determinism and undo batching (D18)

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

#### `[ ]` 7C-7 — Script errors reach the editor

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
scripts for good. If yes, ship `roguelike/scripts/common.rhai`.

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

**Decision:** *(pending)*

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
| `v0.5.7a` | Phase 7A | | | | | |
| `v0.5.7b` | Phase 7B | | | | | |
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
