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
        launch_command: None,
        detach_from_parent: false,
        environment: TerminalEnvironment {
            terminal: terminal.map(str::to_string),
            current_desktop: desktop.map(str::to_string),
        },
        registry: TerminalRegistry::with_known_terminals(),
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

fn custom_registry() -> TerminalRegistry {
    TerminalRegistry::new().register(
        TerminalDefinition::new("coolterm", "CoolTerm", "coolterm")
            .working_dir_args(["--cwd", "{path}"])
            .launch_command_separator("--"),
    )
}

#[test]
fn launcher_inherits_parent_lifetime_by_default() {
    assert!(!TerminalLauncher::new().detach_from_parent);
}

#[test]
fn detach_from_parent_sets_launcher_lifetime_preference() {
    assert!(
        TerminalLauncher::new()
            .detach_from_parent(true)
            .detach_from_parent
    );
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
fn terminal_label_uses_registered_identifier() {
    assert_eq!(
        terminal_label(TerminalPreference::registered("coolterm")),
        "coolterm"
    );
}

#[test]
fn registry_label_for_preference_uses_registered_definition_label() {
    assert_eq!(
        custom_registry().label_for_preference(&TerminalPreference::registered("coolterm")),
        "CoolTerm"
    );
}

#[test]
fn registry_label_for_preference_falls_back_to_registered_identifier() {
    assert_eq!(
        custom_registry().label_for_preference(&TerminalPreference::registered("missingterm")),
        "missingterm"
    );
}

#[test]
fn known_registry_entries_have_stable_ids() {
    let registry = TerminalRegistry::with_known_terminals();

    assert_eq!(
        registry
            .definitions()
            .iter()
            .map(TerminalDefinition::id)
            .collect::<Vec<_>>(),
        vec![
            "gnome-terminal",
            "gnome-console",
            "konsole",
            "xfce4-terminal",
            "mate-terminal",
            "lxterminal",
            "alacritty",
            "ghostty",
            "kitty",
            "wezterm",
            "foot",
            "xterm",
        ]
    );
}

#[test]
fn custom_registry_entry_resolves_registered_preference() {
    let command = launcher(TerminalPreference::registered("coolterm"), None, None)
        .registry(custom_registry())
        .launch_command(launch_command())
        .resolve_with(path(), available(&["coolterm"]))
        .expect("registered terminal should resolve");

    assert_eq!(command.label, "CoolTerm");
    assert_eq!(command.program, "coolterm");
    assert_eq!(
        command.args,
        vec![
            "--cwd",
            "/tmp/repo with spaces",
            "--",
            "cargo",
            "test",
            "--lib"
        ]
    );
}

#[test]
fn custom_registry_entry_matches_terminal_environment() {
    let command = launcher(
        TerminalPreference::Auto,
        Some("/usr/bin/coolterm"),
        Some("KDE"),
    )
    .registry(custom_registry())
    .resolve_with(path(), available(&["coolterm", "konsole"]))
    .expect("registered terminal should resolve");

    assert_eq!(command.label, "CoolTerm");
    assert_eq!(command.program, "coolterm");
    assert_eq!(command.args, vec!["--cwd", "/tmp/repo with spaces"]);
}

#[test]
fn custom_registry_entry_participates_in_probe_order() {
    let command = launcher(TerminalPreference::Auto, None, None)
        .registry(custom_registry())
        .resolve_with(path(), available(&["coolterm"]))
        .expect("registered terminal should resolve");

    assert_eq!(command.program, "coolterm");
}

#[test]
fn register_first_places_custom_entry_before_known_terminals() {
    let registry = TerminalRegistry::with_known_terminals().register_first(
        TerminalDefinition::new("coolterm", "CoolTerm", "coolterm")
            .working_dir_args(["--cwd", "{path}"]),
    );

    let command = launcher(TerminalPreference::Auto, None, None)
        .registry(registry)
        .resolve_with(path(), available(&["coolterm", "gnome-terminal"]))
        .expect("first registered terminal should resolve");

    assert_eq!(command.program, "coolterm");
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
                "cd -- \"$1\" || exit; exec -- \"${SHELL:-sh}\"",
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
                "cd -- \"$1\" || exit; shift; exec -- \"$@\"",
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
fn auto_prefers_unknown_executable_terminal_environment() {
    let command = launcher(TerminalPreference::Auto, Some("coolterm"), Some("KDE"))
        .resolve_with(
            path(),
            available(&["coolterm", "xdg-terminal-exec", "konsole", "gnome-terminal"]),
        )
        .expect("terminal should resolve");

    assert_eq!(command.program, "coolterm");
    assert!(command.args.is_empty());
}

#[test]
fn auto_preserves_unknown_terminal_environment_args() {
    let command = launcher(
        TerminalPreference::Auto,
        Some("coolterm --title 'Repo Terminal' --cwd {path}"),
        Some("KDE"),
    )
    .resolve_with(path(), available(&["coolterm"]))
    .expect("terminal should resolve");

    assert_eq!(command.program, "coolterm");
    assert_eq!(
        command.args,
        vec!["--title", "Repo Terminal", "--cwd", "{path}"]
    );
}

#[test]
fn auto_ignores_unavailable_unknown_terminal_environment() {
    let command = launcher(
        TerminalPreference::Auto,
        Some("coolterm --new-window"),
        Some("KDE"),
    )
    .resolve_with(path(), available(&["xdg-terminal-exec", "konsole"]))
    .expect("terminal should resolve");

    assert_eq!(command.program, "xdg-terminal-exec");
}

#[test]
fn auto_skips_unknown_terminal_environment_with_launch_command() {
    let command = launcher(TerminalPreference::Auto, Some("coolterm"), Some("KDE"))
        .launch_command(launch_command())
        .resolve_with(path(), available(&["coolterm", "konsole"]))
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
fn auto_rejects_unknown_terminal_environment_with_launch_command_when_no_known_terminal_exists() {
    let error = launcher(TerminalPreference::Auto, Some("coolterm"), None)
        .launch_command(launch_command())
        .resolve_with(path(), available(&["coolterm"]))
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
fn auto_uses_xdg_terminal_exec_before_desktop_heuristic() {
    let command = launcher(TerminalPreference::Auto, None, Some("KDE"))
        .resolve_with(path(), available(&["xdg-terminal-exec", "konsole"]))
        .expect("terminal should resolve");

    assert_eq!(command.program, "xdg-terminal-exec");
    assert_eq!(command.args, vec!["--dir=/tmp/repo with spaces"]);
}

#[test]
fn auto_prefers_xdg_terminal_exec_with_launch_command() {
    let command = launcher(TerminalPreference::Auto, None, Some("KDE"))
        .launch_command(launch_command())
        .resolve_with(path(), available(&["xdg-terminal-exec", "konsole"]))
        .expect("XDG helper should resolve");

    assert_eq!(command.program, "xdg-terminal-exec");
    assert_eq!(
        command.args,
        vec![
            "--dir=/tmp/repo with spaces",
            "--",
            "cargo",
            "test",
            "--lib"
        ]
    );
}

#[test]
fn auto_uses_xdg_terminal_exec_with_launch_command_when_no_known_terminal_exists() {
    let command = launcher(TerminalPreference::Auto, None, None)
        .launch_command(launch_command())
        .resolve_with(path(), available(&["xdg-terminal-exec"]))
        .expect("XDG helper supports commands without a known terminal");

    assert_eq!(command.program, "xdg-terminal-exec");
    assert_eq!(
        command.args,
        vec![
            "--dir=/tmp/repo with spaces",
            "--",
            "cargo",
            "test",
            "--lib"
        ]
    );
}

#[test]
fn system_default_ignores_environment_and_registry_with_or_without_a_command() {
    for launch_command in [None, Some(launch_command())] {
        let mut selected = launcher(
            TerminalPreference::SystemDefault,
            Some("kitty"),
            Some("KDE"),
        );
        selected.launch_command = launch_command;
        let command = selected
            .resolve_with(
                path(),
                available(&[
                    "kitty",
                    "konsole",
                    "xdg-terminal-exec",
                    "x-terminal-emulator",
                ]),
            )
            .expect("system default should use XDG helper");
        assert_eq!(command.program, "xdg-terminal-exec");
        let expected = if selected.launch_command.is_some() {
            vec![
                "--dir=/tmp/repo with spaces",
                "--",
                "cargo",
                "test",
                "--lib",
            ]
        } else {
            vec!["--dir=/tmp/repo with spaces"]
        };
        assert_eq!(command.args, expected);
    }
}

#[test]
fn system_default_falls_back_to_x_terminal_emulator_without_a_command() {
    let command = launcher(
        TerminalPreference::SystemDefault,
        Some("kitty"),
        Some("KDE"),
    )
    .resolve_with(
        path(),
        available(&["kitty", "konsole", "x-terminal-emulator"]),
    )
    .expect("alternatives helper should resolve");
    assert_eq!(command.program, "x-terminal-emulator");
    assert!(command.args.is_empty());
}

#[test]
fn system_default_rejects_commands_with_only_x_terminal_emulator() {
    let error = launcher(
        TerminalPreference::SystemDefault,
        Some("kitty"),
        Some("KDE"),
    )
    .launch_command(launch_command())
    .resolve_with(
        path(),
        available(&["kitty", "konsole", "x-terminal-emulator"]),
    )
    .expect_err("must not guess a terminal or forward commands to alternatives");
    assert!(matches!(
        error,
        LaunchError::UnsupportedLaunchCommand { .. }
    ));
    assert!(
        error
            .to_string()
            .contains("use xdg-terminal-exec to run commands")
    );
}

#[test]
fn system_default_requires_a_helper_even_when_known_terminals_are_available() {
    for launch_command in [None, Some(launch_command())] {
        let mut selected = launcher(
            TerminalPreference::SystemDefault,
            Some("kitty"),
            Some("KDE"),
        );
        selected.launch_command = launch_command;
        let error = selected
            .resolve_with(path(), available(&["kitty", "konsole"]))
            .expect_err("must not substitute an unrelated terminal");
        assert!(matches!(error, LaunchError::MissingTerminal { .. }));
        let guidance = if selected.launch_command.is_some() {
            "install xdg-terminal-exec to run commands"
        } else {
            "install xdg-terminal-exec or x-terminal-emulator"
        };
        assert!(error.to_string().contains(guidance));
    }
}

#[test]
fn explicit_terminal_preferences_override_system_discovery() {
    let cases = [
        (TerminalPreference::Known(KnownTerminal::Kitty), "kitty"),
        (TerminalPreference::registered("coolterm"), "coolterm"),
        (
            TerminalPreference::custom("customterm --new-window"),
            "customterm",
        ),
    ];
    for (preference, expected) in cases {
        let command = launcher(preference, Some("konsole"), Some("KDE"))
            .registry(
                TerminalRegistry::with_known_terminals().register(
                    TerminalDefinition::new("coolterm", "CoolTerm", "coolterm")
                        .launch_command_separator("--"),
                ),
            )
            .resolve_with(
                path(),
                available(&[
                    "kitty",
                    "coolterm",
                    "customterm",
                    "konsole",
                    "xdg-terminal-exec",
                ]),
            )
            .expect("explicit preference should win");
        assert_eq!(command.program, expected);
    }
}

#[test]
fn missing_explicit_terminal_does_not_fall_back_to_system_helpers() {
    for preference in [
        TerminalPreference::Known(KnownTerminal::Kitty),
        TerminalPreference::registered("coolterm"),
    ] {
        let error = launcher(preference, None, Some("KDE"))
            .registry(
                TerminalRegistry::with_known_terminals().register(
                    TerminalDefinition::new("coolterm", "CoolTerm", "coolterm")
                        .launch_command_separator("--"),
                ),
            )
            .resolve_with(
                path(),
                available(&["konsole", "xdg-terminal-exec", "x-terminal-emulator"]),
            )
            .expect_err("missing explicit preference must fail");
        assert!(matches!(error, LaunchError::MissingTerminal { .. }));
    }
}

#[test]
fn auto_still_prefers_terminal_environment_when_running_commands() {
    let command = launcher(TerminalPreference::Auto, Some("kitty"), Some("KDE"))
        .launch_command(launch_command())
        .resolve_with(
            path(),
            available(&["kitty", "konsole", "xdg-terminal-exec"]),
        )
        .expect("TERMINAL keeps precedence in Auto");
    assert_eq!(command.program, "kitty");
}

#[test]
fn system_default_has_a_distinct_label() {
    assert_eq!(
        terminal_label(TerminalPreference::SystemDefault),
        "System default terminal"
    );
    assert_eq!(
        TerminalRegistry::default().label_for_preference(&TerminalPreference::SystemDefault),
        "System default terminal"
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
    let command = launcher(TerminalPreference::Auto, None, None)
        .custom_command("my-terminal --working-directory \"{path}\" --flag")
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
    let command = launcher(
        TerminalPreference::custom("my-terminal --new-window"),
        None,
        None,
    )
    .resolve_with(path(), available(&[]))
    .expect("custom command should parse");

    assert_eq!(command.program, "my-terminal");
    assert_eq!(command.args, vec!["--new-window"]);
}

#[test]
fn custom_command_keeps_quoted_arguments_together() {
    let command = launcher(TerminalPreference::Auto, None, None)
        .custom_command("my-terminal --title 'Repo Terminal'")
        .resolve_with(path(), available(&[]))
        .expect("custom command should parse");

    assert_eq!(command.args, vec!["--title", "Repo Terminal"]);
}

#[test]
fn custom_command_rejects_unterminated_quotes() {
    let error = launcher(TerminalPreference::Auto, None, None)
        .custom_command("my-terminal --title \"Repo")
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
    let error = launcher(TerminalPreference::Auto, None, None)
        .custom_command("   ")
        .resolve_with(path(), available(&[]))
        .expect_err("custom command should fail");

    assert!(matches!(error, LaunchError::EmptyCustomCommand));
    assert_eq!(error.to_string(), "Custom terminal command cannot be empty");
}

#[test]
fn custom_command_rejects_launch_command() {
    let error = launcher(TerminalPreference::Auto, None, None)
        .custom_command("my-terminal --working-directory \"{path}\"")
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
fn preference_after_custom_command_takes_precedence() {
    let command = launcher(TerminalPreference::Auto, None, None)
        .custom_command("my-terminal --new-window")
        .preference(TerminalPreference::Known(KnownTerminal::Konsole))
        .resolve_with(path(), available(&["konsole"]))
        .expect("last preference should resolve");

    assert_eq!(command.program, "konsole");
}

#[test]
fn unrecognised_terminal_preferences_fail_without_auto_fallback() {
    let error = launcher(TerminalPreference::Unrecognised, None, None)
        .resolve_with(path(), available(&["konsole"]))
        .expect_err("unrecognised preference should fail");

    assert!(matches!(error, LaunchError::UnsupportedTerminalPreference));

    let error = launcher(
        TerminalPreference::Known(KnownTerminal::Unrecognised),
        None,
        None,
    )
    .resolve_with(path(), available(&["konsole"]))
    .expect_err("unrecognised terminal should fail");

    assert!(matches!(error, LaunchError::UnsupportedTerminalPreference));
}

#[cfg(feature = "serde")]
#[test]
fn serde_uses_stable_terminal_preference_identifiers() {
    let cases = [
        (TerminalPreference::Auto, r#"{"auto":null}"#),
        (
            TerminalPreference::SystemDefault,
            r#"{"system-default":null}"#,
        ),
        (
            TerminalPreference::Known(KnownTerminal::GnomeTerminal),
            r#"{"known":"gnome-terminal"}"#,
        ),
        (
            TerminalPreference::registered("coolterm"),
            r#"{"registered":"coolterm"}"#,
        ),
        (
            TerminalPreference::custom("coolterm --cwd {path}"),
            r#"{"custom":"coolterm --cwd {path}"}"#,
        ),
    ];

    for (preference, json) in cases {
        assert_eq!(serde_json::to_string(&preference).unwrap(), json);
        assert_eq!(
            serde_json::from_str::<TerminalPreference>(json).unwrap(),
            preference
        );
    }
}

#[cfg(feature = "serde")]
#[test]
fn serde_rejects_unrecognised_preferences_when_serialising() {
    let preference = serde_json::from_str::<TerminalPreference>(r#"{"future":null}"#)
        .expect("unknown preference kind should deserialise");
    assert_eq!(preference, TerminalPreference::Unrecognised);
    assert!(serde_json::to_string(&preference).is_err());

    let preference = serde_json::from_str::<TerminalPreference>(
        r#"{"future":{"preserved-by-newer-version":true}}"#,
    )
    .expect("unknown preference with a value should deserialise");
    assert_eq!(preference, TerminalPreference::Unrecognised);
    assert!(serde_json::to_string(&preference).is_err());

    let terminal = serde_json::from_str::<KnownTerminal>(r#""future-terminal""#)
        .expect("unknown terminal should deserialise");
    assert_eq!(terminal, KnownTerminal::Unrecognised);
    assert!(serde_json::to_string(&terminal).is_err());

    let preference = serde_json::from_str::<TerminalPreference>(r#"{"known":"future-terminal"}"#)
        .expect("unknown known terminal should deserialise");
    assert_eq!(
        preference,
        TerminalPreference::Known(KnownTerminal::Unrecognised)
    );
    assert!(serde_json::to_string(&preference).is_err());
}

#[cfg(feature = "serde")]
#[test]
fn serde_rejects_malformed_known_terminal_preferences() {
    assert!(serde_json::from_str::<TerminalPreference>(r#"{"system-default":1}"#).is_err());
    assert!(
        serde_json::from_str::<TerminalPreference>(r#"{"system-default":null,"auto":null}"#)
            .is_err()
    );
    assert!(serde_json::from_str::<TerminalPreference>(r#"{"known":1}"#).is_err());
    assert!(
        serde_json::from_str::<TerminalPreference>(r#"{"auto":null,"known":"konsole"}"#).is_err()
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
