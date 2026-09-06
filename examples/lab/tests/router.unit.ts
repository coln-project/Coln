// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { describe, expect, it } from "vitest"
import { routeFromUrl } from "../src/lib/router.svelte.ts"

describe("Coln Lab routes", () => {
  it("separates tool paths from document hashes", () => {
    expect(
      routeFromUrl(
        new URL(
          "https://lab.example/editor/?theory=automerge%3Atheory#automerge:store",
        ),
      ),
    ).toEqual({
      tool: "editor",
      documentUrl: "automerge:store",
      theoryUrl: "automerge:theory",
      pathname: "/editor/",
    })
  })

  it("supports a static deployment base", () => {
    expect(
      routeFromUrl(
        new URL("https://example.test/lab/sync/#automerge:graph"),
        "/lab/",
      ),
    ).toMatchObject({ tool: "sync", documentUrl: "automerge:graph" })
  })

  it("reports unknown instruments", () => {
    expect(routeFromUrl(new URL("https://lab.example/unknown/"))).toMatchObject(
      {
        tool: "not-found",
      },
    )
  })

  it("rejects extra path segments", () => {
    expect(
      routeFromUrl(new URL("https://lab.example/editor/extra/")),
    ).toMatchObject({
      tool: "not-found",
    })
  })
})
