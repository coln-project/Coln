// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import * as fs from "fs";
import * as path from "path";
import { execFileSync, spawn } from "child_process";
import * as vscode from "vscode";
import { LanguageClient, ServerOptions } from "vscode-languageclient/node";

let client: LanguageClient | undefined;

function binaryExists(binaryPath: string): boolean {
  try {
    fs.accessSync(binaryPath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

/** Try to find a binary on PATH using `which`. */
function whichBinary(name: string): string | null {
  try {
    return execFileSync("which", [name], { encoding: "utf-8" }).trim();
  } catch {
    return null;
  }
}

const COLN_PATH_SETTING = "coln-ls.server.path";

/** Resolve path to the coln binary: config setting, bundled in extension, then PATH. */
function findColnPath(context: vscode.ExtensionContext): string | null {
  const dot = COLN_PATH_SETTING.indexOf(".");
  const configSection = COLN_PATH_SETTING.slice(0, dot);
  const configKey = COLN_PATH_SETTING.slice(dot + 1);
  const configPath = vscode.workspace.getConfiguration(configSection).get<string>(configKey);
  if (configPath?.trim()) {
    const resolved = path.isAbsolute(configPath) ? configPath : path.join(context.extensionPath, configPath);
    if (binaryExists(resolved)) return resolved;
  }

  // Bundled coln binary (when extension is installed from .vsix)
  const bundled = path.join(context.extensionPath, "coln");
  if (binaryExists(bundled)) return bundled;

  // Fall back to coln on PATH
  return whichBinary("coln");
}

export function activate(context: vscode.ExtensionContext): void {
  const colnPath = findColnPath(context);

  if (!colnPath) {
    vscode.window.showErrorMessage(
      "Coln LSP: 'coln' binary not found. Install coln or set the 'coln-ls.server.path' setting."
    );
    return;
  }

  const serverOptions: ServerOptions = () =>
    new Promise((resolve, reject) => {
      const child = spawn(colnPath, ["language-server"], { stdio: ["pipe", "pipe", "pipe"] });

      child.on("error", (err: Error) => {
        reject(err);
      });

      child.on("exit", (code: number | null, signal: string | null) => {
        if (code !== 0 && code !== null) {
          reject(new Error(`Server exited with code ${code}`));
        }
        if (signal) {
          reject(new Error(`Server killed by signal ${signal}`));
        }
      });

      child.stderr?.on("data", (chunk: Buffer | string) => console.error("[coln-ls]", chunk.toString()));

      // Defer resolve so same-tick exit/error is rejected first
      setImmediate(() => {
        if (child.killed || (child.exitCode !== null && child.exitCode !== 0)) {
          reject(new Error("Server process exited before ready"));
          return;
        }
        resolve({ reader: child.stdout!, writer: child.stdin! });
      });
    });

  client = new LanguageClient(
    "coln-ls",
    "Coln Language Server",
    serverOptions,
    {
      documentSelector: [{ scheme: "file", language: "coln" }],
    }
  );

  client.start().then(
    () => {
      console.log("coln-ls client started");
    },
    (err) => {
      client = undefined;
      vscode.window.showErrorMessage(
        `Coln LSP failed to start: ${err.message}.`
      );
    }
  );

  context.subscriptions.push({
    dispose: () => {
      if (!client) return;
      client.stop().catch(() => {
        // Ignore "not running" / "starting" errors on shutdown
      });
    },
  });
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) return undefined;
  return client.stop().catch(() => undefined);
}
