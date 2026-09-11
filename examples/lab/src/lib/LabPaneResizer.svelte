<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import { PaneResizer } from "paneforge"

  let {
    label,
    orientation,
    disabled = false,
    testId,
  }: {
    label: string
    orientation: "horizontal" | "vertical"
    disabled?: boolean
    testId?: string
  } = $props()
</script>

<PaneResizer class="lab-pane-resizer" aria-label={label} aria-orientation={orientation} {disabled} tabindex={disabled ? -1 : 0} data-testid={testId}>
  <span aria-hidden="true"></span>
</PaneResizer>
