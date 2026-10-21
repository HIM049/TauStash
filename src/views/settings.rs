use gpui_kit::base::StyledExt;
use gpui_kit::component::setting::{
    NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings,
};
use gpui_kit::component::{ActiveTheme, Sizable, Size, TitleBar};
use gpui_kit::*;

use crate::AppState;

pub struct SettingsView;

impl SettingsView {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .v_flex()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(TitleBar::new().text_xs().font_semibold().child("Settings"))
            .child(
                div().flex_1().min_h_0().child(
                    Settings::new("picargo-settings")
                        .with_size(Size::Medium)
                        // .with_group_variant(GroupBoxVariant::Outline)
                        .sidebar_width(px(160.))
                        .pages(vec![
                            SettingPage::new("常规设置").default_open(true).groups(vec![
                                SettingGroup::new().title("设备与存储").items(vec![
                                    SettingItem::new(
                                        "设备标识",
                                        // SettingField::input(
                                        //     |cx: &App| {
                                        //         cx.global::<AppState>().config.device_id.clone().into()
                                        //     },
                                        //     |val: SharedString, cx: &mut App| {
                                        //         cx.update_global::<AppState, _>(|state, _| {
                                        //             state.update_config(|cfg| {
                                        //                 cfg.device_id = val.to_string();
                                        //             });
                                        //         });
                                        //     },
                                        // ),
                                        SettingField::render(|_, _, cx| {
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(
                                                    cx.global::<AppState>()
                                                        .config
                                                        .device_id
                                                        .clone()
                                                        .into_element(),
                                                )
                                        }),
                                    ),
                                    // .layout(Axis::Vertical),
                                    SettingItem::new(
                                        "数据库路径",
                                        SettingField::input(
                                            |cx: &App| {
                                                cx.global::<AppState>()
                                                    .config
                                                    .db_path
                                                    .to_string_lossy()
                                                    .to_string()
                                                    .into()
                                            },
                                            |val: SharedString, cx: &mut App| {
                                                cx.update_global::<AppState, _>(|state, _| {
                                                    state.update_config(|cfg| {
                                                        cfg.db_path = std::path::PathBuf::from(
                                                            val.to_string(),
                                                        );
                                                    });
                                                });
                                            },
                                        ),
                                    ),
                                ]),
                                SettingGroup::new().title("后台调度").items(vec![
                                    SettingItem::new(
                                        "启用定时任务",
                                        SettingField::switch(
                                            |cx: &App| {
                                                cx.global::<AppState>().config.schedule_enable
                                            },
                                            |val: bool, cx: &mut App| {
                                                cx.update_global::<AppState, _>(|state, _| {
                                                    state.update_config(|cfg| {
                                                        cfg.schedule_enable = val;
                                                    });
                                                });
                                            },
                                        ),
                                    ),
                                    SettingItem::new(
                                        "增量扫描间隔 (分钟)",
                                        SettingField::number_input(
                                            NumberFieldOptions {
                                                min: 1.0,
                                                max: 1440.0,
                                                ..Default::default()
                                            },
                                            |cx: &App| {
                                                cx.global::<AppState>().config.schedule_scan_min
                                                    as f64
                                            },
                                            |val: f64, cx: &mut App| {
                                                cx.update_global::<AppState, _>(|state, _| {
                                                    state.update_config(|cfg| {
                                                        cfg.schedule_scan_min = val.max(1.0) as u32;
                                                    });
                                                });
                                            },
                                        ),
                                    ),
                                    SettingItem::new(
                                        "元数据维护间隔 (分钟)",
                                        SettingField::number_input(
                                            NumberFieldOptions {
                                                min: 5.0,
                                                max: 10080.0,
                                                ..Default::default()
                                            },
                                            |cx: &App| {
                                                cx.global::<AppState>().config.schedule_maintain_min
                                                    as f64
                                            },
                                            |val: f64, cx: &mut App| {
                                                cx.update_global::<AppState, _>(|state, _| {
                                                    state.update_config(|cfg| {
                                                        cfg.schedule_maintain_min =
                                                            val.max(5.0) as u32;
                                                    });
                                                });
                                            },
                                        ),
                                    ),
                                    SettingItem::new(
                                        "回收站保留时间 (天)",
                                        SettingField::number_input(
                                            NumberFieldOptions {
                                                min: 1.0,
                                                max: 365.0,
                                                ..Default::default()
                                            },
                                            |cx: &App| {
                                                cx.global::<AppState>().config.retention_days
                                                    as f64
                                            },
                                            |val: f64, cx: &mut App| {
                                                cx.update_global::<AppState, _>(|state, _| {
                                                    state.update_config(|cfg| {
                                                        cfg.retention_days = val.max(1.0) as u32;
                                                    });
                                                });
                                            },
                                        ),
                                    ),
                                    SettingItem::new(
                                        "超期清扫频次 (小时)",
                                        SettingField::number_input(
                                            NumberFieldOptions {
                                                min: 1.0,
                                                max: 168.0,
                                                ..Default::default()
                                            },
                                            |cx: &App| {
                                                cx.global::<AppState>().config.schedule_cleanup_hours
                                                    as f64
                                            },
                                            |val: f64, cx: &mut App| {
                                                cx.update_global::<AppState, _>(|state, _| {
                                                    state.update_config(|cfg| {
                                                        cfg.schedule_cleanup_hours =
                                                            val.max(1.0) as u32;
                                                    });
                                                });
                                            },
                                        ),
                                    ),
                                ]),
                                SettingGroup::new().title("调试与性能").items(vec![
                                    SettingItem::new(
                                        "显示 FPS 悬浮窗",
                                        SettingField::switch(
                                            |cx: &App| cx.global::<AppState>().config.fps_monitor,
                                            |val: bool, cx: &mut App| {
                                                cx.update_global::<AppState, _>(|state, _| {
                                                    state.update_config(|cfg| {
                                                        cfg.fps_monitor = val;
                                                    });
                                                });
                                            },
                                        ),
                                    ),
                                ]),
                            ]),
                        ]),
                ),
            )
    }
}
