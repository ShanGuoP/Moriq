//! Export state owns typed option changes; translated labels never become protocol values.
use super::*;
#[derive(Default)]
pub(super) struct ExportState {
    pub options: ExportOptions,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportChange {
    Format(ExportFormat),
    MaxEdge(u32),
    Quality(u8),
    Rotate(u16),
    Resampling(Resampling),
    Layout(ExportLayout),
    Metadata(bool),
    PreserveExif(bool),
    Template(String),
}
impl ExportState {
    /// 将导出表单选项应用到类型化导出配置。
    pub fn apply(&mut self, change: ExportChange) {
        let o = &mut self.options;
        match change {
            ExportChange::Format(v) => o.format = v,
            ExportChange::MaxEdge(v) => o.max_edge = v,
            ExportChange::Quality(v) => o.quality = v,
            ExportChange::Rotate(v) => o.rotate = v,
            ExportChange::Resampling(v) => o.resampling = v,
            ExportChange::Layout(v) => o.layout = v,
            ExportChange::Metadata(v) => o.metadata_sidecar = v,
            ExportChange::PreserveExif(v) => o.preserve_exif = v,
            ExportChange::Template(v) => o.directory_template = v,
        }
    }
}
impl std::fmt::Display for ExportChange {
    /// 输出导出选项的显示标签。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Format(ExportFormat::Original) => "原文件",
            Self::Format(ExportFormat::Jpeg) => "JPEG",
            Self::Format(ExportFormat::Png) => "PNG",
            Self::Format(ExportFormat::Webp) => "WebP",
            Self::Format(ExportFormat::Tiff) => "TIFF",
            Self::Format(ExportFormat::Bmp) => "BMP",
            Self::MaxEdge(v) => return write!(f, "{v}"),
            Self::Quality(v) => return write!(f, "{v}"),
            Self::Rotate(v) => return write!(f, "{v}"),
            Self::Resampling(Resampling::Area) => "面积平均",
            Self::Resampling(Resampling::Lanczos3) => "Lanczos3",
            Self::Resampling(Resampling::Bicubic) => "双三次",
            Self::Resampling(Resampling::Nearest) => "最近邻",
            Self::Layout(ExportLayout::Flat) => "平铺",
            Self::Layout(ExportLayout::Format) => "按格式",
            Self::Layout(ExportLayout::ImportDate) => "按导入日期",
            Self::Metadata(true) => "附带整理信息 JSON",
            Self::Metadata(false) => "不附带",
            Self::PreserveExif(v) => return write!(f, "{v}"),
            Self::Template(v) => v,
        })
    }
}
/// 构建导出格式、尺寸和目录等选项行。
pub(super) fn rows(o: &ExportOptions) -> Vec<(&'static str, Vec<ExportChange>, ExportChange)> {
    use ExportChange::*;
    vec![
        (
            "格式",
            [
                ExportFormat::Original,
                ExportFormat::Jpeg,
                ExportFormat::Png,
                ExportFormat::Webp,
                ExportFormat::Tiff,
                ExportFormat::Bmp,
            ]
            .map(Format)
            .to_vec(),
            Format(o.format),
        ),
        (
            "最长边 / 像素",
            [0, 1280, 1920, 2560, 3840].map(MaxEdge).to_vec(),
            MaxEdge(o.max_edge),
        ),
        (
            "JPEG 质量",
            [70, 80, 90, 95, 100].map(Quality).to_vec(),
            Quality(o.quality),
        ),
        (
            "顺时针旋转",
            [0, 90, 180, 270].map(Rotate).to_vec(),
            Rotate(o.rotate),
        ),
        (
            "重采样",
            [
                moriq_domain::Resampling::Area,
                moriq_domain::Resampling::Lanczos3,
                moriq_domain::Resampling::Bicubic,
                moriq_domain::Resampling::Nearest,
            ]
            .map(ExportChange::Resampling)
            .to_vec(),
            ExportChange::Resampling(o.resampling),
        ),
        (
            "目录布局",
            [
                ExportLayout::Flat,
                ExportLayout::Format,
                ExportLayout::ImportDate,
            ]
            .map(Layout)
            .to_vec(),
            Layout(o.layout),
        ),
        (
            "整理信息",
            [false, true].map(Metadata).to_vec(),
            Metadata(o.metadata_sidecar),
        ),
    ]
}
