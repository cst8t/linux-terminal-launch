# linux-terminal-launch

Terminal emulator resolution and launching for Rust applications.

This crate provides a small typed API for resolving a preferred terminal
emulator on a Linux-based system, building the command line needed to open it in a working directory,
and spawning it without shell expansion.

The crate is a WIP so keeping as a GitHub-only dependency for Gitmun while the API shape settles.
It is intentionally marked `publish = false` and is not available on crates.io
yet but will be in the future.

## Status

- No runtime dependencies.
- Optional `serde` feature for serialising preferences in application settings.
- Licensed under MIT.

## Supported terminals

Use `known_terminals()` to list every known terminal emulator supported by the crate.
This is a static support list and is not filtered by what is installed on the current system.
It is intended for settings UIs, documentation, and stable option lists.
Use `TerminalLauncher::resolve()` or `TerminalLauncher::spawn()` when you need runtime launch behaviour
for the current machine.

```rust
for terminal in linux_terminal_launch::known_terminals() {
    println!("{} ({})", terminal.label, terminal.program);
}
```

## Auto preference order

`TerminalPreference::Auto` resolves the first available terminal in this order:

1. A known terminal named by the `TERMINAL` environment variable.
2. `xdg-terminal-exec`.
3. The current desktop environment heuristic:
   - KDE: Konsole.
   - GNOME: GNOME Console, then GNOME Terminal.
   - Xfce: Xfce Terminal.
4. The built-in probe order: GNOME Terminal, GNOME Console, Konsole, Xfce Terminal, MATE Terminal, LXTerminal, Alacritty, Ghostty, Kitty, WezTerm, Foot, then Xterm.
5. `x-terminal-emulator`.

When a structured launch command is requested, generic launchers that cannot carry it are skipped while known terminals are probed. If only a generic launcher is available, resolution returns `LaunchError::UnsupportedLaunchCommand`.

## Example

```rust
use linux_terminal_launch::{KnownTerminal, LaunchCommand, TerminalLauncher, TerminalPreference};

fn main() -> Result<(), linux_terminal_launch::LaunchError> {
    TerminalLauncher::new()
        .working_dir("/home/user/project")
        .preference(TerminalPreference::Known(KnownTerminal::Ghostty))
        .launch_command(LaunchCommand::new("cargo").arg("test"))
        .spawn()
}
```
