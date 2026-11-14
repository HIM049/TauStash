use gpui_kit::base::StyledExt;
use gpui_kit::component::setting::{
    NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings,
};
use gpui_kit::component::{ActiveTheme, Sizable, Size, TitleBar};
use gpui_kit::*;

use crate::AppState;
use crate::config::{StoreAutoMode, ThemeMode};
use rust_i18n::t;

/// Locale codes with their names in that language
const LOCALES: &[(&str, &str)] = &[("zh-CN", "简体中文"), ("en", "English")];

/// Single-line text field whose text line fits inside the Medium frame.
fn text_input(
    value: impl Fn(&App) -> SharedString + 'static,
    set_value: impl Fn(SharedString, &mut App) + 'static,
) -> SettingField<SharedString> {
    SettingField::input(value, set_value).py_1()
}

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
            .child(
                TitleBar::new()
                    .text_xs()
                    .font_semibold()
                    .child(SharedString::from(t!("settings.title"))),
            )
            .child(
                div().flex_1().min_h_0().child(
                    Settings::new("taustash-settings")
                        .with_size(Size::Medium)
                        // .with_group_variant(GroupBoxVariant::Outline)
                        .sidebar_width(px(160.))
                        .pages(vec![
                            SettingPage::new(t!("settings.page_general"))
                                .default_open(true)
                                .groups(vec![
                                    SettingGroup::new()
                                        .pr_1()
                                        .title(t!("settings.group_interface"))
                                        .items(vec![
                                            SettingItem::new(
                                                t!("settings.language"),
                                                SettingField::dropdown(
                                                    LOCALES
                                                        .iter()
                                                        .map(|(code, name)| {
                                                            ((*code).into(), (*name).into())
                                                        })
                                                        .collect(),
                                                    |cx: &App| {
                                                        cx.global::<AppState>()
                                                            .config
                                                            .locale
                                                            .clone()
                                                            .into()
                                                    },
                                                    |val: SharedString, cx: &mut App| {
                                                        rust_i18n::set_locale(&val);
                                                        cx.update_global::<AppState, _>(
                                                            |state, _| {
                                                                state.update_config(|cfg| {
                                                                    cfg.locale = val.to_string();
                                                                });
                                                            },
                                                        );
                                                        cx.refresh_windows();
                                                    },
                                                ),
                                            ),
                                            SettingItem::new(
                                                t!("settings.appearance"),
                                                SettingField::dropdown(
                                                    ThemeMode::ALL
                                                        .iter()
                                                        .map(|mode| {
                                                            (
                                                                mode.as_str().into(),
                                                                t!(format!(
                                                                    "settings.appearance_{}",
                                                                    mode.as_str()
                                                                ))
                                                                .into(),
                                                            )
                                                        })
                                                        .collect(),
                                                    |cx: &App| {
                                                        cx.global::<AppState>()
                                                            .config
                                                            .theme_mode
                                                            .as_str()
                                                            .into()
                                                    },
                                                    |val: SharedString, cx: &mut App| {
                                                        let mode = ThemeMode::from_str(&val);
                                                        cx.update_global::<AppState, _>(
                                                            |state, _| {
                                                                state.update_config(|cfg| {
                                                                    cfg.theme_mode = mode;
                                                                });
                                                            },
                                                        );
                                                        crate::theme::apply(mode, None, cx);
                                                        cx.refresh_windows();
                                                    },
                                                ),
                                            ),
                                            SettingItem::new(
                                                t!("settings.check_update"),
                                                SettingField::switch(
                                                    |cx: &App| {
                                                        cx.global::<AppState>().config.check_update
                                                    },
                                                    |val: bool, cx: &mut App| {
                                                        cx.update_global::<AppState, _>(
                                                            |state, _| {
                                                                state.update_config(|cfg| {
                                                                    cfg.check_update = val;
                                                                });
                                                            },
                                                        );
                                                    },
                                                ),
                                            ),
                                        ]),
                                    SettingGroup::new()
                                        .pr_1()
                                        .title(t!("settings.group_device"))
                                        .items(vec![
                                            SettingItem::new(
                                                t!("settings.device_id"),
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
                                                t!("settings.db_path"),
                                                text_input(
                                                    |cx: &App| {
                                                        cx.global::<AppState>()
                                                            .config
                                                            .db_path
                                                            .to_string_lossy()
                                                            .to_string()
                                                            .into()
                                                    },
                                                    |val: SharedString, cx: &mut App| {
                                                        cx.update_global::<AppState, _>(
                                                            |state, _| {
                                                                state.update_config(|cfg| {
                                                                    cfg.db_path =
                                                                        std::path::PathBuf::from(
                                                                            val.to_string(),
                                                                        );
                                                                });
                                                            },
                                                        );
                                                    },
                                                ),
                                            ),
                                        ]),
                                    SettingGroup::new()
                                        .pr_1()
                                        .title(t!("settings.group_store"))
                                        .items(vec![
                                            SettingItem::new(
                                                t!("settings.store_enable"),
                                                SettingField::switch(
                                                    |cx: &App| {
                                                        cx.global::<AppState>().config.store_enable
                                                    },
                                                    |val: bool, cx: &mut App| {
                                                        cx.update_global::<AppState, _>(
                                                            |state, _| {
                                                                state.update_config(|cfg| {
                                                                    cfg.store_enable = val;
                                                                });
                                                            },
                                                        );
                                                    },
                                                ),
                                            ),
                                            SettingItem::new(
                                                t!("settings.store_auto"),
                                                SettingField::dropdown(
                                                    StoreAutoMode::ALL
                                                        .iter()
                                                        .map(|mode| {
                                                            (
                                                                mode.as_str().into(),
                                                                t!(format!(
                                                                    "settings.store_auto_{}",
                                                                    mode.as_str()
                                                                ))
                                                                .into(),
                                                            )
                                                        })
                                                        .collect(),
                                                    |cx: &App| {
                                                        cx.global::<AppState>()
                                                            .config
                                                            .store_auto
                                                            .as_str()
                                                            .into()
                                                    },
                                                    |val: SharedString, cx: &mut App| {
                                                        cx.update_global::<AppState, _>(
                                                            |state, _| {
                                                                state.update_config(|cfg| {
                                                                    cfg.store_auto =
                                                                        StoreAutoMode::from_str(
                                                                            &val,
                                                                        );
                                                                });
                                                            },
                                                        );
                                                    },
                                                ),
                                            )
                                            .description(SharedString::from(t!(
                                                "settings.store_auto_desc"
                                            ))),
                                            SettingItem::new(
                                                t!("settings.store_path"),
                                                text_input(
                                                    |cx: &App| {
                                                        cx.global::<AppState>()
                                                            .config
                                                            .store_path
                                                            .to_string_lossy()
                                                            .to_string()
                                                            .into()
                                                    },
                                                    |val: SharedString, cx: &mut App| {
                                                        cx.update_global::<AppState, _>(
                                                            |state, _| {
                                                                state.update_config(|cfg| {
                                                                    cfg.store_path =
                                                                        std::path::PathBuf::from(
                                                                            val.to_string(),
                                                                        );
                                                                });
                                                            },
                                                        );
                                                    },
                                                ),
                                            ),
                                        ]),
                                    SettingGroup::new()
                                        .pr_1()
                                        .title(t!("settings.group_schedule"))
                                        .items(vec![
                                            SettingItem::new(
                                                t!("settings.schedule_enable"),
                                                SettingField::switch(
                                                    |cx: &App| {
                                                        cx.global::<AppState>()
                                                            .config
                                                            .schedule_enable
                                                    },
                                                    |val: bool, cx: &mut App| {
                                                        cx.update_global::<AppState, _>(
                                                            |state, _| {
                                                                state.update_config(|cfg| {
                                                                    cfg.schedule_enable = val;
                                                                });
                                                            },
                                                        );
                                                    },
                                                ),
                                            ),
                                            SettingItem::new(
                                                t!("settings.scan_interval"),
                                                SettingField::number_input(
                                                    NumberFieldOptions {
                                                        min: 1.0,
                                                        max: 1440.0,
                                                        ..Default::default()
                                                    },
                                                    |cx: &App| {
                                                        cx.global::<AppState>()
                                                            .config
                                                            .schedule_scan_min
                                                            as f64
                                                    },
                                                    |val: f64, cx: &mut App| {
                                                        cx.update_global::<AppState, _>(
                                                            |state, _| {
                                                                state.update_config(|cfg| {
                                                                    cfg.schedule_scan_min =
                                                                        val.max(1.0) as u32;
                                                                });
                                                            },
                                                        );
                                                    },
                                                ),
                                            ),
                                            SettingItem::new(
                                                t!("settings.maintain_interval"),
                                                SettingField::number_input(
                                                    NumberFieldOptions {
                                                        min: 5.0,
                                                        max: 10080.0,
                                                        ..Default::default()
                                                    },
                                                    |cx: &App| {
                                                        cx.global::<AppState>()
                                                            .config
                                                            .schedule_maintain_min
                                                            as f64
                                                    },
                                                    |val: f64, cx: &mut App| {
                                                        cx.update_global::<AppState, _>(
                                                            |state, _| {
                                                                state.update_config(|cfg| {
                                                                    cfg.schedule_maintain_min =
                                                                        val.max(5.0) as u32;
                                                                });
                                                            },
                                                        );
                                                    },
                                                ),
                                            ),
                                            SettingItem::new(
                                                t!("settings.retention_days"),
                                                SettingField::number_input(
                                                    NumberFieldOptions {
                                                        min: 1.0,
                                                        max: 365.0,
                                                        ..Default::default()
                                                    },
                                                    |cx: &App| {
                                                        cx.global::<AppState>()
                                                            .config
                                                            .retention_days
                                                            as f64
                                                    },
                                                    |val: f64, cx: &mut App| {
                                                        cx.update_global::<AppState, _>(
                                                            |state, _| {
                                                                state.update_config(|cfg| {
                                                                    cfg.retention_days =
                                                                        val.max(1.0) as u32;
                                                                });
                                                            },
                                                        );
                                                    },
                                                ),
                                            ),
                                            SettingItem::new(
                                                t!("settings.cleanup_interval"),
                                                SettingField::number_input(
                                                    NumberFieldOptions {
                                                        min: 1.0,
                                                        max: 168.0,
                                                        ..Default::default()
                                                    },
                                                    |cx: &App| {
                                                        cx.global::<AppState>()
                                                            .config
                                                            .schedule_cleanup_hours
                                                            as f64
                                                    },
                                                    |val: f64, cx: &mut App| {
                                                        cx.update_global::<AppState, _>(
                                                            |state, _| {
                                                                state.update_config(|cfg| {
                                                                    cfg.schedule_cleanup_hours =
                                                                        val.max(1.0) as u32;
                                                                });
                                                            },
                                                        );
                                                    },
                                                ),
                                            ),
                                        ]),
                                    SettingGroup::new()
                                        .pr_1()
                                        .title(t!("settings.group_debug"))
                                        .items(vec![
                                            SettingItem::new(
                                                t!("settings.fps_monitor"),
                                                SettingField::switch(
                                                    |cx: &App| {
                                                        cx.global::<AppState>().config.fps_monitor
                                                    },
                                                    |val: bool, cx: &mut App| {
                                                        cx.update_global::<AppState, _>(
                                                            |state, _| {
                                                                state.update_config(|cfg| {
                                                                    cfg.fps_monitor = val;
                                                                });
                                                            },
                                                        );
                                                    },
                                                ),
                                            ),
                                            SettingItem::new(
                                                t!("settings.debug_log"),
                                                SettingField::switch(
                                                    |cx: &App| {
                                                        cx.global::<AppState>().config.debug_log
                                                    },
                                                    |val: bool, cx: &mut App| {
                                                        crate::logging::set_debug(val);
                                                        cx.update_global::<AppState, _>(
                                                            |state, _| {
                                                                state.update_config(|cfg| {
                                                                    cfg.debug_log = val;
                                                                });
                                                            },
                                                        );
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
