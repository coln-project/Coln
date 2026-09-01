// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { expect, test, type Page } from "@playwright/test"
import {
  initSubduction,
  Repo,
  type AutomergeUrl,
  type CrdtDocHandle,
} from "@automerge/automerge-repo"
import { colnDocType } from "../src/index.js"
import { itemSchema } from "./fixtures/schema"

const table = "Test.Items"
type RawHandle = CrdtDocHandle<typeof colnDocType>

test("browser and Node repos create, find, and update each other's stores", async ({ browser }) => {
  await initSubduction()
  const context = await browser.newContext()
  const pages: Page[] = []
  let nodeRepo: Repo | undefined

  try {
    nodeRepo = new Repo({
      subductionWebsocketEndpoints: ["ws://127.0.0.1:3031"],
    })
    const browserCreator = await openPage(context.newPage())
    pages.push(browserCreator)
    const browserUrl = await browserCreator.evaluate(
      schema => window.colnTest.create(schema),
      itemSchema,
    )
    await add(browserCreator, "browser-one")
    await browserCreator.evaluate(() => window.colnTest.flush())

    const nodeFinder = await nodeRepo.find(browserUrl, colnDocType)
    await expectNodeRows(nodeFinder, ["browser-one"])

    addInNode(nodeFinder, "node-two")
    await nodeRepo.flush()
    await expectRows(browserCreator, ["browser-one", "node-two"])
    await expectSameHeads(browserCreator, nodeFinder)

    const nodeCreator = nodeRepo.create(itemSchema, colnDocType)
    addInNode(nodeCreator, "node-one")
    await nodeRepo.flush()

    const browserFinder = await openPage(context.newPage())
    pages.push(browserFinder)
    await browserFinder.evaluate(
      url => window.colnTest.find(url),
      nodeCreator.url as AutomergeUrl,
    )
    await expectRows(browserFinder, ["node-one"])

    await add(browserFinder, "browser-two")
    await browserFinder.evaluate(() => window.colnTest.flush())
    await expectNodeRows(nodeCreator, ["browser-two", "node-one"])
    await expectSameHeads(browserFinder, nodeCreator)
  } finally {
    try {
      await shutdownPages(pages)
    } finally {
      try {
        await nodeRepo?.shutdown()
      } finally {
        await context.close()
      }
    }
  }
})

test("reloads without schema and applies typed bindings", async ({ browser }) => {
  const context = await browser.newContext()
  const pages: Page[] = []

  try {
    const creator = await openPage(context.newPage())
    pages.push(creator)
    const url = await creator.evaluate(() => window.colnTest.createTyped())

    await creator.evaluate(() => window.colnTest.addTyped("one"))
    await creator.evaluate(() => window.colnTest.flush())

    const finder = await openPage(context.newPage())
    pages.push(finder)
    await finder.evaluate(url => window.colnTest.find(url), url)
    await expectRows(finder, ["one"])
    await finder.evaluate(() => window.colnTest.applyTypedBindings())
    await expectTypedCount(finder, "one", 1)

    await finder.evaluate(() => window.colnTest.addTyped("two"))
    await finder.evaluate(() => window.colnTest.flush())
    await expectTypedCount(creator, "two", 1)

    await shutdownPages([creator, finder])
    await creator.close()
    await finder.close()

    const reloaded = await openPage(context.newPage())
    pages.push(reloaded)
    await reloaded.evaluate(url => window.colnTest.find(url), url)
    await expectRows(reloaded, ["one", "two"])
    await reloaded.evaluate(() => window.colnTest.applyTypedBindings())
    await expectTypedCount(reloaded, "one", 1)
    await expectTypedCount(reloaded, "two", 1)
  } finally {
    try {
      await shutdownPages(pages)
    } finally {
      await context.close()
    }
  }
})

async function openPage(pagePromise: Promise<Page>): Promise<Page> {
  const page = await pagePromise
  await page.goto("/")
  await page.waitForFunction(() => window.colnTest !== undefined)
  return page
}

async function shutdownPages(pages: Page[]): Promise<void> {
  await Promise.all(
    pages
      .filter(page => !page.isClosed())
      .map(page => page.evaluate(() => window.colnTest.shutdown()).catch(() => undefined)),
  )
}

async function add(page: Page, value: string) {
  await page.evaluate(
    ({ table, value }) => window.colnTest.add(table, [{ tag: "string", value }]),
    { table, value },
  )
}

async function expectRows(page: Page, values: string[]) {
  await expect
    .poll(() =>
      page.evaluate(
        table =>
          window.colnTest
            .rows(table)
            .map(row => row.values[0])
            .filter(value => value?.tag === "string")
            .map(value => value.value)
            .sort(),
        table,
      ),
    )
    .toEqual([...values].sort())
}

function addInNode(handle: RawHandle, value: string) {
  handle.change(transaction => transaction.add(table, [{ tag: "string", value }]))
}

async function expectNodeRows(handle: RawHandle, values: string[]) {
  await expect
    .poll(() =>
      handle
        .doc()
        .scanTable(table)
        .map(row => row.values[0])
        .filter(value => value?.tag === "string")
        .map(value => value.value)
        .sort(),
    )
    .toEqual([...values].sort())
}

async function expectSameHeads(page: Page, handle: RawHandle) {
  await expect
    .poll(async () => {
      const browserHeads = (await page.evaluate(() => window.colnTest.heads())).sort()
      const nodeHeads = handle.doc().heads().sort()
      return browserHeads.join(",") === nodeHeads.join(",")
        ? "equal"
        : `browser: ${browserHeads.join(",")}; Node: ${nodeHeads.join(",")}`
    })
    .toBe("equal")
}

async function expectTypedCount(page: Page, value: string, count: number) {
  await expect
    .poll(() => page.evaluate(value => window.colnTest.typedCount(value), value))
    .toBe(count)
}
