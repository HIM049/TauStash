use std::path::Path;

const UNKNOWN: &str = "application/octet-stream";
const PLAIN_TEXT: &str = "text/plain";

/// Tht types which shared by several formats. effect mix rule.
const CONTAINER_TYPES: &[&str] = &[
    "application/zip",
    "application/x-ole-storage",
    "audio/ogg",
    "video/x-ms-wmv",
];

/// Detect a file's MIME type. Mixed result from mime_guess and infer.
pub fn detect(path: &Path, head: &[u8]) -> String {
    let guess = mime_guess::from_path(path)
        .first()
        .map(|m| m.to_string())
        .filter(|m| m != UNKNOWN)
        .unwrap_or_default();

    let is_text = !head.is_empty() && !head.contains(&0);
    let sniffed = infer::get(head)
        .map(|kind| kind.mime_type())
        .filter(|m| *m != UNKNOWN);

    // for text files, trust file ext first
    let (sniffed, text_hint) = match sniffed {
        Some(m) if m.starts_with("text/") => (None, Some(m)),
        other => (other, None),
    };

    if let Some(sniffed) = sniffed {
        let prefer_guess =
            CONTAINER_TYPES.contains(&sniffed) || (is_text && guess.starts_with("text/"));
        return if !guess.is_empty() && prefer_guess {
            guess
        } else {
            sniffed.to_string()
        };
    }

    if is_text && (guess.is_empty() || guess.starts_with("video/") || guess.starts_with("audio/")) {
        return text_hint.unwrap_or(PLAIN_TEXT).to_string();
    }

    if guess.is_empty() {
        text_hint.unwrap_or(UNKNOWN).to_string()
    } else {
        guess
    }
}
