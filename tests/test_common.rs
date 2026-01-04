mod common;

use common::{create_test_tree, FileSpec, TreeSpec};
use std::fs;

#[test]
fn test_create_empty_tree() {
    let spec = TreeSpec { files: vec![] };
    let temp = create_test_tree(&spec);
    assert!(temp.path().exists());
}

#[test]
fn test_create_tree_with_file() {
    let spec = TreeSpec {
        files: vec![FileSpec::new("test.txt").content("Hello, World!")],
    };
    let temp = create_test_tree(&spec);

    let file_path = temp.path().join("test.txt");
    assert!(file_path.exists());

    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "Hello, World!");
}

#[test]
fn test_create_tree_with_directory() {
    let spec = TreeSpec {
        files: vec![FileSpec::new("subdir").dir()],
    };
    let temp = create_test_tree(&spec);

    let dir_path = temp.path().join("subdir");
    assert!(dir_path.exists());
    assert!(dir_path.is_dir());
}

#[test]
fn test_create_tree_with_nested_structure() {
    let spec = TreeSpec {
        files: vec![
            FileSpec::new("docs/readme.txt").content("README"),
            FileSpec::new("docs/guide/intro.md").content("# Introduction"),
            FileSpec::new("data").dir(),
        ],
    };
    let temp = create_test_tree(&spec);

    assert!(temp.path().join("docs/readme.txt").exists());
    assert!(temp.path().join("docs/guide/intro.md").exists());
    assert!(temp.path().join("data").is_dir());

    let readme = fs::read_to_string(temp.path().join("docs/readme.txt")).unwrap();
    assert_eq!(readme, "README");
}

#[test]
fn test_create_tree_with_random_content() {
    let spec = TreeSpec {
        files: vec![FileSpec::new("random.bin").random(1024)],
    };
    let temp = create_test_tree(&spec);

    let file_path = temp.path().join("random.bin");
    assert!(file_path.exists());

    let content = fs::read(&file_path).unwrap();
    assert_eq!(content.len(), 1024);
}
