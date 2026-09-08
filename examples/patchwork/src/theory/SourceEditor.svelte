<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import { untrack } from "svelte"
  import { automergeSyncPlugin } from "@automerge/automerge-codemirror"
  import CodeMirrorEditor from "../lib/CodeMirrorEditor.svelte"
  import type { TheoryHandle } from "./theory-document.ts"

  let { handle }: { handle: TheoryHandle } = $props()

  // automergeSyncPlugin drives the document directly through automerge
  // cursors, so this editor takes no `onchange`: edits land in `doc.source`
  // and come back out through the handle's change event.
  const extensions = automergeSyncPlugin({
    handle: untrack(() => handle) as never,
    path: ["source"],
  })
</script>

<CodeMirrorEditor
  value={handle.doc().source}
  {extensions}
  placeholderText="Define your Coln theory here…"
  ariaLabel="Coln theory source"
  testId="theory-source"
/>
