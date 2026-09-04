// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

export interface DisplayValue {
  compact: string
  full: string
  kind: "int" | "string" | "ref" | "unknown"
}

type JsonObject = Record<string, unknown>

export function displayValue(value: unknown): DisplayValue {
  if (!isObject(value) || typeof value.tag !== "string") {
    return unknownValue(value)
  }
  if (value.tag === "int" && typeof value.value === "number") {
    const rendered = String(value.value)
    return { compact: rendered, full: rendered, kind: "int" }
  }
  if (value.tag === "string" && typeof value.value === "string") {
    return { compact: value.value, full: value.value, kind: "string" }
  }
  if (value.tag === "row_id") return displayRowRef(value.value)
  return unknownValue(value)
}

export function displayRowRef(value: unknown): DisplayValue {
  if (!isObject(value)) return unknownValue(value)
  if (isObject(value.existing)) {
    const commit = value.existing.commit
    const counter = value.existing.counter
    if (typeof commit === "string" && typeof counter === "number") {
      const full = `${commit}:${counter}`
      const compact = `${commit.slice(0, 8)}…${commit.slice(-4)}:${counter}`
      return { compact, full, kind: "ref" }
    }
  }
  if (isObject(value.pending)) {
    const txId = value.pending.txId
    const counter = value.pending.counter
    if (typeof txId === "number" && typeof counter === "number") {
      const rendered = `pending:${txId}:${counter}`
      return { compact: rendered, full: rendered, kind: "ref" }
    }
  }
  return unknownValue(value)
}

function unknownValue(value: unknown): DisplayValue {
  let rendered: string
  try {
    rendered = JSON.stringify(value) ?? String(value)
  } catch {
    rendered = String(value)
  }
  return { compact: rendered, full: rendered, kind: "unknown" }
}

function isObject(value: unknown): value is JsonObject {
  return value !== null && typeof value === "object" && !Array.isArray(value)
}
