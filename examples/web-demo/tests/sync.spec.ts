import { expect, test, type Page } from "@playwright/test"

test("a tab opened after edits loads the coln graph", async ({ browser }) => {
  const context = await browser.newContext()
  const alice = await context.newPage()

  await alice.goto("/")
  await expect(alice.getByTestId("doc-url")).toHaveValue(/^automerge:/)
  await expect(alice.getByTestId("graph-item")).toHaveCount(1)

  await addTwoVerticesAndEdge(alice)

  const url = await alice.getByTestId("doc-url").inputValue()
  const bob = await context.newPage()
  await bob.goto(`/#${url}`)

  await expect(bob.getByTestId("doc-url")).toHaveValue(url)
  await expect(bob.getByTestId("graph-item")).toHaveCount(1)
  await expect(bob.getByTestId("vertex-item")).toHaveCount(2)
  await expect(bob.getByTestId("edge-item")).toHaveCount(1)

  await bob.getByTestId("add-vertex").click()
  await expect(bob.getByTestId("vertex-item")).toHaveCount(3)
  await expect(alice.getByTestId("vertex-item")).toHaveCount(3)
})

test("a tab already open on the doc receives later coln edits", async ({ browser }) => {
  const context = await browser.newContext()
  const alice = await context.newPage()

  await alice.goto("/")
  await expect(alice.getByTestId("doc-url")).toHaveValue(/^automerge:/)
  await expect(alice.getByTestId("graph-item")).toHaveCount(1)

  const url = await alice.getByTestId("doc-url").inputValue()
  const bob = await context.newPage()
  await bob.goto(`/#${url}`)
  await expect(bob.getByTestId("doc-url")).toHaveValue(url)
  await expect(bob.getByTestId("graph-item")).toHaveCount(1)

  await addTwoVerticesAndEdge(alice)

  await expect(bob.getByTestId("vertex-item")).toHaveCount(2)
  await expect(bob.getByTestId("edge-item")).toHaveCount(1)
})

async function addTwoVerticesAndEdge(page: Page) {
  await page.getByTestId("add-vertex").click()
  await page.getByTestId("add-vertex").click()
  await expect(page.getByTestId("vertex-item")).toHaveCount(2)

  await selectByIndex(page, "from-select", 0)
  await selectByIndex(page, "to-select", 1)
  await page.getByTestId("add-edge").click()
  await expect(page.getByTestId("edge-item")).toHaveCount(1)
}

async function selectByIndex(page: Page, testId: string, index: number) {
  const values = await page.getByTestId(testId).locator("option").evaluateAll(options =>
    options.map(option => (option as HTMLOptionElement).value)
  )
  expect(values.length).toBeGreaterThan(index)
  await page.getByTestId(testId).selectOption(values[index])
}
