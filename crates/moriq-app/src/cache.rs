use super::*;
use std::{collections::HashMap, sync::Mutex};
type Key = (LibraryId, AssetVersionId, u32);
#[derive(Default)]
pub(super) struct MediaCache {
    state: Mutex<State>,
}
#[derive(Default)]
struct State {
    entries: HashMap<Key, (moriq_library::media::Preview, u64)>,
    clock: u64,
    bytes: usize,
}
/// 计算一份预览及其背景像素占用的内存字节数。
fn bytes(value: &moriq_library::media::Preview) -> usize {
    value.image.rgba.len() + value.background.rgba.len()
}
impl MediaCache {
    /// 读取指定库、版本和尺寸的预览缓存，并更新最近使用顺序。
    pub fn get(&self, key: Key) -> Option<moriq_library::media::Preview> {
        let mut state = self.state.lock().ok()?;
        state.clock = state.clock.wrapping_add(1);
        let clock = state.clock;
        state.entries.get_mut(&key).map(|(value, age)| {
            *age = clock;
            value.clone()
        })
    }
    /// 在条目数和总字节预算内放入预览，按使用顺序淘汰旧项。
    pub fn put(&self, key: Key, value: moriq_library::media::Preview) {
        let size = bytes(&value);
        if size > 128 * 1024 * 1024 {
            return;
        }
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if let Some((old, _)) = state.entries.remove(&key) {
            state.bytes = state.bytes.saturating_sub(bytes(&old))
        }
        while state.bytes + size > 256 * 1024 * 1024 || state.entries.len() >= 48 {
            let Some(oldest) = state
                .entries
                .iter()
                .min_by_key(|(_, (_, age))| *age)
                .map(|(key, _)| *key)
            else {
                break;
            };
            if let Some((old, _)) = state.entries.remove(&oldest) {
                state.bytes = state.bytes.saturating_sub(bytes(&old));
            }
        }
        state.clock = state.clock.wrapping_add(1);
        let age = state.clock;
        state.bytes += size;
        state.entries.insert(key, (value, age));
    }
    /// 清空内存预览缓存，避免资源库切换后保留旧素材像素。
    pub fn clear(&self) {
        if let Ok(mut state) = self.state.lock() {
            *state = State::default();
        }
    }
}
