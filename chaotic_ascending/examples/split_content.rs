//! One-time migration helper: split `data/content.yaml` into `data/content/*.yaml`.
//!
//! Run from the crate directory:
//!
//! ```sh
//! cargo run --example split_content
//! ```
//!
//! The script refuses to overwrite an existing `data/content/` directory so custom hand-edited
//! files cannot be lost accidentally. Remove the directory yourself if you really want to re-split
//! the legacy file.

use std::{error::Error, fs, path::Path};

use chaotic_ascending::{
  config::{parse, yaml},
  content::{Content, ContentPatch},
};

fn main() -> Result<(), Box<dyn Error>> {
  let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
  let legacy = root.join("content.yaml");
  let destination = root.join("content");
  let temporary = root.join("content.tmp");

  if !legacy.exists() {
    return Err(format!("找不到旧内容文件 {}", legacy.display()).into());
  }
  if destination.exists() {
    return Err(format!("{} 已存在，拒绝覆盖；请先移走该目录", destination.display()).into());
  }
  if temporary.exists() {
    fs::remove_dir_all(&temporary)?;
  }
  fs::create_dir_all(&temporary)?;

  let source = fs::read_to_string(&legacy)?;
  let content: Content =
    parse(&source).map_err(|error| format!("解析 {} 失败：{error}", legacy.display()))?;
  content.validate()?;

  let files = [
    (
      "skills.yaml",
      "# 混乱擢升 · 技能定义\n#\n# 可被玩家重现的技能需要 castable = true，并消耗灵、法或魂。\n",
      ContentPatch { skills: content.skills.clone(), ..ContentPatch::default() },
    ),
    (
      "items.yaml",
      "# 混乱擢升 · 物品定义\n#\n# kind：food / water / medicine / herb / vaccine / weapon / armor / device / material / key。\n",
      ContentPatch { items: content.items.clone(), ..ContentPatch::default() },
    ),
    (
      "creatures.yaml",
      "# 混乱擢升 · 生物定义\n#\n# habitats 会与地点的 tags 匹配；friendly 生物不会进入战斗。\n",
      ContentPatch { creatures: content.creatures.clone(), ..ContentPatch::default() },
    ),
    (
      "sites.yaml",
      "# 混乱擢升 · 地点定义\n#\n# links 必须双向；loot 引用 items.yaml 中的物品 ID。\n",
      ContentPatch { sites: content.sites.clone(), ..ContentPatch::default() },
    ),
    (
      "intel.yaml",
      "# 混乱擢升 · 情报定义\n#\n# category: identity / map / note / network / rumor / other\n",
      ContentPatch { intel: content.intel.clone(), ..ContentPatch::default() },
    ),
    (
      "npcs.yaml",
      "# 混乱擢升 · NPC 定义\n#\n# attributes 与 intel 会在人物二级页面显示。\n",
      ContentPatch { npcs: content.npcs.clone(), ..ContentPatch::default() },
    ),
    (
      "weaknesses.yaml",
      "# 混乱擢升 · 弱点报告\n#\n# source: network / research / combat / dissection\n",
      ContentPatch { weaknesses: content.weaknesses.clone(), ..ContentPatch::default() },
    ),
  ];

  for (name, header, section) in files {
    let mut document = header.to_string();
    document.push_str(&yaml(&section)?);
    fs::write(temporary.join(name), document)?;
  }

  fs::rename(&temporary, &destination)?;
  fs::remove_file(&legacy)?;
  println!(
    "已拆分到 {}：{} 种技能、{} 种物品、{} 种生物、{} 个地点、{} 条情报、{} 个人物、{} 条弱点。",
    destination.display(),
    content.skills.len(),
    content.items.len(),
    content.creatures.len(),
    content.sites.len(),
    content.intel.len(),
    content.npcs.len(),
    content.weaknesses.len()
  );
  Ok(())
}
