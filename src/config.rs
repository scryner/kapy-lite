use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    import: ImportPath,
}

#[derive(Deserialize, Debug)]
pub struct ImportPath {
    from: PathBuf,
    to: PathBuf,
}

impl Config {
    pub fn build_from_file(path: &Path) -> Result<Config> {
        match fs::read_to_string(&path) {
            Ok(contents) => Config::build(contents),
            Err(err) => Err(anyhow!(
                "Failed to read config file '{}': {}",
                path.display(),
                err
            )),
        }
    }

    pub fn build(contents: String) -> Result<Config> {
        // deserialize from yaml
        match serde_yaml::from_str(&contents) {
            Ok(conf) => Ok(conf),
            Err(e) => Err(anyhow!("Failed to parse config file: {}", e)),
        }
    }

    pub fn set_import_from(&mut self, path: impl AsRef<Path>) {
        self.import.from = path.as_ref().to_path_buf();
    }

    pub fn set_import_to(&mut self, path: impl AsRef<Path>) {
        self.import.to = path.as_ref().to_path_buf();
    }

    pub fn import_from(&self) -> &Path {
        &self.import.from
    }

    pub fn import_to(&self) -> &Path {
        &self.import.to
    }
}

#[derive(Debug)]
pub struct ConfigPath {
    app_home: PathBuf,
    config_path: PathBuf,
    cred_path: PathBuf,
}

impl ConfigPath {
    pub fn app_home(&self) -> &Path {
        &self.app_home
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn cred_path(&self) -> &Path {
        &self.cred_path
    }
}

pub fn default_path() -> ConfigPath {
    let home_dir = home::home_dir().unwrap();
    let app_home = home_dir.join(".kapylite");
    let config_path = app_home.join("config.yaml");
    let cred_path = app_home.join(".cred");

    ConfigPath {
        app_home,
        config_path,
        cred_path,
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{Config, default_path};

    #[test]
    fn build_from_str() {
        let yaml = r#"import:
  from: /Volumes/Untitled/DCIM/108HASBL
  to: ~/images
"#;

        let conf = Config::build(String::from(yaml)).expect("Failed to deserialize from string");
        println!("conf=\n{:#?}", conf);
    }

    #[test]
    fn get_default_path() {
        let conf_path = default_path();
        let app_home = conf_path.app_home();
        let config_path = conf_path.config_path();
        let cred_path = conf_path.cred_path();

        assert_eq!(app_home, home::home_dir().unwrap().join(".kapylite"));
        assert_eq!(config_path, app_home.join("config.yaml"));
        assert_eq!(cred_path, app_home.join(".cred"));
    }
}
