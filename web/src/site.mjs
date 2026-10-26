// Product constants for the landing site. Everything that is the same in both
// languages lives here; everything a reader sees lives in content.<lang>.mjs.

export const SITE = {
  /** Cloudflare Pages project (web/wrangler.toml) and its production origin. */
  project: "virgo-overseas",
  origin: "https://virgo-overseas.pages.dev",
  /** Umami website 「Virgo 海外官网」. Counts only the production host. */
  umami: {
    src: "https://umami.apikv.com/s.js",
    websiteId: "f9c9b17e-5777-435d-bfd1-f5e199b70c6d",
    domains: "virgo-overseas.pages.dev",
  },
  /** `light` = the Sumery desk (tools); `night` = the dark scene (games). */
  scene: "light",
  themeColor: "#f7efe4",
  /** Kept identical in every language: legal names are not translated. */
  filing: {
    owner: "Sumery · 上思希曼科技工作室",
    icp: { text: "桂ICP备2022004535号-5", href: "https://beian.miit.gov.cn/" },
    police: {
      text: "桂公网安备45060002450628号",
      href: "https://beian.mps.gov.cn/#/query/webSearch?code=45060002450628",
    },
    email: "support@looke.cloud",
  },
};
