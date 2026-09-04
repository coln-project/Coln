// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { expect, test } from "@playwright/test"
import {
  initSubduction,
  Repo,
  type AutomergeUrl,
} from "@automerge/automerge-repo"
import { colnDocType } from "@coln-project/repo"
import { executeQuery } from "../src/query/execute-query.ts"

test.beforeAll(async () => {
  await initSubduction()
})

test("browser controls and TypeScript queries converge", async ({ browser }) => {
  test.setTimeout(60_000)
  const context = await browser.newContext()
  await context.grantPermissions(["clipboard-read", "clipboard-write"], {
    origin: "http://127.0.0.1:5176",
  })
  const page = await context.newPage()
  const nodeRepo = new Repo({
    subductionWebsocketEndpoints: ["ws://127.0.0.1:3031"],
  })

  try {
    await page.goto("/")
    await expect(page.getByTestId("sync-status")).toHaveAttribute("data-status", "synced")
    await expect(page.getByTestId("head-count")).toHaveText("1 head")
    await expect(page.getByTestId("subduction-url")).toHaveText("ws://127.0.0.1:3031")
    await expect(page.getByTestId("doc-url")).toHaveValue(/^automerge:/)
    await page.getByTestId("copy-document-url").click()
    await expect(page.getByTestId("feedback")).toHaveText("URL copied")

    const url = await page.getByTestId("doc-url").inputValue() as AutomergeUrl
    const nodeHandle = await nodeRepo.find(url, colnDocType)

    await page.getByTestId("add-vertex").click()
    await expect(page.getByTestId("feedback")).toHaveText("Vertex added")
    const firstVertexFeedbackId = await page.getByTestId("feedback").getAttribute("data-feedback-id")
    await page.getByTestId("add-vertex").click()
    await expect(page.getByTestId("feedback")).not.toHaveAttribute("data-feedback-id", firstVertexFeedbackId!)
    await expect(page.getByTestId("graph-vertex")).toHaveCount(2)
    const initialVertexLabels = await vertexLabels(page)
    await expect(page.getByTestId("sync-status")).toHaveAttribute("data-status", "synced")
    await expect.poll(() => nodeHandle.doc().scanTable("GraphRealm.V").length).toBe(2)

    executeQuery(nodeHandle, 'txn.add("GraphRealm.V", [])')
    await nodeRepo.flush()
    nodeRepo.resyncSubduction(nodeHandle.documentId)
    await expect(page.getByTestId("graph-vertex")).toHaveCount(3)
    const updatedVertexLabels = await vertexLabels(page)
    expect([...initialVertexLabels].map(([id]) => [id, updatedVertexLabels.get(id)]))
      .toEqual([...initialVertexLabels])

    const values = await page.getByTestId("from-select").locator("option").evaluateAll(options =>
      options.slice(1).map(option => (option as HTMLOptionElement).value),
    )
    await page.getByTestId("from-select").selectOption(values[0])
    await page.getByTestId("to-select").selectOption(values[1])
    await expect(page.getByTestId("edge-preview")).toBeVisible()
    await expect(page.getByTestId("edge-preview")).toHaveClass(/\[stroke-dasharray:2_9\]/)
    await expect(page.getByTestId("edge-preview")).toHaveCSS("pointer-events", "none")
    await expect(page.getByTestId("edge-preview")).toHaveAttribute("d", /^M /)
    await page.getByTestId("add-edge").click()
    await expect(page.getByTestId("feedback")).toHaveText("Edge added")
    await expect(page.getByTestId("from-select")).toHaveValue("")
    await expect(page.getByTestId("to-select")).toHaveValue("")
    await expect(page.getByTestId("edge-preview")).toBeHidden()

    await page.getByTestId("from-select").selectOption(values[0])
    await page.getByTestId("to-select").selectOption(values[1])
    await page.getByTestId("add-edge").click()
    await expect(page.getByTestId("graph-edge")).toHaveCount(2)
    await expect(page.getByTestId("graph-counts")).toHaveText("3 VERTICES / 2 EDGES")

    const existingPaths = await page.getByTestId("graph-edge").evaluateAll(edges =>
      edges.map(edge => edge.getAttribute("d")),
    )
    await page.getByTestId("from-select").selectOption(values[1])
    await page.getByTestId("to-select").selectOption(values[0])
    await expect(page.getByTestId("edge-preview")).toBeVisible()
    expect(existingPaths).not.toContain(await page.getByTestId("edge-preview").getAttribute("d"))

    const selectedEdge = page.getByTestId("graph-edge").first()
    await selectedEdge.dispatchEvent("click")
    await expect(page.getByTestId("selected-edge-details")).toBeVisible()
    await expect(selectedEdge).toHaveCSS("outline-style", "none")
    await expect(selectedEdge).toHaveCSS("stroke", "rgb(101, 217, 231)")
    await expect(selectedEdge).toHaveClass(/hover:stroke-\[#d8ff57\]/)

    await page.getByTestId("graph-canvas").dispatchEvent("click")
    await expect(page.getByTestId("selected-edge-details")).toBeHidden()

    await expect(page.getByTestId("sync-status")).toHaveAttribute("data-status", "synced")
    await expect.poll(() => nodeHandle.doc().scanTable("GraphRealm.E").length).toBe(2)

    const secondPage = await context.newPage()
    await secondPage.goto(`/#${url}`)
    await expect(secondPage.getByTestId("graph-vertex")).toHaveCount(3)
    await expect(secondPage.getByTestId("graph-edge")).toHaveCount(2)
  } finally {
    await nodeRepo.shutdown()
    await context.close()
  }
})

test("browser persists graph data in IndexedDB", async ({ browser }) => {
  const context = await browser.newContext()
  const page = await context.newPage()

  try {
    await page.goto("/")
    await expect(page.getByTestId("sync-status")).toHaveAttribute("data-status", "synced")

    await page.getByTestId("add-vertex").click()
    await page.getByTestId("add-vertex").click()
    const vertices = await page.getByTestId("from-select").locator("option").evaluateAll(options =>
      options.slice(1).map(option => (option as HTMLOptionElement).value),
    )
    await page.getByTestId("from-select").selectOption(vertices[0])
    await page.getByTestId("to-select").selectOption(vertices[1])
    await page.getByTestId("add-edge").click()
    await expect(page.getByTestId("sync-status")).toHaveAttribute("data-status", "synced")
    await expect.poll(() => indexedDbRecordCount(page, "coln-sync", "documents")).toBeGreaterThan(0)

    const url = await page.getByTestId("doc-url").inputValue()
    await page.close()

    const reloadedPage = await context.newPage()
    await reloadedPage.goto(`/#${url}`)
    await expect(reloadedPage.getByTestId("graph-vertex")).toHaveCount(2)
    await expect(reloadedPage.getByTestId("graph-edge")).toHaveCount(1)
  } finally {
    await context.close()
  }
})

test("shows a dedicated screen when a document is unavailable", async ({ browser }) => {
  const isolatedRepo = new Repo()
  const unavailableUrl = isolatedRepo.create().url
  await isolatedRepo.shutdown()
  const context = await browser.newContext()
  const page = await context.newPage()

  try {
    await page.goto(`/#${unavailableUrl}`)

    await expect(page.getByTestId("document-load-error")).toHaveAttribute("data-error-kind", "unavailable")
    await expect(page.getByTestId("failed-document-url")).toHaveText(unavailableUrl)
    await expect(page.getByTestId("graph-canvas")).toHaveCount(0)
    await expect(page.getByTestId("add-vertex")).toHaveCount(0)
    await page.getByRole("link", { name: "Create a new graph" }).click()
    await expect(page.getByTestId("document-load-error")).toHaveCount(0)
    await expect(page.getByTestId("graph-canvas")).toBeVisible()
    await expect(page.getByTestId("doc-url")).toHaveValue(/^automerge:/)
    expect(await page.getByTestId("doc-url").inputValue()).not.toBe(unavailableUrl)
  } finally {
    await context.close()
  }
})

test("shows the document error screen for an invalid URL", async ({ page }) => {
  const invalidUrl = "not-an-automerge-url"

  await page.goto(`/#${invalidUrl}`)

  await expect(page.getByTestId("document-load-error")).toHaveAttribute("data-error-kind", "invalid")
  await expect(page.getByRole("heading")).toHaveText("This is not a valid Automerge document URL.")
  await expect(page.getByTestId("failed-document-url")).toHaveText(invalidUrl)
  await expect(page.getByTestId("graph-canvas")).toHaveCount(0)
  await expect(page.getByTestId("add-vertex")).toHaveCount(0)
})

async function vertexLabels(page: import("@playwright/test").Page) {
  const entries = await page.getByTestId("graph-vertex").evaluateAll(vertices =>
    vertices.map(vertex => [
      vertex.querySelector("title")?.textContent ?? "",
      vertex.querySelector("text")?.textContent ?? "",
    ] as const),
  )
  return new Map(entries)
}

async function indexedDbRecordCount(
  page: import("@playwright/test").Page,
  database: string,
  store: string,
) {
  return page.evaluate(({ database, store }) => new Promise<number>((resolve, reject) => {
    const request = indexedDB.open(database)
    request.onerror = () => reject(request.error)
    request.onsuccess = () => {
      const count = request.result.transaction(store).objectStore(store).count()
      count.onerror = () => reject(count.error)
      count.onsuccess = () => resolve(count.result)
    }
  }), { database, store })
}
