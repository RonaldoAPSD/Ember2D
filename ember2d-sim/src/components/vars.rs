// components/vars.rs — Vars: arbitrary per-entity script state.
//
// Step 7.5-3 (docs/ember2d-master-plan.md §5.6): a real ECS component for
// the "fake per-entity state" pattern the roguelike/shooter demo scripts
// were using globals for — a key string-concatenated with the entity id
// (`"hp_" + id`, `"aware_" + id`, `"ehp_" + id`) to fake per-entity scoping
// out of a level-scoped, flat `BTreeMap<String, Dynamic>`. That worked, but
// it's a level-scoped global masquerading as per-entity state: it never
// gets cleared when the entity despawns (`enemy_rat.rhai`'s own "ehp_<id>"
// dance in `director.rhai`'s `resolve_hits` has to `remove_global` by
// hand), it survives past that entity's lifetime by construction, and
// nothing stops one entity's key from colliding with another's if the
// naming convention is ever gotten wrong. `Vars` fixes all three by being
// real per-entity component data instead of a naming convention.
//
// No `SaveState` plumbing needed the way `globals`/`persistent` require
// (see `save.rs`'s own `SaveState::globals`/`persistent` fields) — `Vars`
// lives directly on `World` alongside `Transform`/`Sprite`/`Tag`/etc., and
// `SaveState.world: World` already serializes all of those for free via
// `derive(Serialize, Deserialize)`. `#[serde(default)]` on `World::vars`
// (see world.rs) is what lets a save from before this component existed
// still load, as if every entity's `Vars` were empty — the same fallback
// `animators`/`actors` already established for their own additions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Arbitrary script-set key/value state scoped to ONE entity, backing
/// `ctx.set_var`/`get_var`/`has_var`/`remove_var` (`api_ext.rs`). Cleared
/// automatically on despawn (`World::despawn`) — unlike the `"hp_" + id`
/// global convention it replaces, there is no key left behind to leak into
/// a future entity that happens to reuse the same id space (entity ids
/// never actually get reused within one `World`'s lifetime — `next_id`
/// only increments — but the old convention relied on every script
/// remembering to `remove_global` on death to avoid the level's globals
/// map growing by one stale key per kill for the run; see
/// `demos/shooter/scripts/director.rhai`'s `resolve_hits`, which used to
/// have to do exactly that by hand).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Vars {
    pub values: BTreeMap<String, rhai::Dynamic>,
}
