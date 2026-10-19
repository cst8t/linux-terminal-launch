use std::path::Path;

use crate::{
    KnownTerminal, LaunchCommand, LaunchError, ResolvedTerminalCommand, known_terminal_info,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalRegistry {
    definitions: Vec<TerminalDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalDefinition {
    known_terminal: Option<KnownTerminal>,
    id: String,
    label: String,
    program: String,
    working_dir_args: Vec<String>,
    launch_command: LaunchCommandTemplate,
    command_builder: TerminalCommandBuilder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LaunchCommandTemplate {
    Unsupported,
    Append,
    Prefix(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalCommandBuilder {
    Template,
    Xterm,
}

impl TerminalRegistry {
    pub fn new() -> Self {
        Self {
            definitions: Vec::new(),
        }
    }

    pub fn with_known_terminals() -> Self {
        known_terminal_probe_order()
            .into_iter()
            .filter_map(known_terminal_definition)
            .fold(Self::new(), Self::register)
    }

    pub fn register(mut self, definition: TerminalDefinition) -> Self {
        self.definitions.push(definition);
        self
    }

    pub fn register_first(mut self, definition: TerminalDefinition) -> Self {
        self.definitions.insert(0, definition);
        self
    }

    pub fn add(&mut self, definition: TerminalDefinition) {
        self.definitions.push(definition);
    }

    pub fn prepend(&mut self, definition: TerminalDefinition) {
        self.definitions.insert(0, definition);
    }

    pub fn definitions(&self) -> &[TerminalDefinition] {
        &self.definitions
    }

    pub(crate) fn known(&self, terminal: KnownTerminal) -> Option<&TerminalDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.known_terminal == Some(terminal))
    }

    pub(crate) fn registered(&self, identifier: &str) -> Option<&TerminalDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.id == identifier)
    }

    pub(crate) fn definition_for_command(&self, command: &str) -> Option<&TerminalDefinition> {
        let command = Path::new(command).file_name()?.to_string_lossy();
        self.definitions
            .iter()
            .find(|definition| definition.matches_program(&command))
    }

    pub fn label_for_preference(&self, preference: &crate::TerminalPreference) -> String {
        match preference {
            crate::TerminalPreference::SystemDefault => "System default terminal".to_string(),
            crate::TerminalPreference::Auto
            | crate::TerminalPreference::Custom(_)
            | crate::TerminalPreference::Unrecognised => "Terminal".to_string(),
            crate::TerminalPreference::Known(terminal) => known_terminal_info(*terminal)
                .map(|terminal| terminal.label)
                .unwrap_or("Terminal")
                .to_string(),
            crate::TerminalPreference::Registered(identifier) => self
                .registered(identifier)
                .map(|definition| definition.label().to_string())
                .unwrap_or_else(|| identifier.clone()),
        }
    }

    pub(crate) fn desktop_terminals(&self, desktop: Option<&str>) -> Vec<&TerminalDefinition> {
        let Some(desktop) = desktop else {
            return Vec::new();
        };
        let desktop = desktop.to_ascii_lowercase();
        if desktop.contains("kde") {
            return self.known_terminals([KnownTerminal::Konsole]);
        }
        if desktop.contains("gnome") {
            return self
                .known_terminals([KnownTerminal::GnomeConsole, KnownTerminal::GnomeTerminal]);
        }
        if desktop.contains("xfce") {
            return self.known_terminals([KnownTerminal::Xfce4Terminal]);
        }
        Vec::new()
    }

    fn known_terminals<const N: usize>(
        &self,
        terminals: [KnownTerminal; N],
    ) -> Vec<&TerminalDefinition> {
        terminals
            .into_iter()
            .filter_map(|terminal| self.known(terminal))
            .collect()
    }
}

impl Default for TerminalRegistry {
    fn default() -> Self {
        Self::with_known_terminals()
    }
}

impl TerminalDefinition {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        program: impl Into<String>,
    ) -> Self {
        Self {
            known_terminal: None,
            id: id.into(),
            label: label.into(),
            program: program.into(),
            working_dir_args: Vec::new(),
            launch_command: LaunchCommandTemplate::Unsupported,
            command_builder: TerminalCommandBuilder::Template,
        }
    }

    pub fn working_dir_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.working_dir_args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn launch_command_separator(mut self, separator: impl Into<String>) -> Self {
        self.launch_command = LaunchCommandTemplate::Prefix(vec![separator.into()]);
        self
    }

    pub fn launch_command_prefix_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.launch_command =
            LaunchCommandTemplate::Prefix(args.into_iter().map(Into::into).collect());
        self
    }

    pub fn append_launch_command(mut self) -> Self {
        self.launch_command = LaunchCommandTemplate::Append;
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn known_terminal(&self) -> Option<KnownTerminal> {
        self.known_terminal
    }

    pub fn supports_launch_command(&self) -> bool {
        !matches!(self.launch_command, LaunchCommandTemplate::Unsupported)
            || self.command_builder == TerminalCommandBuilder::Xterm
    }

    pub(crate) fn command(
        &self,
        path: &Path,
        launch_command: Option<&LaunchCommand>,
    ) -> Result<ResolvedTerminalCommand, LaunchError> {
        if self.command_builder == TerminalCommandBuilder::Xterm {
            return self.xterm_command(path, launch_command);
        }

        if launch_command.is_some() && !self.supports_launch_command() {
            return Err(LaunchError::UnsupportedLaunchCommand {
                label: self.label.clone(),
            });
        }

        let path = path.to_string_lossy();
        let mut args = self
            .working_dir_args
            .iter()
            .map(|arg| render_terminal_arg(arg, &path))
            .collect::<Vec<_>>();

        if let Some(command) = launch_command {
            match &self.launch_command {
                LaunchCommandTemplate::Unsupported => {}
                LaunchCommandTemplate::Append => append_launch_command(&mut args, command),
                LaunchCommandTemplate::Prefix(prefix_args) => {
                    args.extend(
                        prefix_args
                            .iter()
                            .map(|arg| render_terminal_arg(arg, &path)),
                    );
                    append_launch_command(&mut args, command);
                }
            }
        }

        Ok(ResolvedTerminalCommand {
            label: self.label.clone(),
            program: self.program.clone(),
            args,
        })
    }

    fn known(mut self, terminal: KnownTerminal) -> Self {
        self.known_terminal = Some(terminal);
        self
    }

    fn xterm(mut self) -> Self {
        self.command_builder = TerminalCommandBuilder::Xterm;
        self
    }

    fn xterm_command(
        &self,
        path: &Path,
        launch_command: Option<&LaunchCommand>,
    ) -> Result<ResolvedTerminalCommand, LaunchError> {
        let path = path.to_string_lossy().to_string();
        let mut args = if launch_command.is_some() {
            vec![
                "-e".to_string(),
                "sh".to_string(),
                "-lc".to_string(),
                "cd -- \"$1\" || exit; shift; exec -- \"$@\"".to_string(),
                "sh".to_string(),
                path,
            ]
        } else {
            vec![
                "-e".to_string(),
                "sh".to_string(),
                "-lc".to_string(),
                "cd -- \"$1\" || exit; exec -- \"${SHELL:-sh}\"".to_string(),
                "sh".to_string(),
                path,
            ]
        };

        if let Some(command) = launch_command {
            append_launch_command(&mut args, command);
        }

        Ok(ResolvedTerminalCommand {
            label: self.label.clone(),
            program: self.program.clone(),
            args,
        })
    }

    fn matches_program(&self, program: &str) -> bool {
        if self.program == program {
            return true;
        }

        Path::new(&self.program)
            .file_name()
            .map(|file_name| file_name == program)
            .unwrap_or(false)
    }
}

fn known_terminal_probe_order() -> [KnownTerminal; 12] {
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
}

fn known_terminal_definition(terminal: KnownTerminal) -> Option<TerminalDefinition> {
    let info = known_terminal_info(terminal)?;
    let definition = match terminal {
        KnownTerminal::Konsole => {
            TerminalDefinition::new(known_terminal_id(terminal), info.label, info.program)
                .working_dir_args(["--workdir", "{path}"])
                .launch_command_separator("-e")
        }
        KnownTerminal::GnomeTerminal | KnownTerminal::GnomeConsole => {
            TerminalDefinition::new(known_terminal_id(terminal), info.label, info.program)
                .working_dir_args(["--working-directory", "{path}"])
                .launch_command_separator("--")
        }
        KnownTerminal::Xfce4Terminal | KnownTerminal::MateTerminal => {
            TerminalDefinition::new(known_terminal_id(terminal), info.label, info.program)
                .working_dir_args(["--working-directory", "{path}"])
                .launch_command_separator("-x")
        }
        KnownTerminal::Lxterminal | KnownTerminal::Ghostty => {
            TerminalDefinition::new(known_terminal_id(terminal), info.label, info.program)
                .working_dir_args(["--working-directory={path}"])
                .launch_command_separator("-e")
        }
        KnownTerminal::Alacritty => {
            TerminalDefinition::new(known_terminal_id(terminal), info.label, info.program)
                .working_dir_args(["--working-directory", "{path}"])
                .launch_command_separator("-e")
        }
        KnownTerminal::Kitty => {
            TerminalDefinition::new(known_terminal_id(terminal), info.label, info.program)
                .working_dir_args(["--directory", "{path}"])
                .launch_command_separator("--")
        }
        KnownTerminal::WezTerm => {
            TerminalDefinition::new(known_terminal_id(terminal), info.label, info.program)
                .working_dir_args(["start", "--cwd", "{path}"])
                .append_launch_command()
        }
        KnownTerminal::Foot => {
            TerminalDefinition::new(known_terminal_id(terminal), info.label, info.program)
                .working_dir_args(["--working-directory={path}"])
                .append_launch_command()
        }
        KnownTerminal::Xterm => {
            TerminalDefinition::new(known_terminal_id(terminal), info.label, info.program).xterm()
        }
        KnownTerminal::Unrecognised => return None,
    };
    Some(definition.known(terminal))
}

fn known_terminal_id(terminal: KnownTerminal) -> &'static str {
    match terminal {
        KnownTerminal::Konsole => "konsole",
        KnownTerminal::GnomeTerminal => "gnome-terminal",
        KnownTerminal::GnomeConsole => "gnome-console",
        KnownTerminal::Xfce4Terminal => "xfce4-terminal",
        KnownTerminal::MateTerminal => "mate-terminal",
        KnownTerminal::Lxterminal => "lxterminal",
        KnownTerminal::Alacritty => "alacritty",
        KnownTerminal::Ghostty => "ghostty",
        KnownTerminal::Kitty => "kitty",
        KnownTerminal::WezTerm => "wezterm",
        KnownTerminal::Foot => "foot",
        KnownTerminal::Xterm => "xterm",
        KnownTerminal::Unrecognised => "unrecognised",
    }
}

fn append_launch_command(args: &mut Vec<String>, command: &LaunchCommand) {
    args.push(command.program.clone());
    args.extend(command.args.iter().cloned());
}

fn render_terminal_arg(arg: &str, path: &str) -> String {
    arg.replace("{path}", path)
}
