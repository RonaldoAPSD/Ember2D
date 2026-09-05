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
fn on_update(id, ctx)
fn on_collide(id, other, ctx)
```
All optional. A missing function is not an error.

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

### Spatial queries
`get_entity_at(x,y)` · `is_solid_at(x,y)` · `find_entities_in_rect(x,y,w,h)` · `get_distance(a,b)` · `get_angle_to(from,to)` (radians)

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

Gamepad: `gp_is_held(pad,btn)` · `gp_just_pressed(pad,btn)` · `gp_axis(pad,axis)`

Mouse: `get_mouse_x()` · `get_mouse_y()` (cells) · `get_mouse_world_x()` · `get_mouse_world_y()` · `mouse_left_pressed()` · `mouse_right_pressed()` · `mouse_left_held()` · `mouse_right_held()`

> `get_mouse_world_y` no longer subtracts a HUD row (Phase 4, Step 4g) — the
> world now gets the full viewport, and `HUD_TOP_ROWS` (the single constant
> both this and `Camera::viewport_origin` go through) is `0`. An earlier
> version of this doc claimed Phase 2 already removed the leak; it hadn't —
> Phase 2 only centralized the old bare `+1`/`-1` literal into that one
> constant, without zeroing it.

### Camera
`get_camera_x()` · `get_camera_y()` · `set_camera(x,y)` · `shake_camera(intensity,duration)`

Setting the camera overrides follow until cleared. Phase 2 gave the
internal `Camera` a `zoom` field, but there is still no scripted zoom
control (no `set_zoom`/`get_zoom`) — nothing here changed as of this
writing.

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

Per-entity, so names never collide. `timer_done` is true once, then consumes itself.

### Randomness
`random_int(min,max)` inclusive · `random_float()` · `random_bool(chance)` · `random_choice(array)`

> **Defect D3:** seeded from system entropy, so nothing is reproducible. Phase 1 moves the seed into the world.

### HUD
`draw_hud(x,y,text,fg,bg)` · `draw_box(x,y,w,h,fg,bg)` · `fill_rect(x,y,w,h,ch,fg,bg)` · `draw_panel(x,y,w,h,title,fg,bg)` · `draw_menu(x,y,w,options,selected,fg,bg,sel_fg,sel_bg)` · `clear_hud()`

Screen space, in cells. Cleared each frame.

### Effects and audio
`emit_particles(x,y,glyph,fg)` · `play_sound(path)` · `play_sound_at(path,x,y)` (volume falls off to 20 units) · `play_music(path)` · `stop_music()`

### Flow
`load_level(path)` · `save_game(path)` · `load_game(path)` · `trigger_turn()` · `log(msg)` · `get_delta()` · `get_elapsed()` · `get_spawn_point(name)` → `[x,y]` or `[]` · `get_viewport_width()` · `get_viewport_height()` · `api_version()` → int, this API's breaking-change generation (see §6)

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
        let a = ctx.get_angle_to(id, player);
        ctx.set_velocity(id, a.cos() * 4.0, a.sin() * 4.0);
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
| 5 | Scripts emit `Command` values; turn functions replace `trigger_turn` | Yes |
| 6 | Collision layers become a bitmask, not `String` | Yes |

`api_version()` was added in Step 3e (deferred from the original Phase 1 plan) —
it currently returns `4`: `1` was the pre-refactor baseline, `2` covers Phase 2's
row above (informational only — nothing script-visible actually changed), `3`
covers Phase 3's breaking renames (`set_color`/`set_z_order`/`set_animation`),
and `4` covers Step 4g's `get_mouse_world_y` change. Bump it at every future
"yes" above.

---

## 7. Planned additions

**Phase 3 — sprites and animation**
The clip API (`register_clip`/`play_clip`/`play_clip_once`/`stop_clip`/`set_clip_speed`/`get_frame`/`set_frame`/`clip_finished`) shipped in Step 3c and is documented under §3 "Animation clips" — it's live, not planned. Still outstanding: `set_size(id,w,h)` · `set_rotation(id,rad)` · `set_src_rect(id,x,y,w,h)`.

Clips referenced **by name**, never by atlas coordinates — that's what keeps levels intact when art changes.

**Phase 5 — turns and animation events**
`act(cost)` · `end_turn()` · `is_my_turn(id)` · `get_turn_number()` · `get_speed(id)` · `set_speed(id,n)` · `animate_move(id,x,y,dur)` · `animate_flash(id,color,dur)` · `is_animating(id)`

Costs are simulation time; animation durations are real seconds. Keeping them separate is what allows a "fast-forward animations" setting later without touching balance.

In realtime these are no-ops with sensible defaults, so one script runs under either time model.

---

## 8. Documentation debt

`FullScriptingAPI.txt` on `main` was a plan, not a record, and went stale. Avoid a repeat:

- Every function gets a signature, argument units, return value including the failure case, and a runnable example.
- Undocumented means not shipped.
- **Corrected in Step 4k**: this used to also say "keep `demo/scripts/api_test.rhai` exercising every function" — that file never existed on any branch, `demo/` is archived as of Phase 4 anyway (see `docs/archive/demo/README.md`), and no all-API smoke script exists today. Logged as future harness work instead, not a maintenance debt on a file that was never real: a script that calls every registered function once and asserts nothing errors would be a good addition whenever Phase 5's headless harness work happens, alongside the roguelike's own combat/level-integrity tests (`tests/roguelike_*.rs`, Step 4j).
