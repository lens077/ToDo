# Virgo 官网（海外站，Cloudflare Pages）

`https://virgo-overseas.pages.dev`。纯静态、零依赖，只需要 Node 18+。

| 路径 | 作用 |
| --- | --- |
| `build.mjs` | 渲染 `dist/`：`/en/`、`/zh/` 两份完整页面，外加根路径的语言路由页、`sitemap.xml`、`robots.txt` |
| `src/content.en.mjs` / `src/content.zh.mjs` | 两种语言的全部文案。两份结构必须一致，构建会校验，缺一段直接失败 |
| `src/page.mjs` | 六屏骨架（首屏 / 怎么用 / 边界 / 视图 / 免费与赞助 / 上线通知 / 页脚），见 `work/docs/brand/LANDING.md` |
| `src/art.mjs` | 首屏的产品物件：一张 `TODO.md` 和它的对账结果，用文字渲染，随语言切换 |
| `src/site.css` | 品牌「小桌面」样式。只有顶部 Product tokens 一段是 Virgo 专有的 |
| `src/site.mjs` | 站点常量：Pages 项目、Umami 站点 ID、备案信息 |
| `public/` | 原样拷进 `dist/`：品牌图标（`work/tools/brand-icon/out/virgo/`）、界面截图（`screens/`，桌面端 mock 数据；桌面端界面目前只有中文，英文页图注已注明）、`_headers` |
| `deploy/k8s/` | `virgo.apikv.com` 的镜像与清单：nginx 静态站，经 `default/cilium-gateway` 的 https 监听 |

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
```

在仓库根目录用 Makefile 发三端（同一份 `dist/`）：

```bash
make site-deploy          # 三端一起发
make pages-deploy         # Cloudflare Pages → virgo-overseas.pages.dev
make pages-deploy-eo      # EdgeOne Pages → virgo-site（域名 virgo-pages.apikv.com）
make k8s-deploy           # k1 集群 → virgo.apikv.com
```

`pages-deploy` 需要本机已登录 Wrangler；`pages-deploy-eo` 需要 EdgeOne CLI 已登录；
`k8s-deploy` 会构建 amd64 镜像推到 TCR，再 `helm upgrade` 到 `virgo` 命名空间。
日常 k8s 发布走 CI + ArgoCD，`make argo-apply` 只负责注册 Application。

集群连 GitHub 不稳定，ArgoCD 的源是 GitLab 镜像 `sumery/virgo`；CI 每次会回写
`values-prod.yaml` 的镜像 tag 并把 main 推到 GitLab。`virgo.apikv.com` 的 Pangolin
资源目标是 `10.10.31.240:443`（HTTPS，保留 Host）。

GitHub Actions 的 Secrets：`TCR_USERNAME`、`TCR_PASSWORD`、`GITLAB_TOKEN`、
`EDGEONE_TOKEN`、`CLOUDFLARE_ACCOUNT_ID`、`CLOUDFLARE_API_TOKEN`。缺 Cloudflare
token 时该步骤只 warning 并跳过。

## 统计

页面加载自托管 Umami（站点「Virgo 海外官网」），只统计 `virgo-overseas.pages.dev`，不带查询串。按钮事件用 `data-umami-event`：`notify`（`from` = status / hero / get）、`lang_switch`（`to`）。页脚写明了这一点。
