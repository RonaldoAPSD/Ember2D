// scheduler.rs — deterministic turn order for TurnBased-mode play (Step 5f,
// docs/ember2d-phase5-plan.md).
//
// Sim-side (no winit, no rendering) — the 5i workspace-split table lists
// `scheduler` alongside `sim`/`commands` under the future `ember2d-sim`
// crate.
//
// DEVIATION FROM THE PLAN'S OWN SKETCH: the plan's pseudocode has this type
// itself decide "AwaitingCommand" vs "run on_turn", threaded through
// `sim::step`'s generic `GameState`/`StepResult` types. That would mean
// teaching the fully-generic `sim::step` (used by every `GameState`, not
// just `PlayState`) about `Actor`/`Controller` — a `PlayState`-specific
// concept `sim.rs` otherwise knows nothing about. Instead this type stays a
// dumb ordering primitive (`peek`/`advance`/`insert`/`remove`);
// `PlayState::update` (play.rs) owns the actual "is the front actor local,
// does it have a command yet" policy, using `World::actors` directly. Same
// observable behavior — one actor's turn resolved per step, render-and-wait
// when a local actor has nothing queued yet — for a smaller diff to the
// generic engine plumbing.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::components::Controller;
use crate::world::EntityId;

/// Every actor in this phase costs exactly this much "energy" per turn —
/// see the plan's `Energy`/`ActionCost`/`Declared` note in §5f; only
/// `Alternating` (uniform cost, ties broken by controller kind then id)
/// ships today. `Actor::speed` doesn't factor into this yet — see that
/// field's own doc comment.
pub const ALTERNATING_COST: u64 = 100;

/// Local actors sort before Ai/Remote at an equal due time. Without this, a
/// level whose enemies happen to have lower `EntityId`s than the player —
/// true for the roguelike, since `play/spawn.rs` spawns every tile
/// (enemies included) before the player — would let AI act before the
/// player's very first input ever arrives, on the very first round where
/// everyone starts at the same due time.
fn controller_rank(c: Controller) -> u8 {
    match c {
        Controller::Local(_) => 0,
        Controller::Ai => 1,
        Controller::Remote(_) => 2,
    }
}

/// One actor per step (plan §5f). A min-heap on `(due, rank, EntityId)` —
/// `Reverse` turns Rust's max-heap `BinaryHeap` into the min-heap this
/// needs. `due` is never real time, just an internal tally that increases
/// by a turn's cost each time its actor acts, so "lowest due" always means
/// "hasn't acted this round yet" (or acted longest ago) — with `rank` and
/// `EntityId` making every tie fully deterministic.
pub struct TurnScheduler {
    queue: BinaryHeap<Reverse<(u64, u8, EntityId)>>,
}

impl TurnScheduler {
    pub fn new() -> Self {
        TurnScheduler { queue: BinaryHeap::new() }
    }

    /// Schedule `actor` to join the current round — the lowest `due`
    /// value already queued, or `0` for an empty scheduler. Called once per
    /// actor when (re)building the scheduler at level load
    /// (`PlayState::rebuild_scheduler`); a hypothetical actor spawned
    /// mid-level would join whatever round is in progress rather than
    /// waiting a full cycle.
    pub fn insert(&mut self, actor: EntityId, controller: Controller) {
        let due = self.queue.peek().map(|Reverse((d, _, _))| *d).unwrap_or(0);
        self.queue.push(Reverse((due, controller_rank(controller), actor)));
    }

    /// Drop `actor` from the queue — call on despawn, or a dead entity's
    /// turn slot keeps cycling forever, doing nothing each time (harmless,
    /// but a leak). See `PlayState::apply_script_result`.
    pub fn remove(&mut self, actor: EntityId) {
        self.queue.retain(|Reverse((_, _, id))| *id != actor);
    }

    /// The actor whose turn it is next, without removing it — `None` only
    /// if nothing is scheduled at all (e.g. every actor has despawned).
    pub fn peek(&self) -> Option<EntityId> {
        self.queue.peek().map(|Reverse((_, _, id))| *id)
    }

    /// `actor` (which must be exactly what `peek()` just returned) has
    /// finished its turn — pop it and reinsert it `cost` energy later.
    /// `cost` is clamped to at least 1 so a script that calls `ctx.act(0.0)`
    /// (or a negative value) can't wedge the actor at its own current due
    /// time and starve everyone behind it.
    pub fn advance(&mut self, actor: EntityId, controller: Controller, cost: u64) {
        if let Some(Reverse((due, _, _))) = self.queue.pop() {
            self.queue.push(Reverse((due + cost.max(1), controller_rank(controller), actor)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_acts_before_ai_at_an_equal_due_time_even_with_a_higher_entity_id() {
        let mut s = TurnScheduler::new();
        // Enemies spawned (and therefore id-assigned) before the player —
        // exactly play/spawn.rs's own order — must not let a lower-id AI
        // actor act before the player's first turn.
        s.insert(1, Controller::Ai);
        s.insert(2, Controller::Ai);
        s.insert(99, Controller::Local(0));
        assert_eq!(s.peek(), Some(99), "the local player must be checked first despite the highest id");
    }

    #[test]
    fn round_robin_cycles_every_actor_once_before_repeating() {
        let mut s = TurnScheduler::new();
        s.insert(10, Controller::Local(0));
        s.insert(20, Controller::Ai);
        s.insert(30, Controller::Ai);

        let mut order = Vec::new();
        for _ in 0..6 {
            let actor = s.peek().unwrap();
            order.push(actor);
            let controller = if actor == 10 { Controller::Local(0) } else { Controller::Ai };
            s.advance(actor, controller, ALTERNATING_COST);
        }
        assert_eq!(order, vec![10, 20, 30, 10, 20, 30], "with uniform cost, every actor gets exactly one turn per round, in a stable order");
    }

    #[test]
    fn remove_drops_an_actor_and_it_never_comes_up_again() {
        let mut s = TurnScheduler::new();
        s.insert(1, Controller::Local(0));
        s.insert(2, Controller::Ai);
        s.remove(2);

        for _ in 0..4 {
            let actor = s.peek().unwrap();
            assert_eq!(actor, 1, "the removed actor must never be scheduled again");
            s.advance(actor, Controller::Local(0), ALTERNATING_COST);
        }
    }

    #[test]
    fn peek_on_an_empty_scheduler_returns_none() {
        let s = TurnScheduler::new();
        assert_eq!(s.peek(), None);
    }
}
