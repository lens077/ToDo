//! The instruction text an agent runs to align tasks with code reality, and the
//! validator for what it sends back.
//!
//! This is the third scan layer. The other two are deterministic; this one is a
//! judgment call, so the design problem is not "can the model do it" but "how do
//! we stop a confident wrong answer from reaching the file".
//!
//! Three constraints shape the prompt, each from a failure other tools hit:
//!
//! - **A false positive is fatal.** Users disable a checker that cries wolf, and
//!   they do not turn it back on. So `unknown` is a first-class verdict, the
//!   model is told explicitly that it is the correct answer when unsure, and any
//!   `done` verdict without a concrete `path:line` citation is rejected here
//!   rather than shown to the user.
//! - **Coverage must be provable.** A model asked to "look for finished work"
//!   silently skips things. It is instead given a fixed list and required to
//!   return a verdict for every id it was given, which the validator checks.
//! - **The model does not get to decide.** Every verdict is a proposal. Nothing
//!   auto-applies; completion stays a human decision (spec §6.5.4).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{Document, Error, Result};

/// Instructions for the repository alignment pass.
///
/// Written as an operating procedure rather than a request: ordered steps, an
/// explicit stop rule, and one output shape. Free-form prompts produce free-form
/// output, and output that cannot be validated cannot be trusted.
pub const ALIGNMENT_PROMPT: &str = r#"# 任务：核对 TODO 与代码现实

你要判断每个任务的记录状态是否与代码库的实际状态相符，然后输出结构化结果。

## 铁律

1. **不确定就输出 `unknown`。** 这不是失败，是正确答案。误报会让用户永久关闭这个功能。
2. **任何 `done` 判定必须附带 `path:line` 引用**，指向你实际读到的实现代码。没有引用的 `done` 会被程序拒绝。
3. **你只提议，不改文件。** 不要编辑任何 `.md` 或源码文件，不要运行 `todo done`。
4. **不要因为「看起来应该做完了」而判定完成。** 判据是代码里存在对应实现，不是任务描述合理。

## 步骤

### 第 1 步：取得任务清单

```bash
todo list --all --json
```

按返回顺序处理，**每个任务都要有一条结论**，一个都不能漏。任务多于 60 条时，只处理前 60 条并在 `truncated` 里说明。

### 第 2 步：逐个任务收集证据

对每个任务，按下面的顺序找，找到即停：

1. **物证优先。** 任务带 `🔗<from>..<to>` 时：
   ```bash
   git diff --name-only <from>..<to>
   git log --oneline <from>..<to>
   ```
   物证是最强依据——它指明了实现在哪次提交。若 commit 不存在或已被 revert，判 `evidence_broken`。
2. **按正文检索。** 从任务正文提取名词与标识符（函数名、文件名、配置键），用 `rg` 精确搜索。同时搜中英文两种写法。
3. **按出处检索。** 任务带 `↳ 来源 path:line` 说明行时，先读那个位置，确认原始的 TODO 注释是否还在。注释已消失通常意味着已处理。
4. **读实现本身。** 只有真正读到实现代码，才可以判 `done`。

### 第 3 步：给出判定

每个任务从下面选一个，不要自创：

| verdict | 何时使用 |
|---|---|
| `done` | 读到了实现代码，且功能确实存在。**必须给 `path:line`** |
| `partial` | 部分实现。必须说明「已完成什么、还差什么」，可给 `percent`(1-99) |
| `not_started` | 明确找不到任何相关实现痕迹 |
| `evidence_broken` | 物证 commit 不存在，或已被 revert |
| `stale` | 任务描述的东西已经不适用了（模块被删、需求被替代） |
| `unknown` | 无法判断。**不确定时选这个** |

### 第 4 步：输出

只输出一个 JSON 代码块，不要有其他内容：

```json
{
  "schema": "todo.alignment.v1",
  "truncated": false,
  "verdicts": [
    {
      "task_id": "a1b2c3",
      "verdict": "done",
      "confidence": "high",
      "reason": "限流中间件已实现并接入路由",
      "citations": ["src/middleware/rate_limit.rs:12", "src/router.rs:88"],
      "percent": null
    }
  ]
}
```

字段说明：
- `task_id`：必须来自第 1 步的清单，不要编造。
- `confidence`：`high` / `medium` / `low`。`low` 会被当作需要人工复核。
- `citations`：`path:line` 数组。`done` 与 `partial` 必填，其余可为空数组。
- `percent`：仅 `partial` 时给，整数 1-99。

## 自检

提交前逐条确认：
- [ ] 每个输入任务都有一条 verdict？
- [ ] 每个 `done` 都有至少一个 `citations`？
- [ ] 每个 `citations` 都是我真的打开看过的位置？
- [ ] 拿不准的都写了 `unknown` 而不是猜一个？
- [ ] 我没有改动任何文件？
"#;

/// One task's verdict, as returned by the agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AlignmentVerdict {
    pub task_id: String,
    pub verdict: Verdict,
    #[serde(default)]
    pub confidence: Confidence,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub citations: Vec<String>,
    #[serde(default)]
    pub percent: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Done,
    Partial,
    NotStarted,
    EvidenceBroken,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    #[default]
    Low,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlignmentResponse {
    #[serde(default)]
    pub truncated: bool,
    pub verdicts: Vec<AlignmentVerdict>,
}

/// What survived validation, and what did not.
#[derive(Debug, Clone, Serialize)]
pub struct AlignmentReview {
    /// Verdicts that are well-formed and reference a real task.
    pub accepted: Vec<AlignmentVerdict>,
    /// Verdicts rejected by a rule, each with the reason.
    pub rejected: Vec<RejectedVerdict>,
    /// Task ids that were given to the agent but got no verdict back.
    pub missing: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RejectedVerdict {
    pub task_id: String,
    pub reason: String,
}

impl AlignmentReview {
    /// Verdicts a human should look at, strongest claim first.
    pub fn actionable(&self) -> Vec<&AlignmentVerdict> {
        let mut items: Vec<_> = self
            .accepted
            .iter()
            .filter(|verdict| !matches!(verdict.verdict, Verdict::Unknown | Verdict::NotStarted))
            .collect();
        items.sort_by_key(|verdict| match verdict.verdict {
            Verdict::EvidenceBroken => 0,
            Verdict::Done => 1,
            Verdict::Partial => 2,
            Verdict::Stale => 3,
            _ => 4,
        });
        items
    }
}

/// Parses and validates an agent's response against the tasks it was given.
///
/// Enforces the rules the prompt states, because a prompt is a request and this is
/// the only place they are guaranteed. Anything rejected here never reaches the
/// user's confirmation screen.
pub fn review_alignment(document: &Document, raw: &str) -> Result<AlignmentReview> {
    let json = extract_json(raw)?;
    let response: AlignmentResponse = serde_json::from_str(&json)
        .map_err(|error| Error::InvalidAlignment(format!("response is not valid JSON: {error}")))?;

    let known: HashSet<&str> = document
        .tasks
        .iter()
        .filter_map(|task| task.id.as_deref())
        .collect();

    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    let mut seen: HashMap<String, ()> = HashMap::new();

    for verdict in response.verdicts {
        if !known.contains(verdict.task_id.as_str()) {
            // A hallucinated id would otherwise attach a verdict to nothing, or worse
            // to a task created later that happens to reuse the string.
            rejected.push(RejectedVerdict {
                task_id: verdict.task_id,
                reason: "unknown task id; not present in the document".into(),
            });
            continue;
        }
        if seen.insert(verdict.task_id.clone(), ()).is_some() {
            rejected.push(RejectedVerdict {
                task_id: verdict.task_id,
                reason: "duplicate verdict for the same task".into(),
            });
            continue;
        }
        if matches!(verdict.verdict, Verdict::Done | Verdict::Partial)
            && verdict.citations.is_empty()
        {
            rejected.push(RejectedVerdict {
                task_id: verdict.task_id,
                reason: "a done or partial verdict requires at least one path:line citation".into(),
            });
            continue;
        }
        if let Some(bad) = verdict
            .citations
            .iter()
            .find(|citation| !is_citation(citation))
        {
            rejected.push(RejectedVerdict {
                task_id: verdict.task_id.clone(),
                reason: format!("citation {bad:?} is not in path:line form"),
            });
            continue;
        }
        if verdict.verdict == Verdict::Partial
            && verdict
                .percent
                .is_some_and(|value| !(1..=99).contains(&value))
        {
            rejected.push(RejectedVerdict {
                task_id: verdict.task_id,
                reason: "percent must be between 1 and 99".into(),
            });
            continue;
        }
        accepted.push(verdict);
    }

    let missing = known
        .iter()
        .filter(|id| !seen.contains_key(**id))
        .map(|id| (*id).to_string())
        .collect::<Vec<_>>();

    Ok(AlignmentReview {
        accepted,
        rejected,
        missing: sorted(missing),
        truncated: response.truncated,
    })
}

fn sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

/// Pulls the JSON object out of a reply that may be wrapped in prose or a fence.
fn extract_json(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('{') {
        return Ok(trimmed.to_string());
    }
    if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        let body = after
            .split_once('\n')
            .map(|(_, rest)| rest)
            .unwrap_or(after);
        if let Some(end) = body.find("```") {
            return Ok(body[..end].trim().to_string());
        }
    }
    // Last resort: the outermost brace pair.
    match (trimmed.find('{'), trimmed.rfind('}')) {
        (Some(start), Some(end)) if end > start => Ok(trimmed[start..=end].to_string()),
        _ => Err(Error::InvalidAlignment(
            "no JSON object found in the response".into(),
        )),
    }
}

/// `path:line`, where line is a positive integer.
fn is_citation(value: &str) -> bool {
    let Some((path, line)) = value.rsplit_once(':') else {
        return false;
    };
    !path.trim().is_empty() && line.parse::<u32>().is_ok_and(|line| line > 0)
}
