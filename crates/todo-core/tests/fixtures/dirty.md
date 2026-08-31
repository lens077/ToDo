---
todo-spec: 1
columns:
  - {id: todo, name: "待办"}
  - {id: doing, name: "进行中"}
  - {id: done, name: "已完成", type: done}
unknown-key:
  nested: 保留
---

# 项目 🧪

<!-- 人写的注释必须保留 -->

## 待办

- [ ] 全角：ＡＢＣ 与 emoji ✈️ <!--t:a1b2c3--> #核心 @me 📅️2026-09-10 future:value ^1000  
    - [x] 嵌套任务 <!--t:d4e5f6--> ✅2026-08-30 ⛔a1b2c3

`示例：- [ ] 行内代码不算任务`

| 列 | 值 |
|---|---|
| checkbox | `- [ ] fake` |

![图片](assets/看板.png)

```markdown
- [ ] 围栏内假任务 <!--t:fake00-->
```

~~~mermaid
flowchart LR
  A["- [ ] 假任务"] --> B
~~~

## 普通章节

说明不会改变。
- [ ] 仍归上游待办 <!--t:g7h8i9--> @alice custom:原样

## 进行中

+ [ ] v1 状态 <!--t:j0k1l2--> 🔗️abc1234..def5678 !1

## 已完成

- [x] 完成项 <!--t:m3n4o5--> ✅️2026-08-31
