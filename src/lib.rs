use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod registry;
pub use registry::{TerminalDefinition, TerminalRegistry};

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TerminalPreference {
    /// Discover a terminal using the environment, system helpers, and registry fallbacks.
    Auto,
    /// Use system terminal helpers without consulting `TERMINAL` or registry fallbacks.
    ///
    /// Prefers `xdg-terminal-exec`. Falls back to `x-terminal-emulator` only when
    /// no structured launch command is requested.
    SystemDefault,
    Known(KnownTerminal),
    Registered(String),
    Custom(String),
    Unrecognised,
}

#[cfg(feature = "serde")]
impl serde::Serialize for TerminalPreference {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            Self::Auto => map.serialize_entry("auto", &())?,
            Self::SystemDefault => map.serialize_entry("system-default", &())?,
            Self::Known(terminal) => map.serialize_entry("known", terminal)?,
            Self::Registered(identifier) => map.serialize_entry("registered", identifier)?,
            Self::Custom(command) => map.serialize_entry("custom", command)?,
            Self::Unrecognised => {
                return Err(<S::Error as serde::ser::Error>::custom(
                    "an unrecognised terminal preference cannot be serialised",
                ));
            }
        }
        map.end()
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for TerminalPreference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(TerminalPreferenceVisitor)
    }
}

#[cfg(feature = "serde")]
struct TerminalPreferenceVisitor;

#[cfg(feature = "serde")]
impl<'de> serde::de::Visitor<'de> for TerminalPreferenceVisitor {
    type Value = TerminalPreference;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a single-entry terminal preference map")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let kind = map
            .next_key::<String>()?
            .ok_or_else(|| <A::Error as serde::de::Error>::custom("missing terminal preference"))?;
        let preference = match kind.as_str() {
            "auto" => {
                map.next_value::<()>()?;
                TerminalPreference::Auto
            }
            "system-default" => {
                map.next_value::<()>()?;
                TerminalPreference::SystemDefault
            }
            "known" => TerminalPreference::Known(map.next_value()?),
            "registered" => TerminalPreference::Registered(map.next_value()?),
            "custom" => TerminalPreference::Custom(map.next_value()?),
            _ => {
                map.next_value::<serde::de::IgnoredAny>()?;
                TerminalPreference::Unrecognised
            }
        };

        if map.next_key::<serde::de::IgnoredAny>()?.is_some() {
            return Err(<A::Error as serde::de::Error>::custom(
                "terminal preferences must contain exactly one entry",
            ));
        }

        Ok(preference)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum KnownTerminal {
    #[cfg_attr(feature = "serde", serde(rename = "konsole"))]
    Konsole,
    #[cfg_attr(feature = "serde", serde(rename = "gnome-terminal"))]
    GnomeTerminal,
    #[cfg_attr(feature = "serde", serde(rename = "gnome-console"))]
    GnomeConsole,
    #[cfg_attr(feature = "serde", serde(rename = "xfce4-terminal"))]
    Xfce4Terminal,
    #[cfg_attr(feature = "serde", serde(rename = "mate-terminal"))]
    MateTerminal,
    #[cfg_attr(feature = "serde", serde(rename = "lxterminal"))]
    Lxterminal,
    #[cfg_attr(feature = "serde", serde(rename = "alacritty"))]
    Alacritty,
    #[cfg_attr(feature = "serde", serde(rename = "ghostty"))]
    Ghostty,
    #[cfg_attr(feature = "serde", serde(rename = "kitty"))]
    Kitty,
    #[cfg_attr(feature = "serde", serde(rename = "wezterm"))]
    WezTerm,
    #[cfg_attr(feature = "serde", serde(rename = "foot"))]
    Foot,
    #[cfg_attr(feature = "serde", serde(rename = "xterm"))]
    Xterm,
    #[cfg_attr(feature = "serde", serde(other, skip_serializing))]
    Unrecognised,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTerminalCommand {
    pub label: String,
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

impl TerminalPreference {
    pub fn registered(id: impl Into<String>) -> Self {
        Self::Registered(id.into())
    }

    pub fn custom(command: impl Into<String>) -> Self {
        Self::Custom(command.into())
    }
}

#[derive(Debug)]
pub enum LaunchError {
    MissingTerminal { label: String, program: String },
    EmptyCustomCommand,
    MalformedCustomCommand(String),
    UnsupportedLaunchCommand { label: String },
    UnsupportedTerminalPreference,
    NoSupportedTerminal,
    SpawnFailure { label: String, source: io::Error },
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
            Self::UnsupportedTerminalPreference => {
                write!(
                    f,
                    "The saved terminal preference is not supported by this version"
                )
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
    launch_command: Option<LaunchCommand>,
    detach_from_parent: bool,
    environment: TerminalEnvironment,
    registry: TerminalRegistry,
}

impl TerminalLauncher {
    pub fn new() -> Self {
        Self {
            working_dir: None,
            preference: TerminalPreference::Auto,
            launch_command: None,
            detach_from_parent: false,
            environment: TerminalEnvironment::current(),
            registry: TerminalRegistry::with_known_terminals(),
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
        self.preference = TerminalPreference::custom(command);
        self
    }

    pub fn registry(mut self, registry: TerminalRegistry) -> Self {
        self.registry = registry;
        self
    }

    pub fn launch_command(mut self, command: LaunchCommand) -> Self {
        self.launch_command = Some(command);
        self
    }

    pub fn detach_from_parent(mut self, detach: bool) -> Self {
        self.detach_from_parent = detach;
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

        let mut process = Command::new(&command.program);
        process.args(&command.args).current_dir(path);
        if self.detach_from_parent {
            configure_detached_process(&mut process);
        }

        process
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
        match &self.preference {
            TerminalPreference::Unrecognised
            | TerminalPreference::Known(KnownTerminal::Unrecognised) => {
                Err(LaunchError::UnsupportedTerminalPreference)
            }
            TerminalPreference::Custom(_) if self.launch_command.is_some() => {
                Err(LaunchError::UnsupportedLaunchCommand {
                    label: "Terminal".to_string(),
                })
            }
            TerminalPreference::Custom(command) => custom_terminal_command(command, path),
            TerminalPreference::SystemDefault => {
                system_default_terminal_command(path, self.launch_command.as_ref(), &available)
            }
            TerminalPreference::Auto => auto_terminal_command(
                path,
                self.launch_command.as_ref(),
                &self.environment,
                &self.registry,
                &available,
            ),
            TerminalPreference::Known(selected) => named_terminal_command(
                *selected,
                path,
                self.launch_command.as_ref(),
                &self.registry,
                &available,
            ),
            TerminalPreference::Registered(selected) => registered_terminal_command(
                selected,
                path,
                self.launch_command.as_ref(),
                &self.registry,
                &available,
            ),
        }
    }
}

fn configure_detached_process(command: &mut Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        command.process_group(0);
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

pub fn terminal_label(preference: TerminalPreference) -> String {
    match preference {
        TerminalPreference::SystemDefault => "System default terminal".to_string(),
        TerminalPreference::Auto
        | TerminalPreference::Custom(_)
        | TerminalPreference::Unrecognised => "Terminal".to_string(),
        TerminalPreference::Known(terminal) => known_terminal_info(terminal)
            .map(|terminal| terminal.label)
            .unwrap_or("Terminal")
            .to_string(),
        TerminalPreference::Registered(identifier) => identifier,
    }
}

fn named_terminal_command(
    terminal: KnownTerminal,
    path: &Path,
    launch_command: Option<&LaunchCommand>,
    registry: &TerminalRegistry,
    available: &impl Fn(&str) -> bool,
) -> Result<ResolvedTerminalCommand, LaunchError> {
    if terminal == KnownTerminal::Unrecognised {
        return Err(LaunchError::UnsupportedTerminalPreference);
    }

    let Some(definition) = registry.known(terminal) else {
        let terminal =
            known_terminal_info(terminal).expect("known terminal enum should have details");
        return Err(LaunchError::MissingTerminal {
            label: terminal.label.to_string(),
            program: terminal.program.to_string(),
        });
    };

    if !available(definition.program()) {
        return Err(LaunchError::MissingTerminal {
            label: definition.label().to_string(),
            program: definition.program().to_string(),
        });
    }
    definition.command(path, launch_command)
}

fn registered_terminal_command(
    identifier: &str,
    path: &Path,
    launch_command: Option<&LaunchCommand>,
    registry: &TerminalRegistry,
    available: &impl Fn(&str) -> bool,
) -> Result<ResolvedTerminalCommand, LaunchError> {
    let terminal = registry
        .registered(identifier)
        .ok_or_else(|| LaunchError::MissingTerminal {
            label: identifier.to_string(),
            program: identifier.to_string(),
        })?;

    if !available(terminal.program()) {
        return Err(LaunchError::MissingTerminal {
            label: terminal.label().to_string(),
            program: terminal.program().to_string(),
        });
    }

    terminal.command(path, launch_command)
}

fn xdg_terminal_command(
    path: &Path,
    launch_command: Option<&LaunchCommand>,
) -> ResolvedTerminalCommand {
    let mut args = vec![format!("--dir={}", path.display())];
    if let Some(command) = launch_command {
        args.push("--".to_string());
        args.push(command.program.clone());
        args.extend(command.args.iter().cloned());
    }
    ResolvedTerminalCommand {
        label: "Terminal".to_string(),
        program: "xdg-terminal-exec".to_string(),
        args,
    }
}

fn system_default_terminal_command(
    path: &Path,
    launch_command: Option<&LaunchCommand>,
    available: &impl Fn(&str) -> bool,
) -> Result<ResolvedTerminalCommand, LaunchError> {
    if available("xdg-terminal-exec") {
        return Ok(xdg_terminal_command(path, launch_command));
    }
    if available("x-terminal-emulator") {
        if launch_command.is_some() {
            return Err(LaunchError::UnsupportedLaunchCommand {
                label: "x-terminal-emulator (use xdg-terminal-exec to run commands)".to_string(),
            });
        }
        return Ok(ResolvedTerminalCommand {
            label: "System default terminal".to_string(),
            program: "x-terminal-emulator".to_string(),
            args: Vec::new(),
        });
    }
    Err(LaunchError::MissingTerminal {
        label: if launch_command.is_some() {
            "System default terminal helper (install xdg-terminal-exec to run commands)"
        } else {
            "System default terminal helper (install xdg-terminal-exec or x-terminal-emulator)"
        }
        .to_string(),
        program: "xdg-terminal-exec".to_string(),
    })
}

fn auto_terminal_command(
    path: &Path,
    launch_command: Option<&LaunchCommand>,
    environment: &TerminalEnvironment,
    registry: &TerminalRegistry,
    available: &impl Fn(&str) -> bool,
) -> Result<ResolvedTerminalCommand, LaunchError> {
    let mut found_unsupported_launcher = false;

    if let Some(parts) = environment
        .terminal
        .as_deref()
        .and_then(|terminal| split_command_spec(terminal).ok())
        .filter(|parts| !parts.is_empty())
    {
        let program = &parts[0];
        if let Some(terminal) = registry.definition_for_command(program) {
            if available(terminal.program()) {
                return terminal.command(path, launch_command);
            }
        } else if available(program) {
            if launch_command.is_some() {
                found_unsupported_launcher = true;
            } else {
                return Ok(ResolvedTerminalCommand {
                    label: "Terminal".to_string(),
                    program: program.clone(),
                    args: parts[1..].to_vec(),
                });
            }
        }
    }

    if available("xdg-terminal-exec") {
        return Ok(xdg_terminal_command(path, launch_command));
    }

    for terminal in registry.desktop_terminals(environment.current_desktop.as_deref()) {
        if available(terminal.program()) {
            return terminal.command(path, launch_command);
        }
    }

    for terminal in registry.definitions() {
        if available(terminal.program()) {
            return terminal.command(path, launch_command);
        }
    }

    if available("x-terminal-emulator") {
        if launch_command.is_some() {
            found_unsupported_launcher = true;
        } else {
            return Ok(ResolvedTerminalCommand {
                label: "Terminal".to_string(),
                program: "x-terminal-emulator".to_string(),
                args: Vec::new(),
            });
        }
    }

    if found_unsupported_launcher {
        return Err(LaunchError::UnsupportedLaunchCommand {
            label: "Terminal".to_string(),
        });
    }

    Err(LaunchError::NoSupportedTerminal)
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
        label: "Terminal".to_string(),
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
mod tests;
