//! File exclusion patterns for synchronization filtering.
//!
//! Manages glob patterns stored in `.rahzomignore` file in the root of sync folder.
//! The file syncs naturally between sides like any other file.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};

/// Exclusions file name (in root directory)
const EXCLUSIONS_FILE: &str = ".rahzomignore";

/// Result of comparing two exclusion sets
#[derive(Debug, Clone)]
pub struct ExclusionsDiff {
    /// Patterns only in left
    pub only_left: Vec<String>,
    /// Patterns only in right
    pub only_right: Vec<String>,
    /// Whether the sets are identical
    pub is_same: bool,
}

/// Manages file exclusion patterns for a sync folder.
///
/// Patterns are stored in `.rahzomignore` with one pattern per line.
/// Supports glob syntax with `*`, `**`, `?`, `[abc]`, `{a,b}` patterns.
/// Directory patterns end with `/` and match the directory and all its contents.
#[derive(Debug, Clone)]
pub struct Exclusions {
    /// Raw pattern strings (for display)
    patterns: Vec<String>,
    /// Compiled glob matcher for efficient matching
    matcher: GlobSet,
}

impl Default for Exclusions {
    fn default() -> Self {
        Self::empty()
    }
}

impl Exclusions {
    /// Creates empty exclusions (no patterns).
    pub fn empty() -> Self {
        Self {
            patterns: Vec::new(),
            matcher: GlobSet::empty(),
        }
    }

    /// Creates exclusions from a list of pattern strings.
    pub fn from_patterns(patterns: &[String]) -> Result<Self> {
        let filtered: Vec<String> = patterns
            .iter()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty() && !p.starts_with('#'))
            .collect();

        let matcher = Self::compile_patterns(&filtered)?;

        Ok(Self {
            patterns: filtered,
            matcher,
        })
    }

    /// Loads exclusions from `.rahzomignore` in the given directory.
    /// Returns empty exclusions if file doesn't exist.
    pub fn load(root: &Path) -> Result<Self> {
        let path = Self::file_path(root);

        if !path.exists() {
            return Ok(Self::empty());
        }

        let file = File::open(&path)
            .with_context(|| format!("Failed to open exclusions file: {:?}", path))?;

        let reader = BufReader::new(file);
        let mut patterns = Vec::new();

        for line in reader.lines() {
            let line = line.with_context(|| "Failed to read exclusions file")?;
            let trimmed = line.trim();

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            patterns.push(trimmed.to_string());
        }

        Self::from_patterns(&patterns)
    }

    /// Returns path to the exclusions file (.rahzomignore in root).
    pub fn file_path(root: &Path) -> PathBuf {
        root.join(EXCLUSIONS_FILE)
    }

    /// Returns the default template content with patterns and syntax comments.
    pub fn default_template() -> String {
        r#"# Rahzom exclusion patterns
# One pattern per line, supports glob syntax:
#   *       - matches any characters except path separator
#   **      - matches any characters including path separator
#   ?       - matches single character
#   [abc]   - matches character class
#   {a,b}   - matches alternatives
#   dir/    - trailing / indicates directory-only pattern

# Temporary files
*.tmp
*.temp
~*
*~

# OS files
.DS_Store
Thumbs.db
desktop.ini
ehthumbs.db

# Version control
.git/
.svn/
.hg/

# Dependencies & build
node_modules/
__pycache__/
*.pyc
.cache/
target/
build/
dist/

# IDE
.idea/
.vscode/
*.swp
*.swo
"#
        .to_string()
    }

    /// Checks if a relative path should be excluded.
    ///
    /// The `is_dir` parameter should be true for directories.
    /// Directory patterns (ending with `/`) only match directories.
    pub fn is_excluded(&self, path: &Path, is_dir: bool) -> bool {
        // Normalize path separators to forward slashes for matching
        let path_str = path.to_string_lossy().replace('\\', "/");

        // Check the path itself
        if self.matcher.is_match(&path_str) {
            return true;
        }

        // For directories, also check with trailing /
        if is_dir {
            let dir_path = format!("{}/", path_str);
            if self.matcher.is_match(&dir_path) {
                return true;
            }
        }

        // Check if any parent directory is excluded
        // This handles cases like "node_modules/" excluding "node_modules/lodash/index.js"
        let mut current = Path::new(&path_str);
        while let Some(parent) = current.parent() {
            if parent.as_os_str().is_empty() {
                break;
            }
            let parent_str = parent.to_string_lossy();
            let parent_dir = format!("{}/", parent_str);
            if self.matcher.is_match(parent_dir.as_str()) {
                return true;
            }
            current = parent;
        }

        false
    }

    /// Returns the raw pattern strings.
    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }

    /// Returns the number of patterns.
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    /// Returns true if there are no patterns.
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Compares two exclusion sets and returns differences.
    pub fn diff(&self, other: &Exclusions) -> ExclusionsDiff {
        let left_set: HashSet<_> = self.patterns.iter().collect();
        let right_set: HashSet<_> = other.patterns.iter().collect();

        let only_left: Vec<String> = self
            .patterns
            .iter()
            .filter(|p| !right_set.contains(p))
            .cloned()
            .collect();

        let only_right: Vec<String> = other
            .patterns
            .iter()
            .filter(|p| !left_set.contains(p))
            .cloned()
            .collect();

        let is_same = only_left.is_empty() && only_right.is_empty();

        ExclusionsDiff {
            only_left,
            only_right,
            is_same,
        }
    }

    /// Compiles patterns into a GlobSet for efficient matching.
    fn compile_patterns(patterns: &[String]) -> Result<GlobSet> {
        let mut builder = GlobSetBuilder::new();

        for pattern in patterns {
            let pattern = pattern.trim();
            if pattern.is_empty() || pattern.starts_with('#') {
                continue;
            }

            // Normalize pattern: use forward slashes
            let pattern = pattern.replace('\\', "/");

            // Handle directory patterns (trailing /)
            // Convert "dir/" to "dir" and "dir/**" for matching both the dir and contents
            let glob_patterns: Vec<String> = if pattern.ends_with('/') {
                let base = pattern.trim_end_matches('/');
                vec![
                    base.to_string(),       // Match the directory itself
                    format!("{}/**", base), // Match all contents
                ]
            } else {
                vec![pattern.clone()]
            };

            for glob_pattern in glob_patterns {
                let glob = Glob::new(&glob_pattern)
                    .with_context(|| format!("Invalid glob pattern: {}", glob_pattern))?;
                builder.add(glob);
            }
        }

        builder
            .build()
            .with_context(|| "Failed to build glob set")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_dir() -> TempDir {
        TempDir::new().expect("Failed to create temp directory")
    }

    #[test]
    fn test_empty_exclusions() {
        let excl = Exclusions::empty();
        assert!(excl.is_empty());
        assert_eq!(excl.len(), 0);
        assert!(!excl.is_excluded(Path::new("file.txt"), false));
    }

    #[test]
    fn test_from_patterns_filters_comments() {
        let patterns = vec![
            "*.tmp".to_string(),
            "# comment".to_string(),
            "".to_string(),
            "  ".to_string(),
            "*.log".to_string(),
        ];
        let excl = Exclusions::from_patterns(&patterns).unwrap();
        assert_eq!(excl.len(), 2);
        assert_eq!(excl.patterns(), &["*.tmp", "*.log"]);
    }

    #[test]
    fn test_simple_pattern_matching() {
        let excl = Exclusions::from_patterns(&["*.tmp".to_string()]).unwrap();

        assert!(excl.is_excluded(Path::new("file.tmp"), false));
        assert!(excl.is_excluded(Path::new("another.tmp"), false));
        assert!(!excl.is_excluded(Path::new("file.txt"), false));
        assert!(!excl.is_excluded(Path::new("file.tmp.bak"), false));
    }

    #[test]
    fn test_directory_pattern() {
        let excl = Exclusions::from_patterns(&["node_modules/".to_string()]).unwrap();

        // Directory itself should be excluded
        assert!(excl.is_excluded(Path::new("node_modules"), true));

        // Files inside should be excluded
        assert!(excl.is_excluded(Path::new("node_modules/lodash/index.js"), false));
        assert!(excl.is_excluded(Path::new("node_modules/express/lib/router.js"), false));

        // Similar named files/dirs should NOT be excluded
        assert!(!excl.is_excluded(Path::new("node_modules_backup"), true));
        assert!(!excl.is_excluded(Path::new("my_node_modules"), true));
    }

    #[test]
    fn test_nested_directory_pattern() {
        let excl = Exclusions::from_patterns(&[".git/".to_string()]).unwrap();

        assert!(excl.is_excluded(Path::new(".git"), true));
        assert!(excl.is_excluded(Path::new(".git/config"), false));
        assert!(excl.is_excluded(Path::new(".git/objects/pack/file"), false));
    }

    #[test]
    fn test_doublestar_pattern() {
        let excl = Exclusions::from_patterns(&["**/*.log".to_string()]).unwrap();

        assert!(excl.is_excluded(Path::new("app.log"), false));
        assert!(excl.is_excluded(Path::new("logs/app.log"), false));
        assert!(excl.is_excluded(Path::new("a/b/c/debug.log"), false));
        assert!(!excl.is_excluded(Path::new("app.txt"), false));
    }

    #[test]
    fn test_question_mark_pattern() {
        let excl = Exclusions::from_patterns(&["file?.txt".to_string()]).unwrap();

        assert!(excl.is_excluded(Path::new("file1.txt"), false));
        assert!(excl.is_excluded(Path::new("fileA.txt"), false));
        assert!(!excl.is_excluded(Path::new("file.txt"), false));
        assert!(!excl.is_excluded(Path::new("file12.txt"), false));
    }

    #[test]
    fn test_character_class_pattern() {
        let excl = Exclusions::from_patterns(&["[0-9].txt".to_string()]).unwrap();

        assert!(excl.is_excluded(Path::new("1.txt"), false));
        assert!(excl.is_excluded(Path::new("9.txt"), false));
        assert!(!excl.is_excluded(Path::new("a.txt"), false));
        assert!(!excl.is_excluded(Path::new("10.txt"), false));
    }

    #[test]
    fn test_alternatives_pattern() {
        let excl = Exclusions::from_patterns(&["*.{tmp,temp}".to_string()]).unwrap();

        assert!(excl.is_excluded(Path::new("file.tmp"), false));
        assert!(excl.is_excluded(Path::new("file.temp"), false));
        assert!(!excl.is_excluded(Path::new("file.txt"), false));
    }

    #[test]
    fn test_tilde_patterns() {
        let excl = Exclusions::from_patterns(&["~*".to_string(), "*~".to_string()]).unwrap();

        assert!(excl.is_excluded(Path::new("~file"), false));
        assert!(excl.is_excluded(Path::new("file~"), false));
        assert!(!excl.is_excluded(Path::new("file"), false));
    }

    #[test]
    fn test_os_files() {
        let excl = Exclusions::from_patterns(&[
            ".DS_Store".to_string(),
            "Thumbs.db".to_string(),
        ])
        .unwrap();

        assert!(excl.is_excluded(Path::new(".DS_Store"), false));
        assert!(excl.is_excluded(Path::new("Thumbs.db"), false));
        assert!(!excl.is_excluded(Path::new("other.db"), false));
    }

    #[test]
    fn test_load_nonexistent_returns_empty() {
        let temp = create_test_dir();
        let excl = Exclusions::load(temp.path()).unwrap();
        assert!(excl.is_empty());
    }

    #[test]
    fn test_file_path() {
        let path = Exclusions::file_path(Path::new("/some/path"));
        assert!(path.ends_with(".rahzomignore"));
    }

    #[test]
    fn test_diff_same_patterns() {
        let a = Exclusions::from_patterns(&["*.tmp".to_string(), "*.log".to_string()]).unwrap();
        let b = Exclusions::from_patterns(&["*.log".to_string(), "*.tmp".to_string()]).unwrap();

        let diff = a.diff(&b);
        assert!(diff.is_same);
        assert!(diff.only_left.is_empty());
        assert!(diff.only_right.is_empty());
    }

    #[test]
    fn test_diff_different_patterns() {
        let a = Exclusions::from_patterns(&["*.tmp".to_string(), "*.log".to_string()]).unwrap();
        let b = Exclusions::from_patterns(&["*.tmp".to_string(), "*.txt".to_string()]).unwrap();

        let diff = a.diff(&b);
        assert!(!diff.is_same);
        assert_eq!(diff.only_left, vec!["*.log"]);
        assert_eq!(diff.only_right, vec!["*.txt"]);
    }

    #[test]
    fn test_diff_empty_vs_non_empty() {
        let a = Exclusions::empty();
        let b = Exclusions::from_patterns(&["*.tmp".to_string()]).unwrap();

        let diff = a.diff(&b);
        assert!(!diff.is_same);
        assert!(diff.only_left.is_empty());
        assert_eq!(diff.only_right, vec!["*.tmp"]);
    }

    #[test]
    fn test_default_template_content() {
        let template = Exclusions::default_template();
        assert!(template.contains("*.tmp"));
        assert!(template.contains("node_modules/"));
        assert!(template.contains(".git/"));
        assert!(template.contains("# Rahzom exclusion patterns"));
    }

    #[test]
    fn test_load_with_comments_and_whitespace() {
        let temp = create_test_dir();

        // Create .rahzomignore file manually
        let content = r#"# This is a comment
*.tmp

  # Another comment with leading whitespace
  *.log

node_modules/
"#;
        fs::write(temp.path().join(".rahzomignore"), content).unwrap();

        let excl = Exclusions::load(temp.path()).unwrap();
        assert_eq!(excl.len(), 3);
        assert!(excl.is_excluded(Path::new("file.tmp"), false));
        assert!(excl.is_excluded(Path::new("file.log"), false));
        assert!(excl.is_excluded(Path::new("node_modules"), true));
    }

    #[test]
    fn test_windows_path_separators() {
        let excl = Exclusions::from_patterns(&["node_modules/".to_string()]).unwrap();

        // Should work with both forward and back slashes
        assert!(excl.is_excluded(Path::new("node_modules\\lodash\\index.js"), false));
        assert!(excl.is_excluded(Path::new("node_modules/lodash/index.js"), false));
    }

    #[test]
    fn test_invalid_pattern_error() {
        // An invalid glob pattern should return an error
        let result = Exclusions::from_patterns(&["[invalid".to_string()]);
        assert!(result.is_err());
    }
}
