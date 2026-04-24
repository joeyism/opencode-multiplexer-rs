# ocmux

A terminal multiplexer for managing [OpenCode](https://opencode.ai) sessions.

<p align="center" width="100%">
<video src="https://github.com/user-attachments/assets/93669f30-bb6c-4a71-935d-29541ee073ad" width="80%" controls></video>
</p>

## Install

**Cargo:**

```
cargo install ocmux-rs
```

**Homebrew:**

```
brew tap joeyism/ocmux
brew install ocmux
```

**Shell (macOS / Linux):**

```
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/joeyism/ocmux-rs/releases/latest/download/ocmux-rs-installer.sh | sh
```

## Usage

Run `ocmux` in your terminal. The sidebar lists active OpenCode sessions sorted by most recently updated. The main pane shows the attached session's terminal output.

- Press `Enter` to attach to a top-level session
- Press `Tab` to expand/collapse child sessions
- Press `v` to open a read-only conversation view (see below)
- Press `s` to collapse the sidebar for more screen space
- Click a sidebar row to select it

## Keybindings

### Sidebar navigation

| Key      | Action                    |
|----------|---------------------------|
| `j` / `Down` | Move down             |
| `k` / `Up`   | Move up               |
| `Enter`  | Attach to session        |
| `Tab`    | Expand/collapse children |
| `s`      | Toggle sidebar collapse  |
| `Ctrl-h` | Hide/show sidebar panel  |
| `/`      | Search and attach session |
| `r`      | Refresh active session   |
| `?`      | Show help overlay        |
| `q`      | Quit (confirm with `y`) |
| `Ctrl-4` | Toggle focus sidebar/main |

### Session actions

| Key | Action                                |
|-----|---------------------------------------|
| `n` | Spawn new session                     |
| `t` | Create git worktree + spawn           |
| `v` | Open read-only conversation view       |
| `d` | Open diff view for session             |
| `f` | Show files modified by session        |
| `!` | Drop into shell in session directory   |
| `c` | Commit/push modified files            |
| `x` | Kill session (`y` confirm, `n`/`Esc` cancel) |

### Conversation view

Press `v` from the sidebar to open a read-only view of the session's conversation history. The view polls the opencode database and renders messages, markdown, syntax-highlighted code blocks, and tool call status.

| Key      | Action                    |
|----------|---------------------------|
| `j` / `Down` | Scroll down           |
| `k` / `Up`   | Scroll up             |
| `G`      | Jump to end               |
| `g`      | Jump to top               |
| `Ctrl-u` | Page up                   |
| `Ctrl-d` | Page down                 |
| `/`      | Search conversation       |
| `n`      | Next search match         |
| `N`      | Previous search match     |
| `q` / `v` / `Esc` | Close view       |

Mouse scroll is supported in the conversation view. Search is incremental — type to filter and `Enter` to confirm.

### Diff view

Press `d` from the sidebar to open a read-only view of the session's git diff. The view shows both tracked and untracked changes, preferring the opencode serve API when available and falling back to `git diff` otherwise.

| Key      | Action                    |
|----------|---------------------------|
| `j` / `Down` | Scroll down           |
| `k` / `Up`   | Scroll up             |
| `G`      | Jump to end               |
| `g`      | Jump to top               |
| `Ctrl-u` | Page up                   |
| `Ctrl-d` | Page down                 |
| `/`      | Search diff               |
| `n`      | Next search match         |
| `N`      | Previous search match     |
| `q` / `d` / `Esc` | Close view       |

Mouse scroll is supported in the diff view. Search is incremental — type to filter and `Enter` to confirm.

## Advanced workflows

- **Inspect conversation output** — press `v` on any session (including child sessions) to watch the agent's progress in real-time without attaching to the PTY
- **Inspect changed files** — press `d` to open a diff view of all changes made by a session (tracked and untracked)
- **Inspect changed files (list)** — press `f` to see which files a session has created or modified
- **Drop into a shell** — press `!` to open a shell in the selected session's working directory
- **Commit session changes** — press `c` to review and commit/push all files modified by the session
- **Search and attach** — press `/` to search across all opencode sessions and attach to one

## Configuration

Keybindings and sidebar width are configurable via `~/.config/ocmux/config.json`:

```json
{
  "sidebar_width": 30,
  "keybindings": {
    "up": "k",
    "down": "j",
    "spawn": "n",
    "kill": "x",
    "help": "?",
    "worktree": "t",
    "quit": "q",
    "view": "v",
    "files": "f",
    "diff": "d"
  }
}
```

Keybinding values are single characters. Default `sidebar_width` is `30`. Only the keys listed above are configurable; other bindings (`Enter`, `Tab`, `/`, `s`, `Ctrl-h`, `r`, `!`, `c`, `Ctrl-4`, arrows) are fixed.

## Notes

- `Ctrl-4` is the actual focus toggle binding (hold `Ctrl` and press `4`)
- `s` collapses the sidebar to a narrow width; `Ctrl-h` hides it entirely for maximum terminal space
- `q` prompts for confirmation before quitting (`y` confirm, `n`/`Esc` cancel)
- Child sessions are expandable and selectable in the sidebar, but `Enter` attach is not yet supported for child rows — use `v` to view their conversation instead
- `c` and `!` operate on top-level sessions only
- `c` prompts for a commit message and then commits and pushes immediately
- `r` refreshes the currently active session's PTY, not the selected sidebar row

## License

Apache-2.0
