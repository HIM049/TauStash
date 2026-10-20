use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tracing::{debug, warn};

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

/// Directory where thumbnail images are stored
pub fn thumbnails_dir() -> &'static Path {
    &THUMBNAILS_DIR
}

/// get thumbnail file path
pub fn get_thumbnail_path(hash: &str) -> PathBuf {
    THUMBNAILS_DIR.join(format!("{hash}.jpg"))
}

/// Check whether thumbnail already exists
pub fn thumbnail_exists(hash: &str) -> bool {
    get_thumbnail_path(hash).is_file()
}

/// Ensure a thumbnail exists
pub fn ensure_thumbnail(src_path: &Path, hash: &str) -> Option<PathBuf> {
    let thumb_path = get_thumbnail_path(hash);
    if thumb_path.is_file() {
        return Some(thumb_path);
    }

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

    debug!("Generated thumbnail for {} in {:?}", hash, start.elapsed());

    Some(thumb_path)
}
