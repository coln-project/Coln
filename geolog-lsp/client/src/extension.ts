import * as fs from "fs";
import * as path from "path";
import { spawn } from "child_process";
import * as vscode from "vscode";
import { LanguageClient, ServerOptions } from "vscode-languageclient/node";

let client: LanguageClient | undefined;

function serverBinaryExists(serverPath: string): boolean {
  try {
    fs.accessSync(serverPath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

const SERVER_PATH_SETTING = "geolog-lsp.server.path";

/** VS Code platform name for bundled server, e.g. x86_64-linux, aarch64-darwin, x86_64-mingw32.
 * Attempts to match build system names, which come from Haskell's `System.Info` module. */
function getServerPlatform(): string {
  const arch = (() => {
    switch (process.arch) {
      case "x64":
        return "x86_64";
      case "arm64":
        return "aarch64";
      default:
        return process.arch;
    }
  })();
  const platform = (() => {
    switch (process.platform) {
      case "win32":
        return "mingw32";
      default:
        return process.platform;
    }
  })();
  return `${arch}-${platform}`;
}

/** Resolve path to geolog-lsp binary: config, then bundled server, then workspace, then extension dir. */
function findServerPath(context: vscode.ExtensionContext): string | null {
  const candidates: string[] = [];

  const dot = SERVER_PATH_SETTING.indexOf(".");
  const configSection = SERVER_PATH_SETTING.slice(0, dot);
  const configKey = SERVER_PATH_SETTING.slice(dot + 1);
  const configPath = vscode.workspace.getConfiguration(configSection).get<string>(configKey);
  if (configPath?.trim()) {
    candidates.push(path.isAbsolute(configPath) ? configPath : path.join(context.extensionPath, configPath));
  }
  // Bundled server (when extension is installed from .vsix)
  candidates.push(path.join(context.extensionPath, "server", getServerPlatform(), "geolog-lsp"));

  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    candidates.push(path.join(folder.uri.fsPath, "target", "debug", "geolog-lsp"));
  }
  // When extension runs from geolog-lsp/client, server is at repo root
  const extRoot = path.join(context.extensionPath, "..");
  candidates.push(path.join(extRoot, "target", "debug", "geolog-lsp"));

  for (const p of candidates) {
    if (serverBinaryExists(p)) return p;
  }
  return null;
}

export function activate(context: vscode.ExtensionContext): void {
  const serverPath = findServerPath(context);

  if (!serverPath) {
    const extRoot = path.join(context.extensionPath, "..");
    const tried = [
      path.join(context.extensionPath, "server", getServerPlatform(), "geolog-lsp"),
      ...(vscode.workspace.workspaceFolders ?? []).map((f: vscode.WorkspaceFolder) => path.join(f.uri.fsPath, "target", "debug", "geolog-lsp")),
      path.join(extRoot, "target", "debug", "geolog-lsp"),
    ];
    vscode.window.showErrorMessage(
      `Geolog LSP: server binary not found. Tried: ${tried.join("; ")}.`
    );
    return;
  }

  const serverOptions: ServerOptions = () =>
    new Promise((resolve, reject) => {
      const child = spawn(serverPath, [], { stdio: ["pipe", "pipe", "pipe"] });

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

      child.stderr?.on("data", (chunk: Buffer | string) => console.error("[geolog-lsp]", chunk.toString()));

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
    "geolog-lsp",
    "Geolog Language Server",
    serverOptions,
    {
      documentSelector: [{ scheme: "file", language: "geolog" }],
    }
  );

  client.start().then(
    () => {
      console.log("geolog-lsp client started");
      // Ensure the editor uses semantic tokens from the LSP (otherwise no highlighting)
      const cfg = vscode.workspace.getConfiguration("editor");
      const current = cfg.get<string | boolean>("semanticHighlighting.enabled");
      if (current !== true) {
        cfg.update("semanticHighlighting.enabled", true, vscode.ConfigurationTarget.Workspace);
      }
    },
    (err) => {
      client = undefined;
      vscode.window.showErrorMessage(
        `Geolog LSP failed to start: ${err.message}.`
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
