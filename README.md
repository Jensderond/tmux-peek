# tmux-peek

Peek at your running tmux sessions from **outside** tmux, with colored window previews, and attach.

Unlike tmux's built-in `choose-tree`, which only works once you're already attached inside a session, `tmux-peek` runs as a read-only viewer from your plain shell. Glance at what's running across all your sessions — with live, colored pane previews — and attach to one only when you want to.

## Install

```sh
cargo install tmux-peek
```

## Usage

Run it from outside tmux:

```sh
tmux-peek
```

### Keys

| Key       | Action            |
| --------- | ----------------- |
| `↑` / `↓` | Move selection    |
| `/`       | Filter sessions   |
| `Enter`   | Attach to session |
| `x`       | Kill session      |
| `r`       | Refresh           |
| `q`       | Quit              |

## License

MIT — see [LICENSE](LICENSE).
