// layers.rs — LayerRegistry: the name<->bit table collision-layer filtering
// runs on internally (Phase 6 Step 7, docs/ember2d-phase6-plan.md).
//
// ── WHY THIS EXISTS ──────────────────────────────────────────────────────────
//
// `Collider.layer`/`.mask` are authored, saved, and script-visible as
// strings — `.level` files, the editor, `ctx.get_collider_layer`/
// `set_collider_layer`, node-graph codegen, all unchanged by this. But
// `World::detect_collisions`'s O(colliders²) pairwise test used to compare
// those strings directly (a `String` equality/`Vec<String>::contains` per
// pair), which meant every collidable entity's layer/mask had to be cloned
// out of `World` into a local `Vec` first — 1,704 `String` + 1,704
// `Vec<String>` clones per step at floor2 scale, on top of the string
// comparisons themselves. A `u32` bitmask makes the comparison a single AND
// and the whole `collidables` list `Copy`, at the cost of needing a
// name->bit table somewhere — this module is that table.
//
// ── WHY A REGISTRY, NOT JUST HASHING THE STRING ──────────────────────────────
//
// Hashing a layer name into a bit position would be simpler (no table to
// build or store) but nondeterministic in the one way that matters here: two
// different layer names could hash to the same bit (a collision, however
// rare), and which names collide depends on the hash function's exact
// behavior — a portability and determinism hazard for the same reason
// `ember2d-sim` avoids transcendental math (§5.2 H2,
// docs/ember2d-refactor-plan.md). Bit assignment by REGISTRATION ORDER
// instead — first name gets bit 0, second gets bit 1, and so on — is exact
// and reproducible: `LevelData.collision_layers` is itself a serialized
// `Vec<String>`, so its order is authored data, not runtime happenstance,
// and the same list always produces the same bits on every machine.
//
// ── WHY BUILT ONCE AT LOAD, NEVER GROWN AT RUNTIME ───────────────────────────
//
// A registry that assigned a fresh bit to a layer name the first time a
// script mentioned it would make bit assignment depend on SCRIPT EXECUTION
// ORDER — which entity's `on_update` happens to run first, which itself can
// depend on iteration order, timing, or (for two clients under future
// lockstep netcode, Phase 9) which machine got there first. That's exactly
// the class of desync this crate's whole determinism discipline exists to
// prevent. Building the registry once, from `LevelData.collision_layers`,
// before any script runs, removes the question entirely: the table is fixed
// before anything can observe it.

use std::collections::HashMap;

/// Bit 31 — reserved, never assigned to a real layer. See `mask_bits`'s doc
/// comment for the one case this is actually used: a *mask* naming a layer
/// the registry doesn't know about.
pub const LAYER_UNKNOWN: u32 = 1 << 31;

/// Bits 0..=30 are assignable to real layers — one short of `u32`'s 32 bits,
/// bit 31 being reserved above.
const MAX_LAYERS: usize = 31;

/// The name<->bit table for one loaded level. Immutable after construction
/// (see this module's header comment for why) — `Clone` is cheap (a small
/// `HashMap`, at most 31 entries) and used to give `Simulation` and
/// `ScriptEngine` each their own independent copy rather than sharing one
/// behind a reference that would otherwise have to be threaded through
/// every `WorldSnapshot`/`ScriptState` constructor.
#[derive(Debug, Clone, Default)]
pub struct LayerRegistry {
    bits: HashMap<String, u32>,
}

impl LayerRegistry {
    /// Build from `LevelData.collision_layers`, in order — the Nth name
    /// (0-indexed) gets bit N. A name repeated later in the list keeps its
    /// first bit rather than being reassigned one (`HashMap::entry` +
    /// `or_insert`, not `insert`) — an authoring mistake, not a reason to
    /// silently drop the earlier bit's meaning. Names past the 31st are
    /// dropped: nothing in this engine's content authors anywhere near that
    /// many layers, and a name that fails to register resolves to
    /// `LAYER_UNKNOWN` when used in a mask (never a silent "matches
    /// everything") or `0` when used as a collider's own layer — both
    /// documented on the methods below.
    pub fn new(names: &[String]) -> Self {
        let mut bits = HashMap::new();
        for (i, name) in names.iter().enumerate().take(MAX_LAYERS) {
            bits.entry(name.clone()).or_insert(1u32 << i);
        }
        LayerRegistry { bits }
    }

    /// A single layer name's bit — what a `Collider`'s OWN `layer` resolves
    /// to. `0` for an empty name or one the registry doesn't recognize.
    ///
    /// Deliberately `0`, not `LAYER_UNKNOWN`, unlike `mask_bits` below —
    /// `0` is also every empty mask's "matches everything" value, so an
    /// entity with no meaningful layer of its own still gets excluded by
    /// any mask that's actually filtering (the AND against a nonzero mask
    /// is `0` either way) while still being caught by an empty ("matches
    /// everything") one, exactly matching the old string behavior where an
    /// empty layer name was never explicitly listed in anyone's mask.
    pub fn bit_for(&self, name: &str) -> u32 {
        if name.is_empty() { return 0; }
        self.bits.get(name).copied().unwrap_or(0)
    }

    /// A mask's resolved bits — what a `Collider`'s `mask` (or a script's
    /// `raycast`/`get_path` mask argument) resolves to. An empty mask
    /// resolves to `0`, matching `bit_for`'s "matches everything" encoding
    /// and the old `Vec::is_empty()` check this replaces directly.
    ///
    /// A name the registry doesn't recognize ORs in `LAYER_UNKNOWN` instead
    /// of contributing `0` — the asymmetry with `bit_for` above is
    /// deliberate, not an oversight: resolving an unregistered mask entry
    /// to `0` would silently turn "filter out everything except this
    /// specific (mistyped, or authored-before-being-registered) layer" into
    /// "matches everything," the opposite of what the mask asked for and a
    /// far worse failure mode than matching nothing. `LAYER_UNKNOWN` never
    /// collides with a real layer's bit (it's never assigned to one by
    /// `new` above) and nothing sets it as a collider's own `layer_bits`,
    /// so it can only ever appear here, in a mask — meaning it can only
    /// ever cause an unregistered-layer mask to match nothing, never
    /// something real.
    pub fn mask_bits(&self, names: &[String]) -> u32 {
        if names.is_empty() { return 0; }
        names.iter().fold(0u32, |acc, name| acc | self.bits.get(name).copied().unwrap_or(LAYER_UNKNOWN))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_get_bits_in_registration_order() {
        let reg = LayerRegistry::new(&["solid".to_string(), "enemy".to_string(), "pickup".to_string()]);
        assert_eq!(reg.bit_for("solid"), 1 << 0);
        assert_eq!(reg.bit_for("enemy"), 1 << 1);
        assert_eq!(reg.bit_for("pickup"), 1 << 2);
    }

    #[test]
    fn a_repeated_name_keeps_its_first_bit() {
        let reg = LayerRegistry::new(&["solid".to_string(), "enemy".to_string(), "solid".to_string()]);
        assert_eq!(reg.bit_for("solid"), 1 << 0, "the second 'solid' entry must not steal or overwrite the first bit");
        assert_eq!(reg.bit_for("enemy"), 1 << 1);
    }

    #[test]
    fn an_empty_or_unregistered_layer_name_resolves_to_zero() {
        let reg = LayerRegistry::new(&["solid".to_string()]);
        assert_eq!(reg.bit_for(""), 0);
        assert_eq!(reg.bit_for("no_such_layer"), 0);
    }

    #[test]
    fn an_empty_mask_resolves_to_zero_matching_everything() {
        let reg = LayerRegistry::new(&["solid".to_string()]);
        assert_eq!(reg.mask_bits(&[]), 0);
    }

    #[test]
    fn a_mask_naming_an_unregistered_layer_ors_in_layer_unknown_not_zero() {
        let reg = LayerRegistry::new(&["solid".to_string()]);
        let bits = reg.mask_bits(&["no_such_layer".to_string()]);
        assert_ne!(bits, 0, "an unregistered mask entry must not silently resolve to \"matches everything\"");
        assert_eq!(bits, LAYER_UNKNOWN);
    }

    #[test]
    fn a_mask_mixing_a_registered_and_unregistered_name_ors_both_in() {
        let reg = LayerRegistry::new(&["solid".to_string()]);
        let bits = reg.mask_bits(&["solid".to_string(), "no_such_layer".to_string()]);
        assert_eq!(bits, (1 << 0) | LAYER_UNKNOWN);
    }

    #[test]
    fn more_than_max_layers_drops_the_overflow_rather_than_panicking() {
        let names: Vec<String> = (0..40).map(|i| format!("layer{}", i)).collect();
        let reg = LayerRegistry::new(&names);
        assert_eq!(reg.bit_for("layer0"), 1 << 0);
        assert_eq!(reg.bit_for("layer30"), 1 << 30, "the 31st name (index 30) is the last real bit");
        assert_eq!(reg.bit_for("layer31"), 0, "the 32nd name and beyond must not register at all");
    }
}
