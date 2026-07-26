use super::collect_output_until_exit;
use super::combine_spawned_output;
use super::find_python;
use super::wait_for_output_contains;
use crate::TerminalSize;
use crate::spawn_pty_process;
use std::collections::HashMap;
use std::path::Path;

const READY_MARKER: &str = "__CODEX_CHILD_READY__";
const VALUE_MARKER: &str = "__CODEX_CHILD_VALUE__";
static CONPTY_INTERACTIVE_TEST_PERMIT: tokio::sync::Semaphore =
    tokio::sync::Semaphore::const_new(1);

struct WindowsShell {
    name: &'static str,
    program: String,
    args: Vec<String>,
    child_command: String,
}

fn find_powershell() -> Option<String> {
    ["pwsh.exe", "powershell.exe"]
        .into_iter()
        .find_map(|candidate| {
            std::process::Command::new(candidate)
                .args(["-NoLogo", "-NoProfile", "-Command", "exit 0"])
                .status()
                .ok()
                .filter(std::process::ExitStatus::success)
                .map(|_| candidate.to_string())
        })
}

fn utf8_hex(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

async fn exercise_shell_input(
    shell: &WindowsShell,
    env: &HashMap<String, String>,
    expected: &str,
    expected_marker: &str,
) -> anyhow::Result<()> {
    let spawned = spawn_pty_process(
        &shell.program,
        &shell.args,
        Path::new("."),
        env,
        /*arg0*/ &None,
        TerminalSize::default(),
    )
    .await?;
    let (session, mut output_rx, exit_rx) = combine_spawned_output(spawned);
    let writer = session.writer_sender();
    writer
        .send(format!("{}\n", shell.child_command).into_bytes())
        .await?;
    wait_for_output_contains(&mut output_rx, READY_MARKER, /*timeout_ms*/ 10_000)
        .await
        .map_err(|err| anyhow::anyhow!("{} child did not become ready: {err}", shell.name))?;

    writer
        .send(format!("{expected}X\u{8}\n").into_bytes())
        .await?;
    let mut output =
        wait_for_output_contains(&mut output_rx, expected_marker, /*timeout_ms*/ 10_000)
            .await
            .map_err(|err| {
                anyhow::anyhow!("{} child received incorrect input: {err}", shell.name)
            })?;

    writer.send(b"exit 0\n".to_vec()).await?;
    let (remaining, exit_code) =
        collect_output_until_exit(output_rx, exit_rx, /*timeout_ms*/ 10_000).await;
    output.extend_from_slice(&remaining);

    assert_eq!(
        exit_code,
        0,
        "{} did not exit cleanly: {:?}",
        shell.name,
        String::from_utf8_lossy(&output)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conpty_delivers_input_to_foreground_children() -> anyhow::Result<()> {
    // Hosted Windows runners can cross-route input between concurrent ConPTY
    // sessions, so keep the interactive shell contracts process-serial.
    let _serial = CONPTY_INTERACTIVE_TEST_PERMIT
        .acquire()
        .await
        .expect("ConPTY test permit remains open");
    let Some(python) = find_python() else {
        eprintln!("python not found; skipping ConPTY input test");
        return Ok(());
    };
    let code = format!(
        "print('__CODEX_CHILD_'+'READY__', flush=True); value=input(); print('{VALUE_MARKER}'+value.encode('utf-8').hex(), flush=True)"
    );
    let expected = "cafeé 漢字";
    let expected_marker = format!("{VALUE_MARKER}{}", utf8_hex(expected));
    let mut shells = vec![WindowsShell {
        name: "cmd",
        program: std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()),
        args: vec!["/D".to_string(), "/Q".to_string()],
        child_command: format!("\"{}\" -u -c \"{code}\"", python.replace('"', "\"\"")),
    }];
    if let Some(program) = find_powershell() {
        shells.push(WindowsShell {
            name: "PowerShell",
            program,
            args: vec!["-NoLogo".to_string(), "-NoProfile".to_string()],
            child_command: format!("& '{}' -u -c \"{code}\"", python.replace('\'', "''")),
        });
    }
    let env: HashMap<String, String> = std::env::vars().collect();

    for shell in shells {
        if let Err(first_error) =
            exercise_shell_input(&shell, &env, expected, &expected_marker).await
        {
            eprintln!(
                "{} ConPTY input attempt failed, retrying once: {first_error:#}",
                shell.name
            );
            tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
            exercise_shell_input(&shell, &env, expected, &expected_marker)
                .await
                .map_err(|retry_error| {
                    anyhow::anyhow!(
                        "{} ConPTY input failed twice; first: {first_error:#}; retry: {retry_error:#}",
                        shell.name
                    )
                })?;
        }
    }

    Ok(())
}

async fn exercise_powershell_ctrl_c(
    program: &str,
    env: &HashMap<String, String>,
) -> anyhow::Result<()> {
    let args = vec!["-NoLogo".to_string(), "-NoProfile".to_string()];
    let spawned = spawn_pty_process(
        program,
        &args,
        Path::new("."),
        env,
        /*arg0*/ &None,
        TerminalSize::default(),
    )
    .await?;
    let (session, mut output_rx, exit_rx) = combine_spawned_output(spawned);
    let writer = session.writer_sender();
    writer.send(b"ping.exe -4 -t localhost\n".to_vec()).await?;
    wait_for_output_contains(&mut output_rx, "127.0.0.1", /*timeout_ms*/ 10_000).await?;
    wait_for_output_contains(&mut output_rx, "127.0.0.1", /*timeout_ms*/ 10_000).await?;

    writer.send(vec![0x03]).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    writer.send(b"cmd.exe /D /C ver\n".to_vec()).await?;
    let mut output = wait_for_output_contains(
        &mut output_rx,
        "Microsoft Windows",
        /*timeout_ms*/ 10_000,
    )
    .await?;

    writer.send(b"exit 0\n".to_vec()).await?;
    let (remaining, exit_code) =
        collect_output_until_exit(output_rx, exit_rx, /*timeout_ms*/ 10_000).await;
    output.extend_from_slice(&remaining);
    anyhow::ensure!(
        exit_code == 0,
        "PowerShell did not resume after Ctrl-C: {:?}",
        String::from_utf8_lossy(&output)
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conpty_ctrl_c_interrupts_powershell_foreground_child() -> anyhow::Result<()> {
    let _serial = CONPTY_INTERACTIVE_TEST_PERMIT
        .acquire()
        .await
        .expect("ConPTY test permit remains open");
    let Some(program) = find_powershell() else {
        return Ok(());
    };
    let env: HashMap<String, String> = std::env::vars().collect();

    if let Err(first_error) = exercise_powershell_ctrl_c(&program, &env).await {
        eprintln!("PowerShell Ctrl-C attempt failed, retrying once: {first_error:#}");
        tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
        exercise_powershell_ctrl_c(&program, &env)
            .await
            .map_err(|retry_error| {
                anyhow::anyhow!(
                    "PowerShell Ctrl-C failed twice; first: {first_error:#}; retry: {retry_error:#}"
                )
            })?;
    }

    Ok(())
}
