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

## Custom terminal registry

Use `TerminalRegistry` when an application needs to add structured support for another terminal
without changing the crate's built-in list.

```rust
use linux_terminal_launch::{
    LaunchCommand, TerminalDefinition, TerminalLauncher, TerminalPreference, TerminalRegistry,
};

fn main() -> Result<(), linux_terminal_launch::LaunchError> {
    let registry = TerminalRegistry::with_known_terminals().register(
        TerminalDefinition::new("coolterm", "CoolTerm", "coolterm")
            .working_dir_args(["--cwd", "{path}"])
            .launch_command_separator("--"),
    );

    TerminalLauncher::new()
        .registry(registry)
        .preference(TerminalPreference::registered("coolterm"))
        .working_dir("/home/user/project")
        .launch_command(LaunchCommand::new("cargo").arg("test"))
        .spawn()
}
```

## Auto preference order

`TerminalPreference::Auto` resolves the first available terminal in this order:

1. A terminal named by the `TERMINAL` environment variable, when it is either registered or another executable command.
2. `xdg-terminal-exec`.
3. The current desktop environment heuristic:
   - KDE: Konsole.
   - GNOME: GNOME Console, then GNOME Terminal.
   - Xfce: Xfce Terminal.
4. The active registry order. The default registry uses this built-in probe order: GNOME Terminal, GNOME Console, Konsole, Xfce Terminal, MATE Terminal, LXTerminal, Alacritty, Ghostty, Kitty, WezTerm, Foot, then Xterm.
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

GUI applications that open a terminal for the user should usually detach the launched terminal from
the parent process. This keeps the crate's default inherited-process behaviour unchanged for command
line callers, while allowing GUI callers to avoid stdio and process-group coupling.

```rust
use linux_terminal_launch::{KnownTerminal, TerminalLauncher, TerminalPreference};

fn main() -> Result<(), linux_terminal_launch::LaunchError> {
    TerminalLauncher::new()
        .working_dir("/home/user/project")
        .preference(TerminalPreference::Known(KnownTerminal::Ghostty))
        .detach_from_parent(true)
        .spawn()
}
```

Detached mode redirects stdin, stdout, and stderr to null and starts the terminal in a new Unix
process group. It does not call `setsid`, and it may not survive launch environments that terminate
an entire desktop or systemd cgroup.
