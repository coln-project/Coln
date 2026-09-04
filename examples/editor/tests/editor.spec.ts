// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { expect, test } from "@playwright/test"
import { Repo, initSubduction } from "@automerge/automerge-repo"
import {
  colnDocType,
  type ColnHandle,
  type ColnSchema,
} from "@coln-project/repo"

const endpoint = "ws://127.0.0.1:3032"
const schema = {
  entities: [
    {
      path: [["Lab"], ["V"]],
      value: {
        entityVariant: { tag: "table" },
        columns: [],
        primaryKey: null,
      },
    },
    {
      path: [["Lab"], ["profile"]],
      value: {
        entityVariant: { tag: "table" },
        columns: [
          { path: [["owner"]], type: { tag: "rowId", path: [["Lab"], ["V"]] } },
          { path: [["name"]], type: { tag: "builtin", type: "builtinString" } },
          { path: [["rank"]], type: { tag: "builtin", type: "builtinInt" } },
        ],
        primaryKey: [[["owner"]]],
      },
    },
  ],
  rules: [],
} satisfies ColnSchema

let repo: Repo
let handle: ColnHandle
let secondHandle: ColnHandle

test.beforeAll(async () => {
  await initSubduction()
  repo = new Repo({ subductionWebsocketEndpoints: [endpoint] })
  handle = repo.create(schema, colnDocType)
  handle.change((transaction) => {
    const ada = transaction.add("Lab.V", [])
    transaction.add("Lab.V", [])
    transaction.add("Lab.profile", [
      ada,
      { tag: "string", value: "Ada" },
      { tag: "int", value: 37 },
    ])
  })
  await repo.flush([handle.documentId])
  secondHandle = repo.create(schema, colnDocType)
  secondHandle.change((transaction) => {
    transaction.add("Lab.V", [])
  })
  await repo.flush([secondHandle.documentId])
})

test.afterAll(async () => {
  await repo.shutdown()
})

test("loads, inspects, and programs a Coln store", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  await page.goto("/")
  await expect(
    page.getByText("Inspect a synchronized Coln store."),
  ).toBeVisible()
  await page.getByTestId("document-url-input").fill("not-a-document")
  await page.getByTestId("open-store").click()
  await expect(page.getByTestId("load-error")).toContainText("valid automerge:")
  await page.getByTestId("document-url-input").fill(handle.url)
  await page.getByTestId("open-store").click()

  await expect(page).toHaveURL(new RegExp(`#${handle.url}$`))
  await expect(page.getByTestId("table-option")).toHaveCount(2)
  const tableBox = await page.getByTestId("table-grid").boundingBox()
  const replBox = await page.getByTestId("run-program").boundingBox()
  expect(tableBox).not.toBeNull()
  expect(replBox).not.toBeNull()
  expect(replBox!.y).toBeGreaterThan(tableBox!.y)
  await page.setViewportSize({ width: 1280, height: 800 })

  await page
    .getByTestId("table-option")
    .filter({ hasText: "Lab.profile" })
    .click()
  await expect(page.getByTestId("table-row")).toHaveCount(1)
  await expect(page.getByTestId("table-grid")).toContainText("Ada")
  await expect(page.getByTestId("table-grid")).toContainText("37")

  const program = `console.log("before", handle.doc().scanTable("Lab.V").length)
handle.change(transaction => transaction.add("Lab.V", []))
return handle.doc().scanTable("Lab.V").length`
  const editor = page.locator(".cm-content")
  await editor.click()
  await page.keyboard.press("ControlOrMeta+A")
  await page.keyboard.insertText(program)
  await page.getByTestId("run-program").click()

  await expect(page.getByTestId("console-entry")).toContainText("before")
  await expect(page.getByTestId("repl-result")).toContainText("3")
  await page.getByTestId("table-option").filter({ hasText: "Lab.V" }).click()
  await expect(page.getByTestId("table-row")).toHaveCount(3)
  await expect.poll(() => handle.doc().scanTable("Lab.V").length).toBe(3)

  handle.change((transaction) => {
    transaction.add("Lab.V", [])
  })
  await repo.flush([handle.documentId])
  repo.resyncSubduction(handle.documentId)
  await expect(page.getByTestId("table-row")).toHaveCount(4)

  await page.reload()
  await expect(page.locator(".cm-content")).toContainText("handle.change")
  await page.getByRole("button", { name: "Switch" }).click()
  await expect(page.getByTestId("document-url-input")).toBeVisible()
  await page.getByTestId("document-url-input").fill(secondHandle.url)
  await page.getByTestId("open-store").click()
  await expect(page.getByTestId("table-option")).toHaveCount(2)
  await page.getByTestId("table-option").filter({ hasText: "Lab.V" }).click()
  await expect(page.getByTestId("table-row")).toHaveCount(1)
  await expect(page.locator(".cm-content")).not.toContainText("handle.change")
})
