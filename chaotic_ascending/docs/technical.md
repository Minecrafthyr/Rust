# 技术细节

> 本页面向修改数据、扩展内容或参与开发的读者。剧透很少，但会提到阶段编号与存档字段。

## 结构

```
chaotic_ascending/
├── Cargo.toml
├── data/                 运行时数据、存档与缓存（部分文件首次运行生成）
│   ├── titles.json5      标题画面与横向主菜单（按语言）
│   ├── config.json5      全局参数（首次运行生成）
│   ├── content/          内容补丁目录，加载全部 *.json5
│   ├── cache/content.bin 内容二进制缓存
│   ├── worlds/*.bin      世界存档
│   └── archive.bin       跨循环档案
├── docs/                 文档（本目录）
├── examples/split_content.rs
└── src/
    ├── main.rs           启动、终端初始化与退出清理
    ├── lib.rs            模块导出
    ├── config.rs         路径、Config、内容加载与缓存
    ├── content.rs        内容定义与校验
    ├── title.rs          标题配置
    ├── model/            与终端无关的模拟
    │   ├── sim.rs        循环、状态、时间推进、转生与解剖判定
    │   ├── sim/action.rs 搜索、移动、休息、进食饮水、如厕、解剖、遭遇
    │   ├── sim/combat.rs 战斗回合、技能与道具
    │   ├── sim/map.rs    二维地图、人流与拥堵
    │   ├── player.rs     角色属性、需求、背包、八大系统
    │   ├── world.rs      世界状态、日志与过滤
    │   ├── archive.rs    跨循环档案
    │   ├── save.rs       世界存档读写
    │   ├── rng.rs        SplitMix64
    │   └── time.rs       时钟与阶段文案
    └── ui/               终端界面
        ├── mod.rs        App、标题流程、设置项
        ├── game.rs       主循环、模式切换、结算与循环衔接
        ├── combat.rs     战斗行动面板
        ├── panels.rs     状态、物资、地图、档案、日志、帮助
        ├── format.rs     进度条与世界标签
        └── title.rs      标题画面渲染
```

## 数据文件

所有数据都在 `data/` 下，使用 JSON5。可以用环境变量 `CHAOTIC_ASCENDING_DATA` 指向别的数据目录。

### `data/config.json5`

首次运行生成，也可以在游戏内设置界面修改。字段分四组：

| 分组 | 内容 |
| --- | --- |
| 顶层 | 版本、初始扭曲、每日增量、增量增速、各阶段阈值、各行动耗时、演算延迟、自动存档间隔、日志上限 |
| `player` | 初始生命与四项属性、初始物资数量 |
| `needs` | 饥饿/口渴/疲倦/尿意/便意的每小时累积、各种阈值、饥饿与缺水的伤害、每次受击的感染量 |
| `combat` | 基础攻击、力量系数、防御系数、逃跑概率、技能伤害倍率、判定「强敌」的生命与伤害门槛 |
| `auto` | 自动模式的治疗/逃跑/进食/饮水/如厕阈值、是否接种疫苗的阈值、施法概率 |

加载时会经过 `sanitize()`：阈值保证递增（二阶段 < 三阶段 < 四阶段），耗时和阈值被限制在合理范围。第四阶段没有终局阈值。

### `data/content/`

目录下的每个 `*.json5` 都是内容补丁，可以只定义任意区块（`creatures`、`items`、`sites`、`skills`、`intel`、`npcs`、`weaknesses`）。默认拆分为：

- `creatures.json5`：生物。字段包括 `human`（可作为人类宿主）、`brain`（具有类脑结构，可解剖并作为怪物宿主）、`habitats`（与地点 `tags` 匹配）、`friendly`、`weight`（遭遇权重）、`materials`（掉落材料）。
- `items.json5`：物品。`kind` 取 `food / water / medicine / herb / vaccine / weapon / armor / device / material / key`；`power` 是效果强度；法器的 `magic_bonus` 会加到法术伤害上。
- `sites.json5`：地点。`region` 决定默认厕所：城市/乡村有、无人区没有，可用 `toilet: true/false` 覆盖；`links` 必须双向；`loot` 按阶段过滤；`guarded` 地点需要通行；`story` 地点在搜索耗尽时触发剧情。
- `skills.json5`：技能。`kind` 取 `innate / soul / psychic / magic`；`castable` 为真时才可被玩家重现；`cost` 为消耗量。
- `items_expanded.json5`：生活物资补丁，追加食物、日用品、学校用品、工具与关键物。
- `intel.json5`：结构化情报。`category` 取 `identity / map / note / network / rumor / other`；`importance` 取 `low / normal / high / critical`；`target_site` 用于核实；`ambiguous`/`fake` 控制模糊与假情报。
- `npcs.json5`：NPC。`tags` 与地点标签匹配；`attributes` 是二级页面显示的属性；`intel` 引用情报 ID。
- `weaknesses.json5`：弱点报告。`source` 取 `network / research / combat / dissection`，分别由网络公告、研究设施、反复攻击和解剖解锁。

地点的补充字段：
- `restock`：`never / possible / guaranteed`，决定每日刷新与 `?`/`!` 标记。
- `relief`：是否发放救济粮（第 1–7 天，每天一次）。
- `occupied`：是否有人，取物时可能被目击。
- `vent` / `back_door` / `fire_escape`：可用的潜入路线。
- `intel` / `npcs`：搜查可发现的情报与可能遇到的人物。

加载器按文件名排序合并所有补丁，因此后加载的文件可以追加同一区块的条目。`Content::validate()` 会检查：ID 唯一、生物引用的技能与材料存在、地点链接双向、战利品物品存在、每个地点至少一次搜索、三个区域的起始地点都在。

`examples/split_content.rs` 演示如何把单文件内容拆成多份补丁。

### 缓存

`data/cache/content.bin` 缓存解析后的内容，键是所有内容文件（含文件名）的 FNV-1a 哈希。任何内容文件变化都会让缓存失效并重新解析。

## 存档

| 文件 | 内容 | 写入时机 |
| --- | --- | --- |
| `data/worlds/<名称>-<种子>.bin` | 世界状态、角色、日志、随机数状态、需求累积、循环统计 | 每 `autosave_turns` 个行动、退出、结算时 |
| `data/archive.bin` | 跨循环档案：条目与信源、循环次数、死亡次数、渗透进度、情报状态、纪事、人物、弱点报告、模式解锁、阶段记忆、解剖记录 | 自动存档、结算、循环切换时 |

存档使用 bincode。`SAVE_VERSION` 不兼容的旧存档会被拒绝加载并提示新建世界；新增字段使用 `#[serde(default)]` 以便在同一版本内兼容。

写入世界存档时先写 `.bin.tmp` 再原子重命名，避免中途退出损坏存档。

存档里的地点 ID 会在加载时校验；如果删除了内容文件里的地点，对应存档会提示检查 `data/content/`。

## 模拟模型

- **随机数**：世界生成使用 SplitMix64；同一 `(种子, 地点, 搜索序号, 阶段)` 决定同一次搜索的战利品，因此同一世界的结果可复现。生物生命与伤害生成时有 ±15% 浮动。
- **时间**：时钟以 0.1 秒为单位。战斗每回合 0.1 秒；日常行动按分钟推进并附加随机抖动。
- **扭曲**：每推进 `d` 天，扭曲增加 `inc*d + 0.5*acc*d²`，并把 `inc += acc*d`，即增量的增量线性增长。
- **需求**：按小时累积；跨越整小时时结算饥饿/缺水伤害、感染变化与长期需求提示。
- **二维地图**：聚合人流模型（地点人口 + 通勤流 + 拥堵），不是逐人代理；每小时重算一次，旅行耗时为 O(距离)。感染沿路线传播，动乱由拥堵与感染合成。
- **阶段判定**：`Config::phase(distortion)` 返回 1–4；默认第 2 天进入二阶段、约第 60 天进入三阶段、第 180 天进入四阶段。四阶段之后没有终局。
- **探索与潜入**：搜索次数换算成百分比；商店按日刷新并在档案中记住必然刷新。有人地点取物按日期、时段和潜入路线计算目击概率，追击失败三次转入拘留。
- **知识**：情报、NPC、纪事、弱点和信源都存放在 `Archive`，因此跨循环保留；假情报只在目标地点搜查后改变状态。
- **转生资格**：`Sim::available_hosts()` 同时检查循环次数、是否见过、人类/类脑标记、解剖记录和起始阶段。权限授予在 `transfer_to_host()` 中完成。
- **转生资格**：`Sim::available_hosts()` 同时检查循环次数、是否见过、人类/类脑标记、解剖记录和起始阶段。权限授予在 `transfer_to_host()` 中完成。

## 分层约定

- `model/` 不依赖终端，可以无终端测试（`src/model/tests.rs`）。
- `ui/` 只负责按键、菜单和渲染；所有规则判定（能否如厕、能否解剖、能否转生、能否重演阶段）都在模型层。
- 阶段文案集中在 `model/time.rs`；标题与主菜单在 `data/titles.json5`（接口 `title::TitleConfig`，也可用 `App::set_language` 按需切换）；帮助文本与按键提示集中在 `ui/game.rs` 与 `ui/panels.rs`。
- 日志带游戏时间、重要程度和类别，主界面与日志界面共用同一份过滤状态。

## 测试与检查

```sh
cd chaotic_ascending
cargo test
cargo clippy --all-targets
cargo fmt
```

测试覆盖内容加载与校验、阶段切换、第一循环意识锁、厕所地域规则、第五循环转生、解剖资格、第四阶段与阶段重演、存档往返、地图与战斗结算等。新增内容后，`Content::validate()` 会在启动时给出具体的报错位置。
