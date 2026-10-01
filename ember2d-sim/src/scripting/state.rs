// scripting/state.rs — ScriptState: the per-frame snapshot + write-queue
// scripts see and mutate through `ScriptCtx`.
//
// Split out of engine.rs (sibling-file convention, matching play.rs's
// mod render;/mod spawn; split) once engine.rs crossed the project's
// 600-line hard limit (CLAUDE.md) — Step 3c's clip/animator wiring pushed
// it over. Pure relocation: nothing here changed behavior, only location.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use crate::color::Color;
use crate::command::{Command, GamepadSnapshot, InputSnapshot, MouseSnapshot};
use crate::components::{AnimationClip, SpriteSource};
use crate::world::World;

use super::types::*;

// Step 5b (docs/ember2d-phase5-plan.md, §5.2 H1): the maps below that are
// ever iterated as a whole — rather than only looked up by a single known
// key — are `BTreeMap`, not `HashMap`. `colliders` in particular backs
// `get_entity_at`, `find_entities_in_rect`, and `raycast`'s exact-tie case
// in `scripting/api.rs`, all of which pick a "first" or "all" result by
// iteration order; a `HashMap` there means that result changes between
// runs for reasons with nothing to do with game state. Maps only ever
// accessed by `.get(&known_key)` (`velocities`, `parents`, `glyphs`,
// `colors`, `textures`, `tag_to_id`, gamepad state) stay
// `HashMap` — their own iteration order, if any, is never observed.
//
// PULLED OUT OF `ScriptState` INTO ITS OWN, SHARED, `Rc`-WRAPPED TYPE IN
// STEP 5F'S PERFORMANCE FIX (docs/ember2d-phase5-plan.md — found live: the
// user reported the game dropping to ~21 fps on floor2, which has ~3x
// floor1's entity/collider count). Building these maps is a full O(entities)
// pass over `World` — before this fix, `PlayState::update` rebuilt one from
// scratch for `on_input`, again for the housekeeping `on_update` pass, and
// again for `on_turn` when an actor's turn resolved: up to 3 full rebuilds
// in a single real frame, added by Step 5e/5f on top of the one rebuild
// that already existed pre-5e. Release-mode benchmarking
// (`ScriptEngine::from_world` called directly, bypassing script execution
// entirely) measured floor2 at ~4.4ms/step with all 2-3 rebuilds and
// ~2.5ms/step with just one — the redundant rebuilds were themselves over
// half the per-step cost, worse in an unoptimized debug build (where the
// user actually saw it) since none of that allocation gets inlined away.
// `WorldSnapshot::build` now runs once per step in `PlayState::update`
// (play.rs) and its `Rc` is cloned (O(1)) into every pass that step needs.
//
// Consequence to know about: because the snapshot is frozen at the start of
// a step, a pass later in that same step won't see a write an *earlier*
// pass in the *same* step made to `World` directly — this was already true
// within a single pass (deferred writes; see this module's header comment
// on `ScriptState` below) and is now also true *across* on_input/on_update/
// on_turn within one step. In practice this doesn't matter for any script
// today: on_input's contract is "read input, call `ctx.submit`" — it never
// mutates world state a later pass would need to observe. If a future
// script ever needs same-step mutation visibility across these three
// passes specifically, that's the seam to revisit.
pub struct WorldSnapshot {
    pub(super) positions: BTreeMap<i64, (f32, f32)>,
    pub(super) velocities: HashMap<i64, (f32, f32)>,
    pub(super) parents: HashMap<i64, i64>,
    /// (width, height, solid, layer, mask, locked, layer_bits). `layer_bits`
    /// (Phase 6 Step 7, docs/ember2d-phase6-plan.md) is copied straight off
    /// each `Collider`'s own pre-resolved bit — this snapshot needs no
    /// `LayerRegistry` access to build it, only to read one back out. It
    /// backs `raycast`/`get_path`'s mask filtering (`api_spatial.rs`);
    /// `mask_bits` is deliberately NOT carried here, since nothing reads a
    /// snapshotted collider's own mask — the pairwise mask test lives in
    /// `World::detect_collisions`, against `World`'s real `Collider`s
    /// directly, not this snapshot.
    ///
    /// Still `String`/`Vec<String>`, not `Rc<str>`/`Rc<[Rc<str>]>` — Step
    /// 7.5-10 (docs/ember2d-master-plan.md §5.6) considered and deliberately
    /// skipped this: `tags`'s own `Rc<str>` (below) pays for itself because
    /// ONE allocation there is shared, by cheap `Rc::clone`, into three
    /// different maps (`tags`/`tag_to_id`/`tag_to_ids`); `layer`/`mask` are
    /// written into exactly one map (this one), and `Collider`'s own fields
    /// (`components/collider.rs`) are themselves `String`/`Vec<String>`, not
    /// `Rc`-backed — so switching this field's TYPE alone would still
    /// allocate exactly once per collider per step, identical cost to
    /// today, while adding a `.to_string()` conversion at every
    /// `get_collider_layer`/`get_collider_mask` read site (`api_ext.rs`) to
    /// hand Rhai a `String` back. The master plan's own text pairs this
    /// line item with a per-step layer/spatial index (deferred at this same
    /// step, see 7.5-10's "Landed as" note) that WOULD share one `Rc` across
    /// several maps the way `tags` does — worth revisiting once that index
    /// exists, not before.
    pub(super) colliders: BTreeMap<i64, (f32, f32, bool, String, Vec<String>, bool, u32)>,
    /// This snapshot's own copy of the level's layer name<->bit table
    /// (Phase 6 Step 7) — what `raycast`/`get_path` fold their incoming
    /// Rhai `mask: Array` argument against, once per call, instead of a
    /// per-candidate string comparison. See `crate::layers::LayerRegistry`'s
    /// own doc comment; cheap to clone in (at most 31 entries), so this
    /// snapshot owns it rather than sharing a reference that would need its
    /// own lifetime threaded through every `ScriptState` constructor.
    pub(super) layers: crate::layers::LayerRegistry,
    /// Phase 6 Step 4 (docs/ember2d-phase6-plan.md): one `Rc<str>` per
    /// tagged entity, shared (by cheap `Rc::clone` — a refcount bump, not an
    /// allocation) into this map, `tag_to_id`, and `tag_to_ids` below,
    /// instead of each of the three doing its own `String::clone()` of the
    /// same name. `find_by_tag`/`find_all_by_tag`/`count_by_tag` still take
    /// a plain `String` from Rhai — `Rc<str>: Borrow<str>` (and its `Hash`/
    /// `Eq`/`Ord` all delegate to `str`'s) is what lets `.get(name.as_str())`
    /// keep working against a map keyed by `Rc<str>` unchanged.
    pub(super) tags: BTreeMap<i64, Rc<str>>,
    pub(super) glyphs: HashMap<i64, char>,
    /// Phase 6 Step 4: stores the tint `Color` values directly rather than
    /// the name strings `get_color` returns — building this used to run
    /// `color_to_name` (a `String` allocation) on both fg and bg for every
    /// sprite whether or not any script ever calls `get_color` on that
    /// entity. `get_color` (api.rs) now does that conversion itself, on the
    /// entities that actually ask for it.
    pub(super) colors: HashMap<i64, (Color, Color)>,
    pub(super) textures: HashMap<i64, Rc<str>>,
    pub(super) tag_to_id: HashMap<Rc<str>, i64>,
    pub(super) tag_to_ids: BTreeMap<Rc<str>, Vec<i64>>,
    pub(super) visibility: BTreeMap<i64, bool>,
    pub(super) z_orders: BTreeMap<i64, i32>,
    /// Read-only per-entity `Animator::frame` snapshot backing `get_frame`.
    pub(super) animator_frames: BTreeMap<i64, usize>,
    /// Entities whose `Animator` reached the last frame of a non-looping
    /// run on the tick this snapshot was taken from — what
    /// `clip_finished(id)` reads. Populated from `Animator::just_finished`,
    /// which is itself only ever true for the one tick that set it.
    pub(super) clip_finished: HashSet<i64>,
    /// Read-only per-entity `Actor::speed` snapshot backing `ctx.get_speed`.
    /// Vestigial today — `TurnScheduler` charges every actor the same flat
    /// cost regardless (`scheduler::ALTERNATING_COST`) — but a real,
    /// honestly-functioning read, not a stub, so a future non-`Alternating`
    /// mode that does consult it needs no scripting-API change.
    pub(super) actor_speeds: HashMap<i64, u32>,
    /// Read-only per-entity `Actor::stats` snapshot (Step 7.5-4, docs/ember2d-
    /// master-plan.md §5.6) backing `ctx.get_stat` — lookup-only, like
    /// `actor_speeds` immediately above, so a `HashMap` here doesn't violate
    /// the sim's no-`HashMap`-iteration invariant (§4.1). Only entities with
    /// a non-empty `stats` map get an entry.
    pub(super) actor_stats: HashMap<i64, BTreeMap<String, f64>>,
    /// Read-only per-entity aware/asleep tint pair backing
    /// `ctx.get_tint_aware`/`get_tint_asleep` (Step 7.5-4) — see
    /// `Actor::tint_aware`/`tint_asleep`'s own doc comment for why these
    /// live outside `stats`. Lookup-only, same as `actor_speeds`.
    pub(super) actor_tints: HashMap<i64, (Color, Color)>,
    /// Read-only per-entity `Vars` snapshot (Step 7.5-3, docs/ember2d-
    /// master-plan.md §5.6) backing `ctx.get_var`/`has_var` — frozen at the
    /// start of the pass, same as every other field here, so a `set_var`
    /// this pass is invisible to a `get_var` later in the SAME pass (see
    /// `ScriptState.pending_vars`'s own doc comment). An entity with no
    /// `Vars` component at all (never had `set_var` called on it) simply
    /// has no entry here — `get_var` treats that the same as an entry with
    /// no matching key.
    pub(super) vars: BTreeMap<i64, BTreeMap<String, rhai::Dynamic>>,
    /// The level's named spawn points (`LevelData::spawns`, one map since
    /// Step 9-4 — `"player"` included) — backs `ctx.get_spawn_point`. A
    /// clone of the level's own `BTreeMap`. Built once here, not once per
    /// `ScriptState`
    /// (Step 7.5-10, docs/ember2d-master-plan.md §5.6): the level's spawn
    /// list never changes mid-play, but every `run_*` method used to
    /// rebuild this same `HashMap` from scratch every single call —
    /// `run_on_input`/`run_scripts`/`run_on_turn` alone meant up to 3
    /// redundant rebuilds of the identical map every step, on top of
    /// whatever `run_collisions` added on a collision-heavy step. Now built
    /// once per `WorldSnapshot`, shared via the same `Rc::clone` every
    /// other field here already relies on.
    pub(super) spawns: BTreeMap<String, (f32, f32)>,
    /// Every `Tilemap` in the world (Step 8-1, docs/ember2d-master-plan.md
    /// §5.7), keyed by its entity id — what `is_solid_at`/`raycast`/
    /// `get_path`/etc. (`api_spatial.rs`) check alongside `colliders`.
    /// An `Rc::clone` of `World`'s own, not a copy: this is the field that
    /// took `WorldSnapshot::build` from copying every wall to not copying
    /// any of them.
    pub(super) tilemaps: BTreeMap<i64, Rc<crate::components::Tilemap>>,
    /// Step 9.5-2: the field of view as of this pass (`is_in_fov`,
    /// `is_explored`) — an `Rc` share, like `tilemaps`.
    pub(super) fov: Option<Rc<crate::fov::FovMap>>,
}

impl WorldSnapshot {
    pub fn build(
        world: &World,
        layers: &crate::layers::LayerRegistry,
        spawns: &BTreeMap<String, (f32, f32)>,
    ) -> Self {
        // Phase 6 Step 4 (docs/ember2d-phase6-plan.md): pre-sized against
        // `World`'s own store lengths rather than growing by reallocation —
        // every one of these maps ends up with at most that many entries
        // (`glyphs`/`colors`/`textures`/`visibility`/`z_orders` are subsets
        // of `world.transforms`; `tag_to_id`/`tag_to_ids` bounded by
        // `world.tags`, which may have fewer unique names than entries, but
        // never more), so this is a safe upper bound, not a guess.
        let n_transforms = world.transforms.len();
        let n_tags = world.tags.len();

        let mut positions = BTreeMap::new();
        let mut velocities = HashMap::with_capacity(n_transforms);
        let mut parents = HashMap::new();
        let mut colliders = BTreeMap::new();
        let mut tags = BTreeMap::new();
        let mut glyphs = HashMap::with_capacity(n_transforms);
        let mut colors = HashMap::with_capacity(n_transforms);
        let mut textures = HashMap::new();
        let mut tag_to_id = HashMap::with_capacity(n_tags);
        let mut tag_to_ids: BTreeMap<Rc<str>, Vec<i64>> = BTreeMap::new();
        let mut visibility = BTreeMap::new();
        let mut z_orders = BTreeMap::new();

        for (id, tf) in &world.transforms {
            let eid = *id as i64;
            positions.insert(eid, (tf.position.x, tf.position.y));
            velocities.insert(eid, (tf.velocity.x, tf.velocity.y));
            if let Some(pid) = tf.parent {
                parents.insert(eid, pid as i64);
            }
            if let Some(sp) = world.sprites.get(id) {
                // bg only means something for a Glyph source; Texture/Clip
                // sprites report Reset ("no override"), matching how
                // draw_texture already ignored background entirely.
                let bg = match &sp.source {
                    SpriteSource::Glyph { ch, bg } => {
                        glyphs.insert(eid, *ch);
                        *bg
                    }
                    SpriteSource::Texture { path, .. } => {
                        textures.insert(eid, Rc::from(path.as_str()));
                        Color::Reset
                    }
                    SpriteSource::Clip { .. } => Color::Reset,
                };
                // Phase 6 Step 4: stored as `Color`, not `color_to_name`'d
                // here — see this struct's `colors` field doc comment.
                colors.insert(eid, (sp.tint, bg));
                visibility.insert(eid, sp.visible);
                z_orders.insert(eid, sp.layer);
            }
        }
        for (id, col) in &world.colliders {
            colliders.insert(
                *id as i64,
                (
                    col.width,
                    col.height,
                    col.solid,
                    col.layer().to_string(),
                    col.mask().to_vec(),
                    col.locked,
                    col.layer_bits(),
                ),
            );
        }
        let mut actor_speeds = HashMap::with_capacity(world.actors.len());
        let mut actor_stats = HashMap::new();
        let mut actor_tints = HashMap::with_capacity(world.actors.len());
        for (id, actor) in &world.actors {
            let eid = *id as i64;
            actor_speeds.insert(eid, actor.speed);
            if !actor.stats.is_empty() {
                actor_stats.insert(eid, actor.stats.clone());
            }
            actor_tints.insert(eid, (actor.tint_aware, actor.tint_asleep));
        }
        for (id, tag) in &world.tags {
            let eid = *id as i64;
            // Phase 6 Step 4: one allocation (`Rc::from`), then two cheap
            // refcount-bump clones — see this struct's `tags` field doc
            // comment for why that's safe against `find_by_tag`/etc.'s
            // `String`-keyed lookups.
            let name: Rc<str> = Rc::from(tag.name.as_str());
            tags.insert(eid, name.clone());
            tag_to_id.entry(name.clone()).or_insert(eid);
            tag_to_ids.entry(name).or_default().push(eid);
        }

        let mut animator_frames = BTreeMap::new();
        let mut clip_finished = HashSet::with_capacity(world.animators.len());
        for (id, animator) in &world.animators {
            let eid = *id as i64;
            animator_frames.insert(eid, animator.frame);
            if animator.just_finished {
                clip_finished.insert(eid);
            }
        }

        let mut vars = BTreeMap::new();
        for (id, v) in &world.vars {
            vars.insert(*id as i64, v.values.clone());
        }


        WorldSnapshot {
            positions,
            velocities,
            parents,
            colliders,
            tags,
            glyphs,
            colors,
            textures,
            tag_to_id,
            tag_to_ids,
            visibility,
            z_orders,
            animator_frames,
            clip_finished,
            actor_speeds,
            actor_stats,
            actor_tints,
            vars,
            layers: layers.clone(),
            spawns: spawns.clone(),
            tilemaps: world.tilemaps.iter().map(|(id, m)| (*id as i64, Rc::clone(m))).collect(),
            fov: world.fov.clone(),
        }
    }
}

pub(super) struct ScriptState {
    /// The expensive, `World`-derived read-only maps — see this field's
    /// type's own doc comment for why it's shared via `Rc` instead of
    /// owned outright. `ScriptState` derefs to it, so every existing
    /// `self.positions`/`self.colliders`/etc. access in `scripting/api.rs`
    /// keeps compiling unchanged — Rust's field-access autoderef finds
    /// them there.
    pub(super) snapshot: Rc<WorldSnapshot>,
    pub(super) delta_time: f32,
    pub(super) elapsed: f32,
    pub(super) next_spawn_id: crate::world::EntityId,
    /// Was two separate `HashSet<String>` fields (`held_keys`/
    /// `just_pressed_keys`) until Step 5e (docs/ember2d-phase5-plan.md)
    /// introduced the sim-safe `InputSnapshot` type — see that type's own
    /// doc comment (command.rs) for why raw input is now this shape.
    pub(super) input: InputSnapshot,
    // The level's spawn points used to live here, rebuilt on every
    // `ScriptState` construction — Step 7.5-10 (docs/ember2d-master-plan.md
    // §5.6) moved them onto `WorldSnapshot` instead (see that field's own
    // doc comment), built once per step and shared the same way every other
    // snapshot field is. `self.spawns` (api.rs's `get_spawn_point`) still
    // resolves — `ScriptState` derefs to `WorldSnapshot`.
    pub(super) mouse_pos: (f32, f32),
    pub(super) mouse_held: (bool, bool),
    pub(super) mouse_pressed: (bool, bool),

    pub(super) gamepad_held: HashSet<(usize, String)>,
    pub(super) gamepad_pressed: HashSet<(usize, String)>,
    pub(super) gamepad_axes: HashMap<(usize, String), f32>,

    pub(super) globals: BTreeMap<String, rhai::Dynamic>,
    /// Script-registered animation definitions (Step 3c), threaded through
    /// the same in/out-per-frame pattern as `globals` since — like
    /// globals — these live on `PlayState`, not `World`.
    pub(super) clips: BTreeMap<String, AnimationClip>,
    pub(super) persistent: BTreeMap<String, rhai::Dynamic>,
    /// R9 (7A-1, docs/ember2d-master-plan.md): set by `clear_all_persistent`
    /// instead of clearing `pending_persistent` directly — see that
    /// method's own doc comment (api.rs) for why clearing the write queue
    /// was a no-op. `apply_ctx` clears `persistent` itself when this is
    /// true, then applies `pending_persistent` on top, so a same-pass
    /// `set_persistent` after the clear still lands.
    pub(super) pending_persistent_clear_all: bool,
    /// R4 (7A-1): distinct malformed/unrecognized color strings already
    /// reported this pass, via `log_bad_color_once` below — rebuilt fresh
    /// every `ScriptState` (i.e. every script pass), so "once" here means
    /// "once per pass," not "once ever"; good enough to stop one `set_tint`
    /// call's fg/bg from double-logging without the extra bookkeeping a
    /// truly permanent dedup set would need to survive across passes.
    pub(super) logged_bad_colors: BTreeSet<String>,
    /// The calling entity's command from the *previous* `on_input` pass,
    /// looked up by actor id — what `ctx.command_action()`/`command_param()`
    /// read (Step 5e, docs/ember2d-phase5-plan.md). Read-only here, unlike
    /// `pending_commands` below; whoever constructs a `ScriptState` decides
    /// what this holds — the on_update pass gets the on_input pass's own
    /// result, every other pass gets an empty map (see the `run_*` methods
    /// in engine.rs).
    pub(super) commands: BTreeMap<i64, Command>,
    /// How many turns the local player has completed so far this level —
    /// what `ctx.get_turn_number()` reads (Step 5f, docs/ember2d-phase5-plan.md).
    /// Engine-tracked plain data, not a script global: `PlayState::turn_number`
    /// increments it directly in `run_actor_turn` right after the player's
    /// own `on_turn` consumes a turn, so unlike the old "turn" global this
    /// has no same-pass deferred-write lag to guard against.
    pub(super) turn_number: i64,
    pub(super) camera_pos: (f32, f32),
    pub(super) viewport_size: (usize, usize),
    pub(super) pending_velocities: Vec<(i64, f32, f32)>,
    pub(super) pending_positions: Vec<(i64, f32, f32)>,
    pub(super) pending_parents: Vec<(i64, i64, bool)>,
    pub(super) pending_glyphs: Vec<(i64, char)>,
    pub(super) pending_colors: Vec<(i64, String, String)>,
    pub(super) pending_textures: Vec<(i64, Option<String>)>,
    pub(super) pending_hud_draws: Vec<HudDraw>,
    pub(super) pending_particles: Vec<ParticleRequest>,
    pub(super) clear_hud: bool,
    pub(super) despawn_queue: Vec<i64>,
    pub(super) spawn_queue: Vec<SpawnRequest>,
    pub(super) pending_level: Option<String>,
    pub(super) pending_save: Option<String>,
    pub(super) pending_load: Option<String>,
    pub(super) pending_logs: Vec<String>,
    pub(super) pending_sounds: Vec<String>,
    pub(super) pending_spatial_sounds: Vec<(String, f32, f32)>,
    pub(super) pending_music: Option<String>,
    pub(super) stop_music: bool,
    /// `PendingWrite`, not a raw `rhai::Dynamic` (7.5-1, docs/ember2d-
    /// master-plan.md §5.6, R32) — see that type's own doc comment for why
    /// a real `Set`/`Remove` distinction replaced writing `Dynamic::UNIT`
    /// to mean "delete."
    pub(super) pending_globals: BTreeMap<String, PendingWrite>,
    /// `register_clip` writes here rather than into `clips` directly, so a
    /// clip a script registers this frame only becomes visible (to that
    /// script or any other) starting next frame — the same "writes settle
    /// at frame end" convention every other pending_* queue follows.
    pub(super) pending_clip_defs: Vec<(String, AnimationClip)>,
    /// (id, clip name, oneshot) — `play_clip`/`play_clip_once`.
    pub(super) pending_play_clip: Vec<(i64, String, bool)>,
    pub(super) pending_stop_clip: Vec<i64>,
    pub(super) pending_clip_speed: Vec<(i64, f32)>,
    pub(super) pending_set_frame: Vec<(i64, usize)>,
    /// `PendingWrite`, same reasoning as `pending_globals` above.
    pub(super) pending_persistent: BTreeMap<String, PendingWrite>,
    /// Step 9-2 (docs/ember2d-master-plan.md §5.8): this pass's camera
    /// requests — was a single `Option<Vec2>` for `set_camera` alone.
    pub(super) pending_camera: super::camera::CameraWrites,
    /// Step 9-2: the camera as scripts had set it when this pass began —
    /// what `get_camera_zoom` reads.
    pub(super) camera_view: super::camera::CameraSettings,
    /// Step 9-5: glyph cells per world unit at zoom 1 (`(1, 1)` unless the
    /// project's world cell isn't 8×16) — `get_mouse_world_x/y` divide by it.
    pub(super) cell_scale: (f32, f32),
    /// Step 9-3 (docs/ember2d-master-plan.md §5.8): menus and dialogue as
    /// they stood when this pass began, plus this pass's widget requests.
    pub(super) ui: super::widgets::UiCtx,
    /// Step 9-7: this pass's sprite requests, in call order (`sprite.rs`).
    pub(super) sprite_ops: Vec<super::sprite::SpriteOp>,
    /// Step 9.5-1: this pass's tile requests, in call order (`tiles.rs`).
    pub(super) tile_ops: Vec<super::tiles::TileOp>,
    /// Step 9.5-2: this pass's field-of-view requests (`fov_api.rs`).
    pub(super) fov_ops: Vec<super::fov_api::FovOp>,
    pub(super) pending_shake: Option<ShakeState>,
    pub(super) pending_visibility: Vec<(i64, bool)>,
    pub(super) pending_z_order: Vec<(i64, i32)>,
    pub(super) pending_tags: Vec<(i64, String)>,
    /// `ctx.set_script`'s write queue (Step 7.5-5, docs/ember2d-master-
    /// plan.md §5.6) — `(entity id, script path)`. Applied in `apply_ctx`
    /// with the same ghost-component guard `pending_tags` uses (R10): no
    /// `Script` for an entity nothing else spawned this pass. A successful
    /// attach also queues the entity onto `ScriptEngine::pending_on_start`
    /// so its `on_start` runs at the next step boundary, not this one —
    /// this pass's own `scripted` list (in whichever `run_*` queued this
    /// write) was already snapshotted before the attach lands.
    pub(super) pending_set_script: Vec<(i64, String)>,
    pub(super) pending_collider_size: Vec<(i64, f32, f32)>,
    pub(super) pending_collider_solid: Vec<(i64, bool)>,
    pub(super) pending_collider_layer: Vec<(i64, String)>,
    pub(super) pending_collider_locked: Vec<(i64, bool)>,
    pub(super) pending_collider_mask: Vec<(i64, Vec<String>)>,
    /// `ctx.start_timer`/`cancel_timer`/`timer_done`'s write queue — Step
    /// 7.5-8 (D22 fix): `TimerWrite`, not a raw `f64` sentinel. See that
    /// type's own doc comment (types.rs) for the ambiguity it replaces.
    pub(super) pending_timers: Vec<(crate::world::EntityId, String, TimerWrite)>,
    /// `ctx.set_var`/`remove_var`'s write queue (Step 7.5-3) — `(entity id,
    /// key, write)`. Applied in `apply_ctx` directly onto `World::vars`,
    /// guarded against a nonexistent entity the same way `pending_tags`
    /// already is (R10, 7A-1) — no `Vars` for an entity nothing else
    /// spawned this pass either. `PendingWrite`, not a raw `Dynamic`, same
    /// `Set`/`Remove` distinction `pending_globals`/`pending_persistent`
    /// use (7.5-1, R32) — `set_var(id, "k", ())` must store unit, not alias
    /// `remove_var(id, "k")`.
    pub(super) pending_vars: Vec<(i64, String, PendingWrite)>,
    /// Phase 6 Step 9 (docs/ember2d-phase6-plan.md): `mem::take`n out of
    /// `ScriptEngine.timers` at the start of whichever `run_*` method built
    /// this `ScriptState`, and put back by `apply_ctx` before it returns —
    /// same round-trip shape `globals`/`clips`/`persistent` already use
    /// (Step 3), just entirely internal to `ScriptEngine` rather than
    /// surfacing through `ScriptUpdateResult`. `BTreeMap` outer and inner,
    /// matching `ScriptEngine.timers`'s own doc comment for why.
    pub(super) timers: BTreeMap<crate::world::EntityId, BTreeMap<String, TimerState>>,
    /// `ctx.submit()`'s write queue (Step 5e, docs/ember2d-phase5-plan.md)
    /// — meaningful only from `on_input`; see `commands`'s own doc comment
    /// for the read side.
    pub(super) pending_commands: Vec<Command>,
    /// `ctx.act()`'s write queue (Step 5f) — see `ScriptUpdateResult::act_cost`'s
    /// doc comment for what this means and who reads it.
    pub(super) pending_act_cost: Option<f64>,
    pub(super) pending_speed: Vec<(i64, u32)>,
    /// `ctx.animate_move`/`animate_flash`/`animate_shake`'s write queue
    /// (Phase 5.5 Part 3, docs/ember2d-phase5.5-plan.md) — drained into
    /// `ScriptUpdateResult::animations`, same shape as `pending_particles`.
    pub(super) pending_animations: Vec<AnimationEvent>,
    /// Step 7.5-7 (docs/ember2d-master-plan.md §5.6): entity ids
    /// `ember2d::play::PlayState`'s own animation queue has an in-flight
    /// `PlayingAnimation` for, this real step — read-only, what
    /// `ctx.is_animating(id)` (scripting/api_animation.rs) checks
    /// membership against. Always empty unless the caller sets it — only
    /// `run_scripts`/`run_on_input`/`run_on_turn` (engine.rs), the three
    /// passes `Simulation::step` actually drives from a real `StepInput`,
    /// ever populate it; `run_on_start_all`/`run_on_load_all`/
    /// `run_collisions` leave it at this default, correctly, since nothing
    /// can be mid-animation before a level has even started stepping (the
    /// former two) or has a `StepInput` of its own to read one from (the
    /// latter, called from `late_step`, not `step`). A plain `Vec`, not a
    /// `HashSet` — never more than a handful of entities animate at once,
    /// so a linear `.contains()` scan in `is_animating` is simpler than
    /// justifying a set for it.
    pub(super) animating: Vec<i64>,
    /// Step 9-1 (docs/ember2d-master-plan.md §5.8): the scene stack as it
    /// stood when this pass began, plus this pass's scene/flow requests —
    /// see `scripting/scene.rs`.
    pub(super) scene: super::scene::SceneCtx,
}

impl std::ops::Deref for ScriptState {
    type Target = WorldSnapshot;
    fn deref(&self) -> &WorldSnapshot {
        &self.snapshot
    }
}

/// The thirteen fields every `ScriptState` constructor below needs beyond
/// `world`/`snapshot`/`persistent` — introduced at Step 7.5-10 (docs/
/// ember2d-master-plan.md §5.6) to replace the positional-argument lists
/// `from_world`/`from_snapshot`, and every `run_*` method in `engine.rs`/
/// `lifecycle.rs`/`collisions.rs` that calls them, had each accumulated one
/// field at a time as this crate grew (`input`/`mouse`/`gamepad` in Phase 5,
/// `commands`/`turn_number` in Step 5f, `viewport_size` later still). Two
/// same-typed positional arguments in a row (`delta_time`/`elapsed`, both
/// `f32`) is exactly the shape a call site can transpose and have the
/// compiler say nothing — named struct fields can't be. `persistent` is
/// deliberately NOT a field here: every caller takes it as `&mut` and reads
/// it back out after the call (`std::mem::take` on the way in, restored via
/// `ScriptUpdateResult::persistent` on the way out), which a struct field
/// consumed by value can't express as cleanly. `spawns`/`animating` (the
/// latter lives on the `run_*` methods that take it directly, not here —
/// see `ScriptState.animating`'s own doc comment for why `run_on_start_all`/
/// `run_on_load_all`/`run_collisions` never populate it) are borrowed, not
/// owned, hence the lifetime. `pub`, not `pub(super)` — unlike `ScriptState`
/// itself, this is built by every `run_*` method's own caller
/// (`simulation.rs`/`simulation/step.rs`/`simulation/spawn.rs`, outside
/// `scripting` entirely), not just from within this module.
pub struct PassArgs<'a> {
    pub delta_time: f32,
    pub elapsed: f32,
    pub input: InputSnapshot,
    pub mouse: MouseSnapshot,
    pub gamepad: GamepadSnapshot,
    pub spawns: &'a BTreeMap<String, (f32, f32)>,
    pub globals: BTreeMap<String, rhai::Dynamic>,
    pub clips: BTreeMap<String, AnimationClip>,
    pub camera_pos: crate::math::Vec2,
    pub commands: BTreeMap<i64, Command>,
    pub turn_number: i64,
    pub viewport_size: (usize, usize),
}

impl ScriptState {
    /// Convenience wrapper for callers that don't (or can't easily) share a
    /// `WorldSnapshot` across multiple passes — `run_on_start_all`/
    /// `run_on_load_all` (`scripting/lifecycle.rs`) and `run_collisions`
    /// (`scripting/collisions.rs`), called far less often than every real
    /// step, so a fresh rebuild each time doesn't matter the way it did for
    /// `on_input`/`on_update`/`on_turn` (see `WorldSnapshot`'s own doc
    /// comment). Frequent callers should build a `WorldSnapshot` once and
    /// call `from_snapshot` instead.
    pub(super) fn from_world(
        world: &World,
        layers: &crate::layers::LayerRegistry,
        persistent: BTreeMap<String, rhai::Dynamic>,
        args: PassArgs,
    ) -> Self {
        let snapshot = Rc::new(WorldSnapshot::build(world, layers, args.spawns));
        Self::from_snapshot(snapshot, world.next_id, persistent, args)
    }

    /// The frequent-caller path: `snapshot` was already built once this
    /// step (`PlayState::update`, play.rs) and is shared — `Rc::clone`
    /// below is O(1), not another full pass over `World`. `next_spawn_id`
    /// is still read fresh from `World` (not the frozen snapshot) since
    /// `apply_ctx` updates `world.next_id` directly and a later pass this
    /// same step must see any spawn an earlier one made.
    pub(super) fn from_snapshot(
        snapshot: Rc<WorldSnapshot>,
        next_spawn_id: crate::world::EntityId,
        persistent: BTreeMap<String, rhai::Dynamic>,
        args: PassArgs,
    ) -> Self {
        // `spawns` deliberately unused here — the snapshot's copy is built
        // once inside `WorldSnapshot::build` (see that field's own doc
        // comment) and read back through `ScriptState`'s `Deref`, not
        // rebuilt per pass. `..` drops `spawns` along with `args`'s other
        // now-consumed fields.
        let PassArgs {
            delta_time,
            elapsed,
            input,
            mouse,
            gamepad,
            globals,
            clips,
            camera_pos,
            commands,
            turn_number,
            viewport_size,
            ..
        } = args;
        let mouse_pos = mouse.cell;
        let mouse_held = mouse.held;
        let mouse_pressed = mouse.pressed;
        let GamepadSnapshot { held: gamepad_held, pressed: gamepad_pressed, axes: gamepad_axes } =
            gamepad;

        ScriptState {
            snapshot,
            delta_time,
            elapsed,
            next_spawn_id,
            input,
            mouse_pos,
            mouse_held,
            mouse_pressed,
            gamepad_held,
            gamepad_pressed,
            gamepad_axes,
            globals,
            clips,
            persistent,
            pending_persistent_clear_all: false,
            logged_bad_colors: BTreeSet::new(),
            commands,
            turn_number,
            camera_pos: (camera_pos.x, camera_pos.y),
            viewport_size,
            pending_velocities: Vec::new(),
            pending_positions: Vec::new(),
            pending_parents: Vec::new(),
            pending_glyphs: Vec::new(),
            pending_colors: Vec::new(),
            pending_textures: Vec::new(),
            pending_hud_draws: Vec::new(),
            pending_particles: Vec::new(),
            clear_hud: false,
            despawn_queue: Vec::new(),
            spawn_queue: Vec::new(),
            pending_level: None,
            pending_save: None,
            pending_load: None,
            pending_logs: Vec::new(),
            pending_sounds: Vec::new(),
            pending_spatial_sounds: Vec::new(),
            pending_music: None,
            stop_music: false,
            pending_globals: BTreeMap::new(),
            pending_clip_defs: Vec::new(),
            pending_play_clip: Vec::new(),
            pending_stop_clip: Vec::new(),
            pending_clip_speed: Vec::new(),
            pending_set_frame: Vec::new(),
            pending_persistent: BTreeMap::new(),
            pending_camera: Default::default(),
            camera_view: Default::default(),
            cell_scale: (1.0, 1.0),
            ui: Default::default(),
            sprite_ops: Vec::new(),
            tile_ops: Vec::new(),
            fov_ops: Vec::new(),
            pending_shake: None,
            pending_visibility: Vec::new(),
            pending_z_order: Vec::new(),
            pending_tags: Vec::new(),
            pending_set_script: Vec::new(),
            pending_collider_size: Vec::new(),
            pending_collider_solid: Vec::new(),
            pending_collider_layer: Vec::new(),
            pending_collider_mask: Vec::new(),
            pending_collider_locked: Vec::new(),
            pending_timers: Vec::new(),
            pending_vars: Vec::new(),
            timers: BTreeMap::new(),
            pending_commands: Vec::new(),
            pending_act_cost: None,
            pending_speed: Vec::new(),
            pending_animations: Vec::new(),
            animating: Vec::new(),
            scene: Default::default(),
        }
    }

    /// R4 (7A-1): records `bad` into a plain info log entry the first time
    /// this pass sees it, via `logged_bad_colors`'s own doc comment above —
    /// silent after that, for the rest of this pass.
    pub(super) fn log_bad_color_once(&mut self, bad: &str) {
        let trimmed = bad.trim().to_string();
        if self.logged_bad_colors.insert(trimmed.clone()) {
            self.pending_logs
                .push(format!("[script] malformed or unrecognized color '{}', ignored", trimmed));
        }
    }
}
