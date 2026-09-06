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

## 4. Steps 4–14

Steps 4 (`WorldSnapshot` allocation diet) through 14 (documentation) are
planned in full detail but not yet started. See the plan file this phase was
approved from for the complete step-by-step design (the `WorldSnapshot`
allocation diet; zero-snapshot `run_collisions`; hot-reload throttling; the
collision-layer bitmask with a private `Collider.layer`/`mask` and
`LevelData.collision_layers` as the name→bit table; sweep-and-prune broad
phase; timers moving to `ScriptEngine`; `prev_positions` buffer reuse; the
`entity_ids` bug fix; `atan2` replaced with a rational approximation;
conditional `DrawList` buffer reuse) — this doc will be updated with a
"Done — Step N" note and real before/after numbers as each one lands,
matching how
`docs/ember2d-phase5-plan.md` and `docs/ember2d-phase5.5-plan.md` record
their own step-by-step history.

**Determinism gate, mandatory and non-negotiable for this phase:**
`cargo test --test replay` run 5× as independent fresh processes,
individually after Steps 3, 7, and 8 — not just once at the end. Steps 3
and 9 in particular touch exactly the map-ordering/deferred-write machinery
the replay test exists to guard.

---

## 3. Documents to update as the phase lands

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
