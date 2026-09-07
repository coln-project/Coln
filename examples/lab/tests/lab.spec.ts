// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { expect, test, type Locator } from "@playwright/test"

const graphTheory = `theory Graph := sig
  V : Set
  E : V -> V -> Set
end

realm GraphRealm @ Graph
end`

async function expectActiveLineHighlight(editor: Locator): Promise<void> {
  await editor.click()
  const activeLine = editor.locator(".cm-activeLine")
  await expect(activeLine).toBeVisible()
  expect(await activeLine.evaluate((element) => getComputedStyle(element).backgroundColor))
    .toBe("rgba(0, 0, 0, 0)")
  expect(
    await activeLine.evaluate(
      (element) => getComputedStyle(element, "::before").backgroundColor,
    ),
  ).toBe("rgb(17, 26, 27)")
}

async function dragResizer(page: import("@playwright/test").Page, resizer: Locator, deltaX: number, deltaY: number): Promise<void> {
  const box = await resizer.boundingBox()
  expect(box).not.toBeNull()
  const x = box!.x + box!.width / 2
  const y = box!.y + box!.height / 2
  await page.mouse.move(x, y)
  await page.mouse.down()
  await page.mouse.move(x + deltaX, y + deltaY, { steps: 5 })
  await page.mouse.up()
}

async function expectMinimumTextSize(
  page: import("@playwright/test").Page,
): Promise<void> {
  const violations = await page.locator("body *").evaluateAll((elements) =>
    elements.flatMap((element) => {
      const style = getComputedStyle(element)
      const hasDirectText = [...element.childNodes].some(
        (node) => node.nodeType === Node.TEXT_NODE && node.textContent?.trim(),
      )
      const isTextControl =
        element instanceof HTMLInputElement ||
        element instanceof HTMLSelectElement ||
        element instanceof HTMLTextAreaElement
      if (
        (!hasDirectText && !isTextControl) ||
        style.display === "none" ||
        style.visibility === "hidden" ||
        element.getClientRects().length === 0
      ) return []
      let size = Number.parseFloat(style.fontSize)
      if (element instanceof SVGTextElement) {
        const transform = element.getScreenCTM()
        if (transform) size *= Math.hypot(transform.a, transform.b)
      }
      const minimum = element.closest("[data-small-detail]") ? 12 : 14
      return size < minimum
        ? [`${element.tagName.toLowerCase()}: ${size}px (${element.textContent?.trim().slice(0, 40)})`]
        : []
    }),
  )
  expect(violations).toEqual([])
}

test("loads every tool from its static route", async ({ page }) => {
  await page.goto("/compiler/")
  await expect(page).toHaveTitle("Theory Editor — Coln Lab")
  await expect(page.getByTestId("active-tool-identity")).toHaveText("Theory Editor")
  await expect(page.getByTestId("active-tool-identity")).toBeVisible()
  await expect(page.getByTestId("active-tool-action")).toHaveAttribute("aria-label", "New theory")
  await expect(page.getByTestId("active-tool-action")).toHaveAttribute("href", "/compiler/")
  await expect(page.locator("main > header")).toHaveCount(1)

  await page.goto("/editor/")
  await expect(page).toHaveTitle("Store Editor — Coln Lab")
  await expect(page.getByTestId("active-tool-identity")).toHaveText("Store Editor")
  await expect(page.getByTestId("active-tool-identity")).toBeVisible()
  await expect(page.getByTestId("active-tool-action")).toHaveAttribute("aria-label", "Open store")
  await expect(page.getByTestId("active-tool-action")).toHaveAttribute("href", "/editor/")
  await expect(
    page.getByRole("heading", { name: "Inspect a Coln store." }),
  ).toBeVisible()

  await page.goto("/sync/")
  await expect(page).toHaveTitle("Graph Demo — Coln Lab")
  await expect(page.getByTestId("active-tool-identity")).toHaveText("Graph Demo")
  await expect(page.getByTestId("active-tool-identity")).toBeVisible()
  await expect(page.getByTestId("active-tool-action")).toHaveAttribute("aria-label", "New graph")
  await expect(page.getByTestId("active-tool-action")).toHaveAttribute("href", "/sync/")
})

test("highlights the active compiler line", async ({ page }) => {
  await page.goto("/compiler/")
  await expectActiveLineHighlight(page.locator(".cm-content"))
})

test("resizes and persists graph panes", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 })
  await page.goto("/sync/")

  const canvas = page.getByTestId("graph-canvas")
  const workspaceResizer = page.getByTestId("graph-workspace-resizer")
  await expect(workspaceResizer).toBeVisible()
  const canvasPaneBox = await page.locator("#graph-canvas-pane").boundingBox()
  const canvasBox = await canvas.boundingBox()
  expect(canvasPaneBox).not.toBeNull()
  expect(canvasBox).not.toBeNull()
  expect(Math.abs(canvasBox!.height - canvasPaneBox!.height)).toBeLessThan(2)
  const graphPanelBox = await page.locator("#graph-graph-panel").boundingBox()
  const storeSummaryBox = await page.getByTestId("store-summary").boundingBox()
  expect(graphPanelBox).not.toBeNull()
  expect(storeSummaryBox).not.toBeNull()
  expect(Math.abs(
    storeSummaryBox!.y + storeSummaryBox!.height - (graphPanelBox!.y + graphPanelBox!.height),
  )).toBeLessThanOrEqual(2)
  const initialCanvasWidth = (await canvas.boundingBox())!.width
  await dragResizer(page, workspaceResizer, -120, 0)
  const resizedCanvasWidth = (await canvas.boundingBox())!.width
  expect(resizedCanvasWidth).toBeLessThan(initialCanvasWidth - 80)

  await expect.poll(() => page.evaluate(() => {
    const value = localStorage.getItem("paneforge:coln-lab-graph-workspace")
    if (!value) return false
    return Object.values(JSON.parse(value) as Record<string, { layout: number[] }>)
      .some(({ layout }) => layout[0] < 60)
  })).toBe(true)
  const persistedWidth = (await canvas.boundingBox())!.width
  await page.reload()
  await expect(page.getByTestId("graph-canvas")).toBeVisible()
  expect(Math.abs((await canvas.boundingBox())!.width - persistedWidth)).toBeLessThan(10)

  await workspaceResizer.focus()
  const keyboardStartWidth = (await canvas.boundingBox())!.width
  await page.keyboard.press("ArrowRight")
  expect((await canvas.boundingBox())!.width).toBeGreaterThan(keyboardStartWidth)

  await page.getByRole("tab", { name: "Store REPL" }).click()
  const output = page.getByTestId("repl-output")
  const outputResizer = page.getByTestId("repl-output-resizer")
  const initialOutputHeight = (await output.boundingBox())!.height
  await dragResizer(page, outputResizer, 0, -80)
  expect((await output.boundingBox())!.height).toBeGreaterThan(initialOutputHeight + 50)

  await page.setViewportSize({ width: 390, height: 844 })
  await expect(workspaceResizer).toBeHidden()
  await expect(outputResizer).toBeHidden()
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
})

test("uses the graph demo and moves from theories into their stores", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 })
  await page.goto("/")
  await expect(
    page.getByRole("heading", { name: "Design theories. Inspect stores. Explore Coln." }),
  ).toBeVisible()
  await expect(
    page.getByRole("navigation", { name: "Coln Lab" }),
  ).toBeVisible()
  await expect(page.getByRole("group", { name: "Workbench" })).toBeVisible()
  await expect(page.getByRole("group", { name: "Demos" })).toBeVisible()
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true)
  await expectMinimumTextSize(page)
  await page.setViewportSize({ width: 1280, height: 800 })

  await page.getByRole("link", { name: /Open Graph Demo/ }).click()
  await expect(page).toHaveURL(/\/sync\/#coln:/)
  const graphUrl = page.url()
  await expect(page.getByTestId("active-tool-identity")).toHaveText("Graph Demo")
  await expect(page.getByTestId("sync-status")).toBeVisible()
  await expect(page.getByTestId("head-count")).toHaveText(/^\d+ heads?$/)
  const graphCanvasBox = await page.getByTestId("graph-canvas").boundingBox()
  const graphControlsBox = await page.getByTestId("graph-sidebar").boundingBox()
  expect(graphCanvasBox).not.toBeNull()
  expect(graphControlsBox).not.toBeNull()
  expect(graphControlsBox!.x).toBeGreaterThan(graphCanvasBox!.x)
  expect(Math.abs(graphControlsBox!.y - graphCanvasBox!.y)).toBeLessThan(2)
  expect(
    await page
      .getByTestId("graph-canvas")
      .evaluate((element) => getComputedStyle(element).minHeight),
  ).toBe("680px")
  await expect(page.getByTestId("store-summary-table")).toHaveCount(2)
  await expectMinimumTextSize(page)
  const vertexSummary = page.locator('[data-table-name="GraphRealm.V"]')
  const edgeSummary = page.locator('[data-table-name="GraphRealm.E"]')
  await page.getByTestId("add-vertex").click()
  await expect(page.getByTestId("graph-vertex")).toHaveCount(1)
  await expect(vertexSummary.getByTestId("store-summary-row-count")).toHaveText("1 row")
  await page.getByTestId("add-vertex").click()
  await expect(page.getByTestId("graph-vertex")).toHaveCount(2)
  await page.getByTestId("graph-vertex").nth(0).click()
  await page.getByTestId("graph-vertex").nth(1).click()
  await expect(page.getByTestId("add-edge")).toBeEnabled()
  await page.getByTestId("add-edge").click()
  await expect(page.getByTestId("graph-edge")).toHaveCount(1)
  await expect(vertexSummary.getByTestId("store-summary-row-count")).toHaveText("2 rows")
  await expect(edgeSummary.getByTestId("store-summary-row-count")).toHaveText("1 row")

  await page.getByRole("tab", { name: "Store REPL" }).click()
  await expect(page).toHaveURL(graphUrl)
  await expect(page.getByTestId("store-repl")).toBeVisible()
  await expectActiveLineHighlight(page.getByTestId("repl-editor"))
  await page.getByTestId("run-program").click()
  await expect(page.getByTestId("repl-result")).toBeVisible()
  await page.getByRole("tab", { name: "Graph" }).click()
  await expect(page.getByTestId("graph-edge")).toHaveCount(1)
  await page.setViewportSize({ width: 390, height: 844 })
  await expect(page.getByRole("tab", { name: "Store REPL" })).toBeVisible()
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true)
  await expectMinimumTextSize(page)
  await page.setViewportSize({ width: 1280, height: 800 })

  const labTools = page.getByRole("navigation", { name: "Coln Lab" })
  await labTools.getByRole("link", { name: /Store Editor/ }).click()
  await expect(page).toHaveURL(/\/editor\/$/)
  await expect(page.getByTestId("recent-store")).toHaveCount(0)

  await labTools.getByRole("link", { name: /Theory Editor/ }).click()
  await expect(page).toHaveURL(/\/compiler\/#automerge:/)
  const theoryUrl = page.url()
  const theoryDocumentUrl = new URL(theoryUrl).hash.slice(1)
  await expect(page.getByTestId("active-tool-identity")).toHaveText("Theory Editor")
  await expect(page.getByText("THEORY SOURCE", { exact: true })).toBeVisible()
  await expect(page.getByText("Diagnostics", { exact: true })).toBeVisible()
  await expect(page.getByTestId("theory-source-resizer")).toBeVisible()
  await expect(page.getByTestId("theory-stores-resizer")).toBeVisible()
  await expectMinimumTextSize(page)
  await page.setViewportSize({ width: 390, height: 844 })
  await expect(page.getByTestId("active-tool-action")).toBeVisible()
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
  await expectMinimumTextSize(page)
  await page.setViewportSize({ width: 1280, height: 800 })
  const editor = page.locator(".cm-content")
  await editor.click()
  await page.keyboard.press("ControlOrMeta+A")
  await page.keyboard.insertText(graphTheory)
  const createStore = page.getByRole("button", { name: /Create store/ })
  await expect(createStore).toBeEnabled({ timeout: 60_000 })
  const sourcePanelBox = await page
    .getByText("THEORY SOURCE", { exact: true })
    .locator("xpath=ancestor::section[1]")
    .boundingBox()
  const storesPanelBox = await page
    .locator("aside")
    .filter({ hasText: "STORES" })
    .boundingBox()
  expect(sourcePanelBox).not.toBeNull()
  expect(storesPanelBox).not.toBeNull()
  expect(storesPanelBox!.x).toBeGreaterThan(sourcePanelBox!.x)
  expect(Math.abs(storesPanelBox!.y - sourcePanelBox!.y)).toBeLessThan(2)
  await createStore.click()
  await expect(
    page.getByRole("link", { name: "Open in Store Editor", exact: true }),
  ).toBeVisible()

  await page.getByRole("link", { name: "Open in Store Editor", exact: true }).click()
  await expect(page).toHaveURL(/\/editor\/\?theory=automerge%3A.*#coln:/)
  await expect(page.getByTestId("table-option")).toHaveCount(2)
  await expectMinimumTextSize(page)
  await page.setViewportSize({ width: 390, height: 844 })
  await expect(page.getByRole("link", { name: "Return to Theory Editor" })).toBeVisible()
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
  await expectMinimumTextSize(page)
  await page.setViewportSize({ width: 1280, height: 800 })
  const storeBrowser = page.locator("#store-browser-pane")
  const storeBrowserWidth = (await storeBrowser.boundingBox())!.width
  await page.getByTestId("store-workspace-resizer").focus()
  await page.keyboard.press("ArrowRight")
  expect((await storeBrowser.boundingBox())!.width).toBeGreaterThan(storeBrowserWidth)
  await expect(
    page.getByRole("link", { name: "Return to Theory Editor", exact: true }),
  ).toBeVisible()
  const replEditor = page.getByTestId("repl-editor")
  await replEditor.click()
  await page.keyboard.press("ControlOrMeta+A")
  await page.keyboard.insertText(`handle.change((transaction) => {
  const from = transaction.add("GraphRealm.V", [])
  const to = transaction.add("GraphRealm.V", [])
  transaction.add("GraphRealm.E", [from, to])
})
return handle.doc().heads()`)
  await page.getByTestId("run-program").click()
  await expect(page.getByTestId("repl-result")).toBeVisible()
  const edgeTable = page.getByTestId("table-option").filter({ hasText: "GraphRealm.E" })
  await edgeTable.click()
  const references = page.getByTestId("table-reference")
  await expect(references).toHaveCount(2)
  const targetId = (await references.first().getAttribute("aria-label"))!
    .replace("Open referenced row ", "")
  await references.first().click()
  await expect(
    page.getByTestId("table-option").filter({ hasText: "GraphRealm.V" }),
  ).toHaveAttribute("data-selected", "true")
  const targetRow = page.getByTestId("table-row").filter({
    has: page.locator(`td[title="${targetId}"]`),
  })
  await expect(targetRow).toHaveAttribute("data-selected", "true")
  await expect(targetRow).toBeFocused()
  await edgeTable.click()
  await references.first().click()
  await expect(targetRow).toBeFocused()
  const storeDocumentUrl = new URL(page.url()).hash.slice(1)

  await labTools.getByRole("link", { name: /Graph Demo/ }).click()
  await expect(page).toHaveURL(graphUrl)
  await expect(page.getByTestId("graph-vertex")).toHaveCount(2)
  await expect(page.getByTestId("graph-edge")).toHaveCount(1)

  await labTools.getByRole("link", { name: /Theory Editor/ }).click()
  await expect(page).toHaveURL(theoryUrl)
  await expect(page.locator(".cm-content")).toContainText("theory Graph")

  await labTools.getByRole("link", { name: /Store Editor/ }).click()
  await expect(page).toHaveURL(/\/editor\/#coln:/)
  expect(new URL(page.url()).hash.slice(1)).toBe(storeDocumentUrl)
  await expect(page.getByTestId("table-option")).toHaveCount(2)
  await page.getByTestId("active-tool-action").click()
  await expect(page).toHaveURL(/\/editor\/$/)
  await expect(page.getByTestId("recent-store").locator("code")).toHaveAttribute(
    "title",
    storeDocumentUrl,
  )

  await page
    .getByTestId("document-url-input")
    .fill(storeDocumentUrl.replace(/^coln:/, "automerge:"))
  await page.getByTestId("open-store").click()
  await expect(page.getByTestId("load-error")).toBeVisible()
  await expect(page.getByTestId("recent-store")).toBeVisible()

  await page.reload()
  await expect(page.getByTestId("recent-store")).toBeVisible()
  await page.goto(
    `/editor/?theory=${encodeURIComponent(theoryDocumentUrl)}#invalid`,
  )
  await expect(page.getByTestId("load-error")).toBeVisible()
  await expect(page.getByTestId("recent-store")).toBeVisible()
  await page.getByRole("button", { name: "Reopen store" }).click()
  await expect(page).toHaveURL(/\/editor\/#coln:/)
  await expect(page.getByTestId("table-option")).toHaveCount(2)
  await expect(
    page.getByRole("link", { name: "Return to Theory Editor", exact: true }),
  ).toHaveCount(0)
})
