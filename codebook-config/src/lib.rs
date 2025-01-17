use anyhow::{anyhow, Context, Result};
use glob::Pattern;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct CodebookConfig {
    /// List of dictionaries to use for spell checking
    #[serde(default = "default_dictionary")]
    pub dictionaries: Vec<String>,

    /// Custom allowlist of words
    #[serde(default)]
    pub words: Vec<String>,

    /// Words that should always be flagged
    #[serde(default)]
    pub flag_words: Vec<String>,

    /// Glob patterns for paths to ignore
    #[serde(default)]
    pub ignore_paths: Vec<String>,

    /// Keep track of the config file path when loaded
    #[serde(skip)]
    config_path: Option<PathBuf>,
}

fn default_dictionary() -> Vec<String> {
    vec!["en".to_string()]
}

impl CodebookConfig {
    /// Load configuration by searching up from the current directory
    pub fn load() -> Result<Self> {
        let current_dir = env::current_dir().context("Failed to get current directory")?;
        Self::find_and_load_config(&current_dir)
    }

    pub fn reload(&mut self) -> Result<()> {
        let config_path = self.config_path.as_ref().ok_or_else(|| {
            anyhow!("No config file path available. Config was not loaded from a file.")
        })?;

        let content = fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

        *self = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", config_path.display()))?;

        Ok(())
    }

    /// Load configuration starting from a specific directory
    pub fn load_from_dir<P: AsRef<Path>>(start_dir: P) -> Result<Self> {
        Self::find_and_load_config(start_dir.as_ref())
    }

    /// Add a word to the allowlist and save the configuration
    pub fn add_word(&mut self, word: &str) -> Result<()> {
        // Check if word already exists
        if self.words.contains(&word.to_string()) {
            return Ok(());
        }

        // Add the word
        self.words.push(word.to_string());
        // Sort for consistency
        self.words.sort();

        // Save the changes
        self.save()?;

        Ok(())
    }

    /// Save the configuration to its file
    pub fn save(&self) -> Result<()> {
        let config_path = self.config_path.as_ref().ok_or_else(|| {
            anyhow!("No config file path available. Config was not loaded from a file.")
        })?;

        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;

        fs::write(config_path, content).with_context(|| {
            format!("Failed to write config to file: {}", config_path.display())
        })?;

        Ok(())
    }

    /// Create a new configuration file if one doesn't exist
    pub fn create_if_not_exists(directory: Option<&PathBuf>) -> Result<Self> {
        let current_dir = env::current_dir().context("Failed to get current directory")?;
        let default_name = "codebook.toml";
        let config_path = match directory {
            Some(d) => d.join(default_name),
            None => current_dir.join(default_name),
        };

        if config_path.exists() {
            return Self::load_from_file(&config_path);
        }

        let config = Self {
            config_path: Some(config_path.clone()),
            ..Default::default()
        };

        // Save the new config
        let content = toml::to_string_pretty(&config).context("Failed to serialize config")?;

        fs::write(&config_path, content)
            .with_context(|| format!("Failed to create config file: {}", config_path.display()))?;

        Ok(config)
    }

    /// Recursively search for and load config from the given directory and its parents
    fn find_and_load_config(start_dir: &Path) -> Result<Self> {
        let config_files = ["codebook.toml", ".codebook.toml"];

        // Start from the given directory and walk up to root
        let mut current_dir = Some(start_dir.to_path_buf());

        while let Some(dir) = current_dir {
            // Try each possible config filename in the current directory
            for config_name in &config_files {
                let config_path = dir.join(config_name);
                if config_path.is_file() {
                    return Self::load_from_file(&config_path);
                }
            }

            // Move to parent directory
            current_dir = dir.parent().map(PathBuf::from);
        }

        // If no config file is found, return default config
        Ok(Self::default())
    }

    /// Load configuration from a specific file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let mut config: Self = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        // Store the config file path
        config.config_path = Some(path.to_path_buf());

        Ok(config)
    }

    /// Check if a path should be ignored based on the ignore_paths patterns
    pub fn should_ignore_path<P: AsRef<Path>>(&self, path: P) -> bool {
        let path_str = path.as_ref().to_string_lossy();
        self.ignore_paths.iter().any(|pattern| {
            Pattern::new(pattern)
                .map(|p| p.matches(&path_str))
                .unwrap_or(false)
        })
    }

    /// Check if a word is in the custom allowlist
    pub fn is_allowed_word(&self, word: &str) -> bool {
        self.words.iter().any(|w| w == word)
    }

    /// Check if a word should be flagged
    pub fn should_flag_word(&self, word: &str) -> bool {
        self.flag_words.iter().any(|w| w == word)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_add_word() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("codebook.toml");

        // Create initial config
        let mut config = CodebookConfig {
            config_path: Some(config_path.clone()),
            ..Default::default()
        };
        config.save()?;

        // Add a word
        config.add_word("testword")?;

        // Reload config and verify
        let loaded_config = CodebookConfig::load_from_file(&config_path)?;
        assert!(loaded_config.is_allowed_word("testword"));

        Ok(())
    }

    #[test]
    fn test_config_recursive_search() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let sub_dir = temp_dir.path().join("sub");
        let sub_sub_dir = sub_dir.join("subsub");
        fs::create_dir_all(&sub_sub_dir)?;

        let config_path = temp_dir.path().join("codebook.toml");
        let mut file = File::create(&config_path)?;
        write!(
            file,
            r#"
            dictionaries = ["en_US"]
            words = ["testword"]
            flag_words = ["todo"]
            ignore_paths = ["target/**/*"]
        "#
        )?;

        let config = CodebookConfig::load_from_dir(&sub_sub_dir)?;
        assert!(config.words.contains(&"testword".to_string()));
        // Check that the config file path is stored
        assert_eq!(config.config_path, Some(config_path));
        Ok(())
    }

    #[test]
    fn test_create_if_not_exists() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_dir_path = temp_dir.path().to_path_buf();
        let config_path = config_dir_path.join("codebook.toml");

        // Create a new config file
        let config = CodebookConfig::create_if_not_exists(Some(&config_dir_path))?;
        assert_eq!(config.config_path, Some(config_path.clone()));

        // Check that the file was created
        assert!(config_path.exists());

        // Check that the file can be loaded
        let loaded_config = CodebookConfig::load_from_file(&config_path)?;
        assert_eq!(config, loaded_config);

        Ok(())
    }

    #[test]
    fn test_should_ignore_path() -> Result<()> {
        let config = CodebookConfig {
            ignore_paths: vec!["target/**/*".to_string()],
            ..Default::default()
        };

        assert!(config.should_ignore_path("target/debug/build"));
        assert!(!config.should_ignore_path("src/main.rs"));

        Ok(())
    }
    #[test]
    fn test_reload() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("codebook.toml");

        // Create initial config
        let mut config = CodebookConfig {
            config_path: Some(config_path.clone()),
            ..Default::default()
        };
        config.save()?;

        // Add a word to the toml file
        let mut file = File::create(&config_path)?;
        write!(
            file,
            r#"
            words = ["testword"]
            "#
        )?;

        // Reload config and verify
        config.reload()?;
        assert!(config.is_allowed_word("testword"));

        Ok(())
    }
}
