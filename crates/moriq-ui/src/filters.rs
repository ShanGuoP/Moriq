//! Typed filter choices are independent of the displayed language.
use super::*;

#[derive(Default)]
pub(super) struct FilterDraft {
    pub query: Query,
    pub date_start: String,
    pub date_end: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    Recent,
    Untagged,
    HighRating,
    Trash,
    Unassigned,
    Edited,
    Raw,
    Video,
}
impl Builtin {
    /// 从当前筛选草稿构造素材查询。
    pub fn query(self, now: u64) -> Query {
        let mut q = Query::default();
        match self {
            Self::Recent => q.since = now.saturating_sub(7 * 86400000),
            Self::Untagged => q.untagged = true,
            Self::HighRating => q.min_rating = 4,
            Self::Trash => q.trash = true,
            Self::Unassigned => q.unassigned = true,
            Self::Edited => q.edited_since = now.saturating_sub(7 * 86400000),
            Self::Raw => q.media_kind = MediaKind::Raw,
            Self::Video => q.media_kind = MediaKind::Video,
        }
        q
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaChoice {
    All,
    Raw,
    Video,
    Format(&'static str),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    All,
    Week,
    Month,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeChoice {
    All,
    Small,
    Large,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    All,
    Landscape,
    Portrait,
    Square,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageChoice {
    All,
    Managed,
    Referenced,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    All,
    Black,
    White,
    Gray,
    Red,
    Orange,
    Yellow,
    Green,
    Cyan,
    Blue,
    Purple,
    Pink,
}
impl ColorChoice {
    /// 取得指定筛选字段当前的类型化值。
    fn value(self) -> &'static str {
        match self {
            Self::All => "",
            Self::Black => "黑",
            Self::White => "白",
            Self::Gray => "灰",
            Self::Red => "红",
            Self::Orange => "橙",
            Self::Yellow => "黄",
            Self::Green => "绿",
            Self::Cyan => "青",
            Self::Blue => "蓝",
            Self::Purple => "紫",
            Self::Pink => "粉",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterChange {
    Medium(&'static str),
    Theme(&'static str),
    Media(MediaChoice),
    Rating(u8),
    Imported(Period),
    Size(SizeChoice),
    Orientation(Orientation),
    Sort(SortOrder),
    MinWidth(u32),
    Color(ColorChoice),
    Storage(StorageChoice),
}
impl FilterChange {
    /// 把筛选选项写入查询或草稿，保留其他条件。
    pub fn apply(self, q: &mut Query, now: u64) {
        match self {
            Self::Medium(id) => {
                q.medium = id.into();
                q.category.clear();
            }
            Self::Theme(id) => {
                q.category.clear();
                if q.themes.iter().any(|s| s == id) {
                    q.themes.retain(|s| s != id);
                } else {
                    q.themes.push(id.into());
                }
            }
            Self::Media(choice) => {
                q.media_kind = match choice {
                    MediaChoice::Raw => MediaKind::Raw,
                    MediaChoice::Video => MediaKind::Video,
                    _ => MediaKind::All,
                };
                q.format = match choice {
                    MediaChoice::Format(format) => format.to_lowercase(),
                    _ => String::new(),
                };
            }
            Self::Rating(rating) => q.min_rating = rating.min(5),
            Self::Imported(period) => {
                q.since = match period {
                    Period::All => 0,
                    Period::Week => now.saturating_sub(7 * 86400000),
                    Period::Month => now.saturating_sub(30 * 86400000),
                }
            }
            Self::Size(size) => {
                q.min_bytes = if size == SizeChoice::Large {
                    10_000_000
                } else {
                    0
                };
                q.max_bytes = if size == SizeChoice::Small {
                    1_000_000
                } else {
                    0
                };
            }
            Self::Orientation(value) => {
                q.orientation = match value {
                    Orientation::All => 0,
                    Orientation::Landscape => 1,
                    Orientation::Portrait => 2,
                    Orientation::Square => 3,
                }
            }
            Self::Sort(order) => q.sort = order,
            Self::MinWidth(width) => q.min_width = width,
            Self::Color(color) => q.color = color.value().into(),
            Self::Storage(value) => {
                q.storage = match value {
                    StorageChoice::All => 0,
                    StorageChoice::Managed => 1,
                    StorageChoice::Referenced => 2,
                }
            }
        }
    }
    /// 判断某个筛选选项是否为当前选中值。
    fn selected(self, q: &Query, now: u64) -> bool {
        match self {
            Self::Imported(period) => match period {
                Period::All => q.since == 0,
                Period::Week => q.since > 0 && now.saturating_sub(q.since) < 14 * 86400000,
                Period::Month => q.since > 0 && now.saturating_sub(q.since) >= 14 * 86400000,
            },
            _ => {
                let mut value = q.clone();
                self.apply(&mut value, now);
                value == *q
            }
        }
    }
}
impl std::fmt::Display for FilterChange {
    /// 格式化筛选选项的显示标签。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match *self {
            Self::Medium("") => "全部媒介",
            Self::Medium(id) | Self::Theme(id) => ai_label(id),
            Self::Media(MediaChoice::All) => "全部类型",
            Self::Media(MediaChoice::Raw) => "RAW",
            Self::Media(MediaChoice::Video) => "视频",
            Self::Media(MediaChoice::Format(format)) => format,
            Self::Rating(value) => return write!(f, "{value}"),
            Self::MinWidth(value) => return write!(f, "{value}"),
            Self::Imported(Period::All) => "全部时间",
            Self::Imported(Period::Week) => "近 7 天",
            Self::Imported(Period::Month) => "近 30 天",
            Self::Size(SizeChoice::All) => "全部大小",
            Self::Size(SizeChoice::Small) => "小于 1 MB",
            Self::Size(SizeChoice::Large) => "大于 10 MB",
            Self::Orientation(Orientation::All) => "全部比例",
            Self::Orientation(Orientation::Landscape) => "横图",
            Self::Orientation(Orientation::Portrait) => "竖图",
            Self::Orientation(Orientation::Square) => "方图",
            Self::Sort(SortOrder::Newest) => "最新导入",
            Self::Sort(SortOrder::Oldest) => "最早导入",
            Self::Sort(SortOrder::Name) => "名称",
            Self::Sort(SortOrder::Largest) => "文件最大",
            Self::Sort(SortOrder::Rating) => "评分最高",
            Self::Color(ColorChoice::All) => "全部颜色",
            Self::Color(color) => color.value(),
            Self::Storage(StorageChoice::All) => "全部方式",
            Self::Storage(StorageChoice::Managed) => "托管导入",
            Self::Storage(StorageChoice::Referenced) => "外部引用",
        })
    }
}
/// 构建素材筛选表单的选项行。
pub(super) fn rows(q: &Query) -> Vec<(&'static str, Vec<FilterChange>, Option<FilterChange>)> {
    use FilterChange::*;
    let media = [
        "JPG", "PNG", "WEBP", "GIF", "BMP", "TIFF", "HEIC", "HEIF", "AVIF", "MP4", "MOV", "MKV",
        "WEBM", "AVI",
    ]
    .into_iter()
    .map(|v| Media(MediaChoice::Format(v)));
    let mut media_choices = vec![
        Media(MediaChoice::All),
        Media(MediaChoice::Raw),
        Media(MediaChoice::Video),
    ];
    media_choices.extend(media);
    let now = chrono::Utc::now().timestamp_millis().max(0) as u64;
    vec![
        ("格式", media_choices),
        ("最低评分", (0..=5).map(Rating).collect()),
        (
            "导入时间",
            [Period::All, Period::Week, Period::Month]
                .map(Imported)
                .to_vec(),
        ),
        (
            "文件大小",
            [SizeChoice::All, SizeChoice::Small, SizeChoice::Large]
                .map(Size)
                .to_vec(),
        ),
        (
            "图片比例",
            [
                self::Orientation::All,
                self::Orientation::Landscape,
                self::Orientation::Portrait,
                self::Orientation::Square,
            ]
            .map(Orientation)
            .to_vec(),
        ),
        (
            "排序",
            [
                SortOrder::Newest,
                SortOrder::Oldest,
                SortOrder::Name,
                SortOrder::Largest,
                SortOrder::Rating,
            ]
            .map(Sort)
            .to_vec(),
        ),
        (
            "最低宽度 / 像素",
            [0, 1920, 3840, 7680].map(MinWidth).to_vec(),
        ),
        (
            "主色调",
            [
                ColorChoice::All,
                ColorChoice::Black,
                ColorChoice::White,
                ColorChoice::Gray,
                ColorChoice::Red,
                ColorChoice::Orange,
                ColorChoice::Yellow,
                ColorChoice::Green,
                ColorChoice::Cyan,
                ColorChoice::Blue,
                ColorChoice::Purple,
                ColorChoice::Pink,
            ]
            .map(Color)
            .to_vec(),
        ),
        (
            "添加方式",
            [
                StorageChoice::All,
                StorageChoice::Managed,
                StorageChoice::Referenced,
            ]
            .map(Storage)
            .to_vec(),
        ),
    ]
    .into_iter()
    .map(|(label, choices)| {
        let selected = choices.iter().copied().find(|v| v.selected(q, now));
        (label, choices, selected)
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    /// 验证筛选修改保留其他条件，月份选择往返一致。
    fn filter_values_preserve_other_fields_and_month_remains_selected() {
        let mut q = Query {
            text: "kept".into(),
            tag: "also kept".into(),
            ..Default::default()
        };
        let now = 90 * 86400000;
        FilterChange::Imported(Period::Month).apply(&mut q, now);
        assert_eq!(q.since, 60 * 86400000);
        assert!(FilterChange::Imported(Period::Month).selected(&q, now));
        assert!(!FilterChange::Imported(Period::Week).selected(&q, now));
        FilterChange::Media(MediaChoice::Raw).apply(&mut q, now);
        assert_eq!(q.media_kind, MediaKind::Raw);
        assert!(q.format.is_empty());
        FilterChange::Media(MediaChoice::Format("PNG")).apply(&mut q, now);
        assert_eq!(q.media_kind, MediaKind::All);
        assert_eq!(q.format, "png");
        assert_eq!(q.text, "kept");
        assert_eq!(q.tag, "also kept");
        assert_eq!(Builtin::Recent.query(1).since, 0);
    }
}
