use std::{
  collections::HashSet,
  error::Error,
  io::Write,
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use crossterm::event::{Event, KeyCode::Char, KeyEvent, read};
use or_strike::{
  io::ScreenWriter,
  ui::{DataItem, DisplayList, StringRequester},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
  City,
  Rural,
  Wilderness,
}

impl Location {
  pub fn name(self) -> &'static str {
    match self {
    Self::City => "城市废墟",
    Self::Rural => "乡村",
    Self::Wilderness => "无人区",
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Phase(u8);
impl From<f64> for Phase {
  fn from(distortion: f64) -> Self {
    Self(if distortion >= 24.0 {
      3
    } else if distortion >= 8.0 {
      2
    } else {
      1
    })
  }
}
impl Phase {
  fn desc(&self) -> &'static str {
    [
      "周围暂时看起来正常，但精密仪器的读数总有偏差。",
      "空气中多了无法解释的力量，远处偶尔传来非人的声响。",
      "电力和通讯彻底失常，现实本身也开始变得不可靠。",
    ][self.0 as usize]
  }

  fn title(&self) -> &'static str {
    ["第一阶段：异常初现", "第二阶段：超凡显现", "第三阶段：秩序崩坏"][self.0 as usize]
  }
}

#[derive(Clone, Copy, Debug)]
pub struct Stats {
  pub strength: u8,
  pub agility: u8,
  pub will: u8,
}

pub struct Player {
  pub name: String,
  pub location: Location,
  pub health: i32,
  pub hunger: u8,
  pub fatigue: u8,
  pub stats: Stats,
  pub food: u8,
  pub medicine: u8,
  pub herbs: u8,
  pub materials: u8,
}

impl Player {
  pub fn new(name: String, location: Location) -> Self {
    Self {
      name,
      location,
      health: 100,
      hunger: 8,
      fatigue: 5,
      stats: Stats { strength: 3, agility: 3, will: 3 },
      food: 2,
      medicine: 2,
      herbs: 0,
      materials: 0,
    }
  }
}

pub struct Enemy {
  pub name: &'static str,
  pub health: i32,
  damage: i32,
  material: &'static str,
}

pub struct Game {
  pub player: Player,
  pub world: World,
  pub enemy: Option<Enemy>,
  random: Random,
  discovered: HashSet<&'static str>,
}

pub struct World {
  pub distortion: f64,
  increment: f64,
  acceleration: f64,
  turn: u32,
}

impl Game {
  pub fn new() -> Result<Self, Box<dyn Error>> {
    let mut s = ScreenWriter::new_screen();
    s.setup()?;
    s.writeln("=== 混乱擢升 ===")?.write_with_after_delay("异界，地球", 5.0)?;
    'input_name: loop {
      let Ok(Some(name)) =
        (StringRequester { prompt: "你的名字", default: Some("幸存者".to_string()), .. }).request()
      else {
        continue;
      };
      loop {
        match s.list(DisplayList::new("选择起始位置", vec![
          DataItem {
            text: "城市",
            selected: (),
            enter: Box::new(|_| Ok(Location::City))
              as Box<dyn FnMut(usize) -> Result<Location, Box<dyn ToString>>>,
          },
          DataItem { text: "乡村", selected: (), enter: Box::new(|_| Ok(Location::Rural)) },
          DataItem { text: "荒野", selected: (), enter: Box::new(|_| Ok(Location::Wilderness)) },
        ]))? {
        Some(location) => {
          let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(1);
          return Ok(Self {
            player: Player::new(name, location),
            world: World { distortion: 0.0, increment: 0.55, acceleration: 0.12, turn: 0 },
            enemy: None,
            random: Random::new(seed),
            discovered: HashSet::new(),
          });
        }
        None => continue 'input_name,
        }
      }
    }
  }

  pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
    loop {
      let mut s = self.describe()?;

      let Event::Key(KeyEvent { code, .. }) = read()? else {
        continue;
      };
      let enemy_was_present = self.enemy.is_some();
      let advances_time = match code {
      Char('H') => {
        s.writeln(
          "s / 探索  搜寻补给、草药，或遭遇异变生物
  a / 攻击  攻击当前敌人
  f / 逃跑  尝试逃离当前敌人
  e / 吃    消耗食物缓解饥饿
  h / 治疗  使用药品或草药恢复生命
  r / 休息  恢复体力并少量恢复生命
  S / 状态  查看属性、身体状态和物资
  H / 帮助
  Q / 退出游戏",
        )?;
        loop {
          if let Event::Key(KeyEvent { .. }) = read()? {
            break;
          }
        }
        false
      }
      Char('S') => {
        self.status()?;
        false
      }
      Char('Q') => return Ok(()),
      Char('g') => {
        if self.enemy.is_some() {
          s.write("敌人就在眼前，先处理它再探索。")?;
          continue;
        }
        self.search()?
      }
      Char('e') => self.eat()?,
      Char('h') => self.heal()?,
      Char('a') => self.attack()?,
      Char('f') => self.flee()?,
      Char('r') => self.rest()?,
      _ => continue,
      };

      if self.player.health <= 0 {
        s.quit_with_message(
          format!("你倒在了异界。最终扭曲值：{:.1}。", self.world.distortion),
          Duration::from_secs(1),
        )?;
        return Ok(());
      }
      if advances_time {
        if enemy_was_present && !matches!(code, Char('a' | 'f')) {
          self.enemy_turn()?;
        }
      }
      if advances_time {
        self.advance_turn()?;
      }
    }
  }

  pub fn describe(&self) -> Result<ScreenWriter, Box<dyn Error>> {
    let mut s = ScreenWriter::new_screen();
    writeln!(
      s,
      "{} | {} | 扭曲值 {:.1} | 第 {} 回合",
      self.player.location.name(),
      Phase::from(self.world.distortion).title(),
      self.world.distortion,
      self.world.turn + 1
    )?;
    s.writeln(Phase::from(self.world.distortion).desc())?;
    if let Some(enemy) = &self.enemy {
      writeln!(s, "威胁：{}（生命 {}）", enemy.name, enemy.health)?;
    }
    s.writeln("s")?.end()?;

    Ok(s)
  }

  pub fn status(&self) -> Result<(), Box<dyn Error>> {
    let mut s = ScreenWriter::new_screen();
    let player = &self.player;
    writeln!(
      s,
      "{} | 生命 {}/100 | 饥饿 {}% | 疲倦 {}%",
      player.name,
      player.health.max(0),
      player.hunger,
      player.fatigue
    )?;
    writeln!(
      s,
      "力量 {} | 敏捷 {} | 意志 {} | 位置 {}",
      player.stats.strength,
      player.stats.agility,
      player.stats.will,
      player.location.name()
    )?;
    writeln!(
      s,
      "食物 {} | 药品 {} | 草药 {} | 材料 {} | 扭曲 {:.1}",
      player.food, player.medicine, player.herbs, player.materials, self.world.distortion
    )?;
    if let Some(enemy) = &self.enemy {
      writeln!(s, "当前敌人：{}（生命 {}）", enemy.name, enemy.health)?;
    };
    s.end()?;
    loop {
      if let Event::Key(KeyEvent { .. }) = read()? {
        break;
      }
    }
    Ok(())
  }

  pub fn search(&mut self) -> Result<bool, Box<dyn Error>> {
    self.player.fatigue = self.player.fatigue.saturating_add(5).min(100);
    let phase = Phase::from(self.world.distortion);
    let roll = self.random.range(100);
    let mut s = ScreenWriter::new_screen();

    match phase.0 {
    1 => match roll {
    0..=17 => {
      self.player.food += 1;
      writeln!(s, "你在废弃的背包里找到一份食物。")?;
    }
    18..=26 => {
      self.player.medicine += 1;
      writeln!(s, "你找到一盒尚未过期的急救药品。")?;
    }
    27..=33 => {
      self.player.food += 1;
      writeln!(s, "你遇到一名惊慌的幸存者。对方留下物资后匆忙离开。")?;
    }
    _ => writeln!(s, "你仔细搜寻了一圈，没有找到有用的东西。")?,
    },
    2 => match roll {
    0..=21 => {
      self.player.food += 1;
      writeln!(s, "你找到一些保存完好的食物。")?;
    }
    22..=33 => {
      self.player.herbs += 1;
      writeln!(s, "你发现一株散发微光的药草。")?;
    }
    34..=58 => self.spawn_enemy(EnemyKind::Zombie)?,
    59..=64 => writeln!(s, "一个有理智的幸存者提醒你：不要相信所有看起来像人的东西。")?,
    _ => writeln!(s, "除了不自然的寂静，你什么也没发现。")?,
    },
    3 => match roll {
    0..=14 => {
      self.player.food += 1;
      writeln!(s, "你从残骸中翻到一份食物。")?;
    }
    15..=22 => {
      self.player.herbs += 1;
      writeln!(s, "你采到一株变异药草。")?;
    }
    23..=66 => {
      let kind = match self.random.range(3) {
      0 => EnemyKind::Zombie,
      1 => EnemyKind::Slime,
      _ => EnemyKind::Vine,
      };
      self.spawn_enemy(kind)?;
    }
    _ => writeln!(s, "周围的空间短暂地扭曲了一下，你决定不再深入。")?,
    },
    _ => {}
    }
    Ok(true)
  }

  pub fn rest(&mut self) -> Result<bool, Box<dyn Error>> {
    let mut s = ScreenWriter::new();
    self.player.fatigue = self.player.fatigue.saturating_sub(28);
    let recovery = if self.player.hunger < 70 { 8 } else { 2 };
    self.player.health = (self.player.health + recovery).min(100);
    writeln!(s, "你找了个相对安全的地方休息，恢复了体力和 {recovery} 点生命。")?;
    Ok(true)
  }

  pub fn eat(&mut self) -> Result<bool, Box<dyn Error>> {
    let mut s = ScreenWriter::new();
    if self.player.food == 0 {
      writeln!(s, "没有食物了。")?;
      return Ok(false);
    }
    self.player.food -= 1;
    self.player.hunger = self.player.hunger.saturating_sub(32);
    writeln!(s, "你吃下一份食物，饥饿感减轻了。")?;
    Ok(true)
  }

  pub fn heal(&mut self) -> Result<bool, Box<dyn Error>> {
    let mut s = ScreenWriter::new();
    if self.player.health >= 100 {
      writeln!(s, "你目前不需要治疗。")?;
      return Ok(false);
    }
    if self.player.medicine > 0 {
      self.player.medicine -= 1;
      self.player.health = (self.player.health + 35).min(100);
      writeln!(s, "你使用急救药品，恢复了生命。")?;
      Ok(true)
    } else if self.player.herbs > 0 {
      self.player.herbs -= 1;
      self.player.health = (self.player.health + 20).min(100);
      writeln!(s, "你使用药草，恢复了一些生命。")?;
      Ok(true)
    } else {
      writeln!(s, "没有可用的药品或药草。")?;
      Ok(false)
    }
  }

  pub fn attack(&mut self) -> Result<bool, Box<dyn Error>> {
    let mut s = ScreenWriter::new();
    let Some(enemy) = &mut self.enemy else {
      writeln!(s, "附近没有敌人。")?;
      return Ok(false);
    };

    let fatigue_penalty = i32::from(self.player.fatigue >= 70);
    let hunger_penalty = i32::from(self.player.hunger >= 70);
    let damage = 4 + i32::from(self.player.stats.strength) * 3 + self.random.range(5) as i32
      - fatigue_penalty * 2
      - hunger_penalty * 2;
    enemy.health -= damage.max(1);
    writeln!(s, "你攻击{}，造成 {} 点伤害。", enemy.name, damage.max(1))?;

    if enemy.health <= 0 {
      let dead = self.enemy.take().expect("enemy exists");
      self.player.materials += 1;
      writeln!(s, "{}倒下了。你收集了一些{}。", dead.name, dead.material)?;
    } else {
      self.enemy_turn()?;
    }
    Ok(true)
  }

  pub fn flee(&mut self) -> Result<bool, Box<dyn Error>> {
    let mut s = ScreenWriter::new();
    if self.enemy.is_none() {
      writeln!(s, "没有需要逃离的敌人。")?;
      return Ok(false);
    }
    let chance = 35 + u32::from(self.player.stats.agility) * 10;
    if self.random.range(100) < chance {
      self.enemy = None;
      writeln!(s, "你成功摆脱了敌人。")?;
    } else {
      writeln!(s, "你没能逃脱！")?;
      self.enemy_turn()?;
    }
    Ok(true)
  }

  pub fn advance_turn(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    self.world.turn += 1;
    self.world.distortion += self.world.increment;
    self.world.increment += self.world.acceleration;
    self.player.hunger = self.player.hunger.saturating_add(4).min(100);
    self.player.fatigue = self.player.fatigue.saturating_add(4).min(100);
    let mut s = ScreenWriter::new();
    if self.player.hunger >= 70 {
      self.player.health -= 2;
      writeln!(s, "饥饿正在削弱你的身体。")?;
    }
    if self.player.fatigue >= 85 {
      self.player.health -= 1;
      writeln!(s, "过度疲倦使你难以保持清醒。")?;
    }
    Ok(())
  }

  fn spawn_enemy(&mut self, kind: EnemyKind) -> Result<(), Box<dyn std::error::Error>> {
    let mut s = ScreenWriter::new();
    let (name, health, damage, material) = match kind {
    EnemyKind::Zombie => ("丧尸", 28, 8, "丧尸组织"),
    EnemyKind::Slime => ("黏液怪", 34, 7, "黏液样本"),
    EnemyKind::Vine => ("荆棘藤蔓", 38, 10, "坚韧藤材"),
    };
    let (health, damage) = if Phase::from(self.world.distortion) == Phase(3) {
      (health + 12, damage + 3)
    } else {
      (health, damage)
    };
    self.enemy = Some(Enemy { name, health, damage, material });
    if self.discovered.insert(name) {
      writeln!(s, "档案解锁：{}。", name)?;
    }
    writeln!(s, "遭遇{}！它挡住了你的去路。", name)?;
    Ok(())
  }

  pub fn enemy_turn(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    let mut s = ScreenWriter::new();
    let Some(enemy) = &self.enemy else {
      return Ok(());
    };
    let defense = i32::from(self.player.stats.agility) / 2;
    let damage = (enemy.damage - defense + self.random.range(4) as i32).max(1);
    self.player.health -= damage;
    writeln!(s, "{}袭击了你，受到 {} 点伤害。", enemy.name, damage)?;
    Ok(())
  }
}

#[derive(Clone, Copy)]
enum EnemyKind {
  Zombie,
  Slime,
  Vine,
}

struct Random(u64);

impl Random {
  fn new(seed: u64) -> Self { Self(seed.max(1)) }

  fn range(&mut self, upper: u32) -> u32 {
    self.0 ^= self.0 << 13;
    self.0 ^= self.0 >> 7;
    self.0 ^= self.0 << 17;
    (self.0 % u64::from(upper)) as u32
  }
}
