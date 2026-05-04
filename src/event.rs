// event.rs — The game event bus.
//
// An EVENT BUS (also called an event queue or message system) lets different
// parts of the game communicate without directly calling each other.
//
// WITHOUT an event bus:
//   The collision system would need a direct reference to the health system
//   to say "entity 5 took damage". Every system would become tangled with
//   every other system — a maintenance nightmare.
//
// WITH an event bus:
//   The collision system emits:  Collision { entity_a: 5, entity_b: 12 }
//   The health system reads all events, finds the collision, deals damage.
//   Neither system knows the other exists. They only share the event types.
//
// HOW OUR EVENT BUS WORKS:
//   - Events are emitted (pushed) into a Vec during a frame.
//   - Any system can read all events by calling events().
//   - At the start of the next frame, the engine clears the queue.
//   - Events only live for ONE frame — they're not persisted.
//
// This "fire-and-forget" model is simple and avoids memory leaks.

use crate::world::EntityId;

/// All the kinds of events that can occur in an ember2d game.
///
/// Add your own variants here as your game grows. The `#[non_exhaustive]`
/// attribute could be used in a production library to allow adding variants
/// without breaking downstream code — we omit it here for simplicity.
#[derive(Debug, Clone)]
pub enum GameEvent {
    /// Two entities' bounding boxes overlapped this frame.
    ///
    /// The engine emits this automatically after collision detection.
    /// Note: the order of entity_a / entity_b is NOT guaranteed.
    /// Always check both orderings when looking for a specific entity:
    ///   if *a == player_id || *b == player_id { ... }
    Collision { entity_a: EntityId, entity_b: EntityId },

    /// An entity was destroyed via world.despawn().
    /// Useful for systems that need to clean up references to the entity.
    EntityDestroyed { entity_id: EntityId },

    /// A free-form string event for game-specific messages.
    ///
    /// Useful for prototyping or one-off events that don't warrant a new variant.
    /// Example: events.emit(GameEvent::Custom("level_complete".into()))
    Custom(String),
}

/// The event queue: collects events during a frame for other systems to read.
pub struct EventBus {
    queue: Vec<GameEvent>,
}

impl EventBus {
    /// Create an empty event bus.
    pub fn new() -> Self {
        EventBus { queue: Vec::new() }
    }

    /// Push an event into the queue so other systems can read it this frame.
    ///
    /// The event is cloneable so multiple systems can consume it without moves.
    pub fn emit(&mut self, event: GameEvent) {
        self.queue.push(event);
    }

    /// Read all events emitted so far this frame.
    ///
    /// Returns a slice (view into the Vec) — callers iterate over it but
    /// cannot modify the queue. This prevents systems from accidentally
    /// clearing or reordering events mid-frame.
    pub fn events(&self) -> &[GameEvent] {
        &self.queue
    }

    /// Remove all events. Called by the engine at the start of each frame.
    /// Game code should never need to call this directly.
    pub fn clear(&mut self) {
        self.queue.clear();
    }

    /// How many events are in the queue this frame.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// True if no events have been emitted this frame.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}
