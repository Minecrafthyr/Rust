use std::{collections::BTreeMap, fs};

use serde::{Deserialize, Serialize};

use crate::config::Paths;

const DEFAULT_TITLES: &str = include_str!("../data/titles.json5");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TitleAction {
  New,
  Continue,
  Delete,
  Archive,
  Settings,
  Quit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TitleMenuItem {
  /// One shortcut key, e.g. `"n"`.
  pub key: String,
  pub label: String,
  pub action: TitleAction,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TitleEntry {
  pub name: String,
  #[serde(default)]
  pub subtitle: String,
  #[serde(default)]
  pub prompt: String,
  /// Lines are printed verbatim so the art can use any Unicode box-drawing characters.
  pub art: Vec<String>,
  /// Horizontal shortcut menu. Labels are language-specific and live in JSON.
  #[serde(default)]
  pub menu: Vec<TitleMenuItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TitleConfig {
  #[serde(default = "default_language")]
  pub default_language: String,
  pub titles: BTreeMap<String, TitleEntry>,
}

fn default_language() -> String { "zh_CN".to_string() }

impl TitleConfig {
  /// On-demand lookup for a language; falls back to the configured default and then the first
  /// available entry. This is the interface callers use when they want to switch titles without
  /// caring about the data layout.
  pub fn select(&self, language: &str) -> Option<&TitleEntry> {
    self
      .titles
      .get(language)
      .or_else(|| self.titles.get(&self.default_language).or_else(|| self.titles.values().next()))
  }

  pub fn languages(&self) -> Vec<&str> { self.titles.keys().map(String::as_str).collect() }

  pub fn cycle_language(&self, current: &str) -> String {
    let languages = self.languages();
    if languages.is_empty() {
      return current.to_string();
    }
    let index = languages.iter().position(|language| *language == current).unwrap_or(0);
    languages[(index + 1) % languages.len()].to_string()
  }

  pub fn validate(&self) -> Result<(), String> {
    if self.titles.is_empty() {
      return Err("标题配置至少需要一个语言条目".to_string());
    }
    for (language, entry) in &self.titles {
      if entry.art.is_empty() {
        return Err(format!("标题语言 {language} 的 art 为空"));
      }
      if entry.name.trim().is_empty() {
        return Err(format!("标题语言 {language} 的 name 为空"));
      }
      for item in &entry.menu {
        if item.key.chars().count() != 1 {
          return Err(format!("标题语言 {language} 的菜单键 {} 必须是单个字符", item.key));
        }
        if item.label.trim().is_empty() {
          return Err(format!("标题语言 {language} 的菜单标签为空"));
        }
      }
    }
    if !self.titles.contains_key(&self.default_language) {
      return Err(format!("default_language {} 不存在", self.default_language));
    }
    Ok(())
  }
}

pub fn load_or_create(paths: &Paths) -> Result<TitleConfig, Box<dyn std::error::Error>> {
  paths.ensure()?;
  let path = paths.titles();
  if !path.exists() {
    fs::write(&path, DEFAULT_TITLES)?;
  }
  let source = fs::read_to_string(&path)?;
  let config: TitleConfig =
    json5::from_str(&source).map_err(|error| format!("解析 {} 失败：{error}", path.display()))?;
  config.validate()?;
  Ok(config)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_titles_parse_and_switch() {
    let config: TitleConfig = json5::from_str(DEFAULT_TITLES).unwrap();
    config.validate().unwrap();
    assert!(config.select("zh_CN").unwrap().art.len() >= 13);
    assert_eq!(config.select("missing").unwrap().name, "混乱擢升");
    assert_eq!(config.select("zh_CN").unwrap().menu.len(), 6);
    assert_eq!(config.select("zh_CN").unwrap().menu[0].action, TitleAction::New);
    assert_eq!(config.cycle_language("zh_CN"), "en_US");
    assert_eq!(config.cycle_language("en_US"), "zh_CN");
  }
}
