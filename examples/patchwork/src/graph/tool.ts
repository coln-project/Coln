// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { ColnPointerDoc } from "../coln/pointer.ts"
import { mountTool } from "../lib/mount.ts"
import { injectTheme } from "../lib/theme.ts"
import type { PatchworkTool } from "../patchwork.ts"
import GraphTool from "./GraphTool.svelte"

const tool: PatchworkTool<ColnPointerDoc> = (handle, element) => {
  injectTheme()
  return mountTool(GraphTool as never, element, { handle })
}

export default tool
