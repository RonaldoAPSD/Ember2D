// components/actor.rs — makes an entity eligible to take turns under
// `TurnScheduler` (Step 5f, docs/ember2d-phase5-plan.md).
//
// Nothing about this component is realtime-specific — it exists purely to
// answer the scheduler's two questions: "who acts next" (via `speed`, once
// a non-`Alternating` mode ships) and "where do their commands come from"
// (via `controller`). A `RealTime`-mode project can ignore it entirely: the
// scheduler still runs (every player unconditionally gets `Local(0)`, see
// `play/spawn.rs`), but a script with no `on_input`/`on_turn` functions
// never notices — see `scheduler.rs`'s header comment.
//
// `stats`/`tint_aware`/`tint_asleep` (Step 7.5-4, docs/ember2d-master-plan.md
// §5.6) are the runtime copy of `TileRecord.actor`'s own `ActorRecord`
// fields — see that type's doc comment (level.rs) for why they exist and
// why `Color` lives outside `stats`. Their presence is why this struct is
// no longer `Copy`: a `BTreeMap` isn't.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::color::Color;

/// Who supplies this actor's commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Controller {
    /// A local player, indexed by slot. Slot 0 is the only one anything
    /// constructs today — Step 5g (pluralizing the player) is what would
    /// ever produce a second one.
    Local(u8),
    /// Anything scripted — every enemy in the roguelike.
    Ai,
    /// A remote peer, indexed by slot. Phase 9's netcode; nothing
    /// constructs this today.
    Remote(u8),
}

/// Makes an entity eligible to take turns under `TurnScheduler`.
///
/// `speed` is currently vestigial: every actor in this phase costs a flat
/// `scheduler::ALTERNATING_COST` per turn regardless of its value — see
/// that constant's doc comment for why ("ship only `Alternating`", per the
/// plan). It's a real, honestly-functioning field (`ctx.get_speed`/
/// `set_speed` read and write it for real) so a future non-`Alternating`
/// scheduling mode doesn't need a level-format migration to start
/// consulting it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    pub speed: u32,
    pub controller: Controller,
    /// See `ActorRecord::stats` (level.rs). Empty for the player
    /// (`Actor::local`) — nothing today reads a local player's own stats
    /// through this mechanism.
    #[serde(default)]
    pub stats: BTreeMap<String, f64>,
    /// See `ActorRecord::tint_aware`/`tint_asleep` (level.rs).
    /// `#[serde(default)]` lets a pre-7.5-4 save (this component already
    /// existed) load with `Color::Reset` — no override — for both.
    #[serde(default = "default_tint")]
    pub tint_aware: Color,
    #[serde(default = "default_tint")]
    pub tint_asleep: Color,
    /// Step 7.5-6 (docs/ember2d-master-plan.md §5.6): opts this actor OUT
    /// of engine-side solid collision resolution (`late_step`'s
    /// `physics_actor_pair`, simulation/step.rs) — an opt-out, not an
    /// opt-in, so `#[serde(default = "default_physics")]` reads `true` for
    /// every pre-7.5-6 save (this field didn't exist before), matching the
    /// behavior every existing `Actor` already had. An actor that moves
    /// only via direct `ctx.set_position` writes and its own `is_solid_at`
    /// guard (every roguelike enemy today) never reaches a physics check
    /// either way — this only matters for an actor that also carries a
    /// `Transform.velocity` the engine's own `World::integrate_physics`
    /// moves for it.
    #[serde(default = "default_physics")]
    pub physics: bool,
}

fn default_tint() -> Color {
    Color::Reset
}

fn default_physics() -> bool {
    true
}

impl Actor {
    /// The player, always — `play/spawn.rs` gives every player entity one
    /// of these unconditionally, regardless of the project's
    /// `GameplayLoop`.
    pub fn local(slot: u8) -> Self {
        Actor {
            speed: 100,
            controller: Controller::Local(slot),
            stats: BTreeMap::new(),
            tint_aware: Color::Reset,
            tint_asleep: Color::Reset,
            physics: true,
        }
    }

    /// Every authored enemy tile (`rat()`/`boss()` in
    /// `examples/gen_roguelike.rs`) — see `TileRecord::actor`. Stats/tint
    /// default empty/`Reset` here; `simulation/spawn.rs::do_on_start` fills
    /// them in from the tile's own `ActorRecord` right after construction.
    pub fn ai(speed: u32) -> Self {
        Actor {
            speed,
            controller: Controller::Ai,
            stats: BTreeMap::new(),
            tint_aware: Color::Reset,
            tint_asleep: Color::Reset,
            physics: true,
        }
    }
}
