// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import * as path from "path";
import { spawn } from "child_process";
import * as vscode from "vscode";
import { LanguageClient, ServerOptions } from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext): void {
  const colnPath = path.join(context.extensionPath, "coln");

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
