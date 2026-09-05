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

use serde::{Serialize, Deserialize};

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
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Actor {
    pub speed: u32,
    pub controller: Controller,
}

impl Actor {
    /// The player, always — `play/spawn.rs` gives every player entity one
    /// of these unconditionally, regardless of the project's
    /// `GameplayLoop`.
    pub fn local(slot: u8) -> Self {
        Actor { speed: 100, controller: Controller::Local(slot) }
    }

    /// Every authored enemy tile (`rat()`/`boss()` in
    /// `examples/gen_roguelike.rs`) — see `TileRecord::actor`.
    pub fn ai(speed: u32) -> Self {
        Actor { speed, controller: Controller::Ai }
    }
}
