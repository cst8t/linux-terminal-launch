#![cfg(unix)]

use linux_terminal_launch::{LaunchCommand, TerminalLauncher, TerminalPreference};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// Run the public API in a subprocess so each case has an isolated environment.
// Mutating PATH in the test runner would race with other tests.
#[test]
fn spawn_in_isolated_environment() {
    let Some(preference) = std::env::var_os("LTL_TEST_PREFERENCE") else {
        return;
    };
    let preference = match preference.to_str().unwrap() {
        "auto" => TerminalPreference::Auto,
        "system-default" => TerminalPreference::SystemDefault,
        _ => panic!("unexpected test preference"),
    };
    let directory = PathBuf::from(std::env::var_os("LTL_TEST_DIRECTORY").unwrap());
    TerminalLauncher::new()
        .preference(preference)
        .working_dir(directory)
        .launch_command(LaunchCommand::new("--program with spaces").args([
            "",
            "two words",
            "'quoted' \"double quoted\"",
            "$(touch expanded); touch expanded; $HOME *",
            "line one\nline two",
            "--title=child option",
        ]))
        .spawn()
        .expect("fake helper should launch");
}

struct TestDirectory(PathBuf);

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn xdg_spawn_preserves_arguments_and_working_directory() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp = TestDirectory(std::env::temp_dir().join(format!(
        "linux-terminal-launch-{}-{unique}",
        std::process::id()
    )));
    let bin = temp.0.join("bin");
    let directory = temp.0.join("project with spaces");
    fs::create_dir_all(&bin).unwrap();
    fs::create_dir(&directory).unwrap();
    let helper = bin.join("xdg-terminal-exec");
    fs::write(
        &helper,
        "#!/bin/sh\nset -eu\nprintf '%s\\0' \"$PWD\" \"$@\" > \"$LTL_TEST_CAPTURE.tmp\"\n/bin/mv \"$LTL_TEST_CAPTURE.tmp\" \"$LTL_TEST_CAPTURE\"\n",
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();

    for preference in ["auto", "system-default"] {
        let capture = temp.0.join(format!("{preference}.capture"));
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "spawn_in_isolated_environment", "--nocapture"])
            .env("PATH", &bin)
            .env("TERMINAL", "unavailable-terminal")
            .env("XDG_CURRENT_DESKTOP", "KDE")
            .env("LTL_TEST_PREFERENCE", preference)
            .env("LTL_TEST_DIRECTORY", &directory)
            .env("LTL_TEST_CAPTURE", &capture)
            .output()
            .unwrap();
        assert!(output.status.success(), "child test failed: {output:?}");

        // spawn() is intentionally asynchronous. The helper renames its capture
        // after writing, so observing the final file means the record is complete.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !capture.exists() {
            assert!(
                Instant::now() < deadline,
                "helper did not record its arguments"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let record = fs::read(&capture).unwrap();
        let directory = directory.to_str().unwrap();
        let expected = [
            directory.to_string(),
            format!("--dir={directory}"),
            "--".to_string(),
            "--program with spaces".to_string(),
            "".to_string(),
            "two words".to_string(),
            "'quoted' \"double quoted\"".to_string(),
            "$(touch expanded); touch expanded; $HOME *".to_string(),
            "line one\nline two".to_string(),
            "--title=child option".to_string(),
        ];
        let expected = expected
            .iter()
            .flat_map(|arg| arg.bytes().chain([0]))
            .collect::<Vec<_>>();
        assert_eq!(
            record, expected,
            "argument boundaries changed for {preference}"
        );
        assert!(!PathBuf::from(directory).join("expanded").exists());
    }
}
