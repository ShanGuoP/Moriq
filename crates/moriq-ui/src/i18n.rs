//! Explicit, per-application localization. User content is never translated.
use moriq_domain::Language;

macro_rules! tr {
    ($language:expr, $chinese:literal => $english:literal) => {
        match $language {
            moriq_domain::Language::SimplifiedChinese => $chinese,
            moriq_domain::Language::English => $english,
        }
    };
}
macro_rules! trf {
    ($language:expr, $chinese:literal => $english:literal $(, $($arguments:tt)*)?) => {
        match $language {
            moriq_domain::Language::SimplifiedChinese => format!($chinese $(, $($arguments)*)?),
            moriq_domain::Language::English => format!($english $(, $($arguments)*)?),
        }
    };
}

/// Fixed option labels only. Dropdown callbacks retain the original value.
pub(super) fn option_label(language: Language, value: &str) -> &str {
    if language != Language::English {
        return value;
    }
    match value {
        "全部类型" => "All types",
        "视频" => "Video",
        "全部时间" => "All time",
        "近 7 天" => "Last 7 days",
        "近 30 天" => "Last 30 days",
        "全部大小" => "All sizes",
        "小于 1 MB" => "Smaller than 1 MB",
        "大于 10 MB" => "Larger than 10 MB",
        "全部比例" => "All ratios",
        "横图" => "Landscape",
        "竖图" => "Portrait",
        "方图" => "Square",
        "最新导入" => "Newest imported",
        "最早导入" => "Oldest imported",
        "名称" => "Name",
        "文件最大" => "Largest file",
        "评分最高" => "Highest rated",
        "全部颜色" => "All colors",
        "黑" => "Black",
        "白" => "White",
        "灰" => "Gray",
        "红" => "Red",
        "橙" => "Orange",
        "黄" => "Yellow",
        "绿" => "Green",
        "青" => "Cyan",
        "蓝" => "Blue",
        "紫" => "Purple",
        "粉" => "Pink",
        "全部方式" => "All methods",
        "托管导入" => "Managed import",
        "外部引用" => "External reference",
        "原文件" => "Original file",
        "双三次" => "Bicubic",
        "最近邻" => "Nearest neighbor",
        "面积平均" => "Area average",
        "平铺" => "Flat",
        "按格式" => "By format",
        "按导入日期" => "By import date",
        "不附带" => "None",
        "附带整理信息 JSON" => "Include organization JSON",
        "保持收藏状态" => "Keep favorite status",
        "收藏" => "Favorite",
        "取消收藏" => "Unfavorite",
        "保持评分" => "Keep rating",
        "自动" => "Auto",
        "按内容类型分组" => "Group by content type",
        "人物管理与纠正" => "People and corrections",
        "全部满足" => "Match all",
        "任一满足" => "Match any",
        "文件名包含" => "File name contains",
        "备注包含" => "Notes contain",
        "精确标签" => "Exact tag",
        "格式" => "Format",
        "最低评分" => "Minimum rating",
        "已收藏" => "Favorited",
        "RAW 格式" => "RAW format",
        "有补充地点" => "Has custom location",
        "全部标签" => "All tags",
        "评分" => "Rating",
        "文件大小" => "File size",
        "添加方式" => "Import method",
        "导入时间" => "Date imported",
        "图片比例" => "Aspect ratio",
        "排序" => "Sort",
        "目录布局" => "Folder layout",
        "最长边 / 像素" => "Longest edge / pixels",
        "JPEG 质量" => "JPEG quality",
        "顺时针旋转" => "Clockwise rotation",
        "重采样" => "Resampling",
        "整理信息" => "Organization metadata",
        "选择图片文件" => "Choose image files",
        "导入文件夹" => "Import folder",
        "引用外部原片" => "Reference external originals",
        _ => value,
    }
}

pub(super) fn category(language: Language, value: &str) -> &str {
    if language != Language::English {
        return value;
    }
    match value {
        "人像" => "People",
        "风景" => "Landscape",
        "城市" => "City",
        "建筑" => "Architecture",
        "植物" => "Plants",
        "插画" => "Illustration",
        "截图" => "Screenshots",
        "动物" => "Animals",
        "美食" => "Food",
        "夜景" => "Night scenes",
        "未分类" => "Uncategorized",
        _ => value,
    }
}

pub(super) fn status(language: Language, value: &str) -> &str {
    let fixed = [
        (
            "本地优先 · 灵感随时可用",
            "Local first · Inspiration at hand",
        ),
        ("资源库已就绪", "Library ready"),
        ("更改已保存到资源库", "Changes saved to the library"),
        (
            "导入设置已保存，下次导入生效",
            "Import settings saved. Changes apply to the next import.",
        ),
    ];
    fixed
        .into_iter()
        .find_map(|(chinese, english)| {
            (value == chinese || value == english).then_some(match language {
                Language::SimplifiedChinese => chinese,
                Language::English => english,
            })
        })
        .unwrap_or(value)
}
