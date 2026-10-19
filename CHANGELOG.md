# Changelog

## Unreleased

### Added

- `TerminalPreference::SystemDefault` selects system terminal helpers without consulting `TERMINAL`,
  desktop guesses, or registry fallbacks. It prefers `xdg-terminal-exec` and falls back to
  `x-terminal-emulator` only for launches without a structured command.
- With the `serde` feature, the new preference uses `{"system-default":null}`.

### Changed

- `Auto` now forwards structured launch commands through `xdg-terminal-exec`. Previously it skipped
  that helper when a command was supplied, choosing a known terminal or reporting an unsupported
  command. This intentionally changes which terminal may open: the helper can now honour the user's
  configured XDG terminal. `TERMINAL` retains its precedence and the remaining fallback order is
  unchanged.

### Compatibility

- Existing method signatures, public struct layouts, and error variants are unchanged. Explicit
  application preferences continue to bypass system discovery.
- The new preference variant is additive to the existing non-exhaustive enum. Older library
  versions deserialise its saved identifier as `Unrecognised` and cannot serialise it again;
  applications should display an unsupported-preference message rather than overwrite the setting.
