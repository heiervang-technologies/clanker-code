//! Polls the cross-process `say` playback signal for the current agent.

use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde::Deserialize;

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const MAX_SIGNAL_AGE: Duration = Duration::from_secs(120);

#[derive(Debug, Deserialize)]
struct TalkingFlag {
    started: u64,
    pid: u32,
}

#[derive(Debug)]
pub(crate) struct TalkingSignal {
    path: Option<PathBuf>,
    last_check: Option<Instant>,
    cached_active: bool,
}

impl TalkingSignal {
    pub(crate) fn new() -> Self {
        let path = resolve_agent_name().and_then(|agent_name| {
            dirs::home_dir().map(|home| {
                home.join(".local")
                    .join("share")
                    .join("agent-tools")
                    .join("talking")
                    .join(format!("{agent_name}.json"))
            })
        });
        Self {
            path,
            last_check: None,
            cached_active: false,
        }
    }

    pub(crate) fn is_active(&mut self) -> bool {
        let now = Instant::now();
        if self
            .last_check
            .is_some_and(|last_check| now.saturating_duration_since(last_check) < POLL_INTERVAL)
        {
            return self.cached_active;
        }
        self.last_check = Some(now);
        self.cached_active = self.path.as_ref().is_some_and(|path| flag_is_active(path));
        self.cached_active
    }

    pub(crate) fn poll_interval() -> Duration {
        POLL_INTERVAL
    }

    #[cfg(test)]
    pub(crate) fn set_active_for_tests(&mut self, active: bool) {
        self.last_check = Some(Instant::now());
        self.cached_active = active;
    }
}

fn resolve_agent_name() -> Option<String> {
    crate::agent_name::resolve()
}

fn flag_is_active(path: &std::path::Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let Ok(flag) = serde_json::from_slice::<TalkingFlag>(&bytes) else {
        return false;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if flag.started > now.saturating_add(/*rhs*/ 5)
        || now.saturating_sub(flag.started) > MAX_SIGNAL_AGE.as_secs()
    {
        return false;
    }
    process_is_alive(flag.pid)
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    if pid <= 0 {
        return false;
    }
    let result = unsafe {
        libc::kill(pid, /*sig*/ 0)
    };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn process_is_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::GetExitCodeProcess;
    use windows_sys::Win32::System::Threading::OpenProcess;
    use windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION;

    const STILL_ACTIVE: u32 = 259;
    if pid == 0 {
        return false;
    }
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION,
            /*bInheritHandle*/ 0,
            pid,
        )
    };
    if handle == 0 {
        return false;
    }
    let mut exit_code = 0;
    let success = unsafe { GetExitCodeProcess(handle, &mut exit_code) } != 0;
    unsafe { CloseHandle(handle) };
    success && exit_code == STILL_ACTIVE
}

#[cfg(test)]
#[path = "talking_signal_tests.rs"]
mod tests;
