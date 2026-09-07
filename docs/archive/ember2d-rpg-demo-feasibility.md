# Ember2D — Dialogue/Menu RPG Demo: Feasibility & Gaps

**Written:** 2026-09-06, during design work for a proposed third demo (a small
Pokemon-style loop: talk to an NPC for a starter creature, walk into grass for
a chance at a wild-creature battle) meant specifically to exercise the
engine's dialogue/menu HUD widgets. That demo was not built — this doc
records why, what actually *is* buildable today, and what's genuinely missing
for the fuller version of this genre.

---

## 1. The short answer is more nuanced than "the engine can't do this"

A **scoped-down version** — exactly what was asked for: one NPC, one starter
creature, one wild species, a two-option (Attack/Run) battle, a single small
map — is realistically buildable **today, with zero engine or Rust changes**,
purely as `.rhai` scripts and generated `.level` content, the same way
`roguelike/` and `shooter/` were built. A full design was produced and
validated against the actual engine source (traced `Simulation::step`'s
per-frame control flow, `TurnScheduler`, the deferred-write semantics, and
`ctx.draw_menu`/`ctx.draw_panel`'s real rendering wiring) before the decision
was made not to implement it right now. The pieces line up:

- **No `spawn_entity` needed at all.** The NPC and the wild creature can both
  be plain, statically-authored level tiles (like `roguelike`'s items and
  enemies) — sidestepping the one real hard blocker (§2.1) entirely, since
  nothing needs to be spawned at runtime.
- **State management is sufficient.** `ctx.set_global`/`get_global`
  (level-scoped) and `ctx.set_persistent`/`get_persistent` (survives a
  `ctx.load_level` transition) are exactly what's needed to carry a starter
  flag and a creature's HP from the overworld into a separate battle level
  and back.
- **The turn/command boundary (`on_input`→`ctx.submit()`→`on_turn`→`ctx.act()`)
  already supports everything a dialogue box or a battle menu needs**: since
  a level with no other scheduled actors keeps the player permanently at the
  front of `TurnScheduler`, `on_input` runs every single step with zero added
  latency versus a naive direct-mutation approach — so dialogue advancement
  and battle-menu confirmation can (and should) go through the same
  replay-safe boundary every other turn-based script in this codebase uses,
  at no real cost.
- **`ctx.draw_menu`/`ctx.draw_panel` are fully wired to real rendering**
  (`ember2d-sim/src/scripting/api.rs` → `ember2d/src/ui.rs`'s `Menu`/`Panel`
  types, drawn in `ember2d/src/play.rs`'s `HudDraw` match arm) — they are not
  stubs. They are simply **unexercised**: grepping every shipped script in
  `roguelike/` and `shooter/` finds zero calls to `draw_menu`, `draw_panel`,
  or `clear_hud`. The only place they're used at all is
  `docs/archive/demo/scripts/ui_test.rhai` — an archived, superseded demo
  that predates this refactor. `draw_box`+`fill_rect`+`draw_hud` (the
  death/victory-screen idiom) is the *only* HUD pattern actually proven in
  current shipped content.

So the honest framing isn't "the engine can't do this" — it's "the engine
*can* do a small, faithful proof-of-concept of this genre today, but every
piece of it beyond that small scope runs into a real, specific gap." The rest
of this document is that gap list.

---

## 2. Where it genuinely stops scaling

### 2.1 No `set_script` — a spawned entity can never carry its own behavior

There is no `set_script` function registered anywhere in
`ember2d-sim/src/scripting/api.rs`. `spawn_entity`/`spawn_entity_full`'s
`SpawnRequest` only ever builds a transform, sprite, collider, and tag — never
a script reference (confirmed by reading both the registration call sites and
the `apply_ctx` spawn-queue drain in `ember2d-sim/src/scripting/apply.rs`).
This is exactly why `shooter/scripts/director.rhai` exists: one
always-present, hand-written entity iterates every spawned enemy/bullet *by
tag* and drives all of their behavior itself, because nothing it spawns can
ever be given an `on_update`/`on_turn` of its own.

For a single NPC and a single wild creature, this is a non-issue — both can
be authored directly in the level file, need no runtime spawning, and
therefore need no script of their own at all (all logic lives in the
player's own script, reading/writing the other entity's state by tag —
exactly `director.rhai`'s pattern, just for one entity instead of dozens).

It becomes a real constraint the moment the genre wants **more than a
handful of independently-behaving actors that must be created at runtime** —
a wild encounter table that spawns a *random* creature (rather than one
fixed species), a town with several NPCs that each need their own dialogue
state machine, or a battle system where fainted party members are swapped
out for freshly-spawned replacements. All of that would have to be funneled
through one director-style script per level, hand-dispatching on tags —
workable for a small roster, unmanageable past it.

**What would close this gap:** a `set_script(id, path)` API (or an
equivalent "attach behavior to an entity after the fact" primitive), so a
runtime-spawned entity can be given its own `on_update`/`on_turn`/`on_collide`
instead of requiring one omniscient script per level to drive it by hand.

### 2.2 No structured party/roster state — persistent storage is flat scalars

`ctx.set_persistent`/`get_persistent` back onto a
`BTreeMap<String, rhai::Dynamic>` (`SaveState::persistent`,
`ember2d-sim/src/save.rs`). `rhai::Dynamic` can technically hold arrays and
maps, but **nothing shipped in this engine has ever exercised persisting a
nested structure** — every persistent value in `roguelike/scripts/player.rhai`
(`"hp"`, `"gold"`, `"potions"`, `"depth"`, `"turns_taken"`) is a bare scalar,
and `SaveState`'s own round-trip tests only cover scalars and a handful of
flat maps.

A single starter creature's HP is a scalar — no issue. A **party of several
creatures**, each with its own species, HP, and moves, would need to be
hand-rolled entirely via naming convention (`"party_0_hp"`, `"party_0_species"`,
`"party_1_hp"`, ...) since there's no first-class "roster" concept anywhere
in the engine, and no precedent for whether a nested array/struct actually
survives a real RON round-trip through `SaveState::to_ron`/`from_ron`
untested.

**What would close this gap:** either a documented, tested convention for
storing structured data in persistent (proving `rhai::Dynamic` arrays/maps
really do round-trip through RON cleanly), or a new first-class component
(e.g. `Party`/`Roster`) with its own serialization, the way `Actor`/`Collider`
etc. already exist as typed components rather than script-side bookkeeping.

### 2.3 No positional continuity across a level swap

`ctx.load_level(path)` always spawns the player at the *target level's own
fixed* `LevelData.spawn_point` (`Simulation::do_on_start`) — there is no
"resume where this transition was triggered from" mechanism anywhere in the
engine. This is fine for the roguelike (stairs are always one-way, into a
level you've never been in) but is a real authenticity gap for a
Pokemon-style overworld↔battle transition, where a real implementation
expects to return to the *exact* tile you were standing on when the
encounter fired. The workaround (accept that every battle returns you to the
overworld's one fixed spawn point) is invisible on a single small room, but
would read as an obvious limitation the moment the overworld map is bigger
than one screen.

**What would close this gap:** either a per-session "return point" the
engine tracks automatically across a `load_level` pair (something like a
transition stack, mirroring how `PauseMenuState` already pushes/pops on top
of `PlayState` without losing world state), or a scripting API to read back
"the position this level was entered from."

### 2.4 `draw_menu`/`draw_panel`/`clear_hud` are wired but battle-untested

As noted in §1, these three functions have real rendering behind them but
have never been exercised by a real script under real play. Anyone building
on them first would be the one finding whatever rough edges exist —
concretely, `Panel::draw` (`ember2d/src/ui.rs`) renders its title text with
`(fg, bg)` swapped relative to the panel's own body colors, which is
easy to get backwards the first time (not a bug, just unproven-by-use
behavior with no existing example to copy).

**What would close this gap:** landing even one small shipped example that
actually uses all three (a natural side effect of building any version of
this demo), establishing a proven idiom the way `fill_rect`+`draw_box`+
`draw_hud` already is for death/victory screens.

### 2.5 No dialogue-tree or text-formatting primitives

`draw_menu` returns nothing on its own — a script must maintain its own
`selected` index, decide what each option means, and redraw the whole widget
every frame. There is no built-in yes/no branching helper, no portrait/name
plate convention, no line-wrapping helper (a long dialogue string must be
manually split into lines that fit the box width), and no "type-on"
character-reveal effect — all things a text-heavy RPG typically wants and
would have to be hand-built per script, with no shared code possible between
scripts (§2.6).

**What would close this gap:** not necessarily new engine functions — these
could all be pure-Rhai patterns — but at minimum a documented convention
(the way `docs/ember2d-scripting-api.md` documents the `or_zero()` pattern
for deferred-write races) so every future dialogue-heavy script doesn't
reinvent line-wrapping from scratch.

### 2.6 Rhai's `no_module` build means zero code sharing between scripts

`ember2d-sim`'s `rhai` dependency is built with the `no_module` feature
(`ember2d-sim/Cargo.toml`), so scripts cannot `import` shared code — this is
precisely why the roguelike has one `pickup.rhai` shared by every pickup
*tile* rather than N nearly-identical files, but it also means there is no
way to factor out common dialogue/menu-drawing logic into a shared library a
future NPC/merchant/healer script could all `import`. Every script that wants
the same dialogue-box or menu-drawing helper must duplicate it verbatim.
Fine for one NPC's dialogue; painful past a handful of NPCs each needing
slightly different conversation logic.

**What would close this gap:** enabling Rhai module support (a real,
documented tradeoff against the reasons `no_module` was originally chosen —
this hasn't been re-investigated as part of this doc), or a Rust-side
"shared script fragment" injection point scripts could opt into.

### 2.7 No generic inventory/item-catalog concept

The roguelike's gold/potions are two hardcoded counters directly in
`player.rhai` — there is no engine-level "item" type, catalog, or bag concept
anywhere. A creature-catching game would want an inventory of catch-items,
healing items, etc.; today that would mean the same kind of one-off
persistent-counter-per-item-type approach the roguelike already uses,
scaled up by hand for however many item types exist.

**What would close this gap:** not necessarily required for a *minimal*
demo (a fixed single catch mechanic needs no inventory at all), but a real
item system would need at least a documented pattern, if not a first-class
`Inventory` component.

### 2.8 Only one turn-scheduling model is actually wired up

`ember2d-sim/src/scheduler.rs` defines `TurnModel::{Alternating, Energy,
ActionCost, Declared}` in the refactor plan's design language, but **only
`Alternating` (flat, equal-cost turns) is ever actually used** —
`Actor::speed` is read/write-able via `ctx.get_speed`/`set_speed` but is
currently vestigial, since `TurnScheduler::advance` always charges
`ALTERNATING_COST` regardless of it (`scheduler.rs`'s own doc comment on
`ALTERNATING_COST` says so explicitly). A real Pokemon-style speed-based
turn order ("the faster creature moves first this round") has no engine
support today beyond a script re-sorting participants by hand every round —
workable for a two-participant battle (player vs. one wild creature), a real
constraint for anything with more combatants.

**What would close this gap:** wiring up the already-designed
`Energy`/`ActionCost` scheduling modes so `Actor::speed` actually affects
turn order, rather than existing only as an honestly-functioning but
currently-inert read/write pair.

---

## 3. Bottom line

Nothing in §2 blocks the demo as originally scoped (one NPC, one starter, one
wild species, Attack/Run only, one small map) — that specific shape was
fully designed and validated against the real engine control flow, and would
have needed zero engine changes to build. The gaps above are what would
start to bite the moment this genre grows past that scope: more than one or
two runtime-spawned actors per level (§2.1), a real multi-creature party
(§2.2), a bigger overworld where "always respawn at the fixed entry point"
stops feeling acceptable (§2.3), heavier reliance on the still-unproven
`draw_menu`/`draw_panel` widgets (§2.4), richer dialogue (§2.5, §2.6), a real
inventory (§2.7), or combat with more than two participants needing
speed-based ordering (§2.8).

If this genre is picked back up later, §2.1 (`set_script`) and §2.3
(return-point continuity) are the two gaps most likely to matter first —
everything else is either a documentation/convention problem (§2.5, §2.6) or
scales gracefully from the minimal design already validated (§2.2, §2.7,
§2.8).
