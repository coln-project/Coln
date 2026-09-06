// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { expect, test } from "@playwright/test"

const graphTheory = `theory Graph := sig
  V : Set
  E : V -> V -> Set
end

realm GraphRealm @ Graph
end`

test("moves from theory and graph ledgers into their stores", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 })
  await page.goto("/")
  await expect(
    page.getByRole("heading", { name: "A workbench for things that follow." }),
  ).toBeVisible()
  await expect(
    page.getByRole("navigation", { name: "Lab tools" }),
  ).toBeVisible()
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true)
  await page.setViewportSize({ width: 1280, height: 800 })

  await page.getByRole("link", { name: /Start an experiment/ }).click()
  await expect(page).toHaveURL(/\/sync\/#automerge:/)
  const graphUrl = page.url()
  await expect(page.getByText("COLN / GRAPH LAB")).toBeVisible()
  await expect(page.getByTestId("sync-status")).toBeVisible()
  await expect(page.getByTestId("head-count")).toHaveText(/^\d+ heads?$/)
  const graphCanvasBox = await page.getByTestId("graph-canvas").boundingBox()
  const graphControlsBox = await page
    .locator("aside")
    .filter({ has: page.getByTestId("add-vertex") })
    .boundingBox()
  expect(graphCanvasBox).not.toBeNull()
  expect(graphControlsBox).not.toBeNull()
  expect(graphControlsBox!.x).toBeGreaterThan(graphCanvasBox!.x)
  expect(Math.abs(graphControlsBox!.y - graphCanvasBox!.y)).toBeLessThan(2)
  expect(
    await page
      .getByTestId("graph-canvas")
      .evaluate((element) => getComputedStyle(element).minHeight),
  ).toBe("680px")
  await page.getByTestId("add-vertex").click()
  await expect(page.getByTestId("graph-vertex")).toHaveCount(1)
  const graphStoreHref = await page
    .getByRole("link", { name: "Open in Store" })
    .getAttribute("href")
  expect(graphStoreHref).toMatch(/^\/editor\/#automerge:/)
  await page.getByRole("link", { name: "Open in Store" }).click()
  await expect(page.getByText("COLN / STORE LAB")).toBeVisible()
  await expect(page.getByText("JAVASCRIPT REPL")).toBeVisible()
  await expect(page.getByTestId("table-option")).toHaveCount(2)
  await expect(
    page.getByTestId("table-option").filter({ hasText: "GraphRealm.V" }),
  ).toBeVisible()

  await page
    .getByRole("link", { name: /Theory/ })
    .first()
    .click()
  await expect(page).toHaveURL(/\/compiler\/#automerge:/)
  const theoryUrl = page.url()
  await expect(page.getByText("COLN / COMPILER")).toBeVisible()
  await expect(page.getByText("SOURCE", { exact: true })).toBeVisible()
  await expect(page.getByText("Diagnostics", { exact: true })).toBeVisible()
  const editor = page.locator(".cm-content")
  await editor.click()
  await page.keyboard.press("ControlOrMeta+A")
  await page.keyboard.insertText(graphTheory)
  const createStore = page.getByRole("button", { name: /Create empty store/ })
  await expect(createStore).toBeEnabled({ timeout: 60_000 })
  const sourcePanelBox = await page
    .getByText("SOURCE", { exact: true })
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
    page.getByRole("link", { name: "Open", exact: true }),
  ).toBeVisible()

  await page.getByRole("link", { name: "Open", exact: true }).click()
  await expect(page).toHaveURL(/\/editor\/\?theory=automerge%3A.*#automerge:/)
  await expect(page.getByTestId("table-option")).toHaveCount(2)
  await expect(
    page.getByRole("link", { name: "Return to Theory", exact: true }),
  ).toBeVisible()
  const storeUrl = page.url()

  const labTools = page.getByRole("navigation", { name: "Lab tools" })
  await labTools.getByRole("link", { name: /Graph/ }).click()
  await expect(page).toHaveURL(graphUrl)
  await expect(page.getByTestId("graph-vertex")).toHaveCount(1)

  await labTools.getByRole("link", { name: /Theory/ }).click()
  await expect(page).toHaveURL(theoryUrl)
  await expect(page.locator(".cm-content")).toContainText("theory Graph")

  await labTools.getByRole("link", { name: /Store/ }).click()
  await expect(page).toHaveURL(storeUrl)
  await expect(page.getByTestId("table-option")).toHaveCount(2)
})
