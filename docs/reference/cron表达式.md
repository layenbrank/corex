# Cron 表达式参考

本文说明 **corex v5** 中 cron 触发器与 `cron.schedule` 所使用的表达式规则。

权威解析器文档：

- 依赖声明：[tokio-cron-scheduler 0.15.1 → Dependencies](https://crates.io/crates/tokio-cron-scheduler/0.15.1/dependencies)（`croner ^3.0.0`）
- crates.io：[croner](https://crates.io/crates/croner)（3.x 线）
- docs.rs：[croner 3.0.1](https://docs.rs/croner/3.0.1/croner/) · [parser](https://docs.rs/croner/3.0.1/croner/parser/index.html)

> **归档说明：** 旧稿 [`docs/archive/cron.md`](../archive/cron.md) 描述的是 zslayton/`cron`（7 字段、日/周默认为「与」）。现行栈为 `tokio-cron-scheduler` → **croner 3**，以下文本为准。

> **版本说明：** [`tokio-cron-scheduler` 0.15.1](https://crates.io/crates/tokio-cron-scheduler/0.15.1/dependencies) 声明依赖 **`croner ^3.0.0`**；本仓库 `Cargo.lock` 当前解析为 **3.0.1**。本文 Pattern / `CronParser` 规则对齐 **croner 3.0.1**，不以 4.x 为准。

---

## 1. 技术栈与适用范围

| 层级 | 组件 | 版本 | 职责 |
|------|------|------|------|
| 调度运行时 | [`tokio-cron-scheduler`](https://crates.io/crates/tokio-cron-scheduler) | **0.15.1**（workspace） | `JobScheduler` / `Job::new_async`：到点执行回调 |
| 表达式解析 | [`croner`](https://crates.io/crates/croner) | **`^3.0.0`**（传递依赖，lock 多为 **3.0.1**） | 解析 pattern、求前后触发点、描述文案 |
| corex 预处理 | [`parse_cron_expr`](../../crates/engine/src/cron/expr.rs) | — | 校验字段数；5 字段补秒为 6 字段 |

**适用入口：**

- Directive `triggers[].type: cron` 的 `expr`
- Action `cron.schedule` 的 `expr`
- 经 supervisor 解析占位符后的最终字符串（见 [指令 YAML · Triggers](./指令YAML.md#triggers)）

**不适用：**

- Linux `crontab` 系统守护（语义相近、实现不同）
- 归档文档中的 zslayton/`cron` 规则
- `tokio-cron-scheduler` 的 `english` 自然语言调度（corex **未启用**该 feature）

---

## 2. Pattern 字段结构（croner 3.0.1）

依据 [docs.rs Pattern](https://docs.rs/croner/3.0.1/croner/#pattern)：表达式接近 Vixie Cron，并扩展 `?` / `L` / `W` / `#` 等。

```text
┌──────────────── (optional) second (0 - 59)
│ ┌────────────── minute (0 - 59)
│ │ ┌──────────── hour (0 - 23)
│ │ │ ┌────────── day of month (1 - 31)
│ │ │ │ ┌──────── month (1 - 12, JAN-DEC)
│ │ │ │ │ ┌────── day of week (0 - 6, SUN–SAT)
│ │ │ │ │ │       (0–6 = Sun–Sat；7 = Sunday，同 0)
│ │ │ │ │ │
* * * * * *
```

可选第 7 字段 **年** 由 `CronParser::year(...)` 控制（见 §3）；**corex 预处理拒绝 7 字段**，YAML 中不要写年。

### 2.1 字段允许值与特殊字符（官方表）

| 字段 | Required | Allowed values | Allowed special characters | Remarks |
|------|----------|----------------|----------------------------|---------|
| Seconds | Optional | `0`–`59` | `*` `,` `-` `/` `?` | 秒可选；corex 归一化后始终带秒 |
| Minutes | Yes | `0`–`59` | `*` `,` `-` `/` `?` | |
| Hours | Yes | `0`–`23` | `*` `,` `-` `/` `?` | |
| Day of Month | Yes | `1`–`31` | `*` `,` `-` `/` `?` `L` `W` | `L`=月末；`W`=最近工作日 |
| Month | Yes | `1`–`12` 或 `JAN`–`DEC` | `*` `,` `-` `/` `?` | 名称大小写不敏感 |
| Day of Week | Yes | `0`–`7` 或 `SUN`–`SAT` | `*` `,` `-` `/` `?` `#` `L`（及 `+`，见 §4.6） | `0`/`7`=周日；`#`=第 N 个星期几 |

**星期编号（默认 POSIX，非 Quartz）：**

| 值 | 星期 |
|----|------|
| `0` 或 `7` | 周日 |
| `1` | 周一 |
| `2` | 周二 |
| `3` | 周三 |
| `4` | 周四 |
| `5` | 周五 |
| `6` | 周六 |

### 2.2 corex 字段数规则

`parse_cron_expr` 在交给 `Job::new_async` 之前：

| 输入字段数 | 行为 | 示例 |
|-----------|------|------|
| **5** | 前缀补 `0`（秒 = 0） | `0 9 * * 1-5` → `0 0 9 * * 1-5` |
| **6** | 原样 | `0 0 9 * * 1-5` |
| 其他（1、7…） | 报错 | `@daily`、带年份的 7 字段均失败 |

因此 YAML 可写 5 或 6 字段；最终进入调度器的始终是 **6 字段**。

官方示例（5 字段、无秒）见 [croner 3.0.1 Example](https://docs.rs/croner/3.0.1/croner/#example)：

```rust
let cron = Cron::from_str("0 0 * * FRI").expect("Successful parsing");
let next = cron.find_next_occurrence(&Utc::now(), false).unwrap();
```

在 corex 中等价写法：`expr: "0 0 * * FRI"`（5 字段）或 `expr: "0 0 0 * * FRI"`（6 字段）。

---

## 3. CronParser 配置（官方 builder）

依据 [croner::parser](https://docs.rs/croner/3.0.1/croner/parser/index.html) 与 [`CronParserBuilder`](https://docs.rs/croner/3.0.1/croner/parser/struct.CronParserBuilder.html)：

```text
Cron::from_str("pattern")
  ≡ CronParser::new().parse("pattern")
```

自定义：

```rust
use croner::parser::{CronParser, Seconds, Year};

let parser = CronParser::builder()
    .seconds(Seconds::Optional)   // 或 Required / Disallowed
    .year(Year::Disallowed)       // 或 Optional / Required
    .dom_and_dow(false)           // true = 全局强制 DOM∧DOW
    .alternative_weekdays(false)  // true = Quartz 星期编号
    .build();
```

### 3.1 `seconds` — [`Seconds`](https://docs.rs/croner/3.0.1/croner/parser/enum.Seconds.html)

| 变体 | 含义 |
|------|------|
| `Optional` | 可写 5 字段（无秒）或 6 字段（有秒） |
| `Required` | **必须**含秒（典型 6 字段） |
| `Disallowed` | **禁止**秒字段 |

docs.rs 示例（Optional）：

```rust
let parser = CronParser::builder().seconds(Seconds::Optional).build();
let _ = parser.parse("*/10 * * * * *"); // 每 10 秒
let _ = parser.parse("* * * * *");      // 每分钟
```

**corex：** 不直接调 builder；用 `parse_cron_expr` 把 5 字段变成 6 字段后再交给调度器，效果接近「YAML 侧秒可选、注册侧秒必填」。

### 3.2 `year` — [`Year`](https://docs.rs/croner/3.0.1/croner/parser/enum.Year.html)

| 变体 | 含义 |
|------|------|
| `Optional` | 可追加第 7 字段（年，常用 `1`–`9999`） |
| `Required` | 必须含年 |
| `Disallowed` | 禁止年字段 |

求值侧有年份搜索上下限：[`YEAR_LOWER_LIMIT`](https://docs.rs/croner/3.0.1/croner/constant.YEAR_LOWER_LIMIT.html)（公元 1）、[`YEAR_UPPER_LIMIT`](https://docs.rs/croner/3.0.1/croner/constant.YEAR_UPPER_LIMIT.html)（防止永不匹配时死循环）。

**corex：** 预处理只接受 5/6 字段 → **等价于禁止年字段**。不要写 `0 0 0 1 1 * 2026`。

### 3.3 `dom_and_dow`

`dom_and_dow(true)`：全局把 DOM 与 DOW 合成 **AND**（某些库默认如此）。

官方建议：更推荐在表达式 DOW 上用 **`+` 前缀**做单条 AND（见 §4.6 / §5），而不是全局打开该开关。

**corex / 调度器默认：** **OR**（POSIX）。需要 AND 时写 `+MON` 等形式。

### 3.4 `alternative_weekdays`（Quartz 模式）

`true` 时星期改为 Quartz：`1`=周日 … `7`=周六（不再是 `0`/`7`=周日）。

**corex：** 视为 **关闭**。请按 POSIX 写 `0`–`7` 或 `SUN`–`SAT`。

### 3.5 与 corex 对照一览

| Builder 选项（croner 3.0.1） | croner 能力 | corex 有效行为 |
|------------------------------|-------------|----------------|
| `seconds` | Optional / Required / Disallowed | YAML 5 或 6；注册前归一化为 6 |
| `year` | Optional / Required / Disallowed | **禁止**（非 5/6 即错） |
| `dom_and_dow` | 全局 AND | 默认 OR；表达式用 `+` |
| `alternative_weekdays` | Quartz 编号 | POSIX |
| crate feature `serde` | `Cron` 可序列化 | 与 YAML `expr` 字符串无关 |

> **说明：** croner **4.x** 另有 `sloppy_ranges` 等选项；**3.0.1** 的 [`CronParserBuilder`](https://docs.rs/croner/3.0.1/croner/parser/struct.CronParserBuilder.html) **无**该方法。请写标准 `*/N` / `X-Y/N`，勿依赖 `0/10`、`/10`。

---

## 4. 特殊字符（按类别）

语义综合 [crates.io/croner](https://crates.io/crates/croner) README 与 [docs.rs Pattern](https://docs.rs/croner/3.0.1/croner/#pattern)。下列示例为 **注册后 6 字段**（或注明 5 字段 YAML 写法）。

### 4.1 `*` `,` `-` `/`

| 字符 | 作用 | 示例 | 含义 |
|------|------|------|------|
| `*` | 该字段全部取值 | `0 * * * * *` | 每分钟第 0 秒 |
| `,` | 枚举 | `0 30 9,12,15 * * *` | 每天 9:30 / 12:30 / 15:30 |
| `-` | 闭区间 | `0 0 9-17 * * *` | 每天 9–17 点整点 |
| `/` | 步长 | `*/10 * * * * *` | 每 10 秒 |

推荐：`*/N` 或 `X-Y/N`。不要写非标准的 `0/10`、`/10`（croner 3.0.1 无 `sloppy_ranges`）。

跨边界范围（`NOV-FEB`、`FRI-MON`）易踩坑，用 `,` 拆开。

### 4.2 `?` — 与 `*` 等价

croner 3.0.1 在 **秒 / 分 / 时 / 日 / 月 / 周** 均允许 `?`（官方表）；语义与 `*` 相同，便于粘贴 Quartz / 遗留表达式。

| 示例 | 含义 |
|------|------|
| `0 0 12 ? * MON` | ≡ `0 0 12 * * MON` |
| `0 0 12 15 * ?` | ≡ `0 0 12 15 * *` |

`?` **不会**「关闭」DOM 或 DOW；默认已是 OR，无需用 `?` 互斥。

### 4.3 `L` — 最后

| 位置 | 示例 | 含义 |
|------|------|------|
| DOM | `0 0 0 L * *` | 每月最后一天 00:00 |
| DOW + `#` | `0 0 0 * * 5#L` / `FRI#L` | 每月最后一个周五 |
| DOW 范围 | `0 0 0 * * 5-6#L` | 每月最后一个周五与周六 |

`L` / `W` 大小写不敏感（croner 行为）。

### 4.4 `W` — 最近工作日（仅 DOM）

离指定日期最近的周一至周五；**搜索不跨月**。

| 情况 | 结果 |
|------|------|
| `15W` 且 15 日周六 | 14 日（周五） |
| `15W` 且 15 日周日 | 16 日（周一） |
| `1W` 且 1 日周六 | **当月** 3 日（周一），不落到上月周五 |

### 4.5 `#` — 第 N 个星期几（仅 DOW）

| 示例 | 含义 |
|------|------|
| `5#2` / `FRI#2` | 每月第二个周五 |
| `MON#1` | 每月第一个周一 |
| `MON-FRI#2` | 第二周的周一至周五（范围 + `#`） |
| `5#L` | 每月最后一个周五 |

`#` 后的序数一般为 `1`–`5`，或 `L`。

### 4.6 `+` — DOM ∧ DOW（表达式级）

默认 DOM 与 DOW 为 **或**。在 DOW 前加 `+` 改为 **与**（[crates.io](https://crates.io/crates/croner) README；docs.rs 字段表可能未单列该字符，以 README / `dom_and_dow` 文档为准）。

| 示例 | 语义 |
|------|------|
| `0 0 0 1 * MON` | 每月 1 日 **或** 每周一 |
| `0 0 0 1 * +MON` | **仅当** 1 日且是周一 |

---

## 5. 日字段与周字段逻辑

```text
默认（POSIX / Vixie / croner）：
  DOM 与 DOW 均「有约束」时 → OR

表达式 + 前缀，或 parser.dom_and_dow(true)：
  → AND
```

| 写法 | 触发条件 |
|------|----------|
| `0 0 12 15 * *` | 每月 15 日 12:00 |
| `0 0 12 * * FRI` | 每周五 12:00 |
| `0 0 12 15 * FRI` | 15 日 **或** 周五 |
| `0 0 12 15 * +FRI` | **仅** 15 日且周五 |

相对归档文档（zslayton/`cron` 默认 AND）这是最大语义差异。

---

## 6. 别名（croner 支持，corex 不可用）

| 别名 | 等价（5 字段） |
|------|----------------|
| `@yearly` / `@annually` | `0 0 1 1 *` |
| `@monthly` | `0 0 1 * *` |
| `@weekly` | `0 0 * * 0` |
| `@daily` | `0 0 * * *` |
| `@hourly` | `0 * * * *` |

corex `parse_cron_expr` 按空白拆字段且要求 5/6 个 → **请写展开式**，例如 `"0 0 * * *"`。

---

## 7. 求值 API 概要（croner，非 corex 直接调用）

调度器内部使用 croner 计算下次触发。了解 [Cron](https://docs.rs/croner/3.0.1/croner/struct.Cron.html) 有助于排查「为何没触发」：

| API | 作用 |
|-----|------|
| `is_time_matching(&time)` | 某时刻是否命中 pattern |
| `find_next_occurrence(&start, inclusive)` | 下一个触发点；`inclusive=false` 不含当前秒 |
| `find_previous_occurrence(...)` | 上一个触发点 |
| `iter_from` / `iter_after` / `iter_before` | 双向迭代 |
| `describe()` / `describe_lang` | 英文（或其它 Language）可读描述 |
| `determine_job_type()` | [`JobType::FixedTime`](https://docs.rs/croner/3.0.1/croner/enum.JobType.html) vs `IntervalWildcard`（影响 DST 规则） |

`find_next_occurrence` 在无法在合理年限内找到匹配时返回错误（与 `YEAR_*_LIMIT` / `TimeSearchLimitExceeded` 相关），避免死循环。

---

## 8. 时区（可配置）

corex 通过 `Job::new_async_tz` 解释表达式，**不要**为本地时间去改写 hour 字段。

### 8.1 解析优先级

1. Directive `triggers.cron.timezone`（或 `cron.schedule` 的 `timezone` 参数）
2. `corex.toml` → `[runtime] cron_timezone`
3. 默认：`local`（系统本地时区）

### 8.2 合法取值（仅 chrono）

| 值 | 含义 |
|----|------|
| `local` / `system` | 主机本地时区（含 DST，若系统支持） |
| `utc` / `z` | UTC |
| `+08:00` / `+0800` / `-05:00` | 固定偏移（无 IANA / 无地区 DST 规则） |

不支持 `Asia/Shanghai` 等 IANA 名（未引入 `chrono-tz`）。中国大陆固定用 `local` 或 `+08:00` 即可。

### 8.3 示例

```yaml
# 工作日本地 09:00（默认 timezone=local，可省略）
triggers:
  - type: cron
    expr: "0 9 * * 1-5"

# 显式 UTC
triggers:
  - type: cron
    expr: "0 9 * * 1-5"
    timezone: utc

# 固定东八区
triggers:
  - type: cron
    expr: "0 9 * * 1-5"
    timezone: "+08:00"
```

```toml
# config/corex.toml
[runtime]
cron_timezone = "local"
```

`JobType`（固定时刻 vs 间隔/通配）仍由 croner 用于 DST 边界处理；跨夏令时地区优先用 `local`，慎用固定偏移。

---

## 9. 与其他实现对照

| 主题 | Linux crontab | zslayton/`cron`（归档） | Quartz 常见 | **corex（croner）** |
|------|---------------|-------------------------|-------------|---------------------|
| 字段数 | 5 | 6–7 | 6–7 | **5 或 6**（→6） |
| 年字段 | 无 | 有 | 可选 | **无**（预处理） |
| DOM ↔ DOW | 通常 OR | 默认 AND | 常用 `?` 互斥 | 默认 **OR**；`+` → AND |
| `?` | 无 | 无 | 常用 | ≡ `*`（多数字段可写） |
| `L` / `W` / `#` | 通常无 | 部分 | 有 | **支持** |
| 星期数字 | 0–7 | 实现相关 | 常 1–7 | **0–7 POSIX** |

迁移检查清单：去掉年份；AND 场景加 `+`；勿用 Quartz 星期数字；勿用 `@daily` / 英文短语。

---

## 10. 常见错误

| 错误 | 原因 | 正确做法 |
|------|------|----------|
| 7 字段含年 | corex 只接受 5/6 | 去掉年字段 |
| `@daily` | 预处理不认别名 | `0 0 * * *` |
| `0 0 0 15 * FRI` 当成「且」 | 默认 OR | `0 0 0 15 * +FRI` |
| `?` 当作「忽略」 | `?` ≡ `*` | 用 `*` 或 `+` |
| 本地 9 点写 `0 0 9 * * *` 却配 `timezone: utc` | 时区选错 | 用默认 `local`，或显式 `timezone: local` |
| `every 15 seconds` | 未开 `english` | `*/15 * * * * *` |
| `0/10 * * * *` | 非标准步长（3.x 无宽松模式） | `*/10 * * * *` |
| Quartz `6`=周五 | 编号不同 | POSIX：周五=`5` 或 `FRI` |

---

## 11. 实用示例（YAML `expr`）

| 场景 | `expr` |
|------|--------|
| 每分钟 | `* * * * *` 或 `0 * * * * *` |
| 每 5 分钟 | `*/5 * * * *` |
| 每小时整点 | `0 * * * *` |
| 每天 09:00（本地） | `0 9 * * *` |
| 工作日 09:00（本地） | `0 9 * * 1-5` |
| 每周一 09:00（本地） | `0 9 * * MON` |
| 每月 1 日 | `0 0 1 * *` |
| 每月最后一天 | `0 0 L * *` |
| 每月第二个周一 | `0 0 * * MON#2` |
| 每月最后一个周五 | `0 0 * * FRI#L` |
| 离 15 日最近工作日 | `0 0 15W * *` |
| 仅 1 日且周一 | `0 0 1 * +MON` |
| 每 30 秒 | `*/30 * * * * *` |

```yaml
triggers:
  - type: cron
    expr: "0 9 * * 1-5"   # 工作日 09:00 UTC；5 字段，启动时补秒
```

```yaml
- id: reg
  action: cron.schedule
  params:
    expr: "0 0 12 * * *"
    directive: hello
```

---

## 12. 相关文档与源码

| 资源 | 说明 |
|------|------|
| [指令 YAML · Triggers](./指令YAML.md#triggers) | `triggers.cron` 与 CLI |
| [架构 · Supervisor](./架构.md#supervisor-子系统cron--watch) | `CronEngine` |
| [内置 Action · cron.schedule](./内置Action.md) | 命令式注册 |
| [`expr.rs`](../../crates/engine/src/cron/expr.rs) / [`engine.rs`](../../crates/engine/src/cron/engine.rs) | 归一化与调度 |
| [croner @ crates.io](https://crates.io/crates/croner) | 包说明、特殊字符、别名、Configuration |
| [croner 3.0.1 @ docs.rs](https://docs.rs/croner/3.0.1/croner/) | Pattern 表、`Cron` API |
| [CronParserBuilder](https://docs.rs/croner/3.0.1/croner/parser/struct.CronParserBuilder.html) | `seconds` / `year` / `dom_and_dow` / `alternative_weekdays` |
| [tokio-cron-scheduler 0.15.1 dependencies](https://crates.io/crates/tokio-cron-scheduler/0.15.1/dependencies) | 声明 `croner ^3.0.0` |
| [tokio-cron-scheduler](https://docs.rs/tokio-cron-scheduler/) | 调度器、UTC、可选持久化 |
| [archive/cron.md](../archive/cron.md) | 旧 `cron` crate（已废弃） |
