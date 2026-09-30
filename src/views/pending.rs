use gpui_kit::component::button::Button;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::*;
use gpui_kit::*;

use crate::database::entities::FileState;
use crate::database::storage::file::TagFilter;
use crate::database::storage::{self};
use crate::services::importer::{self, FileInfo};
use crate::{AppState, components};

/// Review queue for newly scanned files.
pub struct PendingView {
    file_list: Vec<FileInfo>,
    scroll_handle: UniformListScrollHandle,
    selected_file_id: Option<uuid::Uuid>,
    /// Rows with an in-flight state write, so their buttons can be disabled.
    pending_ids: Vec<uuid::Uuid>,
    loading: bool,
}

impl PendingView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            file_list: Vec::new(),
            scroll_handle: UniformListScrollHandle::new(),
            selected_file_id: None,
            pending_ids: Vec::new(),
            loading: false,
        };
        view.reload_files(cx);
        view
    }

    /// Reload the pending queue.
    pub fn reload_files(&mut self, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        self.loading = true;

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();

        cx.spawn(async move |this, cx| {
            let files_res = cx
                .background_spawn(async move {
                    rt.block_on(importer::list_files_filtered(
                        &db,
                        &TagFilter::All,
                        Some(FileState::Pending),
                    ))
                })
                .await;

            this.update(cx, |this, cx| {
                this.loading = false;
                if let Ok(files) = files_res {
                    this.file_list = files;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Assign a state to one file and refresh the queue.
    fn mark_state(
        &mut self,
        file_id: uuid::Uuid,
        state: FileState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_ids.contains(&file_id) {
            return;
        }
        self.pending_ids.push(file_id);
        cx.notify();

        let db = cx.global::<AppState>().db.clone();
        let rt = cx.global::<AppState>().rt.clone();

        cx.spawn_in(window, async move |this, cx: &mut AsyncWindowContext| {
            let res = cx
                .background_spawn(async move {
                    rt.block_on(storage::file::set_file_state(&db, file_id, state))
                })
                .await;

            let message = this
                .update(cx, |this, cx| {
                    this.pending_ids.retain(|id| *id != file_id);
                    if this.selected_file_id == Some(file_id) {
                        this.selected_file_id = None;
                    }
                    match res {
                        Ok(()) => {
                            this.reload_files(cx);
                            None
                        }
                        Err(err) => {
                            let msg = format!("标记失败: {err:#}");
                            cx.notify();
                            Some(msg)
                        }
                    }
                })
                .ok()
                .flatten();

            if let Some(msg) = message {
                cx.update(|w, cx| {
                    w.push_notification(Notification::new().message(msg), cx);
                })
                .ok();
            }
        })
        .detach();
    }
}

impl Render for PendingView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let total = self.file_list.len();
        let rows_count = total;
        let entity = cx.entity().clone();
        let selected_id = self.selected_file_id;
        let pending_ids = self.pending_ids.clone();

        let header = div()
            .h_flex()
            .items_center()
            .gap_4()
            .px_6()
            .pt_6()
            .pb_3()
            .child(div().text_xl().font_bold().child("审核队列"))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("待审核 {total} 项")),
            )
            .child(
                Button::new("pending-refresh")
                    .icon(IconName::RefreshCw)
                    .on_click(cx.listener(|this, _, _, cx| this.reload_files(cx))),
            );

        let list_area = if rows_count == 0 {
            div()
                .h_flex()
                .items_center()
                .justify_center()
                .size_full()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(if self.loading {
                    "加载中..."
                } else {
                    "没有待审核的文件"
                })
        } else {
            let rows = self.file_list.clone();
            div()
                .relative()
                .size_full()
                .child(
                    uniform_list("pending-list", rows_count, move |visible_range, _, _| {
                        visible_range
                            .map(|row_idx| {
                                let item = &rows[row_idx];
                                let extension = item
                                    .ext
                                    .as_deref()
                                    .unwrap_or_else(|| {
                                        item.mime.split('/').nth(1).unwrap_or("file")
                                    })
                                    .to_uppercase();

                                let file_id = item.file_id;
                                let is_selected = selected_id == Some(file_id);
                                let is_pending = pending_ids.contains(&file_id);
                                let file_path = SharedString::new(item.path.as_str());
                                let path_open = file_path.clone();
                                let entity_click = entity.clone();
                                let entity_action = entity.clone();

                                components::file_row::FileRow::new(
                                    ElementId::Name(format!("pending-row-{file_id}").into()),
                                    SharedString::new(item.file_name.as_str()),
                                    extension,
                                    item.size,
                                )
                                .path(file_path)
                                .hash(SharedString::new(item.hash.as_str()))
                                .selected(is_selected)
                                .busy(is_pending)
                                .on_click(move |_, _, cx| {
                                    entity_click.update(cx, |this, cx| {
                                        this.selected_file_id = Some(file_id);
                                        cx.notify();
                                    });
                                })
                                .on_double_click(move |_, _, _| {
                                    components::utils::open_path(path_open.as_ref());
                                })
                                .on_action(
                                    move |state, action_window, cx| {
                                        entity_action.update(cx, |this, cx| {
                                            this.mark_state(file_id, state, action_window, cx);
                                        });
                                    },
                                )
                            })
                            .collect()
                    })
                    .size_full()
                    .track_scroll(&self.scroll_handle),
                )
                .vertical_scrollbar(&self.scroll_handle)
        };

        div().v_flex().size_full().child(header).child(
            div()
                .flex_1()
                .min_h_0()
                .px_6()
                .pb_6()
                .v_flex()
                .child(list_area),
        )
    }
}
