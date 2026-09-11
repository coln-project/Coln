// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

/**
 * Loads the built compiler chunk in headless Chrome and compiles a theory with
 * it, the way the Theory Editor does.
 *
 * This exists because the first version of src/theory/compiler.ts resolved its
 * two files against `import.meta.url`, which pointed at dist/assets/ while the
 * files sat at dist/ — a 404 no unit test could see, and the only thing the
 * tool needs the browser for. The probe imports the *built* chunk, so it is
 * testing the emitted urls and not a copy of the source.
 *
 *   node probe/compiler-probe.mjs [--headed]
 *
 * Needs `pnpm build` first. Exits non-zero with the report on any failure.
 */

import { spawn } from "node:child_process"
import { createReadStream, existsSync, readdirSync, statSync } from "node:fs"
import { rm } from "node:fs/promises"
import { createServer } from "node:http"
import { extname, join, resolve } from "node:path"

const dist = resolve(import.meta.dirname, "..", "dist")
const port = 4457
const profile = "/tmp/coln-patchwork-probe-profile"
const headed = process.argv.includes("--headed")
const MIME = {
  ".js": "text/javascript",
  ".json": "application/json",
  ".html": "text/html",
  ".wasm": "application/wasm",
  ".map": "application/json",
}

if (!existsSync(dist)) {
  console.error("no dist/ — run `pnpm build` first")
  process.exit(2)
}

const chunk = readdirSync(join(dist, "assets")).find(
  name => name.startsWith("compiler-") && name.endsWith(".js"),
)
if (!chunk) {
  console.error("no compiler chunk in dist/assets — did the build change?")
  process.exit(2)
}

const goodTheory = `theory Graph := sig
  V : Set
  E : V -> V -> Set
end

realm GraphRealm @ Graph
end`
const brokenTheory = "realm Nope @ MissingTheory\nend"

const page = `<!doctype html><meta charset="utf-8"><title>coln compiler probe</title>
<script type="module">
const report = value =>
  fetch("/report", { method: "POST", body: JSON.stringify(value, null, 2) })
const timings = []
const at = performance.now()
const step = async (name, work) => {
  const started = performance.now()
  const value = await work()
  timings.push({ step: name, ms: +(performance.now() - started).toFixed(1) })
  return value
}

try {
  const { loadCompiler } = await step("import chunk", () =>
    import(${JSON.stringify(`./assets/${chunk}`)}))
  const compiler = await step("loadCompiler", () => loadCompiler())
  const good = await step("compile graph theory", () =>
    compiler.compile(${JSON.stringify(goodTheory)}))
  const broken = await step("compile broken theory", () =>
    compiler.compile(${JSON.stringify(brokenTheory)}))
  const again = await step("recompile graph theory", () =>
    compiler.compile(${JSON.stringify(goodTheory)}))

  await report({
    ok: true,
    totalMs: +(performance.now() - at).toFixed(1),
    timings,
    good: {
      diagnostics: good.diagnosticsHtml.length,
      prettyIrRealms: good.prettyIr.length,
      irJson: good.irJson,
    },
    broken: {
      diagnostics: broken.diagnosticsHtml.length,
      firstDiagnostic: broken.diagnosticsHtml[0] ?? "",
      prettyIrRealms: broken.prettyIr.length,
    },
    again: {
      diagnostics: again.diagnosticsHtml.length,
      sameIr: again.irJson === good.irJson,
    },
  })
} catch (error) {
  await report({ ok: false, timings, fatal: String(error?.stack ?? error) })
}
</script>`

let reported
const server = createServer((request, response) => {
  response.setHeader("Access-Control-Allow-Origin", "*")
  if (request.method === "POST" && request.url === "/report") {
    let body = ""
    request.on("data", chunk => (body += chunk))
    request.on("end", () => {
      response.writeHead(204).end()
      reported = body
      finish()
    })
    return
  }
  const { pathname } = new URL(request.url, "http://probe")
  if (pathname === "/" || pathname === "/probe.html") {
    response.writeHead(200, { "content-type": "text/html" })
    response.end(page)
    return
  }
  const file = join(dist, pathname)
  if (!file.startsWith(dist) || !existsSync(file) || statSync(file).isDirectory()) {
    response.writeHead(404).end()
    return
  }
  response.writeHead(200, {
    "content-type": MIME[extname(file)] ?? "application/octet-stream",
  })
  createReadStream(file).pipe(response)
})

server.listen(port)

const chrome = spawn(
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  [
    ...(headed ? [] : ["--headless=new"]),
    "--disable-gpu",
    "--no-first-run",
    "--no-default-browser-check",
    `--user-data-dir=${profile}`,
    `http://localhost:${port}/probe.html`,
  ],
  { stdio: "ignore" },
)

const timeout = setTimeout(() => {
  console.error("TIMEOUT: the probe never reported")
  finish(2)
}, 120_000)

let finishing = false
async function finish(code) {
  if (finishing) return
  finishing = true
  clearTimeout(timeout)
  chrome.kill()
  server.close()
  await rm(profile, { force: true, recursive: true }).catch(() => {})
  if (reported) console.log(reported)
  const report = reported ? JSON.parse(reported) : undefined
  process.exit(code ?? (report?.ok ? 0 : 1))
}
