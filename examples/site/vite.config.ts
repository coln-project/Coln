// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { defineConfig } from "vite"

export default defineConfig({
  base: "./",
  // A tree of static pages, not a single-page app: a URL that doesn't resolve
  // should be a 404, rather than quietly serving the landing page in its place.
  appType: "mpa",
  build: {
    outDir: "../../_build/web",
    // The demos build into subdirectories of this one, in no guaranteed order,
    // so leave anything already there alone.
    emptyOutDir: false,
  },
  // to reach the stylesheet shared with the compiler demo
  server: { fs: { allow: ["../.."] } },
})
