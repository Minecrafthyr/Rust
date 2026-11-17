use std::{collections::BTreeMap, fs};

use serde::{Deserialize, Serialize};

use crate::config::{Paths, parse};

const DEFAULT_TITLES: &str = include_str!("../data/titles.yaml");

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

/// A sequence of art frames drawn in place, each shown for `interval_ms`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TitleEntry {
  pub name: String,
  #[serde(default)]
  pub subtitle: String,
  #[serde(default)]
  pub prompt: String,
  /// Every art is one YAML string; frames inside it are separated by `---`. Rows are printed
  /// verbatim so the art can use any Unicode box-drawing characters.
  #[serde(default)]
  pub art: Vec<TitleArt>,
  /// How long each art (cycling through its frames) is shown before the next one.
  #[serde(default = "default_art_interval_ms")]
  pub art_interval_ms: u64,
  /// Horizontal shortcut menu. Labels are language-specific and live in YAML.
  #[serde(default)]
  pub menu: Vec<TitleMenuItem>,
}

fn default_art_interval_ms() -> u64 { 1_500 }

impl TitleEntry {
  /// Terminal width of [`TitleEntry::name`].
  pub fn name_width(&self) -> usize { text_width(&self.name) }
}

/// One art, written as a single YAML block scalar (`|-`). Rows are separated by newlines; frames
/// inside an art are separated by a `---` line, so blank art rows keep their meaning.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TitleArt {
  /// The art as written, kept verbatim so it can be written back unchanged.
  pub text: String,
  pub frames: Vec<ArtFrame>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArtFrame {
  /// The frame with `\n` between rows; every row is printed verbatim.
  pub text: String,
}

/// Separator between two frames of one art string.
pub const FRAME_SEPARATOR: &str = "---";

impl Serialize for TitleArt {
  fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&self.text)
  }
}

impl<'de> Deserialize<'de> for TitleArt {
  fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    Ok(Self::from_text(&String::deserialize(deserializer)?))
  }
}

impl TitleArt {
  /// Splits a multi-frame art string into frames. A frame starts after a `---` line, so blank
  /// rows inside an art keep their meaning (they separate words).
  pub fn from_text(text: &str) -> Self {
    let mut groups: Vec<Vec<&str>> = vec![Vec::new()];
    for line in text.lines() {
      if Self::is_separator(line) {
        groups.push(Vec::new());
      } else {
        groups.last_mut().expect("a frame is always open").push(line);
      }
    }
    Self {
      text: text.to_string(),
      frames: groups
        .into_iter()
        .map(|lines| ArtFrame { text: frame_text(&lines) })
        .filter(|frame| !frame.text.is_empty())
        .collect(),
    }
  }

  /// Only an explicit `---` line separates frames; a blank row stays part of the art.
  fn is_separator(line: &str) -> bool { line.trim().starts_with(FRAME_SEPARATOR) }

  /// Widest row in terminal cells; zero when there is no art.
  pub fn width(&self) -> usize { self.frames.iter().map(ArtFrame::width).max().unwrap_or(0) }

  /// Rows of the tallest frame; zero when there is no art.
  pub fn height(&self) -> usize { self.frames.iter().map(ArtFrame::height).max().unwrap_or(0) }

  pub fn is_empty(&self) -> bool { self.frames.is_empty() }

  /// Width of the art once trailing padding is removed, i.e. the widest row of any frame.
  pub fn content_width(&self) -> usize {
    self.frames.iter().map(ArtFrame::width).max().unwrap_or(0)
  }

  /// True when some row is narrower than the widest row, so the art has margin it can give up
  /// without clipping its content.
  pub fn is_simplified(&self) -> bool {
    let width = self.width();
    self.frames.iter().any(|frame| frame.text.lines().any(|row| text_width(row) < width))
  }

  /// The frame shown `elapsed_ms` after the art started cycling.
  pub fn frame_at(&self, elapsed_ms: u128, interval_ms: u64) -> Option<&ArtFrame> {
    if self.frames.is_empty() {
      return None;
    }
    let interval = u128::from(interval_ms.max(1));
    let index = (elapsed_ms / interval) % self.frames.len() as u128;
    self.frames.get(index as usize)
  }
}

impl ArtFrame {
  pub fn rows(&self) -> impl Iterator<Item = &str> { self.text.lines() }

  pub fn width(&self) -> usize { self.rows().map(text_width).max().unwrap_or(0) }

  pub fn height(&self) -> usize { self.rows().count() }
}

/// Drops blank rows around the frame and the common indentation shared by all of its rows.
fn frame_text(lines: &[&str]) -> String {
  let first = lines.iter().position(|line| !line.trim().is_empty());
  let Some(first) = first else {
    return String::new();
  };
  let last = lines.iter().rposition(|line| !line.trim().is_empty()).unwrap_or(first);
  let lines = &lines[first..=last];
  let indent = lines
    .iter()
    .filter(|line| !line.trim().is_empty())
    .map(|line| line.len() - line.trim_start().len())
    .min()
    .unwrap_or(0);
  lines.iter().map(|line| line.get(indent..).unwrap_or("")).collect::<Vec<_>>().join("\n")
}

/// Terminal width of `text` in cells (CJK characters take two).
pub fn text_width(text: &str) -> usize {
  use unicode_width::UnicodeWidthStr;
  text.width()
}

/// What the title screen shows above the menu, chosen from the terminal size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TitleLayout {
  /// The art chosen for this terminal; its frames cycle while the screen stays open.
  Art(TitleArt),
  /// No art fits, so only `name` is shown, as a single string.
  Name(String),
}

/// A drawable title screen. All frames share one size, so a redraw rewrites only the art block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitleScreen {
  /// Art rows of every frame, back to back; empty when only the name is shown.
  rows: Vec<String>,
  /// Rows per frame; zero when only the name is shown.
  height: usize,
  /// Number of frames; zero when only the name is shown.
  frames: usize,
  pub interval_ms: u64,
  pub name: String,
  pub subtitle: String,
  pub prompt: String,
  pub menu: Vec<String>,
  pub language_line: String,
}

impl TitleScreen {
  pub fn new(entry: &TitleEntry, layout: &TitleLayout, size: (u16, u16), language: &str) -> Self {
    let mut rows = Vec::new();
    let mut height = 0;
    let mut frames = 0;
    if let TitleLayout::Art(art) = layout {
      height = art.height();
      frames = art.frames.len();
      for frame in &art.frames {
        let lines = frame.rows().collect::<Vec<_>>();
        for row in 0..height {
          rows.push(fit_row(lines.get(row).copied().unwrap_or(""), art.width()));
        }
      }
    }
    let mut screen = Self {
      rows,
      height,
      frames,
      interval_ms: entry.art_interval_ms.max(1),
      name: entry.name.clone(),
      subtitle: entry.subtitle.clone(),
      prompt: entry.prompt.clone(),
      menu: entry
        .menu
        .iter()
        .map(|item| format!("[{}]{}", item.key.to_uppercase(), item.label))
        .collect(),
      language_line: format!("[L]语言/Language: {language}"),
    };
    screen.trim_to_height(usize::from(size.1));
    screen
  }

  /// Placeholder for a configuration without any usable language entry.
  pub fn empty(language: &str) -> Self {
    Self {
      rows: Vec::new(),
      height: 0,
      frames: 0,
      interval_ms: default_art_interval_ms(),
      name: String::new(),
      subtitle: String::new(),
      prompt: String::new(),
      menu: Vec::new(),
      language_line: format!("[L]语言/Language: {language}"),
    }
  }

  /// Drops art rows from the bottom while the full screen (art, subtitle, menu) is taller than
  /// `rows`, so a short terminal keeps showing the menu instead of scrolling it away.
  fn trim_to_height(&mut self, rows: usize) {
    let chrome = 1
      + usize::from(!self.subtitle.is_empty())
      + usize::from(!self.menu.is_empty())
      + usize::from(!self.prompt.is_empty())
      + 1;
    let budget = rows.saturating_sub(chrome);
    while self.height > budget && self.height > 0 {
      self.height -= 1;
      let mut kept = Vec::with_capacity(self.frames * self.height);
      for frame in 0..self.frames {
        let start = frame * (self.height + 1);
        kept.extend_from_slice(&self.rows[start..start + self.height]);
      }
      self.rows = kept;
    }
    if self.height == 0 {
      self.rows.clear();
      self.frames = 0;
    }
  }

  /// Index of the frame due `elapsed` after the screen appeared.
  pub fn frame_index(&self, elapsed: std::time::Duration) -> usize {
    let interval = u128::from(self.interval_ms.max(1));
    (elapsed.as_millis() / interval) as usize
  }

  /// Rows of the frame due `elapsed` after the screen appeared.
  pub fn frame_rows(&self, elapsed: std::time::Duration) -> &[String] {
    if self.height == 0 || self.frames == 0 {
      return &[];
    }
    let frame = self.frame_index(elapsed) % self.frames;
    &self.rows[frame * self.height..(frame + 1) * self.height]
  }

  pub fn frames(&self) -> usize { self.frames }
}

/// Pads or clips `row` to exactly `width` terminal cells.
pub fn fit_row(row: &str, width: usize) -> String {
  let cells = text_width(row);
  if cells > width {
    let mut out = String::new();
    let mut used = 0;
    for character in row.chars() {
      let char_cells = unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
      if used + char_cells > width {
        break;
      }
      out.push(character);
      used += char_cells;
    }
    out
  } else {
    format!("{row}{}", " ".repeat(width - cells))
  }
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

  /// Resolves what to draw above the menu for a terminal `columns` wide.
  ///
  /// Two passes, so the most detailed art that genuinely fits always wins:
  /// 1. The first art at full width, in the order they are listed (largest first).
  /// 2. A *simplified* art (one with rows narrower than its widest row, i.e. margin it can afford
  ///    to lose) that can shrink its margin down to the width of the title name.
  ///
  /// When even that fails the title name is shown as a single string, which is what the smallest
  /// terminal gets.
  pub fn layout(&self, entry: &TitleEntry, columns: u16) -> TitleLayout {
    let columns = usize::from(columns);
    let usable = entry.art.iter().filter(|art| !art.is_empty());
    if let Some(art) = usable.clone().find(|art| art.width() <= columns) {
      return TitleLayout::Art(art.clone());
    }
    // A simplified art may lend its own margin: rows are clipped to the terminal, so its content
    // must already sit within `columns` on its widest row, and the name must fit next to it.
    if entry.name_width() <= columns
      && let Some(art) =
        usable.clone().find(|art| art.is_simplified() && art.content_width() <= columns)
    {
      return TitleLayout::Art(art.clone());
    }
    TitleLayout::Name(entry.name.clone())
  }

  pub fn validate(&self) -> Result<(), String> {
    if self.titles.is_empty() {
      return Err("标题配置至少需要一个语言条目".to_string());
    }
    for (language, entry) in &self.titles {
      if entry.art.is_empty() {
        return Err(format!("标题语言 {language} 的 art 为空"));
      }
      if entry.art.iter().any(TitleArt::is_empty) {
        return Err(format!("标题语言 {language} 有空的字符画"));
      }
      if entry.name.trim().is_empty() {
        return Err(format!("标题语言 {language} 的 name 为空"));
      }
      if entry.art_interval_ms == 0 {
        return Err(format!("标题语言 {language} 的 art_interval_ms 必须大于 0"));
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
    parse(&source).map_err(|error| format!("解析 {} 失败：{error}", path.display()))?;
  config.validate()?;
  Ok(config)
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use super::*;

  /// A config with explicit arts, so the layout rules can be tested independently of the data.
  fn config_with(art: &[&str], name: &str) -> TitleConfig {
    let entry = TitleEntry {
      name: name.to_string(),
      subtitle: String::new(),
      prompt: String::new(),
      art: art.iter().map(|text| TitleArt::from_text(text)).collect(),
      art_interval_ms: 100,
      menu: Vec::new(),
    };
    TitleConfig {
      default_language: "zh".to_string(),
      titles: BTreeMap::from([("zh".to_string(), entry)]),
    }
  }

  #[test]
  fn art_string_is_split_into_frames_and_trimmed() {
    let art = TitleArt::from_text("\n  AA\n  BB\n\n  BB2\n---\n  CC\n");
    assert_eq!(art.frames.len(), 2);
    assert_eq!(art.frames[0].text, "AA\nBB\n\nBB2", "共同缩进与首尾空行被去掉，但画内空行保留");
    assert_eq!(art.frames[1].text, "CC");
    assert_eq!(art.width(), 3, "最宽的一行");
    assert_eq!(art.height(), 4, "最高的一帧");
    // Frames cycle and wrap around.
    assert_eq!(art.frame_at(0, 100).unwrap().text, "AA\nBB\n\nBB2");
    assert_eq!(art.frame_at(99, 100).unwrap().text, "AA\nBB\n\nBB2");
    assert_eq!(art.frame_at(100, 100).unwrap().text, "CC");
    assert_eq!(art.frame_at(200, 100).unwrap().text, "AA\nBB\n\nBB2", "循环回到第一帧");
  }

  #[test]
  fn widest_fitting_art_is_chosen_and_smallest_terminal_shows_the_name() {
    let config = config_with(&["WWWWWWWWWW", "WWWW", "WWWWW"], "混乱擢升");
    let entry = config.select("zh").unwrap();
    let width_of = |columns| match config.layout(entry, columns) {
    TitleLayout::Art(art) => art.width(),
    TitleLayout::Name(_) => 0,
    };
    assert_eq!(width_of(200), 10, "最宽且放得下的画优先");
    assert_eq!(width_of(10), 10);
    // "WWWWWWWWWW" has no shorter row, so it never shrinks; "WWWWW" is a single full-width row and
    // does not shrink either, but it still fits below 10 columns on its own merits.
    assert_eq!(width_of(9), 4, "退到放得下的最小画");
    assert_eq!(width_of(5), 4, "简化画可收紧边距，仍比只显示名字优先");
    assert_eq!(width_of(4), 4);
    assert_eq!(width_of(3), 0, "连最小的画都放不下时只显示 name");
    assert_eq!(config.layout(entry, 3), TitleLayout::Name("混乱擢升".to_string()));
  }

  #[test]
  fn simplified_art_may_shrink_its_margin_on_narrow_terminals() {
    // A non-simplified art (every row as wide as the frame) never lets its margin collapse.
    let config = config_with(&["XXXX", "XXXX"], "名");
    let entry = config.select("zh").unwrap();
    assert!(matches!(config.layout(entry, 3), TitleLayout::Name(_)), "满宽的行无法收紧");

    // A simplified art (a row shorter than its widest) may shrink its margin down to the name
    // width, but only once it no longer fits as-is.
    let config = config_with(&["XXXX\nX"], "名称啊");
    let entry = config.select("zh").unwrap();
    assert_eq!(entry.name_width(), 6, "三个全角字符占六格");
    assert_eq!(
      config.layout(entry, 4),
      TitleLayout::Art(TitleArt::from_text("XXXX\nX")),
      "原宽放得下时保持原样"
    );
    assert_eq!(
      config.layout(entry, 6),
      TitleLayout::Art(TitleArt::from_text("XXXX\nX")),
      "简化画可以收紧到与标题名同宽"
    );
    assert_eq!(
      config.layout(entry, 2),
      TitleLayout::Name("名称啊".to_string()),
      "连标题名都放不下时只显示名字"
    );
  }

  #[test]
  fn frames_share_a_padded_block_and_never_reflow() {
    let config = config_with(&["AA\nA\n---\nBBBB"], "名");
    let entry = config.select("zh").unwrap();
    let TitleLayout::Art(art) = config.layout(entry, 80) else {
      panic!("art should fit");
    };
    let screen = TitleScreen::new(entry, &TitleLayout::Art(art), (80, 24), "zh");
    assert_eq!(screen.frames(), 2);
    let first = screen.frame_rows(Duration::ZERO);
    let second = screen.frame_rows(Duration::from_millis(100));
    assert_eq!(first.len(), second.len(), "每帧行数相同");
    assert_eq!(first[0], "AA  ", "短行补齐到画宽");
    assert_eq!(second[0], "BBBB");
    assert_eq!(screen.frame_rows(Duration::from_millis(200)), first, "循环回第一帧");
  }
}
