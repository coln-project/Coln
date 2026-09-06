// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

export type Snapshot =
  | null
  | boolean
  | number
  | string
  | Snapshot[]
  | { [key: string]: Snapshot }

export function snapshot(value: unknown, maxDepth = 8): Snapshot {
  try {
    return visit(value, new WeakSet(), 0, maxDepth)
  } catch {
    return tagged("uninspectable")
  }
}

function visit(
  value: unknown,
  seen: WeakSet<object>,
  depth: number,
  maxDepth: number,
): Snapshot {
  if (
    value === null ||
    typeof value === "boolean" ||
    typeof value === "string"
  ) {
    return value
  }
  if (typeof value === "number") {
    return Number.isFinite(value) ? value : tagged("number", String(value))
  }
  if (typeof value === "undefined") return tagged("undefined")
  if (typeof value === "bigint") return tagged("bigint", value.toString())
  if (typeof value === "symbol")
    return tagged("symbol", value.description ?? "")
  if (typeof value === "function") return tagged("function", value.name)
  if (typeof value !== "object") return tagged("unknown", String(value))
  if (seen.has(value)) return tagged("circular")
  if (depth >= maxDepth) return tagged("truncated")

  seen.add(value)
  try {
    if (value instanceof Error) {
      return {
        $type: "error",
        name: value.name,
        message: value.message,
        ...(value.stack ? { stack: value.stack } : {}),
      }
    }
    if (Array.isArray(value)) {
      return value.map((item) => visit(item, seen, depth + 1, maxDepth))
    }

    const prototype = Object.getPrototypeOf(value)
    if (prototype !== Object.prototype && prototype !== null) {
      return tagged("instance", prototype?.constructor?.name ?? "Object")
    }

    return Object.fromEntries(
      Object.entries(Object.getOwnPropertyDescriptors(value)).map(
        ([key, descriptor]) => [
          key,
          "value" in descriptor
            ? visit(descriptor.value, seen, depth + 1, maxDepth)
            : tagged("accessor"),
        ],
      ),
    )
  } finally {
    seen.delete(value)
  }
}

function tagged(type: string, value?: string): { [key: string]: Snapshot } {
  return value === undefined ? { $type: type } : { $type: type, value }
}
