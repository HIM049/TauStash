use std::collections::HashSet;
use std::path::PathBuf;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::AppState;
use crate::components::tag_bar_editor::TagBatchEditor;
use crate::components::tag_chip::TagChip;
use crate::components::view_scaffold::ViewScaffold;
use crate::database::entities::{FileState, ScanMode, scan_source};
use crate::database::storage;
use crate::services::macrotag::MacroTag;
use crate::services::scanner;

pub struct ScannerView {
    sources: Vec<scan_source::Model>,
    draft: Option<Entity<InputState>>,
    /// Batch scan in progress, all controls are disabled
    is_scanning: bool,
    selected_ids: HashSet<i32>,
    batch_editor: Entity<TagBatchEditor>,
}

impl ScannerView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let overall_entity = cx.entity().downgrade();
        let batch_editor = cx.new(|cx| {
            TagBatchEditor::new(window, cx)
                .on_commit({
                    let entity = overall_entity.clone();
                    move |ensure, remove, window, cx| {
                        if let Some(entity) = entity.upgrade() {
                            let ensure = ensure.to_vec();
                            let remove = remove.to_vec();
                            entity.update(cx, |this, cx| {
                                this.commit_tag_changes(ensure, remove, window, cx);
                            });
                        }
                    }
                })
                .on_cancel({
                    let entity = overall_entity.clone();
                    move |_window, cx| {
                        if let Some(entity) = entity.upgrade() {
                            entity.update(cx, |this, cx| {
                                this.clear_selection(cx);
                            });
                        }
                    }
                })
        });

        let mut view = Self {
            sources: Vec::new(),
            draft: None,
            is_scanning: false,
            selected_ids: HashSet::new(),
            batch_editor,
        };
        view.reload_sources(cx);
        view
    }

    pub fn start_create_sourse(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ipt = cx.new(|cx| InputState::new(window, cx).placeholder("D:\\Pictures"));
        self.draft = Some(ipt);
    }

    /// Quickly register the dropped directories as scan sources
    pub fn handle_drag(
        this: &mut ScannerView,
        e: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let paths: Vec<PathBuf> = e.paths().iter().filter(|p| p.is_dir()).cloned().collect();

        if paths.is_empty() {
            window.push_notification(Notification::new().message("没有可用的目录"), cx);
            return;
        }

        let task = this.add_sources(paths, cx);

        cx.spawn_in(window, async move |this, cx| {
            let (added, failed) = task.await;

            let msg = this
                .update(cx, |this, cx| {
                    this.reload_sources(cx);
                    if failed == 0 {
                        SharedString::new(format!("已添加 {added} 个扫描源"))
                    } else {
                        SharedString::new(format!("已添加 {added} 个，{failed} 个添加失败"))
                    }
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

    /// Register directories as scan sources.
    pub fn add_sources(&self, paths: Vec<PathBuf>, cx: &mut Context<Self>) -> Task<(usize, usize)> {
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();

        // The inserts are sequential by nature, so one worker for the batch.
        cx.background_spawn(async move {
            rt.block_on(async {
                let mut added = 0usize;
                let mut failed = 0usize;

                for path in &paths {
                    match storage::scan_source::create_scan(
                        &db,
                        path.clone(),
                        ScanMode::Manual,
                        None,
                    )
                    .await
                    {
                        Ok(_) => added += 1,
                        Err(err) => {
                            tracing::warn!(
                                path = %path.display(),
                                error = %err,
                                "failed to add scan source"
                            );
                            failed += 1;
                        }
                    }
                }

                (added, failed)
            })
        })
    }

    /// Reload registered scan sources
    fn reload_sources(&mut self, cx: &mut Context<Self>) {
        let db = cx.global::<AppState>().db.clone();

        cx.spawn(async move |this, cx| {
            let Ok(sources) = storage::scan_source::list_scans(&db).await else {
                return;
            };
            this.update(cx, |this, cx| {
                this.sources = sources;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Register a new source directory
    fn on_add_source_submit(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.draft.take() else {
            return;
        };
        let raw_path = input.read(cx).text().to_string();
        let target = raw_path.trim().trim_matches('"').to_string();
        if target.is_empty() {
            window.push_notification(Notification::new().message("请填写目录"), cx);
            return;
        }

        let task = self.add_sources(vec![PathBuf::from(&target)], cx);

        cx.spawn_in(window, async move |this, cx| {
            let (added, failed) = task.await;
            let succeeded = added > 0 && failed == 0;

            let msg = this
                .update(cx, |this, cx| {
                    this.reload_sources(cx);
                    if succeeded {
                        SharedString::new("添加扫描源成功")
                    } else if added > 0 {
                        SharedString::new(format!("已添加 {added} 个，{failed} 个添加失败"))
                    } else {
                        SharedString::new("添加扫描源失败")
                    }
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

    /// Remove a registered source directory
    fn on_delete_source(&mut self, window: &mut Window, source_id: i32, cx: &mut Context<Self>) {
        self.selected_ids.remove(&source_id);
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();

        cx.spawn_in(window, async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    rt.block_on(storage::scan_source::delete_scan(&db, source_id))
                })
                .await;

            let msg = this
                .update(cx, |this, cx| match res {
                    Ok(()) => {
                        this.reload_sources(cx);
                        SharedString::new("移除扫描源成功")
                    }
                    Err(e) => {
                        tracing::error!("failed to delete scan source: {}", e);
                        SharedString::new("移除扫描源失败")
                    }
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

    /// Trigger a cross-source batch scan
    fn on_scan_all(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_scanning {
            return;
        }

        self.is_scanning = true;
        cx.notify();

        let db = cx.global::<AppState>().db.clone();
        let device_id = cx.global::<AppState>().config.device_id.clone();
        let rt = cx.global::<AppState>().rt.clone();

        cx.spawn_in(window, async move |this, cx| {
            let res = cx
                .background_spawn(
                    async move { rt.block_on(scanner::scan_all_sources(&db, &device_id)) },
                )
                .await;

            let msg = this
                .update(cx, |this, cx| {
                    this.is_scanning = false;
                    // is_scanning is view-only state, nothing else notifies
                    cx.notify();
                    let file = cx.global::<AppState>().file.clone();
                    file.update(cx, |_, cx| cx.notify());
                    match res {
                        Ok(report) if report.failed.is_empty() => SharedString::new("扫描完成"),
                        Ok(report) => SharedString::from(format!(
                            "以下源扫描失败: {}",
                            report.failed.join("、")
                        )),
                        Err(e) => {
                            tracing::error!("failed to scan sources: {}", e);
                            SharedString::new("扫描失败")
                        }
                    }
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

    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        if !self.selected_ids.is_empty() {
            self.selected_ids.clear();
            cx.notify();
        }
    }

    pub fn select_exclusive(&mut self, source_id: i32, cx: &mut Context<Self>) {
        if self.selected_ids.len() == 1 && self.selected_ids.contains(&source_id) {
            self.selected_ids.clear();
        } else {
            self.selected_ids.clear();
            self.selected_ids.insert(source_id);
        }
        self.sync_batch_editor(cx);
    }

    pub fn set_source_selected(&mut self, source_id: i32, selected: bool, cx: &mut Context<Self>) {
        if selected {
            self.selected_ids.insert(source_id);
        } else {
            self.selected_ids.remove(&source_id);
        }
        self.sync_batch_editor(cx);
    }

    pub fn sync_batch_editor(&mut self, cx: &mut Context<Self>) {
        if self.selected_ids.is_empty() {
            cx.notify();
            return;
        }

        let selected_tags: Vec<Vec<MacroTag>> = self
            .sources
            .iter()
            .filter(|s| self.selected_ids.contains(&s.id))
            .map(|s| {
                s.initial_tags
                    .as_ref()
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .map(scanner::sanitize_initial_tags)
                    .unwrap_or_default()
            })
            .collect();

        let tag_slices: Vec<&[MacroTag]> = selected_tags.iter().map(|v| v.as_slice()).collect();

        let pinned_tags = vec![
            MacroTag::State(FileState::Managed),
            MacroTag::State(FileState::Ignored),
        ];

        let candidate_tags = cx
            .global::<AppState>()
            .tags
            .read(cx)
            .cached_tags
            .clone();

        self.batch_editor.update(cx, |editor, cx| {
            editor.set_tags(&tag_slices, &candidate_tags, &pinned_tags, cx);
        });

        cx.notify();
    }

    fn commit_tag_changes(
        &mut self,
        ensure: Vec<MacroTag>,
        remove: Vec<MacroTag>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_ids.is_empty() {
            return;
        }

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();
        let selected_ids: Vec<i32> = self.selected_ids.iter().copied().collect();

        let mut updates: Vec<(i32, Option<serde_json::Value>)> = Vec::new();
        for id in &selected_ids {
            if let Some(source) = self.sources.iter().find(|s| s.id == *id) {
                let current_tags: Vec<MacroTag> = source
                    .initial_tags
                    .as_ref()
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .map(scanner::sanitize_initial_tags)
                    .unwrap_or_default();

                let mut next_tags = current_tags;
                for tag in &ensure {
                    if tag.is_exclusive() {
                        next_tags.retain(|t| !t.is_exclusive());
                        next_tags.push(tag.clone());
                    } else if !next_tags.contains(tag) {
                        next_tags.push(tag.clone());
                    }
                }
                for tag in &remove {
                    next_tags.retain(|t| t != tag);
                }

                let final_tags = scanner::sanitize_initial_tags(next_tags);
                let json_val = if final_tags.is_empty() {
                    None
                } else {
                    serde_json::to_value(&final_tags).ok()
                };
                updates.push((*id, json_val));
            }
        }

        self.clear_selection(cx);

        cx.spawn_in(window, async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    rt.block_on(async {
                        for (source_id, initial_tags) in updates {
                            storage::scan_source::update_scan_initial_tags(
                                &db,
                                source_id,
                                initial_tags,
                            )
                            .await?;
                        }
                        Ok::<(), anyhow::Error>(())
                    })
                })
                .await;

            let msg = this
                .update(cx, |this, cx| match res {
                    Ok(()) => {
                        this.reload_sources(cx);
                        SharedString::new("已更新初始标签")
                    }
                    Err(e) => {
                        tracing::error!("failed to update scan initial tags: {}", e);
                        SharedString::new("更新初始标签失败")
                    }
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

    fn render_source_row(&self, source: &scan_source::Model, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let source_id = source.id;
        let is_selected = self.selected_ids.contains(&source_id);

        let initial_tags: Vec<MacroTag> = source
            .initial_tags
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .map(scanner::sanitize_initial_tags)
            .unwrap_or_default();

        let mut tag_chips = Vec::new();
        if initial_tags.is_empty() {
            tag_chips.push(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("无初始标签")
                    .into_any_element(),
            );
        } else {
            for (idx, tag) in initial_tags.iter().enumerate() {
                tag_chips.push(
                    TagChip::new(
                        SharedString::from(format!("src-tag-{source_id}-{idx}")),
                        tag.display_name(),
                    )
                    .small()
                    .squared()
                    .into_any_element(),
                );
            }
        }

        div()
            .id(format!("scan-source-row-{source_id}"))
            .v_flex()
            .gap_1p5()
            .px_3()
            .py_2()
            .rounded(theme.radius)
            .border_1()
            .border_color(if is_selected {
                theme.border
            } else {
                theme.accent
            })
            .when_else(
                is_selected,
                |this| this.bg(theme.accent.opacity(0.4)),
                |this| {
                    this.bg(theme.background)
                        .hover(|s| s.bg(theme.secondary.opacity(0.4)))
                },
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.select_exclusive(source_id, cx);
            }))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "scan-source-cb-wrap-{source_id}"
                            )))
                            .flex_none()
                            .child(
                                Checkbox::new(SharedString::from(format!(
                                    "scan-source-cb-{source_id}"
                                )))
                                .checked(is_selected)
                                .on_change(cx.listener(
                                    move |this, checked: &bool, _, cx| {
                                        this.set_source_selected(source_id, *checked, cx);
                                    },
                                )),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .font_medium()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(SharedString::new(source.path.as_str())),
                    )
                    .child(
                        Button::new(SharedString::from(format!("scan-source-del-{source_id}")))
                            .small()
                            .ghost()
                            .icon(IconName::Trash)
                            .disabled(self.is_scanning)
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.on_delete_source(w, source_id, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .pl_7()
                    .children(tag_chips),
            )
            .into_any_element()
    }

    fn render_source_list(&self, cx: &mut Context<Self>) -> Vec<impl IntoElement> {
        if self.sources.is_empty() {
            return vec![
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("点击按钮或拖入文件夹登记扫描源")
                    .into_any_element(),
            ];
        }

        self.sources
            .iter()
            .map(|source| self.render_source_row(source, cx).into_any_element())
            .collect::<Vec<AnyElement>>()
    }
}

impl Render for ScannerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.is_scanning;
        let has_selection = !self.selected_ids.is_empty();

        let sub_bar = if has_selection {
            div()
                .id("scanner-batch-editor-container")
                .h(px(34.))
                .w_full()
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(self.batch_editor.clone())
                .into_any_element()
        } else {
            div()
                .h_flex()
                .items_center()
                .w_full()
                .h(px(34.))
                .text_color(cx.theme().muted_foreground)
                .child(
                    div()
                        .flex()
                        .justify_center()
                        .items_center()
                        .bg(cx.theme().muted)
                        .rounded_full()
                        .px_4()
                        .py_1()
                        .text_xs()
                        .font_medium()
                        .child(format!("{} 条来源", self.sources.len())),
                )
                .into_any_element()
        };

        ViewScaffold::new("scanner-scaffold", "扫描管理")
            .on_drop(cx.listener(Self::handle_drag))
            .on_click(cx.listener(|this, _, _, cx| {
                this.clear_selection(cx);
            }))
            .actions(vec![
                Button::new("scan-source-add")
                    .icon(IconName::Plus)
                    .disabled(busy)
                    .on_click(cx.listener(Self::start_create_sourse)),
                Button::new("scan-source-scan-all")
                    .label("立即扫描")
                    .icon(IconName::Search)
                    .loading(busy)
                    .on_click(cx.listener(Self::on_scan_all)),
            ])
            .sub_bar(sub_bar)
            .child(
                div()
                    .v_flex()
                    .w_full()
                    .gap_2()
                    .min_h_0()
                    .px_6()
                    .pb_6()
                    .overflow_y_scrollbar()
                    .when_some(self.draft.as_mut(), |this, draft| {
                        this.child(
                            div()
                                .v_flex()
                                .gap_1p5()
                                .px_3()
                                .py_2()
                                .rounded(cx.theme().radius)
                                .border_1()
                                .border_color(cx.theme().muted)
                                .child(Input::new(&draft).focus_bordered(false))
                                .child(
                                    div()
                                        .w_full()
                                        .h_flex()
                                        .justify_end()
                                        .gap_1()
                                        .child(
                                            Button::new("create-source-cancel")
                                                .icon(IconName::Close)
                                                .on_click(
                                                    cx.listener(|this, _, _, _| this.draft = None),
                                                ),
                                        )
                                        .child(
                                            Button::new("create-source-submit")
                                                .icon(IconName::Check)
                                                .primary()
                                                .on_click(cx.listener(Self::on_add_source_submit)),
                                        ),
                                ),
                        )
                    })
                    .children(self.render_source_list(cx)),
            )
    }
}
