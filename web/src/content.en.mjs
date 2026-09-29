// English page. Same facts as content.zh.mjs (source: docs/site-copy.md,
// PRODUCT.md, CONTEXT.md); terms follow the CONTEXT.md glossary — Task Source,
// Evidence, Capture, Triage, restore point. The build fails if the two files
// do not have the same shape.

const notify = "mailto:support@looke.cloud?subject=" + encodeURIComponent("Tell me when Virgo launches");

export default {
  hreflang: "en",
  htmlLang: "en",
  ogLocale: "en_US",
  langName: "English",
  meta: {
    title: "Sumery Virgo · Reconcile your task list with your code · A local task reconciler for developers who work with AI agents",
    description:
      "Sumery Virgo gathers the TODO.md files in a project into one view, finds evidence for each item in your Git history and reports only the differences — whether something is done stays your call. Runs locally, no account. macOS version coming soon.",
  },
  ui: {
    skip: "Skip to content",
    home: "Virgo home",
    nav: "On this page",
    readIn: "Read in English",
    artCaption: "Demo data: a TODO.md and what reconciliation found in it",
  },
  brand: { kicker: "SUMERY · VIRGO", name: "Virgo", sub: "Local · Markdown is the single source of truth" },
  status: {
    label: "Status",
    text: "Not released yet. The macOS build is finishing signing and notarization. Leave us an email and we'll write back on launch day.",
    cta: { label: "Write to us", href: notify, event: { name: "notify", data: { from: "status" } } },
  },
  nav: [
    { href: "#how", label: "How it works" },
    { href: "#bounds", label: "Boundaries" },
    { href: "#views", label: "Views" },
    { href: "#get", label: "Launch notice" },
  ],
  hero: {
    title: ["Your task list,", "checked against your code."],
    lede:
      "Push a repository forward with an agent and the task list soon stops looking like the code: finished work left unticked, ticked items that aren't really done, and an agent that reads a stale line and does completed work all over again. Virgo gathers the project's <code>TODO.md</code> files into one view, looks for evidence of each item in your Git history and reports only the differences — <strong>whether something is done is still your call.</strong>",
    actions: [
      { kind: "primary", label: "Tell me at launch", href: notify, event: { name: "notify", data: { from: "hero" } } },
      { kind: "ghost", label: "See how it reconciles", href: "#how" },
    ],
    fine: "The launch notice is a single email, not a subscription.",
  },
  how: {
    id: "how",
    title: "Thirty seconds to line the list up with the code.",
    sub: null,
    steps: [
      {
        title: "Open a project folder.",
        body: "Virgo finds every <code>TODO.md</code> in it (plus any Markdown that declares <code>todo-spec</code>) and lists them by source. No sign-in, no network.",
      },
      {
        title: "Read the reconciliation.",
        body: "Next to each item is the evidence Git can find: which changes, which commit range. Items with no evidence, or whose evidence doesn't match their status, are flagged.",
      },
      {
        title: "Write the outcome in place.",
        body: "Done, partly done (<code>◐</code>) and failed (<code>⚠</code>) are all written on that line, with a one-line note. Virgo writes back only the bytes it recognises; everything else in the file stays byte-for-byte the same.",
      },
      {
        title: "Your agent reads the same file.",
        body: "Run <code>todo hooks install --agent claude|codex|deepseek-harness</code> and the list it reads at the start of each session is the one you just reconciled.",
      },
    ],
    note: null,
  },
  bounds: {
    id: "bounds",
    title: "It reports. It doesn't decide for you.",
    sub: null,
    items: [
      {
        title: "The file is the fact; the app is a view.",
        body: "SQLite is only a derived index and can be rebuilt from the Markdown at any time. Delete Virgo and your <code>TODO.md</code> is still plain Markdown that any editor opens.",
      },
      {
        title: "Evidence shows a change exists, not that a task is done.",
        body: "Reconciliation tells you whether an item has matching changes in Git. Ticking it off is always a person's decision.",
      },
      {
        title: "A restore point before every write.",
        body: "<code>.todo/history/</code> keeps 200 by default. Whole-file rewrites (scan imports, version restores) show a side-by-side diff before you confirm; if someone changes the file while you're previewing, the write is refused and you're asked to retry.",
      },
      {
        title: "No network, no sign-in.",
        body: "In local mode you open a folder and start. Semantic reconciliation is off by default; turning it on takes your own LLM key, and requests go straight from your machine to the provider you choose — never through our servers.",
      },
      {
        title: "Anonymous usage statistics, and a switch for them.",
        body: "By default the desktop app reports which view you open and which features you use, plus the app version and OS. Never task text, file paths or project names. Turn it off in Settings → Usage statistics.",
      },
      {
        title: "Personal captures stay out of the repo.",
        body: "Whatever you jot in the quick-capture window (<kbd>⌘⇧Space</kbd>) lives in <code>~/.todo/inbox.md</code>. It isn't covered by project restore points and is never committed.",
      },
    ],
  },
  catalog: {
    id: "views",
    title: "Four views, one file.",
    sub: null,
    items: [
      { name: "Goal", body: "In progress, queued and done on one surface; one in-progress item open by default, up to five." },
      { name: "Quadrant", body: "Drag items into four cells by <code>!1</code>–<code>!4</code>. Only that line's <code>!</code> marker is written back; unranked items wait in the tray." },
      { name: "Capture", body: "Your personal inbox, and triage. <kbd>⌘⇧Space</kbd> to jot something down, <kbd>⌘⏎</kbd> to save." },
      { name: "Scan & history", body: "Scan the whole repository for untracked work and review the diff before importing; browse and restore restore points." },
    ],
    after:
      "<code>- [ ] Task !2 ◐ ↳ note 🔗 a1b2c3..d4e5f6 &lt;!-- id --&gt;</code> — priority, partial completion, note, evidence and a stable ID are all plain characters in the file. The <code>todo</code> CLI and the desktop app read and write the same format.",
  },
  get: {
    id: "get",
    title: "Launch day isn't here yet.",
    sub: null,
    body: [
      "The macOS build is going through Developer ID signing and notarization. Until it's signed we won't post an installer — not even an unsigned “use it for now” build. Other platforms come after macOS; nothing is promised for them yet.",
      "Leave an email and we'll write back on launch day. No sign-up and no mailing list — just one email, and the body can be empty.",
    ],
    cta: { label: "Tell me at launch", href: notify, event: { name: "notify", data: { from: "get" } } },
    tableCaption: "Release status by platform",
    columns: ["Platform", "Status"],
    rows: [["macOS · Apple Silicon", "Coming soon"]],
  },
  footer: {
    sub: "Sumery Virgo · Local · Markdown is the single source of truth",
    note: "This page counts visits and button clicks with self-hosted Umami. No cookies, no identity.",
  },
  art: {
    file: "TODO.md",
    lines: [
      { box: "x", text: "Import browser pages into Reading", evidence: "2ef9e9c", flag: "ok" },
      { box: "x", text: "Harden article links", marks: "!2", flag: "warn" },
      { box: " ", text: "Heading priority", marks: "!1 ◐", note: "spec drafted" },
      { box: " ", text: "Document the DSH bridge", evidence: "48ff807", flag: "warn" },
    ],
    findings: {
      title: "Reconciliation · 2 differences",
      items: [
        { task: "Harden article links", text: "Ticked, but no matching change in Git" },
        { task: "Document the DSH bridge", text: "Changed in 48ff807, still unticked" },
      ],
      foot: "Reports only — rewrites nothing",
    },
  },
};
