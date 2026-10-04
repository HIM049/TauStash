use std::collections::HashSet;
use std::path::PathBuf;

use gpui_kit::component::button::Button;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ElementExt as _, *};
use gpui_kit::*;
use sea_orm::ActiveEnum;
use uuid::Uuid;

use crate::components::tag_batch_editor::{TagBatchEditor, TagDeltaBatch};
use crate::components::tag_filter_bar::TagFilterBar;
use crate::database::entities::{FileState, TagNamespace};
use crate::services::importer::{self, FileInfo, TagRef};
use crate::services::tags_applier::UnifiedTagApply;
use crate::services::tags_queryer::{MacroTag, UnifiedTagService};
use crate::{AppState, components};

pub struct OverallView {
    pub file_list: Vec<FileInfo>,
    scroll_handle: UniformListScrollHandle,
    filter_bar: Entity<TagFilterBar>,
    batch_editor: Option<Entity<TagBatchEditor>>,
    last_editor_selection: HashSet<uuid::Uuid>,
    current_filter_tags: Vec<MacroTag>,
    selected_files: HashSet<uuid::Uuid>,
    last_select: Option<Uuid>,

    container_width: Pixels,

    importing: bool,
    error_message: Option<String>,
}

impl OverallView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let base_query = UnifiedTagService::new()
            .tag(MacroTag::FileState(FileState::Managed.to_value()))
            .build()
            .unwrap_or_default();

        let filter_bar = cx.new(|cx| TagFilterBar::new(base_query, cx));

        let mut view = Self {
            file_list: Vec::new(),
            scroll_handle: UniformListScrollHandle::new(),
            filter_bar,
            batch_editor: None,
            last_editor_selection: HashSet::new(),
            current_filter_tags: Vec::new(),
            selected_files: HashSet::new(),
            last_select: None,

            container_width: px(800.0),
            importing: false,
            error_message: None,
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

    pub fn handle_drag(
        &mut self,
        window: &mut Window,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if self.importing || paths.is_empty() {
            return;
        }

        self.importing = true;
        self.error_message = None;
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
                    this.importing = false;
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
        let filter_tags = self.current_filter_tags.clone();

        cx.spawn(async move |this, cx| {
            let files_res = cx
                .background_spawn(async move {
                    let query = UnifiedTagService::new()
                        .tag(MacroTag::FileState(FileState::Managed.to_value()))
                        .tags(filter_tags)
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
        self.filter_bar.update(cx, |bar, cx| bar.reload_stats(cx));
    }

    fn commit_tag_delta(
        &mut self,
        delta: TagDeltaBatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.batch_editor = None;
        self.last_editor_selection.clear();
        if delta.is_empty() {
            self.selected_files.clear();
            cx.notify();
            return;
        }

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let file_ids: Vec<Uuid> = self.selected_files.iter().copied().collect();

        cx.spawn_in(window, async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    rt.block_on(async {
                        for new_name in &delta.to_create_and_attach {
                            UnifiedTagApply::attach_by_name(
                                &db,
                                &file_ids,
                                TagNamespace::User,
                                new_name.clone(),
                            )
                            .await?;
                        }
                        UnifiedTagApply::attach(&db, &file_ids, &delta.to_attach).await?;
                        UnifiedTagApply::detach(&db, &file_ids, &delta.to_detach).await?;

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

impl Render for OverallView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width_entity = cx.entity().clone();
        let current_width = self.container_width;

        let select_entity = cx.entity().clone();

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
            .on_prepaint(move |bounds, _, cx| {
                let new_w = bounds.size.width;
                if (new_w - current_width).abs() > px(2.0) {
                    width_entity.update(cx, |this, cx| {
                        this.container_width = new_w;
                        cx.notify();
                    });
                }
            })
            .on_drop(cx.listener(|this, e: &ExternalPaths, w, cx| {
                this.handle_drag(w, e.paths().to_vec(), cx);
            }))
            .child(
                div()
                    .id("explorer-top-bar")
                    .v_flex()
                    .gap_4()
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .child(div().text_xl().font_bold().child("文件总览"))
                            .child(
                                Button::new("refresh")
                                    .icon(IconName::RefreshCw)
                                    .on_click(cx.listener(|this, _, _, cx| this.reload_all(cx))),
                            ),
                    )
                    .child(if self.selected_files.is_empty() {
                        self.batch_editor = None;
                        self.last_editor_selection.clear();
                        self.filter_bar.clone().into_any_element()
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
                components::file_grid::FileGrid::new(self.file_list.clone())
                    .container_width(self.container_width)
                    .selected_files(self.selected_files.clone())
                    .scroll_handle(&self.scroll_handle)
                    .on_select(move |file_id, window, cx| {
                        let modifiers = window.modifiers();
                        let is_ctrl = modifiers.control || modifiers.platform;
                        let is_shift = modifiers.shift;

                        select_entity.update(cx, |this, cx| {
                            if let Some(editor) = &this.batch_editor {
                                if editor.read(cx).has_changes() {
                                    this.batch_editor = None;
                                }
                            }

                            if is_ctrl {
                                if this.selected_files.contains(&file_id) {
                                    this.selected_files.remove(&file_id);
                                } else {
                                    this.selected_files.insert(file_id);
                                }
                                this.last_select = Some(file_id);

                                cx.notify();
                                return;
                            }
                            if is_shift {
                                let anchor = this.last_select;
                                let (from, to) = (
                                    anchor.and_then(|a| this.index_of(a)),
                                    this.index_of(file_id),
                                );
                                if let (Some(a), Some(b)) = (from, to) {
                                    let (start, end) = (a.min(b), a.max(b));
                                    this.selected_files.clear();
                                    this.selected_files.extend(
                                        this.file_list[start..=end].iter().map(|item| item.file_id),
                                    );

                                    cx.notify();
                                    return;
                                }
                            }

                            // normal select
                            this.selected_files.clear();
                            this.selected_files.insert(file_id);
                            this.last_select = Some(file_id);

                            cx.notify();
                        });
                    })
                    .on_open(|path, _, _| components::utils::open_path(path)),
            )
    }
}
