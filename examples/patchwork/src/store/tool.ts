// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { ColnPointerDoc } from "../coln/pointer.ts"
import { mountTool } from "../lib/mount.ts"
import { injectTheme } from "../lib/theme.ts"
import type { PatchworkTool } from "../patchwork.ts"
import StoreTool from "./StoreTool.svelte"

const tool: PatchworkTool<ColnPointerDoc> = (handle, element) => {
  injectTheme()
  return mountTool(StoreTool as never, element, { handle, element })
}

export default tool
