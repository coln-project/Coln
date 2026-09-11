// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { mountTool } from "../lib/mount.ts"
import { injectTheme } from "../lib/theme.ts"
import type { PatchworkTool } from "../patchwork.ts"
import TheoryTool from "./TheoryTool.svelte"
import type { TheoryDocument } from "./theory-document.ts"

const tool: PatchworkTool<TheoryDocument> = (handle, element) => {
  injectTheme()
  return mountTool(TheoryTool as never, element, { handle, element })
}

export default tool
