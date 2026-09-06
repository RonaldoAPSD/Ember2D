# Phase 5 — Simulation extraction, commands, turn scheduler

**Status:** planned, not started. Written 2026-09-05 against `claude` @ `159007e`.
**Parent:** `docs/ember2d-refactor-plan.md` §5 (the netcode seams) and §7 Phase 5.
**Companions:** `docs/ember2d-scripting-api.md`, `docs/ember2d-regression-checklist.md`.

> Lives in the repo, unlike Phase 4's plan (which sat at
> `C:\Users\ronal\.claude\plans\memoized-discovering-glacier.md` and whose own
> handoff note worried about it going missing). Anything a future session needs
> is here.

---

## 0. How to use this document

The user works **one step at a time**: implement, build, test, smoke-test,
report, then wait for explicit confirmation ("next", "keep going") before
starting the next step. Do not chain steps. Commits happen only when the user
explicitly asks.

Each step below ends with **the engine running**. If a step can't land working,
split it rather than leaving a broken tree.

Before writing code for a step, re-read that step's section *and* §3 ("Engine
facts") — several of those facts are non-obvious and have already caused real
bugs.

---

## 1. What Phase 5 is actually for

Phase 5 carries the §5 netcode seams. The end state:

- The simulation can be stepped **headless**, with no window, renderer, or raw
  input — `step(dt, &[Command])`.
- **Input becomes per-actor commands**, so the engine stops assuming exactly one
  player, and a recorded/transmitted command stream fully determines the next
  state.
- A real **turn scheduler** replaces the `trigger_turn` global bool.
- **Iteration order is deterministic everywhere in the sim**, so two runs (or two
  machines) can't diverge.

**Done when:** a recorded command list replayed against the same seed produces an
identical world, byte for byte. That replay test is the first real automated
determinism check *and* the desync detector Phase 9 needs.

### 1.1 The architectural decision (settled — do not re-litigate)

**The script engine lives inside the sim. Commands are the input boundary, not a
game-logic layer.**

```
step(dt, commands):
  1. route commands to their actors
  2. run scripts (they read commands, never the keyboard)
  3. apply deferred writes
  4. collisions
  5. on_collide + apply
```

A `Command` is an actor id plus a **named action with parameters**. The sim routes
it; the *script* decides what it means. Rust never learns what "attack" is.

```rust
pub struct Command {
    pub actor:  EntityId,
    pub action: String,     // "move" | "attack" | "quaff" | ...
    pub params: Vec<f64>,   // [dx, dy] etc.
}
```

```rhai
fn on_input(id, ctx) {            // local actors only, while the scheduler waits
    if ctx.just_pressed("w") { ctx.submit(id, "move", [0.0, -1.0]); }
    if ctx.just_pressed("q") { ctx.submit(id, "quaff", []); }
}

fn on_turn(id, ctx) {             // the scheduler gave this actor a turn
    if ctx.command_action() == "move" {
        let dx = ctx.command_param(0);
        // is_solid_at / bump-to-attack — exactly the logic that's there today
    }
}
```

**Why this and not a typed `CommandKind` enum the sim applies itself:** the typed
version would move movement rules and combat resolution back into Rust, undoing
what Phase 4 just finished, and every new action kind would need a Rust change —
violating plan Goal 2 ("no engine code branches on project type"). The string
compare in the sim is a Phase 6 interning problem, not a correctness one.

**AI does not go through commands.** A rat's `on_turn` decides and acts directly.
For lockstep netcode only *player* commands are transmitted; AI must be
deterministic on both sides, which is what §2 (deterministic iteration) and the
seeded RNG already guarantee. Recording player commands + the seed is enough to
replay AI exactly.

### 1.2 Decisions already made (do not re-litigate)

| Decision | Resolution |
|---|---|
| Command model | Scripts interpret (§1.1). |
| Workspace split timing | **Last step of the phase** (5i), not first. Step 5a already fixes every bad dependency edge, so by then the split is mechanical and locks in the finished shape. The plan doc's second stated benefit — "headless tests" — is already obsolete: `TurnHarness` runs windowless today. |
| Animation event queue / interpolation | **Deferred** out of Phase 5. The plan's "skip this and turn-based feels broken" warning targets a tactical RPG where a unit walks several tiles; for a roguelike, instant snapping is correct and the user's playthrough confirmed it feels fine. |
| Camera stays in Rust | Already settled in Phase 4; see `docs/HANDOFF.md`. |

### 1.3 Explicitly out of scope

Animation events and inter-step interpolation · binary snapshot path (§5.3 —
turn-based doesn't need it, Phase 6) · `Energy`/`ActionCost`/`Declared` turn
models (define them, ship `Alternating` only) · `EntityId { index, generation }`
(Phase 6) · the transcendental-math decision, §5.2 H2 (Phase 6) · D11 clone churn
(Phase 6) · collision-layer bitmasks (Phase 6).

---

## 2. Cost to go in with eyes open

- **`LEVEL_FORMAT_VERSION` 1 → 2** and **`api_version` 4 → 6** both land here
  (5e bumps to 5, 5f bumps to 6). Update
  `docs/ember2d-scripting-api.md` §6's changelog table each time.
- **Every roguelike script gets rewritten** — `player.rhai`, `enemy_rat.rhai`,
  `enemy_boss.rhai`, `victory.rhai`. Likely *net smaller*: see §2.1.
- **3 of the 5 tests in `tests/roguelike_combat.rs`** encode the current turn
  cadence (`a_rat_acts_at_most_once_per_player_turn_even_across_many_idle_frames`,
  `two_adjacent_rats_each_contribute_their_own_damage_in_the_same_resolve`,
  `bump_attack_kills_a_rat_in_two_hits_and_each_hit_costs_a_turn`) and
  `tests/common/mod.rs`'s `TurnHarness` will need rewriting alongside 5d/5f.
- `trigger_turn()` is **removed** from the scripting API in 5f.

### 2.1 The simplification 5f should unlock

Today `enemy_rat.rhai` and `player.rhai` carry two elaborate workarounds that
exist *only* because every script runs every frame and writes defer to end of
pass:

- `acted_<id>` vs the shared `turn` global — manual turn-gating.
- `atk_turn_<id>` / `atk_dmg_<id>` single-writer damage publishing, plus
  `last_resolved_turn` and its "track the highest value actually applied" rule.

With a scheduler running **one actor per step**, each actor's writes commit at the
end of *its own* pass, before the next actor runs. Turn-gating becomes the
scheduler's job, and two rats attacking in sequence can each just read-modify-write
the player's hp directly. **Both mechanisms should delete.** If they don't, that
is a signal the scheduler design is wrong — stop and re-check rather than porting
the workarounds forward.

---

## 3. Engine facts a fresh session needs

Verified against the current source. Several were found the hard way in Phase 4.

**Deferred writes.** A `get_*` never observes a `set_*` from earlier in the same
script pass — including your own script called twice. `ScriptState`'s `pending_*`
queues only fold into the world in `ScriptEngine::apply_ctx` at end of pass. Guard
any lazy-inited value read back in the same call (the scripts use an `or_zero()`
helper).

**Two scripts writing the same global key in one pass silently last-write-wins.**
`ScriptState::pending_globals` is a plain map, not a merge structure. This is why
`enemy_rat.rhai` inits hp in `on_start` (a separate, earlier pass) rather than
`on_update`.

**Rhai `let` does not persist between calls.** `ScriptEngine::call_fn` uses default
`CallFnOptions`, whose `rewind_scope: true` discards script-declared variables.
The per-entity `Scope` persists only as somewhere the *engine* writes from outside
(timers, via `__timer_*`).

**Rhai has a max-expression-complexity guard** that bit three times in Phase 4 —
once from an 11-term `+` string concatenation, twice from nested `if`/`else` depth.
Fix by splitting into `+=` statements or pulling the block into its own `fn`.
**Treat "does it compile" as mandatory after any nontrivial script edit.**

**A script compile failure logs only to the in-game `script_log`, never to
stdout/stderr.** A clean background-launch log file does *not* mean the scripts
work. Verify with a throwaway `ScriptEngine::compile` check or by actually playing.

**Turn mode currently passes real wall-clock `delta_time` to scripts**
(`engine.rs`'s else-branch), so `get_delta`/`get_elapsed` are already
nondeterministic there. Nothing in the roguelike uses them. 5d fixes this.

**`Collider::new` hardcodes `solid: true`** regardless of `PlayerRecord.solid`, so
the player always blocks `is_solid_at`. The rat/boss generator comments rely on
this.

**`tests/common/mod.rs`'s `TurnHarness` hand-duplicates `engine.rs`'s per-step
sequence.** Get any piece wrong or out of order and it silently tests different
behavior than the real engine. Step 5d deletes this hazard.

**`in_viewport` (`play/render.rs`) still culls the last screen row** with a comment
about a "bottom HUD bar" that Step 4g deleted — the last row is where the script
log renders. Not a Phase 5 concern, but don't be surprised by it.

---

## 4. Step-by-step

### Step 0 — unrelated small bugs (optional, do first if at all)

Three one-line fixes, none of them Phase 5 work. Landing them first keeps them out
of later diffs.

1. `src/editor/input/shortcuts.rs` lines ~83-88 — `Key::G` and `Key::B` are each
   handled **twice**, so both toggles fire in one frame and cancel out. The
   physics-overlay and palette keyboard shortcuts currently do nothing. Delete the
   duplicates.
2. `Key::Tab` is advertised as "Grid" in both the help overlay
   (`editor/ui/panels.rs::draw_help_overlay`) and the View menu
   (`editor/ui/menu.rs`), but nothing binds it. Bind it to `self.show_grid`.
3. `src/editor/node_graph/codegen.rs:114` emits `ctx.spawn(...)`; the registered
   function is `spawn_entity`. Any graph containing a Spawn node generates a
   script that fails at runtime. Argument order is already correct — only the name
   is wrong.

Also fix `docs/HANDOFF.md`'s stale claim that Phase 4 is "DONE — NOT YET
COMMITTED". It is committed (`105604e`, `4857717`, `f04198e`, `159007e`).

**Done when:** builds, 74 lib tests pass, editor smoke test shows G/B/Tab working.

---

### Step 5a — layering cleanup

No behavior change. Cuts every dependency edge that would block the 5i workspace
split, while everything still lives in one crate.

The bad edges, all verified present today:

| Edge | Fix |
|---|---|
| `level.rs` → `editor::node_graph::NodeGraph` (`TileRecord.graph`) | Move `node_graph`'s **data + codegen** out of the editor |
| `play/spawn.rs` → `editor::node_graph::generate_graph` | same move — this edge disappears for free |
| `scripting/types.rs` → `play::ShakeState` | Move `ShakeState` into `scripting/types.rs` |
| `scripting/api.rs` → `play::ShakeState`, `play::HUD_TOP_ROWS` | same, plus delete `HUD_TOP_ROWS` |
| `components/sprite.rs`, `level.rs`, editor → `renderer::Color` | Move `Color` to a crate-root module |
| `components/animator.rs` → `renderer::TextureId` (`ClipFrames::Rects`) | Store a path `String` instead |

Concretely:

- **Move `src/editor/node_graph/{mod.rs, codegen.rs}` → `src/graph/{mod.rs,
  codegen.rs}`.** Leave the drawing/hit-testing half (`node_graph/ui.rs`) in the
  editor as `src/editor/graph_ui.rs`. Note `node_graph/mod.rs` currently does
  `pub use ui::*`, so the re-export split has to be untangled. Call sites to
  update: `level.rs`, `play/spawn.rs`, `editor/impl_render.rs`,
  `editor/impl_state.rs`, `editor/input/graph.rs`, `editor/helpers.rs`.
- **Move `src/renderer/color.rs` → `src/color.rs`** (with `DEFAULT_FG`/`DEFAULT_BG`).
  Keep `pub use crate::color::{Color, DEFAULT_FG, DEFAULT_BG};` in `renderer/mod.rs`
  so the ~100 existing `renderer::color::Color` import sites keep compiling — the
  renderer legitimately uses `Color`, so this re-export is honest, not a shim.
- **Move `ShakeState`** from `play.rs` into `scripting/types.rs`, where
  `ScriptUpdateResult` already owns it conceptually. `play.rs` imports it back.
- **Delete `HUD_TOP_ROWS`.** Its value is `0` and both call sites
  (`Camera::viewport_origin` in `play.rs::update`, and the subtraction in
  `scripting/api.rs::get_mouse_world_y`) are inert. If a future HUD needs to
  reserve rows, `Camera::viewport_origin` is already the mechanism — the constant
  adds nothing. Behaviorally a no-op; note it in the API doc anyway.
- **`ClipFrames::Rects { texture: TextureId }` → `{ texture: String }`**, matching
  `SpriteSource::Texture`'s path-not-handle convention (which exists because
  `TextureId` is runtime-assigned and doesn't survive save/load — see
  `renderer/texture.rs`'s own doc comment). Nothing constructs this variant yet.

**Done when:** `cargo build` clean, 74 lib + 19 integration tests pass, editor and
play smoke tests unchanged. Zero behavior change expected — if anything looks
different, something moved that shouldn't have.

---

### Step 5b — deterministic iteration (§5.2 H1)

The single highest-leverage step for everything downstream. Make iteration order a
property of the *types* rather than of discipline.

- **`World`'s six component stores: `HashMap<EntityId, T>` → `BTreeMap`.** This
  alone kills most of H1: `detect_collisions`'s collidable list, `find_by_tag`,
  and every `world.scripts.iter()` in `ScriptEngine` become sorted for free. RON
  serializes both as maps, so save files stay compatible — confirm
  `world.rs`'s existing `a_saved_world_from_before_the_animators_store_existed_still_loads`
  test (which parses a RON literal) still passes.
- `World::entity_ids()` — `HashSet` → `BTreeSet`, returns sorted.
- **`ScriptState`'s snapshot maps → `BTreeMap`**: `positions`, `colliders`, `tags`,
  `tag_to_ids`, `visibility`, `z_orders`, `animator_frames`. These drive
  `is_solid_at`, `get_entity_at`, `find_entities_in_rect`, `raycast`, and
  `get_path` — all of which currently pick "first hit" or break ties by iteration
  order.
- **`pending_globals` / `pending_persistent` → `BTreeMap`**, so same-key
  last-write-wins becomes deterministic instead of arbitrary (that's the exact 4h
  hazard from §3).
- `PlayState::globals`, `PlayState::clips`, `Engine::persistent` → `BTreeMap`.
  `SaveState.persistent`'s type changes; RON map format is unchanged so old saves
  still load.

`rhai::Dynamic` is not `Ord`, but that's fine — every one of these is keyed by
`String` or `EntityId`.

**Watch for:** `tests/roguelike_combat.rs`'s
`identical_input_sequences_produce_identical_state_across_independent_instances`
currently derives its power from two instances having *different* HashMap random
states. After this step that difference is gone, so the test gets weaker as a
hash-order probe even as the system gets stronger. Leave it (it still guards the
command layer later) but note the change in its comment.

**Done when:** all tests pass, plus a new unit test asserting `World` iteration is
id-sorted. Play the roguelike — behavior should be indistinguishable.

---

### Step 5c — D17: script state survives save/load

A prerequisite for 5h's replay test, not optional. Today `SaveState` holds
`world` + `persistent` only; all of the roguelike's per-entity combat state lives
in `ScriptEngine` globals (`hp_<id>`, `aware_<id>`, …) and is silently lost.

- Add `globals` and `clips` to `SaveState`, both `#[serde(default)]` so existing
  saves load.
- `AnimationClip` / `ClipFrames` need `Serialize`/`Deserialize` derives — possible
  now that 5a made `ClipFrames::Rects` hold a `String`.
- **Restore globals and clips on load; keep *not* re-running `on_start`.** This is
  the subtle part: `enemy_rat.rhai`/`enemy_boss.rhai`'s `on_start` sets
  `hp_<id>` **unconditionally** (deliberately — see §3), so re-running it on load
  would reset every enemy to full health. Restoring the saved globals instead is
  both correct and simpler. `PlayState::on_start`'s `is_loading_save` branch
  already skips `on_start`; it just needs to accept the restored maps.

**Done when:** a new test saves mid-combat (with `hp_<id>` set), loads, and asserts
the globals survived. Update `docs/ember2d-refactor-plan.md` §3's D17 row and
`docs/ember2d-regression-checklist.md` §13/§14 to mark it fixed.

---

### Step 5d — extract a real `step()`

Deletes the "the test harness re-implements the engine loop by hand" hazard, and
makes the sim steppable without a window.

`engine.rs::run()` has two inline branches (realtime accumulator, turn-gated) that
`tests/common/mod.rs::TurnHarness::frame` duplicates by hand. Extract the shared
per-step sequence — `consume_step` → `update` → `[integrate_physics →
detect_collisions → late_update]` → into one function in a new `src/sim.rs`, and
have **both** `engine.rs` and `TurnHarness` call it.

Keep the scope tight: this is extracting the *loop body*, not restructuring
ownership. `World` still lives on `Engine`; `PlayState` still owns the script
engine. 5f/5g tighten that.

Two behavior fixes to land here:

- **Turn mode must pass a fixed sim dt to scripts**, not wall-clock (§3). Split
  `UpdateContext`'s single `delta_time` into a fixed **sim dt** (scripts, physics,
  animators) and a real **frame dt** (camera lerp, shake timer — pure presentation,
  never read back by scripts).
- **Document that the camera is presentation, and `get_camera_x/y` /
  `get_mouse_world_x/y` are not replay-safe.** The camera lerp uses `exp()` on
  real frame time — a named §5.2 H2 cross-platform hazard — so any script branching
  on camera position is outside the deterministic set. Phase 6's H2 decision
  revisits this. Nothing in the roguelike reads them today. Add a note to
  `docs/ember2d-scripting-api.md` §3 under "Camera".

**Done when:** `TurnHarness` no longer contains a hand-written copy of the step
sequence, all tests pass, both smoke tests unchanged.

---

### Step 5e — commands and the input boundary

New sim-side types:

```rust
pub struct Command { pub actor: EntityId, pub action: String, pub params: Vec<f64> }
pub struct InputSnapshot { pub held: BTreeSet<String>, pub pressed: BTreeSet<String> }
```

`InputSnapshot` is plain data with no `winit` in it. `scripting/types.rs`'s
`snapshot_keys` (and its `KEY_MAP`) moves to the host boundary — `Key` and the
winit conversion stay engine-side, the snapshot goes sim-side.

New scripting API:

- `ctx.submit(actor_id, action, params_array)` — queues a `Command`. Meaningful
  only inside `on_input`.
- `ctx.command_action()` → `String`, `""` when the actor has no command.
- `ctx.command_param(i)` → `f64`.
- New lifecycle **`fn on_input(id, ctx)`**, called only for locally-controlled
  actors.

**Pass ordering matters.** `on_input` runs as its own pass, `apply_ctx` commits,
*then* the `on_update` pass runs with commands visible. Two passes per step. That's
required (deferred writes mean a command submitted in the same pass wouldn't be
readable) and it pre-stages 5f's shape exactly.

Before 5f introduces the `Actor` component, "locally controlled" just means the
player entity.

Keep `is_held`/`just_pressed` registered — realtime projects need them — but
document them as **not replay-safe**: only commands are.

Rewrite `player.rhai` (input block → `on_input`; movement/attack/quaff read the
command) and `victory.rhai`. Bump `api_version` 4 → 5 and add the changelog row.

**Done when:** the roguelike plays identically start to finish, all tests pass, and
`player.rhai` no longer calls `just_pressed` outside `on_input`.

---

### Step 5f — turn scheduler, `on_turn`, and D7

The big one. Consider splitting into 5f-1 (scheduler + `on_turn`, engine side) and
5f-2 (script rewrites) if it starts sprawling.

**New component** (`src/components/actor.rs`):

```rust
pub struct Actor { pub speed: u32, pub controller: Controller }
pub enum Controller { Local(u8), Ai, Remote(u8) }
```

**Level format v2.** `TileRecord` gains an optional actor record; the player is
implicitly `Local(0)` at speed 100. `LEVEL_FORMAT_VERSION` 1 → 2 with
`#[serde(default)]`, so a v1 level still loads: it gets a player actor and no AI
actors, which plays gracefully (nothing else takes turns) rather than hanging.
Update `examples/gen_roguelike.rs` — `rat()` and `boss()` become `Ai` actors — and
regenerate `roguelike/*.level`.

**Scheduler:**

```rust
pub struct TurnScheduler {
    now:   u64,
    queue: BinaryHeap<Reverse<(u64, EntityId)>>,  // EntityId breaks ties deterministically
}
```

- One actor per step. Pop earliest; if it's `Local` and has no queued command,
  return `StepResult::AwaitingCommand(actor)` so the host renders, polls input,
  and runs `on_input`. Otherwise run that actor's `on_turn`, apply writes,
  reinsert at `now + cost`.
- `Alternating` = every speed and cost 100. Define `Energy`/`ActionCost`/`Declared`
  in the enum; ship only `Alternating`.
- **Keep the per-step `on_update` pass for every scripted entity.** This is what
  lets `stairs.rhai` update its lock state and enemies run their death check
  between turns; `docs/ember2d-regression-checklist.md` §12 already records this
  as confirmed-intended behavior.
- **D7's fix: `integrate_physics` becomes realtime-only.** Turn mode stops calling
  it entirely (it currently integrates a full second of velocity per turn, which
  is meaningless for grid movement). Collisions still run after each turn's writes.
  Nothing in the roguelike uses velocity. Document in §3 of the plan doc and mark
  D7 fixed in the checklist's §14 table.
- **Remove `trigger_turn()`.** Add `ctx.get_turn_number()`, `ctx.get_speed(id)`,
  `ctx.set_speed(id, n)`, and `ctx.act(cost)` (override this action's cost).
  `api_version` 5 → 6.

**Script rewrites** — expect these to get *shorter* (§2.1):

- `player.rhai`: `on_turn` does move/attack/quaff; `turn`/`turns_taken` come from
  `ctx.get_turn_number()`; `last_resolved_turn` and the whole
  `resolve_incoming_damage` scan **delete**.
- `enemy_rat.rhai` / `enemy_boss.rhai`: `acted_<id>` gating and
  `atk_turn_*`/`atk_dmg_*` **delete**; `on_turn` chases and attacks (writing hp
  directly); `on_update` keeps only the death check and awareness tint.
- Rewrite `TurnHarness` to drive commands + the scheduler, and rewrite the three
  cadence tests in `tests/roguelike_combat.rs` listed in §2.

**Done when:** the roguelike plays start to finish (floor1 → floor2 → floor3 →
victory) with combat, boss-gating, and quaffing all behaving as before; the full
test suite passes.

---

### Step 5g — pluralise the player

Falls out of 5e/5f. `PlayState::player_id` (singular) and the
`other == self.player_id` check in `late_update`'s exit-trigger handling both
assume exactly one player.

- Player identity comes from `Controller::Local(slot)`, not a stored id.
- `late_update`'s exit trigger fires for **any** local player.
- `camera_entity` stays singular — one viewport. Split-screen is out of scope; say
  so in a comment rather than leaving it ambiguous.
- Authoring stays singular (one `PlayerRecord` per level); only the *code* stops
  assuming it. Prefabs remain deferred per plan §11 Q5.

**Done when:** nothing in `play.rs` / `play/spawn.rs` reads a singular player id,
tests pass, the game plays unchanged.

---

### Step 5h — the replay test (the phase's "done when")

New `tests/replay.rs`:

1. Run the roguelike headless through `TurnHarness`, recording
   `(step_index, Vec<Command>)` plus the level seed.
2. Replay against a fresh simulation with the same seed and the same command list.
3. Assert the final state is byte-identical: RON-serialize `World` + `globals` +
   `persistent` + `clips` and compare. 5b's `BTreeMap`s are what make that
   serialization stable; 5c is what makes `globals`/`clips` part of it at all.

Also assert an intermediate hash every N steps, so a failure reports the *first*
divergent step rather than just "different at the end" — that's the shape Phase
9a's desync detector needs anyway.

No CI exists to run this in; note it as a manual gate for now.

**Done when:** the replay test passes repeatedly across fresh processes (run it 5×,
same discipline Phase 4 used for its throwaway harness tests).

---

### Step 5i — workspace split (§5.5)

Now mechanical, because 5a already fixed the edges.

```
ember2d-sim/     math, color, world, components, level, save, scripting,
                 sim, commands, scheduler, graph (codegen), rng
                 — no renderer, window, or input
ember2d/         engine, renderer, input, mouse, gamepad, audio, play, app, ui
                 → depends on sim
ember2d-editor/  editor/*  → depends on both
```

Plus a thin bin crate for `main.rs`.

Gotchas, both already handled if 5a and 5e went in cleanly:

- `scripting/api.rs`'s presentation calls (`draw_hud`, `play_sound`,
  `emit_particles`, `shake_camera`) are *queued*, never executed, so they belong
  in sim — but only because 5a moved `Color` out of `renderer`. Verify no other
  renderer type leaked into `HudDraw` / `ParticleRequest`.
- `InputSnapshot` is sim-side; `Key` and the winit conversion stay engine-side (5e).

**Done when:** all three crates build, the full test suite passes, and both smoke
tests run. `cargo build -p ember2d-sim` must succeed with no renderer/window
dependency in its tree — that's the constraint the split exists to enforce.

---

## 5. Verification commands

```
cargo build
cargo build --example gen_roguelike
cargo run --example gen_roguelike        # regenerate roguelike/*.level after editing the generator
cargo test --lib
cargo test --test persistent_on_start --test trigger_collider_layer \
           --test roguelike_floor1 --test roguelike_combat --test roguelike_level_integrity
cargo run -- roguelike/floor1.level           # play smoke test
cargo run -- --editor roguelike/floor1.level  # editor smoke test
```

Baseline at the start of this phase: **74 lib tests + 19 integration tests, all
passing.**

**Never run bare `cargo test`** — it also builds a test binary for `main.rs`, which
this machine's Application Control policy blocks ("An Application Control policy
has blocked this file"). Always `--lib` plus the named integration tests. Same
policy blocks test binaries whose filename starts with an underscore; use plain
identifiers for scratch test files.

**Verifying a new script actually works** (not just that it compiles): write a
throwaway integration test under `tests/` using `TurnHarness`, teleport entities
into the scenario directly (`world.transforms...position = `) rather than walking
them there, assert the outcome, run it, then **delete the file**. This is what
caught both of Step 4h's real bugs; a clean smoke-test log and a passing unit suite
caught neither.

**Background-launch pattern** (Bash tool): `(cargo run -- roguelike/floor1.level >
/tmp/play.log 2>&1 &) ; sleep 8; tasklist | grep -i ember2d`, then
`taskkill //F //IM ember2d.exe //T`. Remember it does **not** catch script
compile/runtime errors (§3).

---

## 6. Documents to update as the phase lands

- `docs/ember2d-scripting-api.md` — §3 for every new/removed call, §6's changelog
  table for each `api_version` bump (4→5 in 5e, 5→6 in 5f), the camera
  replay-safety note (5d).
- `docs/ember2d-regression-checklist.md` — §12 (turn-based) needs rewriting for
  scheduler cadence; §14's table marks D7 (5f) and D17 (5c) fixed.
- `docs/ember2d-refactor-plan.md` — §3's D7/D17 rows, and a Phase 5 amendment block
  recording what actually shipped vs. what was planned (same convention Phase 4
  used — append, don't rewrite the original).
- `docs/HANDOFF.md` — rewrite for Phase 5 once the first step lands.
