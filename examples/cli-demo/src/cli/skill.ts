// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync } from "node:fs"
import { homedir } from "node:os"
import { join, relative } from "node:path"
import { fileURLToPath } from "node:url"

export const skillName = "coln-repo"

/**
 * The skill directory ships beside `src/` and `dist/`, so the same relative
 * path resolves whether this module runs from source (tsx) or from the build.
 */
export const skillSourceDirectory = fileURLToPath(
  new URL(`../../skills/${skillName}/`, import.meta.url),
)

export const defaultSkillInstallRoot = join(homedir(), ".agents", "skills")

/** Body of SKILL.md with its YAML frontmatter removed. */
export function readGuide(): string {
  const text = readFileSync(join(skillSourceDirectory, "SKILL.md"), "utf8")
  const match = /^---\r?\n[\s\S]*?\r?\n---\r?\n/.exec(text)
  return (match === null ? text : text.slice(match[0].length)).trim()
}

export type InstallStatus = "installed" | "updated" | "unchanged"

export interface InstallResult {
  status: InstallStatus
  source: string
  target: string
  files: string[]
}

/** Copy the skill directory to `<root>/<skillName>`, replacing stale content. */
export function installSkill(root: string): InstallResult {
  const target = join(root, skillName)
  const files = listFiles(skillSourceDirectory)
  const existed = existsSync(target)
  const status: InstallStatus = !existed
    ? "installed"
    : sameContent(skillSourceDirectory, target, files) ? "unchanged" : "updated"

  if (status !== "unchanged") {
    // Replace, don't merge: files dropped from the skill must not linger.
    rmSync(target, { recursive: true, force: true })
    mkdirSync(target, { recursive: true })
    cpSync(skillSourceDirectory, target, { recursive: true, force: true })
  }

  return { status, source: skillSourceDirectory, target, files }
}

function listFiles(directory: string, prefix = directory): string[] {
  const files: string[] = []
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) files.push(...listFiles(path, prefix))
    else files.push(relative(prefix, path))
  }
  return files.sort()
}

function sameContent(source: string, target: string, files: string[]): boolean {
  if (!existsSync(target) || !statSync(target).isDirectory()) return false
  const targetFiles = listFiles(target)
  if (targetFiles.length !== files.length || targetFiles.some((file, index) => file !== files[index])) {
    return false
  }
  return files.every(file => {
    const targetPath = join(target, file)
    if (!existsSync(targetPath) || !statSync(targetPath).isFile()) return false
    return readFileSync(join(source, file)).equals(readFileSync(targetPath))
  })
}
