use sysinfo::System;

#[derive(Clone, Debug)]
pub struct MemoryItem {
    pub cache: u64,
    pub used: u64,
    pub total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
}

impl MemoryItem {
    pub fn new(sys: &System) -> Self {
        let total = sys.total_memory();
        let used = total.saturating_sub(sys.available_memory());
        let used_plus_cache = total.saturating_sub(sys.free_memory());
        //TODO: this may not be completely accurate
        let cache = used_plus_cache.saturating_sub(used);
        Self {
            cache,
            used,
            total,
            swap_used: sys.used_swap(),
            swap_total: sys.total_swap(),
        }
    }
}
