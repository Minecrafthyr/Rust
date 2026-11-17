use or_strike::io::ScreenWriter;

use crate::game::{ActionError, Game};
use crate::item::{ItemStack, ItemStacks};
use crate::utils::NameAndDesc;

impl Game {
  /// Takes one worn item off. It goes into the backpack when possible; otherwise the player
  /// holds it, and only when both are full does it end up on the ground.
  pub(crate) fn unequip_item(
    &mut self, s: &mut ScreenWriter, index: usize,
  ) -> Result<(), Box<dyn std::error::Error>> {
    if index >= self.player.worn.len() {
      return Err(Box::new(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        ActionError::ItemMissing,
      )));
    }
    let item = self.player.worn.remove(index);
    self.player.update_posture_from_load();
    let name = item.name().to_string();

    let Some(stack) = self.player.insert_stack(item * 1) else {
      s.message(t!("action.worn.unequipped", name = name))?;
      return Ok(());
    };

    let held = match self.player.choose_hands(&stack.item) {
    Ok(hands) => match self.player.hold_item(stack.clone(), hands) {
    Ok(()) => true,
    Err(_) => false,
    },
    Err(_) => false,
    };
    if held {
      s.message(t!("action.worn.unequipped_held", name = name))?;
      Ok(())
    } else {
      self.drop_unequipped(s, name, stack)
    }
  }

  fn drop_unequipped(
    &mut self, s: &mut ScreenWriter, name: String, stack: ItemStack,
  ) -> Result<(), Box<dyn std::error::Error>> {
    self.drop_items_at_player(ItemStacks(vec![stack]));
    s.message(t!("action.worn.unequipped_dropped", name = name))?;
    Ok(())
  }
}
