use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::*;
use gpui_kit::*;
use uuid::Uuid;

use crate::components::tag_batch_editor::{TagBatchEditor, TagDeltaBatch};
use crate::components::tag_filter_bar::TagFilterBar;
use crate::components::virtual_grid::VirtualGrid;
use crate::database::entities::TagNamespace;
use crate::database::entities::collection;
use crate::services::collection::CollectionConfig;
use crate::services::importer::{self, FileInfo, TagRef};
use crate::services::macrotag::MacroTag;
use crate::services::tags_queryer::MtagService;
use crate::{AppState, components};

pub struct GenericView {
    pub config: CollectionConfig,
    pub title: SharedString,
    pub icon: Option<SharedString>,
    on_back: Option<Rc<dyn Fn(&mut Window, &mut App) + 'static>>,

    pub file_list: Vec<FileInfo>,
    scroll_handle: UniformListScrollHandle,
    filter_bar: Entity<TagFilterBar>,
    batch_editor: Option<Entity<TagBatchEditor>>,
    last_editor_selection: HashSet<uuid::Uuid>,
    current_filter_tags: Vec<MacroTag>,
    selected_files: HashSet<uuid::Uuid>,
    last_select: Option<Uuid>,
    // for any working state
    working: bool,
    enable_drag: bool,
}

impl GenericView {
    pub fn new(
        title: impl Into<SharedString>,
        config: CollectionConfig,
        cx: &mut Context<Self>,
    ) -> Self {
        let base_query = MtagService::new()
            .tags(&config.filters)
            .build()
            .unwrap_or_default();

        let is_empty_filter = config.filters.is_empty();
        let filter_bar = cx.new(|cx| {
            TagFilterBar::with_excluded_tags(
                base_query,
                config.filters.clone(),
                is_empty_filter,
                cx,
            )
        });

        let mut view = Self {
            config,
            title: title.into(),
            icon: None,
            on_back: None,

            file_list: Vec::new(),
            scroll_handle: UniformListScrollHandle::new(),
            filter_bar,
            batch_editor: None,
            last_editor_selection: HashSet::new(),
            current_filter_tags: Vec::new(),
            selected_files: HashSet::new(),
            last_select: None,
            working: false,
            enable_drag: false,
        };

        let overall_entity = cx.entity().clone();
        view.filter_bar.update(cx, |bar, _cx| {
            bar.on_change(move |tags, _window, cx| {
                overall_entity.update(cx, |this, cx| {
                    this.apply_filter(tags.to_vec(), cx);
                });
            });
        });

        view.reload_files(cx);
        view
    }

    pub fn from_collection(model: &collection::Model, cx: &mut Context<Self>) -> Self {
        let config: CollectionConfig =
            serde_json::from_value(model.config.clone()).unwrap_or_default();
        let icon = if model.icon.is_empty() {
            Some("folder".into())
        } else {
            Some(model.icon.clone().into())
        };

        let mut view = Self::new(model.name.clone(), config, cx);
        view.icon = icon;
        view
    }

    pub fn on_back(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_back = Some(Rc::new(handler));
        self
    }

    pub fn icon(mut self, icon: impl Into<Option<SharedString>>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn enable_drag(mut self) -> Self {
        self.enable_drag = true;
        self
    }

    pub fn handle_drag(
        &mut self,
        window: &mut Window,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if !self.enable_drag || self.working || paths.is_empty() {
            return;
        }

        self.working = true;
        window.push_notification(Notification::new().message("正在导入文件"), cx);
        cx.notify();

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let device_id = cx.global::<AppState>().device_id.clone();

        cx.spawn_in(window, async move |this, cx: &mut AsyncWindowContext| {
            let res = cx
                .background_spawn(async move {
                    rt.block_on(importer::import_from_paths(&db, &paths, &device_id))
                })
                .await;

            let msg = this
                .update(cx, |this, cx| {
                    this.working = false;
                    let message: SharedString = if let Ok(report) = res {
                        if report.failed.is_empty() {
                            format!("{} 个内容导入成功", report.imported).into()
                        } else {
                            format!("{} 内容径导入失败", report.failed.len()).into()
                        }
                    } else {
                        "导入成功".into()
                    };

                    this.reload_all(cx);
                    message
                })
                .ok();

            if let Some(msg) = msg {
                cx.update(|w, cx| {
                    w.push_notification(Notification::new().message(msg), cx);
                })
                .unwrap();
            }
        })
        .detach();
    }

    fn apply_filter(&mut self, tags: Vec<MacroTag>, cx: &mut Context<Self>) {
        if self.current_filter_tags == tags {
            return;
        }

        self.current_filter_tags = tags;
        self.scroll_handle.scroll_to_item(0, ScrollStrategy::Top);

        self.reload_files(cx);
        cx.notify();
    }

    pub fn reload_files(&mut self, cx: &mut Context<Self>) {
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let config_filters = self.config.filters.clone();
        let filter_tags = self.current_filter_tags.clone();

        if config_filters.is_empty() && filter_tags.is_empty() {
            self.file_list.clear();
            cx.notify();
            return;
        }

        cx.spawn(async move |this, cx| {
            let files_res = cx
                .background_spawn(async move {
                    let query = MtagService::new()
                        .tags(&config_filters)
                        .tags(&filter_tags)
                        .build()?;

                    rt.block_on(importer::list_files(&db, &query))
                })
                .await;

            this.update(cx, |this, cx| {
                if let Ok(files) = files_res {
                    this.file_list = files;
                }
                cx.notify();
            })
            .unwrap();
        })
        .detach();
    }

    /// Position of a file inside the currently loaded and filtered list.
    fn index_of(&self, file_id: Uuid) -> Option<usize> {
        self.file_list
            .iter()
            .position(|item| item.file_id == file_id)
    }

    /// refresh file list and tell filter_bar to reload its facet stats
    pub fn reload_all(&mut self, cx: &mut Context<Self>) {
        self.reload_files(cx);
        let is_empty = self.config.filters.is_empty();
        let excluded = self.config.filters.clone();
        self.filter_bar.update(cx, |bar, cx| {
            bar.set_excluded_tags(excluded, is_empty, cx);
        });
    }

    fn handle_secondary_select(&mut self, file_id: Uuid, cx: &mut Context<Self>) {
        if !self.selected_files.contains(&file_id) {
            if let Some(editor) = &self.batch_editor {
                if editor.read(cx).has_changes() {
                    self.batch_editor = None;
                }
            }
            self.selected_files.clear();
            self.selected_files.insert(file_id);
            self.last_select = Some(file_id);
            cx.notify();
        }
    }

    fn handle_file_select(
        &mut self,
        file_id: Uuid,
        is_ctrl: bool,
        is_shift: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = &self.batch_editor {
            if editor.read(cx).has_changes() {
                self.batch_editor = None;
            }
        }

        if is_ctrl {
            if self.selected_files.contains(&file_id) {
                self.selected_files.remove(&file_id);
            } else {
                self.selected_files.insert(file_id);
            }
            self.last_select = Some(file_id);
            cx.notify();
            return;
        }

        if is_shift {
            let anchor = self.last_select;
            let (from, to) = (
                anchor.and_then(|a| self.index_of(a)),
                self.index_of(file_id),
            );
            if let (Some(a), Some(b)) = (from, to) {
                let (start, end) = (a.min(b), a.max(b));
                self.selected_files.clear();
                self.selected_files.extend(
                    self.file_list[start..=end].iter().map(|item| item.file_id),
                );
                cx.notify();
                return;
            }
        }

        // normal select
        self.selected_files.clear();
        self.selected_files.insert(file_id);
        self.last_select = Some(file_id);
        cx.notify();
    }

    /// Soft delete all currently selected files and reload.
    pub fn delete_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_files.is_empty() {
            return;
        }

        let ids: Vec<Uuid> = self.selected_files.iter().copied().collect();
        self.selected_files.clear();
        self.last_select = None;
        self.batch_editor = None;
        self.last_editor_selection.clear();

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let now = chrono::Utc::now();

        cx.spawn_in(window, async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    rt.block_on(crate::database::storage::file::mark_files_deleted(
                        &db, &ids, now,
                    ))
                })
                .await;

            this.update(cx, |this, cx| {
                if let Err(err) = res {
                    tracing::error!("删除失败: {err:#}");
                }
                this.reload_all(cx);
            })
            .ok();
        })
        .detach();
    }

    fn build_context_menu(
        &self,
        mut menu: PopupMenu,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> PopupMenu {
        if let Some(file_id) = self.last_select {
            if self.selected_files.len() > 1 && self.selected_files.contains(&file_id) {
                let count = self.selected_files.len();
                menu = menu.item(
                    PopupMenuItem::new(format!("删除内容 ({count})"))
                        .icon(IconName::Delete)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.delete_selected(window, cx);
                        })),
                );
            } else {
                if let Some(file) = self.file_list.iter().find(|f| f.file_id == file_id) {
                    let open_p = file.path.clone();
                    let reveal_p = file.path.clone();
                    let copy_p = file.path.clone();

                    menu = menu
                        .item(
                            PopupMenuItem::new("打开")
                                .icon(IconName::ExternalLink)
                                .on_click(move |_, _, _| {
                                    components::utils::open_path(&open_p);
                                }),
                        )
                        .item(
                            PopupMenuItem::new("在文件管理器中显示")
                                .icon(IconName::Folder)
                                .on_click(move |_, _, _| {
                                    components::utils::reveal_in_file_manager(&reveal_p);
                                }),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new("复制路径")
                                .icon(IconName::Copy)
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        copy_p.clone(),
                                    ));
                                }),
                        );
                }

                menu = menu.separator().item(
                    PopupMenuItem::new("删除内容")
                        .icon(IconName::Delete)
                        .on_click(cx.listener(|this, _, w, cx| {
                            this.delete_selected(w, cx);
                        })),
                );
            }
        } else {
            menu = menu.item(
                PopupMenuItem::new("刷新")
                    .icon(IconName::RefreshCw)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.reload_all(cx);
                    })),
            );
        }

        menu
    }

    fn commit_tag_delta(
        &mut self,
        delta: TagDeltaBatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let file_ids: Vec<Uuid> = self.selected_files.iter().copied().collect();

        cx.spawn_in(window, async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    rt.block_on(async {
                        for new_name in &delta.to_create_and_attach {
                            MtagService::attach_by_name(
                                &db,
                                &file_ids,
                                TagNamespace::User,
                                &[new_name.clone()],
                            )
                            .await?;
                        }
                        MtagService::attach(&db, &file_ids, &delta.to_attach).await?;
                        MtagService::detach(&db, &file_ids, &delta.to_detach).await?;

                        Ok::<(), anyhow::Error>(())
                    })
                })
                .await;

            this.update(cx, |this, cx| {
                if let Err(e) = res {
                    tracing::error!("批量应用标签失败: {:#}", e);
                }
                this.selected_files.clear();
                this.last_editor_selection.clear();
                this.reload_all(cx);
            })
            .ok();
        })
        .detach();
    }
}

impl Render for GenericView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let menu_entity = cx.entity().clone();

        let mut title_bar = div().h_flex().items_center().gap_2();
        if let Some(on_back) = &self.on_back {
            let on_back = on_back.clone();
            title_bar = title_bar.child(
                Button::new("scope-back")
                    .ghost()
                    .small()
                    .icon(IconName::ChevronLeft)
                    .on_click(move |_, window, cx| {
                        on_back(window, cx);
                    }),
            );
        }
        if let Some(ref icon) = self.icon {
            title_bar = title_bar.child(
                Icon::default()
                    .path(format!("icons/{}.svg", icon))
                    .size_5()
                    .text_color(cx.theme().primary),
            );
        }
        title_bar = title_bar
            .child(div().text_xl().font_bold().child(self.title.clone()))
            .child(
                Button::new("refresh")
                    .icon(IconName::RefreshCw)
                    .on_click(cx.listener(|this, _, _, cx| this.reload_all(cx))),
            );

        div()
            .id("explorer-main")
            .v_flex()
            .size_full()
            .gap_3()
            .p_6()
            .pb_0()
            .pr_0()
            .on_click(cx.listener(|this, _, _window, cx| {
                if let Some(editor) = &this.batch_editor {
                    let has_changes = editor.read(cx).has_changes();
                    if has_changes {
                        this.batch_editor = None;
                    }
                }
                this.last_select = None;
                this.selected_files.clear();
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _window, cx| {
                    if let Some(editor) = &this.batch_editor {
                        let has_changes = editor.read(cx).has_changes();
                        if has_changes {
                            this.batch_editor = None;
                        }
                    }
                    this.last_select = None;
                    this.selected_files.clear();
                }),
            )
            .on_drop(cx.listener(|this, e: &ExternalPaths, w, cx| {
                this.handle_drag(w, e.paths().to_vec(), cx);
            }))
            .context_menu({
                let menu_entity = menu_entity.clone();
                move |menu, window, cx| {
                    menu_entity.update(cx, |this, cx| this.build_context_menu(menu, window, cx))
                }
            })
            .child(
                div()
                    .id("explorer-top-bar")
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .v_flex()
                    .gap_4()
                    .child(title_bar)
                    .child(if self.selected_files.is_empty() {
                        self.batch_editor = None;
                        self.last_editor_selection.clear();
                        if self.config.is_hide_tagbar {
                            div().into_any_element()
                        } else {
                            self.filter_bar.clone().into_any_element()
                        }
                    } else {
                        if self.last_editor_selection != self.selected_files {
                            self.last_editor_selection = self.selected_files.clone();
                            self.batch_editor = None;
                        }

                        let selected_items: Vec<FileInfo> = self
                            .file_list
                            .iter()
                            .filter(|f| self.selected_files.contains(&f.file_id))
                            .cloned()
                            .collect();

                        let popular_tags: Vec<TagRef> = self
                            .filter_bar
                            .read(cx)
                            .tags_summary()
                            .iter()
                            .take(8)
                            .map(|s| TagRef {
                                id: s.tag_id,
                                namespace: TagNamespace::User,
                                name: s.name.clone(),
                            })
                            .collect();

                        let editor = self.batch_editor.get_or_insert_with(|| {
                            let overall_entity = cx.entity().clone();
                            cx.new(|cx| {
                                let mut editor =
                                    TagBatchEditor::new(&selected_items, &popular_tags, window, cx);

                                let entity = overall_entity.clone();
                                editor.on_commit(move |delta, window, cx| {
                                    entity.update(cx, |this, cx| {
                                        this.commit_tag_delta(delta.clone(), window, cx);
                                    });
                                });

                                let entity = overall_entity.clone();
                                editor.on_cancel(move |_window, cx| {
                                    entity.update(cx, |this, cx| {
                                        this.selected_files.clear();
                                        this.batch_editor = None;
                                        this.last_editor_selection.clear();
                                        cx.notify();
                                    });
                                });

                                editor
                            })
                        });

                        editor.clone().into_any_element()
                    }),
            )
            .child(
                VirtualGrid::new("file-grid", self.file_list.clone(), {
                    let selected_files = self.selected_files.clone();
                    let entity = cx.entity().clone();

                    move |item, _idx, _window, _cx| {
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

                        components::file_card::FileCard::new(
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
                            components::utils::open_path(&path_open);
                        })
                        .into_any_element()
                    }
                })
                .scroll_handle(&self.scroll_handle)
                .empty_placeholder("当前筛选下暂无匹配文件"),
            )
            .into_any_element()
    }
}
