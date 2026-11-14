//! Unified inventory screen.
//!
//! Backpack items, worn equipment, and items lying nearby are shown together. Letter keys perform
//! the actions available on the selected entry (use, wear, hold, pick up), Shift+category keys
//! toggle a category filter, and Alt+category keys show only that category.

use std::error::Error;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::Stylize;
use crossterm::terminal;
use or_strike::io::ScreenWriter;

use super::pickup::PickupOption;
use crate::game::{ActionError, Game};
use crate::item::{Item, ItemStack};
use crate::utils::{IterStack, NameAndDesc, TakeItems};

#[derive(Clone)]
enum InventorySource {
  Worn(usize),
  Carried(ItemStack),
  Nearby(PickupOption),
}

struct InventoryEntry {
  source: InventorySource,
  text: String,
  desc: String,
  usable: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InventoryAction {
  Use,
  Wear,
  Unequip,
  Hold,
  Pickup,
  Inspect,
}

impl InventoryAction {
  fn key(self) -> char {
    match self {
    Self::Use => 'u',
    Self::Wear | Self::Unequip => 'w',
    Self::Hold => 'h',
    Self::Pickup => 'p',
    Self::Inspect => 'l',
    }
  }

  fn label(self) -> String {
    match self {
    Self::Use => t!("action.inventory.act_use").into_owned(),
    Self::Wear => t!("action.inventory.act_wear").into_owned(),
    Self::Unequip => t!("action.inventory.act_unequip").into_owned(),
    Self::Hold => t!("action.inventory.act_hold").into_owned(),
    Self::Pickup => t!("action.inventory.act_pickup").into_owned(),
    Self::Inspect => t!("action.inventory.act_look").into_owned(),
    }
  }
}

fn action_hints(actions: &[InventoryAction]) -> String {
  actions
    .iter()
    .map(|action| format!("[{}]{}", action.key(), action.label()))
    .collect::<Vec<_>>()
    .join(" ")
}

fn actions_for(entry: &InventoryEntry) -> Vec<InventoryAction> {
  match &entry.source {
  InventorySource::Worn(_) => vec![InventoryAction::Unequip, InventoryAction::Inspect],
  InventorySource::Carried(stack) => {
    let mut actions = Vec::new();
    if entry.usable {
      actions.push(InventoryAction::Use);
    }
    if stack.item.wearable {
      actions.push(InventoryAction::Wear);
    }
    actions.push(InventoryAction::Hold);
    actions.push(InventoryAction::Inspect);
    actions
  }
  InventorySource::Nearby(option) => {
    let mut actions = vec![InventoryAction::Hold];
    if option.item.wearable {
      actions.push(InventoryAction::Wear);
    }
    actions.push(InventoryAction::Pickup);
    actions.push(InventoryAction::Inspect);
    actions
  }
  }
}

#[derive(Clone, Copy)]
struct InventoryFilter {
  carried: bool,
  worn: bool,
  nearby: bool,
  usable_only: bool,
}

impl Default for InventoryFilter {
  fn default() -> Self { Self { carried: true, worn: true, nearby: true, usable_only: false } }
}

impl InventoryFilter {
  /// Shift+key: toggle one filter flag.
  fn toggle(&mut self, key: char) {
    match key {
    'i' => self.carried = !self.carried,
    'w' => self.worn = !self.worn,
    'p' => self.nearby = !self.nearby,
    'u' => self.usable_only = !self.usable_only,
    _ => {}
    }
  }

  /// Alt+key: show only one kind of entry, or go back to showing everything.
  fn toggle_exclusive(&mut self, key: char) {
    match key {
    'i' => {
      let only = self.carried && !self.worn && !self.nearby;
      self.worn = only;
      self.nearby = only;
      self.carried = true;
    }
    'w' => {
      let only = !self.carried && self.worn && !self.nearby;
      self.carried = only;
      self.nearby = only;
      self.worn = true;
    }
    'p' => {
      let only = !self.carried && !self.worn && self.nearby;
      self.carried = only;
      self.worn = only;
      self.nearby = true;
    }
    'u' => self.usable_only = !self.usable_only,
    _ => {}
    }
  }
}

fn source_key(source: &InventorySource) -> char {
  match source {
  InventorySource::Worn(_) => 'w',
  InventorySource::Carried(_) => 'i',
  InventorySource::Nearby(_) => 'p',
  }
}

fn source_tag(source: &InventorySource) -> std::borrow::Cow<'_, str> {
  match source {
  InventorySource::Worn(_) => t!("action.inventory.tag_worn"),
  InventorySource::Carried(_) => t!("action.inventory.tag_carried"),
  InventorySource::Nearby(_) => t!("action.inventory.tag_nearby"),
  }
}

fn entry_quantity(entry: &InventoryEntry) -> u64 {
  match &entry.source {
  InventorySource::Worn(_) => 1,
  InventorySource::Carried(stack) => stack.count,
  InventorySource::Nearby(option) => option.count,
  }
}

fn item_usable(item: &Item) -> bool {
  !item.uses.is_empty()
    || item.compass
    || item.watch
    || item.thermometer
    || item.paused_craft.is_some()
}

impl Game {
  fn inventory_entries(&self) -> Vec<InventoryEntry> {
    let mut entries = Vec::new();
    for (index, item) in self.player.worn.iter().enumerate() {
      entries.push(InventoryEntry {
        source: InventorySource::Worn(index),
        text: t!(
          "action.inventory.worn_line",
          name = item.name(),
          volume = item.volume(),
          weight = item.weight()
        )
        .into_owned(),
        desc: item.desc().into_owned(),
        usable: false,
      });
    }
    for stack in self.player.iter_stack() {
      entries.push(InventoryEntry {
        source: InventorySource::Carried(stack.clone()),
        text: t!(
          "action.inventory.carried_line",
          name = stack.item.name(),
          count = stack.count,
          volume = stack.volume(),
          weight = stack.weight()
        )
        .into_owned(),
        desc: stack.item.desc().into_owned(),
        usable: item_usable(&stack.item),
      });
    }
    for (option, _, _) in self.pickup_options() {
      // The inventory only shows items close enough to interact with from here.
      if self.player.position.distance(option.position) > self.player.view_range() {
        continue;
      }
      let (angle, distance) = self.direction_distance_to(option.position, false);
      entries.push(InventoryEntry {
        source: InventorySource::Nearby(option.clone()),
        text: t!(
          "action.inventory.nearby_line",
          name = option.item.name(),
          count = option.count,
          angle = angle,
          distance = distance
        )
        .into_owned(),
        desc: option.item.desc().into_owned(),
        usable: item_usable(&option.item),
      });
    }
    entries
  }

  fn inventory_visible(
    entries: &[InventoryEntry], filter: InventoryFilter,
  ) -> Vec<&InventoryEntry> {
    entries
      .iter()
      .filter(|entry| match entry.source {
      InventorySource::Worn(_) => filter.worn && !filter.usable_only,
      InventorySource::Carried(_) => filter.carried && (!filter.usable_only || entry.usable),
      InventorySource::Nearby(_) => filter.nearby && (!filter.usable_only || entry.usable),
      })
      .collect()
  }

  fn inventory_header(filter: InventoryFilter) -> String {
    let tag = |enabled: bool, text: &str| {
      if enabled { text.to_string() } else { text.grey().to_string() }
    };
    [
      tag(filter.carried, &t!("action.inventory.filter_carried")),
      tag(filter.worn, &t!("action.inventory.filter_worn")),
      tag(filter.nearby, &t!("action.inventory.filter_nearby")),
      tag(filter.usable_only, &t!("action.inventory.filter_usable")),
    ]
    .join(" ")
  }

  fn render_inventory(
    &self, entries: &[&InventoryEntry], filter: InventoryFilter, selecting: usize, scroll: usize,
    view_height: usize,
  ) -> Result<usize, std::io::Error> {
    let total_objects: u64 = entries.iter().map(|entry| entry_quantity(entry)).sum();
    let mut s = ScreenWriter::new_screen();
    s.writeln(t!("action.inventory.title"))?
      .writeln(t!(
        "action.inventory.index_line",
        index = if entries.is_empty() { 0 } else { selecting + 1 },
        entries = entries.len(),
        objects = total_objects
      ))?
      .writeln(Self::inventory_header(filter))?
      .writeln(t!("action.inventory.separator"))?;
    let start = scroll.min(entries.len().saturating_sub(view_height));
    for (offset, entry) in entries.iter().skip(start).take(view_height).enumerate() {
      let index = start + offset;
      let line = format!(
        "[{}][{}] {}：{}",
        index + 1,
        source_key(&entry.source),
        source_tag(&entry.source),
        entry.text
      );
      if index == selecting {
        s.writeln(line.bold())?;
      } else {
        s.writeln(line)?;
      }
    }
    if entries.is_empty() {
      s.writeln(t!("action.inventory.empty"))?;
    }
    s.end()?;
    Ok(start)
  }

  fn render_inventory_footer(
    &self, selected: Option<&InventoryEntry>, actions: &[InventoryAction],
  ) -> Result<(), std::io::Error> {
    let mut s = ScreenWriter::new();
    s.writeln(t!("action.inventory.filter_help"))?.writeln(t!("action.inventory.footer"))?;
    if let Some(entry) = selected {
      s.writeln(t!("action.inventory.selected", text = entry.text, desc = entry.desc))?;
      if !actions.is_empty() {
        s.writeln(t!("action.inventory.actions", actions = action_hints(actions)))?;
      }
    }
    self.write_log_tail(&mut s, 2)?;
    s.end()
  }

  fn activate_inventory_entry(
    &mut self, screen: &mut ScreenWriter, entry: &InventoryEntry,
  ) -> Result<(), Box<dyn Error>> {
    match &entry.source {
    InventorySource::Worn(index) => self.unequip_item(screen, *index)?,
    InventorySource::Carried(stack) => self.activate_carried(screen, stack)?,
    InventorySource::Nearby(option) => self.pickup_target(screen, option)?,
    }
    Ok(())
  }

  fn activate_carried(
    &mut self, screen: &mut ScreenWriter, stack: &ItemStack,
  ) -> Result<(), Box<dyn Error>> {
    if item_usable(&stack.item) {
      return self.use_item_target(screen, stack.clone());
    }
    if stack.item.wearable {
      self.equip_item(&stack.item)?;
      screen.message(t!("action.inventory.equipped", name = stack.item.name()))?;
      return Ok(());
    }
    screen.message(t!(
      "action.inventory.no_action",
      name = stack.item.name(),
      desc = stack.item.desc()
    ))?;
    Ok(())
  }

  /// Use the selected entry as the natural action for its kind.
  fn inventory_enter(
    &mut self, screen: &mut ScreenWriter, entry: &InventoryEntry,
  ) -> Result<(), Box<dyn Error>> {
    self.activate_inventory_entry(screen, entry)
  }

  fn inventory_action(
    &mut self, screen: &mut ScreenWriter, entry: &InventoryEntry, action: InventoryAction,
  ) -> Result<(), Box<dyn Error>> {
    match action {
    InventoryAction::Use => match &entry.source {
    InventorySource::Carried(stack) if item_usable(&stack.item) => {
      self.use_item_target(screen, stack.clone())?;
    }
    _ => {
      screen.message(t!("action.inventory.not_usable"))?;
    }
    },
    InventoryAction::Wear => match &entry.source {
    InventorySource::Carried(stack) if stack.item.wearable => {
      self.equip_item(&stack.item)?;
      screen.message(t!("action.inventory.equipped", name = stack.item.name()))?;
    }
    InventorySource::Nearby(option) if option.item.wearable => {
      self.wear_target(screen, option)?;
    }
    _ => {
      screen.message(t!("action.inventory.not_wearable"))?;
    }
    },
    InventoryAction::Unequip => match &entry.source {
    InventorySource::Worn(index) => {
      self.unequip_item(screen, *index)?;
    }
    _ => {
      screen.message(t!("action.inventory.not_worn"))?;
    }
    },
    InventoryAction::Hold => match &entry.source {
    InventorySource::Carried(stack) => {
      self.hold_carried(screen, stack)?;
    }
    InventorySource::Nearby(option) => {
      self.hold_target(screen, option)?;
    }
    InventorySource::Worn(index) => {
      // Taking an item off already falls back to the hand when the backpack is full.
      self.unequip_item(screen, *index)?;
    }
    },
    InventoryAction::Pickup => match &entry.source {
    InventorySource::Nearby(option) => {
      self.pickup_target(screen, option)?;
    }
    _ => {
      screen.message(t!("action.inventory.not_nearby"))?;
    }
    },
    InventoryAction::Inspect => {
      screen.message_detail(t!(
        "action.inventory.inspect",
        name = entry.text,
        desc = entry.desc
      ))?;
    }
    }
    Ok(())
  }

  fn hold_carried(
    &mut self, screen: &mut ScreenWriter, stack: &ItemStack,
  ) -> Result<(), Box<dyn Error>> {
    let hands = match self.player.choose_hands(&stack.item) {
    Ok(hands) => hands,
    Err(error) => {
      screen.message(error.to_string())?;
      return Ok(());
    }
    };
    let (mut removed, mismatch) = self.player.take_items(|item| item == &stack.item, 1);
    if mismatch != 0 {
      screen.message(t!("action.inventory.not_carried"))?;
      return Ok(());
    }
    let Some(mut item) = removed.pop() else {
      screen.message(t!("action.inventory.not_carried"))?;
      return Ok(());
    };
    item.count = 1;
    let name = item.item.name().to_string();
    if let Err(item) = self.player.hold_item(item, hands) {
      if let Some(left) = self.player.insert_stack(item) {
        self.drop_items_at_player(crate::item::ItemStacks(vec![left]));
      }
      screen.message(t!("action.inventory.hand_unavailable"))?;
      return Ok(());
    }
    screen.message(t!("action.inventory.held", name = name))?;
    Ok(())
  }

  pub fn inventory(&mut self) -> Result<(), Box<dyn Error>> {
    let mut filter = InventoryFilter::default();
    let mut selecting = 0usize;
    let mut scroll = 0usize;
    loop {
      let entries = self.inventory_entries();
      let visible = Self::inventory_visible(&entries, filter);
      selecting = if visible.is_empty() { 0 } else { selecting.min(visible.len() - 1) };
      let (_, rows) = terminal::size()?;
      let view_height = usize::from(rows).saturating_sub(12).max(3);
      if selecting < scroll {
        scroll = selecting;
      } else if selecting >= scroll + view_height {
        scroll = selecting + 1 - view_height;
      }
      scroll = scroll.min(visible.len().saturating_sub(view_height));
      scroll = self.render_inventory(&visible, filter, selecting, scroll, view_height)?;
      let selected = visible.get(selecting).copied();
      let actions = selected.map(actions_for).unwrap_or_default();
      self.render_inventory_footer(selected, &actions)?;

      let KeyEvent { code, modifiers, kind, .. } = self.read_key()?;
      if !kind.is_press() {
        continue;
      }
      let ctrl = modifiers.contains(KeyModifiers::CONTROL);
      let alt = modifiers.contains(KeyModifiers::ALT);
      let step = if ctrl { 5 } else { 1 };
      match code {
      KeyCode::Up => selecting = selecting.saturating_sub(step),
      KeyCode::Down =>
        if !visible.is_empty() {
          selecting = (selecting + step).min(visible.len() - 1);
        },
      KeyCode::PageUp => selecting = selecting.saturating_sub(view_height),
      KeyCode::PageDown =>
        if !visible.is_empty() {
          selecting = (selecting + view_height).min(visible.len() - 1);
        },
      KeyCode::Home => selecting = 0,
      KeyCode::End => selecting = visible.len().saturating_sub(1),
      KeyCode::Esc => return Ok(()),
      KeyCode::Enter =>
        if let Some(entry) = visible.get(selecting).copied() {
          let mut screen = ScreenWriter::new_screen();
          self.inventory_enter(&mut screen, entry)?;
        },
      KeyCode::Char(ch) => {
        let key = ch.to_ascii_lowercase();
        let shifted = ch.is_ascii_uppercase() || modifiers.contains(KeyModifiers::SHIFT);
        if alt && matches!(key, 'i' | 'w' | 'p' | 'u') {
          filter.toggle_exclusive(key);
          continue;
        }
        if shifted && matches!(key, 'i' | 'w' | 'p' | 'u') {
          filter.toggle(key);
          continue;
        }
        let Some(entry) = visible.get(selecting).copied() else { continue };
        if let Some(action) = actions_for(entry).into_iter().find(|action| action.key() == key) {
          let mut screen = ScreenWriter::new_screen();
          self.inventory_action(&mut screen, entry, action)?;
        }
      }
      _ => {}
      }
    }
  }

  pub(super) fn equip_item(&mut self, item: &Item) -> Result<(), std::io::Error> {
    if !item.wearable {
      return Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        ActionError::ItemNotWearable,
      ));
    }
    let (mut removed, mismatch) = self.player.take_items(|candidate| candidate == item, 1);
    if mismatch != 0 {
      return Err(std::io::Error::new(std::io::ErrorKind::NotFound, ActionError::ItemMissing));
    }
    let Some(stack) = removed.pop() else {
      return Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        ActionError::ItemRemovalFailed,
      ));
    };
    self.player.worn.push(stack.item);
    self.player.update_posture_from_load();
    Ok(())
  }
}

#[cfg(test)]
mod tests;
