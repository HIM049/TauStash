use std::collections::HashSet;

use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;
use uuid::Uuid;

use crate::components::file_card::FileCard;
use crate::components::item_grid_view::ItemGridView;
use crate::components::item_list_view::ItemListView;
use crate::components::utils;
use crate::services::collection::ViewOption;
use crate::services::file::FileInfo;

#[derive(Clone, Debug)]
pub enum FileContentEvent {
    SelectionChanged,
    DeleteRequested(Vec<Uuid>),
    RefreshRequested,
}

pub struct FilePoolView {
    file_list: Vec<FileInfo>,
    view_mode: ViewOption,
    selected_files: HashSet<Uuid>,
    last_select: Option<Uuid>,
    scroll_handle: UniformListScrollHandle,
    empty_placeholder: SharedString,
}

impl EventEmitter<FileContentEvent> for FilePoolView {}

impl FilePoolView {
    pub fn new(view_mode: ViewOption) -> Self {
        Self {
            file_list: Vec::new(),
            view_mode,
            selected_files: HashSet::new(),
            last_select: None,
            scroll_handle: UniformListScrollHandle::new(),
            empty_placeholder: "当前筛选下暂无匹配文件".into(),
        }
    }

    pub fn set_files(&mut self, files: Vec<FileInfo>, cx: &mut Context<Self>) {
        self.file_list = files;
        let valid_ids: HashSet<Uuid> = self.file_list.iter().map(|f| f.file_id).collect();
        let prev_count = self.selected_files.len();
        self.selected_files.retain(|id| valid_ids.contains(id));
        if let Some(last) = self.last_select {
            if !valid_ids.contains(&last) {
                self.last_select = None;
            }
        }
        if self.selected_files.len() != prev_count {
            cx.emit(FileContentEvent::SelectionChanged);
        }
        cx.notify();
    }

    pub fn set_view_mode(&mut self, mode: ViewOption, cx: &mut Context<Self>) {
        self.view_mode = mode;
        cx.notify();
    }

    pub fn view_mode(&self) -> ViewOption {
        self.view_mode
    }

    pub fn file_list(&self) -> &[FileInfo] {
        &self.file_list
    }

    pub fn selected_files(&self) -> &HashSet<Uuid> {
        &self.selected_files
    }

    pub fn selected_ids(&self) -> Vec<Uuid> {
        self.selected_files.iter().copied().collect()
    }

    pub fn selected_items(&self) -> Vec<FileInfo> {
        self.file_list
            .iter()
            .filter(|f| self.selected_files.contains(&f.file_id))
            .cloned()
            .collect()
    }

    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        if !self.selected_files.is_empty() || self.last_select.is_some() {
            self.selected_files.clear();
            self.last_select = None;
            cx.emit(FileContentEvent::SelectionChanged);
            cx.notify();
        }
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        self.selected_files = self.file_list.iter().map(|f| f.file_id).collect();
        self.last_select = self.file_list.last().map(|f| f.file_id);
        cx.emit(FileContentEvent::SelectionChanged);
        cx.notify();
    }

    pub fn scroll_handle(&self) -> &UniformListScrollHandle {
        &self.scroll_handle
    }

    fn index_of(&self, file_id: Uuid) -> Option<usize> {
        self.file_list.iter().position(|f| f.file_id == file_id)
    }

    pub fn handle_file_select(
        &mut self,
        file_id: Uuid,
        is_ctrl: bool,
        is_shift: bool,
        cx: &mut Context<Self>,
    ) {
        if is_shift {
            let start = self.last_select.and_then(|id| self.index_of(id));
            let end = self.index_of(file_id);

            match (start, end) {
                (Some(s), Some(e)) => {
                    let (min, max) = if s < e { (s, e) } else { (e, s) };
                    for item in &self.file_list[min..=max] {
                        self.selected_files.insert(item.file_id);
                    }
                }
                _ => {
                    self.selected_files.insert(file_id);
                }
            }
            self.last_select = Some(file_id);
        } else if is_ctrl {
            if self.selected_files.contains(&file_id) {
                self.selected_files.remove(&file_id);
            } else {
                self.selected_files.insert(file_id);
            }
            self.last_select = Some(file_id);
        } else {
            self.selected_files.clear();
            self.selected_files.insert(file_id);
            self.last_select = Some(file_id);
        }

        cx.emit(FileContentEvent::SelectionChanged);
        cx.notify();
    }

    pub fn handle_secondary_select(&mut self, file_id: Uuid, cx: &mut Context<Self>) {
        if !self.selected_files.contains(&file_id) {
            self.selected_files.clear();
            self.selected_files.insert(file_id);
            self.last_select = Some(file_id);
            cx.emit(FileContentEvent::SelectionChanged);
            cx.notify();
        }
    }

    fn build_context_menu(&self, mut menu: PopupMenu, cx: &mut Context<Self>) -> PopupMenu {
        if let Some(file_id) = self.last_select {
            if self.selected_files.len() > 1 && self.selected_files.contains(&file_id) {
                let count = self.selected_files.len();
                let ids = self.selected_ids();
                menu = menu.item(
                    PopupMenuItem::new(format!("删除内容 ({count})"))
                        .icon(IconName::Delete)
                        .on_click(cx.listener(move |_this, _, _window, cx| {
                            cx.emit(FileContentEvent::DeleteRequested(ids.clone()));
                        })),
                );
            } else if let Some(file) = self.file_list.iter().find(|f| f.file_id == file_id) {
                let open_p = file.path.clone();
                let reveal_p = file.path.clone();
                let copy_p = file.path.clone();
                let ids = vec![file_id];

                menu = menu
                    .item(
                        PopupMenuItem::new("打开")
                            .icon(IconName::ExternalLink)
                            .on_click(move |_, _, _| {
                                utils::open_path(&open_p);
                            }),
                    )
                    .item(
                        PopupMenuItem::new("在文件管理器中显示")
                            .icon(IconName::Folder)
                            .on_click(move |_, _, _| {
                                utils::reveal_in_file_manager(&reveal_p);
                            }),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new("复制路径")
                            .icon(IconName::Copy)
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy_p.clone()));
                            }),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new("删除内容")
                            .icon(IconName::Delete)
                            .on_click(cx.listener(move |_this, _, _w, cx| {
                                cx.emit(FileContentEvent::DeleteRequested(ids.clone()));
                            })),
                    );
            }
        } else {
            menu = menu.item(
                PopupMenuItem::new("刷新")
                    .icon(IconName::RefreshCw)
                    .on_click(cx.listener(|_this, _, _, cx| {
                        cx.emit(FileContentEvent::RefreshRequested);
                    })),
            );
        }

        menu
    }
}

impl Render for FilePoolView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let menu_entity = cx.entity().clone();

        let inner_view = match self.view_mode {
            ViewOption::List => ItemListView::new("file-list", self.file_list.clone())
                .selected_files(self.selected_files.clone())
                .scroll_handle(&self.scroll_handle)
                .empty_placeholder(self.empty_placeholder.clone())
                .on_select(cx.processor(|this, (file_id, is_ctrl, is_shift), _, cx| {
                    this.handle_file_select(file_id, is_ctrl, is_shift, cx);
                }))
                .on_secondary_select(cx.processor(|this, file_id, _, cx| {
                    this.handle_secondary_select(file_id, cx);
                }))
                .on_open(|path, _, _| {
                    utils::open_path(path);
                })
                .into_any_element(),
            ViewOption::Grid => {
                let selected_files = self.selected_files.clone();
                let entity = cx.entity().clone();

                ItemGridView::new(
                    "file-grid",
                    self.file_list.clone(),
                    move |item, _idx, _, _cx| {
                        let extension = item
                            .ext
                            .as_deref()
                            .unwrap_or_else(|| item.mime.split('/').nth(1).unwrap_or("file"))
                            .to_uppercase();
                        let file_id = item.file_id;
                        let is_selected = selected_files.contains(&file_id);
                        let file_path = SharedString::new(item.path.as_str());
                        let path_open = file_path.clone();
                        let hash = SharedString::new(item.hash.as_str());
                        let mime = SharedString::new(item.mime.as_str());

                        let select_entity = entity.clone();
                        let sec_entity = entity.clone();

                        FileCard::new(
                            ElementId::Name(format!("file-card-{file_id}").into()),
                            SharedString::new(item.file_name.as_str()),
                            extension,
                            item.size,
                        )
                        .path(file_path)
                        .hash(hash)
                        .mime(mime)
                        .selected(is_selected)
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            let modifiers = window.modifiers();
                            let is_ctrl = modifiers.control || modifiers.platform;
                            let is_shift = modifiers.shift;

                            select_entity.update(cx, |this, cx| {
                                this.handle_file_select(file_id, is_ctrl, is_shift, cx);
                            });
                        })
                        .on_secondary_click(move |_window, cx| {
                            sec_entity.update(cx, |this, cx| {
                                this.handle_secondary_select(file_id, cx);
                            });
                        })
                        .on_double_click(move |_, _, _| {
                            utils::open_path(&path_open);
                        })
                        .into_any_element()
                    },
                )
                .scroll_handle(&self.scroll_handle)
                .empty_placeholder(self.empty_placeholder.clone())
                .into_any_element()
            }
        };

        div()
            .id("file-content-view")
            .size_full()
            .on_click(cx.listener(|this, _, _window, cx| {
                this.clear_selection(cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _window, cx| {
                    this.clear_selection(cx);
                }),
            )
            .context_menu({
                let menu_entity = menu_entity.clone();
                move |menu, _window, cx| {
                    menu_entity.update(cx, |this, cx| this.build_context_menu(menu, cx))
                }
            })
            .child(inner_view)
    }
}
