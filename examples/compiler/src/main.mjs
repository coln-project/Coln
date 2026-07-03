import { init, compile, getDiagnostics, prettyIr, irToJson } from "coln-compiler";

const inputEl = document.getElementById("input");
const diagsEl = document.getElementById("diagnostics");
const prettyEl = document.getElementById("pretty");
const jsonEl = document.getElementById("json");
const examplesEl = document.getElementById("examples");
const exampleFiles = await fetch("examples/index.json").then((r) => r.json());
for (const name of exampleFiles) {
  const opt = document.createElement("option");
  opt.value = name;
  opt.textContent = name.split(".")[0];
  examplesEl.appendChild(opt);
}
examplesEl.addEventListener("change", async () => {
  if (!examplesEl.value) {
    inputEl.value = "";
    diagsEl.replaceChildren();
    prettyEl.replaceChildren();
    jsonEl.textContent = "";
  } else {
    inputEl.value = await fetch("examples/" + examplesEl.value).then((r) =>
      r.text(),
    );
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

let debounceTimer;
inputEl.addEventListener("input", () => {
  clearTimeout(debounceTimer);
  debounceTimer = setTimeout(run, 50);
});

run();
