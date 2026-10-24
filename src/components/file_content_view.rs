use std::collections::HashSet;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Enter, Escape, InputState};
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_kit::*;
use uuid::Uuid;

use crate::AppState;
use crate::components::grid_card::CardImage;
use crate::components::item_grid_view::{CardItem, ItemGridView};
use crate::components::item_list_view::ItemListView;
use crate::components::utils;
use crate::database::storage::query::FileSortField;
use crate::services::collection::{GroupMode, TimeGroup, ViewOption};
use crate::services::file::FileInfo;

impl CardItem for FileInfo {
    fn card_id(&self) -> Uuid {
        self.file_id
    }

    fn card_title(&self) -> SharedString {
        SharedString::new(self.file_name.as_str())
    }

    fn card_image(&self) -> CardImage {
        if self.mime.starts_with("image/") {
            let p = crate::services::thumbnail::get_thumbnail_path(&self.hash);
            CardImage::Image(p)
        } else {
            let ext = self.ext.as_deref().unwrap_or("FILE");
            CardImage::Icon {
                name: IconName::File.path(),
                label: Some(ext.into()),
            }
        }
    }
}

fn format_date(dt: Option<chrono::DateTime<chrono::Utc>>, group: TimeGroup) -> (String, String) {
    match dt {
        Some(dt) => {
            let local = dt.with_timezone(&chrono::Local);
            match group {
                TimeGroup::Day => {
                    let key = local.format("%Y-%m-%d").to_string();
                    let title = local.format("%Y年%m月%d日").to_string();
                    (key, title)
                }
                TimeGroup::Month => {
                    let key = local.format("%Y-%m").to_string();
                    let title = local.format("%Y年%m月").to_string();
                    (key, title)
                }
                TimeGroup::Year => {
                    let key = local.format("%Y").to_string();
                    let title = local.format("%Y年").to_string();
                    (key, title)
                }
            }
        }
        None => ("other".to_string(), "其他分组".to_string()),
    }
}

fn format_size_group(size: i64) -> (String, String) {
    if size >= 1024 * 1024 * 1024 {
        (
            "bigger_than_1gb".to_string(),
            "超大文件 (> 1GB)".to_string(),
        )
    } else if size >= 100 * 1024 * 1024 {
        (
            "bigger_than_100mb".to_string(),
            "大文件 (100MB - 1GB)".to_string(),
        )
    } else if size >= 10 * 1024 * 1024 {
        (
            "bigger_than_10mb".to_string(),
            "中等文件 (10MB - 100MB)".to_string(),
        )
    } else if size >= 1024 * 1024 {
        (
            "bigger_than_1mb".to_string(),
            "小文件 (1MB - 10MB)".to_string(),
        )
    } else {
        (
            "smaller_than_10mb".to_string(),
            "微型文件 (< 1MB)".to_string(),
        )
    }
}

fn get_group_by_fn(
    sort_field: FileSortField,
    group_mode: GroupMode,
) -> Option<Box<dyn Fn(&FileInfo) -> (String, String)>> {
    match group_mode {
        GroupMode::None => None,
        GroupMode::Size => Some(Box::new(|f| format_size_group(f.size))),
        GroupMode::Time(group) => match sort_field {
            FileSortField::ContentCreatedAt => {
                Some(Box::new(move |f| format_date(f.content_created_at, group)))
            }
            FileSortField::ContentModifiedAt => {
                Some(Box::new(move |f| format_date(f.content_modified_at, group)))
            }
            FileSortField::CreatedAt => {
                Some(Box::new(move |f| format_date(Some(f.created_at), group)))
            }
            FileSortField::UpdatedAt => {
                Some(Box::new(move |f| format_date(Some(f.updated_at), group)))
            }
            FileSortField::Size => Some(Box::new(|f| format_size_group(f.size))),
        },
    }
}

#[derive(Clone, Debug)]
pub enum FileContentEvent {
    SelectionChanged,
    DeleteRequested(Vec<Uuid>),
    RefreshRequested,
}

pub struct FilePoolView {
    file_list: Vec<FileInfo>,
    view_mode: ViewOption,
    sort_field: FileSortField,
    group_mode: GroupMode,
    selected_files: HashSet<Uuid>,
    last_select: Option<Uuid>,
    cursor_id: Option<Uuid>,
    name_input: Entity<InputState>,
    focus_handle: FocusHandle,
    editing_id: Option<Uuid>,
    scroll_handle: UniformListScrollHandle,
    empty_placeholder: SharedString,
}

impl EventEmitter<FileContentEvent> for FilePoolView {}

impl FilePoolView {
    pub fn new(
        view_mode: ViewOption,
        sort_field: FileSortField,
        group_mode: GroupMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx));
        let focus_handle = cx.focus_handle();

        Self {
            file_list: vec![],
            view_mode,
            sort_field,
            group_mode,
            selected_files: HashSet::new(),
            last_select: None,
            cursor_id: None,
            name_input,
            focus_handle,
            editing_id: None,
            scroll_handle: UniformListScrollHandle::new(),
            empty_placeholder: "当前筛选下暂无匹配文件".into(),
        }
    }

    pub fn set_files(&mut self, files: Vec<FileInfo>, cx: &mut Context<Self>) {
        self.editing_id = None;
        self.file_list = files;
        let valid_ids: HashSet<Uuid> = self.file_list.iter().map(|f| f.file_id).collect();
        let prev_count = self.selected_files.len();
        self.selected_files.retain(|id| valid_ids.contains(id));
        if let Some(last) = self.last_select {
            if !valid_ids.contains(&last) {
                self.last_select = None;
            }
        }
        if let Some(cursor) = self.cursor_id {
            if !valid_ids.contains(&cursor) {
                self.cursor_id = None;
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

    pub fn set_grouping(
        &mut self,
        sort_field: FileSortField,
        group_mode: GroupMode,
        cx: &mut Context<Self>,
    ) {
        self.sort_field = sort_field;
        self.group_mode = group_mode;
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
        if !self.selected_files.is_empty() || self.last_select.is_some() || self.cursor_id.is_some()
        {
            self.selected_files.clear();
            self.last_select = None;
            self.cursor_id = None;
            cx.emit(FileContentEvent::SelectionChanged);
            cx.notify();
        }
    }

    pub fn scroll_handle(&self) -> &UniformListScrollHandle {
        &self.scroll_handle
    }

    pub fn is_renaming(&self) -> bool {
        self.editing_id.is_some()
    }

    pub fn start_rename(
        &mut self,
        file_id: Uuid,
        initial_name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editing_id = Some(file_id);
        self.selected_files.clear();
        self.selected_files.insert(file_id);
        self.last_select = Some(file_id);
        self.cursor_id = Some(file_id);

        self.name_input.update(cx, |input, cx| {
            input.set_value(initial_name, window, cx);
            input.select_all(window, cx);
        });

        let focus = self.name_input.read(cx).focus_handle(cx);
        focus.focus(window, cx);

        cx.emit(FileContentEvent::SelectionChanged);
        cx.notify();
    }

    pub fn cancel_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_id.is_none() {
            return;
        }
        self.editing_id = None;
        self.name_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        cx.notify();
    }

    pub fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(file_id) = self.editing_id.take() else {
            return;
        };

        let new_name = self.name_input.read(cx).value().trim().to_string();
        if new_name.is_empty() {
            self.cancel_rename(window, cx);
            return;
        }

        if let Some(existing) = self.file_list.iter().find(|f| f.file_id == file_id) {
            if existing.file_name == new_name {
                cx.notify();
                return;
            }
        }

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();

        cx.spawn_in(window, async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    rt.block_on(crate::database::storage::file::rename_file(
                        &db, file_id, &new_name,
                    ))
                })
                .await;

            this.update(cx, |_, cx| {
                if let Err(err) = res {
                    tracing::error!("重命名文件失败: {err:#}");
                }
                cx.emit(FileContentEvent::RefreshRequested);
            })
            .ok();
        })
        .detach();
    }

    fn index_of(&self, file_id: Uuid) -> Option<usize> {
        self.file_list.iter().position(|f| f.file_id == file_id)
    }

    pub fn handle_file_select(
        &mut self,
        file_id: Uuid,
        is_ctrl: bool,
        is_shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editing_id.is_some() {
            self.commit_rename(window, cx);
        }

        self.cursor_id = Some(file_id);

        if is_ctrl {
            if self.selected_files.contains(&file_id) {
                self.selected_files.remove(&file_id);
            } else {
                self.selected_files.insert(file_id);
            }
            self.last_select = Some(file_id);
            cx.emit(FileContentEvent::SelectionChanged);
            cx.notify();
            return;
        }

        if is_shift && let Some(anchor) = self.last_select {
            if let (Some(from), Some(to)) = (self.index_of(anchor), self.index_of(file_id)) {
                self.selected_files.clear();
                let (start, end) = (from.min(to), from.max(to));
                for item in &self.file_list[start..=end] {
                    self.selected_files.insert(item.file_id);
                }
                cx.emit(FileContentEvent::SelectionChanged);
                cx.notify();
                return;
            }
        }

        self.selected_files.clear();
        self.selected_files.insert(file_id);
        self.last_select = Some(file_id);
        cx.emit(FileContentEvent::SelectionChanged);
        cx.notify();
    }

    pub fn handle_secondary_select(
        &mut self,
        file_id: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editing_id.is_some() {
            self.commit_rename(window, cx);
        }

        self.cursor_id = Some(file_id);

        if !self.selected_files.contains(&file_id) {
            self.selected_files.clear();
            self.selected_files.insert(file_id);
            self.last_select = Some(file_id);
            cx.emit(FileContentEvent::SelectionChanged);
        }
        cx.notify();
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
                let rename_id = file_id;
                let raw_name = file.file_name.clone();

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
                        PopupMenuItem::new("重命名")
                            .icon(IconName::Replace)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.start_rename(rename_id, &raw_name, window, cx);
                            })),
                    )
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
        let editing_id = self.editing_id;
        let name_input = self.name_input.clone();

        let group_by_fn = get_group_by_fn(self.sort_field, self.group_mode);

        let inner_view = match self.view_mode {
            ViewOption::List => {
                let mut list = ItemListView::new("file-list", self.file_list.clone())
                    .selected_files(self.selected_files.clone())
                    .cursor_id(self.cursor_id)
                    .editing_id(editing_id)
                    .name_input(if editing_id.is_some() {
                        Some(name_input.clone())
                    } else {
                        None
                    })
                    .scroll_handle(&self.scroll_handle)
                    .empty_placeholder(self.empty_placeholder.clone())
                    .on_select(
                        cx.processor(|this, (file_id, is_ctrl, is_shift), window, cx| {
                            this.handle_file_select(file_id, is_ctrl, is_shift, window, cx);
                        }),
                    )
                    .on_secondary_select(cx.processor(|this, file_id, window, cx| {
                        this.handle_secondary_select(file_id, window, cx);
                    }))
                    .on_open(|path, _, _| {
                        utils::open_path(path);
                    });

                if let Some(key_fn) = group_by_fn {
                    list = list.group_by(key_fn);
                }

                list.into_any_element()
            }
            ViewOption::Grid => {
                let mut grid = ItemGridView::new("file-grid", self.file_list.clone())
                    .selected_ids(self.selected_files.clone())
                    .cursor_id(self.cursor_id)
                    .editing_id(editing_id)
                    .name_input(if editing_id.is_some() {
                        Some(name_input.clone())
                    } else {
                        None
                    })
                    .scroll_handle(&self.scroll_handle)
                    .empty_placeholder(self.empty_placeholder.clone())
                    .on_select(
                        cx.processor(|this, (file_id, is_ctrl, is_shift), window, cx| {
                            this.handle_file_select(file_id, is_ctrl, is_shift, window, cx);
                        }),
                    )
                    .on_secondary_select(cx.processor(|this, file_id, window, cx| {
                        this.handle_secondary_select(file_id, window, cx);
                    }))
                    .on_open(|file, _, _| {
                        utils::open_path(&file.path);
                    });

                if let Some(key_fn) = group_by_fn {
                    grid = grid.group_by(key_fn);
                }

                grid.into_any_element()
            }
        };

        div()
            .id("file-content-view")
            .size_full()
            .track_focus(&self.focus_handle)
            .on_click(cx.listener(|this, _, window, cx| {
                if this.editing_id.is_some() {
                    this.commit_rename(window, cx);
                } else {
                    this.clear_selection(cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, window, cx| {
                    if this.editing_id.is_some() {
                        this.commit_rename(window, cx);
                    } else {
                        this.clear_selection(cx);
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &Enter, window, cx| {
                this.commit_rename(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                this.cancel_rename(window, cx);
            }))
            .context_menu({
                let menu_entity = menu_entity.clone();
                move |menu, _window, cx| {
                    menu_entity.update(cx, |this, cx| this.build_context_menu(menu, cx))
                }
            })
            .child(inner_view)
    }
}
