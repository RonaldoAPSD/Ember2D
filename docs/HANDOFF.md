# Session Handoff (temporary)

> Not part of the permanent doc set — a scratch note for picking the current
> refactor phase back up in a fresh session/chat. Safe to delete once the
> phase in progress wraps (or fold anything still useful into
> `ember2d-refactor-plan.md` first). Rewritten from scratch for the Phase 6 →
> Phase 7 transition — everything below Phase 4-era detail (git log
> `9795993` and earlier, or `docs/archive/` for the Phase 5/5.5 plan docs) is
> gone from this file; it lives in `docs/ember2d-phase6-plan.md` and
> `docs/ember2d-refactor-plan.md` §3/§7 instead, not duplicated here.
> Last updated: 2026-09-06.

## Where things stand

Branch: `claude` (only branch we ever commit to — never `main`). Nothing on
`claude` is committed yet as of this writing — the user's standing
instruction this phase was "commit at the end of the phase, not after every
step" (see memory), so Phase 6's 14 steps all sit uncommitted together,
same as Phase 4's 4a–4e once did.

**Phase 6 (performance and data-model hardening) is done — all 14 steps.**
Full step-by-step account, every measured number, and the reasoning behind
every design call live in `docs/ember2d-phase6-plan.md` — this file is a
summary and pointer, not a replacement for reading that one before
continuing related work.

- Steps 1–12: done, each independently verified (build/test/replay-gate
  where applicable/manual smoke test), each with its own `git diff --stat`
  scope check. Headline result: `roguelike/floor2.level`'s p50 ms/step went
  from 9.723ms to 1.817ms (-81%), allocs/step from 35,299 to 6,598 (-81%).
- Step 13 (`DrawList` buffer reuse): **skipped**, per the plan's own
  explicit condition — a verified F3 debug-overlay reading of `FPS:59` on an
  unoptimized-own-code debug build confirmed the target was already met by
  Steps 1–12's cumulative work.
- Step 14 (documentation): done — this pass. `docs/ember2d-refactor-plan.md`
  (D11 closed with final numbers, D21 added, §5.3/§5.4 deferrals noted, §7
  Phase 6 amendment block), `docs/ember2d-scripting-api.md` (collision-layer
  bitmask semantics section, previously entirely absent), and
  `docs/ember2d-regression-checklist.md` (§14 defect table, §17 performance
  checkboxes, a collision-layer save/load manual item) were all updated;
  `CLAUDE.md`'s "Current State" section below.

Two items the original Phase 6 plan asked for were **deferred, not built** —
see `docs/ember2d-phase6-plan.md` §0 for the full reasoning: `EntityId {
index, generation }` (no benefit until Phase 9's netcode needs
authority-prefixed ranges; ~79 call sites would need to change) and a binary
snapshot path (`rhai::Dynamic` can't serialize through a non-self-describing
format without a hand-written tagged-value enum first; its only consumer,
Phase 9c rollback, is itself conditional). A component-registration macro
was **rejected outright**, not deferred — see Step 11's own write-up for why
CLAUDE.md's "deliberate learning artifact" rule and the step's own finding
(the macro wouldn't have prevented the actual bug either) both argue against
it.

Three live-reported correctness bugs jumped the queue mid-phase, out of the
original planned scope, and are already fixed: D19 (dropped input during an
animation-blocked frame), D20 (the same gate's per-actor fix), and a
hot-reload syscall throttle (D21). One defect was found and deliberately
left unfixed: D22 (a cancelled timer and a just-fired one share a storage
sentinel, so `timer_done` over-reports "done" for ~8 minutes) — logged
because no shipped script calls any timer function, so nothing observable
breaks today; see `docs/ember2d-refactor-plan.md` §3 for the fix a future
session would need.

## What's next

Per `docs/ember2d-refactor-plan.md` §7, **Phase 7 (editor viewport
panelization and chrome)** is next — not started, not planned out in detail.
Read that section and `docs/ember2d-refactor-plan.md` §6 (the `EditorUi`
abstraction it references) before starting it.

Separately, the user asked (this session) for a feasibility assessment of a
small Pokemon-style RPG demo (dialogue + menu HUD widgets — `draw_menu`/
`draw_panel` are registered and rendered but **zero shipped script uses
them**). Verdict: the minimal loop (NPC dialogue, wild encounter, Attack/Run
battle) is buildable today with zero engine changes, but a fuller RPG genre
hits eight real gaps — no `set_script` for spawned entities, no structured
party/roster storage, no positional continuity across `load_level`, no
dialogue-tree/text-wrap helpers, `no_module` blocking script code sharing,
no inventory/item-catalog concept, and only `TurnModel::Alternating` wired
up despite `Energy`/`ActionCost` existing in the type system. Full detail in
`docs/ember2d-rpg-demo-feasibility.md`. The user did not ask for this to be
built — it's a reference doc, not a queued task.

## Housekeeping done this session, unrelated to Phase 6 itself

- `docs/ember2d-phase5-plan.md` and `docs/ember2d-phase5.5-plan.md` moved to
  `docs/archive/` (completed phases); `CLAUDE.md`'s references to them
  updated to the new path.
- `.gemini/` and `.github/` deleted at the user's request (the latter
  because GitHub Actions billing is unavailable — CI is manual-only now;
  `docs/ember2d-regression-checklist.md` §15/§17's own text about CI running
  `tests/replay.rs` per-push is stale as a result and should be corrected
  next time that section is touched).

## Workflow reminders (standing, not phase-specific)

- **One step at a time**: implement, build, test, smoke-test, report, wait
  for explicit confirmation before the next step. Established since Phase 4,
  held throughout Phase 6's 14 steps without exception.
- **Commit only at the end of a phase**, not after every step (new this
  session, superseding the implicit "commit whenever" default earlier
  phases used) — confirm with the user before committing Phase 6's work,
  don't assume finishing Step 14 alone is the go-ahead.
- Only ever commit to `claude`, never `main`.
- The 600-line `.rs` file hard limit is enforced by splitting into a sibling
  file (`#[path = "..."] mod ...;`) or a true child module
  (`mod foo;` → `parent/foo.rs`), not by shrinking comments — both patterns
  are precedented multiple times in `ember2d-sim/src/scripting/` and
  `ember2d-sim/src/simulation/`.
- A `SendFeedback` note was filed this session after a real safety mistake:
  an early automated FPS-check attempt trusted `AppActivate`'s reported
  success without independently verifying window focus via
  `GetForegroundWindow`, and a keystroke may have gone to an unrelated
  foreground application instead. Any future window/input automation in
  this environment (PowerShell `SendKeys`, focus-dependent screenshots, etc.)
  must verify focus by handle comparison before *and* after sending input —
  never trust `AppActivate`/`Start-Process` alone.

## Verification commands used throughout Phase 6

```
cargo build --workspace --examples
cargo test --workspace --lib
cargo test --test <name> [--test <name> ...]     # all 11 named integration tests
cargo test --test replay                          # run 5x as independent fresh processes for §3/§7/§8-class changes
cargo run --release -p ember2d-sim --example bench_sim   # headless perf bench, always --release
cargo run --example gen_roguelike                 # regenerate roguelike/*.level after a generator edit
cargo run -- roguelike/floor2.level               # play mode smoke test (heaviest shipped level)
cargo run -- --editor roguelike/floor1.level      # editor smoke test
```

## Other durable context

- No `.rs` file may exceed 600 lines (CLAUDE.md hard limit).
- This doc, the phase plan docs, and Claude's own memory
  (`C:\Users\ronal\.claude\projects\c--dev-Ember2D\memory\`) can drift from
  the actual code. Before acting on anything above, verify against `git
  log` / the current source rather than trusting this snapshot blindly.
