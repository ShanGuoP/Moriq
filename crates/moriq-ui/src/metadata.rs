use super::*;
use crate::components::*;
use iced::widget::{column, row, text_input};
const FIELDS: [(&str, &str); 10] = [
    ("make", "厂商"),
    ("camera", "相机型号"),
    ("lens", "镜头"),
    ("taken_at", "拍摄日期"),
    ("iso", "ISO"),
    ("aperture_x100", "光圈（如 2.8）"),
    ("exposure_us", "曝光时间（秒，如 1/1000）"),
    ("focal_x100", "焦距（毫米）"),
    ("latitude_e6", "纬度（度）"),
    ("longitude_e6", "经度（度）"),
];
fn display_number(index: usize, value: Option<i64>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if index == 3 {
        let value = format!("{value:014}");
        return format!(
            "{}-{}-{} {}:{}:{}",
            &value[0..4],
            &value[4..6],
            &value[6..8],
            &value[8..10],
            &value[10..12],
            &value[12..14]
        );
    }
    let scale = match index {
        5 | 7 => 100.,
        6 | 8 | 9 => 1_000_000.,
        _ => 1.,
    };
    (value as f64 / scale).to_string()
}
fn parse_number(index: usize, text: &str, upper: bool) -> Result<Option<i64>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    if index == 3 {
        let date = if text.len() == 10 {
            let date = chrono::NaiveDate::parse_from_str(&text.replace(':', "-"), "%Y-%m-%d")
                .map_err(|_| "日期请填写 YYYY-MM-DD")?;
            date.and_hms_opt(
                if upper { 23 } else { 0 },
                if upper { 59 } else { 0 },
                if upper { 59 } else { 0 },
            )
            .ok_or("日期无效")?
        } else {
            chrono::NaiveDateTime::parse_from_str(&text.replace('T', " "), "%Y-%m-%d %H:%M:%S")
                .map_err(|_| "时间请填写 YYYY-MM-DD HH:MM:SS")?
        };
        return date
            .format("%Y%m%d%H%M%S")
            .to_string()
            .parse()
            .map(Some)
            .map_err(|_| "日期范围无效".into());
    }
    let number = if index == 6 && text.contains('/') {
        let (a, b) = text.split_once('/').ok_or("曝光时间无效")?;
        let a = a.trim().parse::<f64>().map_err(|_| "曝光时间无效")?;
        let b = b.trim().parse::<f64>().map_err(|_| "曝光时间无效")?;
        a / b
    } else {
        text.parse::<f64>().map_err(|_| "请输入数字")?
    };
    let limit = match index {
        8 => 90.,
        9 => 180.,
        _ => f64::MAX,
    };
    if !number.is_finite() || number.abs() > limit || (index < 8 && number < 0.) {
        return Err("元数据数值超出有效范围".into());
    }
    let scale = match index {
        5 | 7 => 100.,
        6 | 8 | 9 => 1_000_000.,
        _ => 1.,
    };
    let scaled = (number * scale).round();
    if !scaled.is_finite() || scaled.abs() >= i64::MAX as f64 {
        return Err("元数据数值过大".into());
    }
    Ok(Some(scaled as i64))
}
#[derive(Default)]
pub(super) struct MetadataState {
    pub values: [[String; 3]; 10],
}
#[derive(Debug, Clone)]
pub enum MetadataMessage {
    Open,
    Input(usize, usize, String),
    Apply,
    Clear,
}
impl App {
    pub(super) fn metadata_message(&mut self, event: MetadataMessage) -> Task<Message> {
        match event {
            MetadataMessage::Open => {
                self.metadata = Default::default();
                for predicate in &self.query.metadata {
                    if let Some(index) = FIELDS.iter().position(|(key, _)| *key == predicate.field)
                    {
                        self.metadata.values[index] = [
                            predicate.contains.clone(),
                            display_number(index, predicate.min),
                            display_number(index, predicate.max),
                        ];
                    }
                }
                self.manager.panel = Some(manage::Panel::Metadata);
            }
            MetadataMessage::Input(index, field, value) if index < 10 && field < 3 => {
                self.metadata.values[index][field] = value
            }
            MetadataMessage::Clear => {
                self.metadata = Default::default();
                self.query.metadata.clear();
                self.manager.panel = None;
                return self.reset_query();
            }
            MetadataMessage::Apply => {
                let mut predicates = Vec::new();
                for (index, (field, _)) in FIELDS.iter().enumerate() {
                    let values = &self.metadata.values[index];
                    if values.iter().all(|v| v.trim().is_empty()) {
                        continue;
                    }
                    let (min, max) = match (
                        parse_number(index, &values[1], false),
                        parse_number(index, &values[2], true),
                    ) {
                        (Ok(min), Ok(max)) => (min, max),
                        (Err(e), _) | (_, Err(e)) => {
                            self.error = Some(e);
                            return Task::none();
                        }
                    };
                    if min.zip(max).is_some_and(|(a, b)| a > b) {
                        self.error = Some("范围起点不能大于终点".into());
                        return Task::none();
                    }
                    predicates.push(MetadataPredicate {
                        field: (*field).into(),
                        contains: values[0].trim().into(),
                        min,
                        max,
                    });
                }
                self.query.metadata = predicates;
                self.screen = Screen::Assets;
                self.manager.panel = None;
                return self.reset_query();
            }
            _ => {}
        }
        Task::none()
    }
    pub(super) fn metadata_panel(&self) -> Element<'_, Message> {
        let mut view = column![
            muted("条件同时满足。经纬度填写常用度数；拍摄日期支持起止范围，结束日期包含当天。"),
            action(
                "为已有素材建立 / 更新 EXIF 索引",
                Message::Jobs(tasks::JobMessage::Queue("metadata".into()))
            )
        ]
        .spacing(12);
        for (index, (_, label)) in FIELDS.iter().enumerate() {
            let values = &self.metadata.values[index];
            view = view.push(heading(*label, 13));
            if index < 3 {
                view = view.push(
                    text_input("包含文字", &values[0])
                        .style(crate::theme::input)
                        .padding(10)
                        .on_input(move |v| Message::Metadata(MetadataMessage::Input(index, 0, v))),
                );
            } else {
                view = view.push(
                    row![
                        text_input(
                            if index == 3 {
                                "开始日期 YYYY-MM-DD"
                            } else {
                                "最小值"
                            },
                            &values[1]
                        )
                        .style(crate::theme::input)
                        .padding(10)
                        .on_input(move |v| Message::Metadata(MetadataMessage::Input(index, 1, v))),
                        text_input(
                            if index == 3 {
                                "结束日期 YYYY-MM-DD"
                            } else {
                                "最大值"
                            },
                            &values[2]
                        )
                        .style(crate::theme::input)
                        .padding(10)
                        .on_input(move |v| Message::Metadata(MetadataMessage::Input(index, 2, v)))
                    ]
                    .spacing(8),
                );
            }
        }
        view.push(
            row![
                action("应用检索", Message::Metadata(MetadataMessage::Apply)),
                action("清除元数据条件", Message::Metadata(MetadataMessage::Clear))
            ]
            .spacing(8),
        )
        .into()
    }
}
