// components/mod.rs — Component module root.
//
// This file declares all the sub-modules that live in the `components/`
// directory and re-exports their types so callers can write:
//
//   use ember2d::components::Transform;
//   use ember2d::components::Sprite;
//
// instead of the longer:
//
//   use ember2d::components::transform::Transform;
//
// THE COMPONENT IDEA IN A NUTSHELL:
//   Rather than a deep class hierarchy (Player extends Character extends Entity),
//   entities are just IDs, and components are bags of data attached to those IDs.
//
//   - Entity = a u64 number (the "primary key" of a game object)
//   - Component = pure data: no behavior, no logic, no methods beyond helpers
//   - System = logic that reads and writes components (movement, rendering, AI)
//
//   This is called the Entity-Component-System (ECS) pattern.
//   Our engine uses a simplified version without a full ECS framework.

pub mod collider;
pub mod script;
pub mod sprite;
pub mod tag;
pub mod transform;

// Re-export the most commonly used types at the `components` level.
pub use collider::Collider;
pub use script::Script;
pub use sprite::Sprite;
pub use tag::Tag;
pub use transform::Transform;
