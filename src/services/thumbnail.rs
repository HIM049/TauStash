use sea_orm::DatabaseConnection;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::database::storage;

/// Target resolution: 360px on the longest edge.
const MAX_THUMBNAIL_EDGE: u32 = 360;

static THUMBNAILS_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let dir = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("app_data")
        .join("thumbnails");
    if !dir.exists() {
        let _ = std::fs::create_dir_all(&dir);
    }
    dir
});

pub fn get_thumbnail_path_ensure(hash: &str) -> PathBuf {
    let thumb_path = get_thumbnail_path(hash);
    if let Some(parent) = thumb_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    thumb_path
}

/// get thumbnail file path
pub fn get_thumbnail_path(hash: &str) -> PathBuf {
    THUMBNAILS_DIR.join(&hash[..2]).join(format!("{hash}.jpg"))
}

/// Check whether thumbnail already exists
pub fn thumbnail_exists(hash: &str) -> bool {
    get_thumbnail_path(hash).is_file()
}

/// Ensure a thumbnail exists
fn generate_thumbnail(src_path: &Path, hash: &str) -> Option<(PathBuf, i64)> {
    let thumb_path = get_thumbnail_path_ensure(hash);

    if !src_path.is_file() {
        warn!(
            "Cannot generate thumbnail: source does not exist: {}",
            src_path.display()
        );
        return None;
    }

    let start = std::time::Instant::now();

    // Read and decode the image
    let img = match image::open(src_path) {
        Ok(img) => img,
        Err(err) => {
            warn!(
                "Failed to decode image for thumbnail ({}): {err}",
                src_path.display()
            );
            return None;
        }
    };

    let thumbnail = img.thumbnail(MAX_THUMBNAIL_EDGE, MAX_THUMBNAIL_EDGE);

    if let Err(err) = thumbnail.save(&thumb_path) {
        warn!(
            "Failed to save thumbnail to {}: {err}",
            thumb_path.display()
        );
        return None;
    }

    let size = std::fs::metadata(&thumb_path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);

    debug!("Generated thumbnail for {} in {:?}", hash, start.elapsed());

    Some((thumb_path, size))
}

pub async fn create_thumbnail(
    db: &DatabaseConnection,
    file_id: Uuid,
    file_hash: &str,
    source: PathBuf,
) -> anyhow::Result<()> {
    if let Some((path, size)) = generate_thumbnail(&source, &file_hash) {
        storage::file::upsert_thumbnail(db, file_id, path, size).await?;
    } else {
        anyhow::bail!("failed to generate thumbnail")
    }

    Ok(())
}
