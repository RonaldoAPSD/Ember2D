# Ember2D — Scripting API Reference

**Written against:** `gemini` branch, v0.5.0 (`src/scripting/api.rs`, registrations in `src/scripting/engine.rs`)
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
fn on_input(id, ctx)
fn on_update(id, ctx)
fn on_turn(id, ctx)
fn on_collide(id, other, ctx)
```
All optional. A missing function is not an error.

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
typically lives (see `roguelike/scripts/player.rhai`'s header comment), and
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
against the rhai 1.24 source, not just observed behavior). The scope
persists only as a place the *engine* writes into directly, from outside
the call, between calls — timers are the one example today (`__timer_*`
variables, set via `Scope::set_value` from `apply_ctx`, never by a
script's own `let`).

Any per-entity value a script itself needs to remember across calls must
go through `ctx.set_global`/`get_global` (level-scoped — resets on every
level load) or `ctx.set_persistent`/`get_persistent` (survives level
transitions) instead, keyed per-entity by string concatenation (e.g.
`"hp_" + id`). See `roguelike/scripts/player.rhai` and `enemy_rat.rhai` for
this in practice, including the sharp edge that comes with it: **a `get_*`
never observes a `set_*` from earlier in the same script pass** — every
write is deferred and only applied after every script has run that frame,
so a value lazy-initialized this same call still reads back as `()`, not
the value just set. Guard such reads (an `or_zero()`-style helper, or a
value computed directly in its own init branch rather than defaulted) —
see either script's header comment for two real bugs this caused and how
they were found.

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

Colours are **name strings** (`"Red"`, `"Reset"`) or an explicit `"#RRGGBB"` hex value (Step 3e). Unknown names silently become `Reset`.

### Animation clips
`register_clip(name,"abc",fps,looping)` defines (or redefines) a named clip from a string of glyphs. `play_clip(id,name)` plays it respecting the clip's own `looping` flag; `play_clip_once(id,name)` plays it but always stops on the last frame. `stop_clip(id)` · `set_clip_speed(id,x)` · `get_frame(id)` → int · `set_frame(id,n)` · `clip_finished(id)` → bool, true for exactly the tick a non-looping run reaches its last frame.

> Replaces `set_animation(id,"abc",rate)`, removed in Step 3e — a clip is named and shared, not a bag of fields re-set every call.

### Entity lifecycle
`spawn_entity(glyph,x,y,tag)` → id · `despawn(id)`

> `spawn_entity` hardcodes white / z=2 / 1×1 trigger collider (defect D10). Phase 1 adds parameters.

### Colliders
`get_collider_w(id)` · `get_collider_h(id)` · `set_collider_size(id,w,h)` · `is_collider_solid(id)` · `set_collider_solid(id,bool)` · `get_collider_layer(id)` · `set_collider_layer(id,name)` · `get_collider_mask(id)` · `set_collider_mask(id,array)`

An empty mask means "collide with everything".

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

`raycast(x1,y1,x2,y2,mask)` → `[id, hit_x, hit_y]` or `[]`. Finite segment, solids only, skips self.

`get_path(x1,y1,x2,y2,mask)` → `[[x,y],…]`. A\* on the integer grid, 4-directional, 2000-node cap. Empty array means no path or already there.

### Hierarchy
`get_parent(id)` · `set_parent(id,parent)` · `set_parent_keep_world(id,parent)` · `get_world_x(id)` · `get_world_y(id)`

Pass `-1` as parent to detach. Cycle guard at depth 100.

### Input
`is_held(key)` · `just_pressed(key)` — lowercase names (`"w"`, `"space"`, `"escape"`, `"left"`).

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
  `cost` energy (100 is a normal turn under today's `Alternating`-only
  scheduling — see `scheduler.rs`'s `ALTERNATING_COST`). Replaces the
  removed `ctx.trigger_turn()`. For a `Local` actor, **not** calling this
  is how a rejected action (a wall bump, an empty-handed quaff) costs
  nothing — the same actor is asked again next step instead of the turn
  advancing. An AI actor's turn always counts whether or not it calls this
  (a sleeping monster still "used" its turn doing nothing — unconditionally
  skipping the advance for AI would wedge the scheduler on it forever).
- `get_turn_number()` → how many turns the local player has completed so
  far this level. Engine-tracked (not a script global), so unlike the old
  "turn" global this has no same-pass deferred-write lag to guard against.
- `get_speed(id)` / `set_speed(id, n)` — an actor's `Actor::speed`.
  Vestigial today: `TurnScheduler` charges every actor the same flat cost
  regardless of speed (only `Alternating` scheduling ships) — but a real,
  honestly-functioning read/write, not a stub, so a future non-`Alternating`
  mode needs no scripting-API change to start consulting it.

This whole boundary is what makes a recorded/transmitted command stream
(the eventual replay test, Step 5h; lockstep netcode, Phase 9b) fully
determine what happens next without real key events at replay time. See
`roguelike/scripts/player.rhai` for the full pattern, including how it
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
`roguelike/scripts/enemy_rat.rhai` and `enemy_boss.rhai` call `animate_move`
right alongside their own `set_position`; the player's own movement is
deliberately left un-animated, both to avoid adding input latency to
something that already felt instant, and because it means the player is
*never* gated by this at all — only an actor that animates itself waits on
its own animation.

`is_animating(id)` always returns `false` today, but not for the reason an
earlier version of this doc gave (that nothing scripted could run at all
while any animation was in flight — no longer true after D20). The real
reason: this function is registered in `ember2d-sim`, which by design has
no visibility into `PlayState.animations` — that queue is presentation
state, owned entirely by the `ember2d` crate, and the sim/presentation
split this engine maintains means the sim can't see it regardless of how
fine-grained the gate is. It's registered now, not stubbed out or left
erroring, so a future revision that threads a per-entity "is animating"
flag back into the sim's own snapshot could make it meaningful without a
further scripting-API change — but that's a real design question (what
should own that state, and does it belong in `WorldSnapshot`), not a small
fix.

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
`get_camera_x()` · `get_camera_y()` · `set_camera(x,y)` · `shake_camera(intensity,duration)`

Setting the camera overrides follow until cleared. Phase 2 gave the
internal `Camera` a `zoom` field, but there is still no scripted zoom
control (no `set_zoom`/`get_zoom`) — nothing here changed as of this
writing.

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
Globals (per level): `set_global` · `get_global` · `has_global` · `remove_global`
Persistent (across levels): `set_persistent` · `get_persistent` · `has_persistent` · `clear_persistent` · `clear_all_persistent`

> **Defect D2 (fixed in Phase 1):** `set_persistent` inside `on_start` used
> to be silently discarded — `PlayState::on_start` ran `on_start` scripts
> against a fresh, throwaway `HashMap` instead of the engine's real
> persistent store. Fixed by threading the real store through
> `GameState::on_start`'s signature instead; see `tests/persistent_on_start.rs`
> for the regression test.

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

> **`timer_done` is not quite "true once, then consumes itself."** Corrected
> in Step 9, since tracing the exact sentinel path found it isn't: a
> cancelled timer (`cancel_timer`) and a just-fired one both resolve to the
> same internal storage value, which is itself still within the range
> `timer_done`'s own guard treats as "done" — so the very next check after
> either event reports `true` again, and keeps doing so until enough real
> simulation steps decay it past that range (roughly 8 minutes at 60
> steps/second). Logged as **D22** (docs/ember2d-refactor-plan.md §3), not
> fixed — no shipped script (`roguelike/`, `shooter/`) calls any of these
> three functions today, so nothing observable is broken by it. Don't rely
> on a single `timer_done` check being the last one that ever returns `true`
> for a given name; a script that cares should track its own "already
> handled this" flag alongside it.

### Randomness
`random_int(min,max)` inclusive · `random_float()` · `random_bool(chance)` · `random_choice(array)`

> **Defect D3:** seeded from system entropy, so nothing is reproducible. Phase 1 moves the seed into the world.

### HUD
`draw_hud(x,y,text,fg,bg)` · `draw_box(x,y,w,h,fg,bg)` · `fill_rect(x,y,w,h,ch,fg,bg)` · `draw_panel(x,y,w,h,title,fg,bg)` · `draw_menu(x,y,w,options,selected,fg,bg,sel_fg,sel_bg)` · `clear_hud()`

Screen space, in cells. Cleared each frame.

### Effects and audio
`emit_particles(x,y,glyph,fg)` · `play_sound(path)` · `play_sound_at(path,x,y)` (volume falls off to 20 units) · `play_music(path)` · `stop_music()`

### Flow
`load_level(path)` · `save_game(path)` · `load_game(path)` · `log(msg)` · `get_delta()` · `get_elapsed()` · `get_spawn_point(name)` → `[x,y]` or `[]` · `get_viewport_width()` · `get_viewport_height()` · `api_version()` → int, this API's breaking-change generation (see §6)

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

**Failure is quiet.** Setters on missing entities do nothing; getters return zero values. A script error should disable that script, not kill the game — currently it only suppresses the log (defect D9).

**Sentinels.** `-1` means "no entity". Never `0` — that's the reserved null id.

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

**Phase 6 is a zero-API-break phase** — `API_VERSION` stays `6`, unchanged
since Step 5f. Both rows above are corrections/clarifications, not breaks:
the collision-layer bitmask never touched a script-facing signature, and the
`atan2` replacement keeps `get_angle_to`'s exact signature and units, just
computing the same angle via different (deterministic) arithmetic.

`api_version()` was added in Step 3e (deferred from the original Phase 1 plan) —
it currently returns `6`: `1` was the pre-refactor baseline, `2` covers Phase 2's
row above (informational only — nothing script-visible actually changed), `3`
covers Phase 3's breaking renames (`set_color`/`set_z_order`/`set_animation`),
`4` covers Step 4g's `get_mouse_world_y` change, `5` covers Step 5e's command
boundary (`on_input`, `submit`, `command_action`, `command_param`), and `6`
covers Step 5f's turn scheduler (`on_turn`, `act`, `get_turn_number`,
`get_speed`, `set_speed`, `trigger_turn` removed). Bump it at every future
"yes" above.

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
