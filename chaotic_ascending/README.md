# 混乱擢升

灵感来源：[小说《混乱擢升》（未完工）](https://github.com/Minecrafthyr/ObjectiveReasonedDocument/blob/dev/zh-CN/%E5%B0%8F%E8%AF%B4/%E6%B7%B7%E4%B9%B1%E6%93%A2%E5%8D%87.md)

> 一个终端文字冒险游戏。世界在扭曲中演算，角色会自行探索、休息与战斗；你观察、记录，并从档案中拼出真相。角色会死亡，但世界、时间和档案会进入下一次循环。

## 构建

需要 nightly Rust:

```sh
cd chaotic_ascending
cargo run --release
```

数据默认读写 crate 下的 `data/`，与当前工作目录无关。用 `CHAOTIC_ASCENDING_DATA` 可以指向别的数据目录。

## 存档

- 世界存档在 `data/worlds/*.bin`，每若干回合自动保存，退出时也会保存。
- 跨循环档案在 `data/archive.bin`。
- 不同版本之间的世界存档可能不兼容；遇到「存档版本不兼容」提示时新建世界即可。

## 文档

参见 [docs/README.md](docs/README.md)：

- [玩法系统](docs/gameplay.md)：模式、循环与需求、转生与解剖、战斗
- [世界观与设定](docs/worldview.md)：世界背景、实质、扭曲值、角色身体
- [阶段演变](docs/evolution.md)：一至四阶段的世界线与内容设计
- [实现清单](docs/implemented.md)：已实现的地点、生物、物品、技能
- [技术细节](docs/technical.md)：数据格式、存档、模块结构、模拟模型
