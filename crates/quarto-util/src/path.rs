//! Cross-platform path utilities

use std::path::Path;

/// Returns `true` if `path` has a root component.
///
/// Use this, not [`std::path::Path::is_absolute`], for "should this path be
/// used as-is, or resolved against a working directory?" decisions. On
/// `wasm32-unknown-unknown` (no `target_family`) `is_absolute()` returns
/// `false` even for rooted paths like `/foo`, whereas `has_root()` is correct
/// on both native and WASM targets. Same rationale as `quarto-core`'s
/// `artifact.rs` / `output_sink.rs` (bd-cfl67).
pub fn is_rooted(path: &Path) -> bool {
    path.has_root()
}

/// Convert a path to a string using forward slashes only.
///
/// Windows paths like `C:\Users\chris\file.txt` become `C:/Users/chris/file.txt`.
/// On Unix, this is a no-op since paths already use forward slashes.
/// Forward slashes are accepted by Windows APIs, making this safe for file operations.
pub fn to_forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_forward_slashes_preserves_unix_paths() {
        let path = PathBuf::from("relative/path/file.txt");
        assert_eq!(to_forward_slashes(&path), "relative/path/file.txt");
    }

    #[cfg(windows)]
    #[test]
    fn test_forward_slashes_converts_windows_paths() {
        // Use a real OS-provided path that naturally contains backslashes
        let temp = std::env::temp_dir().join("test_file.txt");
        let result = to_forward_slashes(&temp);
        assert!(
            !result.contains('\\'),
            "Expected no backslashes, got: {result}"
        );
        assert!(
            result.contains('/'),
            "Expected forward slashes, got: {result}"
        );
    }

    #[test]
    fn test_is_rooted_distinguishes_rooted_from_relative() {
        assert!(is_rooted(Path::new("/abs/file.txt")));
        assert!(!is_rooted(Path::new("relative/file.txt")));
    }

    #[cfg(windows)]
    #[test]
    fn test_is_rooted_recognizes_windows_drive_paths() {
        // Guards against regressing to a `starts_with('/')`-style check, which
        // would wrongly report a drive-rooted path as not rooted.
        assert!(is_rooted(Path::new("C:/abs/file.txt")));
    }
}
