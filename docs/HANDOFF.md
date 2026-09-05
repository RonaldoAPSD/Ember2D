# Session Handoff (temporary)

> Not part of the permanent doc set — a scratch note for picking the current
> refactor phase back up in a fresh session/chat. Safe to delete once the
> phase in progress wraps (or fold anything still useful into
> `ember2d-refactor-plan.md` first). Rewritten from scratch when Phase 4's
> design changed, and rewritten again (chronological order, deduplicated)
> when the session that did Steps 4a–4e ended. See git log (`df48ad1` and
> earlier) for Phase 3's detail if it's ever needed again.
> Last updated: 2026-09-04.

## Where things stand

Branch: `claude` (only branch we ever commit to — never `main`).

`docs/ember2d-refactor-plan.md` Phases 0–3 are done and committed:
- `cf96c9f` — Phase 3 Steps 3a-3c (texture handles, SpriteSource, animation clips)
- `df48ad1` — Phase 3 Steps 3d-3e (level format v2 + graph sidecar migration, scripting API breaking renames)

**Phase 4 (de-hardcode play mode) is DONE — all steps 4a–4k complete, but
NOT YET COMMITTED** — they sit together in the working tree (commits only
happen when the user explicitly asks; see "Workflow reminder" below). The
roguelike demo (`roguelike/`, 3 floors + victory) was played start to
finish by the user and confirmed working. Phase 5 (simulation extraction,
commands, turn scheduler — see the plan doc) is next whenever work resumes,
but has not been started or planned out yet.

## Phase 4's design changed mid-phase — read this before continuing

The refactor plan's Phase 4 section says "move movement/collection into
scripts", assuming `demo/`'s existing player script could just grow to hold
it. Auditing `demo/` found only 2 of its 6 levels have *any* player script
attached at all — moving movement into "the player's script" would leave 4
levels (including the main level2→level3 progression) unable to move. Rhai
here is built with `no_module`, so scripts can't share code via import —
there's no clean fix that keeps the old six-level demo working.

**Decision (user-approved): build a new, small, turn-based roguelike with 3
connected floors from scratch; archive `demo/` rather than retrofit it.**
This also finally gives the refactor a deterministic, playable fixture for
automated tests — the completion test `docs/ember2d-scripting-api.md` §1
already describes ("build the demo into a full game... using only .rhai,
with zero Rust changes").

**The full reasoning and staged plan live at
`C:\Users\ronal\.claude\plans\memoized-discovering-glacier.md` on this
machine — read that file before continuing.** This doc is only a status
tracker; the plan file has the actual engine facts, script designs, and
verification steps. If that file is ever gone, re-derive from
`docs/ember2d-refactor-plan.md` §7 Phase 4 plus the step table below, but
know you'll be missing the Rhai-scope-doesn't-persist correction and the
determinism hazards documented there.

## Step status

| Step | What | Status |
|---|---|---|
| 4a | `z_for_tag` removed (`tile.layer * 10` directly); player collider size → `PlayerRecord.collider_w/collider_h` (default 0.75) | done |
| 4b | Level generator (`examples/gen_roguelike.rs`) + `roguelike/` project + floor 1 + `player.rhai`/`pickup.rhai`/`stairs.rhai` | done |
| 4c | Archive `demo/` → `docs/archive/demo/`, delete `tesst/`; move audio into `roguelike/`; add sound calls; make `play/tests.rs::level_with_exit` hermetic | done |
| 4d | Strip `PLAYER_SPEED` + WASD + corridor-snap from `play.rs`; delete the temporary velocity guard from `player.rhai` | done |
| 4e | Headless turn-based test harness + floor-1 integration tests; found and fixed a live script bug (below) and a real engine defect, D15 (below) | done |
| 4f | Delete `score`/`total_items`, the `"item"`/`"chest"` tag branch, the dead `"spawn"` tag skip, `find_by_tag("player")` → `find_by_tag(&level.player.tag)` | done |
| 4g | HUD: delete both Rust bars, `HUD_TOP_ROWS` → 0, full-height viewport, F3 debug overlay, fix HUD-doesn't-survive-pause bug | done |
| 4h | Floor 2 + `enemy_rat.rhai` (combat) | done |
| 4i | Floor 3 + `enemy_boss.rhai` + victory level; boss-gated stairs | done |
| 4j | Combat / turn-cadence / determinism / level-integrity tests | done |
| 4k | Docs: regression checklist rewrite, scripting-API corrections (per-entity scope claim is wrong — see plan file), `api_version` bump, plan-doc amendment | done |

**Current verification state**: 74 lib tests + 19 integration tests passing
(1 `persistent_on_start` + 2 `trigger_collider_layer` + 5 `roguelike_floor1`
+ 5 `roguelike_combat` + 6 `roguelike_level_integrity` — the latter two new
in 4j, replacing 4h/4i/the quaff amendment's throwaway-verified-then-deleted
checks with permanent equivalents). Play mode and editor smoke-tested clean
on every floor after every step. **The user played the full run start to
finish (floor1 → floor2 → floor3 → victory, using all their potions) and
confirmed it works.** **F3's visual appearance and the pause-persistence
behavior (Step 4g) specifically are still only verified at the unit-test
level**, not confirmed by that playthrough or otherwise looked at directly
— still worth a manual look whenever convenient.

## What each done step actually did

**4a** — mechanical de-hardcoding, no behavior change. Safe because
`LevelGrid` keys tiles by `(x,y,layer)`, so two tiles on the same layer can
never occupy the same cell — the old per-tag z sub-ordering never actually
resolved a real draw-order conflict.

**4b** — `examples/gen_roguelike.rs` is a committed generator, not a
throwaway (`examples/` not `src/bin/`, since Cargo auto-runs a test target
for every bin, and this environment can't execute `main.rs`'s — see
"Verification commands" below). It carves rooms into an all-wall canvas so
every cell is guaranteed wall-or-floor, then emits a `LevelData` with tiles
sorted by `(layer,y,x)` and a pinned seed — both mandatory, since building
straight from a HashMap-backed `LevelGrid` would make the generator's own
output nondeterministic across runs. `roguelike/floor1.level` (40×20) has
two gold, one potion, and stairs to a `floor2.level` that doesn't exist yet
(safe — a missing exit target just logs a warning). `player.rhai` implements
turn-gated grid movement (`is_solid_at` → `set_position` → `trigger_turn`,
never velocity) plus bump-to-attack and damage resolution, written in full
even though floor 1 has no enemies yet, so it won't need revisiting in
4h/4i. `pickup.rhai` and `stairs.rhai` round out the set.

  **Gotcha hit immediately, applies to every future script:** `player.rhai`
  originally failed to *compile* — an 11-`+` chained string concatenation
  in the HUD line tripped Rhai's default max-expression-depth guard. **A
  script compile failure logs to the in-game `script_log` only — never to
  stdout/stderr** — so a headless background-launch smoke test's log file
  looked completely clean despite the game being unplayable; the user
  caught it by actually looking at the running window. Fixed by building
  the string incrementally with `+=` instead of one long chain. **Going
  forward: verify any edited/new script compiles via a throwaway
  `ScriptEngine::compile` check before trusting a smoke test — don't trust
  a quiet log alone.** Snippet (scratch `examples/*.rs`, delete before
  committing):
  ```rust
  use ember2d::scripting::ScriptEngine;
  fn main() {
      let mut engine = ScriptEngine::new(0);
      for path in ["roguelike/scripts/player.rhai", /* ... */] {
          let mut log = Vec::new();
          println!("{}: {}", path, if engine.compile(path, &mut log) { "OK" } else { "FAILED" });
          for entry in &log { println!("  {:?}: {}", entry.level, entry.text); }
      }
  }
  ```

**4c** — `demo/` moved via `git mv`: audio (`hurt.ogg`, `music.ogg`,
`victory.ogg`) → `roguelike/audio/`; everything else (6 levels,
`project.ron`, `project.palette.ron`, 10 scripts) → `docs/archive/demo/`,
with a `README.md` there explaining the archived scripts' now-stale audio
paths (moved, not copied — no reason to duplicate ~1.5 MB for a project
that isn't run anymore). `tesst/` was **deleted** outright, not archived —
pure pre-refactor junk (an empty `.rhai`, a `project.ron` pointing at a
nonexistent level), unlike `demo/`'s scripts which have real reference
value for the roguelike's own enemy AI later. `play/tests.rs::level_with_exit`
no longer points at `demo/level1.level`; it writes its own tiny target
level to the temp directory per test (unique filename per call site, so
cleanup in one test can't race a load in another). Only 3 real audio files
exist and none fit "attack"/"pickup"/"stairs" thematically — rather than
inventing fake cues, `hurt.ogg` does double duty for both landing and
taking a hit, `music.ogg` starts via the same lazy-init-in-`on_update`
pattern as everything else, and pickup/stairs stay silent with a comment
explaining why.

**4d** — removed `PLAYER_SPEED`, the WASD-to-velocity block, and the
corridor-snap math from `PlayState::update`, plus the now-dead
`ctx.set_velocity(id,0,0)` guard from `player.rhai` (its only job was
cancelling that Rust code within the same frame). Movement is now
**entirely** script-driven — nothing in Rust ever touches the player's
transform again.

**4e** — `tests/common/mod.rs`'s `TurnHarness` replicates `engine.rs`'s
TurnBased branch exactly (`consume_step()` *before* `update`, real
`integrate_physics(1.0)` not `SIM_DT`, `handle_released`, decay every
frame — get any piece wrong and the harness silently tests different
behavior than the real engine). `tests/roguelike_floor1.rs` has 5 tests:
one-cell movement, wall-bump costs no turn, no-input triggers no turn, gold
pickup despawns+credits on the same turn, stairs stay unlocked.

  **The first real run caught a live bug**: `player.rhai` never actually
  finished a turn — `trigger_turn()` was never reached. Root cause:
  lazy-initing `"turn"` via `set_global` and reading it back via
  `get_global` a few lines later *in the same `on_update` call* doesn't see
  the write (deferred writes — the same hazard already documented for
  cross-script damage, fallen into within one script). On frame 1 this
  computed `() + 1`, which Rhai has no operator for. Fixed with an
  `or_zero()` guard used everywhere a lazy-inited value is read back in the
  same call, plus a stronger fix for `hp` specifically — computed directly
  in its own init branch, never defaulted to 0, since a wrong 0 there would
  silently read as "already dead" rather than just being cosmetically
  wrong. Applied the same defensive guard to `pickup.rhai`/`stairs.rhai`.

  **That bug was invisible because of a real engine defect (now fixed as
  D15)**: the error's message contained the substring `"Function not
  found"` — the exact phrase `ScriptEngine::run_scripts` uses to mean "this
  optional lifecycle function doesn't exist, that's fine, don't log or
  disable" — so `scripting/engine.rs`'s check
  (`!e.to_string().contains("Function not found")`) silently swallowed a
  genuine runtime error: no compile error, no log entry, no panic, the
  script just quietly stopped executing partway through, every frame. This
  wasn't a one-off — any type-mismatch bug in any script anywhere would
  have hit the same silent swallow, since Rhai reports "the function you
  called doesn't exist" and "the operator overload you used doesn't exist"
  as the exact same error variant (operators are functions internally).
  **Fixed**: new `ScriptEngine::is_missing_optional_fn`
  (`src/scripting/engine.rs`) matches `rhai::EvalAltResult::ErrorFunctionNotFound`'s
  exact payload against the specific lifecycle function name instead of a
  substring of the whole error text — confirmed the two message shapes
  straight from the rhai 1.24 source (`func/call.rs`, `api/call_fn.rs`): a
  genuinely-missing top-level function reports the bare name with no
  signature; a failed call *inside* an existing function reports `"name
  (arg, types)"`. Two new regression tests in `scripting/engine_tests.rs`
  pin both directions. Recorded as **D15** in
  `docs/ember2d-refactor-plan.md` §3, alongside D1–D14.

  **Also found during 4e, unrelated, resolved**: `.gemini/GEMINI.md` showed
  as deleted in the working tree (unstaged); nothing in the session touched
  that path before or after. Restored via `git restore` — cause still
  unknown, but restoring was the safe/reversible default.

**4f** — pure deletion, no behavior change for any current level. Removed
`PlayState::score`/`total_items` (fields, initializers, and the top HUD
bar's now-meaningless "Items X/Y" segment — the rest of that bar is
untouched, full removal is 4g's job); the `to_collect`/`other_tag`/
`matches!(…"item"|"chest")` branch in `late_update` and its collection loop
(collection is now entirely `pickup.rhai`'s `on_collide`, via the existing
script-collision path — nothing in engine code named "item"/"chest" was
still load-bearing); the dead `tile.tag == "spawn"` skip in
`play/spawn.rs::do_on_start` (confirmed via grep: nothing in `src/` writes
that tag, no committed `.level` carries it — this was pre-refactor debris,
not exercised by anything); and `on_start`'s hardcoded
`world.find_by_tag("player")` → `world.find_by_tag(&self.level.player.tag)`
(only reachable on the `is_loading_save` branch, so no behavior change
today since every level's `player.tag` still defaults to `"player"`, but it
stops silently breaking a save whose player was ever retagged). Verified via
grep across `src/` and `tests/` before touching anything — the only other
`find_by_tag("player")` call sites are test-fixture helpers unrelated to
this hardcoded path.

**4g** — user sign-off obtained for both flagged items (the `get_mouse_world_y`
break and the optional `PlayerRecord.layer` add-on) before landing either.

- Deleted both hardcoded HUD bars (`play.rs::render`'s top status line and
  bottom "WASD / Arrows: Move   Esc: Pause" bar) and their `draw_str` calls.
  Replaced with an F3-toggled debug overlay (`PlayState::show_debug`, same
  content as the old top bar minus the already-deleted Items counter) —
  matching the standing preference that this kind of info be a toggleable
  debug tool, not permanent chrome. The bottom bar's control-hint text was
  already fully redundant with what `player.rhai`'s own `draw_hud` calls
  draw (`ctx.get_viewport_height()` now returns the true full height, so its
  own bottom-row hint lands on the actual last row).
- `HUD_TOP_ROWS` (`play.rs`) is now `0`, not `1` — the constant's *value*
  changed, not its call sites, so `Camera::viewport_origin` and
  `get_mouse_world_y` (`scripting/api.rs`) both go inert automatically
  without becoming two hand-duplicated literals again. `game_h` in both
  `PlayState::update` and `play/spawn.rs::do_on_start` dropped its `- 2`
  (both hardcoded bars reserved one row each; there's nothing left to
  reserve for). **Breaking, documented**: `API_VERSION` 3→4,
  `docs/ember2d-scripting-api.md` §6 changelog row added, its stale "Phase 2
  already removed this" claim corrected (Phase 2 only centralized the fudge
  into one constant — it never actually zeroed it, contrary to what that doc
  previously said).
- Added `PlayerRecord.layer` (serde default 15, `Default` impl updated) to
  replace the hardcoded `Z_PLAYER` constant `play.rs` no longer has —
  `play/spawn.rs` now builds the player's sprite with `pr.layer`. Mirrors
  Step 4a's `collider_w`/`collider_h` treatment exactly. Two new tests
  (`play/tests.rs`) pin the default-matches-old-hardcoded-value case and the
  now-configurable case.
- **Fixed the HUD-doesn't-survive-pause bug**: `PlayState::render` used to
  call `self.script_engine.pending_hud_draws.clear()` itself, every frame,
  regardless of whether a script actually ran that frame. `PlayState::update`
  (and therefore `ScriptEngine::run_scripts`) only runs for the
  top-of-stack `GameState` — pushing `PauseMenuState` on top stops it — but
  `render` still runs for every stacked state every frame. Net effect: a
  script's `ctx.draw_hud`-drawn text (health, gold, etc.) vanished the
  instant the game paused, since nothing ever refilled the queue while
  paused but the render-side clear kept firing anyway. Fix: the clear moved
  to the *start* of `run_scripts` instead — the one call site that only
  fires on a real, unpaused frame — so a skipped pass now just leaves last
  frame's draws rendering unchanged. Pinned by a new
  `scripting/engine_tests.rs` test at the `ScriptEngine` level (constructing
  a real paused `PlayState` + `PauseMenuState` stack wasn't necessary to pin
  the actual mechanism that changed).
- Log messages (last 3 `script_log` entries) shifted from "2 rows above the
  bottom edge" to "at the bottom edge" — there's no bottom bar to sit above
  anymore.
- Rewrote the two tests whose expected values were the pre-4g formula:
  `tile_z_uses_only_the_authored_layer_not_a_tag_based_offset` (now captures
  `data.player.layer` instead of referencing the deleted `Z_PLAYER` const)
  and the camera-origin test (renamed
  `script_camera_origin_uses_the_full_viewport_now_that_the_hud_bars_are_gone`;
  `half_h` is `10.0` now, not `9.0`, since `game_h` is the full
  `viewport_height`).
- **Not independently verified**: F3's on-screen appearance and the
  pause-persistence behavior end-to-end through a real paused `PlayState`.
  Both are exercised at the unit level only (see "Current verification
  state" above) — a manual look at the running game is worth doing before
  treating 4g as fully proven, not just compiling and passing tests.

**4h** — `examples/gen_roguelike.rs` gained `floor2()`: 80×32 (bigger than
the 80×24 viewport specifically to exercise the camera clamp, unlike
floor1's single hall), three rooms connected by two corridors (using the
`corridor_h`/`corridor_v` helpers that had sat `#[allow(dead_code)]` since
4b, removed now that they're finally used), 3 rats in the combat-arena
room, gold/potions in all three rooms, stairs to a `floor3.level` that
doesn't exist yet (safe, same as floor1→floor2 before this step). New
`roguelike/scripts/enemy_rat.rhai`: turn-gated (`acted_<id>` vs the shared
`turn` global, same pattern as everything else), greedy dominant-axis
chase, bump-attacks when cardinally adjacent, single-writer damage
(`atk_turn_<id>`/`atk_dmg_<id>`, matching player.rhai's existing
`resolve_incoming_damage` which was written in 4b but had never actually
been exercised — floor1 has zero enemies).

  **Two real bugs found by actually driving combat through `TurnHarness`,
  neither caught by compiling, the unit suite, or a clean smoke-test log**
  (matching 4e's own precedent exactly — a scratch integration test using
  the existing harness, deleted after use, is what caught both; see
  "Verification commands" below for the pattern):

  1. **enemy_rat.rhai's own hp lazy-init raced player.rhai's `attack()` on
     the very first frame combat could happen.** Both write the *same*
     global key (`"hp_" + id`) — the rat's `!has_global` init-branch and
     the player's damage-subtraction — and `ScriptState::pending_globals`
     is a plain `HashMap<String, Dynamic>`: two `set_global` calls to the
     same key in the same pass don't merge or error, the second one
     (whichever entity's script happens to run later in that frame's
     HashMap-ordered iteration) just silently overwrites the first. A rat's
     `on_update` had never run before its first hit, so its init-write
     (resetting to 6) could — and in testing, did — land *after* and
     clobber the player's damage-write in the exact same frame. **Fixed**:
     moved the init from `on_update` to `on_start`, which runs in its own,
     earlier pass (level load, before the first real engine frame) — by
     the time any `on_update` ever runs, including the player's, `hp_<id>`
     already has its real committed value, so the rat's `on_update` never
     needs to write that key again in the normal case (a defensive fallback
     branch stays, for the same "never default a missing value to 0"
     reason player.rhai's own `hp` avoids `or_zero`).
  2. **player.rhai's `resolve_incoming_damage` (written in 4b) silently
     dropped every enemy attack, permanently** — confirmed by hand: a
     player standing next to a live rat for 6 turns took zero damage. Root
     cause is a one-frame lag inherent to deferred writes: an enemy can
     only publish `atk_turn_<id> = T` no earlier than the same frame `T`
     first becomes visible to *it*, and that write doesn't commit until the
     end of that frame — one frame after player.rhai's own `on_update`
     (same pass) already read `"turn" == T` and set
     `last_resolved_turn := T` right then, having found nothing (the
     attack hadn't landed yet). By the next frame, `atk_turn(T)` is no
     longer `> last_resolved_turn(T)`, so the check that was supposed to
     catch it never can. **Fixed**: stopped tracking "the raw turn counter
     as of last resolve" and started tracking "the highest `atk_turn` value
     actually found and applied" instead — never skip the scan, and only
     ever raise `last_resolved_turn` to that highest applied value. See
     `player.rhai`'s own header comment for the full trace. **This is the
     same deferred-write-lag hazard class as D15/the 4e script bug, just
     manifesting across frames via a premature marker update instead of
     within one pass** — worth remembering for `enemy_boss.rhai` in 4i,
     which will hit the exact same shared-global patterns.

  Both fixes verified via a throwaway `tests/rat_combat_manual_check.rs`
  (three cases: bump-attack kills a rat in exactly 2 hits and consumes a
  turn each time; a rat chases the player across turns; the player
  actually loses hp standing next to a live rat) — deleted after
  confirming all three passed, per the "don't leave scratch test files
  behind" convention this phase already established for script-compile
  checks. Not a substitute for 4j's real combat test suite — this was
  purely "does the mechanic work at all," not full coverage.

  **Amendment (still within 4h, requested after the fact): line-of-sight
  aggro.** A rat now starts asleep (does nothing) until it either gets an
  unobstructed `ctx.raycast(ex, ey, px, py, ["solid"])` to the player, or
  its own hp drops below max (a landed hit implies proximity, which implies
  visibility, without needing a clean raycast on that exact turn). Once
  awake, sticky — never goes back to sleep, matching classic roguelike
  convention (Rogue/NetHack) rather than a patrol/search state machine,
  which would need real per-entity memory the API doesn't have yet. Tinted
  `DarkRed` asleep / `Red` awake so it reads visually. `mask=["solid"]` on
  the raycast matters: walls and rats share that collider layer by default
  (see `rat()`'s own comment in the generator), but the player's collider
  is a different, empty layer — so the mask can never mistake the player
  itself for something blocking its own line of sight.

  **Hit the max-expression-complexity guard again** (same class as
  player.rhai's HUD-string-concatenation issue, this time from nested
  if/else depth, not chained `+`) — pulled the whole awareness check into
  its own `update_awareness()` function rather than inlining it in
  `on_update`, which is what actually fixed it. **Second time this exact
  guard has bitten a script in this phase (4b's HUD string concatenation
  was the first) — treat "does it actually compile" as a mandatory check
  after *any* nontrivial control-flow change to a script, not just after
  adding string concatenation.**

  Verified via two more throwaway harness tests (`tests/rat_fov_manual_check.rs`,
  deleted after use): a rat with no line of sight (player still at spawn,
  walls between it and every rat) stays motionless across several turns;
  a rat given a clear line of sight (same open room, no walls between)
  wakes and steps toward the player on its very next turn. **Gotcha hit
  writing that first test**: `world.tags.iter().find(...)` for "the first
  entity tagged enemy" picks whichever rat HashMap iteration order returns
  first — process-randomized, so it varies between separate `cargo test`
  invocations (though stable across multiple `#[test]` fns *within* one
  invocation, since they share one process's hash seed). A fixed test
  offset that happens to place the player just outside room B's actual
  floor bounds for one specific rat (but comfortably inside for the other
  two) passed or failed *depending entirely on which rat got picked that
  run* — looked like a flaky feature bug, was actually an imprecise test
  fixture. Confirmed by running the fixed version 5 times in fresh
  processes before trusting it.

**4i** — `examples/gen_roguelike.rs` gained `floor3()` (56×28: an entry room,
one corridor, one big boss-arena room with 2 warm-up rats + the boss) and
`victory()` (20×10, a single enclosed room, no enemies, no exit — the
finale). New `roguelike/scripts/enemy_boss.rhai`: an exact copy of
`enemy_rat.rhai`'s whole design (turn-gating, single-writer damage,
sticky-aggro line-of-sight, `on_start` hp-init) with different numbers and
tagged `"boss"` instead of `"enemy"` — `player.rhai`'s
`resolve_incoming_damage`/`attack()` already treat both tags identically
(written that way in 4b), and `stairs.rhai` already locks any level's
stairs while `ctx.count_by_tag("boss") > 0`, so placing one `boss()` tile
on floor 3 *is* the entire boss-gate mechanism — zero changes needed to
either of those two already-shipped scripts. New
`roguelike/scripts/victory.rhai`: free-roam movement (no turn-gating, no
combat, nothing else on the level ever acts), victory music, and a run
summary pulled from `PERSISTENT` stats (gold/potions/turns_taken — the
only reason those survive the level transition into this one is that
they're persistent, not globals; see `player.rhai`'s own header comment).

  **Same compile guard bitten a third time**, same fix as 4h: pulled
  `enemy_boss.rhai`'s awareness check into its own `update_awareness()`
  function up front this time, rather than discovering the failure again.

  **A real balance problem found by actually fighting the boss through
  `TurnHarness`** (same throwaway-test discipline as 4h; deleted after
  confirming 3 cases passed — stairs locked-then-unlocked around the boss's
  death, player takes counter-attack damage, boss stays asleep with no
  line of sight): the first version shipped with 20 hp / 4 dmg, matching
  neither rat stat (6 hp / 2 dmg) by a simple multiplier. Fighting it
  straight-up (no potions, no retreating) means the boss starts landing
  counter-hits almost as soon as the player's own attacks do (same 1-frame
  aggro-then-hit-lands-next-frame timing `enemy_rat.rhai` already has) — at
  4 dmg/hit the boss was on track to kill a full-hp (12 hp) player at
  around turn 5–6, *before* the player could land the 7 hits a 20-hp boss
  needs. Not a script bug — every mechanic fired exactly as designed — just
  numbers that made the fixture's own "finale fight" unwinnable by a
  straightforward melee player. **Retuned to 15 hp / 3 dmg**: a bigger
  health-pool attrition check (2.5× a rat's hp) rather than a damage race
  (matches the player's own 3 dmg/hit rather than exceeding it) — confirmed
  by hand that a full-hp player now wins with room to spare. Worth knowing
  if 4j's balance/determinism tests ever want to assert specific turn
  counts: these are the current numbers, not law — reopen this if the real
  playthrough (see below) says otherwise.

  **Not yet done** (at the time 4i landed): nobody had played `floor1 →
  floor2 → floor3 → victory` start to finish as a human — see the very next
  amendment for what that surfaced.

  **Amendment: the user actually played it, and found potions were dead
  weight.** Collected across three floors but with no way to ever use
  them — by floor 3 the user was at 2 hp with potions sitting unused in
  inventory. Added quaffing to `player.rhai`: "Q" consumes one potion (if
  any, and only if not already at full hp) for +6 hp, costing a turn like
  waiting does — same tier as bump-to-attack/wait, not a separate action
  type. HUD hint updated to advertise it.

  **This surfaced a real staleness bug while implementing it, not after**:
  `resolve_incoming_damage` writes persistent `"hp"` *internally*, but
  `on_update`'s own local `hp` (captured once at the top of the call) never
  saw that write — same "a get_* never observes a set_* from earlier in the
  same pass" rule as everywhere else in this file, just easy to miss when
  the write happens inside a helper function rather than inline. Left
  unfixed, quaffing (or the death check) would silently act on a
  one-frame-stale hp whenever incoming damage landed the same frame —
  e.g. healing from *before* that frame's damage instead of after it.
  **Fixed**: `resolve_incoming_damage` now returns the post-damage hp, and
  the call site reassigns its own local `hp` to that return value. Pinned
  by a throwaway harness test with hand-picked numbers specifically chosen
  so a stale-vs-fresh baseline gives *different, both-uncapped* results
  (hp=4, 2 incoming dmg, +6 heal → correct answer 8, the bug's answer would
  have been 10) — a naive test using round numbers could easily have passed
  either way by coincidence via the `hp_max` cap absorbing the difference.
  Two more throwaway tests confirmed quaffing correctly no-ops (no turn
  consumed, no potion spent) at full hp and with zero potions on hand. All
  three deleted after passing; reran 5× fresh to rule out flakiness, same
  discipline as every other throwaway test this phase.

  **Confirmed: the user played the full run (floor1 → floor2 → floor3 →
  victory) start to finish after this fix landed, and beat it using all
  their potions.** This is the first real end-to-end confirmation the
  whole Phase 4 roguelike actually works as a playable game, not just as
  individually-passing harness checks — closes the "not yet done" item
  above. Using *all* potions to win is a reasonable difficulty signal for
  a fixture (tight but doable), not necessarily evidence the numbers are
  exactly right — revisit balance if a future playthrough feels
  unwinnable or trivial rather than treating 15hp/3dmg boss + 6hp potions
  as final.

**4j** — the plan file's "Tests to write now" list, made permanent. Three
new files:

- `tests/roguelike_level_integrity.rs` (6 tests, pure `LevelData` checks,
  no `TurnHarness`): every level has a pinned nonzero seed; tiles are
  sorted `(layer, y, x)`; every `script`/`next_level` path referenced
  actually exists on disk; spawn isn't inside a solid tile; no cell has
  more than one collider-bearing tile; every level is fully walkable from
  spawn to the stairs and every enemy/boss (BFS flood-fill through
  non-solid terrain).
- `tests/roguelike_combat.rs` (5 tests, `TurnHarness`): bump-attack kills a
  rat in exactly 2 hits and each hit costs a turn; a rat acts at most once
  per player turn even across 10 idle frames; two adjacent rats each land
  their own damage in one resolve (pins the single-writer pattern actually
  sums, doesn't clobber); boss-gated stairs lock/unlock around its death;
  identical input sequences produce identical final state across two
  independent `TurnHarness` instances (the real determinism test — each
  instance's `World`/`ScriptEngine` has its own process-randomized HashMap
  iteration order, so this genuinely catches script-execution-order
  dependence, unlike re-running one instance twice).

  **Immediately caught a real, false-positive-shaped bug in my OWN new
  test**, not the game: the flood-fill's "every enemy is reachable" check
  initially built its blocking set from every tile with `solid == true`,
  which includes rats/bosses themselves (`rat()`/`boss()` mark them solid
  so the player can't walk through a live one — see their own comments in
  `examples/gen_roguelike.rs`). That means a rat's own cell blocked itself
  from ever being "reached" by the flood-fill — correct gameplay behavior,
  wrong thing for a *static architecture* check. **Fixed**: exclude
  `enemy`/`boss`-tagged tiles from the terrain-blocking set (monsters are
  killable/movable obstacles, not walls) while still requiring their cell
  end up in the reachable set once they're excluded from blocking it.

  **Then caught a real regression the quaff amendment introduced**: the
  boss-gated-stairs test (ported straight from 4i's throwaway version, same
  "player attacks 5 times with no potions" setup) started failing at hit
  #5 — after passing cleanly at retune time. Root cause: the quaff
  amendment's hp-staleness fix (`resolve_incoming_damage` now returns the
  post-damage hp instead of leaving the caller's local `hp` one frame
  stale) makes the death check fire up to one frame *earlier* than before,
  since it's no longer reading a lagging value. The pre-fix staleness bug
  had been *accidentally* giving the player one extra frame of being able
  to act even after their true persistent hp had already reached 0 — the
  15hp/3dmg boss retune (Step 4i) was tuned against that bug's behavior
  without knowing it, and fixing the bug (necessary and correct for
  quaffing) tightened the already-thin pure-melee margin enough to flip
  a "just barely wins" fight into "doesn't, without a potion." **Not a
  regression to reverse** — the user's own successful playthrough already
  used potions to get through, which is the intended path now that
  quaffing exists; the fight being close enough to need one is a
  reasonable difficulty signal, not a bug. **Fixed the test, not the
  game**: pre-seeded a deliberately generous hp/hp_max (999/999) for that
  one test, decoupling "does bump-attack correctly kill the boss in 5 hits
  and unlock the stairs" (a mechanic-correctness question, this test's real
  job) from "what are today's exact survivability numbers" (a
  difficulty-balance question the user already validated by actually
  playing, and which can reasonably change again later without
  invalidating this test).

**4k** — docs cleanup, closing out Phase 4. Before writing anything, verified
the phase's actual "done when" criterion against real code (`grep` across
`src/play.rs`/`src/play/spawn.rs` for tag-specific strings/score — clean)
rather than just asserting it.

- `docs/ember2d-scripting-api.md`: fixed the per-entity-scope section, which
  previously claimed the *opposite* of the truth ("`let` variables... survive
  between calls" — they don't; `CallFnOptions::rewind_scope: true`). Also
  fixed a stale D2 note still describing a Phase-1-fixed defect as live, a
  "Phase 2 adds zoom" line that read as if scripts got zoom control (they
  didn't — internal `Camera.zoom` only), and §8's claim about a
  `demo/scripts/api_test.rhai` file that never existed on any branch.
- `docs/ember2d-regression-checklist.md`: rewrote the header (automated
  tests exist now — 74 lib + 19 integration — this list isn't the only net
  anymore); §11 dropped realtime-AABB-movement items (corridor snapping,
  diagonal normalization, corner sliding — none apply to this demo's
  grid-based turn model) and the deleted score/HUD-bar items, added
  bump-to-attack/F3-overlay/HUD-survives-pause items; §12 promoted to the
  primary play-mode section per the plan file's own instruction, expanded
  with turn-cadence/sticky-aggro/quaff items; §13's "per-entity scope
  persists" item **deleted** (per the plan file's explicit instruction — it
  tested behavior Rhai does not provide) and replaced with a note about
  where per-entity state actually lives (globals/persistent) plus a flagged
  known gap around save/load.
- **Logged D17**: found while fixing the save/load checklist item — globals
  (all of the roguelike's own per-entity combat state) aren't part of
  `SaveState`, and `on_start` never re-runs on the `is_loading_save` path,
  so mid-run save/load would silently desync any script relying on globals.
  Not fixed (nothing currently exercises it — no script calls
  `save_game`/`load_game` — so nothing observable is broken today), logged
  per the plan file's own explicit note to do so rather than tested as if
  it worked. Added to `docs/ember2d-refactor-plan.md` §3 alongside D1–D16
  and to the regression checklist's §14 table.
- `docs/ember2d-refactor-plan.md`'s Phase 4 section got an amendment block
  (added, not rewritten — the original plan paragraph stays as historical
  record) documenting what actually shipped vs. what was originally
  planned: `demo/` archived instead of retrofitted, a from-scratch
  roguelike instead of `player_controller.rhai`/`collectible.rhai`/
  `hud.rhai`, and the camera-follow-becomes-scripted bullet explicitly
  reversed (camera stayed in Rust — data-driven engine machinery, not game
  logic; scripting it would leak `exp()`, a named cross-platform desync
  hazard, into script-visible state for no benefit).
- `api_version` bump: already done in Step 4g (3→4) — verified still
  consistent between `src/scripting/types.rs` and the doc's changelog
  table, nothing further needed.

**Phase 4 is now complete.** Full regression re-run after all doc edits:
74 lib tests + 19 integration tests, all passing (docs-only changes, as
expected nothing broke). Next phase per the refactor plan is Phase 5
(simulation extraction, commands, turn scheduler) — not started, not
planned out in any detail yet; read `docs/ember2d-refactor-plan.md` §7's
Phase 5 section and §5 (the seams) before starting it in a future session.

## Camera and HUD decisions already made (don't re-litigate)

- **Camera stays in Rust.** Follow/lerp/clamp contain no tag strings, no
  movement code, no score — it's data-driven engine machinery (the follow
  target comes from the authored `camera_follow` flag), and moving it would
  leak `exp()` (a named cross-platform desync hazard, plan §5.2 H2) into
  script-visible state. Full reasoning in the plan file's "7a" section.

## Workflow reminder

The user wants **one step at a time**: implement, build, test, smoke-test,
report, then wait for explicit confirmation ("next", "lets keep going")
before starting the next step. Don't chain multiple plan steps in one turn.
Commits happen only when the user explicitly says so — Steps 4a–4e all sit
uncommitted together right now, same as 3d+3e did earlier in this refactor.

## Verification commands used throughout this refactor

```
cargo build
cargo build --example gen_roguelike
cargo run --example gen_roguelike        # regenerate roguelike/*.level after editing the generator
cargo test --lib
cargo test --test persistent_on_start --test trigger_collider_layer --test roguelike_floor1 --test roguelike_combat --test roguelike_level_integrity
cargo run -- roguelike/floor1.level            # play mode smoke test
cargo run -- roguelike/floor2.level             # ditto, once floor2 exists (Step 4h on)
cargo run -- --editor roguelike/floor1.level   # editor smoke test
```

**Verifying a new script's actual behavior, not just that it compiles**
(established in 4h): write a throwaway integration test under `tests/`
(auto-discovered by cargo — anything directly under `tests/`, not
subdirectories) using `tests/common/mod.rs`'s `TurnHarness`, teleport
entities into whatever scenario needs checking (see
`play/tests.rs`'s own precedent — direct `world.transforms...position =`
writes, not walking there over several turns), assert on the outcome, run
it, then **delete the file** once it passes — same "don't leave scratch
files behind" discipline as the script-compile-check snippet below. This
is what caught both of 4h's real bugs; a clean smoke-test log and a
passing unit suite caught neither.

One filename gotcha hit in 4h: a test file named with a leading
underscore (`_scratch_rat_combat.rs`) got its compiled test binary blocked
by this machine's Application Control policy before it could even run —
a different failure mode than the already-documented bare-`cargo test`
block below, though the same class of environment restriction. Renaming
to a plain identifier (no leading underscore) fixed it immediately.

Background-launch + log-inspection pattern (Bash tool, adjust the scratch
path): `(cargo run -- roguelike/floor1.level > /tmp/play.log 2>&1 &) ; sleep 8;
tasklist | grep -i ember2d` then check the log for panics, `taskkill //F
//IM ember2d.exe //T` to clean up. Two things this pattern does NOT catch,
learned the hard way this session — check for both separately:
- **Script compile/runtime errors** render to the in-game `script_log`
  only, never stdout/stderr. A clean log file does not mean the scripts
  work — verify with the throwaway `ScriptEngine::compile` snippet above
  (or actually play it) before trusting anything script-related.
- Bare `cargo test` (no `--lib`) also tries to build/run a test binary for
  `main.rs` and fails in this environment with "An Application Control
  policy has blocked this file" — unrelated to any code change, always run
  `--lib` plus the named integration tests instead.

## Other durable context

- No `.rs` file may exceed 600 lines (CLAUDE.md hard limit) — split into a
  sibling submodule when approaching it. `src/editor/ui/panels.rs` is
  already at 662 lines — pre-existing debt from before this refactor, out
  of scope unless the user asks for it.
- Rhai facts that surprised us mid-phase, all verified against the rhai
  1.24 source and this project's own `ScriptEngine::call_fn` usage (full
  detail + evidence in the plan file): a script's `let` does NOT persist
  between `on_update` calls (contradicts current
  `docs/ember2d-scripting-api.md` §2 — will be corrected in Step 4k); a
  `get_*` never observes a `set_*` from earlier in the same pass, including
  your own script called twice — this bit us for real in 4e (see above),
  guard with a pattern like `or_zero()` anywhere a lazy-inited value is
  read back same-call; `random_*` draws from one shared stream consumed in
  nondeterministic script order, so it's unsafe anywhere in gameplay logic,
  cosmetic effects (e.g. `torch.rhai`, once it exists) only; **two different
  scripts writing the literal same global key in the same pass silently
  last-write-wins with no error** (`ScriptState::pending_globals` is a plain
  `HashMap`, not a merge/append structure) — found in 4h between
  `enemy_rat.rhai`'s hp lazy-init and `player.rhai`'s `attack()`, fixed by
  moving the init to `on_start` (a separate, earlier pass) instead of
  avoiding the shared key entirely, since the write is unconditional and
  the value doesn't depend on frame timing; **a "have I already resolved
  up to X" marker must track the highest value actually seen, never the
  raw source-of-truth counter** — a "turn"-driven marker like
  `last_resolved_turn` that gets set to the *current* turn value on the
  same frame that value first becomes visible will always run one frame
  ahead of anything another script tries to publish *for* that same turn
  value (since that publish only commits at the end of the frame it's read
  in) — found in 4h in `player.rhai`'s `resolve_incoming_damage`, silently
  dropping every enemy attack since Step 4b; will recur in `enemy_boss.rhai`
  (4i) if the same "mark to current value" shortcut gets reused there.
- This doc, the plan file, and Claude's own memory
  (`C:\Users\ronal\.claude\projects\C--dev-Ember2D\memory\`) can drift from
  the actual code. Before acting on anything above, verify against
  `git log` / the current source rather than trusting this snapshot blindly.
