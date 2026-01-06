use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// Project settings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectSettings {
    /// Whether to verify file hashes during sync
    #[serde(default)]
    pub verify_hash: bool,
    /// Number of backup versions to keep
    #[serde(default = "default_backup_versions")]
    pub backup_versions: usize,
    /// Days to keep deleted files in registry
    #[serde(default = "default_deleted_retention_days")]
    pub deleted_retention_days: u32,
    /// Whether to use soft delete (move to trash)
    #[serde(default = "default_soft_delete")]
    pub soft_delete: bool,
}

fn default_backup_versions() -> usize {
    5
}

fn default_deleted_retention_days() -> u32 {
    90
}

fn default_soft_delete() -> bool {
    true
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            verify_hash: false,
            backup_versions: default_backup_versions(),
            deleted_retention_days: default_deleted_retention_days(),
            soft_delete: default_soft_delete(),
        }
    }
}

/// A sync project definition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    /// Project name (used as filename)
    pub name: String,
    /// Left side path
    pub left_path: PathBuf,
    /// Right side path
    pub right_path: PathBuf,
    /// Project-specific settings
    #[serde(default)]
    pub settings: ProjectSettings,
}

impl Project {
    /// Creates a new project with default settings
    pub fn new(name: impl Into<String>, left_path: PathBuf, right_path: PathBuf) -> Self {
        Self {
            name: name.into(),
            left_path,
            right_path,
            settings: ProjectSettings::default(),
        }
    }

    /// Validates project configuration
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            bail!("Project name cannot be empty");
        }

        if !is_valid_project_name(&self.name) {
            bail!(
                "Invalid project name '{}': must contain only alphanumeric characters, dashes, and underscores",
                self.name
            );
        }

        if self.left_path.as_os_str().is_empty() {
            bail!("Left path cannot be empty");
        }

        if self.right_path.as_os_str().is_empty() {
            bail!("Right path cannot be empty");
        }

        Ok(())
    }
}

/// Checks if a project name is valid (alphanumeric, dashes, underscores)
fn is_valid_project_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Manages project configurations stored in ~/.rahzom/projects/
pub struct ProjectManager {
    config_dir: PathBuf,
}

impl ProjectManager {
    /// Creates a new ProjectManager using the default config directory (~/.rahzom/)
    pub fn new() -> Result<Self> {
        let config_dir = dirs::home_dir()
            .context("Could not determine home directory")?
            .join(".rahzom");

        Ok(Self { config_dir })
    }

    /// Creates a ProjectManager with a custom config directory (for testing)
    pub fn with_config_dir(config_dir: PathBuf) -> Self {
        Self { config_dir }
    }

    /// Returns the path to the projects directory
    fn projects_dir(&self) -> PathBuf {
        self.config_dir.join("projects")
    }

    /// Returns the path to a project file
    fn project_path(&self, name: &str) -> PathBuf {
        self.projects_dir().join(format!("{}.toml", name))
    }

    /// Ensures the projects directory exists
    fn ensure_projects_dir(&self) -> Result<()> {
        let dir = self.projects_dir();
        if !dir.exists() {
            fs::create_dir_all(&dir)
                .with_context(|| format!("Failed to create projects directory: {:?}", dir))?;
        }
        Ok(())
    }

    /// Lists all available project names
    pub fn list_projects(&self) -> Result<Vec<String>> {
        let dir = self.projects_dir();

        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut projects = Vec::new();

        for entry in fs::read_dir(&dir)
            .with_context(|| format!("Failed to read projects directory: {:?}", dir))?
        {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map(|e| e == "toml").unwrap_or(false) {
                if let Some(stem) = path.file_stem() {
                    projects.push(stem.to_string_lossy().to_string());
                }
            }
        }

        projects.sort();
        Ok(projects)
    }

    /// Loads a project by name
    pub fn load_project(&self, name: &str) -> Result<Project> {
        if !is_valid_project_name(name) {
            bail!("Invalid project name: {}", name);
        }

        let path = self.project_path(name);

        if !path.exists() {
            bail!("Project '{}' not found", name);
        }

        let file = File::open(&path)
            .with_context(|| format!("Failed to open project file: {:?}", path))?;

        let mut reader = BufReader::new(file);
        let mut content = String::new();
        reader
            .read_to_string(&mut content)
            .with_context(|| format!("Failed to read project file: {:?}", path))?;

        let project: Project = toml::from_str(&content)
            .with_context(|| format!("Failed to parse project file: {:?}", path))?;

        Ok(project)
    }

    /// Saves a project
    pub fn save_project(&self, project: &Project) -> Result<()> {
        project.validate()?;
        self.ensure_projects_dir()?;

        let path = self.project_path(&project.name);

        let content = toml::to_string_pretty(project)
            .with_context(|| format!("Failed to serialize project: {}", project.name))?;

        let file = File::create(&path)
            .with_context(|| format!("Failed to create project file: {:?}", path))?;

        let mut writer = BufWriter::new(file);
        writer
            .write_all(content.as_bytes())
            .with_context(|| format!("Failed to write project file: {:?}", path))?;

        Ok(())
    }

    /// Deletes a project
    pub fn delete_project(&self, name: &str) -> Result<()> {
        if !is_valid_project_name(name) {
            bail!("Invalid project name: {}", name);
        }

        let path = self.project_path(name);

        if !path.exists() {
            bail!("Project '{}' not found", name);
        }

        fs::remove_file(&path)
            .with_context(|| format!("Failed to delete project file: {:?}", path))?;

        Ok(())
    }

    /// Checks if a project exists
    pub fn project_exists(&self, name: &str) -> bool {
        if !is_valid_project_name(name) {
            return false;
        }
        self.project_path(name).exists()
    }

    /// Returns the config directory path
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_manager() -> (ProjectManager, TempDir) {
        let temp = TempDir::new().expect("Failed to create temp directory");
        let manager = ProjectManager::with_config_dir(temp.path().to_path_buf());
        (manager, temp)
    }

    fn sample_project(name: &str) -> Project {
        Project::new(
            name,
            PathBuf::from("/home/user/docs"),
            PathBuf::from("/mnt/backup/docs"),
        )
    }

    #[test]
    fn test_create_and_load_project() {
        let (manager, _temp) = create_test_manager();

        let project = sample_project("test-project");
        manager.save_project(&project).unwrap();

        let loaded = manager.load_project("test-project").unwrap();

        assert_eq!(loaded.name, "test-project");
        assert_eq!(loaded.left_path, PathBuf::from("/home/user/docs"));
        assert_eq!(loaded.right_path, PathBuf::from("/mnt/backup/docs"));
    }

    #[test]
    fn test_list_projects() {
        let (manager, _temp) = create_test_manager();

        // No projects initially
        assert!(manager.list_projects().unwrap().is_empty());

        // Add some projects
        manager.save_project(&sample_project("alpha")).unwrap();
        manager.save_project(&sample_project("beta")).unwrap();
        manager.save_project(&sample_project("gamma")).unwrap();

        let projects = manager.list_projects().unwrap();
        assert_eq!(projects, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn test_delete_project() {
        let (manager, _temp) = create_test_manager();

        manager.save_project(&sample_project("to-delete")).unwrap();
        assert!(manager.project_exists("to-delete"));

        manager.delete_project("to-delete").unwrap();
        assert!(!manager.project_exists("to-delete"));
    }

    #[test]
    fn test_project_exists() {
        let (manager, _temp) = create_test_manager();

        assert!(!manager.project_exists("nonexistent"));

        manager.save_project(&sample_project("existing")).unwrap();
        assert!(manager.project_exists("existing"));
    }

    #[test]
    fn test_save_and_reload_preserves_settings() {
        let (manager, _temp) = create_test_manager();

        let mut project = sample_project("with-settings");
        project.settings.verify_hash = true;
        project.settings.backup_versions = 10;
        project.settings.deleted_retention_days = 30;
        project.settings.soft_delete = false;

        manager.save_project(&project).unwrap();
        let loaded = manager.load_project("with-settings").unwrap();

        assert_eq!(loaded.settings.verify_hash, true);
        assert_eq!(loaded.settings.backup_versions, 10);
        assert_eq!(loaded.settings.deleted_retention_days, 30);
        assert_eq!(loaded.settings.soft_delete, false);
    }

    #[test]
    fn test_invalid_project_name_rejected() {
        let (manager, _temp) = create_test_manager();

        // Empty name
        let mut project = sample_project("");
        assert!(manager.save_project(&project).is_err());

        // Invalid characters
        project.name = "test/project".to_string();
        assert!(manager.save_project(&project).is_err());

        project.name = "test project".to_string();
        assert!(manager.save_project(&project).is_err());

        project.name = "test.project".to_string();
        assert!(manager.save_project(&project).is_err());
    }

    #[test]
    fn test_valid_project_names() {
        let (manager, _temp) = create_test_manager();

        // Valid names
        for name in &["simple", "with-dash", "with_underscore", "Mixed123"] {
            let project = sample_project(name);
            assert!(manager.save_project(&project).is_ok(), "Name '{}' should be valid", name);
        }
    }

    #[test]
    fn test_load_nonexistent_project() {
        let (manager, _temp) = create_test_manager();

        let result = manager.load_project("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_nonexistent_project() {
        let (manager, _temp) = create_test_manager();

        let result = manager.delete_project("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_project_file_is_valid_toml() {
        let (manager, temp) = create_test_manager();

        let project = sample_project("toml-test");
        manager.save_project(&project).unwrap();

        // Read raw file content
        let path = temp.path().join("projects/toml-test.toml");
        let content = fs::read_to_string(&path).unwrap();

        // Should be valid TOML
        assert!(content.contains("name = \"toml-test\""));
        assert!(content.contains("left_path = \"/home/user/docs\""));
    }

    #[test]
    fn test_default_settings() {
        let settings = ProjectSettings::default();

        assert_eq!(settings.verify_hash, false);
        assert_eq!(settings.backup_versions, 5);
        assert_eq!(settings.deleted_retention_days, 90);
        assert_eq!(settings.soft_delete, true);
    }
}
