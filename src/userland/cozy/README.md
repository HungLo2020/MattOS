# cozy

[![crates.io](https://img.shields.io/crates/v/cozy.svg)](https://crates.io/crates/cozy)
![license](https://img.shields.io/crates/l/cozy.svg)

**English** | [Japanese](README.ja.md)

**A Comfort First TUI — type like nano, navigate like vim.**

![cozy welcome screen](https://raw.githubusercontent.com/takakix2/cozy/main/docs/assets/screenshot.png)

cozy is a small Rust terminal text editor for people who want direct, low-friction editing by default, with optional modal navigation when they need it. It is intended to feel approachable like `nano` while keeping a focused set of vim-like motions in Glide mode.

## Install

**macOS — Homebrew.** This is the recommended way on a mac: Homebrew's prefix is always
on `PATH`, so `cozy` is there the moment it finishes.

```bash
brew install takakix2/tap/cozy
```

**macOS / Linux — prebuilt binary.** Installs to `~/.local/bin`. No Rust toolchain
needed.

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/takakix2/cozy/releases/latest/download/cozy-installer.sh | sh
```

Archives for each platform are attached to every [release](https://github.com/takakix2/cozy/releases)
if you would rather place the binary yourself. Linux binaries need glibc 2.35 or newer.

**With a Rust toolchain.**

```bash
cargo install cozy
```

Or build from source:

```bash
cargo build --release
```

## Usage

```bash
# Open a new buffer
cozy

# Open a file
cozy <file>

# Browse a directory
cozy <folder>
```

## Highlights

- Direct text editing by default: type, save, and exit without learning a modal workflow first
- File open/save/save-as, unsaved-change prompts, and folder browsing
- Search and replace with literal, case-sensitive, word-boundary, and regex modes
- Undo/redo, line cut, clipboard paste, line numbers, line wrap, and goto-line
- Tree-sitter syntax highlighting for Rust, Python, JavaScript, TypeScript, Go, JSON, TOML, and Markdown
- Markdown preview powered by `ratatui-markdown`, including Mermaid diagram blocks
- Fast reading controls in Markdown preview
- Glide mode for vim-like movement, operators, yanking, changing, deleting, joining, and paste
- TOML configuration and per-action key overrides
- Reducer-based architecture with focused tests for cursor behavior, motions, editing, replace, clipboard, and browse mode

## Screenshots

Edit mode is the default: open a file and type directly, with line numbers and a compact shortcut footer.

![cozy edit mode](https://raw.githubusercontent.com/takakix2/cozy/main/docs/assets/edit-mode.png)

Glide mode adds vim-like movement and editing commands while keeping the visible footer focused on the active mode.

![cozy glide mode](https://raw.githubusercontent.com/takakix2/cozy/main/docs/assets/glide-mode.png)

Markdown preview renders the current document with readable wrapping, code block formatting, and Mermaid diagram blocks.

![cozy markdown preview with Mermaid diagrams](https://raw.githubusercontent.com/takakix2/cozy/main/docs/assets/markdown-preview-current.png)

## Editor Modes

- **Edit**: Default mode. Type directly, like `nano`.
- **Glide**: Vim-like modal navigation and editing (`Ctrl+G`).
- **Search**: Incremental search (`Ctrl+F`).
- **Replace**: Search and replace (`Ctrl+R`).
- **Goto**: Jump to a line number (`Ctrl+J`).
- **Save**: Save dialog (`Ctrl+S`).
- **Open**: Open-file dialog (`Ctrl+O`).
- **Browse**: Full-screen file tree (`Ctrl+B`, or `F3` inside tmux where `Ctrl+B` is the prefix).
- **Command**: Command palette (`Ctrl+P`).
- **Markdown**: Markdown reading mode powered by `ratatui-markdown` (`F2` or `Ctrl+D`).
- **Help**: Help screen (`Ctrl+H` or `F1`).

## Key Bindings

The source of truth for key bindings is `src/shortcuts.rs` (`get_shortcuts()`). Defaults are shown below. Per-action overrides are available through the `[keys]` section of `config.toml`.

### File

- `Ctrl+S`: Save
- `Ctrl+Shift+S`: Save as
- `Ctrl+O`: Open file
- `Ctrl+B` / `F3`: Browse files (`F3` is a tmux-safe fallback; tmux uses `Ctrl+B` as its prefix)
- `Ctrl+X`: Exit, prompting to save when modified
- `Ctrl+Q`: Quit immediately without saving

### Navigation

- `Up` / `Down` / `Left` / `Right`: Move cursor
- `Ctrl+A`: Line start
- `Ctrl+E`: Line end
- `PageUp` / `PageDown`: Scroll page
- `Ctrl+J`: Jump to line number
- `Ctrl+G`: Enter Glide mode

### Editing

- `Enter`: Insert newline
- `Backspace` / `Delete`: Delete before or at cursor
- `Ctrl+K`: Cut current line
- `Ctrl+V`: Paste from system clipboard
- `Ctrl+Z`: Undo
- `Ctrl+Y`: Redo

### Search And Replace

- `Ctrl+F`: Search
- `Ctrl+N`: Next match
- `Ctrl+P` in Search/Replace: Previous match
- `Ctrl+T`: Toggle search options
- `Ctrl+R`: Replace mode; press again to replace all
- `Tab` in replace mode: Switch between query and replacement fields
- `Enter` in replace mode: Replace current match

### View And Help

- `Ctrl+H` / `F1`: Help
- `Ctrl+L`: Toggle line numbers
- `Ctrl+W`: Toggle line wrap
- `Ctrl+U`: Toggle shortcut footer visibility
- `F2` / `Ctrl+D`: Toggle Markdown preview
- `Esc` / `Ctrl+[`: Cancel current operation or leave the current mode

### Command Palette

- `Ctrl+P`: Open Command mode
- Type to filter commands
- `Up` / `Down` or `j` / `k`: Select a command
- `Tab`: Complete the common label prefix
- `Enter`: Run the selected command
- `Esc`: Return to the home mode

Built-in commands are currently grouped as:

- `Mode.*`: sustained editing and navigation modes
- `Search.Find`
- `Search.Replace`
- `File.SaveAs`
- `File.Open`
- `Browse.Files`
- `Navigate.GotoLine`
- `View.Markdown`
- `View.ToggleLineNumbers`
- `View.ToggleWrap`
- `View.ToggleFooter`
- `Config.Open`
- `Config.Reload`
- `App.Quit`
- `App.QuitWithoutSaving`

## Markdown Preview

Markdown preview is available with `F2` or `Ctrl+D`. It is a read-only view for quickly reading README files, plans, notes, and other Markdown documents. cozy now uses `ratatui-markdown` for the rendered preview, so headings, lists, block quotes, inline code, wrapped paragraphs, fenced code blocks, and Mermaid diagram blocks follow the renderer's output instead of the old hand-written formatter.

- Move: `j`/`k` or `Up`/`Down`
- Page: `PageUp` / `PageDown`
- Jump: `gg`/`G`, `Ngg`/`NG`
- Screen: `H`/`M`/`L` for top/middle/bottom of the visible area
- Counted move: `5j`, `5k`, `5gg`, `5G`
- `Esc`: Return to your configured home mode

## Glide Mode

Glide mode is available with `Ctrl+G`. A leading number repeats a motion or linewise operation.

- Move: `h` `j` `k` `l`, `w`/`b`/`e`, `W`/`B`/`E`, `0`/`^`/`$`, `gg`/`G`, `H`/`M`/`L`
- Find/till: `>`/`<`, `t`/`T`, then `.`/`,` to repeat the last jump forward/back
- Operators: `d`/`c`/`y` plus a motion, such as `dw`, `de`, `d$`, `dj`, `cw`, `yw`, `d3w`
- Linewise: `dd`/`cc`/`yy`, with counts such as `3dd`
- Edit: `x`, `X`, `~`, `J`
- Paste: `p`/`P`
- Insert: `i`/`I`, `a`/`A`, `o`/`O`
- `Esc`: Return to Edit mode

## Configuration

The repository ships `config.example.toml` as a template. Copy it to `config.toml` if you want a local override in the project root.

Configuration is loaded from the first path found:

1. `./config.toml`
2. `~/.config/cozy/config.toml`
3. `~/.cozy/config.toml`

Example:

```toml
page_size = 20
theme = "dark"
show_line_numbers = true
status_duration = 3
line_number_bg = "darkgray"
line_number_fg = "white"

# Footer and status bar colors accept color names or #RRGGBB true color values.
footer_bg = "#222226"
footer_key_fg = "cyan"
footer_fg = "gray"
status_bar_bg = "darkgray"
status_bar_fg = "white"

cursor_blink = true

# Which mode you rest in: "edit" (default, type like nano) or "glide"
# (navigate like vim). This is your home — every action returns here, not
# just startup. Newcomers stay in "edit" (zero hidden state); vim users can
# opt into "glide" and enter Edit with i/a/o.
default_mode = "edit"
```

Key bindings can be overridden by action name:

```toml
[keys]
enter_browse = "ctrl+b"
enter_glide = "ctrl+g"
enter_help = "f1"
toggle_markdown = "f2"
toggle_footer = "ctrl+u"
```

## Architecture

cozy uses a Redux-inspired core with thin host adapters:

```text
Host (CLI / embedded)
  -> EventSource + input mapping
  -> Keymap
  -> Action
  -> Reducer
  -> EditorState
  -> UI render

File / config / clipboard / runtime IO stay behind small adapter modules.
```

The main editor state lives in `EditorState`, text is stored in a line-based `TextBuffer`, and editor behavior is implemented through reducers. CLI terminal setup, event sources, file/config loading, clipboard access, and startup runtime are kept at the host/IO boundary so the core editor behavior can be tested directly and embedded by hosts such as hsh-ios.

## Development

```bash
cargo test
cargo fmt
```

The current test suite covers editor reducers, cursor movement, word and screen motions, replace behavior, clipboard/register handling, and browse mode behavior.

## License

Licensed under either of:

- MIT License ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
