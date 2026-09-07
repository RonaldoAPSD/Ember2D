// renderer/texture_budget.rs — R26 (7B-3, docs/ember2d-master-plan.md
// §5.2): approximate GPU byte accounting and least-recently-used order for
// `WgpuBackend::texture_cache` — split into its own file (not just its own
// type in backend.rs) once adding it pushed that file past the project's
// 750-line hard limit (CLAUDE.md). A plain data structure, deliberately
// independent of `wgpu::BindGroup` (which has no size of its own to
// query), so the eviction *decision* is unit-testable without a live GPU
// device — the same reasoning `screen_cell_to_pixel`/`compute_layout`
// (renderer/mod.rs) already use for GPU-adjacent math.
// `WgpuBackend::upload_texture` (backend.rs) is the only real caller: it
// inserts on every fresh upload and removes whatever this reports as
// evicted from the real `texture_cache` too.

use std::collections::{HashMap, VecDeque};

pub(super) struct TextureBudget {
    /// Tracked size in bytes, per texture id.
    bytes: HashMap<u64, usize>,
    /// Least-recently-used order — front is the next eviction candidate.
    lru: VecDeque<u64>,
    budget_bytes: usize,
}

impl TextureBudget {
    pub(super) fn new(budget_bytes: usize) -> Self {
        TextureBudget { bytes: HashMap::new(), lru: VecDeque::new(), budget_bytes }
    }

    pub(super) fn total_bytes(&self) -> usize {
        self.bytes.values().sum()
    }

    /// Move `id` to the most-recently-used end without changing its
    /// tracked size — called on every cache hit, not just first upload, so
    /// a texture drawn every frame never looks "least recently used" just
    /// because it was cached long ago.
    pub(super) fn touch(&mut self, id: u64) {
        if let Some(pos) = self.lru.iter().position(|&x| x == id) {
            self.lru.remove(pos);
        }
        self.lru.push_back(id);
    }

    /// Record a fresh upload of `id` at `size_bytes` and evict
    /// least-recently-used entries (oldest first) until back under
    /// budget. Returns the evicted ids — the caller (`upload_texture`)
    /// still has to remove them from the real `texture_cache`, this type
    /// has no reference to it. Never evicts down to zero tracked
    /// textures — `id` itself (just inserted, now most-recently-used) is
    /// always safe.
    pub(super) fn insert(&mut self, id: u64, size_bytes: usize) -> Vec<u64> {
        self.bytes.insert(id, size_bytes);
        self.touch(id);
        let mut evicted = Vec::new();
        while self.total_bytes() > self.budget_bytes && self.lru.len() > 1 {
            if let Some(oldest) = self.lru.pop_front() {
                self.bytes.remove(&oldest);
                evicted.push(oldest);
            }
        }
        evicted
    }

    pub(super) fn remove(&mut self, id: u64) {
        self.bytes.remove(&id);
        if let Some(pos) = self.lru.iter().position(|&x| x == id) {
            self.lru.remove(pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TextureBudget;

    #[test]
    fn insert_evicts_nothing_while_under_budget() {
        let mut b = TextureBudget::new(1000);
        assert_eq!(b.insert(1, 400), Vec::<u64>::new());
        assert_eq!(b.insert(2, 400), Vec::<u64>::new());
        assert_eq!(b.total_bytes(), 800);
    }

    #[test]
    fn insert_evicts_the_least_recently_used_id_first() {
        let mut b = TextureBudget::new(1000);
        b.insert(1, 400);
        b.insert(2, 400);
        // Pushes total to 1200 — over budget by 200 — id 1 (inserted
        // first, never touched again) is the oldest.
        let evicted = b.insert(3, 400);
        assert_eq!(evicted, vec![1]);
        assert_eq!(b.total_bytes(), 800);
    }

    #[test]
    fn touch_protects_a_texture_thats_still_being_drawn_every_frame() {
        let mut b = TextureBudget::new(1000);
        b.insert(1, 400);
        b.insert(2, 400);
        b.touch(1); // id 1 drawn again — now more-recently-used than id 2
        let evicted = b.insert(3, 400);
        assert_eq!(evicted, vec![2], "id 2, not the touched id 1, should be evicted");
    }

    #[test]
    fn insert_never_evicts_down_to_zero_tracked_textures() {
        let mut b = TextureBudget::new(100);
        // A single texture larger than the whole budget must not evict
        // itself — there would be nothing left to draw with.
        let evicted = b.insert(1, 500);
        assert!(evicted.is_empty());
        assert_eq!(b.total_bytes(), 500);
    }

    #[test]
    fn remove_drops_an_id_from_both_size_tracking_and_lru_order() {
        let mut b = TextureBudget::new(1000);
        b.insert(1, 400);
        b.remove(1);
        assert_eq!(b.total_bytes(), 0);
        // A removed id must not still count as an eviction candidate.
        let evicted = b.insert(2, 900);
        assert_eq!(evicted, Vec::<u64>::new());
    }
}
