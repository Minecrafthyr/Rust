//! Timestamped operation log with merging and occurrence switching.

use std::error::Error;
use std::sync::{LazyLock, Mutex};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::Stylize;
use crossterm::terminal;
use or_strike::io::{ScreenWriter, log_snapshot};

use crate::game::Game;
use crate::units::Duration;

static LOG_TIMES: LazyLock<Mutex<Vec<Duration>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Keeps a timestamp list aligned with the shared operation log.
pub(crate) fn sync_log_times(now: Duration) {
  let length = log_snapshot().len();
  let Ok(mut times) = LOG_TIMES.lock() else { return };
  if times.len() > length {
    times.truncate(length);
  }
  while times.len() < length {
    times.push(now);
  }
}

#[derive(Clone)]
struct LogEntry {
  time: Duration,
  message: String,
}

impl LogEntry {
  fn text(&self, count: usize) -> String {
    let message = if count > 1 {
      t!("game.log_repeat", message = self.message, count = count).into_owned()
    } else {
      self.message.clone()
    };
    format!("{} {message}", Game::format_clock_time(self.time))
  }
}

/// A log line plus every time it occurred, in chronological order.
struct LogGroup {
  switch: String,
  occurrences: Vec<LogEntry>,
  member: usize,
}

impl LogGroup {
  fn current(&self) -> &LogEntry { &self.occurrences[self.member] }
}

fn log_entries(now: Duration) -> Vec<LogEntry> {
  sync_log_times(now);
  let messages = log_snapshot();
  let times = LOG_TIMES.lock().map(|times| times.clone()).unwrap_or_default();
  messages
    .into_iter()
    .enumerate()
    .map(|(index, message)| LogEntry { time: times.get(index).copied().unwrap_or(now), message })
    .collect()
}

fn group_log_entries(
  entries: Vec<LogEntry>, members: &std::collections::HashMap<String, usize>, merge: bool,
) -> Vec<LogGroup> {
  let mut groups = if merge {
    let mut grouped: std::collections::HashMap<String, Vec<LogEntry>> =
      std::collections::HashMap::new();
    for entry in entries {
      grouped.entry(entry.message.clone()).or_default().push(entry);
    }
    grouped
      .into_iter()
      .map(|(switch, occurrences)| {
        let member = members.get(&switch).copied().unwrap_or(0).min(occurrences.len() - 1);
        LogGroup { switch, occurrences, member }
      })
      .collect::<Vec<_>>()
  } else {
    entries
      .into_iter()
      .map(|entry| LogGroup { switch: entry.message.clone(), occurrences: vec![entry], member: 0 })
      .collect::<Vec<_>>()
  };
  groups
    .sort_by(|a, b| a.current().time.cmp(&b.current().time).then_with(|| a.switch.cmp(&b.switch)));
  groups
}

fn time_distance(a: Duration, b: Duration) -> Duration { if a > b { a - b } else { b - a } }

impl Game {
  /// Writes the most recent operation messages for a screen footer.
  pub(crate) fn write_log_tail(
    &self, screen: &mut ScreenWriter, lines: usize,
  ) -> Result<(), std::io::Error> {
    let entries = log_entries(self.time);
    if entries.is_empty() {
      return Ok(());
    }
    let mut merged: Vec<(LogEntry, usize)> = Vec::new();
    for entry in entries {
      if let Some((last, count)) = merged.last_mut()
        && last.message == entry.message
      {
        last.time = entry.time;
        *count += 1;
      } else {
        merged.push((entry, 1));
      }
    }
    screen.writeln(t!("game.log_footer"))?;
    for (entry, count) in merged.iter().rev().take(lines).rev() {
      screen.writeln(format!("  {}", entry.text(*count)))?;
    }
    Ok(())
  }

  /// Shows every completed or failed operation recorded this session.
  pub fn show_log(&mut self) -> Result<(), Box<dyn Error>> {
    let mut merge = true;
    let mut members: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut selected: Option<String> = None;
    let mut selected_time = Duration::ZERO;
    let mut selecting = 0usize;
    let mut scroll = 0usize;
    loop {
      let groups = group_log_entries(log_entries(self.time), &members, merge);
      if groups.is_empty() {
        selecting = 0;
        selected = None;
      } else {
        let by_switch = selected.as_ref().and_then(|switch| {
          groups
            .iter()
            .enumerate()
            .filter(|(_, group)| &group.switch == switch)
            .min_by(|(_, a), (_, b)| {
              a.current().time.cmp(&b.current().time).then_with(|| {
                time_distance(a.current().time, selected_time)
                  .cmp(&time_distance(b.current().time, selected_time))
              })
            })
            .map(|(index, _)| index)
        });
        selecting = by_switch.unwrap_or_else(|| selecting.min(groups.len() - 1));
        selected = Some(groups[selecting].switch.clone());
        selected_time = groups[selecting].current().time;
      }
      let (_, rows) = terminal::size()?;
      let view_height = usize::from(rows).saturating_sub(10).max(3);
      if selecting < scroll {
        scroll = selecting;
      } else if selecting >= scroll + view_height {
        scroll = selecting + 1 - view_height;
      }
      scroll = scroll.min(groups.len().saturating_sub(view_height));
      let total: usize = groups.iter().map(|group| group.occurrences.len()).sum();
      let mut screen = ScreenWriter::new_screen();
      screen
        .writeln(t!("action.log.title"))?
        .writeln(t!(
          "action.log.index_line",
          index = if groups.is_empty() { 0 } else { selecting + 1 },
          groups = groups.len(),
          entries = total,
          merge = if merge { t!("action.log.merge_on") } else { t!("action.log.merge_off") }
        ))?
        .writeln(t!("action.log.separator"))?;
      let start = scroll.min(groups.len().saturating_sub(view_height));
      for (offset, group) in groups.iter().skip(start).take(view_height).enumerate() {
        let index = start + offset;
        let entry = group.current();
        let mut line = format!("[{}] {}", index + 1, entry.text(1));
        if group.occurrences.len() > 1 {
          line.push_str(&t!(
            "action.log.merged",
            member = group.member + 1,
            total = group.occurrences.len()
          ));
        }
        if index == selecting {
          screen.writeln(line.bold())?;
        } else {
          screen.writeln(line)?;
        }
      }
      if groups.is_empty() {
        screen.writeln(t!("action.log.empty"))?;
      }
      screen.writeln(t!("action.log.footer"))?.end()?;

      let KeyEvent { code, modifiers, kind, .. } = self.read_key()?;
      if !kind.is_press() {
        continue;
      }
      let ctrl = modifiers.contains(KeyModifiers::CONTROL);
      let fast = if ctrl { 5 } else { 1 };
      match code {
      KeyCode::Up => selecting = selecting.saturating_sub(fast),
      KeyCode::Down =>
        if !groups.is_empty() {
          selecting = (selecting + fast).min(groups.len() - 1);
        },
      KeyCode::PageUp => selecting = selecting.saturating_sub(view_height),
      KeyCode::PageDown =>
        if !groups.is_empty() {
          selecting = (selecting + view_height).min(groups.len() - 1);
        },
      KeyCode::Home => selecting = 0,
      KeyCode::End => selecting = groups.len().saturating_sub(1),
      KeyCode::Left => {
        let Some(group) = groups.get(selecting) else { continue };
        if group.occurrences.len() > 1 {
          let member =
            if group.member == 0 { group.occurrences.len() - 1 } else { group.member - 1 };
          members.insert(group.switch.clone(), member);
        } else if let Some(index) = same_message_group(&groups, selecting, false) {
          selecting = index;
          selected = Some(groups[index].switch.clone());
          selected_time = groups[index].current().time;
        }
      }
      KeyCode::Right => {
        let Some(group) = groups.get(selecting) else { continue };
        if group.occurrences.len() > 1 {
          let member = (group.member + 1) % group.occurrences.len();
          members.insert(group.switch.clone(), member);
        } else if let Some(index) = same_message_group(&groups, selecting, true) {
          selecting = index;
          selected = Some(groups[index].switch.clone());
          selected_time = groups[index].current().time;
        }
      }
      KeyCode::Esc => return Ok(()),
      KeyCode::Char('c') => merge = !merge,
      _ => {}
      }
    }
  }
}

fn same_message_group(groups: &[LogGroup], selecting: usize, forward: bool) -> Option<usize> {
  let switch = &groups.get(selecting)?.switch;
  let found = if forward {
    groups.iter().enumerate().skip(selecting + 1).find(|(_, group)| &group.switch == switch)
  } else {
    groups.iter().enumerate().take(selecting).rev().find(|(_, group)| &group.switch == switch)
  };
  found.map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn entry(time: i64, message: &str) -> LogEntry {
    LogEntry { time: Duration::s(time), message: message.to_string() }
  }

  #[test]
  fn merging_groups_identical_messages_and_keeps_occurrences() {
    let entries = vec![entry(1, "a"), entry(2, "b"), entry(3, "a"), entry(4, "a")];
    let groups = group_log_entries(entries, &std::collections::HashMap::new(), true);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].switch, "a");
    assert_eq!(groups[0].occurrences.len(), 3);
    assert_eq!(groups[1].switch, "b");
  }

  #[test]
  fn unmerged_groups_keep_every_entry() {
    let entries = vec![entry(1, "a"), entry(2, "b"), entry(3, "a")];
    let groups = group_log_entries(entries, &std::collections::HashMap::new(), false);
    assert_eq!(groups.len(), 3);
    assert_eq!(groups.iter().map(|group| group.occurrences.len()).sum::<usize>(), 3);
  }
}
