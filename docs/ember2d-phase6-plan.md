# Ember2D — Phase 6: Performance and Data-Model Hardening

**Status:** in progress. Written 2026-09-05 against `claude` @ `4e4da60` (Phase
5.5 complete).
**Parent:** `docs/ember2d-refactor-plan.md` §7 Phase 6.
**Companions:** `docs/ember2d-regression-checklist.md`, `docs/ember2d-scripting-api.md`.

---

## 0. Why this phase looks different from the plan doc's own sketch

Three research passes against the current (post-Phase-5.5) code found the
plan doc's own Phase 6 section partly wrong, and the scale question
different from what it assumed:

- **`roguelike/floor2.level` already spawns ~2,570 entities / ~1,704
  colliders.** The plan's "2,000-entity level holds 60fps" done-when is
  shipped content, not a hypothetical future level, and floor2 is the exact
  level previously reported at ~21fps (root-caused then to an unoptimized
  `[profile.dev]`, since fixed — clone churn was never actually proven to be
  the dominant cost until this phase's own benchmark, §1 below, measured it).
- **"Borrow instead of snapshot" is architecturally impossible.** Rhai
  requires registered types be `'static` (`ScriptCtx` is `Clone` and handed
  to `call_fn` by value) — a lifetime-borrowing `ScriptState` could never be
  registered. That's *why* `Rc<RefCell<ScriptState>>` exists. The snapshot
  must stay owned; the only real lever is making it cheap.
- **Two named offenders in the plan doc turn out not to be offenders.**
  `Vec2::normalized` is `sqrt` + divides (both IEEE-754-exact) and has **zero
  callers** anywhere in the workspace — not a determinism hazard, corrected
  in §5.2 below. And the `// Phase 6` comment left on `StepOutcome` in Phase
  5.5 was a false alarm: `Vec::new()` doesn't allocate.
- **The transcendental-math "decision" the plan frames as a major fork
  (fixed-point? lookup-table trig?) is a two-line problem.** Exhaustive grep
  of `ember2d-sim/src/` finds exactly one real offender — `atan2` in
  `get_angle_to` — and no shipped script calls it.

Two items the plan lists for this phase are **deferred**, not built:

| Item | Why deferred |
|---|---|
| `EntityId { index, generation }` | ~79 `as i64` boundary casts, ~17 script sites doing `"hp_" + id` string concatenation, the `-1` no-entity sentinel, two competing id allocators (`World::spawn` and the script-side `next_spawn_id`), and it breaks every existing save file — for zero benefit until Phase 9's netcode needs authority-prefixed ranges. |
| Binary snapshot path (§5.3) | `rhai::Dynamic`'s `Deserialize` is `deserialize_any` (rhai 1.24 `src/serde/deserialize.rs`), which non-self-describing formats like bincode cannot serve. Making it work needs a hand-written tagged value enum for `globals`/`persistent`, and its only consumer — Phase 9c rollback — is itself explicitly conditional in the refactor plan ("only if the platformer needs online play"). |

The component-registration macro the plan asks for is **closed, not
deferred** — see Step 11.

---

## 1. Step 1 — Benchmark harness + baseline ✅ Done

New `ember2d-sim/examples/bench_sim.rs` — an example, not a `[[bench]]` or
criterion target: no dev-dependency exists anywhere in the workspace today,
`cargo bench`'s built-in harness needs nightly, and (same reasoning
`examples/gen_roguelike.rs` documents for itself) an example is compiled but
never *executed* by `cargo build`/`cargo test`, so it can't become a target
for this machine's Application Control policy. Lives in `ember2d-sim`
specifically so it builds in seconds without pulling in wgpu/winit/kira, and
so it structurally proves what's being measured is the sim.

Contains: a counting `#[global_allocator]` (binary-local — makes "no
per-frame allocation proportional to entity count" a directly measured
number instead of an impression from playing the game); a synthetic
N-entity level generator (carve-from-solid, same shape as
`gen_roguelike.rs`'s own approach, ~66% interior collider density to match
floor2's real composition); a driver duplicating `TurnHarness::frame`'s
step/detect-collisions/late-step sequence; per-phase attribution by timing
`WorldSnapshot::build` and `World::detect_collisions` directly on a cloned,
identically-staged world (zero engine instrumentation); and a floor1/2/3
baseline using the real shipped scripts (`player.rhai`, `enemy_rat.rhai`).

**Run:** `cargo run --release -p ember2d-sim --example bench_sim` (always
`--release` — see the file's own header comment for why debug numbers are
worthless here). No CI timing gate — shared runners make ms numbers noise —
but `--examples` was added to CI's `cargo build --workspace` step so this
(and `gen_roguelike`) can't bit-rot.

### Baseline, measured 2026-09-05, release build, this machine

| Level | Entities | Colliders | p50 ms/step | allocs/step | bytes/step | snapshot build | detect_collisions |
|---|---|---|---|---|---|---|---|
| synthetic n=500 | 530 | ~353 | 1.499 | 6,632 | 678 KB | 0.502ms | 0.278ms |
| synthetic n=2000 | 2,026 | ~1,351 | 7.141 | 23,728 | 2.55 MB | 1.923ms | 2.344ms |
| synthetic n=5000 | 5,042 | ~3,361 | 25.326 | 58,098 | 5.50 MB | 4.918ms | 13.507ms |
| synthetic n=10000 | 10,001 | ~6,667 | 75.040 | 114,566 | 10.9 MB | 9.936ms | 52.512ms |
| `roguelike/floor1.level` | 805 | 228 | 1.558 | 10,290 | 843 KB | 0.559ms | 0.093ms |
| `roguelike/floor2.level` | 2,570 | 1,704 | 9.723 | 35,299 | 3.34 MB | 2.199ms | 3.175ms |
| `roguelike/floor3.level` | 1,576 | 925 | 4.430 | 21,439 | 1.83 MB | 1.146ms | 1.039ms |

**What this confirms:**
- **Allocation count scales roughly linearly with entity count** — the
  synthetic 500→2000 step (a 3.6× entity increase) moves allocs/step
  6,632→23,728, almost exactly 3.6×. This is the concrete violation of the
  phase's own done-when, now a number instead of a suspicion.
- **The O(n²) collision loop dominates at scale**: at n=10,000,
  `detect_collisions` alone is 52.5ms of the 75ms total step cost (70%). At
  floor2 it's a smaller but still real 33% (3.175ms of 9.723ms).
  `WorldSnapshot::build` is ~23% of floor2's step cost.
- **~40% of floor2's per-step cost (≈4.3ms) is neither snapshot build nor
  collision detection** — globals/clips/persistent clones, timer scanning,
  the hot-reload syscall, and script execution itself. Steps 3, 4, 6, and 9
  target exactly this remainder.
- floor1's `detect_collisions` (0.093ms, 228 colliders) is far cheaper than
  the *smaller-entity-count* synthetic n=500 (0.278ms, ~353 colliders) —
  confirming collision cost tracks collider count, not raw entity count,
  which is why the synthetic generator's higher collider density is a
  reasonable stress test even at modest scale.

**Target for this phase, set from these numbers:** re-run this bench after
Steps 3–6 and 9 land and confirm floor2's allocs/step drops substantially
(the six eliminated map-clones and the snapshot diet should remove the
majority of the ~35,300 figure above, independent of anything Step 7/8 do to
collisions); and confirm the synthetic n=500→n=2000 allocs/step ratio drops
from the current ~3.6× toward whatever the remaining, genuinely-O(colliders)
collision-phase cost implies — i.e. the *non-collision* portion of
allocs/step should stop scaling with entity count at all. Re-run again after
Step 7 (bitmask) and Step 8 (sweep-and-prune) and confirm
`detect_collisions`'s share of total step time falls sharply at every scale,
most dramatically at n=10,000.

### 1.1 Out-of-band fix landed between Steps 1 and 2: D19

Not part of this phase's planned scope — reported live by the user while
reviewing Step 1 ("player movement doesn't feel great when the enemy is
moving") and fixed immediately since it's a real correctness regression
from Phase 5.5 Part 3, not a design/feel question. See
`docs/ember2d-refactor-plan.md` §3 D19 for the full mechanism: a player
keypress made while an enemy's move animation was playing got silently
dropped (not delayed) by `ember2d::sim::step`'s unconditional
`consume_step()` call, since `PlayState::update`'s animation-blocked branch
never read the input it claimed. Fixed by carrying `pressed` sets forward
across blocked frames in a small `PlayState`-owned buffer. Also reduced
`enemy_rat.rhai`/`enemy_boss.rhai`'s `animate_move` duration 0.15s → 0.08s,
since multiple enemies acting in one round each pay their own animation's
duration serially (the scheduler blocks *all* stepping, including the
player's next turn, until each one finishes). This step's numbering (D19)
lands ahead of D20/D21 below, which are still-planned Phase 6 findings, not
yet logged — a fix found and shipped live jumped the queue.

---

## 2. Step 2 ✅ Done — split the two over-limit scripting files

Pure relocation, no behavior change. `scripting/apply.rs` ← `ScriptEngine::apply_ctx`
(a second `impl ScriptEngine` block, same pattern `api_animation.rs` established
in Phase 5.5); `scripting/api_spatial.rs` ← `get_entity_at`/`is_solid_at`/
`find_entities_in_rect`/`get_distance`/`get_angle_to`/`raycast`/`get_path`
(a second `impl ScriptCtx` block) — exactly the set this phase's later steps
modify. `engine.rs`: 573 → 480 lines. `api.rs`: 629 → 453 lines. `git diff
--stat` confirmed only moves/deletions. Full test suite unchanged.

## 3. Step 3 ✅ Done — `mem::take` instead of cloning globals/clips/persistent

Six conceptual clone-eliminations (globals/clips/persistent, in and out of
`ScriptState`), spread across every `Simulation` call site that hands them to
a `run_*` method (`do_on_start`, `step`'s `on_input`/`run_scripts` calls,
`run_actor_turn`, `late_step`), every `run_*` method's own `persistent.clone()`,
and `apply_ctx`'s final `ScriptUpdateResult` construction. `run_scripts`'s
dead `_events: &mut EventBus` parameter also removed (and the `EventBus::new()`
callers had to allocate just to satisfy it). Added a `debug_assert!` at the
top of `apply_script_result` checking `self.globals`/`self.clips` are already
empty — it must always be true now (a preceding take is the only thing that
gets there), so it fires immediately if a future edit reverts to `.clone()`
or an early return skips the matching take/restore pair. It did not fire
across the full test suite.

**Measured (release, this machine, before → after):**

| Level | p50 ms/step | allocs/step |
|---|---|---|
| synthetic n=500 | 1.499 → 1.480 (-1%) | 6,632 → 6,539 (-1%) |
| synthetic n=2000 | 7.141 → 6.649 (-7%) | 23,728 → 23,634 (~0%) |
| synthetic n=5000 | 25.326 → 22.154 (-13%) | 58,098 → 58,005 (~0%) |
| synthetic n=10000 | 75.040 → 67.257 (-10%) | 114,566 → 114,472 (~0%) |
| floor2 | 9.723 → 7.912 (**-19%**) | 35,299 → 35,220 (~0%) |

**A real finding, worth recording plainly rather than letting the earlier
"watch allocs/step" framing stand uncorrected: this step's win shows up in
*time*, not allocation *count*.** `globals`/`clips`/`persistent` are small
maps (tens of entries at most — keyed by things like `"hp_" + id` for a
handful of actors, not one entry per entity), so their clones were never a
meaningful share of the allocation total that `WorldSnapshot::build`'s
per-entity work dominates (that's Step 4's target). But `BTreeMap::clone()`
does real CPU work — walking and copying every entry — independent of how
many *allocations* that costs, and a small map still costs real time to
clone three-to-five times per step. Removing that work cuts floor2's p50 by
~19% with almost no change to allocs/step. The phase's "no per-frame
allocation proportional to entity count" done-when is unaffected either way
— that property was never about this step, it's Steps 4/5/7/8's to establish.

---

## 4. Step 4 ✅ Done — `WorldSnapshot` allocation diet

Three changes to `WorldSnapshot::build`, all in `scripting/state.rs`, none
touching `scripting/api.rs`'s function signatures (only their bodies — see
below):

- **`colors: HashMap<i64, (Color, Color)>`, not `(String, String)`.** Building
  the snapshot used to call `color_to_name` (a `String` allocation) on both fg
  and bg for *every* sprite, whether or not any script that step ever calls
  `get_color` on that entity. `get_color` now runs `color_to_name` itself, on
  read, only for whichever id a script actually asks about.
- **`tags`/`tag_to_id`/`tag_to_ids` share one `Rc<str>` per tagged entity.**
  Building used to call `tag.name.clone()` three times per tagged entity (once
  per map). Now one `Rc::from(tag.name.as_str())` allocation is cloned twice
  more (an `Rc` clone is a refcount bump, not an allocation) into the other
  two maps. `find_by_tag`/`find_all_by_tag`/`count_by_tag`/`has_tag`/`get_tag`
  still take/compare a plain Rhai `String` — `Rc<str>: Borrow<str>` (and its
  `Hash`/`Eq`/`Ord` all delegate to `str`'s) is what lets a `String`-keyed
  lookup keep working against a map now keyed by `Rc<str>` with no API change.
- **`textures: HashMap<i64, Rc<str>>`, not `String`.** Same sharing primitive
  as tags, applied to the one other per-entity string this snapshot carries.
- **Pre-sized `HashMap`s** (`velocities`/`glyphs`/`colors` at
  `world.transforms.len()`, `tag_to_id`/`actor_speeds`/`clip_finished` at
  their own source stores' lengths) — a safe upper bound in every case, since
  each is built by iterating that exact store or a subset of it, so this
  removes the reallocate-and-copy steps a `HashMap` growing from empty would
  otherwise do.

Deliberately **not** touched: `colliders` (`(f32, f32, bool, String,
Vec<String>, bool)`, cloning a `String` and a `Vec<String>` per collider) —
that's Step 7's bitmask, which replaces the `String`/`Vec<String>` layer/mask
representation outright rather than just changing how they're shared, so
reworking the sharing here first would be wasted motion.

**Measured (release, this machine, before → after — "before" is Step 3's
already-landed numbers, the most recent prior baseline):**

| Level | p50 ms/step | allocs/step | bytes/step |
|---|---|---|---|
| synthetic n=500 | 1.480 → 1.210 (-18%) | 6,539 → 2,908 (**-56%**) | — |
| synthetic n=2000 | 6.649 → 6.053 (-9%) | 23,634 → 9,871 (**-58%**) | — |
| synthetic n=5000 | 22.154 → 21.834 (-1%) | 58,005 → 23,828 (**-59%**) | — |
| synthetic n=10000 | 67.257 → 68.216 (+1%, noise) | 114,472 → 46,734 (**-59%**) | — |
| floor1 | 1.558 → 1.164 (-25%) | 10,290 → 3,726 (**-64%**) | 843 KB → 576 KB (-32%) |
| floor2 | 7.912 → 7.657 (-3%) | 35,220 → 14,596 (**-59%**) | 3.34 MB → 2.34 MB (-30%) |
| floor3 | 4.430 → 3.520 (-21%) | 21,439 → 8,692 (**-59%**) | 1.83 MB → 1.31 MB (-28%) |

(bytes/step wasn't tracked per-level in Step 3's table, so floor1/2/3's
"before" bytes column above is Step 1's original baseline, not Step 3's —
Step 3 touched no allocation this diet doesn't also affect, so that
comparison is still apples-to-apples.)

**A consistent ~58-59% allocs/step cut across every scale, real but smaller
time wins, and confirmation the remaining allocation growth is
collision-driven, not snapshot-driven:**
`WorldSnapshot::build` itself dropped from 2.199ms to 1.556ms at floor2 (-29%,
the direct effect of this diet), but floor2's *total* step time only fell 3%
— `World::detect_collisions`'s O(colliders²) loop (still cloning a `String`
layer and `Vec<String>` mask per collidable *pair test*, untouched until Step
7) is now the dominant remaining cost, exactly as Step 1's baseline analysis
predicted it would become once the snapshot itself got cheap. The synthetic
n=500→n=2000 allocs/step ratio (the phase's actual done-when signal) moved
from 3.61× to 3.39×, against a 3.82× entity-count ratio — real progress
toward "allocs/step stops scaling with entity count," but not there yet,
because the O(n²) collision phase Step 4 deliberately left alone is still the
part that scales. Steps 7 (bitmask) and 8 (sweep-and-prune) are what closes
that remaining gap.

**Verified:** `cargo build --workspace --examples` and `cargo test
--workspace --lib` (41 passed) both clean; all 10 named integration tests
(32 sub-tests, including `shooter_arena`) passed; `cargo test --test replay`
run 5× as independent fresh processes, all passed (this step touches shared
per-entity state — `Rc<str>` construction order — so the gate applied even
though nothing here is deferred-write or map-ordering logic in the sense
Steps 3/9 are). `git diff --stat` on `ember2d/src/sim.rs`,
`ember2d/src/engine.rs`, `ember2d-editor/` stayed empty throughout.

---

## 5. Step 5 ✅ Done — `run_collisions` builds zero snapshots in the common case

`run_collisions` already built its `calls: Vec<(i64, i64, String)>` list
(which entity/other/script-path triples actually need `on_collide`) *before*
building a `ScriptState`/`WorldSnapshot` to run them against. If `calls` is
empty — no colliding pair this step involved a scripted entity — nothing was
ever going to call `on_collide`, so the snapshot that pass would have built
is now skipped entirely and the function returns
`globals`/`clips`/`persistent` straight back out unlanded, exactly what
`apply_ctx` would produce from a pass that ran zero scripts (every other
`ScriptUpdateResult` field set to that pass's quiescent default: nothing
spawned, despawned, drawn, or submitted).

**Deliberately not** done by sharing `step`'s own `WorldSnapshot` (the `Rc`
Step 5f's fix already threads through `on_input`/`on_update`/`on_turn`):
`late_step` calls `resolve_solid_collision` directly against `world` between
building the collision-pair list and calling `run_collisions`, so a snapshot
taken earlier in the step would hand a colliding script stale positions for
whichever pairs the solid-resolution pass just moved.

**Measured (release, this machine, before → after — before = Step 4's
numbers):**

| Level | p50 ms/step | allocs/step | bytes/step |
|---|---|---|---|
| synthetic n=500 | 1.210 → 0.789 (-35%) | 2,908 → 1,968 (**-32%**) | — |
| synthetic n=2000 | 6.053 → 4.064 (-33%) | 9,871 → 6,374 (**-35%**) | — |
| synthetic n=5000 | 21.834 → 21.174 (-3%) | 23,828 → 15,203 (**-36%**) | — |
| synthetic n=10000 | 68.216 → 66.263 (-3%) | 46,734 → 29,690 (**-36%**) | — |
| floor1 | 1.164 → 0.789 (-32%) | 3,726 → 2,095 (**-44%**) | 589 KB → 332 KB (-44%) |
| floor2 | 7.657 → 6.570 (-14%, ±1ms run-to-run) | 14,596 → 8,306 (**-43%**) | 2.34 MB → 1.36 MB (-42%) |
| floor3 | 3.520 → 2.717 (-23%) | 8,692 → 4,961 (**-43%**) | 1.31 MB → 774 KB (-42%) |

allocs/step and bytes/step are exact counts (the bench's counting allocator),
reproducible bit-for-bit across repeated runs — re-measured twice to confirm.
p50 ms/step is not: floor2 read anywhere from 5.5ms to 7.7ms across separate
runs, consistent with every prior step's own noise band on this machine: the
allocation counts are this step's real signal, not the timings.

**A second, larger cut on top of Step 4's, and a data point on why the
synthetic scale numbers diverge from the shipped-content ones:** at floor1-3
scale, roughly 40-45% of *all remaining* per-step allocation is a
`run_collisions` pass that, on a typical step, has nothing to actually run —
confirming the plan's "most turn-resolving steps have no scripted collide
pair" assumption directly, now as a number instead of a guess. At n=5,000/
n=10,000 synthetic scale the *time* win nearly vanishes (-3%) even though the
*allocation* win holds (-36%): `World::detect_collisions`'s O(colliders²)
loop so dominates total step time at that scale (85% of it at n=10,000, per
Step 1's own baseline analysis) that removing an allocation-only pass barely
moves the total, even though it removes over a third of the allocations.
Steps 7/8 are what will make that show up in the timing column too.

**Verified:** `cargo build --workspace --examples` and `cargo test
--workspace --lib` (41 passed, both `ember2d-sim` and `ember2d`) clean; all
10 named integration tests (32 sub-tests) passed; `cargo test --test replay`
run 5× as independent fresh processes, all passed — the early-return path
returns a result provably identical to what the pre-Step-5 code path would
have produced when `calls` is empty (every field traced above), so this
carries no real determinism risk, but the gate ran anyway rather than
asserting that from reasoning alone. `git diff --stat` on
`ember2d/src/sim.rs`, `ember2d/src/engine.rs`, `ember2d-editor/` stayed
empty.

---

## 6. Step 6 ✅ Done — throttle `check_hot_reload`

Two independent fixes to the same function, landed together since both are
about the same wasted `fs::metadata` call:

- **Throttled to once every `HOT_RELOAD_CHECK_INTERVAL` (30) calls to
  `run_scripts`** (one call per simulation step), via a plain counter on
  `ScriptEngine`, not `Instant::now()` — wall-clock time in `ember2d-sim`
  would be a determinism violation (CLAUDE.md's own rule: two machines'
  clocks don't advance in lockstep under replay/netcode the way step counts
  do). The throttle lives at the one call site inside `run_scripts`, not
  inside `check_hot_reload` itself — that function's own existing unit test
  (`hot_reload_clears_only_the_reloaded_scripts_entities`) calls it directly
  and expects it to check immediately every time, so its unconditional
  behavior stays exactly as it was; only the caller now decides how often to
  ask. Tradeoff: a live script edit can take up to 30 steps (0.5s at 60
  steps/s) to be noticed instead of the very next one — imperceptible for
  the dev-time-only workflow this exists for.
- **`__script_<id>` synthetic keys are skipped entirely.** These are a node
  graph's generated Rhai source, cached via `compile_str` under that key
  (`Simulation::do_on_start`), never backed by a real file — every
  `fs::metadata` call against one was already guaranteed to fail (D20,
  logged in `docs/ember2d-refactor-plan.md` §3). Filtering them out of the
  path list checked doesn't just reduce their frequency, it removes them
  from this loop outright, at every throttle interval, not just most of
  them.

New test `check_hot_reload_only_runs_once_every_throttle_interval`
(`engine_tests.rs`) exercises the throttle boundary directly: fewer than 30
`run_scripts` calls after forcing a script to look stale must leave its
recorded mtime unchanged (no check ran); the 30th must refresh it (a check
ran). The existing `a_script_that_errors_is_disabled_and_stops_being_called`
test had to force `hot_reload_counter` to the boundary before its own
re-enable-on-fix assertion, since it drives far fewer than 30 calls and
would otherwise never actually observe a hot-reload under the new throttle.

**Measured:** the exact, unconditional part of this fix is a syscall-count
argument, not a benchmark one — `check_hot_reload` previously issued one
`fs::metadata` call per cached script *every single step*; it now issues the
same calls, but only once every 30 steps: a flat **30× reduction** in that
syscall's frequency, independent of level size (floor2 alone caches 4
distinct script paths — `player`/`enemy_rat`/`pickup`/`stairs.rhai` — so this
is 4 syscalls/step collapsing to 4 syscalls per 30 steps). The `__script_<id>`
skip is a 100% elimination of a guaranteed-failing call, but applies only to
node-graph-authored scripts — no shipped level or `bench_sim`'s synthetic
generator uses `tile.graph`, so neither shows any effect from that half in
the numbers below; it will matter the moment a level built with the editor's
visual scripter is played.

`bench_sim`'s allocs/step barely moved (floor2: 8,306 → 8,293, roughly -0.2%)
— **expected, not a shortfall**: `fs::metadata` is an OS call, not a
`Vec`/`String` allocation the counting allocator this bench uses would
necessarily see move at all, so allocation count was never going to be this
step's signal any more than it was Step 3's (`mem::take` vs. `.clone()`) —
see that step's own write-up for the same distinction between "wall time
saved" and "allocations saved." p50 ms/step showed no consistent change
either direction at any scale, within this machine's usual run-to-run noise
band — also expected, since one syscall every 30 steps was never going to be
a measurable fraction of floor2's ~6ms/step budget. This step's payoff is in
I/O pressure (relevant on a slow or networked filesystem, and simply
correct — a script's on-disk mtime has no business being polled 60 times a
second when nothing on disk can plausibly have changed in under a frame),
not in anything `bench_sim` is built to see.

**Verified:** `cargo build --workspace --examples` and `cargo test
--workspace --lib` clean across all three crates (`ember2d`: 41,
`ember2d-editor`: 1, `ember2d-sim`: 42 — one more than Step 5's count, the
new throttle-boundary test above); all 10 named integration tests (32
sub-tests) passed; `cargo test --test replay` run 3× as independent fresh
processes (this step touches no map-ordering or deferred-write machinery —
a plain per-call counter — so the full 5× mandatory gate wasn't required,
but a sanity check cost little); manual smoke test of play, editor, and the
shooter demo. `git diff --stat` on `ember2d/src/sim.rs`,
`ember2d/src/engine.rs`, `ember2d-editor/` stayed empty.

---

## 8. Steps 7–14

Steps 7 (collision-layer bitmask) through 14 (documentation) are planned in
full detail but not yet started. See the plan file this phase was approved
from for the complete step-by-step design (the collision-layer bitmask with
a private `Collider.layer`/`mask` and `LevelData.collision_layers` as the
name→bit table; sweep-and-prune broad phase; timers moving to
`ScriptEngine`; `prev_positions` buffer reuse; the `entity_ids` bug fix;
`atan2` replaced with a rational approximation; conditional `DrawList`
buffer reuse) — this doc will be updated with a "Done — Step N" note and
real before/after numbers as each one lands, matching how
`docs/ember2d-phase5-plan.md` and `docs/ember2d-phase5.5-plan.md` record
their own step-by-step history.

**Determinism gate, mandatory and non-negotiable for this phase:**
`cargo test --test replay` run 5× as independent fresh processes,
individually after Steps 7 and 8 — not just once at the end (Step 3 already
had its own 5× gate, recorded in §3 above).

---

## 9. Documents to update as the phase lands

- `docs/ember2d-refactor-plan.md` — §3 D11 closed with real numbers once
  Steps 3–9 land, plus new D20 (hot-reload syscall) and D21
  (`cancel_timer`/`timer_done` sentinel overlap, logged not fixed) — D19
  (dropped input during an animation-blocked frame) already landed, out of
  sequence with the rest of this phase's numbered steps; see §0 below. §5.2
  records the transcendental-math decision and corrects the
  `Vec2::normalized` claim; §5.3/§5.4 record both deferrals; §7 Phase 6
  rewritten to what shipped.
- `docs/ember2d-scripting-api.md` — timers' save/load-loss note, collision-layer
  semantics (31-layer limit, `collision_layers` level field, unregistered-name
  fallback), a "no API break" §6 changelog row, the `get_angle_to` example
  rewrite.
- `docs/ember2d-regression-checklist.md` — this doc's own §17 (performance
  baseline, updated as steps land); collision-layer save/load checklist item.
- `docs/HANDOFF.md`, `CLAUDE.md` — updated once the phase closes.
