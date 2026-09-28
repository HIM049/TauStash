pub fn standardize_path(path: String) -> String {
    // "\\?\C:\Users\me\Pictures" "\\?\UNC\nas\media\photos"
    if let Some(path) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{path}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(&path).to_string()
    }
}
