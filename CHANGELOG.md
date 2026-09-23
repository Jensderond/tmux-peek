# Changelog

## 0.2.0 — 2026-09-23

### Added

- Quick-select: sessions are labelled `0`–`9`, then `a`–`z`; press a label to attach to that session directly

### Changed

- Smoother scrolling while holding the arrow keys: each session's window previews are captured with a single `tmux` call, and only once pending keypresses are handled

## 0.1.0 — 2026-06-26

- Initial release: browse tmux sessions from outside tmux with colored window previews, `/` filter, attach, kill and refresh
