// Hero object for Virgo: a TODO.md sheet lying on the desk, with the
// reconciliation beside it. Built from text, not a picture, so both languages
// render their own demo lines and the sheet stays sharp at any size.
//
// content.art = {
//   file: "TODO.md",
//   lines: [{ box: "x" | " ", text, marks?, evidence?, note?, flag?: "ok" | "warn" }],
//   findings: { title, items: [{ task, text }] , foot },
// }

export function renderArt(a) {
  const lines = a.lines
    .map(
      (l) => `
        <li class="todo-line${l.flag ? ` is-${l.flag}` : ""}">
          <span class="todo-box" aria-hidden="true">${l.box === "x" ? "✓" : ""}</span>
          <span class="todo-text">${l.text}${l.marks ? ` <span class="todo-marks">${l.marks}</span>` : ""}${
            l.note ? `<span class="todo-note">↳ ${l.note}</span>` : ""
          }</span>
          ${l.evidence ? `<code class="todo-ev">🔗 ${l.evidence}</code>` : `<span class="todo-ev todo-ev--none">—</span>`}
        </li>`,
    )
    .join("");
  const findings = a.findings.items
    .map((f) => `<li><strong>${f.task}</strong><span>${f.text}</span></li>`)
    .join("");
  return `
    <div class="desk" aria-hidden="true">
      <div class="sheet">
        <p class="sheet-name"><span class="sheet-dot"></span>${a.file}</p>
        <ul class="todo">${lines}</ul>
      </div>
      <div class="findings">
        <p class="findings-title">${a.findings.title}</p>
        <ul>${findings}</ul>
        <p class="findings-foot">${a.findings.foot}</p>
      </div>
    </div>`;
}
