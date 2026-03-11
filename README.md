# geolog-lsp

Language Server for Geolog, currently providing **syntax highlighting** (semantic tokens)
for `.glog` and `geolog` files. The lexer mirrors `geolog-lang` (Haskell); the
language is experimental and syntax may change.

## How to use

Currently I believe we have no CI, so you would have to run `./script/package.sh`
to get a .vsix file in client/ which you can then install into your vscode/cursor.

Let me know if you having trouble with this!

## Build

```bash
cargo build
```

## Package as a single extension (.vsix)

To build a VS Code/Cursor installable extension (Linux only for now):

1. From the repo root, run:
   ```bash
   bash scripts/package.sh
   ```
   This will: build the Rust server in release mode, copy it to `client/server/linux-x64/`, compile the client, and run `vsce package`. The `.vsix` file is created in `client/`.

2. Install the extension: in VS Code/Cursor, open the Extensions view, click the `...` menu, choose **Install from VSIX...**, and select `client/geolog-lsp-client-0.1.0.vsix` (or the generated filename).

Requires: Rust, Node/npm, and `@vscode/vsce` (the script uses `npx @vscode/vsce package`, so no global install needed).

## Testing

### Run in Cursor / VS Code (end-to-end)

1. **Build the LSP server:**
   ```bash
   cargo build
   ```

2. **Build the client extension:**
   ```bash
   cd client && npm install && npm run compile && cd ..
   ```

3. **Launch the extension:**  
   Open this repo in Cursor (or VS Code), then press **F5** or use **Run → Start Debugging** and choose **"Run Geolog LSP Extension"**. A new window opens with the extension loaded.

4. **Open a Geolog file:**  
   In that window, open `example.glog` (or any `.glog` file). The language server will start and you should get **semantic highlighting** (keywords like `theory`, `Query`, symbols, etc.) if the editor supports semantic tokens.

   **If your workspace is not the geolog-lsp repo** (e.g. you work in `geolog-lang`), the extension looks for the server in every workspace folder and in the extension’s parent directory. If it still can’t find the binary, set **Geolog LSP: Server Path** in settings to the full path to the `geolog-lsp` binary (e.g. `/home/you/proj/geolog-lsp/target/debug/geolog-lsp`).

### 1. Unit tests (lexer + logic)

```bash
cargo test
```

Runs all tests in `src/lexer.rs` (and any other `#[test]` in the crate).

### 2. Other editors

**Neovim (with nvim-lspconfig):**  
   Add a config for `geolog` that runs the binary, e.g.:

   ```lua
   require'lspconfig'.geolog.setup {
     cmd = { '/path/to/geolog-lsp/target/debug/geolog-lsp' },
     filetypes = { 'glog' },
   }
   ```

   Ensure `.glog` is recognized (e.g. `vim.bo.filetype = 'glog'` or an ftplugin).

### 3. Quick LSP sanity check (optional)

From the project root:

```bash
# Start the server in the background
./target/debug/geolog-lsp &
PID=$!

# Send an LSP initialize request (Content-Length + JSON-RPC)
printf 'Content-Length: 85\r\n\r\n{"jsonrpc":"2.0","id":1,"method":"initialize","params":{},"method":"initialize"}\n' | head -1
# Or use a small script to send proper Content-Length + body

kill $PID 2>/dev/null
```

For a more reliable check, use a small script (e.g. Python or Node) that writes the correct `Content-Length: N\r\n\r\n` header plus the JSON-RPC body for `initialize` and optionally `textDocument/semanticTokens/full`, then parses the response.
