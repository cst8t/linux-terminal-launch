use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalPreference {
    Auto,
    Known(KnownTerminal),
    Custom,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnownTerminal {
    Konsole,
    GnomeTerminal,
    GnomeConsole,
    Xfce4Terminal,
    MateTerminal,
    Lxterminal,
    Alacritty,
    Ghostty,
    Kitty,
    WezTerm,
    Foot,
    Xterm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTerminalCommand {
    pub label: &'static str,
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownTerminalInfo {
    pub terminal: KnownTerminal,
    pub label: &'static str,
    pub program: &'static str,
}

impl LaunchCommand {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }
}

#[derive(Debug)]
pub enum LaunchError {
    MissingTerminal {
        label: &'static str,
        program: &'static str,
    },
    EmptyCustomCommand,
    MalformedCustomCommand(String),
    UnsupportedLaunchCommand {
        label: &'static str,
    },
    NoSupportedTerminal,
    SpawnFailure {
        label: &'static str,
        source: io::Error,
    },
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTerminal { label, .. } => write!(f, "{label} was not found on PATH"),
            Self::EmptyCustomCommand => write!(f, "Custom terminal command cannot be empty"),
            Self::MalformedCustomCommand(message) => write!(f, "{message}"),
            Self::UnsupportedLaunchCommand { label } => {
                write!(f, "{label} does not support a structured launch command")
            }
            Self::NoSupportedTerminal => write!(f, "No supported terminal emulator was found"),
            Self::SpawnFailure { label, source } => write!(f, "Failed to open {label}: {source}"),
        }
    }
}

impl std::error::Error for LaunchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SpawnFailure { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TerminalLauncher {
    working_dir: Option<PathBuf>,
    preference: TerminalPreference,
    custom_command: Option<String>,
    launch_command: Option<LaunchCommand>,
    environment: TerminalEnvironment,
}

impl TerminalLauncher {
    pub fn new() -> Self {
        Self {
            working_dir: None,
            preference: TerminalPreference::Auto,
            custom_command: None,
            launch_command: None,
            environment: TerminalEnvironment::current(),
        }
    }

    pub fn working_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(path.into());
        self
    }

    pub fn preference(mut self, preference: TerminalPreference) -> Self {
        self.preference = preference;
        self
    }

    pub fn custom_command(mut self, command: impl Into<String>) -> Self {
        self.custom_command = Some(command.into());
        self
    }

    pub fn launch_command(mut self, command: LaunchCommand) -> Self {
        self.launch_command = Some(command);
        self
    }

    pub fn resolve(&self) -> Result<ResolvedTerminalCommand, LaunchError> {
        let path = self
            .working_dir
            .as_deref()
            .unwrap_or_else(|| Path::new("."));
        self.resolve_with(path, command_available)
    }

    pub fn spawn(&self) -> Result<(), LaunchError> {
        let command = self.resolve()?;
        let path = self
            .working_dir
            .as_deref()
            .unwrap_or_else(|| Path::new("."));

        Command::new(&command.program)
            .args(&command.args)
            .current_dir(path)
            .spawn()
            .map(|_| ())
            .map_err(|source| LaunchError::SpawnFailure {
                label: command.label,
                source,
            })
    }

    fn resolve_with(
        &self,
        path: &Path,
        available: impl Fn(&str) -> bool,
    ) -> Result<ResolvedTerminalCommand, LaunchError> {
        match self.preference {
            TerminalPreference::Custom if self.launch_command.is_some() => {
                Err(LaunchError::UnsupportedLaunchCommand { label: "Terminal" })
            }
            TerminalPreference::Custom => {
                custom_terminal_command(self.custom_command.as_deref().unwrap_or_default(), path)
            }
            TerminalPreference::Auto => auto_terminal_command(
                path,
                self.launch_command.as_ref(),
                &self.environment,
                available,
            ),
            TerminalPreference::Known(selected) => {
                named_terminal_command(selected, path, self.launch_command.as_ref(), available)
            }
        }
    }
}

impl Default for TerminalLauncher {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
struct TerminalEnvironment {
    terminal: Option<String>,
    current_desktop: Option<String>,
}

impl TerminalEnvironment {
    fn current() -> Self {
        Self {
            terminal: std::env::var_os("TERMINAL").and_then(|value| value.into_string().ok()),
            current_desktop: std::env::var_os("XDG_CURRENT_DESKTOP")
                .and_then(|value| value.into_string().ok()),
        }
    }
}

impl KnownTerminalInfo {
    fn command(
        self,
        path: &Path,
        launch_command: Option<&LaunchCommand>,
    ) -> Result<ResolvedTerminalCommand, LaunchError> {
        let path = path.to_string_lossy().to_string();
        let mut args = match self.terminal {
            KnownTerminal::Konsole => vec!["--workdir".to_string(), path],
            KnownTerminal::GnomeTerminal
            | KnownTerminal::GnomeConsole
            | KnownTerminal::Xfce4Terminal
            | KnownTerminal::MateTerminal
            | KnownTerminal::Alacritty => vec!["--working-directory".to_string(), path],
            KnownTerminal::Lxterminal | KnownTerminal::Ghostty | KnownTerminal::Foot => {
                vec![format!("--working-directory={path}")]
            }
            KnownTerminal::Kitty => vec!["--directory".to_string(), path],
            KnownTerminal::WezTerm => vec!["start".to_string(), "--cwd".to_string(), path],
            KnownTerminal::Xterm if launch_command.is_some() => vec![
                "-e".to_string(),
                "sh".to_string(),
                "-lc".to_string(),
                "cd \"$1\" || exit; shift; exec \"$@\"".to_string(),
                "sh".to_string(),
                path,
            ],
            KnownTerminal::Xterm => vec![
                "-e".to_string(),
                "sh".to_string(),
                "-lc".to_string(),
                "cd \"$1\" || exit; exec \"${SHELL:-sh}\"".to_string(),
                "sh".to_string(),
                path,
            ],
        };

        if let Some(command) = launch_command {
            match self.terminal {
                KnownTerminal::Konsole
                | KnownTerminal::Lxterminal
                | KnownTerminal::Alacritty
                | KnownTerminal::Ghostty => {
                    args.push("-e".to_string());
                    append_launch_command(&mut args, command);
                }
                KnownTerminal::GnomeTerminal
                | KnownTerminal::GnomeConsole
                | KnownTerminal::Kitty => {
                    args.push("--".to_string());
                    append_launch_command(&mut args, command);
                }
                KnownTerminal::Xfce4Terminal | KnownTerminal::MateTerminal => {
                    args.push("-x".to_string());
                    append_launch_command(&mut args, command);
                }
                KnownTerminal::WezTerm | KnownTerminal::Foot => {
                    append_launch_command(&mut args, command)
                }
                KnownTerminal::Xterm => append_launch_command(&mut args, command),
            }
        }

        Ok(ResolvedTerminalCommand {
            label: self.label,
            program: self.program.to_string(),
            args,
        })
    }
}

pub fn terminal_label(preference: TerminalPreference) -> &'static str {
    match preference {
        TerminalPreference::Auto | TerminalPreference::Custom => "Terminal",
        TerminalPreference::Known(terminal) => known_terminal_info(terminal)
            .map(|terminal| terminal.label)
            .unwrap_or("Terminal"),
    }
}

fn named_terminal_command(
    terminal: KnownTerminal,
    path: &Path,
    launch_command: Option<&LaunchCommand>,
    available: impl Fn(&str) -> bool,
) -> Result<ResolvedTerminalCommand, LaunchError> {
    let terminal = known_terminal_info(terminal).expect("known terminal enum should have details");
    if !available(terminal.program) {
        return Err(LaunchError::MissingTerminal {
            label: terminal.label,
            program: terminal.program,
        });
    }
    terminal.command(path, launch_command)
}

fn auto_terminal_command(
    path: &Path,
    launch_command: Option<&LaunchCommand>,
    environment: &TerminalEnvironment,
    available: impl Fn(&str) -> bool,
) -> Result<ResolvedTerminalCommand, LaunchError> {
    let mut found_unsupported_launcher = false;

    if let Some(terminal) = environment
        .terminal
        .as_deref()
        .and_then(terminal_from_command_spec)
    {
        if available(terminal.program) {
            return terminal.command(path, launch_command);
        }
    }

    if available("xdg-terminal-exec") {
        if launch_command.is_some() {
            found_unsupported_launcher = true;
        } else {
            return Ok(ResolvedTerminalCommand {
                label: "Terminal",
                program: "xdg-terminal-exec".to_string(),
                args: vec![format!("--dir={}", path.display())],
            });
        }
    }

    for terminal in desktop_terminals(environment.current_desktop.as_deref()) {
        if available(terminal.program) {
            return terminal.command(path, launch_command);
        }
    }

    for terminal in known_terminal_probe_order() {
        if available(terminal.program) {
            return terminal.command(path, launch_command);
        }
    }

    if available("x-terminal-emulator") {
        if launch_command.is_some() {
            found_unsupported_launcher = true;
        } else {
            return Ok(ResolvedTerminalCommand {
                label: "Terminal",
                program: "x-terminal-emulator".to_string(),
                args: Vec::new(),
            });
        }
    }

    if found_unsupported_launcher {
        return Err(LaunchError::UnsupportedLaunchCommand { label: "Terminal" });
    }

    Err(LaunchError::NoSupportedTerminal)
}

fn append_launch_command(args: &mut Vec<String>, command: &LaunchCommand) {
    args.push(command.program.clone());
    args.extend(command.args.iter().cloned());
}

pub fn known_terminal_info(terminal: KnownTerminal) -> Option<&'static KnownTerminalInfo> {
    known_terminals()
        .iter()
        .find(|details| details.terminal == terminal)
}

pub fn known_terminals() -> &'static [KnownTerminalInfo] {
    &[
        KnownTerminalInfo {
            terminal: KnownTerminal::Konsole,
            label: "Konsole",
            program: "konsole",
        },
        KnownTerminalInfo {
            terminal: KnownTerminal::GnomeTerminal,
            label: "GNOME Terminal",
            program: "gnome-terminal",
        },
        KnownTerminalInfo {
            terminal: KnownTerminal::GnomeConsole,
            label: "GNOME Console",
            program: "kgx",
        },
        KnownTerminalInfo {
            terminal: KnownTerminal::Xfce4Terminal,
            label: "Xfce Terminal",
            program: "xfce4-terminal",
        },
        KnownTerminalInfo {
            terminal: KnownTerminal::MateTerminal,
            label: "MATE Terminal",
            program: "mate-terminal",
        },
        KnownTerminalInfo {
            terminal: KnownTerminal::Lxterminal,
            label: "LXTerminal",
            program: "lxterminal",
        },
        KnownTerminalInfo {
            terminal: KnownTerminal::Alacritty,
            label: "Alacritty",
            program: "alacritty",
        },
        KnownTerminalInfo {
            terminal: KnownTerminal::Ghostty,
            label: "Ghostty",
            program: "ghostty",
        },
        KnownTerminalInfo {
            terminal: KnownTerminal::Kitty,
            label: "Kitty",
            program: "kitty",
        },
        KnownTerminalInfo {
            terminal: KnownTerminal::WezTerm,
            label: "WezTerm",
            program: "wezterm",
        },
        KnownTerminalInfo {
            terminal: KnownTerminal::Foot,
            label: "Foot",
            program: "foot",
        },
        KnownTerminalInfo {
            terminal: KnownTerminal::Xterm,
            label: "Xterm",
            program: "xterm",
        },
    ]
}

fn known_terminal_probe_order() -> Vec<KnownTerminalInfo> {
    [
        KnownTerminal::GnomeTerminal,
        KnownTerminal::GnomeConsole,
        KnownTerminal::Konsole,
        KnownTerminal::Xfce4Terminal,
        KnownTerminal::MateTerminal,
        KnownTerminal::Lxterminal,
        KnownTerminal::Alacritty,
        KnownTerminal::Ghostty,
        KnownTerminal::Kitty,
        KnownTerminal::WezTerm,
        KnownTerminal::Foot,
        KnownTerminal::Xterm,
    ]
    .into_iter()
    .filter_map(known_terminal_info)
    .copied()
    .collect()
}

fn desktop_terminals(desktop: Option<&str>) -> Vec<KnownTerminalInfo> {
    let Some(desktop) = desktop else {
        return Vec::new();
    };
    let desktop = desktop.to_ascii_lowercase();
    if desktop.contains("kde") {
        return [KnownTerminal::Konsole]
            .into_iter()
            .filter_map(known_terminal_info)
            .copied()
            .collect();
    }
    if desktop.contains("gnome") {
        return [KnownTerminal::GnomeConsole, KnownTerminal::GnomeTerminal]
            .into_iter()
            .filter_map(known_terminal_info)
            .copied()
            .collect();
    }
    if desktop.contains("xfce") {
        return [KnownTerminal::Xfce4Terminal]
            .into_iter()
            .filter_map(known_terminal_info)
            .copied()
            .collect();
    }
    Vec::new()
}

fn terminal_from_command_spec(command_spec: &str) -> Option<KnownTerminalInfo> {
    let command = split_command_spec(command_spec).ok()?.into_iter().next()?;
    let command = Path::new(&command).file_name()?.to_string_lossy();
    known_terminals()
        .iter()
        .copied()
        .find(|terminal| terminal.program == command)
}

fn custom_terminal_command(
    command_spec: &str,
    path: &Path,
) -> Result<ResolvedTerminalCommand, LaunchError> {
    let mut parts = split_command_spec(command_spec)?;
    if parts.is_empty() {
        return Err(LaunchError::EmptyCustomCommand);
    }

    let program = parts.remove(0);
    let path = path.to_string_lossy();
    let args = parts
        .into_iter()
        .map(|arg| arg.replace("{path}", &path))
        .collect();

    Ok(ResolvedTerminalCommand {
        label: "Terminal",
        program,
        args,
    })
}

fn split_command_spec(command_spec: &str) -> Result<Vec<String>, LaunchError> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = command_spec.chars().peekable();
    let mut quote: Option<char> = None;

    while let Some(ch) = chars.next() {
        match ch {
            '\'' | '"' if quote.is_none() => quote = Some(ch),
            ch if Some(ch) == quote => quote = None,
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ch if ch.is_whitespace() && quote.is_none() => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }
            ch => current.push(ch),
        }
    }

    if quote.is_some() {
        return Err(LaunchError::MalformedCustomCommand(
            "Custom terminal command has an unterminated quote".to_string(),
        ));
    }

    if !current.is_empty() {
        parts.push(current);
    }

    Ok(parts)
}

fn command_available(command: &str) -> bool {
    let path = Path::new(command);
    if path.components().count() > 1 {
        return is_executable_file(path);
    }

    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| is_executable_file(&dir.join(command)))
        })
        .unwrap_or(false)
}

fn is_executable_file(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        path.is_file()
            && path
                .metadata()
                .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn path() -> &'static Path {
        Path::new("/tmp/repo with spaces")
    }

    fn launcher(
        preference: TerminalPreference,
        terminal: Option<&str>,
        desktop: Option<&str>,
    ) -> TerminalLauncher {
        TerminalLauncher {
            working_dir: Some(path().to_path_buf()),
            preference,
            custom_command: None,
            launch_command: None,
            environment: TerminalEnvironment {
                terminal: terminal.map(str::to_string),
                current_desktop: desktop.map(str::to_string),
            },
        }
    }

    fn available(commands: &[&str]) -> impl Fn(&str) -> bool {
        let commands = commands
            .iter()
            .map(|command| command.to_string())
            .collect::<HashSet<_>>();
        move |command| commands.contains(command)
    }

    fn launch_command() -> LaunchCommand {
        LaunchCommand::new("cargo").args(["test", "--lib"])
    }

    fn supported_terminals() -> [KnownTerminal; 12] {
        [
            KnownTerminal::Konsole,
            KnownTerminal::GnomeTerminal,
            KnownTerminal::GnomeConsole,
            KnownTerminal::Xfce4Terminal,
            KnownTerminal::MateTerminal,
            KnownTerminal::Lxterminal,
            KnownTerminal::Alacritty,
            KnownTerminal::Ghostty,
            KnownTerminal::Kitty,
            KnownTerminal::WezTerm,
            KnownTerminal::Foot,
            KnownTerminal::Xterm,
        ]
    }

    #[test]
    fn known_terminals_exposes_supported_terminal_metadata() {
        let terminals = known_terminals();

        assert_eq!(terminals.len(), supported_terminals().len());
        assert_eq!(
            terminals
                .iter()
                .map(|terminal| terminal.terminal)
                .collect::<Vec<_>>(),
            supported_terminals()
        );

        for terminal in terminals {
            assert!(!terminal.label.is_empty());
            assert!(!terminal.program.is_empty());
        }
    }

    #[test]
    fn known_terminal_info_returns_entries_from_supported_list() {
        for terminal in supported_terminals() {
            let info = known_terminal_info(terminal).expect("terminal metadata should exist");
            assert_eq!(
                known_terminals()
                    .iter()
                    .find(|candidate| candidate.terminal == terminal),
                Some(info)
            );
        }
    }

    #[test]
    fn terminal_label_uses_supported_terminal_metadata() {
        for terminal in supported_terminals() {
            let info = known_terminal_info(terminal).expect("terminal metadata should exist");
            assert_eq!(
                terminal_label(TerminalPreference::Known(terminal)),
                info.label
            );
        }
    }

    #[test]
    fn known_terminals_emit_expected_working_directory_arguments() {
        let cases = [
            (
                KnownTerminal::Konsole,
                "konsole",
                vec!["--workdir", "/tmp/repo with spaces"],
            ),
            (
                KnownTerminal::GnomeTerminal,
                "gnome-terminal",
                vec!["--working-directory", "/tmp/repo with spaces"],
            ),
            (
                KnownTerminal::GnomeConsole,
                "kgx",
                vec!["--working-directory", "/tmp/repo with spaces"],
            ),
            (
                KnownTerminal::Xfce4Terminal,
                "xfce4-terminal",
                vec!["--working-directory", "/tmp/repo with spaces"],
            ),
            (
                KnownTerminal::MateTerminal,
                "mate-terminal",
                vec!["--working-directory", "/tmp/repo with spaces"],
            ),
            (
                KnownTerminal::Lxterminal,
                "lxterminal",
                vec!["--working-directory=/tmp/repo with spaces"],
            ),
            (
                KnownTerminal::Alacritty,
                "alacritty",
                vec!["--working-directory", "/tmp/repo with spaces"],
            ),
            (
                KnownTerminal::Ghostty,
                "ghostty",
                vec!["--working-directory=/tmp/repo with spaces"],
            ),
            (
                KnownTerminal::Kitty,
                "kitty",
                vec!["--directory", "/tmp/repo with spaces"],
            ),
            (
                KnownTerminal::WezTerm,
                "wezterm",
                vec!["start", "--cwd", "/tmp/repo with spaces"],
            ),
            (
                KnownTerminal::Foot,
                "foot",
                vec!["--working-directory=/tmp/repo with spaces"],
            ),
            (
                KnownTerminal::Xterm,
                "xterm",
                vec![
                    "-e",
                    "sh",
                    "-lc",
                    "cd \"$1\" || exit; exec \"${SHELL:-sh}\"",
                    "sh",
                    "/tmp/repo with spaces",
                ],
            ),
        ];

        for (terminal, program, args) in cases {
            let command = launcher(TerminalPreference::Known(terminal), None, None)
                .resolve_with(path(), available(&[program]))
                .expect("terminal should resolve");
            assert_eq!(command.program, program);
            assert_eq!(
                command.args,
                args.into_iter().map(str::to_string).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn known_terminals_emit_expected_launch_command_arguments() {
        let cases = [
            (
                KnownTerminal::Konsole,
                "konsole",
                vec![
                    "--workdir",
                    "/tmp/repo with spaces",
                    "-e",
                    "cargo",
                    "test",
                    "--lib",
                ],
            ),
            (
                KnownTerminal::GnomeTerminal,
                "gnome-terminal",
                vec![
                    "--working-directory",
                    "/tmp/repo with spaces",
                    "--",
                    "cargo",
                    "test",
                    "--lib",
                ],
            ),
            (
                KnownTerminal::GnomeConsole,
                "kgx",
                vec![
                    "--working-directory",
                    "/tmp/repo with spaces",
                    "--",
                    "cargo",
                    "test",
                    "--lib",
                ],
            ),
            (
                KnownTerminal::Xfce4Terminal,
                "xfce4-terminal",
                vec![
                    "--working-directory",
                    "/tmp/repo with spaces",
                    "-x",
                    "cargo",
                    "test",
                    "--lib",
                ],
            ),
            (
                KnownTerminal::MateTerminal,
                "mate-terminal",
                vec![
                    "--working-directory",
                    "/tmp/repo with spaces",
                    "-x",
                    "cargo",
                    "test",
                    "--lib",
                ],
            ),
            (
                KnownTerminal::Lxterminal,
                "lxterminal",
                vec![
                    "--working-directory=/tmp/repo with spaces",
                    "-e",
                    "cargo",
                    "test",
                    "--lib",
                ],
            ),
            (
                KnownTerminal::Alacritty,
                "alacritty",
                vec![
                    "--working-directory",
                    "/tmp/repo with spaces",
                    "-e",
                    "cargo",
                    "test",
                    "--lib",
                ],
            ),
            (
                KnownTerminal::Ghostty,
                "ghostty",
                vec![
                    "--working-directory=/tmp/repo with spaces",
                    "-e",
                    "cargo",
                    "test",
                    "--lib",
                ],
            ),
            (
                KnownTerminal::Kitty,
                "kitty",
                vec![
                    "--directory",
                    "/tmp/repo with spaces",
                    "--",
                    "cargo",
                    "test",
                    "--lib",
                ],
            ),
            (
                KnownTerminal::WezTerm,
                "wezterm",
                vec![
                    "start",
                    "--cwd",
                    "/tmp/repo with spaces",
                    "cargo",
                    "test",
                    "--lib",
                ],
            ),
            (
                KnownTerminal::Foot,
                "foot",
                vec![
                    "--working-directory=/tmp/repo with spaces",
                    "cargo",
                    "test",
                    "--lib",
                ],
            ),
            (
                KnownTerminal::Xterm,
                "xterm",
                vec![
                    "-e",
                    "sh",
                    "-lc",
                    "cd \"$1\" || exit; shift; exec \"$@\"",
                    "sh",
                    "/tmp/repo with spaces",
                    "cargo",
                    "test",
                    "--lib",
                ],
            ),
        ];

        for (terminal, program, args) in cases {
            let command = launcher(TerminalPreference::Known(terminal), None, None)
                .launch_command(launch_command())
                .resolve_with(path(), available(&[program]))
                .expect("terminal should resolve");
            assert_eq!(command.program, program);
            assert_eq!(
                command.args,
                args.into_iter().map(str::to_string).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn auto_prefers_mapped_terminal_environment() {
        let command = launcher(TerminalPreference::Auto, Some("kitty"), Some("KDE"))
            .resolve_with(
                path(),
                available(&["kitty", "konsole", "xdg-terminal-exec"]),
            )
            .expect("terminal should resolve");

        assert_eq!(command.program, "kitty");
    }

    #[test]
    fn auto_uses_xdg_terminal_exec_before_desktop_heuristic() {
        let command = launcher(TerminalPreference::Auto, None, Some("KDE"))
            .resolve_with(path(), available(&["xdg-terminal-exec", "konsole"]))
            .expect("terminal should resolve");

        assert_eq!(command.program, "xdg-terminal-exec");
        assert_eq!(command.args, vec!["--dir=/tmp/repo with spaces"]);
    }

    #[test]
    fn auto_skips_xdg_terminal_exec_with_launch_command() {
        let command = launcher(TerminalPreference::Auto, None, Some("KDE"))
            .launch_command(launch_command())
            .resolve_with(path(), available(&["xdg-terminal-exec", "konsole"]))
            .expect("known terminal should resolve");

        assert_eq!(command.program, "konsole");
        assert_eq!(
            command.args,
            vec![
                "--workdir",
                "/tmp/repo with spaces",
                "-e",
                "cargo",
                "test",
                "--lib"
            ]
        );
    }

    #[test]
    fn auto_rejects_xdg_terminal_exec_with_launch_command_when_no_known_terminal_exists() {
        let error = launcher(TerminalPreference::Auto, None, None)
            .launch_command(launch_command())
            .resolve_with(path(), available(&["xdg-terminal-exec"]))
            .expect_err("generic terminal should reject launch command");

        assert!(matches!(
            error,
            LaunchError::UnsupportedLaunchCommand { .. }
        ));
        assert_eq!(
            error.to_string(),
            "Terminal does not support a structured launch command"
        );
    }

    #[test]
    fn auto_uses_desktop_heuristic_before_probe_order() {
        let command = launcher(TerminalPreference::Auto, None, Some("KDE"))
            .resolve_with(path(), available(&["gnome-terminal", "konsole"]))
            .expect("terminal should resolve");

        assert_eq!(command.program, "konsole");
    }

    #[test]
    fn auto_falls_back_to_probe_order_then_x_terminal_emulator() {
        let probed = launcher(TerminalPreference::Auto, None, None)
            .resolve_with(path(), available(&["kitty"]))
            .expect("terminal should resolve");
        assert_eq!(probed.program, "kitty");

        let debian_fallback = launcher(TerminalPreference::Auto, None, None)
            .resolve_with(path(), available(&["x-terminal-emulator"]))
            .expect("terminal should resolve");
        assert_eq!(debian_fallback.program, "x-terminal-emulator");
        assert!(debian_fallback.args.is_empty());
    }

    #[test]
    fn auto_rejects_x_terminal_emulator_with_launch_command() {
        let error = launcher(TerminalPreference::Auto, None, None)
            .launch_command(launch_command())
            .resolve_with(path(), available(&["x-terminal-emulator"]))
            .expect_err("generic fallback should reject launch command");

        assert!(matches!(
            error,
            LaunchError::UnsupportedLaunchCommand { .. }
        ));
        assert_eq!(
            error.to_string(),
            "Terminal does not support a structured launch command"
        );
    }

    #[test]
    fn custom_command_replaces_path_and_keeps_spaces() {
        let command = TerminalLauncher {
            custom_command: Some("my-terminal --working-directory \"{path}\" --flag".to_string()),
            ..launcher(TerminalPreference::Custom, None, None)
        }
        .resolve_with(path(), available(&[]))
        .expect("custom command should parse");

        assert_eq!(command.program, "my-terminal");
        assert_eq!(
            command.args,
            vec!["--working-directory", "/tmp/repo with spaces", "--flag"]
        );
    }

    #[test]
    fn custom_command_can_omit_path_argument() {
        let command = TerminalLauncher {
            custom_command: Some("my-terminal --new-window".to_string()),
            ..launcher(TerminalPreference::Custom, None, None)
        }
        .resolve_with(path(), available(&[]))
        .expect("custom command should parse");

        assert_eq!(command.program, "my-terminal");
        assert_eq!(command.args, vec!["--new-window"]);
    }

    #[test]
    fn custom_command_keeps_quoted_arguments_together() {
        let command = TerminalLauncher {
            custom_command: Some("my-terminal --title 'Repo Terminal'".to_string()),
            ..launcher(TerminalPreference::Custom, None, None)
        }
        .resolve_with(path(), available(&[]))
        .expect("custom command should parse");

        assert_eq!(command.args, vec!["--title", "Repo Terminal"]);
    }

    #[test]
    fn custom_command_rejects_unterminated_quotes() {
        let error = TerminalLauncher {
            custom_command: Some("my-terminal --title \"Repo".to_string()),
            ..launcher(TerminalPreference::Custom, None, None)
        }
        .resolve_with(path(), available(&[]))
        .expect_err("custom command should fail");

        assert!(matches!(error, LaunchError::MalformedCustomCommand(_)));
        assert_eq!(
            error.to_string(),
            "Custom terminal command has an unterminated quote"
        );
    }

    #[test]
    fn custom_command_rejects_empty_command() {
        let error = TerminalLauncher {
            custom_command: Some("   ".to_string()),
            ..launcher(TerminalPreference::Custom, None, None)
        }
        .resolve_with(path(), available(&[]))
        .expect_err("custom command should fail");

        assert!(matches!(error, LaunchError::EmptyCustomCommand));
        assert_eq!(error.to_string(), "Custom terminal command cannot be empty");
    }

    #[test]
    fn custom_command_rejects_launch_command() {
        let error = TerminalLauncher {
            custom_command: Some("my-terminal --working-directory \"{path}\"".to_string()),
            ..launcher(TerminalPreference::Custom, None, None)
        }
        .launch_command(launch_command())
        .resolve_with(path(), available(&[]))
        .expect_err("custom terminal should reject launch command");

        assert!(matches!(
            error,
            LaunchError::UnsupportedLaunchCommand { .. }
        ));
        assert_eq!(
            error.to_string(),
            "Terminal does not support a structured launch command"
        );
    }

    #[test]
    fn missing_named_terminal_returns_clear_error() {
        let error = launcher(
            TerminalPreference::Known(KnownTerminal::Konsole),
            None,
            None,
        )
        .resolve_with(path(), available(&[]))
        .expect_err("missing terminal should fail");

        assert!(matches!(error, LaunchError::MissingTerminal { .. }));
        assert_eq!(error.to_string(), "Konsole was not found on PATH");
    }

    #[test]
    fn auto_without_available_terminal_returns_clear_error() {
        let error = launcher(TerminalPreference::Auto, None, None)
            .resolve_with(path(), available(&[]))
            .expect_err("missing terminals should fail");

        assert!(matches!(error, LaunchError::NoSupportedTerminal));
        assert_eq!(
            error.to_string(),
            "No supported terminal emulator was found"
        );
    }
}
