# 「阅读」功能设计稿

状态：**已确认并实现**（2026-09-27）。裁定：阅读记录为个人级、跨项目共享；图片本地化默认开启；正文同时保存 Markdown（因此抽取库选 `readabilityrs` 而非 `dom_smoothie`）；「转为任务 / 转为随记」留到下一版。实现记录见 `../../adr/0003-reading-library.md`。

设计图：

- `01-reading-list.png` — 阅读列表（侧栏「阅读」置于「目标」之前）
- `02-reader-notes.png` — 阅读器（左正文 / 右标记与笔记）
- `03-interactions.png` — 三个交互细节：继上次继续阅读、圈选段落弹出标记菜单、添加链接的抓取与预览

> 注：`02` 图中生图模型把「阅读」画在了第三位并改了品牌名，以 `01` 图的导航顺序（阅读 → 目标 → 象限 → 随记 → AI → 扫描与历史）与 Virgo 品牌为准。

## 社区调研结论

| 需求 | 候选库 | 结论 |
| --- | --- | --- |
| 从任意 URL 抽取可读正文（标题、封面、正文 HTML、字数） | Rust: `dom_smoothie`（readability.js 移植，MIT）、`readabilityrs`（Mozilla 测试集 93.8%，Apache-2.0，可同时输出 Markdown）；JS: `@mozilla/readability`、`defuddle` | **采用 `readabilityrs`**（用户要求同时保存 Markdown），在 Rust 端抓取（复用已有 `reqwest`），避免前端跨域与 Referer 问题 |
| 微信公众号文章 | 无现成库专门处理 | Readability 能抓到 `#js_content`，但需自己补：`data-src → src`、图片本地化（`mmbiz.qpic.cn` 有 Referer 防盗链） |
| 圈选段落 + 高亮 + 笔记，重开可复活 | `@recogito/text-annotator` + `@recogito/react-text-annotator`（W3C Web Annotation 模型，活跃维护）；`web-highlighter`；`apache-annotator` | **采用 recogito**：锚点 = `quote + start/end 字符偏移`，正文不可变所以复活稳定 |
| 继上次继续阅读 | Readeck `read_progress + read_anchor`；Karakeep `readingProgressOffset/Anchor/Percent` | **照搬语义**：同时存百分比 + 锚点，锚点优先、百分比兜底；0/100 不存锚点；100% 即已读且不回退 |
| 整体读后即用的桌面库 | Omnivore（已停更、8 容器）、Readeck / Karakeep（服务端） | 无法整体搬入 Tauri；采用「三块积木 + 胶水」 |

## 产品语义（沿用社区共识）

- **进度**：正文滚动时节流计算 `percent`（视口底部相对正文高度）与 `anchor`（视口顶部第一个可见块级元素的稳定序号 `data-r-idx`）；在离开阅读器、窗口失焦、关闭时落盘，不是每次滚动都写。
- **已读**：`percent == 100` 即已读（sticky，之后回滚不再降），也可手动「标为已读 / 标为未读」。
- **继续阅读**：打开进度在 10–99% 的文章时显示横幅「上次读到 42% · 继续阅读」，点击滚到锚点；用户自己滚过 15% 则横幅自动消失（Karakeep 做法）。
- **标记**：选中正文 → 浮出「标记 / 标记并写笔记 / 复制」；标记以琥珀马克笔底显示，有笔记的标记在页边加琥珀点；右侧笔记栏列出全部标记，点击互相定位。
- **离线**：正文与图片在添加时即落盘，之后不再请求原站；「在浏览器打开」跳原链接。

## 拟定架构

```
apps/desktop/src-tauri (Rust)
  reading/fetch.rs      reqwest 抓取 → dom_smoothie 抽取 → 微信补丁 → 图片下载到 app data
  reading/store.rs      SQLite（复用 workspace 的 rusqlite）
  commands              reading_add(url) / reading_list() / reading_get(id)
                        reading_progress(id, percent, anchor) / reading_mark_read(id, read)
                        reading_annotation_upsert / _delete / reading_remove(id)

crates/todo-core（可选）：若希望 CLI / `todo serve` 也能用，把 store 与 fetch 放这里，desktop 只做 command 包装

apps/desktop/src
  components/ReadingView.tsx      列表 + 添加行 + 过滤
  components/ReaderView.tsx       正文（sanitize 后的 HTML）+ recogito 注解层 + 进度追踪
  components/ReaderNotes.tsx      右侧标记与笔记栏
  lib/reading.ts                  transport 三态（tauri / http / mock）与类型
```

SQLite 表：

```sql
articles(id, url, canonical_url, title, site_name, excerpt, cover_path, content_html,
         word_count, reading_minutes, fetched_at, added_at,
         progress_percent INTEGER DEFAULT 0, progress_anchor TEXT, read_at, last_opened_at)
article_images(article_id, original_url, local_path)
annotations(id, article_id, quote, start_offset, end_offset, note, color, created_at, updated_at)
```

存储位置：与 capture / ai store 同级的 app data 目录（不写进用户项目目录——阅读记录是个人的，不是项目任务源）。

## 用户裁定（已定）

1. 阅读记录**跨项目共享**（个人级）。
2. 正文**同时保存 Markdown**（`articles.markdown` 列，暂未在界面暴露）。
3. 图片本地化**默认开启**。
4. 「转为任务 / 转为随记」**本版不做**。
