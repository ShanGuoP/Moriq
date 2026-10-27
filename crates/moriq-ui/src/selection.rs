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
    /// 清空批量选择与范围锚点，避免旧查询选择泄漏。
    pub fn clear(&mut self) {
        self.assets.clear();
        self.anchor = None;
        self.additive = false;
    }
    /// 切换多选模式，并按模式清理或保留有效选择。
    pub fn toggle_mode(&mut self) {
        self.enabled = !self.enabled;
        self.clear();
    }
    /// 切换指定素材的选择状态并更新范围锚点。
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
    /// 标记 Synco 轮询正在执行，并重置已等待的轮询计数。
    pub fn start(&mut self) {
        self.running = true;
        self.ticks = 0;
    }
    /// 结束 Synco 轮询；成功时恢复短等待，失败时增加有上限的退避间隔。
    pub fn finish(&mut self, success: bool) {
        self.running = false;
        self.ticks = 0;
        self.backoff = if success {
            8
        } else {
            (self.backoff.max(8) * 2).min(120)
        };
    }
    /// 推进轮询等待计数，判断当前是否空闲且已经达到退避间隔。
    pub fn poll_due(&mut self) -> bool {
        self.ticks = self.ticks.saturating_add(1);
        self.ticks >= self.backoff.max(8) && !self.running
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    /// 验证选择重置移除范围锚点，Synco 成功后重置轮询退避。
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
