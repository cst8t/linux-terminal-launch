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

## Custom terminal commands

Use `custom_command()` when terminal-specific arguments cannot be represented by a registry
definition. It selects the custom preference as well as setting the command.

```rust
use linux_terminal_launch::TerminalLauncher;

fn main() -> Result<(), linux_terminal_launch::LaunchError> {
    TerminalLauncher::new()
        .working_dir("/home/user/project")
        .custom_command("my-terminal --working-directory {path}")
        .spawn()
}
```

`TerminalPreference::custom()` provides the same preference for applications that persist or
otherwise manage preferences separately from the launcher.

## Serialised preferences

With the optional `serde` feature, terminal preferences use stable, tagged identifiers. Unknown
terminal and preference identifiers deserialise as an unsupported preference and cannot be
serialised again, preventing a newer saved value from being silently replaced by an older version.

## Application preferences

The application owns saving the user's preference and passes it to the launcher on each launch.
Explicit known, registered, and custom terminal preferences bypass system discovery. A missing
explicit terminal produces an error instead of silently selecting a different terminal.

```rust
use linux_terminal_launch::{LaunchCommand, LaunchError, TerminalLauncher, TerminalPreference};

fn open_terminal(saved_preference: TerminalPreference) -> Result<(), LaunchError> {
    TerminalLauncher::new()
        .preference(saved_preference)
        .working_dir("/home/user/project")
        .launch_command(LaunchCommand::new("cargo").arg("test"))
        .spawn()
}
```

With the optional `serde` feature, an application can save a preference using its existing settings
storage and deserialise it before calling this function. `SystemDefault` is stored as
`{"system-default":null}`. Custom command strings still cannot be combined with `launch_command()`;
use a registered `TerminalDefinition` for structured command execution with a custom terminal.

Applications upgrading from the original unit `TerminalPreference::Custom` variant should construct
`TerminalPreference::custom(saved_command)` only when the custom option is selected. Pass that
preference to `.preference(...)`; do not call `.custom_command(...)` afterwards for every selection,
as that method selects the custom terminal and overrides Auto or a named terminal preference.

## System default preference

`TerminalPreference::SystemDefault` follows system terminal helpers without consulting `TERMINAL`,
desktop guesses, or the registry probe order:

1. `xdg-terminal-exec`, for both opening a terminal and running a structured launch command.
2. `x-terminal-emulator`, only when opening a terminal without a structured launch command.

Install `xdg-terminal-exec` to run commands through the configured system terminal. The library
passes the working directory and command arguments separately, equivalent to:

```text
xdg-terminal-exec --dir=/home/user/project -- cargo test
```

The library delegates configuration discovery to
[`xdg-terminal-exec`](https://github.com/Vladimir-csp/xdg-terminal-exec), whose proposed XDG Default
Terminal Execution Specification covers `xdg-terminals.list`, desktop-specific lists, and terminal
`.desktop` entries. It does not parse these files or desktop-specific preference stores itself.
The `x-terminal-emulator` fallback follows the system's alternatives configuration where available;
it does not imply support for XDG terminal configuration. Its working directory is inherited from
the spawned process, which some terminal implementations may not preserve when reusing a server.

If neither helper is installed, resolution returns `LaunchError::MissingTerminal` with installation
guidance. If only `x-terminal-emulator` is available and a command was requested, resolution returns
`LaunchError::UnsupportedLaunchCommand`, suggesting `xdg-terminal-exec`. It does not guess a known
terminal in either case. A helper found on `PATH` can still fail when launched (for example, if its
configuration has no usable terminal). The library does not retry another terminal after spawning;
`spawn()` reports process creation failures, not the eventual helper or terminal exit status.

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

`xdg-terminal-exec` carries structured launch commands and their arguments without shell expansion.
When a command is requested, unknown `TERMINAL` commands and `x-terminal-emulator` are skipped because
the library cannot reliably construct their command-execution arguments. If only these unsupported
launchers are available, resolution returns `LaunchError::UnsupportedLaunchCommand`.

Choose `Auto` for environment-based discovery with fallbacks, or `SystemDefault` to require a system
helper. In `Auto`, `TERMINAL` still takes precedence over `xdg-terminal-exec`, and known terminal
probes still take precedence over `x-terminal-emulator`.

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
