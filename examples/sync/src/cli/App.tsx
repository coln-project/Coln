// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type { Repo } from "@automerge/automerge-repo"
import { Box, Text, useInput, useStdout } from "ink"
import TextInput from "ink-text-input"
import { useEffect, useRef, useState } from "react"
import {
  addEdge,
  addVertex,
  readGraph,
  watchGraph,
  type GraphHandle,
} from "../lib/graph.ts"
import { executeQuery, QueryError } from "../query/execute-query.ts"

type Props = {
  handle: GraphHandle
  repo: Repo
  endpoint: string
  onQuit: () => void
}

type Endpoint = "source" | "target"

export default function App({ handle, repo, endpoint, onQuit }: Props) {
  const { stdout } = useStdout()
  const [graph, setGraph] = useState(() => readGraph(handle.doc()))
  const [sourceIndex, setSourceIndex] = useState(0)
  const [targetIndex, setTargetIndex] = useState(0)
  const [activeEndpoint, setActiveEndpoint] = useState<Endpoint>("source")
  const [queryMode, setQueryMode] = useState(false)
  const [query, setQuery] = useState("")
  const [history, setHistory] = useState<string[]>([])
  const [historyIndex, setHistoryIndex] = useState(0)
  const [message, setMessage] = useState("Ready")
  const [connected, setConnected] = useState(repo.isSubductionConnected())
  const [quitting, setQuitting] = useState(false)
  const [terminalHeight, setTerminalHeight] = useState(stdout.rows || 24)
  const quittingRef = useRef(false)

  useEffect(() => {
    const resize = () => setTerminalHeight(stdout.rows || 24)
    stdout.on("resize", resize)
    return () => {
      stdout.off("resize", resize)
    }
  }, [stdout])
  useEffect(() => watchGraph(handle, setGraph), [handle])
  useEffect(() => {
    const update = ({ connected }: { connected: boolean }) => setConnected(connected)
    repo.on("subduction-connection", update)
    return () => {
      repo.off("subduction-connection", update)
    }
  }, [repo])
  useEffect(() => {
    const last = Math.max(0, graph.vertices.length - 1)
    setSourceIndex(index => Math.min(index, last))
    setTargetIndex(index => Math.min(index, last))
  }, [graph.vertices.length])
  useEffect(() => {
    if (quitting) onQuit()
  }, [onQuit, quitting])

  useInput((input, key) => {
    if (quittingRef.current) return
    if (key.ctrl && input === "c") {
      quit()
      return
    }
    if (queryMode) {
      if (key.escape) {
        setQueryMode(false)
        setMessage("Query cancelled")
      } else if (key.upArrow && history.length > 0) {
        const next = Math.max(0, historyIndex - 1)
        setHistoryIndex(next)
        setQuery(history[next] ?? "")
      } else if (key.downArrow && history.length > 0) {
        const next = Math.min(history.length, historyIndex + 1)
        setHistoryIndex(next)
        setQuery(history[next] ?? "")
      }
      return
    }

    if (input === "q") quit()
    else if (input === "v") run("Vertex added", () => addVertex(handle))
    else if (input === "e") createEdge()
    else if (input === "/") {
      setQueryMode(true)
      setQuery("")
      setHistoryIndex(history.length)
      setMessage("Enter a TypeScript change callback body")
    } else if (key.tab) {
      setActiveEndpoint(value => value === "source" ? "target" : "source")
    } else if (key.upArrow) {
      moveSelection(-1)
    } else if (key.downArrow) {
      moveSelection(1)
    }
  })

  const source = graph.vertices[sourceIndex]
  const target = graph.vertices[targetIndex]

  function quit() {
    if (quittingRef.current) return
    quittingRef.current = true
    setQuitting(true)
  }

  function moveSelection(delta: number) {
    if (graph.vertices.length === 0) return
    const move = (index: number) =>
      (index + delta + graph.vertices.length) % graph.vertices.length
    if (activeEndpoint === "source") setSourceIndex(move)
    else setTargetIndex(move)
  }

  function createEdge() {
    if (!source || !target) {
      setMessage("Add a vertex before drawing an edge")
      return
    }
    run(`${source.label} -> ${target.label} added`, () => addEdge(handle, source, target))
  }

  function submitQuery(source: string) {
    if (!source.trim()) {
      setQueryMode(false)
      return
    }
    try {
      executeQuery(handle, source)
      setHistory(values => [...values, source])
      setHistoryIndex(history.length + 1)
      setMessage("Query committed")
      setQuery("")
      setQueryMode(false)
    } catch (cause) {
      const prefix = cause instanceof QueryError ? cause.phase : "query"
      setMessage(`${prefix}: ${cause instanceof Error ? cause.message : String(cause)}`)
    }
  }

  function run(success: string, action: () => void) {
    try {
      action()
      setMessage(success)
    } catch (cause) {
      setMessage(cause instanceof Error ? cause.message : String(cause))
    }
  }

  const names = new Map(graph.vertices.map(vertex => [vertex.id, vertex.label]))

  return (
    <Box flexDirection="column" paddingX={1} height={terminalHeight} overflow="hidden">
      <Box justifyContent="space-between" borderStyle="single" borderColor="#d8ff57" paddingX={1}>
        <Text bold color="#d8ff57" wrap="truncate-end">COLN / GRAPH CLI</Text>
        <Text color={quitting ? "yellow" : connected ? "green" : "yellow"} wrap="truncate-end">
          {quitting ? "◌ shutting down" : connected ? "● relay online" : "○ connecting"}
        </Text>
      </Box>

      <Box marginTop={1} flexDirection="column">
        <Text dimColor wrap="truncate-end">DOCUMENT</Text>
        <Text wrap="truncate-end">{handle.url}</Text>
        <Text dimColor wrap="truncate-end">{endpoint} · {graph.heads.length} synchronized head{graph.heads.length === 1 ? "" : "s"}</Text>
      </Box>

      <Box gap={2} marginTop={1} flexGrow={1} flexShrink={1} overflow="hidden">
        <Box flexDirection="column" borderStyle="single" borderColor="#526264" paddingX={1} width="42%" overflow="hidden">
          <Text bold>VERTICES  <Text color="#d8ff57">{graph.vertices.length}</Text></Text>
          {graph.vertices.length === 0
            ? <Text dimColor>(empty)</Text>
            : graph.vertices.map((vertex, index) => (
              <Text key={vertex.id}>
                <Text color={index === sourceIndex ? "#ff7657" : index === targetIndex ? "#65d9e7" : undefined}>
                  {index === sourceIndex ? "S" : " "}{index === targetIndex ? "T" : " "} {vertex.label}
                </Text>
                <Text dimColor>  {vertex.id.slice(0, 8)}</Text>
              </Text>
            ))}
        </Box>

        <Box flexDirection="column" borderStyle="single" borderColor="#526264" paddingX={1} flexGrow={1} overflow="hidden">
          <Text bold>EDGES  <Text color="#d8ff57">{graph.edges.length}</Text></Text>
          {graph.edges.length === 0
            ? <Text dimColor>(empty)</Text>
            : graph.edges.map(edge => (
              <Text key={edge.id}>{names.get(edge.fromId)} → {names.get(edge.toId)}</Text>
            ))}
        </Box>
      </Box>

      <Box marginTop={1} flexDirection="column" flexShrink={0}>
        <Text wrap="truncate-end">
          Endpoint: <Text bold color={activeEndpoint === "source" ? "#ff7657" : "#65d9e7"}>{activeEndpoint}</Text>
          {"  "}S: {source?.label ?? "-"} → T: {target?.label ?? "-"}
        </Text>
        {quitting ? (
          <Text color="yellow">Syncing and quitting…</Text>
        ) : queryMode ? (
          <Box borderStyle="single" borderColor="#d8ff57" paddingX={1} height={3} overflow="hidden" flexShrink={0}>
            <Text color="#d8ff57">change(txn) › </Text>
            <TextInput value={query} onChange={setQuery} onSubmit={submitQuery} />
          </Box>
        ) : (
          <Text dimColor wrap="truncate-end">v add vertex · e add edge · tab switch endpoint · ↑/↓ select · / TypeScript · q quit</Text>
        )}
        {!quitting && <Text wrap="truncate-end" color={message.startsWith("compile:") || message.startsWith("runtime:") ? "red" : "gray"}>{message}</Text>}
      </Box>
    </Box>
  )
}
