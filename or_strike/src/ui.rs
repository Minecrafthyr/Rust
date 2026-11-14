use std::fmt::{Display, Write as _};
use std::io::ErrorKind::Other;
use std::io::Write as _;
use std::ops::{Bound, RangeBounds};
use std::str::FromStr;
use std::thread::sleep;

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, read};
use crossterm::style::Attribute::Bold;
use crossterm::style::{Print, Stylize};
use crossterm::terminal::Clear;
use crossterm::terminal::ClearType::UntilNewLine;
use crossterm::{execute, queue};

use crate::io::{AVOID_MISTAKE_INPUT_DUR, ScreenWriter};

/// Cursor position for inline text input.
///
/// Terminals that do not answer the cursor-position query fall back to the last row, which
/// keeps text input usable instead of aborting the whole screen.
fn input_cursor_position() -> (u16, u16) {
  crossterm::cursor::position().unwrap_or_else(|_| {
    (0, crossterm::terminal::size().map_or(0, |(_, rows)| rows.saturating_sub(1)))
  })
}

fn draw_input(
  s: &mut ScreenWriter, column: u16, row: u16, input: &str, placeholder: Option<&str>,
) -> Result<(), std::io::Error> {
  execute!(s.out, MoveTo(column, row), Clear(UntilNewLine))?;
  if input.is_empty() {
    if let Some(placeholder) = placeholder {
      execute!(s.out, Print(placeholder.grey()), MoveTo(column, row))?;
    }
  } else {
    execute!(s.out, Print(input))?;
  }
  Ok(())
}

#[expect(unevaluated_default_field_value)]
pub struct NumberRequester<
  'a,
  PromptT: Display,
  T: std::cmp::PartialOrd + FromStr + Display + Copy,
  RangeT: RangeBounds<T>,
> {
  pub prompt: PromptT,
  pub range: Option<RangeT> = None,
  pub check: Option<&'a dyn Fn(T) -> Option<String>> = None,
  pub default: Option<T> = None,
}
impl<
  'a,
  PromptT: Display,
  T: std::cmp::PartialOrd + FromStr + Display + Copy,
  RangeT: RangeBounds<T>,
> NumberRequester<'a, PromptT, T, RangeT>
{
  pub fn request(self) -> Result<Option<T>, std::io::Error>
  where
    <T as FromStr>::Err: std::fmt::Debug,
  {
    // The game keeps the terminal cursor hidden while it only shows passive screens; typing is the
    // only time the player needs to see it, so reveal it for the prompt and hide it again after.
    let mut s = ScreenWriter::new();
    execute!(s.out, Show)?;
    let result = self.request_visible(&mut s);
    execute!(s.out, Hide)?;
    result
  }

  fn request_visible(self, s: &mut ScreenWriter) -> Result<Option<T>, std::io::Error>
  where
    <T as FromStr>::Err: std::fmt::Debug,
  {
    let mut p = format!("\n{}", self.prompt);
    if let Some(r) = &self.range {
      _ = match r.start_bound() {
      Bound::Included(v) => write!(p, "[{}", v),
      Bound::Excluded(v) => write!(p, "({}", v),
      Bound::Unbounded => write!(p, "(-∞"),
      };
      _ = p.write_str(", ");
      _ = match r.end_bound() {
      Bound::Included(v) => write!(p, "{}]", v),
      Bound::Excluded(v) => write!(p, "{})", v),
      Bound::Unbounded => write!(p, "+∞)"),
      };
    }
    write!(p, "：").unwrap();
    s.writeln(&p)?.end()?;
    let (mut input_column, mut input_row) = input_cursor_position();
    let placeholder = self.default.map(|default| default.to_string());
    draw_input(s, input_column, input_row, "", placeholder.as_deref())?;

    let mut buf = String::new();
    loop {
      let Event::Key(KeyEvent { code, .. }) = read()? else {
        continue;
      };
      // let (x, y) = position().unwrap();
      use KeyCode::*;
      match code {
      Esc => break,
      Char(ch) if ch.is_ascii_graphic() || ch == ' ' => {
        if ch == '0' {
          buf.clear();
        } else {
          buf.push(ch);
        }
        draw_input(s, input_column, input_row, &buf, placeholder.as_deref())?;
      }
      Backspace => {
        buf.pop();
        draw_input(s, input_column, input_row, &buf, placeholder.as_deref())?;
      }
      Enter => {
        let number = if buf.is_empty() {
          if let Some(v) = self.default {
            v
          } else {
            s.endl()?.writeln(t!("ui.get_number.empty", prompt = p))?.flush()?;
            (input_column, input_row) = input_cursor_position();
            draw_input(s, input_column, input_row, &buf, placeholder.as_deref())?;
            continue;
          }
        } else {
          let Ok(number) = buf.parse() else {
            s.endl()?.writeln(t!("ui.get_number.invalid", prompt = p))?.flush()?;
            buf.clear();
            (input_column, input_row) = input_cursor_position();
            draw_input(s, input_column, input_row, &buf, placeholder.as_deref())?;
            continue;
          };
          number
        };
        if self.range.as_ref().map(|r| !r.contains(&number)).unwrap_or(false) {
          s.endl()?.writeln(t!("ui.get_number.invalid", prompt = p))?.flush()?;
          buf.clear();
          (input_column, input_row) = input_cursor_position();
          draw_input(s, input_column, input_row, &buf, placeholder.as_deref())?;
          continue;
        }
        if let Some(c) = self.check
          && let Some(f) = c(number)
        {
          s.endl()?.write(f)?.write(&p)?.endl()?.flush()?;
          buf.clear();
          (input_column, input_row) = input_cursor_position();
          draw_input(s, input_column, input_row, &buf, placeholder.as_deref())?;
          continue;
        }
        return Ok(Some(number));
      }
      _ => {}
      }
    }
    Ok(None)
  }
}
#[expect(unevaluated_default_field_value)]
pub struct StringRequester<'a, PromptT: Display> {
  pub prompt: PromptT,
  pub check: Option<&'a dyn Fn(&str) -> Option<String>> = None,
  pub default: Option<String> = None,
}
impl<'a, PromptT: Display> StringRequester<'a, PromptT> {
  pub fn request(self) -> Result<Option<String>, std::io::Error> {
    // See [`NumberRequester::request`]: the cursor is only visible while the player is typing.
    let mut s = ScreenWriter::new();
    execute!(s.out, Show)?;
    let result = self.request_visible(&mut s);
    execute!(s.out, Hide)?;
    result
  }

  fn request_visible(self, s: &mut ScreenWriter) -> Result<Option<String>, std::io::Error> {
    let mut prompt = format!("\n{}", self.prompt);
    write!(prompt, "：").unwrap();
    s.writeln(&prompt)?.end()?;
    let (mut input_column, mut input_row) = input_cursor_position();
    let placeholder = self.default.as_deref();
    draw_input(s, input_column, input_row, "", placeholder)?;

    let mut buf = String::new();
    loop {
      let Event::Key(KeyEvent { code, .. }) = read()? else {
        continue;
      };
      use KeyCode::*;
      match code {
      Esc => return Ok(None),
      Char(ch) if !ch.is_control() => {
        buf.push(ch);
        draw_input(s, input_column, input_row, &buf, placeholder)?;
      }
      Backspace =>
        if buf.pop().is_some() {
          draw_input(s, input_column, input_row, &buf, placeholder)?;
        },
      Enter => {
        let value =
          if buf.is_empty() { self.default.clone().unwrap_or_default() } else { buf.clone() };
        if let Some(message) = self.check.and_then(|check| check(&value)) {
          s.endl()?.writeln(message)?.write(&prompt)?.endl()?.flush()?;
          buf.clear();
          (input_column, input_row) = input_cursor_position();
          draw_input(s, input_column, input_row, &buf, placeholder)?;
          continue;
        }
        return Ok(Some(value));
      }
      _ => {}
      }
    }
  }
}
pub struct DataItem<
  StrT: AsRef<str>,
  SelectedFn, //FnMut(&StrT) -> StrT
  EnterFn,    //FnMut(usize) -> Result<ReT, Box<dyn ToString>>
> {
  pub text: StrT,
  pub selected: SelectedFn,
  pub enter: EnterFn,
}

/// An inline selector for a value in a range or in a fixed list of options.
///
/// The same block can be used standalone ([`ChoiceBlock::request`]), as a header of a
/// [`DisplayList`] (with [`DisplayList::choice`]), or driven directly with
/// [`ChoiceBlock::handle_key`]. Left/Right, `<`/`>`, `-`/`+` and `=` move the selection;
/// holding Ctrl moves five steps at once.
#[derive(Clone, Debug)]
pub struct ChoiceBlock {
  pub label: String,
  options: ChoiceValues,
  selected: usize,
}

#[derive(Clone, Debug)]
enum ChoiceValues {
  Options { labels: Vec<String>, wrap: bool },
  Range { min: f64, max: f64, step: f64, value: f64, decimals: usize, unit: String },
}

impl ChoiceBlock {
  pub fn options(
    label: impl Into<String>, options: impl IntoIterator<Item = impl ToString>,
  ) -> Self {
    let labels = options.into_iter().map(|option| option.to_string()).collect::<Vec<_>>();
    Self { label: label.into(), options: ChoiceValues::Options { labels, wrap: true }, selected: 0 }
  }

  pub fn range(label: impl Into<String>, min: f64, max: f64, step: f64, value: f64) -> Self {
    Self {
      label: label.into(),
      options: ChoiceValues::Range { min, max, step, value, decimals: 0, unit: String::new() },
      selected: 0,
    }
  }

  pub fn range_with(
    label: impl Into<String>, min: f64, max: f64, step: f64, value: f64, decimals: usize,
    unit: impl Into<String>,
  ) -> Self {
    Self {
      label: label.into(),
      options: ChoiceValues::Range { min, max, step, value, decimals, unit: unit.into() },
      selected: 0,
    }
  }

  /// Prevent option selection from wrapping around.
  pub fn no_wrap(mut self) -> Self {
    if let ChoiceValues::Options { wrap, .. } = &mut self.options {
      *wrap = false;
    }
    self
  }

  pub fn label(mut self, label: impl Into<String>) -> Self {
    self.label = label.into();
    self
  }

  pub fn selected(mut self, index: usize) -> Self {
    self.selected = index;
    self
  }

  pub fn selected_index(&self) -> usize { self.selected }

  /// Numeric value in range mode, selected index in option mode.
  pub fn value(&self) -> f64 {
    match &self.options {
    ChoiceValues::Options { .. } => self.selected as f64,
    ChoiceValues::Range { value, .. } => *value,
    }
  }

  pub fn set_value(&mut self, value: f64) {
    match &mut self.options {
    ChoiceValues::Options { labels, .. } =>
      if !labels.is_empty() {
        self.selected = (value.round().max(0.0) as usize).min(labels.len() - 1);
      },
    ChoiceValues::Range { min, max, value: current, decimals, .. } => {
      let factor = 10_f64.powi(*decimals as i32);
      *current = ((value.clamp(*min, *max) * factor).round() / factor).clamp(*min, *max);
    }
    }
  }

  pub fn value_text(&self) -> String {
    match &self.options {
    ChoiceValues::Options { labels, .. } => labels.get(self.selected).cloned().unwrap_or_default(),
    ChoiceValues::Range { value, decimals, unit, .. } => {
      format!("{value:.decimals$}{unit}")
    }
    }
  }

  pub fn text(&self) -> String {
    match &self.options {
    ChoiceValues::Options { labels, .. } =>
      format!("{}：◀ {} ▶（{}/{}）", self.label, self.value_text(), self.selected + 1, labels.len()),
    ChoiceValues::Range { .. } => format!("{}：◀ {} ▶", self.label, self.value_text()),
    }
  }

  pub fn prev(&mut self, fast: bool) {
    let delta = if fast { 5 } else { 1 };
    match &mut self.options {
    ChoiceValues::Options { labels, wrap } => {
      if labels.is_empty() {
        return;
      }
      if *wrap {
        self.selected = (self.selected + labels.len() * delta - delta) % labels.len();
      } else {
        self.selected = self.selected.saturating_sub(delta);
      }
    }
    ChoiceValues::Range { min, max, step, value, decimals, .. } => {
      let factor = 10_f64.powi(*decimals as i32);
      *value = ((*value - *step * delta as f64).clamp(*min, *max) * factor).round() / factor;
    }
    }
  }

  pub fn next(&mut self, fast: bool) {
    let delta = if fast { 5 } else { 1 };
    match &mut self.options {
    ChoiceValues::Options { labels, wrap } => {
      if labels.is_empty() {
        return;
      }
      if *wrap {
        self.selected = (self.selected + delta) % labels.len();
      } else {
        self.selected = (self.selected + delta).min(labels.len() - 1);
      }
    }
    ChoiceValues::Range { min, max, step, value, decimals, .. } => {
      let factor = 10_f64.powi(*decimals as i32);
      *value = ((*value + *step * delta as f64).clamp(*min, *max) * factor).round() / factor;
    }
    }
  }

  /// Applies a selector key. Returns whether the key belonged to the block.
  pub fn handle_key(&mut self, code: KeyCode, ctrl: bool) -> bool {
    use KeyCode::*;
    match code {
    Left | Char('<') | Char('-') => self.prev(ctrl),
    Right | Char('>') | Char('+') | Char('=') => self.next(ctrl),
    _ => return false,
    }
    true
  }

  /// Displays the block and lets the player change it until Enter confirms.
  pub fn request(mut self) -> Result<Option<Self>, std::io::Error> {
    let mut s = ScreenWriter::new_screen();
    loop {
      s.reset_pos()?;
      s.writeln(self.text())?.writeln(t!("ui.choice.help"))?.end()?;
      let Event::Key(KeyEvent { code, modifiers, kind, .. }) = read()? else {
        continue;
      };
      if !kind.is_press() {
        continue;
      }
      let ctrl = modifiers.contains(KeyModifiers::CONTROL);
      match code {
      KeyCode::Enter => return Ok(Some(self)),
      KeyCode::Esc => return Ok(None),
      _ => {
        self.handle_key(code, ctrl);
      }
      }
    }
  }
}

impl Display for ChoiceBlock {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.text()) }
}

pub trait DrawEntries {
  fn draw_entries(&mut self, s: &mut ScreenWriter) -> Result<(), std::io::Error>;
}
impl<TitleT: AsRef<str>, StrT: AsRef<str>, SelectedFn: FnMut(&StrT) -> StrT, EnterFn> DrawEntries
  for DisplayList<TitleT, StrT, SelectedFn, EnterFn>
{
  fn draw_entries(&mut self, s: &mut ScreenWriter) -> Result<(), std::io::Error> {
    let range = self.visible_range();
    for (i, DataItem { text, selected, .. }) in
      self.data.iter_mut().enumerate().skip(range.start).take(range.len())
    {
      if self.indexed {
        write!(s, "[{i}] ")
          .map_err(|_| std::io::Error::new(Other, "format error: cannot write to screen"))?;
      }
      if i == self.selecting {
        s.writeln((selected)(text).as_ref().attribute(Bold))?;
      } else {
        s.writeln(text.as_ref())?;
      }
    }
    Ok(())
  }
}
impl<TitleT: AsRef<str>, StrT: AsRef<str>, EnterFn> DrawEntries
  for DisplayList<TitleT, StrT, (), EnterFn>
{
  fn draw_entries(&mut self, s: &mut ScreenWriter) -> Result<(), std::io::Error> {
    let range = self.visible_range();
    for (i, DataItem { text, .. }) in
      self.data.iter_mut().enumerate().skip(range.start).take(range.len())
    {
      if self.indexed {
        write!(s, "[{i}] ")
          .map_err(|_| std::io::Error::new(Other, "format error: cannot write to screen"))?;
      }
      if i == self.selecting {
        s.writeln(text.as_ref().attribute(Bold))?;
      } else {
        s.writeln(text.as_ref())?;
      }
    }
    Ok(())
  }
}
pub trait Enter {
  type ReT;
  fn enter(&mut self, s: &mut ScreenWriter) -> Result<Option<Self::ReT>, std::io::Error>;
}

impl<
  ReT,
  TitleT: AsRef<str>,
  StrT: AsRef<str>,
  SelectedFn,
  EnterFn: FnMut(usize) -> Result<ReT, Box<dyn ToString>>,
> Enter for DisplayList<TitleT, StrT, SelectedFn, EnterFn>
{
  type ReT = ReT;

  fn enter(&mut self, s: &mut ScreenWriter) -> Result<Option<ReT>, std::io::Error> {
    let f = &mut self.data[self.selecting].enter;
    match (f)(self.selecting) {
    Ok(r) => return Ok(Some(r)),
    Err(e) => _ = s.message(e.to_string())?,
    }
    Ok(None)
  }
}
impl<TitleT: AsRef<str>, StrT: AsRef<str>, SelectedFn> Enter
  for DisplayList<TitleT, StrT, SelectedFn, ()>
{
  type ReT = ();

  fn enter(&mut self, _s: &mut ScreenWriter) -> Result<Option<()>, std::io::Error> { Ok(None) }
}
#[expect(unevaluated_default_field_value)]
pub struct DisplayList<TitleT: AsRef<str>, StrT: AsRef<str>, SelectedFn, EnterFn> {
  origin_row: u16,
  pub title: Option<TitleT> = None,
  pub data: Vec<DataItem<StrT, SelectedFn, EnterFn>>,
  pub selecting: usize = 0,
  pub overflow: bool = true,
  pub indexed: bool = false,
  /// Maximum number of entries drawn at once. `None` shows everything.
  pub height: Option<usize> = None,
  /// Optional inline selector shown under the title.
  pub choice: Option<ChoiceBlock> = None,
  /// First entry currently drawn; maintained automatically while selecting.
  scroll: usize = 0,
}
impl<TitleT: AsRef<str>, StrT: AsRef<str>, SelectedFn, EnterFn>
  DisplayList<TitleT, StrT, SelectedFn, EnterFn>
{
  pub fn new(title: TitleT, data: Vec<DataItem<StrT, SelectedFn, EnterFn>>) -> Self {
    let origin_row = crossterm::cursor::position().map_or(0, |(_, row)| row);
    Self { origin_row, title: Some(title), data, .. }
  }

  pub fn indexed(title: TitleT, data: Vec<DataItem<StrT, SelectedFn, EnterFn>>) -> Self {
    Self::new(title, data).indexing()
  }

  pub fn title(mut self, title: TitleT) -> Self {
    self.title = Some(title);
    self
  }

  /// Turn on index before display item to select instantly.
  pub fn indexing(mut self) -> Self {
    self.indexed = true;
    self
  }

  /// Select index on startup
  pub fn select_default(mut self, index: usize) -> Self {
    self.selecting = index;
    self.recalc_select();
    self
  }

  pub fn at(mut self, row: u16) -> Self {
    self.origin_row = row;
    self
  }

  /// Limit the number of entries shown at once. The selected entry always stays visible.
  pub fn height(mut self, height: usize) -> Self {
    self.height = Some(height.max(1));
    self
  }

  pub fn scroll(mut self, offset: usize) -> Self {
    self.scroll = offset;
    self
  }

  /// Attach an inline option/range selector under the list title.
  pub fn choice(mut self, choice: ChoiceBlock) -> Self {
    self.choice = Some(choice);
    self
  }

  /// Disable flow from max to zero index
  pub fn no_flow(mut self) -> Self {
    self.overflow = false;
    self
  }

  fn visible_len(&self) -> usize {
    self.height.unwrap_or(self.data.len().max(1)).max(1).min(self.data.len().max(1))
  }

  fn visible_range(&self) -> std::ops::Range<usize> {
    let len = self.data.len();
    if len == 0 {
      return 0..0;
    }
    let view = self.visible_len();
    let start = self.scroll.min(len.saturating_sub(view));
    start..start + view.min(len)
  }

  /// Keeps the selected entry inside the viewport.
  fn adjust_scroll(&mut self) {
    let len = self.data.len();
    if len == 0 {
      self.scroll = 0;
      return;
    }
    let view = self.visible_len();
    if self.selecting < self.scroll {
      self.scroll = self.selecting;
    } else if self.selecting >= self.scroll + view {
      self.scroll = self.selecting + 1 - view;
    }
    self.scroll = self.scroll.min(len.saturating_sub(view));
  }

  fn draw_header(&mut self, s: &mut ScreenWriter) -> Result<(), std::io::Error> {
    if let Some(t) = self.title.as_ref() {
      s.write(t.as_ref())?;
    }
    if self.height.is_some() && self.data.len() > self.visible_len() {
      let range = self.visible_range();
      s.write(format!("  [{}-{}/{}]", range.start + 1, range.end, self.data.len()))?;
    }
    s.endl()?;
    Ok(())
  }

  fn recalc_select(&mut self) {
    let len = self.data.len();
    if len == 0 {
      self.selecting = 0;
      self.scroll = 0;
      return;
    }
    if self.overflow {
      self.selecting %= len;
    } else if self.selecting >= len {
      self.selecting = len - 1;
    }
    self.adjust_scroll();
  }
}
impl<TitleT: AsRef<str>, StrT: AsRef<str>, SelectedFn, EnterFn>
  DisplayList<TitleT, StrT, SelectedFn, EnterFn>
where
  Self: DrawEntries + Enter,
{
  pub fn run(
    &mut self, s: &mut ScreenWriter,
  ) -> Result<Option<<Self as Enter>::ReT>, std::io::Error> {
    'io: loop {
      if self.height.is_none() {
        let (_, rows) = crossterm::terminal::size()?;
        let choice_rows = usize::from(self.choice.is_some());
        self.height = Some(
          usize::from(rows)
            .saturating_sub(usize::from(self.origin_row))
            .saturating_sub(2 + choice_rows)
            .max(3),
        );
      }
      self.adjust_scroll();
      queue!(s.out, MoveTo(self.origin_row, 0))?;
      self.draw_header(s)?;
      if let Some(choice) = self.choice.as_ref() {
        s.writeln(choice.text())?;
      }
      if self.data.is_empty() {
        s.message(t!("ui.display_list.empty"))?;
        return Ok(None);
      }
      self.draw_entries(s)?;
      s.end()?;
      let mut last_press = std::time::Instant::now();
      let mut count = 0usize;

      loop {
        let Event::Key(KeyEvent { code, kind, modifiers, .. }) = read()? else {
          continue;
        };
        let now = std::time::Instant::now();
        let ctrl = modifiers.contains(KeyModifiers::CONTROL);
        if let Some(choice) = self.choice.as_mut()
          && choice.handle_key(code, ctrl)
        {
          continue 'io;
        }
        let max_select = self.data.len() - 1;
        let view = self.visible_len();
        let fast = if ctrl { 5 } else { 1 };
        use KeyCode::*;
        match code {
        Up => {
          if self.selecting == 0 && !self.overflow {
            continue;
          }
          self.selecting = if self.overflow {
            let len = self.data.len();
            (self.selecting + len - fast % len) % len
          } else {
            self.selecting.saturating_sub(fast)
          };
          continue 'io;
        }
        Down => {
          if self.selecting == max_select && !self.overflow {
            continue;
          }
          self.selecting = if self.overflow {
            (self.selecting + fast) % self.data.len()
          } else {
            (self.selecting + fast).min(max_select)
          };
          continue 'io;
        }
        PageUp => {
          self.selecting = self.selecting.saturating_sub(view);
          self.adjust_scroll();
          continue 'io;
        }
        PageDown => {
          self.selecting = (self.selecting + view).min(max_select);
          self.adjust_scroll();
          continue 'io;
        }
        Home => {
          self.selecting = 0;
          self.adjust_scroll();
          continue 'io;
        }
        End => {
          self.selecting = max_select;
          self.adjust_scroll();
          continue 'io;
        }
        Enter => {
          if let r @ Some(_) = self.enter(s)? {
            return Ok(r);
          }
          break;
        }
        Esc => return Ok(None),
        Char(n) if self.indexed && kind.is_press() && n.is_ascii_digit() => {
          let n = n.to_digit(10).unwrap() as usize;
          if n == 0 {
            count = 0;
          } else if now - last_press > std::time::Duration::from_millis(300) {
            count = n;
          } else {
            count = count.saturating_mul(10).saturating_add(n);
          }
          self.selecting = count.min(max_select);
          self.adjust_scroll();
          queue!(s.out, MoveTo(self.origin_row, 0))?;
          self.draw_header(s)?;
          self.draw_entries(s)?;
          s.end()?;
        }
        _ => {}
        }
        if kind.is_press() {
          last_press = now;
        }
      }
    }
  }
}
impl ScreenWriter {
  pub fn list<TitleT: AsRef<str>, StrT: AsRef<str>, SelectedFn, EnterFn>(
    &mut self, mut display_list: DisplayList<TitleT, StrT, SelectedFn, EnterFn>,
  ) -> Result<Option<<DisplayList<TitleT, StrT, SelectedFn, EnterFn> as Enter>::ReT>, std::io::Error>
  where
    DisplayList<TitleT, StrT, SelectedFn, EnterFn>: DrawEntries + Enter,
  {
    display_list.run(self)
  }

  /// Like [`Self::list`], but also returns the inline option block so callers can read the
  /// value chosen with Left/Right or `-`/`+` while browsing the list.
  pub fn list_with_choice<TitleT: AsRef<str>, StrT: AsRef<str>, SelectedFn, EnterFn>(
    &mut self, mut display_list: DisplayList<TitleT, StrT, SelectedFn, EnterFn>,
  ) -> Result<
    (Option<<DisplayList<TitleT, StrT, SelectedFn, EnterFn> as Enter>::ReT>, Option<ChoiceBlock>),
    std::io::Error,
  >
  where
    DisplayList<TitleT, StrT, SelectedFn, EnterFn>: DrawEntries + Enter,
  {
    let result = display_list.run(self)?;
    Ok((result, display_list.choice))
  }

  pub fn message(&mut self, msg: impl AsRef<str>) -> Result<&mut Self, std::io::Error> {
    crate::io::record_log(msg.as_ref());
    self.message_inner(msg)
  }

  /// Shows a message without adding it to the operation history (for descriptions and help).
  pub fn message_detail(&mut self, msg: impl AsRef<str>) -> Result<&mut Self, std::io::Error> {
    self.message_inner(msg)
  }

  fn message_inner(&mut self, msg: impl AsRef<str>) -> Result<&mut Self, std::io::Error> {
    self.writeln(msg.as_ref())?;
    self.end()?;
    sleep(AVOID_MISTAKE_INPUT_DUR);
    self.writeln(t!("ui.continue_prompt"))?.flush()?;
    loop {
      if let Event::Key(key_event) = read()?
        && key_event.is_press()
      {
        return Ok(self);
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn option_choice_walks_options_and_ctrl_moves_five() {
    let mut choice = ChoiceBlock::options("test", ["a", "b", "c", "d", "e", "f", "g"]);
    assert_eq!(choice.value_text(), "a");
    choice.next(false);
    assert_eq!(choice.value_text(), "b");
    choice.next(true);
    assert_eq!(choice.value_text(), "g");
    choice.next(false);
    assert_eq!(choice.value_text(), "a");
  }

  #[test]
  fn choice_block_can_be_attached_to_a_display_list() {
    let list = DisplayList::indexed(
      "title",
      Vec::<DataItem<String, (), fn(usize) -> Result<(), Box<dyn ToString>>>>::new(),
    )
    .choice(ChoiceBlock::options("choice", ["on", "off"]));
    assert_eq!(list.choice.as_ref().map(ChoiceBlock::value_text).as_deref(), Some("on"));
  }

  #[test]
  fn range_choice_respects_bounds_and_step() {
    let mut choice = ChoiceBlock::range_with("test", 0.0, 10.0, 2.0, 4.0, 1, "");
    choice.next(false);
    assert_eq!(choice.value(), 6.0);
    choice.next(true);
    assert_eq!(choice.value(), 10.0);
    choice.prev(true);
    assert_eq!(choice.value(), 0.0);
  }
}
