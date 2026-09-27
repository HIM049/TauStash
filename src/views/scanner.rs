use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::*;
use gpui_kit::*;

use crate::AppState;
use crate::services::scanner;

pub struct ScannerView {
    is_scanning: bool,
    scan_dir_input: Entity<InputState>,
    status_message: Option<(bool, String)>,
}

impl ScannerView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let scan_dir_input = cx.new(|cx| InputState::new(window, cx));
        Self {
            is_scanning: false,
            scan_dir_input,
            status_message: None,
        }
    }

    fn on_scan_dir(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_scanning {
            return;
        }

        let raw_path = self.scan_dir_input.read(cx).text().to_string();
        let trimmed_path = raw_path.trim().trim_matches('"').to_string();
        if trimmed_path.is_empty() {
            self.status_message = Some((true, "请输入待扫描的源目录绝对路径".to_string()));
            cx.notify();
            return;
        }

        self.is_scanning = true;
        self.status_message = Some((false, format!("正在扫描目录「{}」中...", trimmed_path)));
        cx.notify();

        let db = cx.global::<AppState>().db.clone();
        cx.spawn(async move |this, cx| {
            let res = scanner::scan_directory(&db, &trimmed_path, "local-device").await;
            this.update(cx, |this, cx| {
                this.is_scanning = false;
                match res {
                    Ok(report) => {
                        this.status_message = Some((
                            false,
                            format!(
                                "扫描完成！共发现 {} 项，快速跳过未修改 {} 项，新增导入 {} 项，标记丢失 {} 项",
                                report.total_found, report.skipped_unchanged, report.newly_imported, report.marked_missing
                            ),
                        ));
                    }
                    Err(e) => {
                        this.status_message = Some((true, format!("扫描失败: {:#}", e)));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Render for ScannerView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .gap_4()
            .size_full()
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::BOLD)
                            .child("扫描中心 (Reconciliation Scanner)"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("主动周期性递归扫描指定目录，根据 mtime/size 极速比对并对齐增删状态。"),
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
                            .child("来源目录绝对路径:"),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .child(div().flex_1().child(Input::new(&self.scan_dir_input)))
                            .child(
                                Button::new("scan-btn")
                                    .label(if self.is_scanning {
                                        "扫描中..."
                                    } else {
                                        "开始扫描"
                                    })
                                    .disabled(self.is_scanning)
                                    .on_click(cx.listener(Self::on_scan_dir)),
                            ),
                    ),
            )
    }
}
