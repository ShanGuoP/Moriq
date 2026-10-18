use super::*;
use crate::{components::*, manage::Panel};
use iced::widget::{column, container, image as image_widget, row};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionChoice {
    id: AssetVersionId,
    label: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dismissed_comparison_and_stale_requests_never_replace_images() {
        let mut app = crate::tests::app();
        let pair = [AssetVersionId::default(), AssetVersionId::default()];
        let library = app.library.as_ref().map(|l| l.info.id).unwrap_or_default();
        let asset = AssetId::default();
        app.selected = Some(Asset {
            id: asset,
            version_id: pair[0],
            name: "sample".into(),
            format: "png".into(),
            width: 10,
            height: 10,
            bytes: 1,
            favorite: false,
            rating: 0,
            tags: vec![],
            imported_at: 0,
        });
        app.manager.panel = Some(Panel::Compare);
        app.compare.pair = Some(pair);
        app.compare.request = 4;
        let result = || {
            Ok([
                image::Handle::from_rgba(1, 1, vec![0; 4]),
                image::Handle::from_rgba(1, 1, vec![0; 4]),
            ])
        };
        drop(app.compare_message(CompareMessage::Loaded(library, asset, pair, 3, result())));
        assert!(app.compare.images.is_none());
        drop(app.compare_message(CompareMessage::Loaded(library, asset, pair, 4, result())));
        assert!(app.compare.images.is_some());
        app.compare.clear();
        drop(app.compare_message(CompareMessage::Loaded(library, asset, pair, 4, result())));
        assert!(app.compare.images.is_none());
        assert!(app.compare.pair.is_none());
    }
}
impl std::fmt::Display for VersionChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
#[derive(Default)]
pub(super) struct Compare {
    choices: Vec<VersionChoice>,
    pair: Option<[AssetVersionId; 2]>,
    images: Option<[image::Handle; 2]>,
    request: u64,
    task: Option<iced::task::Handle>,
    loading: bool,
}
impl Compare {
    pub fn clear(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
        *self = Self {
            request: self.request + 1,
            ..Default::default()
        };
    }
}
#[derive(Debug, Clone)]
pub enum CompareMessage {
    Open,
    Pick(bool, VersionChoice),
    Loaded(
        LibraryId,
        AssetId,
        [AssetVersionId; 2],
        u64,
        Result<[image::Handle; 2], String>,
    ),
}
impl App {
    pub(super) fn compare_message(&mut self, event: CompareMessage) -> Task<Message> {
        match event {
            CompareMessage::Open => {
                self.compare.clear();
                let Some(asset) = &self.selected else {
                    return Task::none();
                };
                self.compare.choices = self
                    .manager
                    .details
                    .versions
                    .iter()
                    .enumerate()
                    .map(|(i, v)| VersionChoice {
                        id: v.id,
                        label: format!("V{} · {}", i + 1, v.origin),
                    })
                    .collect();
                let Some(first) = self.compare.choices.first() else {
                    self.error = Some("版本信息尚未加载，请稍后重试".into());
                    return Task::none();
                };
                let right = if asset.version_id != first.id {
                    asset.version_id
                } else {
                    self.compare
                        .choices
                        .last()
                        .map(|v| v.id)
                        .unwrap_or(first.id)
                };
                self.compare.pair = Some([first.id, right]);
                self.manager.panel = Some(Panel::Compare);
            }
            CompareMessage::Pick(right, choice) => {
                if self.manager.panel != Some(Panel::Compare)
                    || !self.compare.choices.contains(&choice)
                {
                    return Task::none();
                }
                if let Some(pair) = &mut self.compare.pair {
                    pair[usize::from(right)] = choice.id;
                }
            }
            CompareMessage::Loaded(library, asset, pair, request, result) => {
                if self.library.as_ref().is_some_and(|l| l.info.id == library)
                    && self.selected.as_ref().is_some_and(|a| a.id == asset)
                    && self.manager.panel == Some(Panel::Compare)
                    && self.compare.request == request
                    && self.compare.pair == Some(pair)
                {
                    self.compare.loading = false;
                    self.compare.task = None;
                    match result {
                        Ok(images) => self.compare.images = Some(images),
                        Err(e) => self.error = Some(e),
                    }
                }
                return Task::none();
            }
        }
        let (Some(lib), Some(asset), Some(pair)) =
            (&self.library, &self.selected, self.compare.pair)
        else {
            return Task::none();
        };
        if let Some(task) = self.compare.task.take() {
            task.abort();
        }
        self.compare.request += 1;
        self.compare.loading = true;
        self.compare.images = None;
        let request = self.compare.request;
        let library = lib.info.id;
        let asset = asset.id;
        let service = self.service.clone();
        let (task, handle) = Task::perform(
            async move {
                service
                    .compare_versions(library, asset, pair[0], pair[1])
                    .await
                    .map(|(a, b)| {
                        [
                            image::Handle::from_rgba(a.width, a.height, a.rgba),
                            image::Handle::from_rgba(b.width, b.height, b.rgba),
                        ]
                    })
            },
            move |result| {
                Message::Compare(CompareMessage::Loaded(
                    library, asset, pair, request, result,
                ))
            },
        )
        .abortable();
        self.compare.task = Some(handle);
        task
    }
    pub(super) fn compare_panel(&self) -> Element<'_, Message> {
        let mut view=column![muted("左右独立选择版本，不改变展示版本。两侧均为最长边 1600 像素预览；未接入 ICC/HDR 色彩校准。")].spacing(12);
        let mut sides = row![].spacing(16);
        for index in 0..2 {
            let selected = self.compare.pair.and_then(|p| {
                self.compare
                    .choices
                    .iter()
                    .find(|v| v.id == p[index])
                    .cloned()
            });
            let mut side = column![choice(self.compare.choices.clone(), selected, move |v| {
                Message::Compare(CompareMessage::Pick(index == 1, v))
            })]
            .spacing(10)
            .width(iced::Length::Fill);
            if let Some(images) = &self.compare.images {
                side = side.push(
                    container(
                        image_widget(images[index].clone())
                            .content_fit(iced::ContentFit::Contain)
                            .width(iced::Length::Fill)
                            .height((self.viewport.height - 330.0).clamp(180.0, 650.0)),
                    )
                    .width(iced::Length::Fill),
                );
            }
            sides = sides.push(side);
        }
        view = view.push(sides);
        if self.compare.loading {
            view = view.push(muted("正在读取所选版本…"));
        }
        view.into()
    }
}
