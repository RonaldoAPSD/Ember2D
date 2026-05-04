// components/tag.rs — Name and category label for entities.
//
// A Tag is a string label attached to an entity to describe "what it is."
//
// WHY NOT JUST USE THE ENTITY ID?
//   Entity IDs are opaque integers — entity 42 tells you nothing.
//   A tag says "player", "wall", "pickup", "enemy" — human-readable and
//   meaningful to game logic.
//
// USES:
//   - Finding a specific entity: `world.find_by_tag("player")`
//   - Filtering events: "did the player collide with a 'wall'?"
//   - Game rules: "coins disappear when tagged 'pickup' is collected"
//
// A real engine might support MULTIPLE tags per entity (a Vec<String>),
// but a single tag covers most use cases and keeps the API simple.

/// A human-readable label for an entity.
#[derive(Debug, Clone)]
pub struct Tag {
    /// The label string. Common values: "player", "wall", "floor",
    /// "enemy", "pickup", "trigger", "projectile".
    pub name: String,
}

impl Tag {
    /// Create a Tag with the given name.
    ///
    /// The `impl Into<String>` parameter means you can pass either a
    /// `String` or a `&str` — Rust will convert it automatically:
    ///   Tag::new("player")          // &str — most common
    ///   Tag::new(String::from("x")) // String
    pub fn new(name: impl Into<String>) -> Self {
        Tag { name: name.into() }
    }

    /// Check if this tag matches a given string.
    pub fn is(&self, name: &str) -> bool {
        self.name == name
    }
}
