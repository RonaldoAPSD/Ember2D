# Ember2D — Refactor Plan

**Written against:** the `gemini` branch, v0.5.0
**Supersedes:** the earlier version of this plan, which was written against `main` (v0.3.4) and was wrong about roughly half the codebase
**Companions:** `ember2d-regression-checklist.md`, `ember2d-scripting-api.md`

---

## 1. Goals

1. **One renderer, world-space.** ASCII is not a mode. A glyph is a textured quad; a sprite is a textured quad. Both go through the same pipeline, and an ASCII project can place sprites in the same scene.
2. **General purpose.** No engine code branches on project type. Every capability is available to every project.
3. **Two time models.** Realtime and turn-based, switchable in project settings.
4. **Full in-engine authoring.** Levels, scripts, assets, animation — without leaving the editor.
5. **Multiplayer-ready seams.** Netcode is not built here, but the simulation is shaped so it can be added later (§5).
6. **Learning is a first-class goal** — except where a dependency removes work that has already caused burnout once (§6).

### Non-goals
3D · physics beyond AABB · shipping netcode · mobile/console · API backwards compatibility (0.x, one user — break cleanly).

---

## 2. Where the code actually is

`gemini` is two minor versions ahead of `main` and far more capable than the previous plan assumed.

### Already built

| Area | State |
|---|---|
| Window & input | `winit` 0.29 via `pump_events`, so the loop stays a normal `loop {}`. Own `Key` and `MouseButton` enums. Gamepad support through `gilrs`. |
| GPU | `wgpu` 0.19. `RenderBackend` trait, `WgpuBackend`, instanced quads, batching by texture, font atlas generated at startup, texture cache, scissor rects, dynamic instance buffer growth. |
| Engine loop | Single `run()`. Fixed-timestep accumulator (`SIM_DT`, `MAX_SIM_STEPS = 8`) for realtime; turn mode gated on `turn_triggered`. State stack with push/pop/pause/resume. |
| World | Full `Serialize`/`Deserialize`. Entity **parenting** with `get_global_position`. Collision layers + masks. |
| Save system | `SaveState` — world + persistent globals + level path, round-tripped through RON. |
| Scripting | ~95 registered functions across `scripting/api.rs`. Includes globals, persistence, timers, RNG, spatial queries, raycast, A\* pathfinding, camera, mouse, gamepad, HUD widgets, particles, spatial audio, save/load. |
| Project settings | `visual_style` (ClassicASCII/Sprites2D), `gameplay_loop` (RealTime/TurnBased), `start_level` — all persisted and wired in `main.rs`. |
| Editor | Layer-aware grid (`HashMap<(x,y,layer)>`), float zoom, smooth scroll, HSV colour picker, context menus, modals, in-engine script editor, savable palette, native file dialogs via `rfd`. |

**This means the old plan's Phase 3 (winit), most of Phase 4 (wgpu), all of Phase 0.5 (scripting API), the realtime half of Phase 6, and the loop-dedup cleanup are done.** Open question 5 from the old plan — scene graph vs flat list — is answered: you have a hierarchy.

### Not built

**Everything above wgpu still thinks in character cells.**

- `draw_char(x: usize, y: usize)` takes cell coordinates. The ortho projection is `0..width` in cells. There is no world-space draw call and no camera inside the renderer — `PlayState` computes an integer camera offset and subtracts it before drawing.
- `Sprite` is glyph-primary. `texture: Option<String>` is a path re-resolved every draw. The call site is `draw_texture(col * 8, row * 16, t, 32.0)` — magic scale, no src rect, no size, no rotation, tint forced to white, and texture sprites skip the bounds culling glyphs get.
- Animation is `frames: Vec<char>` only. No sprite-sheet frames.
- `VisualStyle::Sprites2D` exists as an enum variant. Nothing constructs a real 2D pipeline behind it.

### Two rendering details that will bite

- **Colour space mismatch.** The font atlas is `Rgba8UnormSrgb`, loaded textures are `Rgba8Unorm`, and the surface deliberately selects a non-sRGB format. Glyph and sprite colours will not match.
- **Batching is order-dependent.** `ensure_batch` only merges *consecutive* same-texture runs. A z-sorted list alternating glyphs and textures degenerates to one draw call per sprite.

---

## 3. Defects found in review

Concrete bugs, not design opinions. Phase 1 clears most of them.

| # | Defect | Location |
|---|---|---|
| D1 | **Input edge detection breaks under fixed timestep.** `poll_events` clears input once per frame, but the accumulator can run `update` up to 8 times that frame with the same `just_pressed` set — *and* zero times on a light frame, silently dropping the press entirely. Duplicates and drops, both directions. **Resolution: buffer until consumed (§4.1).** | `engine.rs` |
| D2 | **`set_persistent` in `on_start` is discarded.** `do_on_start` passes a local `HashMap` commented "Dummy" and drops it. | `play/spawn.rs` |
| D3 | **RNG is nondeterministic in three places.** `SmallRng::from_entropy()` for scripts, plus a fresh entropy-seeded RNG allocated *every frame* for particles and again for camera shake. | `scripting/engine.rs`, `play.rs` |
| D4 | **Trigger colliders default to layer `"solid"`** when `collider_layer` is empty, corrupting mask filtering and `is_solid_at`. | `play/spawn.rs` |
| D5 | **Equal-z draw order is nondeterministic** — `sort_unstable_by_key` over HashMap iteration order. Reads as flicker; breaks replay. | `play.rs` |
| D6 | **The editor obeys the project's `gameplay_loop`.** A TurnBased project changes how the *editor* updates. The editor should have no time model. | `main.rs`, `engine.rs` |
| D7 | **Turn mode integrates physics with `dt = 1.0`** — one full second of velocity per turn, unrelated to any scheduler. | `engine.rs` |
| D8 | Hot-reload of one script clears **all** entity scopes. | `scripting/engine.rs` |
| D9 | A script that errors keeps being called and failing every frame; only the log is suppressed. | `scripting/engine.rs` |
| D10 | `spawn_entity` hardcodes white, z=2, and a 1×1 trigger collider. | `scripting/api.rs` |
| D11 | **Allocation churn per frame:** `globals.clone()` + `persistent.clone()` twice; a ~12-HashMap `ScriptState` snapshot up to 3×; every collider's `layer: String` and `mask: Vec<String>` cloned in both `ScriptState::from_world` and `detect_collisions`; timers scanned by string prefix with `.replace()` per entity per pass. | `scripting/engine.rs`, `world.rs` |
| D12 | `layer != "locked"` — a magic string gating level exits. | `play.rs` |
| D13 | Texture sprites bypass viewport culling (`continue` before the bounds check). | `play.rs` |
| D14 | Colour-space mismatch between font atlas and textures (§2). | `renderer/backend.rs` |

---

## 4. Target architecture

### 4.1 Input buffering — resolution to D1

**The problem is two-sided.** Edge-triggered input (`just_pressed`) is produced per *frame*, but consumed per *simulation step*, and those cadences don't match. On a heavy frame the accumulator runs several steps and every one sees the same press. On a light frame it runs zero steps and the press is cleared by the next `poll_events` without any step observing it. Duplicates and silent drops, from the same root cause.

**Approach chosen: buffer until consumed.**

A press enters a pending set with a short lifetime. The first simulation step to run consumes it and clears it. If no step runs that frame, it survives into the next. Continuous state (`is_held`) is untouched — it's correct at any cadence and needs no buffering.

```rust
struct BufferedInput {
    pending: HashMap<Key, f32>,   // key → seconds remaining in the buffer
}
// poll_events:  insert/refresh pressed keys with BUFFER_WINDOW
// each sim step: consume → the step's just_pressed set, then remove
// each frame:    decay remaining entries, drop expired
```

`BUFFER_WINDOW` around 100–150ms. Same treatment for mouse buttons and gamepad buttons.

**Why this over the alternatives.** Unity and Bevy split the reads — edge input is only valid in the per-frame update, and you latch a bool for the fixed step. That would mean splitting script callbacks into per-frame and per-step variants: a large API change for a bug fix. Godot tags each press with the frame index for both process and physics counters, which solves duplicates cleanly but not drops. Buffering fixes both.

**Two things it buys beyond correctness:**

- A buffer window *is* jump buffering and coyote time — the input forgiveness players feel as responsiveness in platformers and action games. You get real game feel from the bug fix.
- **Turn-based mode needs it.** A keypress may have to wait several frames for the player's turn to come around. Without a buffer, turn-based input would feel like it drops presses constantly.

**Documented semantics for scripts:** `just_pressed(key)` is true in exactly one simulation step per physical press, no matter how many steps run that frame, and no press is lost to a frame that ran zero steps. Write this in the API spec.

**Edge case to handle:** a press and release inside the same frame must still register. Since the buffer records the press independently of `is_held`, that works — but test it, because it's the case a naive implementation misses.


```
Game code / Editor / Scripts
        │  emit draw commands in WORLD or SCREEN space
        ▼
   DrawList  ── sort by (space, layer, z, texture) → batch
        ▼
   Camera (world→screen)      ← new
        ▼
   RenderBackend  ── WgpuBackend (exists, extended)
```

### The command

```rust
pub struct DrawCommand {
    pub texture: TextureId,
    pub src:     Rect,      // pixels in source texture
    pub dest:    Rect,      // world units, or screen pixels if space == Screen
    pub rotation: f32,
    pub tint:    Color32,
    pub layer:   i32,
    pub z:       f32,
    pub space:   Space,     // World | Screen
}
```

`SpriteInstance` in `backend.rs` is already 90% of this — it has position, size, uv_offset, uv_size, two colours and a mode flag. What's missing is rotation, and the fact that positions arrive in cells rather than world units. **This is an extension of existing code, not a rewrite.**

### Coordinate spaces — write these down before coding

| Space | Unit | Used by |
|---|---|---|
| World | float units | entities, tiles, colliders |
| Camera | world + position/zoom | one per viewport |
| Screen | physical pixels | final output |
| UI | logical pixels, DPI-scaled | editor chrome, HUD |

In an ASCII project one world unit is one cell and zoom is constrained to integers so glyphs stay crisp. That is a **project setting**, not an engine mode.

### The cell-height question

`font8x8` is genuinely 8×8 — square. `CELL_H = 16` doubles it. On the GPU this is now a UV stretch rather than a CPU blit trick, and apparent size is a camera concern. **Rasterize at true 8×8 and let world units be square**, so physics behaves identically on both axes. This matters the moment anyone builds a platformer.

### Sprite

```rust
pub struct Sprite {
    pub source:  SpriteSource,
    pub tint:    Color32,
    pub size:    Option<Vec2>,   // None = natural size via pixels-per-unit
    pub layer:   i32,
    pub visible: bool,
}

pub enum SpriteSource {
    Texture { id: TextureId, src: Option<Rect> },
    Glyph   { ch: char, bg: Option<Color32> },
    Clip    { id: ClipId },
}
```

Unify at the render layer, keep meaning at the data layer. A bare `TextureId` + `Rect` would make the inspector show atlas pixel offsets, store those offsets in level files (so changing fonts corrupts levels), and make `set_glyph` impossible.

### Animation

Split the clip (authored, shared) from playback state (per-entity):

```rust
pub struct AnimationClip { pub frames: ClipFrames, pub fps: f32, pub looping: bool }
pub enum ClipFrames {
    Rects  { texture: TextureId, frames: Vec<Rect> },
    Glyphs { frames: Vec<char> },      // torch flicker, spinning coins
}
pub struct Animator { pub clip: ClipId, pub frame: usize, pub elapsed: f32, pub playing: bool, pub speed: f32 }
```

The existing `frames: Vec<char>` / `frame_rate` / `frame_timer` on `Sprite` migrates into `ClipFrames::Glyphs` + `Animator`. Static tiles — most of a tilemap — then carry no animation state at all.

### Turn scheduling

```rust
pub enum GameplayLoop { RealTime { }, TurnBased { model: TurnModel } }
pub enum TurnModel { Alternating, Energy, ActionCost, Declared }
```

One priority queue keyed by time: an actor acts, the action costs time, the actor is reinserted at `now + cost`. `Alternating` is that queue with every speed and cost at 100. Ship only `Alternating` exposed; define the rest.

Three structural requirements, and they are the actual work:
1. **The scheduler suspends** on a player-controlled actor and waits for a command while rendering continues.
2. **Animation is separate from simulation.** A turn resolves instantly in the sim; actions emit animation events that play over real time while the sim waits. Skip this and turn-based feels broken.
3. **`trigger_turn` today is a global flag**, not a per-actor scheduler. It's a placeholder, not a foundation.

---

## 5. Networking

**Target: 2-player online.** That's the friendliest case in networking — one player hosts, the other connects. No dedicated server, no matchmaking, no interest management, no server costs. Most netcode difficulty scales with player count, and at two it stays small.

### 5.1 Two models, and the turn-based one is nearly free

**Turn-based (tactical RPG, turn-based RPG).** A turn is a committed command batch. Send the batch, apply it on both sides. Latency doesn't matter because nobody is waiting on frame-perfect timing. Once Phase 5's command layer exists, this is a small amount of additional work.

**Realtime (platformer, action).** A remote player's input arrives 30–80ms late and you can't wait for it. The standard answer at 2P is **rollback**: predict the remote input, simulate forward, and when the real input arrives, restore to that frame and re-simulate. It's what fighting games use and it's specifically strong at two players.

**Sequencing consequence:** ship networked turn-based first. It validates the whole stack — transport, command serialization, determinism — on the easy case, and it's the model your tactical RPG wants anyway.

### 5.2 Determinism requirements

Both models require both machines to produce identical state from identical inputs. Three current hazards:

**H1 — Nondeterministic iteration order.** `detect_collisions` builds its collidable list by iterating `HashMap`, whose order varies *between processes*. Two machines with identical inputs would emit collision events in different orders, resolve them differently, and desync within seconds. **Every iteration in the sim must be deterministic** — `BTreeMap`, or collect-and-sort by `EntityId`. This is the most likely cause of a desync you'd otherwise spend a week chasing.

**H2 — Transcendental math.** IEEE 754 makes `+ - * /` and `sqrt` reproducible across platforms; `sin`, `cos`, `atan2`, `exp`, and `powf` are **not** — they come from platform libm and differ. Current offenders: `get_angle_to` (`atan2`), `get_distance` (`sqrt`, safe), the camera lerp (`exp`), and `Vec2::normalized`. Options, in increasing order of effort: restrict sim math to the safe set; ship your own lookup-table trig; or move the sim to fixed-point. Decide before Phase 6.

**H3 — Ambient randomness and time.** Already tracked as D3. The RNG must be world-owned and seeded, and both machines must start from the same seed.

**Determinism is testable without a network.** The replay test from Phase 5 — same commands plus same seed produces identical state — is exactly the desync test. Run it in CI.

### 5.3 Snapshot performance

Rollback saves and restores world state every frame. `SaveState` serializes through RON, which is orders of magnitude too slow for that. You need a **binary snapshot path** — `bincode` or hand-rolled — alongside the human-readable save format. Target: sub-millisecond for a few thousand entities. Turn-based doesn't need this; rollback can't work without it.

### 5.4 The five seams

Structural constraints that make netcode possible later. All of them improve save/load, replay, and testing regardless.

1. **Sim steppable headless.** `step(dt, &[Command])` with no renderer, window, or input dependency.
2. **Input becomes commands, per actor.** Today scripts read the keyboard and write velocity in one breath, and the engine assumes a single player (`player_id`, one `PlayerRecord`, one `spawn_point`, one `camera_entity`). Commands must carry which actor they belong to. This is the same change that makes local co-op work.
3. **World fully serializes.** ✅ Already done via `SaveState` — needs a binary path added (§5.3).
4. **Entity IDs survive multiple authorities.** `EntityId` is `u64` monotonic with no generation. Move to `{ index: u32, generation: u32 }`, and allocate spawned IDs from authority-prefixed ranges so host and client can't collide.
5. **Randomness and time route through the sim.** Currently violated three ways (D3).

### 5.5 Enforcement

These must survive months of single-player work that never rewards keeping them. Make the compiler do it — split into a workspace:

```
ember2d-sim/     world, components, commands, step(), rng, serialization   (no renderer/window/input)
ember2d/         engine loop, renderer, wgpu, winit, audio  → depends on sim
ember2d-editor/  editor, viewport, chrome                   → depends on both
```

Constraint 1 becomes a build error rather than a guideline. Second benefit, felt sooner: a sim crate with no window dependency is **testable**, and you currently have zero tests largely because everything needs a display.

### 5.6 Transport, and the thing that's actually hardest

**Not a plugin.** Rust has no stable ABI. Use a Cargo feature plus a `Transport` trait.

```rust
trait Transport {
    fn send(&mut self, to: PeerId, msg: &[u8]);
    fn poll(&mut self) -> Vec<(PeerId, Vec<u8>)>;
}
```

**Build `LoopbackTransport` first.** Two simulations in one process, with configurable artificial latency, jitter, and packet loss. Essentially all netcode development and debugging happens here, on one machine, before a socket exists. This is the single highest-leverage decision in the networking work.

**NAT traversal is usually harder than the netcode.** Two players behind home routers cannot simply connect. Realistic options: Steam networking (free relay and punch-through, but ties you to Steam), a WebRTC crate such as `matchbox`, or running your own relay. Budget real time here — it surprises people who assumed the simulation was the hard part.

---

## 6. Editor UI — the open decision

**The viewport is hand-written in every option.** Tile grid, camera, selection, painting, picking, gizmos — built on the engine's own renderer. That's where the engine-specific learning is, and it is never outsourced.

The question is only **chrome**: menu bar, dockable panels, text fields, file dialogs, scroll areas, DPI.

The picture has changed since the last version of this plan. You have already built a lot of chrome that works — panels, context menus, modals, a colour picker, a script editor. The old argument ("egui deletes the thing that burned you out") is weaker now, because much of it is done and the `Issues.txt` list it was aimed at is largely resolved.

**Revised recommendation: keep your own chrome, but put it behind an `EditorUi` abstraction during Phase 7.** Reasons: the sunk work is real and functional; egui would mean discarding a working script editor and palette editor; and your roadmap's V0.5.3–V0.5.9 items are exactly chrome polish you seem to want to do. The abstraction keeps the escape hatch open if panel layout becomes a sink again.

**The risk to watch:** the original burnout came from this exact work. If Phase 7 starts consuming sessions without visible progress, that's the signal to reconsider, not a reason to push harder.

---

## 7. Phases

Each phase ends with the engine **running**. Tag a release at each boundary.

### Phase 0 — Consolidate and baseline
**Nothing else starts until this is done.**

- **Decide which branch is trunk.** `gemini` is v0.5.0; `main` is v0.3.4. Merge `gemini` into `main`, or rename. Running a multi-phase refactor with a stale default branch will cause a lost-work incident.
- **Recover the demo content.** `demo/` — `level1/2/3.level` and the four `.rhai` scripts — exists **only on `main`**. `gemini` has just `tesst/`, whose `project.ron` points at a nonexistent `main.level`, plus an empty `a.rhai`. Port `demo/` forward and confirm it loads under v0.5.0's format. Without it there is nothing to regression-test.
- Run `ember2d-regression-checklist.md` against the current build; mark every item.
- Record a screen capture of the editor working.
- Tag `v0.5.0-pre-refactor`.
- Move §3 defects into GitHub issues.

**Done when:** one trunk branch, demo levels load and play, checklist marked, tag pushed.

---

### Phase 1 — Defect sweep
Low risk, no architecture change, immediately makes everything feel more solid. Also a gentle re-entry after time away.

- **D1 input buffering** — the important one. Implement buffer-until-consumed per §4.1: pending set with a decay window, consumed by the first sim step, surviving frames that run zero steps. Covers keyboard, mouse, and gamepad. Test the press-and-release-within-one-frame case.
- **D6** — the editor stops obeying `gameplay_loop`. Editor always runs realtime.
- **D2** persistent-in-`on_start`; **D4** trigger layer default; **D5** stable draw order via `EntityId` tiebreak; **D13** texture culling; **D8** per-script scope clearing; **D9** disable failed scripts; **D10** `spawn_entity` parameters; **D12** replace `"locked"` with a real flag.
- **D3 RNG** — seed from the level/world, remove the two per-frame `from_entropy()` allocations. This is also §5 constraint 5.
- **D14** colour space — pick one format and convert at load.

**Done when:** every §3 defect except D7 and D11 is closed, checklist passes.

---

### Phase 2 — World space and camera
The core of the refactor, and the thing standing between you and real 2D.

- `DrawCommand`, `DrawList`, `Space`, `Camera { position, zoom, viewport }`.
- Extend `SpriteInstance` with rotation; extend the shader.
- Renderer gains world-space entry points; existing `draw_char(cell, cell)` becomes a thin screen-space helper so the editor keeps working unchanged.
- Sort by `(space, layer, z, texture)` **before** batching, so `ensure_batch` stops degenerating.
- Move camera out of `PlayState` into a real `Camera`; delete the integer offset subtraction and the `+1` HUD-row fudge that leaks into `get_mouse_world_y`.
- Rasterize the font atlas at true 8×8; world units become square.

**Done when:** play mode renders through the camera at arbitrary float zoom, the editor is unchanged, checklist passes.

---

### Phase 3 — Sprite and asset model
- `SpriteSource` per §4; `Sprite::glyph()` and `Sprite::texture()` constructors.
- `TextureId` handles replacing per-draw path strings; `AssetManager` returns handles.
- `AnimationClip` / `ClipFrames` / `Animator`; migrate existing `Vec<char>` animation into glyph clips.
- **Level format v2**: add a `version` field (there is none today), convert cell coords to world units, store clip references by name.
- `pixels_per_unit` in `ProjectData`.
- Migrate embedded `graph:` fields — generate the Rhai once, write it beside the level, drop the field so no level file carries editor types.

**Done when:** a glyph and a PNG sprite render in one scene with correct sizes and tints, a clip plays, v1 levels load, `World → bytes → World` still exact.

---

### Phase 4 — De-hardcode play mode
`PlayState` is still a specific game: `PLAYER_SPEED`, WASD, corridor snapping, `"item"`/`"chest"` collection, score, victory condition, `z_for_tag`, two fixed HUD bars.

Move each into scripts — `player_controller.rhai`, `collectible.rhai`, `hud.rhai`. The API to do this already exists.

- Replace `z_for_tag` with the authored `layer` field (already on `TileRecord`).
- Player collider size becomes a `PlayerRecord` field, not a hardcoded `0.75`.
- Camera follow becomes a script concern using the Phase 2 camera API.

**Done when:** `PlayState` contains no tag-specific strings, no movement code, no score; the demo plays identically from scripts.

---

### Phase 5 — Simulation extraction, commands, turn scheduler
Carries the §5 seams. Same restructuring, do it together.

- **Workspace split** (§5.5) — makes the rest of this phase checkable by the compiler.
- Headless `step(dt, &[Command])`.
- **Commands are per-actor**, not global: `Command { actor: EntityId, kind: CommandKind, cost: u32 }`. This is what makes 2P possible and is the same change local co-op would need. Cost defaults to 100 and is ignored under `Alternating`; actors carry `speed`.
- **Pluralise the player.** `PlayState::player_id` singular, one `PlayerRecord`, one `spawn_point`, one `camera_entity` — all assume exactly one player. Move to a collection of player actors, each with its own input source. Falls out naturally from the command layer.
- Turn scheduler as a priority queue; expose `Alternating` only. Fixes D7.
- Scheduler suspension on player-controlled actors.
- Animation event queue, decoupled from sim time.
- Interpolation between fixed steps for rendering.
- **Deterministic iteration everywhere in the sim** (§5.2 H1) — `BTreeMap` or collect-and-sort. Start here rather than retrofitting.

**Done when:** a recorded command list replayed against the same seed produces an identical world, byte for byte. That replay test is your first automated test *and* your desync test.

---

### Phase 6 — Performance and data-model hardening
- **D11** — kill the per-frame clone churn. Snapshot only what scripts read; borrow instead of clone; intern or index globals.
- **Collision layers become a bitmask** with names in project settings. Currently `String` compares run inside `raycast` and `get_path`'s inner loops.
- `EntityId { index, generation }` (§5.4 constraint 4), with authority-prefixed allocation ranges so host and client can't collide. Also fixes stale-handle reuse.
- **Transcendental math decision** (§5.2 H2) — restrict sim math to the reproducible set, ship lookup-table trig, or move to fixed-point. `get_angle_to`, `Vec2::normalized`, and the camera lerp are the current offenders. Decide here; retrofitting later is painful.
- **Binary snapshot path** (§5.3) — `bincode` alongside RON. Target sub-millisecond for a few thousand entities. Rollback is impossible without it; save/load benefits immediately.
- Timers move off scope-variable string scanning into a real per-entity store.
- Spatial hash for collision broad-phase, only if profiling says so.
- Component registration macro so `despawn`/`entity_ids` stop needing manual edits.

**Done when:** a 2,000-entity level holds 60fps, profiling shows no per-frame allocation proportional to entity count, and a full world snapshot round-trips in under a millisecond.

---

### Phase 7 — Editor: viewport panelization and chrome
Absorbs roadmap V0.5.3–V0.5.9.

- Viewport becomes a dockable, non-closable panel; master-fill layout (V0.5.3).
- Rulers, high-contrast selection, smooth camera — now trivial on the Phase 2 camera.
- Inspector 2.0 property grid, collapsible sections, inline widgets (V0.5.4).
- Asset preview and drag-and-drop (V0.5.5), tooltips and toasts (V0.5.6), command palette (V0.5.7), themes and layout profiles (V0.5.8), undo/redo audit and perf pass (V0.5.9).
- Chrome behind an `EditorUi` abstraction (§6).

**Done when:** the roadmap's V0.5.x items are closed and the checklist passes.

---

### Phase 8 — Asset and animation authoring
Roadmap V0.6.0.

- Tileset importer: slice a sheet into a grid, name regions, write to project assets.
- Sprite animation editor: build clips, scrub frames, preview looping.
- Both write the Phase 3 formats — which is why they come after it, not before.

---

### Phase 9 — Networked 2-player
Everything before this builds the seams; this is the first phase that ships netcode. **Can be pulled earlier — right after Phase 5 — if the tactical RPG is the game you build first.** Turn-based networking needs almost nothing from Phases 6–8.

**10a — Loopback and harness**
- `Transport` trait behind a `netcode` Cargo feature.
- `LoopbackTransport`: two sims in one process, with configurable latency, jitter, and packet loss.
- Command serialization; a `NetSession` that exchanges command batches per step.
- Desync detector: hash world state every N steps, compare, log the first divergent frame.

All of 10a runs on one machine. Most netcode debugging happens here.

**10b — Turn-based over the wire**
- Host/client roles; host is authoritative on turn order.
- Send committed command batches; both sides apply identically.
- Reconnect and resync via a full snapshot.
- **Done when:** two instances play a tactical-RPG level to completion with no desync at 150ms simulated latency.

**10c — Realtime rollback** *(only if the platformer needs online play)*
- Ring buffer of binary snapshots (§5.3).
- Predict remote input, roll back and re-simulate on arrival.
- Input delay tuning; rollback frame cap.
- **Done when:** two instances play a platformer level at 100ms latency with no visible correction under normal movement.

**10d — Real transport**
- Pick one: Steam networking, `matchbox`/WebRTC, or a self-hosted relay (§5.6).
- **Budget real time for NAT traversal** — it is usually harder than the netcode.

---

### Phase 10 — Presets and cleanup
- **Presets, not modes.** `VisualStyle` stops being a renderer switch and becomes initial project settings: zoom constraints, grid snapping, default sprite constructor, palette, tool defaults. Ship ASCII, Sprite, and Empty presets. If a preset ever needs an `if` inside the engine, it's wrong.
- Delete `rollback_position` (superseded by `resolve_solid_collision`).
- `PlayerRecord` becomes a normal entity (prefab groundwork).
- Doc comments reconciled with reality.

---

## 8. Where this conflicts with `roadmaptoV0.6.md`

Your roadmap does editor polish (V0.5.3–V0.5.9) **before** assets and animation (V0.6.0). This plan puts the renderer and sprite work first.

**The argument for renderer-first:** V0.5.3's viewport panelization, rulers, and smooth camera are all camera and coordinate-space features. Built on today's cell renderer they get rebuilt in Phase 2. Built after it, they're straightforward. Same for V0.5.5 asset preview — it previews assets that don't have a real model until Phase 3.

**The argument for your order:** editor polish is visible, satisfying, and low-risk, and momentum matters more than optimal sequencing for a solo project with a burnout history.

**Suggested compromise:** Phases 0–1 first regardless — they're cheap and they fix real bugs. Then, if you want editor work before the renderer, take V0.5.4, V0.5.6, V0.5.7, and V0.5.8 early (inspector, toasts, command palette, theming — none of them touch coordinates), and hold V0.5.3 and V0.5.5 until after Phase 2 and 3.

---

## 9. Notes for the AI implementing this

- **One phase per branch, one logical change per commit.**
- **Preserve the comment style.** This codebase is heavily commented as a deliberate learning artifact. When code changes, rewrite the comment to match — never delete it, never leave it describing behaviour that no longer exists.
- **The engine must run at the end of every phase.** If a phase can't land working, split it.
- **Run the regression checklist before declaring a phase done.** Until Phase 5 there are no automated tests.
- **Don't opportunistically refactor** outside the current phase; note it and move on.
- **Ask before changing public API shape** — scripts and level files depend on it.
- Suggested reading order: `lib.rs` → `engine.rs` → `world.rs` → `renderer/mod.rs` → `renderer/backend.rs` → `play.rs` → `scripting/api.rs` → the phase's targets.

---

## 10. Risks

| Risk | Mitigation |
|---|---|
| Branch divergence causes lost work | Phase 0, before anything else |
| No demo content on `gemini` to test against | Phase 0 ports `demo/` forward |
| Phase 2 touches every draw call | Keep `draw_char` working as a screen-space helper; editor stays untouched until Phase 7 |
| D1's fix changes script-visible input semantics | Semantics settled (§4.1) and documented in the API spec; verify against the demo scripts |
| Editor polish becomes another months-long sink | §6 — watch for sessions without visible progress |
| Netcode seams erode during single-player work | Workspace split makes violations a build error; the replay test in CI doubles as the desync test |
| Desync from nondeterministic iteration or platform libm | §5.2 — deterministic iteration from Phase 5, transcendental decision in Phase 6, hash-compare detector in Phase 9a |
| NAT traversal turns out to be the real cost | Phase 9d is scoped separately; loopback transport keeps everything else testable without it |
| Netcode competes with shipping a game | Phase 9a–b only; defer 9c rollback until a realtime game actually needs online play |
| Scope creep back to "engine does everything" | After Phase 4, let the game you're building with your friend decide what's next |

---

## 11. Open questions

1. ~~D1's rule: how should scripts see `just_pressed`?~~ **Resolved** — buffer until consumed. See §4.1.
2. What does the game with your friend need? That reorders Phases 6–10.
3. ~~Multiplayer model?~~ **Resolved** — 2-player online. Turn-based games get lockstep command exchange (Phase 9b); realtime gets rollback if and when a realtime game needs online (Phase 9c). See §5.
4. **Genre requirements not yet folded into phases.** Platformer needs acceleration and a grounded state on `Transform` (currently velocity-only Euler) plus one-way platforms, which need collision resolution to know movement direction. Tactical RPG needs weighted and 8-directional pathfinding, a "reachable within N movement" query, and the `ActionCost` or `Energy` turn model rather than `Alternating`. RPG mostly needs UI scaffolding — dialogue boxes with word wrap and scrolling text. All three want movement out of the engine, which is Phase 4.
5. Prefabs — deferred by earlier decision. `PlayerRecord` being special-cased is the same problem showing up early; Phases 5 and 10 touch it.
6. Keep `font8x8`, or move to a scalable font once the atlas is rebuilt?
