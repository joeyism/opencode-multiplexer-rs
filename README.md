# ocmux

A terminal multiplexer for managing [OpenCode](https://opencode.ai) sessions.

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

Run `ocmux` in your terminal. The sidebar lists active OpenCode sessions; press Enter to attach.

## Keybindings

| Key     | Action              |
|---------|---------------------|
| `j`     | Move down           |
| `k`     | Move up             |
| `n`     | Spawn new session   |
| `x`     | Kill session        |
| `t`     | Create git worktree |
| `/`     | Search sessions     |
| `s`     | Toggle sidebar      |
| `r`     | Refresh session     |
| `?`     | Show help           |
| `q`     | Quit                |
| `Ctrl-4`| Toggle focus        |
| `Tab`   | Expand/collapse     |

Keybindings are configurable via `~/.config/ocmux/config.json`:

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
    "quit": "q"
  }
}
```

## License

Apache-2.0
