//! Selection owns its anchor and mode so resets cannot leave a stale range origin.
use super::*;
#[derive(Default)]
pub(super) struct SelectionState {
    pub assets: std::collections::HashSet<AssetId>,
    pub enabled: bool,
    pub additive: bool,
    pub anchor: Option<AssetId>,
}
impl SelectionState {
    pub fn clear(&mut self) {
        self.assets.clear();
        self.anchor = None;
        self.additive = false;
    }
    pub fn toggle_mode(&mut self) {
        self.enabled = !self.enabled;
        self.clear();
    }
    pub fn toggle(&mut self, id: AssetId) {
        self.anchor = Some(id);
        if !self.assets.remove(&id) {
            self.assets.insert(id);
        }
    }
}
#[derive(Default)]
pub(super) struct SyncoState {
    pub running: bool,
    ticks: u16,
    backoff: u16,
}
impl SyncoState {
    pub fn start(&mut self) {
        self.running = true;
        self.ticks = 0;
    }
    pub fn finish(&mut self, success: bool) {
        self.running = false;
        self.ticks = 0;
        self.backoff = if success {
            8
        } else {
            (self.backoff.max(8) * 2).min(120)
        };
    }
    pub fn poll_due(&mut self) -> bool {
        self.ticks = self.ticks.saturating_add(1);
        self.ticks >= self.backoff.max(8) && !self.running
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resetting_selection_removes_range_anchor_and_synco_backoff_resets_on_success() {
        let mut selection = SelectionState::default();
        selection.toggle_mode();
        let id = AssetId::default();
        selection.toggle(id);
        assert_eq!(selection.anchor, Some(id));
        assert!(selection.assets.contains(&id));
        selection.clear();
        assert!(selection.anchor.is_none());
        assert!(selection.assets.is_empty());
        assert!(selection.enabled);
        let mut synco = SyncoState::default();
        for _ in 0..7 {
            assert!(!synco.poll_due());
        }
        assert!(synco.poll_due());
        synco.start();
        assert!(!synco.poll_due());
        synco.finish(false);
        assert_eq!(synco.backoff, 16);
        synco.finish(true);
        assert_eq!(synco.backoff, 8);
        assert_eq!(synco.ticks, 0);
    }
}
