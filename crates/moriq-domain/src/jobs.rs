//! Runtime protocols use typed values; SQLite keeps its existing string format.
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelId {
    ChineseClip,
    Florence,
    Faces,
    Qwen35_4b,
}
impl ModelId {
    /// 返回当前枚举的稳定协议值，供数据库和任务消息使用。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ChineseClip => "chinese-clip",
            Self::Florence => "florence",
            Self::Faces => "faces",
            Self::Qwen35_4b => "qwen35-4b",
        }
    }
}
impl FromStr for ModelId {
    type Err = &'static str;
    /// 从持久化协议值解析枚举，拒绝未知值以避免错误调度。
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "chinese-clip" => Ok(Self::ChineseClip),
            "florence" => Ok(Self::Florence),
            "faces" => Ok(Self::Faces),
            "qwen35-4b" => Ok(Self::Qwen35_4b),
            _ => Err("未登记的分析模型"),
        }
    }
}
impl fmt::Display for ModelId {
    /// 按类型约定输出协议值或可读错误说明。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobKind {
    Metadata,
    Integrity,
    Fingerprints,
    Thumbnails,
    Batch,
    Analysis(ModelId),
    QwenClassification,
    QwenDescription,
    QwenFaces,
}
impl JobKind {
    /// 返回当前枚举的稳定协议值，供数据库和任务消息使用。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::Integrity => "integrity",
            Self::Fingerprints => "fingerprints",
            Self::Thumbnails => "thumbnails",
            Self::Batch => "batch",
            Self::Analysis(ModelId::ChineseClip) => "analysis:chinese-clip",
            Self::Analysis(ModelId::Florence) => "analysis:florence",
            Self::Analysis(ModelId::Faces) => "analysis:faces",
            Self::Analysis(ModelId::Qwen35_4b) => "analysis:qwen35-4b",
            Self::QwenClassification => "analysis:qwen35-4b/classify",
            Self::QwenDescription => "analysis:qwen35-4b/describe",
            Self::QwenFaces => "analysis:qwen35-4b/faces",
        }
    }
    /// 返回该任务使用的模型身份；非模型任务按类型约定返回空值。
    pub const fn model(self) -> Option<ModelId> {
        match self {
            Self::Analysis(model) => Some(model),
            Self::QwenClassification | Self::QwenDescription | Self::QwenFaces => {
                Some(ModelId::Qwen35_4b)
            }
            _ => None,
        }
    }
    /// 返回该任务种类的共享内存准入预算。
    pub const fn memory_mib(self) -> u32 {
        match self {
            Self::Analysis(ModelId::Qwen35_4b) => 3072,
            Self::Analysis(_) => 3072,
            Self::QwenClassification | Self::QwenDescription | Self::QwenFaces => 3072,
            Self::Batch | Self::Metadata | Self::Integrity => 128,
            Self::Fingerprints | Self::Thumbnails => 1024,
        }
    }
}
impl FromStr for JobKind {
    type Err = &'static str;
    /// 从持久化协议值解析枚举，拒绝未知值以避免错误调度。
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "metadata" => Ok(Self::Metadata),
            "integrity" => Ok(Self::Integrity),
            "fingerprints" => Ok(Self::Fingerprints),
            "thumbnails" => Ok(Self::Thumbnails),
            "batch" => Ok(Self::Batch),
            "analysis:qwen35-4b/classify" => Ok(Self::QwenClassification),
            "analysis:qwen35-4b/describe" => Ok(Self::QwenDescription),
            "analysis:qwen35-4b/faces" => Ok(Self::QwenFaces),
            value => value
                .strip_prefix("analysis:")
                .ok_or("后台任务类型无效")?
                .parse()
                .map(Self::Analysis),
        }
    }
}
impl fmt::Display for JobKind {
    /// 按类型约定输出协议值或可读错误说明。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Running,
    Paused,
    Completed,
    Cancelled,
}
impl JobState {
    /// 返回当前枚举的稳定协议值，供数据库和任务消息使用。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }
    /// 判断任务是否处于需要调度或展示活动状态的阶段。
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::Paused)
    }
    /// 判断任务是否已经完成或终止。
    pub const fn is_finished(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
}
impl FromStr for JobState {
    type Err = &'static str;
    /// 从持久化协议值解析枚举，拒绝未知值以避免错误调度。
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "paused" => Ok(Self::Paused),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err("后台任务状态无效"),
        }
    }
}
impl fmt::Display for JobState {
    /// 按类型约定输出协议值或可读错误说明。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskError {
    Cancelled,
    StaleLibrary,
    InvalidInput(String),
    Storage(String),
    Runtime(String),
    Unavailable,
}
impl fmt::Display for TaskError {
    /// 按类型约定输出协议值或可读错误说明。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("任务已停止"),
            Self::StaleLibrary => f.write_str("任务所属资源库已切换"),
            Self::InvalidInput(s) | Self::Storage(s) | Self::Runtime(s) => f.write_str(s),
            Self::Unavailable => f.write_str("后台服务已关闭"),
        }
    }
}
impl std::error::Error for TaskError {}
pub type TaskResult<T> = Result<T, TaskError>;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    /// 验证任务和模型协议往返解析一致，未知值不会被静默接受。
    fn persisted_protocol_roundtrips_and_unknown_values_are_rejected() {
        for kind in [
            JobKind::Metadata,
            JobKind::Integrity,
            JobKind::Fingerprints,
            JobKind::Thumbnails,
            JobKind::Batch,
            JobKind::Analysis(ModelId::ChineseClip),
            JobKind::Analysis(ModelId::Florence),
            JobKind::Analysis(ModelId::Faces),
            JobKind::QwenClassification,
            JobKind::QwenDescription,
        ] {
            assert_eq!(kind.as_str().parse(), Ok(kind));
            assert!((1..=8192).contains(&kind.memory_mib()));
        }
        for state in [
            JobState::Queued,
            JobState::Running,
            JobState::Paused,
            JobState::Completed,
            JobState::Cancelled,
        ] {
            assert_eq!(state.as_str().parse(), Ok(state));
            assert_ne!(state.is_active(), state.is_finished());
        }
        for invalid in ["", "analysis:", "analysis:future-model", "Metadata"] {
            assert!(invalid.parse::<JobKind>().is_err());
        }
        assert!("pending".parse::<JobState>().is_err());
    }
}
