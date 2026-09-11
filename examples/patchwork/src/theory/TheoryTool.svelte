<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import { onDestroy, onMount, untrack } from "svelte"
  import Feedback from "../lib/Feedback.svelte"
  import JsonViewer from "../lib/JsonViewer.svelte"
  import { DocState } from "../lib/doc.svelte.ts"
  import { openDocument } from "../patchwork.ts"
  import OutputPanel from "./OutputPanel.svelte"
  import SourceEditor from "./SourceEditor.svelte"
  import StoresPanel from "./StoresPanel.svelte"
  import basicGraphTheory from "./graph-example.coln?raw"
  import { parseCompiledRealms, type CompiledRealm } from "./compiled-realms.ts"
  import type { Compilation, Compiler } from "./compiler.ts"
  import { STORE_TYPE } from "../store/datatype.ts"
  import type { StoreRecord, TheoryHandle } from "./theory-document.ts"

  let { handle, element }: { handle: TheoryHandle; element: HTMLElement } =
    $props()

  const emptyCompilation: Compilation = {
    diagnosticsHtml: [],
    prettyIr: [],
    irJson: "",
  }

  const doc = new DocState(untrack(() => handle))

  let compilation = $state<Compilation>(emptyCompilation)
  let status = $state<"loading" | "ready" | "compiling" | "error">("loading")
  let error = $state("")
  let compiler = $state<Compiler>()
  let compiledSource = $state("")
  let realms = $state<CompiledRealm[]>([])
  let selectedRealmName = $state("")
  let creatingStore = $state(false)
  let storeError = $state("")
  let feedback = $state("")
  let compileTimer: ReturnType<typeof setTimeout> | undefined
  let feedbackTimer: ReturnType<typeof setTimeout> | undefined
  let removeDocumentListener: (() => void) | undefined
  let compileVersion = 0
  let destroyed = false

  const theory = $derived(doc.current)
  const sourceLines = $derived(
    theory.source === "" ? 0 : theory.source.split("\n").length,
  )
  const statusLabel = $derived(
    status === "loading"
      ? "loading compiler"
      : status === "compiling"
        ? "compiling theory"
        : status === "error"
          ? "compilation failed"
          : "theory compiled",
  )
  const selectedRealm = $derived(
    realms.find(realm => realm.name === selectedRealmName),
  )
  const stores = $derived([...theory.stores].reverse())
  const canCreateStore = $derived(
    status === "ready" &&
      compilation.diagnosticsHtml.length === 0 &&
      compiledSource === theory.source &&
      selectedRealm !== undefined &&
      !creatingStore,
  )
  const createStoreHint = $derived(
    creatingStore
      ? "Saving the store to Coln's sync server before linking it."
      : status === "compiling" || compiledSource !== theory.source
        ? "Waiting for the current theory source to compile."
        : compilation.diagnosticsHtml.length > 0
          ? "Resolve diagnostics before creating a store."
          : realms.length > 1 && !selectedRealm
            ? "Choose a realm for the new store."
            : "",
  )

  onMount(() => {
    let currentSource = handle.doc().source
    const documentChanged = () => {
      const nextSource = handle.doc().source
      if (nextSource === currentSource) return
      currentSource = nextSource
      sourceChanged(nextSource, 50)
    }
    handle.on("change", documentChanged)
    removeDocumentListener = () => handle.off("change", documentChanged)
    void initializeCompiler()
  })

  onDestroy(() => {
    destroyed = true
    compileVersion += 1
    clearTimeout(compileTimer)
    clearTimeout(feedbackTimer)
    removeDocumentListener?.()
  })

  async function initializeCompiler(): Promise<void> {
    try {
      // Dynamic, so the compiler — the WASI shim, the JSFFI glue and 4.4MB of
      // wasm — is a chunk of its own that nothing else pulls in.
      const { loadCompiler } = await import("./compiler.ts")
      const loadedCompiler = await loadCompiler()
      if (destroyed) return
      compiler = loadedCompiler
      sourceChanged(handle.doc().source, 0)
    } catch (cause) {
      showError(cause)
    }
  }

  function sourceChanged(source: string, delay: number): void {
    clearTimeout(compileTimer)
    const version = ++compileVersion
    error = ""
    storeError = ""
    compiledSource = ""
    realms = []
    if (source === "") {
      compilation = emptyCompilation
      selectedRealmName = ""
      status = compiler ? "ready" : "loading"
      return
    }
    if (!compiler) {
      status = "loading"
      return
    }
    status = "compiling"
    compileTimer = setTimeout(() => void runCompile(version, source), delay)
  }

  async function runCompile(version: number, source: string): Promise<void> {
    if (!compiler) return
    try {
      const nextCompilation = await compiler.compile(source)
      if (destroyed || version !== compileVersion) return
      const nextRealms = parseCompiledRealms(nextCompilation.irJson)
      compilation = nextCompilation
      compiledSource = source
      realms = nextRealms
      selectedRealmName =
        nextRealms.length === 1
          ? nextRealms[0].name
          : nextRealms.some(realm => realm.name === selectedRealmName)
            ? selectedRealmName
            : ""
      status = "ready"
    } catch (cause) {
      if (version === compileVersion) showError(cause)
    }
  }

  function showError(cause: unknown): void {
    if (destroyed) return
    error = cause instanceof Error ? cause.message : String(cause)
    status = "error"
  }

  async function createSelectedStore(): Promise<void> {
    if (!canCreateStore || !selectedRealm) return
    creatingStore = true
    storeError = ""
    try {
      // Imported here rather than at the top: this is the only part of the
      // Theory Editor that needs Coln's repo, so the fork and the store
      // runtime stay unfetched until a theory actually produces a store.
      const { createStore } = await import("./stores.ts")
      const record = await createStore(handle, selectedRealm)
      showFeedback(`Store created for ${record.realmName}`)
    } catch (cause) {
      storeError = cause instanceof Error ? cause.message : String(cause)
    } finally {
      if (!destroyed) creatingStore = false
    }
  }

  /**
   * Open a store as a Patchwork document. Stores created here already have a
   * pointer document; ones recorded by Coln Lab do not, so make one and
   * remember it on the record.
   */
  async function openStore(store: StoreRecord): Promise<void> {
    storeError = ""
    try {
      let docUrl = store.docUrl
      if (!docUrl) {
        const { createPointerDoc } = await import("../coln/pointer.ts")
        docUrl = await createPointerDoc({
          type: STORE_TYPE,
          title: `${store.realmName} store`,
          storeUrl: store.url,
          realmName: store.realmName,
          theoryUrl: handle.url,
        })
        handle.change(document => {
          const record = document.stores.find(entry => entry.url === store.url)
          if (record) record.docUrl = docUrl
        })
      }
      openDocument(element, docUrl, STORE_TYPE)
    } catch (cause) {
      storeError = cause instanceof Error ? cause.message : String(cause)
    }
  }

  function loadBasicGraphTheory(): void {
    if (handle.doc().source !== "") return
    handle.change(document => {
      if (document.source === "") document.source = basicGraphTheory
    })
  }

  async function copyText(value: string, message: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(value)
      showFeedback(message)
    } catch (cause) {
      showError(cause)
    }
  }

  function showFeedback(message: string): void {
    feedback = message
    clearTimeout(feedbackTimer)
    feedbackTimer = setTimeout(() => (feedback = ""), 2_000)
  }
</script>

<section class="coln-tool coln-theory">
  {#if feedback}<Feedback message={feedback} />{/if}

  <div class="panes" data-columns="3">
    <section class="pane">
      <div class="pane-head">
        <div>
          <p class="label">Theory source</p>
          <p class="hint">Source for this Coln theory</p>
        </div>
        <div class="cluster">
          <span class="status" data-state={status} role="status">{statusLabel}</span>
          <span class="mono lines">
            {sourceLines} {sourceLines === 1 ? "line" : "lines"}
          </span>
        </div>
      </div>
      <div class="source">
        <SourceEditor {handle} />
        {#if theory.source === ""}
          <button
            class="action seed"
            type="button"
            onclick={loadBasicGraphTheory}
            data-testid="use-graph-example"
          >Use basic graph example</button>
        {/if}
      </div>
    </section>

    <section class="pane">
      <div
        class="pane-body output"
        aria-busy={status === "compiling"}
        data-busy={status === "compiling" || undefined}
      >
        {#if error}
          <p class="alert" role="alert">Theory Editor error: {error}</p>
        {/if}
        <OutputPanel label="Diagnostics" index="01">
          {#if compilation.diagnosticsHtml.length === 0}
            <p class="empty">No compilation diagnostics</p>
          {:else}
            <div class="coln-diagnostics diagnostics">
              {#each compilation.diagnosticsHtml as diagnostic}
                <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                <div>{@html diagnostic}</div>
              {/each}
            </div>
          {/if}
        </OutputPanel>
        <OutputPanel label="Compiled IR" index="02">
          {#if compilation.prettyIr.length === 0}
            <p class="empty">No compiled intermediate representation</p>
          {:else}
            <div class="ir">
              {#each compilation.prettyIr as realm}<pre>{realm}</pre>{/each}
            </div>
          {/if}
        </OutputPanel>
        <OutputPanel label="IR as JSON" index="03">
          {#if compilation.irJson === ""}
            <p class="empty">No compiled IR JSON</p>
          {:else}
            <JsonViewer value={compilation.irJson} />
          {/if}
        </OutputPanel>
      </div>
    </section>

    <aside class="pane">
      <StoresPanel
        {stores}
        {realms}
        {selectedRealmName}
        canCreate={canCreateStore}
        creating={creatingStore}
        error={storeError}
        hint={createStoreHint}
        onselect={name => (selectedRealmName = name)}
        oncreate={createSelectedStore}
        oncopy={url => void copyText(url, "Store url copied")}
        onopen={store => void openStore(store)}
      />
    </aside>
  </div>
</section>

<style>
  .lines {
    color: var(--coln-faint);
  }

  .source {
    display: flex;
    min-height: 0;
    flex: 1;
    position: relative;
  }

  .seed {
    left: 50%;
    position: absolute;
    top: 50%;
    transform: translate(-50%, -50%);
    z-index: 1;
  }

  .output {
    align-content: start;
    display: grid;
    gap: var(--studio-space-md, 1rem);
    transition: opacity var(--studio-transition-medium, 0.15s ease);
  }

  .output[data-busy] {
    opacity: 0.55;
  }

  .ir {
    display: grid;
    gap: var(--studio-space-sm, 0.75rem);
  }

  .ir pre {
    margin: 0;
    overflow-wrap: break-word;
    white-space: pre-wrap;
  }
</style>
