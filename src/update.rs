use std::sync::Arc;

use anyhow::Context;
use futures::AsyncReadExt;
use gpui_kit::component::WindowExt;
use gpui_kit::{
    http_client::{AsyncBody, HttpClient},
    *,
};
use rust_i18n::t;
use semver::Version;
use serde_json::Value;

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/HIM049/TauStash/releases/latest";

/// Return the release page URL when GitHub has a newer version than this build.
pub async fn check_update(
    http_client: Arc<dyn HttpClient>,
) -> anyhow::Result<Option<SharedString>> {
    let mut result = http_client
        .get(LATEST_RELEASE_URL, AsyncBody::empty(), true)
        .await?;

    if !result.status().is_success() {
        anyhow::bail!("GitHub API returned {}", result.status());
    }

    let mut body = String::new();
    result
        .body_mut()
        .read_to_string(&mut body)
        .await
        .context("failed to read response")?;

    let json: Value = serde_json::from_str(&body).context("failed to read json")?;
    let latest_tag = json["tag_name"]
        .as_str()
        .context("failed to read tag name")?;
    let url = json["html_url"]
        .as_str()
        .context("failed to read release url")?;

    let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
    let latest = Version::parse(latest_tag.trim_start_matches('v'))?;

    if latest > current {
        Ok(Some(SharedString::new(url)))
    } else {
        Ok(None)
    }
}

/// Ask whether to open the release page for the new version.
pub fn show_update_dialog(window: AnyWindowHandle, url: SharedString, cx: &mut AsyncApp) {
    let _ = window.update(cx, move |_, window, cx| {
        window.open_alert_dialog(cx, move |alert, _, _| {
            let url = url.clone();
            alert
                .title(t!("update_dialog.title"))
                .description(t!("update_dialog.description"))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    cx.open_url(url.as_ref());
                    true
                })
        });
    });
}
