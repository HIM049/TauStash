use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::*;
use gpui_kit::*;
use sea_orm::DatabaseConnection;
use std::sync::Arc;
use uuid::Uuid;

use crate::database::storage::{self, FileInfo};

pub struct DebugApp {
    db: Arc<DatabaseConnection>,
    path_input: Entity<InputState>,
    tag_input: Entity<InputState>,
    current_file: Option<FileInfo>,
    file_list: Vec<FileInfo>,
    status_message: Option<(bool, String)>,
}

impl DebugApp {
    pub fn new(db: Arc<DatabaseConnection>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let path_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("输入本地文件绝对路径 (例如: D:\\file.pdf)...")
        });
        let tag_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("输入标签名称 (例如: 发票, 工作)...")
        });

        let mut app = Self {
            db,
            path_input,
            tag_input,
            current_file: None,
            file_list: Vec::new(),
            status_message: None,
        };

        // Initial load of files
        app.reload_files(cx);
        app
    }

    fn reload_files(&mut self, cx: &mut Context<Self>) {
        let db = self.db.clone();
        cx.spawn(async move |this, cx| {
            let res = storage::list_files(&db).await;
            this.update(cx, |this, cx| {
                if let Ok(files) = res {
                    this.file_list = files;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn on_import(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let raw_path = self.path_input.read(cx).text().to_string();
        let trimmed_path = raw_path.trim().trim_matches('"').to_string();
        if trimmed_path.is_empty() {
            self.status_message = Some((true, "请输入有效的文件路径".to_string()));
            cx.notify();
            return;
        }

        let db = self.db.clone();
        cx.spawn(async move |this, cx| {
            let res = storage::import_file(&db, &trimmed_path, "local-device").await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(info) => {
                        let msg = if info.is_duplicate {
                            format!("发现相同哈希，已记录副本！(ID: {})", info.file_id)
                        } else {
                            format!("新文件导入成功！(ID: {})", info.file_id)
                        };
                        this.status_message = Some((false, msg));
                        this.current_file = Some(info);
                        this.reload_files(cx);
                    }
                    Err(e) => {
                        this.status_message = Some((true, format!("导入失败: {:#}", e)));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn on_add_tag(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(file_info) = &self.current_file else {
            self.status_message = Some((true, "请先选定一个文件".to_string()));
            cx.notify();
            return;
        };

        let file_id = file_info.file_id;
        let tag_name = self.tag_input.read(cx).text().to_string();
        let trimmed_tag = tag_name.trim().to_string();
        if trimmed_tag.is_empty() {
            self.status_message = Some((true, "标签名称不能为空".to_string()));
            cx.notify();
            return;
        }

        let db = self.db.clone();
        cx.spawn(async move |this, cx| {
            let res = storage::add_tag(&db, file_id, &trimmed_tag).await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(tag) => {
                        if let Some(info) = &mut this.current_file
                            && !info.tags.contains(&tag.name)
                        {
                            info.tags.push(tag.name.clone());
                        }

                        this.status_message =
                            Some((false, format!("标签「{}」添加成功！", tag.name)));
                        this.reload_files(cx);
                    }
                    Err(e) => {
                        this.status_message = Some((true, format!("添加标签失败: {:#}", e)));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn on_remove_tag(&mut self, tag_name: String, cx: &mut Context<Self>) {
        let Some(file_info) = &self.current_file else {
            return;
        };
        let file_id = file_info.file_id;
        let db = self.db.clone();

        cx.spawn(async move |this, cx| {
            let res = storage::remove_tag(&db, file_id, &tag_name).await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(_) => {
                        if let Some(info) = &mut this.current_file {
                            info.tags.retain(|t| t != &tag_name);
                        }
                        this.status_message =
                            Some((false, format!("标签「{}」已移除！", tag_name)));
                        this.reload_files(cx);
                    }
                    Err(e) => {
                        this.status_message = Some((true, format!("移除标签失败: {:#}", e)));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn on_delete_file(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(file_info) = &self.current_file else {
            return;
        };
        let file_id = file_info.file_id;
        let name = file_info.file_name.clone();
        let db = self.db.clone();

        cx.spawn(async move |this, cx| {
            let res = storage::delete_file(&db, file_id).await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(_) => {
                        this.current_file = None;
                        this.status_message =
                            Some((false, format!("文件「{}」已从数据库注销！", name)));
                        this.reload_files(cx);
                    }
                    Err(e) => {
                        this.status_message = Some((true, format!("注销文件失败: {:#}", e)));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn on_select_file(&mut self, file_id: Uuid, cx: &mut Context<Self>) {
        if let Some(f) = self.file_list.iter().find(|item| item.file_id == file_id) {
            self.current_file = Some(f.clone());
            self.status_message = Some((false, format!("已选定文件: {}", f.file_name)));
            cx.notify();
        }
    }
}

impl Render for DebugApp {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_file_card = self.current_file.as_ref().map(|info| {
            let tags_view = if info.tags.is_empty() {
                div()
                    .text_xs()
                    .text_color(rgb(0x888888))
                    .child("(暂无标签)")
            } else {
                let tags = info.tags.iter().map(|tag| {
                    let tag_name = tag.clone();
                    div()
                        .h_flex()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py_0p5()
                        .bg(rgb(0x2a374a))
                        .text_color(rgb(0x60a5fa))
                        .rounded_sm()
                        .text_xs()
                        .child(format!("#{}", tag))
                        .child(
                            Button::new(format!("del-tag-{}", tag_name))
                                .label("×")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.on_remove_tag(tag_name.clone(), cx);
                                })),
                        )
                });
                div().flex().flex_wrap().gap_1().children(tags)
            };

            let duplicate_badge = if info.is_duplicate {
                div()
                    .px_2()
                    .py_0p5()
                    .bg(rgb(0x5c3b1e))
                    .text_color(rgb(0xfbbf24))
                    .rounded_sm()
                    .text_xs()
                    .child("重复文件 (去重副本)")
            } else {
                div()
                    .px_2()
                    .py_0p5()
                    .bg(rgb(0x1e4620))
                    .text_color(rgb(0x4ade80))
                    .rounded_sm()
                    .text_xs()
                    .child("库中记录")
            };

            div()
                .v_flex()
                .gap_2()
                .p_4()
                .bg(rgb(0x1f2329))
                .border_1()
                .border_color(rgb(0x333b47))
                .rounded_md()
                .child(
                    div()
                        .h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .font_weight(FontWeight::BOLD)
                                .text_base()
                                .child(info.file_name.clone()),
                        )
                        .child(duplicate_badge),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x9ca3af))
                        .child(format!("路径: {}", info.path)),
                )
                .child(
                    div()
                        .h_flex()
                        .gap_4()
                        .text_xs()
                        .text_color(rgb(0xd1d5db))
                        .child(format!(
                            "大小: {} 字节 ({:.2} MB)",
                            info.size,
                            info.size as f64 / 1024.0 / 1024.0
                        ))
                        .child(format!("MIME: {}", info.mime)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x9ca3af))
                        .child(format!("BLAKE3: {}", info.hash)),
                )
                .child(
                    div()
                        .v_flex()
                        .gap_1()
                        .pt_2()
                        .border_t_1()
                        .border_color(rgb(0x2d3748))
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x9ca3af))
                                .child("标签管理 (点击 × 解绑):"),
                        )
                        .child(tags_view),
                )
                .child(
                    div()
                        .h_flex()
                        .gap_2()
                        .pt_2()
                        .child(div().flex_1().child(Input::new(&self.tag_input)))
                        .child(
                            Button::new("tag-btn")
                                .label("添加 Tag")
                                .on_click(cx.listener(Self::on_add_tag)),
                        ),
                )
                .child(
                    div().h_flex().justify_end().pt_2().child(
                        Button::new("delete-btn")
                            .label("从数据库注销/移除该文件")
                            .on_click(cx.listener(Self::on_delete_file)),
                    ),
                )
        });

        let status_banner = self.status_message.as_ref().map(|(is_error, msg)| {
            let (bg_color, text_color) = if *is_error {
                (rgb(0x451a1a), rgb(0xf87171))
            } else {
                (rgb(0x143823), rgb(0x4ade80))
            };

            div()
                .p_2()
                .bg(bg_color)
                .text_color(text_color)
                .text_xs()
                .rounded_sm()
                .child(msg.clone())
        });

        // File list card
        let file_list_rows: Vec<_> = if self.file_list.is_empty() {
            vec![
                div()
                    .p_3()
                    .text_xs()
                    .text_color(rgb(0x6b7280))
                    .child("库中暂无文件记录，请在上方输入路径导入。")
                    .into_any_element(),
            ]
        } else {
            self.file_list
                .iter()
                .map(|item| {
                    let fid = item.file_id;
                    let is_active = self
                        .current_file
                        .as_ref()
                        .map(|c| c.file_id == fid)
                        .unwrap_or(false);

                    let (bg, border) = if is_active {
                        (rgb(0x263345), rgb(0x3b82f6))
                    } else {
                        (rgb(0x1a1d24), rgb(0x2d3748))
                    };

                    let tags_preview = if item.tags.is_empty() {
                        div().text_xs().text_color(rgb(0x6b7280)).child("(无标签)")
                    } else {
                        let tag_spans = item.tags.iter().map(|t| {
                            div()
                                .px_1p5()
                                .py_0p5()
                                .bg(rgb(0x212a38))
                                .text_color(rgb(0x93c5fd))
                                .rounded_sm()
                                .text_xs()
                                .child(format!("#{}", t))
                        });
                        div().flex().gap_1().children(tag_spans)
                    };

                    div()
                        .h_flex()
                        .justify_between()
                        .items_center()
                        .p_2()
                        .bg(bg)
                        .border_1()
                        .border_color(border)
                        .rounded_sm()
                        .child(
                            div()
                                .v_flex()
                                .gap_0p5()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(item.file_name.clone()),
                                )
                                .child(
                                    div()
                                        .h_flex()
                                        .gap_2()
                                        .text_xs()
                                        .text_color(rgb(0x9ca3af))
                                        .child(format!(
                                            "{:.2} MB",
                                            item.size as f64 / 1024.0 / 1024.0
                                        ))
                                        .child(tags_preview),
                                ),
                        )
                        .child(
                            Button::new(format!("sel-{}", fid))
                                .label(if is_active {
                                    "当前选定"
                                } else {
                                    "查看/选定"
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.on_select_file(fid, cx);
                                })),
                        )
                        .into_any_element()
                })
                .collect()
        };

        div()
            .v_flex()
            .gap_4()
            .size_full()
            .p_6()
            .bg(rgb(0x181a1f))
            .text_color(rgb(0xe5e7eb))
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::BOLD)
                            .child("PiCargo 调试工作台"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x9ca3af))
                            .child("本地文件导入、CAS 查重、标签解绑与已导入列表 (CRUD)"),
                    ),
            )
            .children(status_banner)
            // 1. Import Card
            .child(
                div()
                    .v_flex()
                    .gap_2()
                    .p_4()
                    .bg(rgb(0x21252b))
                    .rounded_md()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("1. 输入本地文件路径导入 (Create):"),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .child(div().flex_1().child(Input::new(&self.path_input)))
                            .child(
                                Button::new("import-btn")
                                    .primary()
                                    .label("导入")
                                    .on_click(cx.listener(Self::on_import)),
                            ),
                    ),
            )
            // 2. Selected File Card (if any)
            .children(current_file_card)
            // 3. File List Card
            .child(
                div()
                    .v_flex()
                    .gap_2()
                    .p_4()
                    .bg(rgb(0x21252b))
                    .rounded_md()
                    .child(
                        div()
                            .h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!(
                                        "已收录文件列表 (共 {} 项):",
                                        self.file_list.len()
                                    )),
                            )
                            .child(
                                Button::new("refresh-btn")
                                    .label("刷新列表")
                                    .on_click(cx.listener(|this, _, _, cx| this.reload_files(cx))),
                            ),
                    )
                    .child(div().v_flex().gap_2().children(file_list_rows)),
            )
    }
}
