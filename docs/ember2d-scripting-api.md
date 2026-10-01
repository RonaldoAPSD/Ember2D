# Ember2D — Scripting API Reference

**Written against:** `claude` branch, v0.5.0 (`ember2d-sim/src/scripting/api.rs`, registrations in `ember2d-sim/src/scripting/engine.rs` — moved here from bare `src/scripting/` by Phase 5 Step 5i's workspace split, docs/archive/ember2d-phase5-plan.md §5.5)
**Status:** the API is largely **built**. This document is a reference plus a record of what the refactor changes.

---

## 1. Why this matters

The scripting API is Ember2D's real public contract. Games are written in Rhai, not Rust. Treat a breaking change here like a breaking change to the level format.

**Completion test:** build the demo into a full game — controller, enemy AI, combat, inventory, HUD — using only `.rhai`, with zero Rust changes. Phase 4 of the refactor is the first real attempt at this, since the player controller is still hardcoded in `play.rs`.

---

## 2. Model

### Lifecycle
```rhai
fn on_start(id, ctx)
fn on_load(id, ctx)
fn on_input(id, ctx)
fn on_update(id, ctx)
fn on_turn(id, ctx)
fn on_collide(id, other, ctx)
```
All optional. A missing function is not an error, and since Step 9.5-5 it
costs nothing either: the engine notes which of these each script defines
when it compiles it, and doesn't call the rest. A bullet whose script has
only `on_collide` is free every frame until it hits something. Before,
every scripted entity's `on_update` was looked up every step, and a miss
built an error value that was then thrown away.

`on_load` (Step 7.5-5, docs/ember2d-master-plan.md §5.6) runs once per
scripted entity, **instead of** `on_start`, on the one path `on_start`
never runs: loading a save. Several scripts' `on_start` writes are
unconditional (a fresh "hp"/"hp_max" seed, say) — re-running them on load
would silently reset a run already in progress, which is exactly why
`Simulation::on_start`'s loading-save branch has always skipped `on_start`
entirely (R7, 7A-3). `on_load` gives a script a hook on that same path for
whatever it still needs to do once, on load — re-deriving presentation-only
state a save doesn't carry, say — without touching `persistent`/`globals`,
which the save already restored faithfully. Missing `on_load` is exactly as
fine as a missing `on_start`; most scripts need neither —
`demos/roguelike/scripts/player.rhai`'s own `on_start` is the "fresh
'hp'/'hp_max' seed" example above verbatim (moved there from a lazy-init
inside `on_update` by this same step, once `on_start` became reliably
fresh-spawn-only rather than needing to double as "first on_update after
either a fresh spawn or a load"), and it has no `on_load` at all — nothing
it owns needs re-deriving after a load.

`on_input` is Step 5e (docs/ember2d-phase5-plan.md) — see "Input" in §3
below for what it's for and why `on_update` shouldn't read raw keys
anymore. `on_turn` is Step 5f — see "The command boundary" in §3 for what
it's for and why `on_update` shouldn't mutate turn-based gameplay state
anymore either. In a `TurnBased` project, per-step call order is:
`on_input` (only for a locally-controlled actor awaiting a command) → every
scripted entity's `on_update` → `on_turn` (only for whichever single actor
`TurnScheduler` is resolving this step) → `on_collide` (only if a turn was
actually resolved). `on_update` running *before* `on_turn`, not after, is
deliberate and load-bearing: `on_update` is where a script's own lazy-init
typically lives (see `demos/roguelike/scripts/player.rhai`'s header comment), and
`on_turn` reads that same state — the reverse order would mean a level's
very first turn reads pre-init values.

### Deferred mutation
Writes queue and apply after all scripts run. Consequences:
- `set_position` isn't visible to a later `get_x` in the same pass.
- Two scripts writing the same field: last write wins, order unspecified.
- `spawn_entity` returns a usable id immediately, but the entity doesn't exist until the queue drains.

This is the foundation Phase 5's command layer builds on.

### Per-entity scope
**Corrected in Step 4k — this section previously claimed the opposite of
the truth.** Each entity keeps a persistent `rhai::Scope` object, but a
script's own `let` declarations do **not** survive between
`on_start`/`on_update`/`on_collide` calls: `ScriptEngine::call_fn` uses
Rhai's default `CallFnOptions`, whose `rewind_scope: true` discards
anything the script itself declared once the call returns (verified
against the rhai 1.24 source, not just observed behavior).

**Corrected again, Phase 6 Step 9** (docs/ember2d-phase6-plan.md): this
section used to go on to say the scope persists as a place the *engine*
writes into directly between calls, with timers as the one example
(`__timer_*` variables, set via `Scope::set_value` from `apply_ctx`). That
stopped being true the moment Step 9 gave timers their own store —
`ScriptEngine.timers`, keyed by entity id directly, never smuggled through
Rhai `Scope` variables at all. Nothing writes into an entity's `Scope`
between calls anymore; it is currently dead state, kept alive only for
hot-reload/despawn bookkeeping (R22, docs/ember2d-master-plan.md §3.2,
scheduled for cleanup in step 7.5-10).

A per-entity value a script needs to remember across calls goes through
`ctx.set_var`/`get_var`/`has_var`/`remove_var` (Step 7.5-3, docs/ember2d-
master-plan.md §5.6) — real per-entity component state (`Vars`,
`components/vars.rs`), automatically cleared when that entity despawns. See
`demos/roguelike/scripts/enemy.rhai` and `demos/shooter/scripts/
director.rhai`'s `spawn_enemy`/`resolve_hits` for this in practice. Before
7.5-3, the only way to fake per-entity scope was `ctx.set_global`/
`get_global` keyed by string concatenation (`"hp_" + id`) — that
convention is gone from every shipped script, but the mechanism it was
built on (`set_global`/`get_global`, still the right tool for a
genuinely LEVEL-scoped value — resets on every level load — with
`set_persistent`/`get_persistent` for a value that must survive a level
transition) has the same sharp edge `set_var`/`get_var` inherits: **a
`get_*` never observes a `set_*` from earlier in the same script pass** —
every write is deferred and only applied after every script has run that
frame, so a value lazy-initialized this same call still reads back as `()`
(or, via `add_global`/`add_persistent`/`add_var`, accumulates correctly
regardless — see §4 "State" below), not the value just set. A read that
might race its own same-pass lazy-init needs either restructuring (pass
the just-computed value as a parameter instead of re-reading it — see
`demos/roguelike/scripts/player.rhai`'s `on_update`/`draw_hud`) or to
simply happen in a LATER pass than the write (every `on_turn` read in the
demo scripts qualifies, since `on_turn` always runs after that step's own
`on_update` has fully committed — see either script's header comment for
the two real bugs this caused before that ordering guarantee was
understood, and how they were found).

### `ctx` carries the calling entity
`ctx.with_entity(id)` means `start_timer` / `timer_done` / `cancel_timer` need no id argument, and `raycast` skips self.

---

## 3. The live surface

Everything below is registered and callable today.

### Transform
`get_x(id)` · `get_y(id)` · `get_position(id)` → `[x,y]` · `get_vel_x(id)` · `get_vel_y(id)` · `get_velocity(id)` → `[x,y]` · `set_position(id,x,y)` · `set_velocity(id,vx,vy)`

### Tags and lookup
`get_tag(id)` · `set_tag(id,tag)` · `has_tag(id,name)` · `find_by_tag(name)` → id or -1 · `find_all_by_tag(name)` → array · `count_by_tag(name)` · `entity_exists(id)`

### Appearance
`get_glyph(id)` · `set_glyph(id,"X")` · `get_color(id)` → `[fg,bg]` · `set_tint(id,fg,bg)` · `get_texture(id)` · `set_texture(id,path)` · `is_visible(id)` · `set_visible(id,bool)` · `get_layer_order(id)` · `set_layer_order(id,z)`

> **Tileset sprite tiles (Step 8-2, level format v5).** A tile painted from
> an imported tileset (`<project>/assets/tilesets/<name>.ron`) is drawn as
> that sheet's named region. Scripts see it as a textured sprite:
> `get_texture(id)` returns the sheet image's path, and `get_glyph(id)` has
> no glyph to report (the tile's authored glyph is only a fallback used when
> the tileset can't be found). `set_texture(id, path)` replaces it with the
> whole image at `path`, as before; `set_sprite` (below) picks a region by
> name. `set_src_rect` (§7) is still outstanding.

**Sprites** (Step 9-7): `set_size(id,w,h)` · `set_flip(id,fx,fy)` · `set_sprite(id,tileset,region)` · `play_project_clip(id,name)` · `play_project_clip_once(id,name)` · `set_y_sort(on)`

- `set_size(id, w, h)` draws `id` at `w`×`h` world units (one unit is one
  level cell); `0` or less goes back to natural size. Whole, decimal and
  mixed numbers all work.
- `set_flip(id, fx, fy)` mirrors the image left-right and/or upside down —
  a character facing the way it walks. Images and clip frames only; a
  glyph draws as it is.
- `set_sprite(id, tileset, region)` draws `id` as a named region of a
  project tileset (`assets/tilesets/<tileset>.ron`, made with File >
  Import Tileset), one cell big, untinted (`set_tint` after it still
  colours it). A missing tileset or region logs one warning and leaves the
  sprite alone.
- `play_project_clip(id, name)` loops the project clip
  `assets/clips/<name>.ron` (File > Animation Clips) — loading it the
  first time, unlike `play_clip`, which only knows clips a script
  registered or a tile uses. `play_project_clip_once` plays it once and
  stops on the last frame.
- `set_y_sort(true)`: among sprites with the same layer order, whichever
  is lower on screen draws in front — characters walking past each other
  and in front of / behind furniture. Saved with the game. Give the
  sprites that should interleave the same `set_layer_order`.

Colours are **name strings** (`"Red"`, `"Reset"`) or an explicit `"#RRGGBB"` hex value (Step 3e). Unknown names silently become `Reset`.

### Animation clips
`register_clip(name,"abc",fps,looping)` defines (or redefines) a named clip from a string of glyphs. `play_clip(id,name)` plays it respecting the clip's own `looping` flag; `play_clip_once(id,name)` plays it but always stops on the last frame. `stop_clip(id)` · `set_clip_speed(id,x)` · `get_frame(id)` → int · `set_frame(id,n)` · `clip_finished(id)` → bool, true for exactly the tick a non-looping run reaches its last frame.

Since Step 8-3 a project can also author **sprite-sheet clips** in the editor (File > Animation Clips...), saved as `<project>/assets/clips/<name>.ron` — frames are named regions of one tileset. A tile placed from a palette entry that names a clip plays it on loop automatically (level format v6). Every clip a level's tiles use is loaded with the level, so `play_clip(id, name)` can play it on any entity too; a project clip that no tile in the level uses is loaded by `play_project_clip` (Step 9-7, below).

> Replaces `set_animation(id,"abc",rate)`, removed in Step 3e — a clip is named and shared, not a bag of fields re-set every call.

### Entity lifecycle
> **Paths** (Step 9-6): every path a script passes — `set_script`,
> `set_texture`, `play_sound`/`play_sound_at`/`play_music`,
> `load_level`, a scene's `script` — is **project-relative**:
> `"scripts/bullet.rhai"`, `"audio/hit.ogg"`, `"floor2.level"`. It's
> looked for beside the current level, then in each folder above it up to
> the project root (the folder with `project.ron`), then in the working
> directory — so a project works wherever its folder is, and an older
> repo-relative path (`"demos/roguelike/..."`) still loads when the game
> runs from the repo root.

`spawn_entity(glyph,x,y,tag)` → id · `despawn(id)` · `set_script(id,path)`

`set_script` (Step 7.5-5) attaches (or replaces) `id`'s script — deferred
like every other setter, so `on_start` for the newly-attached script runs
at the *next* step, not this one (matching `spawn_entity`'s own "usable id
immediately, entity exists once the queue drains" contract above). What
`spawn_entity` alone could never do: give a spawned entity its own
`on_update`/`on_collide`, rather than needing every enemy/bullet driven by
hand from whichever script spawned them — see `demos/shooter/scripts/
director.rhai`'s pre-7.5-5 header comment for what that used to force. A
path that fails to compile is never attached (logged the same way a bad
`script:` field in a level file already is).

> **Defect D10, fixed in Phase 1** (docs/ember2d-refactor-plan.md §3): this
> 4-arg overload used to hardcode every appearance/collider detail
> regardless of what the script asked for. It's now deliberately kept as a
> convenience default (white glyph, z-order 2, a 1×1 non-solid trigger with
> no layer) — existing scripts calling it see no behavior change — with a
> second, extended overload for everything else:
>
> `spawn_entity(glyph,x,y,tag,fg,bg,z,solid,w,h,layer)` → id — same Rhai
> name, picked by argument count. `fg`/`bg` are color names/hex exactly
> like `set_tint`; `z` is draw order; `solid` marks a physical obstacle
> rather than a trigger; `w`/`h` are the collider size; `layer` is the
> collision layer name (empty = unlabeled, matching the trigger-layer
> default from defect D4).

### Colliders
`get_collider_w(id)` · `get_collider_h(id)` · `set_collider_size(id,w,h)` · `is_collider_solid(id)` · `set_collider_solid(id,bool)` · `get_collider_layer(id)` · `set_collider_layer(id,name)` · `get_collider_mask(id)` · `set_collider_mask(id,array)` · `is_collider_locked(id)` · `set_collider_locked(id,bool)`

An empty mask means "collide with everything".

`is_collider_locked`/`set_collider_locked` are a real flag, not a layer
name (defect D12, docs/ember2d-refactor-plan.md §3) — an exit trigger
checks this to gate a level transition (a locked door/stairs still detects
overlap normally, it just doesn't fire). Independent of layer/mask
filtering entirely; `false` by default.

> **Layer/mask names resolve to a bitmask internally (Phase 6 Step 7,
> docs/ember2d-phase6-plan.md), with no script-facing API change** —
> `get_collider_layer`/`set_collider_layer`/`get_collider_mask`/
> `set_collider_mask`, and `raycast`/`get_path`'s own `mask` array argument,
> still take and return plain name strings exactly as before. Semantics
> worth knowing, since the bitmask has a hard limit a string-based API
> didn't:
> - A level's usable layer names are `LevelData.collision_layers: Vec<String>`
>   (a project/level-authored list, `["solid"]` by default for any level
>   saved before this field existed). Names are assigned bits **in list
>   order** — the Nth name gets bit N — not by hashing the name, so two
>   machines given the same list always agree on the same bits.
> - **At most 31 layer names per level.** Bit 31 is reserved internally
>   (`LAYER_UNKNOWN`); a 32nd name has no bit left to claim.
> - The registry is built once, from `collision_layers`, before any script
>   runs, and is **never grown at runtime** — a layer name a script sets
>   that isn't in that list doesn't get a new bit allocated for it.
> - An empty layer name, or a name not present in `collision_layers`,
>   resolves to `0` — the same value as "collide with everything" — so an
>   unregistered-layer collider still matches an empty-mask collider (they
>   share the value that means "everything"), and an unregistered layer name
>   is functionally indistinguishable from having no layer at all.
> - A **mask** naming a layer the registry doesn't recognize behaves
>   differently from a collider's own unregistered layer: it ORs in
>   `LAYER_UNKNOWN` instead of contributing `0`, so it matches **nothing**,
>   not everything — resolving it to `0` would silently turn "filter out
>   everything except this one (mistyped or not-yet-registered) layer" into
>   its exact opposite.

### Spatial queries
`get_entity_at(x,y)` · `is_solid_at(x,y)` · `find_entities_in_rect(x,y,w,h)` · `get_distance(a,b)` · `get_angle_to(from,to)` (radians)

> **Static tiles are cells of a tilemap, not entities (Step 8-1,
> docs/ember2d-master-plan.md §5.7).** Every tile with no script, node
> graph, trigger, actor, `next_level`, collider mask, or camera follow — in
> the shipped demos, every wall and floor — is stored in the level's one
> `Tilemap`, which is itself a single entity. What that means for these
> functions:
>
> - `is_solid_at`, `get_path`, `reachable_within`: **unchanged** — a solid
>   cell blocks exactly as the wall entity it replaced did (same mask
>   filtering; an unregistered layer still blocks unmasked queries only).
> - `get_entity_at`, `find_entities_in_rect`, `raycast` (and a script's
>   `on_collide(me, other)`): a solid cell reports **the tilemap's entity
>   id**, not a per-wall id (a cell has none). `find_entities_in_rect` lists
>   the tilemap once however many of its cells the rect touches. Ties for
>   "first" still go to the lowest id. A non-solid cell (a floor) is
>   invisible to all three, as a floor tile always was (it never had a
>   collider).
> - So a script that recognised a wall by `has_tag(hit, "wall")` must test
>   `is_tilemap(hit)` instead (or both — see `demos/shooter/scripts/
>   bullet.rhai`). `find_by_tag("wall")`/`count_by_tag("wall")` no longer
>   see collapsed walls either; `get_tile_tag(x,y)` reads a cell's tag.
> - The tilemap entity has a position (`get_x`/`get_y` = its grid's
>   top-left cell) and `entity_exists` is true for it, but moving it does
>   **not** move the walls — cell geometry is fixed at level load.

`is_tilemap(id)` → `bool` (Step 8-1) — `true` for the tilemap entity every static-tile hit above reports, `false` for anything else (including an id that doesn't exist).

`get_tile_tag(x,y)` → `String` (Step 8-1) — the tag of the static tile covering (x, y) — the topmost layer's that has one — or `""` if none. Unlike the queries above it answers for non-solid cells too (floors). Accepts int or float coordinates. Interactive tiles are still entities: use `get_entity_at`/`get_tag` for those.

> **`get_angle_to` is deterministic; a script's own `.cos()`/`.sin()` on its
> result usually isn't.** As of Phase 6 Step 12 (docs/ember2d-phase6-plan.md,
> §5.2 H2) `get_angle_to` no longer calls the platform's `atan2` — it uses a
> rational (`+ - * /` only) approximation instead, accurate to within ~0.01
> radians (~0.6°), which produces the exact same bits on every platform. But
> Rhai's own `cos()`/`sin()` functions (what a script would naturally call on
> the angle this returns to turn it back into a direction) are Rhai's
> libm, not this engine's — calling them reintroduces the same
> cross-platform nondeterminism this fix exists to remove, just one step
> later. **If you need a direction vector, skip the angle entirely**: divide
> `(get_x(to)-get_x(from), get_y(to)-get_y(from))` by `get_distance(from,to)`
> — both IEEE-754-exact operations, so the whole computation stays
> deterministic end to end. See §4's chase example, rewritten this way.

`raycast(x1,y1,x2,y2,mask)` → `[id, hit_x, hit_y]` or `[]`. Finite segment, solids only, skips self. A hit on a static wall reports the tilemap's id (see the Step 8-1 note above); `hit_x`/`hit_y` are the exact point it entered that cell, computed the same way as for a wall entity.

`get_path(x1,y1,x2,y2,mask)` → `[[x,y],…]`. A\* on the integer grid, 4-directional, 2000-node cap. Empty array means no path or already there.

`get_path(x1,y1,x2,y2,mask,diagonal)` (Step 7.5-6, docs/ember2d-master-plan.md §5.6) — the same A\*, 8-directional when `diagonal` is `true`. A diagonal step costs more than a straight one (so the heuristic stays admissible) and is refused when it would cut between two solids that share only a corner. Registered under the same name as a 6-argument overload — the 5-argument form above is unchanged and still 4-directional.

`reachable_within(id,budget)` → `[[x,y],…]` (Step 7.5-6) — every cell reachable from `id`'s own current position within `budget` orthogonal steps (a plain BFS; no `diagonal`/`mask` option — every solid blocks, matching `is_solid_at`). Never includes `id`'s own starting cell. An unknown `id` or `budget <= 0` returns `[]`. The tactical-RPG movement-range preview the old refactor plan's open question 4 asked for.

### Tiles (Step 9.5-1)
`tile_def(name, #{…})` · `tilemap_resize(w,h)` · `tile_set(x,y,name)` · `tile_set_layer(x,y,layer,name)` · `tile_fill(x,y,w,h,name)` · `tile_fill_layer(x,y,w,h,layer,name)` · `tile_clear(x,y)` · `tile_clear_layer(x,y,layer)` · `tile_clear_rect(x,y,w,h)` · `tile_clear_rect_layer(x,y,w,h,layer)` · `get_tile(x,y,layer)` → `String`

Scripts can place static tiles: cells of the level's tilemap (Step 8-1),
not entities, so a generated 80×45 floor costs what a painted one does.

- **Declare a tile once.** `tile_def("wall", #{ glyph: "#", fg: "Grey",
  bg: "Reset", solid: true, tag: "wall", layer: 1 })` names it. Every key
  is optional; the defaults are a space, white on nothing, walkable,
  layer 0.
  - `sprite: "tileset:region"` draws a project tileset region instead of
    the glyph. A missing region warns once and falls back to the glyph.
  - `collider_layer` is the tile's collision layer, as a painted tile's.
  - A key only an entity can have (`trigger`, `clip`, `script`, `exit`,
    `actor`) is refused with a warning: spawn an entity for those. So is
    an unknown key, to catch typos.
  - Declaring a name again changes the tiles placed after that; cells
    already placed keep their look.
  - Declarations are part of `World`, so saves keep them.
- **Place.** `tile_set`/`tile_fill` put the tile on its own layer;
  `_layer` puts it on another.
- **Clear.** `tile_clear`/`tile_clear_rect` empty every layer of a cell;
  `_layer` empties one.
- **Read.** `get_tile` returns the name a cell's tile was declared with,
  or `""` for an empty cell or an editor-painted tile (read those with
  `get_tile_tag`).
- **Which map.** The level's own tilemap. Before the first edit it grows to
  cover the whole level, since a painted level's map spans only its
  painted box. A level with none gets an empty one the level's size.
- **Resize.** `tilemap_resize(w,h)` replaces the map with an empty `w`×`h`
  grid at (0, 0), up to 2048×2048 cells.
- **Out of range.** A rect is clipped to the map. A request entirely
  outside it, or naming a tile no `tile_def` declared, does nothing and
  warns once per pass.
- **When changes show.** Ops apply in call order, after the pass. This
  step's collision already sees them. The script's own reads
  (`is_solid_at`, `get_tile`, `get_tile_tag`, `get_path`) see them from the
  next step, like every deferred write.

Coordinates and layers accept ints or floats (floats floor to a cell); the single-cell calls also take float coordinates with an int layer (`get_tile(ctx.get_x(id), ctx.get_y(id), 0)`). See
`ember2d/tests/tile_script.rs`.

### Field of view (Step 9.5-2)
`compute_fov(x,y,radius)` · `is_in_fov(x,y)` → `bool` · `is_explored(x,y)` → `bool` · `fov_reset()` · `set_fov_visibility(id,mode)`

Fog of war, opt-in.

- **Turning it on.** Nothing changes until a script calls
  `compute_fov(x, y, radius)`. That makes the view: every cell visible
  from (x, y) within `radius` (a disc), solid tilemap cells blocking
  sight. Call it again whenever the viewer moves. The radius is capped at
  256.
- **The algorithm.** Symmetric shadowcasting in exact integer slopes: if A
  sees B, B sees A, and a wall's whole face is visible from the room it
  bounds. Entities don't block sight.
- **What play mode draws:**
  - a cell never seen isn't drawn;
  - a cell seen before but out of view is drawn dimmed;
  - an entity out of view follows its `set_fov_visibility` mode:
    `"hide"` isn't drawn, `"remember"` is drawn dimmed once its cell has
    been seen, `"always"` is drawn normally.

  The default mode is `"always"` for the player, `"hide"` for an actor and
  `"remember"` for anything else (items, stairs). A bad mode name warns.
- **Reading.** `is_in_fov`/`is_explored` read the view as of the start of
  the pass. While fog is off (before the first `compute_fov`, or after
  `fov_reset()`) they answer `true` everywhere, so a monster's "can I see
  the player?" works the same in a level without fog.
- **Ordering.** `compute_fov` is applied after the same pass's tile
  requests, so carving a floor and computing the view in one `on_start`
  sees the new floor. Like every write it's deferred: compute from the
  position you're moving *to*, since `get_x` still reads the old one
  this pass.
- **Grid and saves.** The view covers the level's tilemap grid. A
  `tilemap_resize` (a new floor) starts a fresh one; a new level starts
  with none. The explored map is part of a save.

`compute_fov` also takes float coordinates with an int radius
(`compute_fov(ctx.get_x(id), ctx.get_y(id), 8)`). See
`ember2d/tests/fov_script.rs`.

### Hierarchy
`get_parent(id)` · `set_parent(id,parent)` · `set_parent_keep_world(id,parent)` · `get_world_x(id)` · `get_world_y(id)`

Pass `-1` as parent to detach. `set_parent` rejects a reparent that would create a cycle as a no-op (Step 7.5-9, docs/ember2d-master-plan.md §5.6) — checked up front, not discovered later; `get_global_position`'s own depth-100 walk still exists underneath as a safety net for a cycle that bypasses `set_parent` entirely (hand-edited save/level data), which is now the only way one can still occur. `despawn(id)` also clears every child's own `parent` link (rather than leaving it pointing at a dead id), preserving each child's current world position so it doesn't visually jump.

### Input
`is_held(key)` · `just_pressed(key)` — lowercase names: every letter `"a"`–`"z"` (R108: only twelve of them worked before Step 9-3), digits `"0"`–`"9"`, `"up"`/`"down"`/`"left"`/`"right"`, `"space"`, `"enter"`, `"escape"`, `"tab"`, `"backspace"`, `"shift"`, `"ctrl"`, `"f1"`–`"f12"`. While an engine menu or dialogue box is open, the keys it uses (arrows, W/S, Enter, Space, Escape) are taken out of what scripts see — see "Menus and dialogue".

**Semantics (from Phase 1 onward): buffered until consumed.** `just_pressed` is true in exactly **one** simulation step per physical press — no matter how many steps run in a frame — and a press is never lost to a frame that ran zero steps. A press held for less than one frame still registers. `is_held` is continuous state and is unbuffered.

The buffer window is ~100–150ms, which also gives you input forgiveness for free: a jump pressed slightly before landing still fires. In turn-based mode the buffer is what lets a keypress wait for the actor's turn to come around instead of being dropped.

> Before Phase 1 the behaviour is broken in both directions: a press can fire on several sub-steps in one frame, or be dropped entirely on a light frame (defect D1).

> **Not replay-safe outside `on_input` (Step 5e, docs/ember2d-phase5-plan.md).**
> `is_held`/`just_pressed` read real engine-side key state, not a recorded
> command stream — a replay of the same commands won't reproduce the same
> raw key state on every machine/run. `on_input` is the one place a script
> should read either function; everywhere else (`on_update`, `on_collide`),
> read `command_action()`/`command_param()` instead. Both functions stay
> registered and still work anywhere for compatibility, but only `on_input`
> gets this guarantee.

**The command boundary.** `on_input` runs once per step and is called only
for locally-controlled actors — an entity with an `Actor` component whose
`controller` is `Local` (Step 5f, docs/ember2d-phase5-plan.md; before that
component existed, in Step 5e, this just meant the player). Inside it,
translate raw input into an action:
```rhai
fn on_input(id, ctx) {
    if ctx.just_pressed("w") { ctx.submit(id, "move", [0.0, -1.0]); }
}
```
- `submit(actor_id, action, params)` — queues a `Command` for `actor_id`.
  `action` is a name your own scripts choose and interpret; the engine
  never looks inside it. Meaningful only inside `on_input`.
- `submit(actor_id, action, params, cost)` (Step 7.5-7, docs/ember2d-
  master-plan.md §5.6) — same as above, plus a turn cost the command
  itself carries. Only ever read back by `TurnModel::ActionCost` (see
  below); every other model ignores it, so calling this instead of the
  3-argument form is harmless under `Alternating`/`Energy`.
- `command_action()` → the calling entity's command's action this step, or
  `""` if none was submitted.
- `command_param(i)` → the `i`-th param (`f64`), or `0.0` if there's no
  command or the index is out of range.

**`on_turn` reads the command back out and does the actual game-state
mutation** — move/attack/quaff, whatever your actions mean:
```rhai
fn on_turn(id, ctx) {
    if ctx.command_action() == "move" {
        let dx = ctx.command_param(0);
        let dy = ctx.command_param(1);
        ctx.set_position(id, ctx.get_x(id) + dx, ctx.get_y(id) + dy);
        ctx.act(100.0); // this turn cost 100 energy — see below
    }
}
```
Under `TurnScheduler` (Step 5f), `on_turn` runs exactly once, only for
whichever single actor is currently due — an AI actor's turn always, a
`Local` actor's only once `on_input` has queued something for it. Turn
scheduling functions:
- `act(cost)` — marks this `on_turn` call as having consumed a turn, at
  `cost` energy. **Always wins over whatever the project's `TurnModel`
  would otherwise pick** (see below) — this is the one universal override,
  in every model. Replaces the removed `ctx.trigger_turn()`. For a `Local`
  actor, **not** calling this is how a rejected action (a wall bump, an
  empty-handed quaff) costs nothing — the same actor is asked again next
  step instead of the turn advancing. An AI actor's turn always counts
  whether or not it calls this (a sleeping monster still "used" its turn
  doing nothing — unconditionally skipping the advance for AI would wedge
  the scheduler on it forever).
- `get_turn_number()` → how many turns the local player has completed so
  far this level. Engine-tracked (not a script global), so unlike the old
  "turn" global this has no same-pass deferred-write lag to guard against.
- `get_speed(id)` / `set_speed(id, n)` — an actor's `Actor::speed`. Live
  under `TurnModel::Energy` (Step 7.5-7, docs/ember2d-master-plan.md §5.6)
  — see `TurnModel` below — and a real, honestly-functioning read/write
  regardless of model, so switching a project's `TurnModel` needs no
  scripting-API change on either side.
- `make_actor(id, speed)` (Step 9.5-3) — makes `id` an AI actor that takes
  turns: its script's `on_turn` runs whenever the scheduler reaches it,
  starting at once. Before this only a level tile could be an actor, so a
  script-generated level couldn't have monsters that act. Deferred; does
  nothing to an entity that's already an actor (use `set_speed`) or doesn't
  exist. Spawn, `set_script`, then `make_actor`: the first turn may come
  before the script attaches (`set_script` lands a step later) and does
  nothing, which is harmless.
- **`ai_turns_per_step`** (project setting, `project.ron` / File > Project
  Settings, Step 9.5-3) — how many turns one simulation step may resolve
  while the next actor due is AI. The scheduler hands out one turn at a
  time, and by default (1, every project before this step) one per step,
  so a level with forty monsters answers each player move one monster per
  frame, about two-thirds of a second. A higher number (the roguelike uses
  256; a new turn-based project starts there) resolves them all in the
  player's own step; it always stops when the player is due, the level
  changes or a scene pauses. Each extra turn gets a fresh snapshot, so
  every monster sees the moves the ones before it made.

**`TurnModel` (Step 7.5-7): what `act`'s cost defaults to when a script
doesn't call it at all.** A project selects one via `ProjectData::turn_model`
(`project.ron`); it never needs a scripting-API call of its own — a script
only ever sees its effect through `act`'s own fallback and, for
`ActionCost`, through `submit`'s 4-argument overload above.
- `Alternating` (the default, and the only model that ever shipped before
  this step) — every turn costs `ALTERNATING_COST` (100) regardless of
  `Actor::speed` or the command acted on. Every pre-7.5-7 project keeps
  this exact behavior unless it opts into one of the other two.
- `Energy` — cost scales inversely with `Actor::speed`:
  `ALTERNATING_COST * 100 / speed`. A speed-200 actor's turn costs half a
  speed-100 actor's, so it comes back up to act again twice as often — the
  classic "the faster creature acts more often" order a Pokemon-style
  battle needs. No script-side bookkeeping required: an actor that never
  calls `act` at all (every `demos/roguelike/scripts/enemy.rhai` enemy
  today) already gets this for free the moment a project selects `Energy`.
- `ActionCost` — cost comes from the currently-resolving actor's own
  `Command.cost`, set via `submit`'s 4-argument overload, if it queued one
  this round; `ALTERNATING_COST` otherwise. Lets the ACTION an actor
  chooses (a heavy attack costing more than a quick jab) drive turn order
  instead of a fixed per-actor speed.
- `get_stat(id, key)` (Step 7.5-4, docs/ember2d-master-plan.md §5.6) — a
  numeric value authored on this actor's tile via `TileRecord.actor.stats`
  (a `BTreeMap<String, f64>`, e.g. `"hp"`/`"atk"`/`"awareness_range"`). `0.0`
  for a missing key or a non-actor entity, same neutral-default convention
  every other `get_*` uses (R32, 7.5-1) — there's no separate "does this key
  exist" query, same as `get_global`/`get_var`. What lets `enemy.rhai` be
  one shared script for both the roguelike's rat and boss instead of one
  script per role.
- `get_tint_aware(id)` / `get_tint_asleep(id)` (Step 7.5-4) — the color name
  string this actor's sprite should wear once aware of the player, and
  while it's still asleep, authored via `TileRecord.actor.tint_aware`/
  `tint_asleep`. Kept as their own fields, not `stats` entries, since
  `stats` is numeric-only and a color isn't. `"Reset"` for a non-actor
  entity.

This whole boundary is what makes a recorded/transmitted command stream
(the eventual replay test, Step 5h; lockstep netcode, Phase 9b) fully
determine what happens next without real key events at replay time. See
`demos/roguelike/scripts/player.rhai` for the full pattern, including how it
handles an action `on_turn` doesn't recognize for the player's current
state (falls through to "no turn consumed", same as no command at all).

### Turn animation

`animate_move(id,to_x,to_y,duration)` · `animate_flash(id,color,duration)` ·
`animate_shake(id,duration)` · `is_animating(id)` — Phase 5.5 Part 3
(docs/ember2d-phase5.5-plan.md).

Grid/game state has already resolved by the time you call any of these —
call `set_position`/whatever the real consequence is exactly as before,
then queue one of these to say what to *show* while real time passes.
`duration` is **real seconds**, unrelated to a turn's energy cost — a
100-cost turn might animate for 0.1s or 1.0s with identical game
consequences, which is what would let a future "fast-forward animations"
setting exist without touching balance.

**An actor's own turn will not resolve again until its own previously
queued animation has finished playing.** This is the actual point, not a
side effect: skip it and an animation is purely decorative while the sim
races ahead underneath it for that same entity. **Corrected in defect D20
(docs/ember2d-refactor-plan.md §3)** — this used to gate the WHOLE
scheduler on the WHOLE animation queue (no actor's turn could resolve while
ANY entity's animation was still draining, even an unrelated one), which
meant several actors acting in one round each paid their own animation's
duration serially. The gate is per-actor now: a different actor's turn
resolves immediately regardless of what's still playing, so their
animations overlap in real time instead of stacking.
`demos/roguelike/scripts/enemy.rhai` calls `animate_move`
right alongside its own `set_position`; the player's own movement is
deliberately left un-animated, both to avoid adding input latency to
something that already felt instant, and because it means the player is
*never* gated by this at all — only an actor that animates itself waits on
its own animation.

`is_animating(id)` tells the truth now (Step 7.5-7, docs/ember2d-master-
plan.md §5.6): whether `id` has an in-flight `PlayingAnimation` in
`ember2d::play::PlayState`'s own queue this real step. It used to always
return `false` — not because nothing scripted could run while an animation
was in flight (D20 already made the gate per-actor, so that was never the
real reason even before this step), but because the function is registered
in `ember2d-sim`, which by design has no visibility into
`PlayState.animations` — presentation state, owned entirely by the
`ember2d` crate. `PlayState::update` now hands a snapshot of which entity
ids are currently animating in through `StepInput::animating` each real
step, the same way it already threads in `external_commands`; `Simulation`
carries it into `ScriptState` for whichever passes actually see a
`StepInput` (`on_input`/`on_update`/`on_turn` — not `on_start`/`on_load`,
which run before any animation could exist, and not `on_collide`, which
runs from `late_step`, no `StepInput` of its own). A script can therefore
ask about ANY entity, not just itself — an enemy whose `animate_move`
outlives its own turn is a `true` for everyone else's `is_animating` call
on it in the meantime, even though the scheduler-front gate above only
ever waited on that one entity's own next turn.

In realtime mode these still work the same way, but are usually
unnecessary — movement there is typically already continuous via velocity,
so there's rarely a "resolved instantly, now show it" gap to bridge.

Gamepad: `gp_is_held(pad,btn)` · `gp_just_pressed(pad,btn)` · `gp_axis(pad,axis)`

Mouse: `get_mouse_x()` · `get_mouse_y()` (cells) · `get_mouse_world_x()` · `get_mouse_world_y()` · `mouse_left_pressed()` · `mouse_right_pressed()` · `mouse_left_held()` · `mouse_right_held()`

> `get_mouse_world_y` no longer subtracts a HUD row (Phase 4, Step 4g) — the
> world now gets the full viewport. An earlier version of this doc claimed
> Phase 2 already removed the leak; it hadn't — Phase 2 only centralized the
> old bare `+1`/`-1` literal into one constant, `HUD_TOP_ROWS`, without
> zeroing it. That constant was itself deleted in Phase 5 Step 5a
> (docs/ember2d-phase5-plan.md) once both its call sites (`Camera::viewport_origin`
> and this function) had been inert since Step 4g — a purely internal
> cleanup, not a further behavior change here. **Not replay-safe** — see
> "Camera" below; `get_mouse_world_x/y` add the mouse's screen position to
> the camera's, so they inherit the same nondeterminism.

### Camera
`get_camera_x()` · `get_camera_y()` · `set_camera(x,y)` · `shake_camera(intensity,duration)` · `set_camera_target(id)` · `set_camera_target(x,y)` · `clear_camera_target()` · `set_camera_zoom(z)` · `get_camera_zoom()` → float · `set_camera_bounds(x,y,w,h)` · `clear_camera_bounds()` · `set_camera_speed(s)`

Step 9-2 (docs/ember2d-master-plan.md §5.8) made the camera scriptable,
which is what cutscenes need:

- `set_camera_target(id)` follows an entity (back to the default follow if
  it's despawned); `set_camera_target(x, y)` centres on a world point;
  `clear_camera_target()` returns to following the level's camera-follow
  actor. `set_camera(x, y)` is the same as `set_camera_target(x, y)` — it
  used to pin the camera with no way back.
- `set_camera_zoom(z)`: screen cells per world unit, clamped to 0.25–8
  (glyphs and sprites scale with it). `get_camera_zoom()` reads the last
  zoom a script set, as of the start of the pass.
- `set_camera_bounds(x, y, w, h)` keeps the view inside that world rect
  instead of the level's; `clear_camera_bounds()` goes back to the level.
  When the bounds are smaller than the view, the view's top-left edge sits
  on the bounds' top-left corner (as a small level always has).
- `set_camera_speed(s)` sets how quickly the camera catches up with its
  target, per second (default 5); `0` jumps straight there.
- `get_mouse_world_x/y` divide the screen cell by the zoom (and, since
  Step 9-5, by the project's world cell — see below); `get_mouse_x/y` stay
  screen cells.
- **World cell size** (Step 9-5): a project's `project.ron` can set
  `world_cell: (w, h)` in pixels — `(16, 16)` for square pixel art. One
  world unit (one level cell) is then that big on screen at zoom 1, so
  square sprites draw square. The default is the glyph cell, `(8, 16)`.
  Only the world stretches: `draw_hud`, menus, dialogue, `get_mouse_x/y`
  and `get_viewport_width/height` stay on the 8×16 glyph grid, so a
  16×16 project's HUD still has 80 columns while the camera shows 40
  world columns.
- None of this is saved with the game — a script that wants its camera
  back after a load sets it again in `on_load`. NaN and other nonsense
  values are ignored (a negative bounds size, a negative entity id).

> **Not replay-safe** (Step 5d, docs/ember2d-phase5-plan.md). Camera
> position is presentation, not simulation: `PlayState`'s follow-lerp runs
> on real wall-clock time (`UpdateContext::frame_delta_time`, not the fixed
> `delta_time` scripts otherwise see) so it stays visually smooth regardless
> of the sim's own clock, and that lerp uses `exp()` — a named
> cross-platform determinism hazard (§5.2 H2, docs/ember2d-refactor-plan.md)
> since transcendental math isn't guaranteed bit-identical across platform
> libm implementations. `get_camera_x/y` therefore return a value that can
> differ between two runs (or two machines) fed identical input — as can
> `get_mouse_world_x/y` below, which are derived from it. **Don't branch
> game logic on either.** Nothing in the roguelike does today. Revisit once
> Phase 6 decides §5.2 H2 (restrict sim math to the reproducible set, ship
> lookup-table trig, or move to fixed-point).

### State
Globals (per level): `set_global` · `get_global` · `has_global` · `remove_global` · `add_global(key, delta)` → new value
Persistent (across levels): `set_persistent` · `get_persistent` · `has_persistent` · `clear_persistent` · `clear_all_persistent` · `add_persistent(key, delta)` → new value
Per-entity (`Vars`, cleared on despawn): `set_var(id,key,value)` · `get_var(id,key)` · `has_var(id,key)` · `remove_var(id,key)` · `add_var(id,key,delta)` → new value

> **Defect D2 (fixed in Phase 1):** `set_persistent` inside `on_start` used
> to be silently discarded — `PlayState::on_start` ran `on_start` scripts
> against a fresh, throwaway `HashMap` instead of the engine's real
> persistent store. Fixed by threading the real store through
> `GameState::on_start`'s signature instead; see `tests/persistent_on_start.rs`
> for the regression test.

> **`add_global`/`add_persistent` (Step 7.5-2, docs/ember2d-master-plan.md
> §5.6):** read the CURRENT value — this pass's own already-queued write if
> there is one, falling back to the resolved store, falling back to `0` if
> the key has never been set — add `delta`, queue the result, and return
> the new value. This is what makes calling either more than once for the
> same key in the SAME script pass safe: each call sees the previous call's
> write, so N calls this pass land as N additions. A hand-rolled
> `set_global(k, get_global(k) + d)` can't do that — `get_global` never
> observes a `set_global` from earlier in the same pass — which is why
> every script that needed to add to a running total used to either tally
> duplicate writes itself before one `set_*` call, or guard every read
> against an uninitialized key with a local `or_zero()`-style helper.
> `delta` accepts both an int and a float literal (same uniform-typing
> convention Step 7.5-1 gave every coordinate/size argument); the return
> value and the stored value are always a float, even when `delta` was an
> int — a script displaying the result should cast with `.to_int()` if it
> wants to avoid Rhai's string concatenation rendering a whole-number float
> with a trailing `.0`.

> **`set_var`/`get_var`/`has_var`/`remove_var`/`add_var` (Step 7.5-3,
> docs/ember2d-master-plan.md §5.6):** real per-entity state, backed by a
> `Vars` component (`components/vars.rs`) rather than a level-scoped
> global. `World::despawn` clears an entity's entire `Vars` automatically —
> no `remove_var` bookkeeping needed on death, unlike the `"hp_" + id`/
> `"ehp_" + id` global-key-concatenation convention this replaces (still
> readable in git history; every shipped script has migrated off it).
> Same same-pass-write-invisible rule as `get_global`/`set_global` (see
> `docs/ember2d-scripting-api.md` §2's note above), and `add_var` gives it
> the same atomic-accumulate guarantee `add_global`/`add_persistent` have.

### Save data (Step 9.5-5)
`save_data(path, value)` · `load_data(path)` → value or `()`

Keeps one script value (a number, a string, a map) in its own small RON
file, for what has to outlive a run: a best score, an option.
`save_game` stores a whole game, which is the wrong tool for that.

- **Paths** work as `save_game`'s do: as given, relative to the working
  directory. The demos use `.sav` names, which the repository ignores.
- **`save_data`** writes after the pass, alongside `save_game`.
- **`load_data`** reads at once, through the simulation's level source. A
  missing or unreadable file reads as `()`: test it with
  `type_of(v) == "()"`.
- **Not replay-safe:** a loaded value is whatever is on this machine's
  disk. Keep it for things a replay needn't reproduce.

See `demos/shooter/scripts/player.rhai`.

### Timers
`start_timer(name,seconds)` · `timer_done(name)` · `cancel_timer(name)`

Per-entity, so names never collide. Backed by a real per-entity store on
`ScriptEngine` as of Phase 6 Step 9 (docs/ember2d-phase6-plan.md) — plain
engine-owned state now, not smuggled through each entity's Rhai `Scope` as
`__timer_<name>` variables scanned out by string prefix every script pass.

> **Not part of any save.** Timers are lost across `save_game`/`load_game` —
> true before Step 9 and unchanged by it, just now documented rather than an
> accident of where the state happened to live. If a script's timer-driven
> behavior matters across a save/load, track the deadline in
> `set_persistent`/`get_persistent` yourself (e.g. `ctx.get_elapsed()` plus a
> duration) instead of relying on `start_timer`.

> **`timer_done` really is "true once, then consumes itself"** (Step 7.5-8,
> docs/ember2d-master-plan.md §5.6, D22 fix). Storage is a real
> `TimerState { Running(f32), Fired, Cancelled, Consumed }` enum now, not a
> single float doing quadruple duty by sign and magnitude — the old bug was
> that a cancelled timer and a just-fired-and-consumed one both collapsed to
> the same stored sentinel (`-1.0`), which was itself still inside the range
> `timer_done`'s own guard treated as "done," so the check after either
> event reported `true` again and kept doing so until enough real steps
> decayed it out of range (roughly 8 minutes at 60 steps/second). Each state
> now means exactly one thing: `timer_done` reads `true` only for `Fired`,
> transitions it to `Consumed` in the same call, and `cancel_timer` moves a
> timer straight to `Cancelled` — a stable dead end `timer_done` always
> reads as `false`, distinct from `Consumed`.

### Randomness
`random_int(min,max)` inclusive · `random_float()` · `random_bool(chance)` · `random_choice(array)`

> **Defect D3, fixed in Phase 1** (docs/ember2d-refactor-plan.md §3): used to
> seed from system entropy on every run, so nothing was reproducible. Now
> seeded once from the level's own stored `seed` field — the same seed
> reused for the level's whole lifetime, so a replay with identical inputs
> reproduces identical `random_*` results. **7A-1** (docs/ember2d-master-plan.md
> §5.1) additionally guards `random_int`'s argument order (a reversed
> `min`/`max` used to panic) and `random_bool`'s chance against non-finite
> input (a `NaN` used to panic).

`set_random_seed(seed)` (Step 9.5-3) restarts the stream from `seed`, at
once. It isn't a deferred write: every later `random_*` call, this pass and
after, follows from it. A generated level can then be a function of its own
inputs. The roguelike seeds each floor from the run's seed and the depth,
so floor 10 of a run is the same floor 10 however the run got there.

### HUD
`draw_hud(x,y,text,fg,bg)` · `draw_box(x,y,w,h,fg,bg)` · `fill_rect(x,y,w,h,ch,fg,bg)` · `draw_panel(x,y,w,h,title,fg,bg)` · `draw_menu(x,y,w,options,selected,fg,bg,sel_fg,sel_bg)` · `clear_hud()`

Screen space, in cells. Cleared each frame.

### Menus and dialogue
`menu_open(items)` → id · `menu_open(items, opts)` → id · `menu_selection(id)` → int · `menu_closed(id)` → bool · `menu_close(id)` · `draw_dialogue(text, speaker)` → id · `dialogue_advance()` · `dialogue_open()` → bool · `dialogue_done(id)` → bool · `close_dialogue()` · `wrap_text(text, width)` → array of lines

Step 9-3 (docs/ember2d-master-plan.md §5.8). Unlike `draw_menu` (which
only draws a list), these are **engine-owned widgets**: open one once, and
the engine handles the keyboard and draws it — in the bundled Cascadia
Mono font on the pixel path, above every HUD — until it closes.

- `menu_open(items, opts)`: `opts` may hold `title`, `x`/`y` (cells;
  default centred, always kept on screen), `width` (cells; default fits the
  longest line), `cancelable` (default `true`), `selected` (starting row).
  Up/Down or W/S move the highlight (wrapping), Enter or Space confirms,
  Escape cancels when allowed. Only the newest open menu takes the keys.
- `menu_selection(id)`: the highlighted row while open, the chosen row once
  confirmed, `-1` once cancelled (or for an unknown id). `menu_closed(id)`
  turns true when it's confirmed or cancelled. A closed menu stays
  readable until `menu_close(id)` forgets it (the 32 newest are kept).
- `draw_dialogue(text, speaker)` shows `text` in a box along the bottom,
  with `speaker` (may be `""`) above it. The text is word-wrapped to the
  screen and shown three lines per page; Enter or Space turns the page and
  closes the box after the last. Call it **once** when the conversation
  starts — calling it again with the same text and speaker while it's
  showing does nothing (returns the same id), but after it closes the
  same call opens it again. `dialogue_done(id)` is true once that dialogue
  was read through, closed or replaced; `dialogue_advance()` turns the page
  from a script; `close_dialogue()` closes it.
- While a dialogue box is open it has the keyboard, then the newest menu.
  The keys a widget uses are removed from what every script sees that
  step, so a player doesn't walk while a menu is open.
- Widgets opened by a scene's script close when that scene is popped.
  Open widgets are saved with the game.
- `wrap_text(text, width)` is the same word-wrap the dialogue box uses:
  breaks at spaces, honours newlines, and splits a word longer than a line.

### Effects and audio
`emit_particles(x,y,glyph,fg)` · `play_sound(path)` · `play_sound_at(path,x,y)` (volume falls off to 20 units, stereo pans left/right with the camera's horizontal offset) · `play_music(path)` (a no-op if `path` is already the current track — see the note below) · `stop_music()`

> **Step 7.5-11** (docs/ember2d-master-plan.md §5.6, R30): the audio device
> stream now survives level transitions (it used to live on the per-level
> play state, so every transition reopened it and killed any playing
> music). `play_music` calling itself a no-op when `path` matches the
> already-playing track is what that survival actually buys a script: an
> `on_start` that unconditionally calls `ctx.play_music("theme.ogg")` on
> every level — the natural way to author "make sure this track is
> playing" — now keeps the SAME track running seamlessly across a
> transition into another level that wants the same music, instead of
> restarting it from the beginning every time. Call `stop_music()` first if
> a script genuinely needs to restart the current track from the top.
> Sound/music files are also decoded once and cached by path now, not
> re-read from disk on every `play_sound`/`play_music` call.

### Scenes
`push_scene(name)` · `push_scene(name, opts)` · `pop_scene()` · `current_scene()` → string · `scene_count()` → int · `scene_data()` → any · `quit_game()` · `return_to_editor()` · `is_editor_preview()` → bool

Step 9-1 (docs/ember2d-master-plan.md §5.8). A **scene** is a named state
layered over the level — a pause menu, a battle, a dialogue — with its own
script and its own hidden entity (the `id` its functions receive, so
`set_var`/`start_timer` work on it like on any entity).

- `push_scene(name)` runs `scenes/<name>.rhai` (found like an exit path:
  beside the level first, then from the working directory). `opts` is a map:
  `script` (a different path), `pauses_world` (default `true`), and `data`
  (anything; the scene reads it back with `scene_data()`). Like every write,
  the push lands at the end of the pass; the scene's `on_start(id, ctx)`
  runs on the next step. An unknown scene logs a warning and pushes nothing.
- While a scene with `pauses_world: true` is on the stack, the level's
  `on_input`/`on_update`/`on_turn`/`on_collide`, physics, collisions and
  clip animators all stop. The level's last HUD stays on screen; the
  scene's HUD draws above it.
- Each step: a newly pushed scene gets `on_start`; the **top** scene gets
  `on_input(id, ctx)` — but not on the step it was pushed, so the key that
  opened a scene never also acts inside it; then `on_update(id, ctx)` runs
  for every scene from the topmost pausing one upward. A scene with
  `pauses_world: false` (a HUD overlay, say) runs alongside the level.
- `pop_scene()` removes the top scene and its entity (there is no
  `on_exit`; clean up before popping). `current_scene()` is the top
  scene's name, `""` when only the level runs.
- `quit_game()` closes the game; `return_to_editor()` goes back to the
  editor, and only works in an editor preview (F5) — check
  `is_editor_preview()`.
- **Timing:** a pushed scene gets `on_start` the next step, and its first
  `on_update` the step after that — so `on_update` always sees what
  `on_start` set (R110). A scene whose script fails is closed, with an
  error in the log, rather than left on top of a frozen game (R111); its
  HUD goes with it (R112).
- **The Esc pause menu is a scene.** Esc with no scene open pushes
  `"pause"`: the project's own `scenes/pause.rhai` if it has one, otherwise
  the engine's built-in menu (Resume / Back to Editor / Quit Game). A save
  made with a scene open restores it on load.

### Flow
`load_level(path)` · `load_level(path, spawn)` · `save_game(path)` · `load_game(path)` · `log(msg)` · `get_delta()` · `get_elapsed()` · `get_spawn_point(name)` → `[x,y]` or `[]` · `get_viewport_width()` · `get_viewport_height()` · `api_version()` → int, this API's breaking-change generation (see §6)

**Spawn points and level transitions** (Step 9-4). A level's spawn points
are one set of names (format v7's `spawns`): `"player"` is where the
player starts when the level is entered normally, and every point placed
with Shift+P in the editor is another name. `get_spawn_point(name)` reads
any of them, `"player"` included.

- `load_level(path, spawn)` enters the next level at its spawn point
  `spawn` instead of `"player"` — walking out of the inn puts the player
  at the inn's door on the town map.
- An exit tile's target can do the same: `town.level#inn_door`.
- A name the level doesn't have logs a warning and uses `"player"`.

Structured state needs no new type: `set_persistent` (and
`set_global`) accept nested maps and arrays — a party roster as an array
of maps, an inventory as a map of counts — and they survive save files and
level changes intact.

> `trigger_turn()` was removed in Step 5f (docs/ember2d-phase5-plan.md) —
> see "The command boundary" above for `ctx.act`, its replacement.

---

## 4. Examples

```rhai
// Chase the player, but only when there's line of sight.
fn on_update(id, ctx) {
    let player = ctx.find_by_tag("player");
    if !ctx.entity_exists(player) { return; }

    let hit = ctx.raycast(ctx.get_x(id), ctx.get_y(id),
                          ctx.get_x(player), ctx.get_y(player), []);

    if hit.is_empty() {
        // A direction vector via get_distance, not an angle via
        // get_angle_to + .cos()/.sin() — Rhai's own cos()/sin() call into
        // the HOST's libm and are not covered by the engine's determinism
        // guarantee, even though get_angle_to itself now is (see the
        // "Spatial queries" note above). Dividing by get_distance (sqrt
        // only, IEEE-754-exact) gets the same normalized direction and
        // stays deterministic end to end.
        let dist = ctx.get_distance(id, player);
        let dx = (ctx.get_x(player) - ctx.get_x(id)) / dist;
        let dy = (ctx.get_y(player) - ctx.get_y(id)) / dist;
        ctx.set_velocity(id, dx * 4.0, dy * 4.0);
        ctx.set_tint(id, "Red", "Reset");
    } else {
        let path = ctx.get_path(ctx.get_x(id), ctx.get_y(id),
                                ctx.get_x(player), ctx.get_y(player), ["solid"]);
        if path.len() > 0 {
            let step = path[0];
            ctx.set_velocity(id, (step[0] - ctx.get_x(id)) * 4.0,
                                 (step[1] - ctx.get_y(id)) * 4.0);
        }
        ctx.set_tint(id, "Yellow", "Reset");
    }
}
```

```rhai
// Fire on a cooldown.
fn on_update(id, ctx) {
    if ctx.is_held("space") && ctx.timer_done("cooldown") {
        let b = ctx.spawn_entity("*", ctx.get_x(id), ctx.get_y(id) - 1.0, "bullet");
        ctx.set_velocity(b, 0.0, -12.0);
        ctx.start_timer("cooldown", 0.25);
        ctx.play_sound_at("assets/shoot.ogg", ctx.get_x(id), ctx.get_y(id));
    }
}
```

---

## 5. Conventions

**Naming.** `get_*` reads, `set_*` writes (deferred), `is_*`/`has_*` return bool, `find_*` returns an id or -1, array returns are `[]` on failure.

**Failure is quiet.** Setters on missing entities do nothing; getters return zero values. A script error disables that script rather than killing the game — **defect D9, fixed in Phase 1** (docs/ember2d-refactor-plan.md §3): a runtime error used to only suppress its own repeated log message while the script kept being called (and kept failing) every frame; now the script stops being invoked at all until it hot-reloads successfully.

**Sentinels.** `-1` means "no entity". Never `0` — that's the reserved null id.

**Numeric parameters.** Every coordinate, size, or layer-order argument
accepts either an int or a float literal (Phase 7.5, §6) — write a call's
numeric literals in one consistent style, though; mixing (`draw_hud(1,
2.0, ...)`) isn't guaranteed to resolve.

**Limits.** An expression may nest 64 deep (32 inside a function) and
calls may go 64 levels deep, the same in debug and release builds (R116,
Step 9-8: a debug build used to keep Rhai's lower debug defaults, 32/16/8,
so the RPG demo's battle script compiled in a release build and failed
"too complex" in a debug one).

**Rhai reminders.** A function can't see the script's top-level `let`s
and `const`s; write a tuning value as a tiny function instead
(`fn step_time() { 0.14 }`). A few ordinary-looking words are reserved by
Rhai (`go` is one) and can't name a function.

---

## 6. What the refactor changes

| Phase | Change | Breaking? |
|---|---|---|
| 1 | `just_pressed` becomes buffered-until-consumed: exactly once per press, never dropped | Behaviour only — fixes both duplicates and drops |
| 1 | RNG becomes deterministic and world-seeded | No (behaviour only) |
| 1 | `set_persistent` works in `on_start` | No (fixes a silent failure) |
| 1 | `spawn_entity` gains colour, layer, collider parameters | Additive |
| 2 | Camera gains zoom (`Camera.zoom` — no scripted control yet) | No |
| 3 | `set_color` → `set_tint`; colour names → explicit values | Yes |
| 3 | `set_z_order` → `set_layer_order` | Yes |
| 3 | `set_animation(chars)` → clip references by name; sheet clips added | Yes |
| 3 | `set_texture(path)` → texture handles | Yes |
| 4 | Player movement, score, HUD move from `play.rs` into scripts | Additive |
| 4 | Mouse world coords lose the HUD-row fudge (`HUD_TOP_ROWS` → 0, Step 4g) | Yes |
| 5 | Step 5e: `on_input` lifecycle plus `submit`/`command_action`/`command_param`; `is_held`/`just_pressed` no longer replay-safe outside `on_input` | Additive (new functions; existing ones keep working, just lose their replay guarantee outside `on_input`) |
| 5 | Step 5f: `on_turn` lifecycle plus `act`/`get_turn_number`/`get_speed`/`set_speed`; `trigger_turn` removed | Yes (`trigger_turn` removal) |
| 5.5 | `animate_move`/`animate_flash`/`animate_shake`/`is_animating` (the animation queue, Part 3) | Additive — no existing function's behavior changed |
| 6 | Collision layers become a bitmask internally (`docs/ember2d-phase6-plan.md` Step 7) | **No** — corrected here from an earlier "Yes" this table carried since before Step 7 actually shipped: `get_collider_layer`/`set_collider_layer`/`get_collider_mask`/`set_collider_mask` and every level file, the editor, and node-graph codegen still speak plain layer-name strings, completely unchanged. The `u32` bitmask is a private, internal representation swap behind those same signatures. |
| 6 | `get_angle_to`'s `atan2` replaced with a deterministic rational approximation (`docs/ember2d-phase6-plan.md` Step 12, §5.2 H2) | No — same signature, same units (radians), numerically different by up to ~0.01 rad (~0.6°) from the old libm-backed value. See §3's "Spatial queries" note below for why a script converting the result back to a direction via Rhai's own `.cos()`/`.sin()` is a *separate*, still-unresolved determinism hazard this fix does not (and structurally cannot) close. |
| 7.5 | Step 7.5-1 (docs/ember2d-master-plan.md §5.6, R31/R32): every registered function taking a coordinate, size, or layer-order argument now accepts BOTH Rhai int and float literals (was a strict type match — Rhai never coerces between them for a registered native function, so `draw_hud(1, 2, ...)` used to fail "function not found" against an `i64`-typed signature, and `set_position(id, 5, 5)` failed the same way against an `f64`-typed one) | **No** — purely additive: each affected function gained a same-name overload for the other numeric type, the original signature/behavior unchanged. A script's numeric literals within one call must still be consistently one style (all int or all float), not freely mixed. |
| 7.5 | Step 7.5-1: `remove_global`/`clear_persistent` no longer alias `set_global`/`set_persistent(key, ())` — both used to signal "delete" by writing `Dynamic::UNIT` into the same pending-write map as a real value, so a script storing unit legitimately (`set_global("k", ())`) was silently deleted instead | Yes — `set_global("k", ())` now actually stores `()` at key `"k"`, readable back via `get_global`/`has_global`, where it used to delete the key instead. |
| 7.5 | Step 7.5-1: `load_level` is last-wins now (matching `save_game`/`play_music`, which already were) — was first-wins | Yes — a script calling `load_level` more than once in the same pass now loads whichever path it named LAST, not the first. |
| 7.5 | Step 7.5-2 (docs/ember2d-master-plan.md §5.6): `add_global`/`add_persistent` added, for accumulating a running total without the same-pass read-modify-write hazard every other `set_*` call has | **No** — purely additive: two new functions, nothing existing changed shape or behavior. |
| 7.5 | Step 7.5-3 (docs/ember2d-master-plan.md §5.6): `set_var`/`get_var`/`has_var`/`remove_var`/`add_var` added — real per-entity state (a new `Vars` component), replacing the `"hp_" + id`-style global-key-concatenation convention | **No** — purely additive: five new functions backed by a new component; every existing function's shape and behavior is unchanged. |
| 7.5 | Step 7.5-4 (docs/ember2d-master-plan.md §5.6): `get_stat`/`get_tint_aware`/`get_tint_asleep` added — numeric stats and an aware/asleep tint pair authored per actor tile via `TileRecord.actor.stats`/`tint_aware`/`tint_asleep`, letting `demos/roguelike/scripts/enemy.rhai` replace the near-duplicate `enemy_rat.rhai`/`enemy_boss.rhai` | **No** — purely additive: three new functions backed by new `ActorRecord`/`Actor` fields; every existing function's shape and behavior is unchanged. |
| 8 | Step 8-1 (docs/ember2d-master-plan.md §5.7): static tiles become cells of one `Tilemap` entity; `is_tilemap(id)` and `get_tile_tag(x,y)` added | **No bump** — no signature or return shape changed and `is_solid_at`/`get_path`/`reachable_within` answer identically (pinned by `ember2d/tests/tilemap_equivalence.rs` against the real floor2). But an *identity* changed: `get_entity_at`/`find_entities_in_rect`/`raycast`/`on_collide`'s `other` now return the tilemap's id where they used to return a wall's, so a script recognising walls by `has_tag(hit, "wall")` needs `is_tilemap(hit)`. Every shipped script that did (`bullet.rhai`, the only one) was updated in the same step. |
| 9 | Step 9-1 (docs/ember2d-master-plan.md §5.8): the scene stack — `push_scene`/`pop_scene`/`current_scene`/`scene_count`/`scene_data`/`quit_game`/`return_to_editor`/`is_editor_preview`, and the `on_start`/`on_input`/`on_update` contract for scene scripts. The Esc pause menu became a scene. | **No** — purely additive; a level's own scripts run exactly as before whenever no world-pausing scene is open. |
| 9 | Step 9-2: `set_camera_target`/`clear_camera_target`/`set_camera_zoom`/`get_camera_zoom`/`set_camera_bounds`/`clear_camera_bounds`/`set_camera_speed`; `get_mouse_world_x/y` account for zoom | **No** — additive. `set_camera(x,y)` now behaves as `set_camera_target(x,y)` (same effect, but `clear_camera_target()` can now undo it); at the default zoom of 1 every existing function returns what it did before. |
| 9 | Step 9-3: `menu_open`/`menu_selection`/`menu_closed`/`menu_close`, `draw_dialogue`/`dialogue_advance`/`dialogue_open`/`dialogue_done`/`close_dialogue`, `wrap_text`; R108 — every letter key now reaches `is_held`/`just_pressed` | **No** — additive (`draw_menu` and the cell HUD are unchanged). While a widget is open its keys are withheld from scripts, which no script could rely on before because no widget existed. |
| 9 | Step 9-4: `load_level(path, spawn)`; exit targets `path#spawn`; `get_spawn_point("player")` now returns the player's start | **No** — additive. Level format v7 (`spawns`) is a level-file change, not an API one; older levels are migrated when they load. |
| 9 | Step 9-5: `project.ron`'s `world_cell` — `get_mouse_world_x/y` divide by it | **No** — additive; a project without `world_cell` behaves exactly as before. |
| 9 | Step 9-7: `set_size`, `set_flip`, `set_sprite`, `play_project_clip`/`play_project_clip_once`, `set_y_sort` | **No** — additive. |
| 9.5 | Step 9.5-5: `save_data`, `load_data`; lifecycle functions a script doesn't define are no longer called | **No** — additive; skipping an undefined function changes nothing a script can observe. |
| 9.5 | Step 9.5-3: `make_actor`, `set_random_seed`; the project setting `ai_turns_per_step` | **No** — additive; a project without the setting resolves one actor per step, exactly as before. |
| 9.5 | Step 9.5-2: `compute_fov`, `is_in_fov`, `is_explored`, `fov_reset`, `set_fov_visibility` | **No** — additive; a level that never calls `compute_fov` draws exactly as before. A save gains `World::fov`/`fov_visibility` (`serde(default)`). |
| 9.5 | Step 9.5-1: `tile_def`, `tilemap_resize`, `tile_set`/`tile_set_layer`, `tile_fill`/`tile_fill_layer`, `tile_clear`/`tile_clear_layer`, `tile_clear_rect`/`tile_clear_rect_layer`, `get_tile` | **No** — additive. A save gains `World::tile_defs` (`serde(default)`; older saves load without it). |

**Phase 6 is a zero-API-break phase** — `API_VERSION` stayed `6` through
Step 5f. Phase 7.5-1 is the next break after it; 7.5-2 and 7.5-3 (the two
rows directly above) are both additive and do not bump it further.

`api_version()` was added in Step 3e (deferred from the original Phase 1 plan) —
it currently returns `7`: `1` was the pre-refactor baseline, `2` covers Phase 2's
row above (informational only — nothing script-visible actually changed), `3`
covers Phase 3's breaking renames (`set_color`/`set_z_order`/`set_animation`),
`4` covers Step 4g's `get_mouse_world_y` change, `5` covers Step 5e's command
boundary (`on_input`, `submit`, `command_action`, `command_param`), `6`
covers Step 5f's turn scheduler (`on_turn`, `act`, `get_turn_number`,
`get_speed`, `set_speed`, `trigger_turn` removed), and `7` covers Step 7.5-1's
three rows above it (uniform int/float typing, `set_global`/`set_persistent`
unit storage, `load_level` last-wins) — Step 7.5-2's `add_global`/
`add_persistent` row, Step 7.5-3's `set_var`/`get_var`/`has_var`/
`remove_var`/`add_var` row, and Step 7.5-4's `get_stat`/`get_tint_aware`/
`get_tint_asleep` row all shipped after `7` without needing an `8`/`9`/`10`,
being additive. Bump it at every future "yes" above.

---

## 7. Planned additions

**Phase 3 — sprites and animation**
The clip API (`register_clip`/`play_clip`/`play_clip_once`/`stop_clip`/`set_clip_speed`/`get_frame`/`set_frame`/`clip_finished`) shipped in Step 3c and is documented under §3 "Animation clips" — it's live, not planned. Still outstanding: `set_size(id,w,h)` · `set_rotation(id,rad)` · `set_src_rect(id,x,y,w,h)`.

Clips referenced **by name**, never by atlas coordinates — that's what keeps levels intact when art changes.

**Phase 5 — turns and animation events**
`act(cost)` · `get_turn_number()` · `get_speed(id)` · `set_speed(id,n)` shipped in Step 5f and are documented under §3 "The command boundary" — live, not planned. `animate_move(id,x,y,dur)` · `animate_flash(id,color,dur)` · `animate_shake(id,dur)` · `is_animating(id)` shipped in Phase 5.5 Part 3 and are documented under §3 "Turn animation" — also live, not planned. Still outstanding: `end_turn()` · `is_my_turn(id)`.

Costs are simulation time; animation durations are real seconds. Keeping them separate is what allows a "fast-forward animations" setting later without touching balance.

In realtime these are no-ops with sensible defaults, so one script runs under either time model.

---

## 8. Documentation debt

`FullScriptingAPI.txt` on `main` was a plan, not a record, and went stale. Avoid a repeat:

- Every function gets a signature, argument units, return value including the failure case, and a runnable example.
- Undocumented means not shipped.
- **Corrected in Step 4k**: this used to also say "keep `demo/scripts/api_test.rhai` exercising every function" — that file never existed on any branch, `demo/` is archived as of Phase 4 anyway (see `docs/archive/demo/README.md`), and no all-API smoke script exists today. Logged as future harness work instead, not a maintenance debt on a file that was never real: a script that calls every registered function once and asserts nothing errors would be a good addition whenever Phase 5's headless harness work happens, alongside the roguelike's own combat/level-integrity tests (`tests/roguelike_*.rs`, Step 4j).
