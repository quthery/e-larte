use std::path::Path;

pub(crate) fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "bmp"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::is_image;
    use std::path::Path;

    #[test]
    fn recognizes_supported_image_extensions_case_insensitively() {
        assert!(is_image(Path::new("photo.JPEG")));
        assert!(is_image(Path::new("drawing.webp")));
        assert!(!is_image(Path::new("notes.txt")));
    }
}
