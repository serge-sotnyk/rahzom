use std::fs;
use std::io::Write;
use std::path::Path;

use chrono::{DateTime, Utc};
use tempfile::TempDir;

/// Content specification for a test file
pub enum Content {
    /// Fixed string content
    Fixed(&'static str),
    /// Random bytes of given size
    Random(usize),
    /// Empty file (0 bytes)
    Empty,
}

/// Specification for a single file or directory in the test tree
pub struct FileSpec {
    /// Relative path within the test directory
    pub path: &'static str,
    /// Content of the file (None for directories)
    pub content: Option<Content>,
    /// Optional modification time
    pub mtime: Option<DateTime<Utc>>,
    /// Whether this is a directory
    pub is_dir: bool,
}

impl FileSpec {
    /// Create a file specification
    pub fn new(path: &'static str) -> Self {
        Self {
            path,
            content: Some(Content::Empty),
            mtime: None,
            is_dir: false,
        }
    }

    /// Mark this as a directory
    pub fn dir(mut self) -> Self {
        self.is_dir = true;
        self.content = None;
        self
    }

    /// Set fixed string content
    pub fn content(mut self, content: &'static str) -> Self {
        self.content = Some(Content::Fixed(content));
        self
    }

    /// Set random content of given size
    pub fn random(mut self, size: usize) -> Self {
        self.content = Some(Content::Random(size));
        self
    }

    /// Set modification time
    #[allow(dead_code)]
    pub fn mtime(mut self, mtime: DateTime<Utc>) -> Self {
        self.mtime = Some(mtime);
        self
    }
}

/// Specification for a test directory tree
pub struct TreeSpec {
    pub files: Vec<FileSpec>,
}

/// Creates a temporary directory with the specified file structure.
/// The returned TempDir will be automatically cleaned up when dropped.
pub fn create_test_tree(spec: &TreeSpec) -> TempDir {
    let temp = TempDir::new().expect("Failed to create temp directory");
    let root = temp.path();

    for file_spec in &spec.files {
        let path = root.join(file_spec.path);

        if file_spec.is_dir {
            fs::create_dir_all(&path).expect("Failed to create directory");
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("Failed to create parent directory");
            }

            let content = match &file_spec.content {
                Some(Content::Fixed(s)) => s.as_bytes().to_vec(),
                Some(Content::Random(size)) => generate_random_bytes(*size),
                Some(Content::Empty) | None => Vec::new(),
            };

            let mut file = fs::File::create(&path).expect("Failed to create file");
            file.write_all(&content).expect("Failed to write content");
        }

        if let Some(mtime) = file_spec.mtime {
            set_file_mtime(&path, mtime);
        }
    }

    temp
}

fn generate_random_bytes(size: usize) -> Vec<u8> {
    // Simple pseudo-random generator for test data
    let mut bytes = Vec::with_capacity(size);
    let mut state: u32 = 12345;
    for _ in 0..size {
        state = state.wrapping_mul(1103515245).wrapping_add(12345);
        bytes.push((state >> 16) as u8);
    }
    bytes
}

#[allow(dead_code)]
fn set_file_mtime(path: &Path, mtime: DateTime<Utc>) {
    use std::time::{Duration, UNIX_EPOCH};

    let timestamp = mtime.timestamp();
    let system_time = if timestamp >= 0 {
        UNIX_EPOCH + Duration::from_secs(timestamp as u64)
    } else {
        UNIX_EPOCH
    };

    // TODO: Use filetime crate for proper mtime setting
    let _ = (path, system_time);
}
