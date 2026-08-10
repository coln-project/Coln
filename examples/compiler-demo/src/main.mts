// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import "../../style.css";

import {
  init,
  compile,
  getDiagnostics,
  prettyIr,
  irToJson,
} from "coln-compiler";

const inputEl = document.getElementById("input") as HTMLTextAreaElement;
const diagsEl = document.getElementById("diagnostics")!;
const prettyEl = document.getElementById("pretty")!;
const jsonEl = document.getElementById("json")!;
const examplesEl = document.getElementById("examples") as HTMLSelectElement;

// The compiler's own golden test cases, bundled in at build time. They're small
// enough that inlining them beats fetching them at runtime.
const examples = new Map(
  Object.entries(
    import.meta.glob("../../../packages/coln-compiler/test/golden/*.coln", {
      query: "?raw",
      import: "default",
      eager: true,
    }) as Record<string, string>,
  ).map(([path, source]) => [path.split("/").pop()!.replace(/\.coln$/, ""), source]),
);

for (const name of [...examples.keys()].sort()) {
  const opt = document.createElement("option");
  opt.value = name;
  opt.textContent = name;
  examplesEl.appendChild(opt);
}
examplesEl.addEventListener("change", () => {
  if (!examplesEl.value) {
    inputEl.value = "";
    diagsEl.replaceChildren();
    prettyEl.replaceChildren();
    jsonEl.textContent = "";
  } else {
    inputEl.value = examples.get(examplesEl.value)!;
    run();
  }
});

await init();

async function run() {
  document.body.dataset.compiling = "";
  const result = await compile(inputEl.value);
  prettyEl.replaceChildren(
    ...(await prettyIr(result)).map((chunk) => {
      const pre = document.createElement("pre");
      pre.innerHTML = chunk;
      return pre;
    }),
  );
  diagsEl.replaceChildren(
    ...(await getDiagnostics(true, result)).map((chunk) => {
      const pre = document.createElement("div");
      pre.innerHTML = chunk;
      return pre;
    }),
  );
  jsonEl.textContent = await irToJson(result);
  delete document.body.dataset.compiling;
}

let debounceTimer: ReturnType<typeof setTimeout>;
inputEl.addEventListener("input", () => {
  clearTimeout(debounceTimer);
  debounceTimer = setTimeout(run, 50);
});

run();
