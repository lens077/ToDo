# Virgo 官网（海外站，Cloudflare Pages）

`https://virgo-overseas.pages.dev`。纯静态、零依赖，只需要 Node 18+。

| 路径 | 作用 |
| --- | --- |
| `build.mjs` | 渲染 `dist/`：`/en/`、`/zh/` 两份完整页面，外加根路径的语言路由页、`sitemap.xml`、`robots.txt` |
| `src/content.en.mjs` / `src/content.zh.mjs` | 两种语言的全部文案。两份结构必须一致，构建会校验，缺一段直接失败 |
| `src/page.mjs` | 六屏骨架（首屏 / 怎么用 / 边界 / 视图 / 上线通知 / 页脚），见 `work/docs/brand/LANDING.md` |
| `src/art.mjs` | 首屏的产品物件：一张 `TODO.md` 和它的对账结果，用文字渲染，随语言切换 |
| `src/site.css` | 品牌「小桌面」样式。只有顶部 Product tokens 一段是 Virgo 专有的 |
| `src/site.mjs` | 站点常量：Pages 项目、Umami 站点 ID、备案信息 |
| `public/` | 原样拷进 `dist/`：品牌图标（`work/tools/brand-icon/out/virgo/`）、`_headers` |

文案出处是 `docs/site-copy.md`；英文版按 `CONTEXT.md` 的术语翻译，事实与中文逐条对应。

## 语言

- `/` 是路由页：用户点过顶部的语言切换就按那次选择（`localStorage` 的 `sumery.lang`），否则取 `navigator.languages` 里第一个 `zh` 或 `en`，都没有就用英文。
- `/en/`、`/zh/` 是完整渲染好的静态页，互相用 `hreflang` 标注；直接打开哪个就是哪个，不会被自动跳走。
- 没有 JavaScript 时根路径显示两个语言链接。

## 构建与发布

```bash
cd web
npm run build      # → dist/
npm run preview    # 本机 http://localhost:5191
npm run deploy     # 构建并发布到 Pages 项目 virgo-overseas（main 分支即生产）
```

`deploy` 需要环境变量 `CLOUDFLARE_API_TOKEN`（Pages 编辑权限）与 `CLOUDFLARE_ACCOUNT_ID`，不要写进仓库。

## 统计

页面加载自托管 Umami（站点「Virgo 海外官网」），只统计 `virgo-overseas.pages.dev`，不带查询串。按钮事件用 `data-umami-event`：`notify`（`from` = status / hero / get）、`lang_switch`（`to`）。页脚写明了这一点。
