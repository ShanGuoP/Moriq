use super::*;
use crate::components::*;
use iced::widget::{column, row, text_input};
use std::collections::HashMap;
#[derive(Default)]
pub(super) struct FolderState {
    pub name: String,
    pub covers: HashMap<String, (AssetId, image::Handle)>,
    pub owner: Option<LibraryId>,
}
#[derive(Debug, Clone)]
pub enum FolderMessage {
    Name(String),
    Create(bool),
    Open(String),
    Rename(String),
    Delete(String),
    Members(String, bool),
    Cover(String),
    Stored(LibraryId, Result<(), String>),
    Covers,
    CoverLoaded(LibraryId, String, AssetId, Result<image::Handle, String>),
}
impl App {
    pub(super) fn folder_message(&mut self, event: FolderMessage) -> Task<Message> {
        let Some(library) = &self.library else {
            return Task::none();
        };
        let id = library.info.id;
        let service = self.service.clone();
        let edit = match event {
            FolderMessage::Name(name) => {
                self.folders.name = name;
                return Task::none();
            }
            FolderMessage::Create(favorite) => {
                FolderEdit::Create(self.folders.name.clone(), favorite)
            }
            FolderMessage::Rename(folder) => FolderEdit::Rename(folder, self.folders.name.clone()),
            FolderMessage::Delete(folder) => FolderEdit::Delete(folder),
            FolderMessage::Members(folder, add) => {
                FolderEdit::Members(folder, self.selected_ids(), add)
            }
            FolderMessage::Cover(folder) => {
                let Some(asset) = &self.viewer.selected else {
                    return Task::none();
                };
                FolderEdit::Cover(folder, asset.id)
            }
            FolderMessage::Open(folder) => {
                self.query = Query {
                    folder: Some(folder),
                    ..Default::default()
                };
                self.screen = Screen::Assets;
                self.manager.panel = None;
                return self.reset_query();
            }
            FolderMessage::Stored(owner, result) if owner == id => {
                if let Err(e) = result {
                    self.error = Some(e)
                }
                return Task::batch([self.refresh_organizer(), self.refresh()]);
            }
            FolderMessage::Covers => {
                if self.folders.owner != Some(id) {
                    self.folders.covers.clear();
                    self.folders.owner = Some(id)
                }
                let tasks = self.manager.organizer.folders.iter().filter_map(|folder| {
                    let asset = folder.cover?;
                    if self
                        .folders
                        .covers
                        .get(&folder.id)
                        .is_some_and(|(old, _)| *old == asset)
                    {
                        return None;
                    }
                    let service = service.clone();
                    let folder = folder.id.clone();
                    Some(Task::perform(
                        async move {
                            service
                                .preview(id, asset)
                                .await
                                .map(|p| image::Handle::from_rgba(p.width, p.height, p.rgba))
                        },
                        move |r| {
                            Message::Folder(FolderMessage::CoverLoaded(
                                id,
                                folder.clone(),
                                asset,
                                r,
                            ))
                        },
                    ))
                });
                return Task::batch(tasks);
            }
            FolderMessage::CoverLoaded(owner, folder, asset, Ok(handle))
                if owner == id
                    && self
                        .manager
                        .organizer
                        .folders
                        .iter()
                        .any(|f| f.id == folder && f.cover == Some(asset)) =>
            {
                self.folders.covers.insert(folder, (asset, handle));
                return Task::none();
            }
            _ => return Task::none(),
        };
        Task::perform(
            async move { service.folder_edit(id, edit).await },
            move |r| Message::Folder(FolderMessage::Stored(id, r)),
        )
    }
    pub(super) fn folders_panel(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let mut view = column![
            form_heading(
                crate::icons::Icon::Folder,
                tr!(language, "收藏分组与虚拟文件夹" => "Favorite groups and virtual folders"),
                tr!(language, "分组只保存素材引用，不复制原片。每个素材可以加入多个分组。" => "Groups store asset references without copying originals. Each asset can belong to multiple groups.")
            ),
            text_input(tr!(language, "分组名称 / 新名称" => "Group name / new name"), &self.folders.name)
                .style(crate::theme::input)
                .padding(10)
                .on_input(|v| Message::Folder(FolderMessage::Name(v))),
            row![
                form_action(
                    crate::icons::Icon::Plus,
                    tr!(language, "新建收藏分组" => "New favorite group"),
                    Message::Folder(FolderMessage::Create(true)),
                    false
                ),
                form_action(
                    crate::icons::Icon::Folder,
                    tr!(language, "新建虚拟文件夹" => "New virtual folder"),
                    Message::Folder(FolderMessage::Create(false)),
                    false
                )
            ]
            .spacing(8)
        ]
        .spacing(12);
        for folder in &self.manager.organizer.folders {
            let id = &folder.id;
            let cover = self.folders.covers.get(id).map(|(_, handle)| handle);
            view = view
                .push(
                    row![
                        photo(cover, iced::Length::Fixed(72.), 54.),
                        action(
                            &format!(
                                "{} · {} · {}",
                                folder.name,
                                folder.count,
                                if folder.favorite_group {
                                    tr!(language, "收藏" => "Favorite")
                                } else {
                                    tr!(language, "文件夹" => "Folder")
                                }
                            ),
                            Message::Folder(FolderMessage::Open(id.clone()))
                        )
                    ]
                    .spacing(10),
                )
                .push(
                    row![
                        action(
                            tr!(language, "加入所选图片" => "Add selected images"),
                            Message::Folder(FolderMessage::Members(id.clone(), true))
                        ),
                        action(
                            tr!(language, "移出所选图片" => "Remove selected images"),
                            Message::Folder(FolderMessage::Members(id.clone(), false))
                        ),
                        action(
                            tr!(language, "以当前图片为封面" => "Use current image as cover"),
                            Message::Folder(FolderMessage::Cover(id.clone()))
                        )
                    ]
                    .spacing(6),
                )
                .push(
                    row![
                        action(
                            tr!(language, "重命名" => "Rename"),
                            Message::Folder(FolderMessage::Rename(id.clone()))
                        ),
                        action(
                            tr!(language, "删除分组" => "Delete group"),
                            Message::Folder(FolderMessage::Delete(id.clone()))
                        )
                    ]
                    .spacing(6),
                );
        }
        view.into()
    }
}
