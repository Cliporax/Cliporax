# Cliporax CLI Usage Guide

`cliporax-cli` is a CopyQ-like command-line tool for accessing Cliporax clipboard history directly from a terminal.

## Quick Start

### Build The CLI

```bash
# Development build
npm run cli:build

# Release build with optimizations
npm run cli:build:release
```

### Install To A System Path

```bash
# Copy to /usr/local/bin, recommended
sudo cp src-tauri/target/release/cliporax-cli /usr/local/bin/cliporax-cli

# Or copy to /usr/bin
sudo cp src-tauri/target/release/cliporax-cli /usr/bin/cliporax-cli
```

The desktop DEB/RPM package installs the CLI as `cliporax-cli`. The standalone
installer uses the same name to avoid colliding with the `cliporax` desktop
executable:

```bash
cliporax-cli get latest
cliporax-cli list
cliporax-cli search "keyword"
cliporax-cli copy "text"
cliporax-cli save "text"
```

The installer also enables command and option completion for Bash and Zsh. Restart
your shell after installation, then press Tab after `cliporax-cli` or any
subcommand. Pressing Tab after `cliporax-cli get ` queries the local database and
shows the 10 most recent items with content previews. Selecting an item completes
its ID. If the database is unavailable, completion falls back without printing an
error.

### Enable Shell Completion Manually

Generate and load a completion script without running the installer:

```bash
# Bash (current session)
source <(cliporax-cli completion bash)

# Zsh (current session)
source <(cliporax-cli completion zsh)
```

To enable completion permanently, install the generated script in your shell's
completion directory:

```bash
# Bash
cliporax-cli completion bash | sudo tee \
  /usr/local/share/bash-completion/completions/cliporax-cli >/dev/null

# Zsh
cliporax-cli completion zsh | sudo tee \
  /usr/local/share/zsh/site-functions/_cliporax >/dev/null
```

## Command Reference

### 1. Get Clipboard Content

#### Get By ID

```bash
# Get content with ID 123
npm run cli -- get 123

# Print only the content, without metadata
npm run cli -- get 123 --raw

# Get the item and copy it to the system clipboard
npm run cli -- get 123 --copy
```

#### Get The Latest Item

```bash
# Get the latest record
npm run cli -- get latest

# Get the latest record and copy it to the system clipboard
npm run cli -- get latest --copy
```

#### Get By Index

```bash
# Get the first item, index 0
npm run cli -- get --index 0

# Get the fifth item, index 4
npm run cli -- get --index 4

# Select a tab, default tab is 1
npm run cli -- get --index 0 --tab 2
```

### 2. List Items

```bash
# List the latest 10 items, the default
npm run cli -- list

# List the latest 20 items
npm run cli -- list --limit 20

# Show full content without truncation
npm run cli -- list --full

# List items from a specific tab
npm run cli -- list --tab 2
```

### 3. Search Content

```bash
# Search for content containing "meeting"
npm run cli -- search "meeting"

# Search and limit the number of results
npm run cli -- search "code" --limit 5

# Search inside a specific tab
npm run cli -- search "email" --tab 2
```

### 4. Copy To The System Clipboard

```bash
# Copy text to the system clipboard
npm run cli -- copy "Hello World"

# Read content from a file and copy it
npm run cli -- copy --file ./myfile.txt

# Copy an image to the clipboard on Linux/macOS
npm run cli -- copy --image ./screenshot.png

# Copy text and save it to history at the same time
npm run cli -- copy "Important text" --save
```

### 5. Save To History

```bash
# Save text to history
npm run cli -- save "Note to self"

# Read from a file and save it
npm run cli -- save --file ./notes.txt

# Save an image to history
npm run cli -- save --image ./photo.png

# Save text and copy it to the system clipboard at the same time
npm run cli -- save "Copy this" --copy
```

## Practical Workflows

### Workflow 1: Quickly Read The Latest Clipboard Content

```bash
# Print the latest clipboard content directly in the terminal
npm run cli -- get latest --raw
```

### Workflow 2: Script Automation

```bash
#!/bin/bash
# Save the latest code snippet to a file
npm run cli -- get latest --raw > ~/latest_snippet.txt
```

### Workflow 3: Pipeline Usage

```bash
# Search for content containing "deploy" and copy the first match
npm run cli -- search "deploy" --limit 1 | head -n 1 | npm run cli -- get --copy
```

### Workflow 4: Interactive Selection With fzf

```bash
# List recent items, select one with fzf, then copy it
npm run cli -- list --limit 50 --raw | fzf | xclip -selection clipboard
```

## Command Options

### `get`

| Option        | Description                         | Default                         |
| ------------- | ----------------------------------- | ------------------------------- |
| `[ID]`        | Item ID or `latest`                 | Required unless `--index` is set |
| `-i, --index` | Zero-based item index               | -                               |
| `-t, --tab`   | Tab ID                              | 1                               |
| `-c, --copy`  | Copy to the system clipboard        | false                           |
| `-r, --raw`   | Print only content                  | false                           |

### `list`

| Option        | Description                         | Default |
| ------------- | ----------------------------------- | ------- |
| `-l, --limit` | Number of items to display          | 10      |
| `-t, --tab`   | Tab ID                              | 1       |
| `-f, --full`  | Show full content                   | false   |

### `search`

| Option        | Description                         | Default  |
| ------------- | ----------------------------------- | -------- |
| `<query>`     | Search query                        | Required |
| `-t, --tab`   | Tab ID                              | 1        |
| `-l, --limit` | Number of results                   | 20       |

## Database Paths

The CLI automatically searches for the database in these locations:

- **Linux**: `~/.local/share/cliporax/cliporax.db`
- **Linux fallback**: `~/.local/share/com.cliporax.app/cliporax.db`
- **macOS**: `~/Library/Application Support/com.cliporax.app/cliporax.db`
- **Windows**: `%APPDATA%\com.cliporax.app\cliporax.db`

## Advanced Usage

### 1. Monitor The Latest Item With watch

```bash
# Refresh every 2 seconds and show the latest clipboard content
watch -n 2 "npm run cli -- get latest --raw"
```

### 2. Export History

```bash
# Export as text output
npm run cli -- list --limit 1000 --full > clipboard_history.txt
```

### 3. Count Records

```bash
# Count history records in CLI output
npm run cli -- list --limit 10000 | grep "^ID:" | wc -l
```

## Troubleshooting

### Problem: "Database not found"

**Solution**: Run the Cliporax desktop app at least once so it can create the database file.

### Problem: Non-ASCII Search Returns No Results

**Solution**: SQLite `LIKE` queries support UTF-8 text, but confirm the database and terminal encoding are both UTF-8.

### Problem: xclip Not Found On Linux

**Solution**:

```bash
sudo apt install xclip
```

## CopyQ Command Comparison

| CopyQ command     | Cliporax CLI                   | Description              |
| ----------------- | ------------------------------ | ------------------------ |
| `copyq read 0`    | `cliporax get --index 0`       | Read by index            |
| `copyq read 0 --` | `cliporax get --index 0 --raw` | Plain text output        |
| `copyq separator` | `cliporax list`                | List items               |
| `copyq find text` | `cliporax search "text"`       | Search content           |
| `copyq copy 0`    | `cliporax get 0 --copy`        | Copy to clipboard        |

## Development Notes

### Rebuild

```bash
npm run cli:build
```

### Show Help

```bash
npm run cli -- --help
npm run cli -- get --help
npm run cli -- list --help
npm run cli -- search --help
```

### Debug Mode

```bash
cd src-tauri
cargo run --bin cliporax-cli -- get latest --raw
```
