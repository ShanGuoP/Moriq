//! Stable content vocabulary shared by inference, storage, and navigation.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiCategoryKind {
    Medium,
    Theme,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiCategory {
    pub id: String,
    pub kind: AiCategoryKind,
    pub label: String,
    pub definition: String,
    pub enabled: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiProfile {
    pub revision: u64,
    pub max_themes: u8,
    pub categories: Vec<AiCategory>,
}
impl Default for AiProfile {
    fn default() -> Self {
        Self {
            revision: 1,
            max_themes: 3,
            categories: AI_MEDIA
                .iter()
                .chain(AI_THEMES)
                .map(|(id, label)| AiCategory {
                    id: (*id).into(),
                    kind: if AI_MEDIA.iter().any(|(key, _)| key == id) {
                        AiCategoryKind::Medium
                    } else {
                        AiCategoryKind::Theme
                    },
                    label: (*label).into(),
                    definition: ai_definition(id).into(),
                    enabled: true,
                })
                .collect(),
        }
    }
}
impl AiProfile {
    pub fn with_template(&self, template: &Self) -> Result<Self, &'static str> {
        template.validate()?;
        let mut profile = template.clone();
        profile.revision = self.revision;
        for category in &self.categories {
            if let Some(replacement) = profile.categories.iter().find(|c| c.id == category.id) {
                if replacement.kind != category.kind {
                    return Err("模板中的分类类型不一致");
                }
            } else {
                let mut hidden = category.clone();
                hidden.enabled = false;
                profile.categories.push(hidden);
            }
        }
        profile.validate()?;
        Ok(profile)
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        let active = self
            .categories
            .iter()
            .filter(|c| c.enabled)
            .collect::<Vec<_>>();
        if active
            .iter()
            .filter(|c| c.kind == AiCategoryKind::Theme)
            .count()
            > 24
            || active
                .iter()
                .map(|c| c.definition.chars().count())
                .sum::<usize>()
                > 1600
        {
            return Err("启用主题最多 24 个，识别说明合计最多 1600 字，以控制本地模型的上下文用量");
        }
        if !(1..=8).contains(&self.max_themes) || self.categories.len() > 64 {
            return Err("主题上限为 1～8，分类总数最多 64 个");
        }
        let mut ids = std::collections::HashSet::new();
        let mut labels = std::collections::HashSet::new();
        for c in &self.categories {
            if c.id.is_empty()
                || c.id.starts_with('_')
                || c.id.len() > 64
                || !c
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                || !ids.insert(&c.id)
                || !labels.insert(c.label.trim())
                || c.label.trim().is_empty()
                || c.label.chars().count() > 32
                || c.label == "未分类"
                || c.definition.trim().is_empty()
                || c.definition.chars().count() > 512
            {
                return Err("分类 ID、名称或识别说明无效；名称不能重复或使用“未分类”");
            }
            if c.kind == AiCategoryKind::Medium && !AI_MEDIA.iter().any(|(id, _)| *id == c.id) {
                return Err("媒介类型固定，新增内容请使用主题分类");
            }
        }
        if AI_MEDIA.iter().any(|(id, _)| {
            !self
                .categories
                .iter()
                .any(|c| c.id == *id && c.kind == AiCategoryKind::Medium)
        }) {
            return Err("固定媒介不能删除，可选择禁用识别");
        }
        Ok(())
    }
}
pub fn ai_definition(id: &str) -> &str {
    match id {
        "photography" => "相机拍摄的真实可见场景，包括对屏幕或印刷品的拍摄，不判断是否由 AI 生成。",
        "illustration" => "绘画、插画、漫画等绘制的视觉内容，不判断是否由 AI 生成。",
        "3d_render" => "以三维建模与渲染为明显成像形式的图像。",
        "screenshot" => "直接截取的软件界面、网页或屏幕图像，不包括拍摄屏幕的照片。",
        "uncertain" => "缺乏足够可见依据判断成像媒介。",
        "portrait" => "人物是画面主体，而不是远处或背景中的路人。",
        "landscape" => "自然景观是主体，不因为背景有山树就选择。",
        "architecture" => "建筑或室内空间为画面主体，不只是背景。",
        "animal" => "真实或绘制的动物为主体；动物造型的物件不自动归入动物。",
        "plant" => "植物为画面主体，不只是背景植被。",
        "food" => "食物或饮品为主体，不猜测看不清的手持物是食物。",
        "cosplay" => "有明确角色扮演特征的人物呈现；普通汉服或舞台服装不单凭服装归入。",
        "interface_document" => "软件界面、网页、文字文档等为内容主体。",
        _ => id,
    }
}
#[derive(Debug, Clone)]
pub struct AiGroup {
    pub id: String,
    pub label: String,
    pub query: crate::Query,
    pub count: u64,
    pub cover: Option<crate::AssetVersionId>,
}
pub const AI_MEDIA: &[(&str, &str)] = &[
    ("photography", "摄影"),
    ("illustration", "插画"),
    ("3d_render", "三维渲染"),
    ("screenshot", "界面截图"),
    ("uncertain", "不确定"),
];
pub const AI_THEMES: &[(&str, &str)] = &[
    ("portrait", "人像"),
    ("landscape", "风景"),
    ("architecture", "建筑"),
    ("animal", "动物"),
    ("plant", "植物"),
    ("food", "美食"),
    ("cosplay", "角色扮演"),
    ("interface_document", "界面与文档"),
];
pub fn ai_label(id: &str) -> &str {
    AI_MEDIA
        .iter()
        .chain(AI_THEMES)
        .find(|(key, _)| *key == id)
        .map_or(id, |(_, label)| *label)
}

#[derive(Debug, Clone)]
pub enum AiEdit {
    Classification(String, Vec<String>),
    Medium(Option<String>),
    Themes(Option<Vec<String>>),
    Description(Option<String>),
    RejectTag(String),
    PromoteTag(String),
    ResetRejectedTags,
}
