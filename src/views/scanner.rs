use std::path::PathBuf;

use gpui_kit::component::button::*;
use gpui_kit::component::empty::{
    Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle,
};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::AppState;
use crate::database::entities::{ScanMode, scan_source};
use crate::database::storage::scan_source as scan_storage;
use crate::services::scanner;

pub struct ScannerView {
    sources: Vec<scan_source::Model>,
    scan_dir_input: Entity<InputState>,
    /// Batch scan in progress, all controls are disabled
    is_scanning: bool,
}

impl ScannerView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let scan_dir_input = cx.new(|cx| InputState::new(window, cx).placeholder("D:\\Pictures"));

        let mut view = Self {
            sources: Vec::new(),
            scan_dir_input,
            is_scanning: false,
        };
        view.reload_sources(cx);
        view
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
                    match scan_storage::create_scan(&db, path.clone(), ScanMode::Manual).await {
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
            let Ok(sources) = scan_storage::list_scans(&db).await else {
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
    fn on_add_source(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let raw_path = self.scan_dir_input.read(cx).text().to_string();
        let target = raw_path.trim().trim_matches('"').to_string();
        if target.is_empty() {
            window.push_notification(Notification::new().message("请填写目录"), cx);
            return;
        }

        let task = self.add_sources(vec![PathBuf::from(&target)], cx);
        let input = self.scan_dir_input.clone();

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
                    if succeeded {
                        input.update(cx, |input, input_cx| {
                            input.set_value("", w, input_cx);
                        });
                    }
                    w.push_notification(Notification::new().message(msg), cx);
                })
                .unwrap();
            }
        })
        .detach();
    }

    /// Remove a registered source directory
    fn on_delete_source(&mut self, window: &mut Window, source_id: i32, cx: &mut Context<Self>) {
        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();

        cx.spawn_in(window, async move |this, cx| {
            let res = cx
                .background_spawn(
                    async move { rt.block_on(scan_storage::delete_scan(&db, source_id)) },
                )
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
        let device_id = cx.global::<AppState>().device_id.clone();
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

    fn render_source_row(
        &self,
        source: &scan_source::Model,
        has_divider: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let source_id = source.id;

        let delete_entity = cx.entity().clone();
        let delete_id = source_id;

        div()
            .h_flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .when(has_divider, |d| d.border_t_1().border_color(border))
            .child(
                Icon::new(IconName::Folder)
                    .size_4()
                    .flex_none()
                    .text_color(muted),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(SharedString::new(source.path.as_str())),
            )
            .child(
                Button::new(SharedString::from(format!("scan-source-del-{source_id}")))
                    .small()
                    .ghost()
                    .label("删除")
                    .disabled(self.is_scanning)
                    .on_click(move |_, w, cx| {
                        delete_entity
                            .update(cx, |this, cx| this.on_delete_source(w, delete_id, cx));
                    }),
            )
            .into_any_element()
    }

    fn render_source_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.sources.is_empty() {
            return Empty::new()
                .h_full()
                .header(
                    EmptyHeader::new()
                        .media(
                            EmptyMedia::new()
                                .with_variant(EmptyMediaVariant::Icon)
                                .child(Icon::new(IconName::Inbox).size_5()),
                        )
                        .title(EmptyTitle::new().child("尚未登记扫描源"))
                        .description(EmptyDescription::new().child(
                            "在上方填入要监控的目录绝对路径，扫描器会按修改时间与大小做增量比对。",
                        )),
                )
                .into_any_element();
        }

        let rows: Vec<AnyElement> = self
            .sources
            .iter()
            .enumerate()
            .map(|(index, source)| self.render_source_row(source, index > 0, cx))
            .collect();

        v_flex().w_full().children(rows).into_any_element()
    }
}

impl Render for ScannerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let busy = self.is_scanning;

        div()
            .id("scanner-main")
            .on_drop(cx.listener(Self::handle_drag))
            .v_flex()
            .size_full()
            .gap_4()
            .p_6()
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::BOLD)
                            .child("扫描源"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("按目录登记来源，扫描器按修改时间与大小做增量比对。"),
                    ),
            )
            .child(
                div()
                    .v_flex()
                    .gap_2()
                    .p_4()
                    .bg(cx.theme().secondary)
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(cx.theme().radius)
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("源目录绝对路径"),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .child(Input::new(&self.scan_dir_input).disabled(busy)),
                            )
                            .child(
                                Button::new("scan-source-add")
                                    .label("添加源")
                                    .icon(IconName::Plus)
                                    .disabled(busy)
                                    .on_click(cx.listener(Self::on_add_source)),
                            ),
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("已登记的源 ({})", self.sources.len())),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("scan-source-scan-all")
                            .label("开始扫描")
                            .icon(IconName::RotateCw)
                            // loading is already inert, do not also set disabled
                            .loading(busy)
                            .on_click(cx.listener(Self::on_scan_all)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_y_scrollbar()
                    .child(self.render_source_list(cx)),
            )
    }
}
