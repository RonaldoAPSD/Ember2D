// scripting/api.rs — ScriptCtx implementation (the Rhai API).

use super::state::ScriptState;
use super::types::*;
use crate::color::Color;
use crate::command::Command;
use rand::rngs::SmallRng;
use rhai::{Array, Dynamic};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct ScriptCtx {
    pub(super) inner: Rc<RefCell<ScriptState>>,
    pub(super) rng: Rc<RefCell<SmallRng>>,
    pub(super) entity_id: i64,
}

impl ScriptCtx {
    pub(super) fn new(state: ScriptState, rng: Rc<RefCell<SmallRng>>) -> Self {
        ScriptCtx { inner: Rc::new(RefCell::new(state)), rng, entity_id: -1 }
    }

    pub(super) fn with_entity(&self, id: i64) -> Self {
        let mut cloned = self.clone();
        cloned.entity_id = id;
        cloned
    }

    pub fn get_x(&mut self, id: i64) -> f64 {
        self.inner.borrow_mut().positions.get(&id).map(|(x, _)| *x as f64).unwrap_or(0.0)
    }
    pub fn get_y(&mut self, id: i64) -> f64 {
        self.inner.borrow_mut().positions.get(&id).map(|(_, y)| *y as f64).unwrap_or(0.0)
    }
    pub fn get_position(&mut self, id: i64) -> Array {
        self.inner
            .borrow_mut()
            .positions
            .get(&id)
            .map(|&(x, y)| vec![Dynamic::from(x as f64), Dynamic::from(y as f64)])
            .unwrap_or_default()
    }
    pub fn get_vel_x(&mut self, id: i64) -> f64 {
        self.inner.borrow_mut().velocities.get(&id).map(|(x, _)| *x as f64).unwrap_or(0.0)
    }
    pub fn get_vel_y(&mut self, id: i64) -> f64 {
        self.inner.borrow_mut().velocities.get(&id).map(|(_, y)| *y as f64).unwrap_or(0.0)
    }
    pub fn get_velocity(&mut self, id: i64) -> Array {
        self.inner
            .borrow_mut()
            .velocities
            .get(&id)
            .map(|&(x, y)| vec![Dynamic::from(x as f64), Dynamic::from(y as f64)])
            .unwrap_or_default()
    }

    pub fn get_tag(&mut self, id: i64) -> String {
        self.inner.borrow_mut().tags.get(&id).map(|t| t.to_string()).unwrap_or_default()
    }
    pub fn set_tag(&mut self, id: i64, tag: String) {
        self.inner.borrow_mut().pending_tags.push((id, tag));
    }
    pub fn has_tag(&mut self, id: i64, name: String) -> bool {
        self.inner.borrow_mut().tags.get(&id).map(|t| **t == name).unwrap_or(false)
    }

    /// Attaches (or replaces) `id`'s script (Step 7.5-5, docs/ember2d-
    /// master-plan.md §5.6). Deferred like every other setter — see
    /// `pending_set_script`'s own doc comment (scripting/state.rs) for why
    /// `on_start` doesn't run until the step *after* this one. Lets a
    /// script give an entity it just spawned its own `on_update`/
    /// `on_collide`, which `spawn_entity` alone never could — see
    /// `demos/shooter/scripts/director.rhai`'s pre-7.5-5 header comment for
    /// why that gap forced every enemy/bullet to be driven by hand from one
    /// script instead of scripts of their own.
    pub fn set_script(&mut self, id: i64, path: String) {
        self.inner.borrow_mut().pending_set_script.push((id, path));
    }

    pub fn get_glyph(&mut self, id: i64) -> String {
        self.inner.borrow_mut().glyphs.get(&id).map(|c| c.to_string()).unwrap_or_default()
    }
    // Phase 6 Step 4 (docs/ember2d-phase6-plan.md): `colors` stores `Color`
    // now, not a pre-formatted name string — see `WorldSnapshot::colors`'s
    // doc comment. `color_to_name` runs here instead, only for whichever
    // entity a script actually asks about.
    pub fn get_color(&mut self, id: i64) -> Array {
        self.inner
            .borrow_mut()
            .colors
            .get(&id)
            .map(|&(fg, bg)| {
                vec![Dynamic::from(color_to_name(fg)), Dynamic::from(color_to_name(bg))]
            })
            .unwrap_or_default()
    }
    pub fn get_texture(&mut self, id: i64) -> String {
        self.inner.borrow_mut().textures.get(&id).map(|p| p.to_string()).unwrap_or_default()
    }
    // `Rc<str>: Borrow<str>` (and its Hash/Eq/Ord delegate to `str`'s) is
    // what lets these three keep taking a plain Rhai `String` and looking it
    // up against a map keyed by `Rc<str>` — see `WorldSnapshot::tags`'s doc
    // comment (Phase 6 Step 4).
    pub fn find_by_tag(&mut self, tag: String) -> i64 {
        self.inner.borrow_mut().tag_to_id.get(tag.as_str()).copied().unwrap_or(-1)
    }
    pub fn find_all_by_tag(&mut self, tag: String) -> Array {
        self.inner
            .borrow_mut()
            .tag_to_ids
            .get(tag.as_str())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(Dynamic::from)
            .collect()
    }

    /// Kept registered for compatibility (Step 5e, docs/ember2d-phase5-plan.md)
    /// but no longer replay-safe: real key state is engine-side wall-clock
    /// input, not part of a recorded command stream, so a script that reads
    /// it outside `on_input` will diverge between a live run and a replay
    /// of the same commands. `on_input` is the only place a script should
    /// read raw input at all — everywhere else, read `command_action`/
    /// `command_param` instead.
    pub fn is_held(&mut self, key: String) -> bool {
        self.inner.borrow_mut().input.is_held(&key)
    }
    pub fn just_pressed(&mut self, key: String) -> bool {
        self.inner.borrow_mut().input.just_pressed(&key)
    }

    // ── Step 5e: the command boundary ─────────────────────────────────────────

    /// Queue a `Command` for `actor_id`. Meaningful only inside `on_input`
    /// — anything submitted from `on_update`/`on_collide`/`on_start` is
    /// still collected into this pass's `ScriptUpdateResult.commands`, but
    /// by the time the *next* `on_input` pass runs it will have already
    /// been overwritten, since commands don't accumulate across passes
    /// (see that field's doc comment). `params` accepts either ints or
    /// floats — Rhai integer literals (`1`, not `1.0`) are the common case
    /// in a script's `submit` call, so both are coerced to `f64` here
    /// rather than making every caller write `.0`.
    pub fn submit(&mut self, actor_id: i64, action: String, params: Array) {
        self.submit_with_cost_opt(actor_id, action, params, None);
    }

    /// Step 7.5-7 (docs/ember2d-master-plan.md §5.6): `submit` with an
    /// explicit turn cost — `TurnModel::ActionCost`'s write side. A
    /// 4-argument overload under the same Rhai name as `submit` above
    /// (arity-overload, same mechanism `spawn_entity`'s own multi-arity
    /// registration uses), rather than a required 4th parameter on
    /// `submit` itself, so every existing 3-argument call keeps compiling
    /// unchanged. Registered as `submit` in `registry.rs`, not under this
    /// Rust name.
    pub fn submit_with_cost(&mut self, actor_id: i64, action: String, params: Array, cost: f64) {
        self.submit_with_cost_opt(actor_id, action, params, Some(cost));
    }

    fn submit_with_cost_opt(
        &mut self,
        actor_id: i64,
        action: String,
        params: Array,
        cost: Option<f64>,
    ) {
        let params: Vec<f64> = params
            .into_iter()
            .filter_map(|d| d.as_float().ok().or_else(|| d.as_int().ok().map(|i| i as f64)))
            .collect();
        self.inner.borrow_mut().pending_commands.push(Command {
            actor: actor_id as crate::world::EntityId,
            action,
            params,
            cost,
        });
    }

    /// This entity's command for the current step, or `""` if `on_input`
    /// (this entity's own, or whichever actor's `ctx.submit` named it)
    /// didn't queue one.
    pub fn command_action(&mut self) -> String {
        self.inner
            .borrow_mut()
            .commands
            .get(&self.entity_id)
            .map(|c| c.action.clone())
            .unwrap_or_default()
    }

    /// The `i`-th param of this entity's current command, or `0.0` if
    /// there's no command or the index is out of range.
    pub fn command_param(&mut self, i: i64) -> f64 {
        self.inner
            .borrow_mut()
            .commands
            .get(&self.entity_id)
            .and_then(|c| c.params.get(i as usize).copied())
            .unwrap_or(0.0)
    }

    // ── Step 5f: the turn scheduler ───────────────────────────────────────────

    /// Marks this `on_turn` call as having consumed a turn, at `cost`
    /// energy (`TurnScheduler::advance`'s unit — 100 is a normal turn under
    /// today's `Alternating`-only scheduling). Replaces the removed
    /// `ctx.trigger_turn()`. For a locally-controlled actor, NOT calling
    /// this is how a rejected action (a wall bump, an out-of-potions quaff)
    /// costs nothing — the actor's own next `on_input` gets asked again
    /// immediately, no turn spent. An AI actor's turn always counts whether
    /// or not it calls this (a sleeping monster still "used" its turn doing
    /// nothing, and unconditionally NOT advancing it would wedge the
    /// scheduler forever) — see `PlayState::run_actor_turn`.
    pub fn act(&mut self, cost: f64) {
        self.inner.borrow_mut().pending_act_cost = Some(cost);
    }

    /// How many turns the local player has completed so far this level —
    /// replaces the old "turn" global (see player.rhai's header comment for
    /// what used to live there).
    pub fn get_turn_number(&mut self) -> i64 {
        self.inner.borrow_mut().turn_number
    }

    /// `Actor::speed` — vestigial until a non-`Alternating` scheduling mode
    /// ships (see that field's own doc comment); `0` for an entity with no
    /// `Actor` component at all.
    pub fn get_speed(&mut self, id: i64) -> f64 {
        self.inner.borrow_mut().actor_speeds.get(&id).copied().unwrap_or(0) as f64
    }
    pub fn set_speed(&mut self, id: i64, n: f64) {
        self.inner.borrow_mut().pending_speed.push((id, n.max(0.0) as u32));
    }

    /// A numeric stat authored on this actor's tile via `TileRecord.actor.
    /// stats` (Step 7.5-4, docs/ember2d-master-plan.md §5.6) — e.g.
    /// `ctx.get_stat(id, "hp")`. `0.0` for a missing key or a non-actor
    /// entity, matching every other `get_*` neutral-value convention
    /// (R32, 7.5-1) — there is no separate "does this key exist" query,
    /// same as `get_global`/`get_var`.
    pub fn get_stat(&mut self, id: i64, key: String) -> f64 {
        self.inner
            .borrow_mut()
            .actor_stats
            .get(&id)
            .and_then(|stats| stats.get(&key))
            .copied()
            .unwrap_or(0.0)
    }

    /// The tint this actor's sprite should wear once aware of the player /
    /// while it's still asleep (Step 7.5-4) — authored per-tile via
    /// `TileRecord.actor.tint_aware`/`tint_asleep`, kept out of `stats`
    /// because a `Color` isn't numeric. `"Reset"` (no override) for a
    /// non-actor entity, same neutral-default convention as `get_stat`.
    pub fn get_tint_aware(&mut self, id: i64) -> String {
        let inner = self.inner.borrow_mut();
        color_to_name(inner.actor_tints.get(&id).map(|&(aware, _)| aware).unwrap_or(Color::Reset))
    }
    pub fn get_tint_asleep(&mut self, id: i64) -> String {
        let inner = self.inner.borrow_mut();
        color_to_name(
            inner.actor_tints.get(&id).map(|&(_, asleep)| asleep).unwrap_or(Color::Reset),
        )
    }

    pub fn get_spawn_point(&mut self, name: String) -> Array {
        self.inner
            .borrow_mut()
            .spawns
            .get(&name)
            .map(|&(x, y)| vec![Dynamic::from(x as f64), Dynamic::from(y as f64)])
            .unwrap_or_default()
    }

    pub fn get_delta(&mut self) -> f64 {
        self.inner.borrow_mut().delta_time as f64
    }
    pub fn get_elapsed(&mut self) -> f64 {
        self.inner.borrow_mut().elapsed as f64
    }

    pub fn set_velocity(&mut self, id: i64, vx: f64, vy: f64) {
        self.inner.borrow_mut().pending_velocities.push((id, vx as f32, vy as f32));
    }
    /// `i64` overload — same reasoning as `draw_hud_f` above.
    pub fn set_velocity_i(&mut self, id: i64, vx: i64, vy: i64) {
        self.set_velocity(id, vx as f64, vy as f64)
    }
    /// R6 (7A-1): a non-finite position (NaN from a script's own bad math,
    /// e.g. `0.0 / 0.0`) used to flow straight into `Transform.position`,
    /// where it later broke `detect_collisions`'s sort (see that method's
    /// own R6 comment). Rejecting it here — a no-op, same convention as a
    /// setter targeting a missing entity — stops it at the boundary instead
    /// of chasing it through every downstream consumer.
    pub fn set_position(&mut self, id: i64, x: f64, y: f64) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        self.inner.borrow_mut().pending_positions.push((id, x as f32, y as f32));
    }
    /// `i64` overload — same reasoning as `draw_hud_f` above. An `i64` is
    /// always finite, so this never hits `set_position`'s own NaN guard.
    pub fn set_position_i(&mut self, id: i64, x: i64, y: i64) {
        self.set_position(id, x as f64, y as f64)
    }
    pub fn set_glyph(&mut self, id: i64, glyph_str: String) {
        if let Some(ch) = glyph_str.chars().next() {
            self.inner.borrow_mut().pending_glyphs.push((id, ch));
        }
    }
    /// Step 3e: was `set_color`. Still takes color names (`"Red"`) or, since
    /// `parse_color` now also reads them, explicit `"#RRGGBB"` hex values —
    /// `color_to_name` already emits that format for `Color::Rgb`, so this
    /// is a read-side addition, not a new wire format.
    ///
    /// R4 (7A-1): both channels are validated with `try_parse_color` before
    /// queueing anything. A byte-sliced-non-ASCII hex string used to panic
    /// here (`parse_color` indexed into the middle of a multi-byte
    /// character); an unrecognized name or malformed hex now just leaves
    /// the tint unchanged instead of silently overwriting it with `Reset` —
    /// the same "setter given garbage is a no-op" convention `set_position`
    /// above follows for a non-finite value. Logged once per distinct bad
    /// string (`ScriptState::logged_bad_colors`) rather than every call, so
    /// a script that repeats the same mistake every frame doesn't flood the
    /// console.
    pub fn set_tint(&mut self, id: i64, fg: String, bg: String) {
        let fg_ok = try_parse_color(&fg).is_some();
        let bg_ok = try_parse_color(&bg).is_some();
        let mut s = self.inner.borrow_mut();
        if !fg_ok {
            s.log_bad_color_once(&fg);
        }
        if !bg_ok {
            s.log_bad_color_once(&bg);
        }
        if fg_ok && bg_ok {
            s.pending_colors.push((id, fg, bg));
        }
    }

    pub fn set_texture(&mut self, id: i64, path: String) {
        let p = if path.is_empty() { None } else { Some(path) };
        self.inner.borrow_mut().pending_textures.push((id, p));
    }

    pub fn despawn(&mut self, id: i64) {
        self.inner.borrow_mut().despawn_queue.push(id);
    }

    /// Spawn an entity with just a glyph, position, and tag. Appearance and
    /// collider fall back to the engine's long-standing defaults: white
    /// glyph, z_order 2, a 1x1 non-solid trigger with no layer. Registered
    /// under the same Rhai name as `spawn_entity_full` (arity picks the
    /// overload), so existing scripts calling this 4-arg form are unaffected.
    pub fn spawn_entity(&mut self, glyph_str: String, x: f64, y: f64, tag: String) -> i64 {
        self.spawn_entity_full(
            glyph_str,
            x,
            y,
            tag,
            "White".to_string(),
            "Reset".to_string(),
            2,
            false,
            1.0,
            1.0,
            String::new(),
        )
    }
    /// `i64` overload — same reasoning as `draw_hud_f` above. Registered
    /// under the same Rhai name as `spawn_entity`/`spawn_entity_full_i`
    /// (arity picks the overload, same as the existing 4-arg/11-arg split).
    pub fn spawn_entity_i(&mut self, glyph_str: String, x: i64, y: i64, tag: String) -> i64 {
        self.spawn_entity(glyph_str, x as f64, y as f64, tag)
    }

    /// Spawn an entity with full control over appearance and collider
    /// (defect D10 — `spawn_entity` used to hardcode all of this).
    /// `fg`/`bg` are color names as in `set_tint`; `z` is draw order;
    /// `solid` marks a physical obstacle rather than a trigger; `w`/`h` are
    /// the collider size; `layer` is the collision layer (empty = unlabeled,
    /// matching the trigger-layer default from defect D4).
    pub fn spawn_entity_full(
        &mut self,
        glyph_str: String,
        x: f64,
        y: f64,
        tag: String,
        fg: String,
        bg: String,
        z: i64,
        solid: bool,
        w: f64,
        h: f64,
        layer: String,
    ) -> i64 {
        let glyph = glyph_str.chars().next().unwrap_or('?');
        let mut s = self.inner.borrow_mut();
        let id = s.next_spawn_id;
        s.next_spawn_id += 1;
        s.spawn_queue.push(SpawnRequest {
            id,
            glyph,
            x: x as f32,
            y: y as f32,
            tag,
            fg: parse_color(&fg),
            bg: parse_color(&bg),
            z: z as i32,
            solid,
            w: w as f32,
            h: h as f32,
            layer,
        });
        id as i64
    }
    /// `i64` overload — same reasoning as `draw_hud_f` above. `z` was
    /// already `i64` (draw order is already the right type — not cast).
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_entity_full_i(
        &mut self,
        glyph_str: String,
        x: i64,
        y: i64,
        tag: String,
        fg: String,
        bg: String,
        z: i64,
        solid: bool,
        w: i64,
        h: i64,
        layer: String,
    ) -> i64 {
        self.spawn_entity_full(
            glyph_str, x as f64, y as f64, tag, fg, bg, z, solid, w as f64, h as f64, layer,
        )
    }

    /// R32 (7.5-1, docs/ember2d-master-plan.md §5.6): was `if
    /// pending_level.is_none() { ... }` — first-wins, inconsistent with
    /// `save_game`/`play_music` below, which already overwrite
    /// unconditionally. A script calling `load_level` more than once in
    /// the same pass now gets the LAST one, matching those two.
    pub fn load_level(&mut self, path: String) {
        self.inner.borrow_mut().pending_level = Some(path);
    }
    /// Step 9-4: `load_level(path, spawn)` — enter the level at its spawn
    /// point named `spawn` instead of `"player"`. The same request as the
    /// `"path#spawn"` target an exit tile can hold (`Simulation::
    /// load_transition` splits it), so both routes share one path.
    pub fn load_level_at(&mut self, path: String, spawn: String) {
        let target = if spawn.is_empty() { path } else { format!("{path}#{spawn}") };
        self.inner.borrow_mut().pending_level = Some(target);
    }
    pub fn log(&mut self, msg: String) {
        self.inner.borrow_mut().pending_logs.push(msg);
    }
    pub fn draw_hud(&mut self, x: i64, y: i64, text: String, fg: String, bg: String) {
        self.inner.borrow_mut().pending_hud_draws.push(HudDraw::Text {
            x: x as usize,
            y: y as usize,
            text,
            fg: parse_color(&fg),
            bg: parse_color(&bg),
        });
    }
    /// `f64` overload (7.5-1, docs/ember2d-master-plan.md §5.6, R31) — Rhai
    /// dispatches to a registered function by EXACT argument type, with no
    /// int<->float coercion, so a script writing `draw_hud(1.0, 2.0, ...)`
    /// (float literals) needs this registered under the same Rhai name as
    /// the `i64` version above (`registry.rs`), or the call fails to
    /// resolve at all. Every other coordinate/size/layer-order function in
    /// this crate gets the same treatment, each with a short comment
    /// pointing back to this one rather than repeating the full "why."
    pub fn draw_hud_f(&mut self, x: f64, y: f64, text: String, fg: String, bg: String) {
        self.draw_hud(x as i64, y as i64, text, fg, bg)
    }

    pub fn draw_menu(
        &mut self,
        x: i64,
        y: i64,
        w: i64,
        options: Array,
        selected: i64,
        fg: String,
        bg: String,
        sel_fg: String,
        sel_bg: String,
    ) {
        let opts: Vec<String> = options.into_iter().map(|d| d.to_string()).collect();
        self.inner.borrow_mut().pending_hud_draws.push(HudDraw::Menu {
            x: x as usize,
            y: y as usize,
            w: w as usize,
            options: opts,
            selected: selected as usize,
            fg: parse_color(&fg),
            bg: parse_color(&bg),
            sel_fg: parse_color(&sel_fg),
            sel_bg: parse_color(&sel_bg),
        });
    }
    /// `f64` overload — same reasoning as `draw_hud_f` above.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_menu_f(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        options: Array,
        selected: f64,
        fg: String,
        bg: String,
        sel_fg: String,
        sel_bg: String,
    ) {
        self.draw_menu(
            x as i64,
            y as i64,
            w as i64,
            options,
            selected as i64,
            fg,
            bg,
            sel_fg,
            sel_bg,
        )
    }

    pub fn draw_panel(
        &mut self,
        x: i64,
        y: i64,
        w: i64,
        h: i64,
        title: String,
        fg: String,
        bg: String,
    ) {
        self.inner.borrow_mut().pending_hud_draws.push(HudDraw::Panel {
            x: x as usize,
            y: y as usize,
            w: w as usize,
            h: h as usize,
            title,
            fg: parse_color(&fg),
            bg: parse_color(&bg),
        });
    }
    /// `f64` overload — same reasoning as `draw_hud_f` above.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_panel_f(&mut self, x: f64, y: f64, w: f64, h: f64, title: String, fg: String, bg: String) {
        self.draw_panel(x as i64, y as i64, w as i64, h as i64, title, fg, bg)
    }

    pub fn play_sound(&mut self, path: String) {
        self.inner.borrow_mut().pending_sounds.push(path);
    }
    pub fn play_sound_at(&mut self, path: String, x: f64, y: f64) {
        self.inner.borrow_mut().pending_spatial_sounds.push((path, x as f32, y as f32));
    }
    /// `i64` overload — same reasoning as `draw_hud_f` above.
    pub fn play_sound_at_i(&mut self, path: String, x: i64, y: i64) {
        self.play_sound_at(path, x as f64, y as f64)
    }
    pub fn play_music(&mut self, path: String) {
        self.inner.borrow_mut().pending_music = Some(path);
    }
    pub fn stop_music(&mut self) {
        self.inner.borrow_mut().stop_music = true;
    }

    pub fn emit_particles(&mut self, x: f64, y: f64, glyph_str: String, fg: String) {
        let glyph = glyph_str.chars().next().unwrap_or('*');
        let fg_col = parse_color(&fg);
        self.inner.borrow_mut().pending_particles.push(ParticleRequest {
            x: x as f32,
            y: y as f32,
            glyph,
            fg: fg_col,
        });
    }
    /// `i64` overload — same reasoning as `draw_hud_f` above.
    pub fn emit_particles_i(&mut self, x: i64, y: i64, glyph_str: String, fg: String) {
        self.emit_particles(x as f64, y as f64, glyph_str, fg)
    }

    // Phase 5.5 Part 3's animation-queue methods (animate_move/animate_flash/
    // animate_shake/is_animating) live in api_animation.rs, a sibling
    // `impl ScriptCtx` block — see that file's own header comment for why.

    // Everything from the old "V0.4 Extensions" marker through `api_version`
    // (global state, randomness, entity/collider queries, mouse, camera,
    // persistence, HUD/draw utilities, timers, collision layers & masks,
    // V0.5 hierarchy, named animation clips) moved to api_ext.rs at 7A-10
    // (docs/ember2d-master-plan.md §5.1, R42) — this file was at 771/750
    // lines (CLAUDE.md's hard limit) after 7A-9's `cargo fmt --all`, with no
    // logic change of its own. Same third-sibling-`impl ScriptCtx`-block
    // pattern api_animation.rs/api_spatial.rs already established.
}
