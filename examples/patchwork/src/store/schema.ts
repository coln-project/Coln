// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

export interface StoreColumn {
  name: string
  type: string
  primary: boolean
  referenceTable?: string
}

export interface StoreTable {
  name: string
  columns: StoreColumn[]
}

type JsonObject = Record<string, unknown>
type IrPath = string[][]

export function readTables(json: string): StoreTable[] {
  const schema: unknown = JSON.parse(json)
  if (!isObject(schema) || !Array.isArray(schema.entities)) {
    throw new TypeError("Store IR does not contain an entities array")
  }

  return schema.entities.map((entry, index) => readTable(entry, index))
}

function readTable(entry: unknown, index: number): StoreTable {
  if (!isObject(entry)) throw new TypeError(`Entity ${index} is not an object`)
  const path = readPath(entry.path, `Entity ${index} path`)
  const definition = entry.value
  if (!isObject(definition)) {
    throw new TypeError(`Entity ${pathName(path)} has no definition`)
  }
  const variant = definition.entityVariant
  if (!isObject(variant) || variant.tag !== "table") {
    throw new TypeError(`Entity ${pathName(path)} is not a table`)
  }
  if (!Array.isArray(definition.columns)) {
    throw new TypeError(`Table ${pathName(path)} has no columns array`)
  }

  const primaryKey = definition.primaryKey
  const primaryPaths = Array.isArray(primaryKey)
    ? primaryKey.map((value, column) =>
        pathName(readPath(value, `Primary key ${column}`)),
      )
    : []

  return {
    name: pathName(path),
    columns: definition.columns.map((column, columnIndex) => {
      if (!isObject(column)) {
        throw new TypeError(
          `Column ${columnIndex} in ${pathName(path)} is invalid`,
        )
      }
      const columnPath = readPath(
        column.path,
        `Column ${columnIndex} in ${pathName(path)}`,
      )
      const name = pathName(columnPath)
      const referenceTable = referenceTableName(column.type)
      return {
        name,
        type: typeName(column.type),
        primary: primaryPaths.includes(name),
        ...(referenceTable ? { referenceTable } : {}),
      }
    }),
  }
}

function readPath(value: unknown, label: string): IrPath {
  if (
    !Array.isArray(value) ||
    value.length === 0 ||
    value.some(
      (segment) =>
        !Array.isArray(segment) ||
        segment.length === 0 ||
        segment.some((part) => typeof part !== "string" || part === ""),
    )
  ) {
    throw new TypeError(`${label} is invalid`)
  }
  return value as IrPath
}

function pathName(path: IrPath): string {
  return path.flat().join(".")
}

function typeName(value: unknown): string {
  if (!isObject(value) || typeof value.tag !== "string") return "unknown"
  if (value.tag === "builtin") {
    if (value.type === "builtinInt") return "int"
    if (value.type === "builtinString") return "string"
  }
  if (value.tag === "rowId") {
    try {
      return `ref ${pathName(readPath(value.path, "Row reference path"))}`
    } catch {
      return "ref"
    }
  }
  return value.tag
}

function referenceTableName(value: unknown): string | undefined {
  if (!isObject(value) || value.tag !== "rowId") return undefined
  try {
    return pathName(readPath(value.path, "Row reference path"))
  } catch {
    return undefined
  }
}

function isObject(value: unknown): value is JsonObject {
  return value !== null && typeof value === "object" && !Array.isArray(value)
}
