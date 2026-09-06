// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { createSubscriber } from "svelte/reactivity"

export type Tool = "home" | "compiler" | "editor" | "sync" | "not-found"

export interface Route {
  tool: Tool
  documentUrl: string
  theoryUrl: string
  pathname: string
}

const paths: Record<Exclude<Tool, "not-found">, string> = {
  home: "",
  compiler: "compiler/",
  editor: "editor/",
  sync: "sync/",
}

export class Router {
  readonly #subscribe: () => void

  constructor() {
    this.#subscribe = createSubscriber((update) => {
      addEventListener("popstate", update)
      addEventListener("hashchange", update)
      return () => {
        removeEventListener("popstate", update)
        removeEventListener("hashchange", update)
      }
    })
  }

  get current(): Route {
    this.#subscribe()
    return routeFromUrl(new URL(location.href), import.meta.env.BASE_URL)
  }

  href(
    tool: Exclude<Tool, "not-found">,
    documentUrl = "",
    theoryUrl = "",
  ): string {
    const url = new URL(paths[tool], appBaseUrl())
    if (theoryUrl) url.searchParams.set("theory", theoryUrl)
    url.hash = documentUrl
    return `${url.pathname}${url.search}${url.hash}`
  }

  navigate(
    tool: Exclude<Tool, "not-found">,
    documentUrl = "",
    theoryUrl = "",
  ): void {
    history.pushState(null, "", this.href(tool, documentUrl, theoryUrl))
    dispatchEvent(new PopStateEvent("popstate"))
  }

  follow(
    event: MouseEvent,
    tool: Exclude<Tool, "not-found">,
    documentUrl = "",
    theoryUrl = "",
  ): void {
    if (
      event.defaultPrevented ||
      event.button !== 0 ||
      event.metaKey ||
      event.ctrlKey ||
      event.shiftKey ||
      event.altKey
    )
      return
    event.preventDefault()
    this.navigate(tool, documentUrl, theoryUrl)
  }

  replace(
    tool: Exclude<Tool, "not-found">,
    documentUrl = "",
    theoryUrl = "",
  ): void {
    history.replaceState(null, "", this.href(tool, documentUrl, theoryUrl))
    dispatchEvent(new PopStateEvent("popstate"))
  }
}

export function readRoute(): Route {
  return routeFromUrl(new URL(location.href), import.meta.env.BASE_URL)
}

export function routeFromUrl(url: URL, basePath = "/"): Route {
  const base = new URL(basePath, url.origin).pathname
  const relativePath = url.pathname.startsWith(base)
    ? url.pathname.slice(base.length)
    : url.pathname.slice(1)
  const segments = relativePath
    .replace(/^\/+|\/+$/g, "")
    .split("/")
    .filter(Boolean)
  const segment = segments[0] ?? ""
  const tool: Tool =
    segments.length > 1
      ? "not-found"
      : segment === ""
        ? "home"
        : segment === "compiler" || segment === "editor" || segment === "sync"
          ? segment
          : "not-found"
  const theoryUrl = url.searchParams.get("theory") ?? ""
  return {
    tool,
    documentUrl: url.hash.slice(1),
    theoryUrl,
    pathname: url.pathname,
  }
}

function appBaseUrl(): URL {
  return new URL(import.meta.env.BASE_URL, location.origin)
}
