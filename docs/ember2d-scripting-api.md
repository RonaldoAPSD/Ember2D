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
Each entity keeps a persistent `rhai::Scope`. `let` variables at function top level survive between calls. Timers currently live here as `__timer_*` variables.

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
`get_glyph(id)` · `set_glyph(id,"X")` · `get_color(id)` → `[fg,bg]` · `set_color(id,fg,bg)` · `get_texture(id)` · `set_texture(id,path)` · `set_animation(id,"abc",rate)` · `is_visible(id)` · `set_visible(id,bool)` · `get_z_order(id)` · `set_z_order(id,z)`

Colours are **name strings** (`"Red"`, `"Reset"`). Unknown names silently become `Reset`.

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

> `get_mouse_world_y` subtracts 1 for the hardcoded HUD row. Phase 2 removes that leak.

### Camera
`get_camera_x()` · `get_camera_y()` · `set_camera(x,y)` · `shake_camera(intensity,duration)`

Setting the camera overrides follow until cleared. Phase 2 adds zoom.

### State
Globals (per level): `set_global` · `get_global` · `has_global` · `remove_global`
Persistent (across levels): `set_persistent` · `get_persistent` · `has_persistent` · `clear_persistent` · `clear_all_persistent`

> **Defect D2:** `set_persistent` inside `on_start` is silently discarded. Phase 1.

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
`load_level(path)` · `save_game(path)` · `load_game(path)` · `trigger_turn()` · `log(msg)` · `get_delta()` · `get_elapsed()` · `get_spawn_point(name)` → `[x,y]` or `[]` · `get_viewport_width()` · `get_viewport_height()`

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
        ctx.set_color(id, "Red", "Reset");
    } else {
        let path = ctx.get_path(ctx.get_x(id), ctx.get_y(id),
                                ctx.get_x(player), ctx.get_y(player), ["solid"]);
        if path.len() > 0 {
            let step = path[0];
            ctx.set_velocity(id, (step[0] - ctx.get_x(id)) * 4.0,
                                 (step[1] - ctx.get_y(id)) * 4.0);
        }
        ctx.set_color(id, "Yellow", "Reset");
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
| 2 | Mouse world coords lose the HUD-row fudge; camera gains zoom | Yes |
| 3 | `set_color` → `set_tint`; colour names → explicit values | Yes |
| 3 | `set_z_order` → `set_layer_order` | Yes |
| 3 | `set_animation(chars)` → clip references by name; sheet clips added | Yes |
| 3 | `set_texture(path)` → texture handles | Yes |
| 4 | Player movement, score, HUD move from `play.rs` into scripts | Additive |
| 5 | Scripts emit `Command` values; turn functions replace `trigger_turn` | Yes |
| 6 | Collision layers become a bitmask, not `String` | Yes |

Add `api_version()` in Phase 1 and bump it at every "yes" above.

---

## 7. Planned additions

**Phase 3 — sprites and animation**
`play_clip(id,name)` · `play_clip_once(id,name)` · `stop_clip(id)` · `set_clip_speed(id,x)` · `get_frame(id)` · `set_frame(id,n)` · `clip_finished(id)` · `set_size(id,w,h)` · `set_rotation(id,rad)` · `set_src_rect(id,x,y,w,h)`

Clips referenced **by name**, never by atlas coordinates — that's what keeps levels intact when art changes.

**Phase 5 — turns and animation events**
`act(cost)` · `end_turn()` · `is_my_turn(id)` · `get_turn_number()` · `get_speed(id)` · `set_speed(id,n)` · `animate_move(id,x,y,dur)` · `animate_flash(id,color,dur)` · `is_animating(id)`

Costs are simulation time; animation durations are real seconds. Keeping them separate is what allows a "fast-forward animations" setting later without touching balance.

In realtime these are no-ops with sensible defaults, so one script runs under either time model.

---

## 8. Documentation debt

`FullScriptingAPI.txt` on `main` was a plan, not a record, and went stale. Avoid a repeat:

- Every function gets a signature, argument units, return value including the failure case, and a runnable example.
- Keep `demo/scripts/api_test.rhai` exercising every function; run it as part of the regression checklist.
- Undocumented means not shipped.
