// Modified by Heiervang Technologies.
use crate::logging;
use crate::runner_client::connect_pipe_with_timeout;
use crate::runner_pipe::PIPE_ACCESS_INBOUND;
use crate::runner_pipe::PIPE_ACCESS_OUTBOUND;
use crate::runner_pipe::create_named_pipe_for_sid;
use crate::runner_pipe::pipe_pair;
use crate::token::LocalSid;
use crate::token::get_current_token_for_restriction;
use crate::token::get_logon_sid_bytes;
use crate::token::get_user_sid_bytes;
use crate::winutil::format_last_error;
use crate::winutil::quote_windows_arg;
use crate::winutil::string_from_sid_bytes;
use crate::winutil::to_wide;
use anyhow::Context;
use anyhow::Result;
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use std::ffi::c_void;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::os::windows::io::AsRawHandle;
use std::os::windows::io::FromRawHandle;
use std::path::Path;
use std::ptr;
use std::time::Duration;
use std::time::Instant;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Foundation::HLOCAL;
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
use windows_sys::Win32::Security::LOGON32_LOGON_NEW_CREDENTIALS;
use windows_sys::Win32::Security::LOGON32_PROVIDER_WINNT50;
use windows_sys::Win32::Security::LogonUserW;
use windows_sys::Win32::Security::PSECURITY_DESCRIPTOR;
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::CreateFileW;
use windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_READ;
use windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_WRITE;
use windows_sys::Win32::Storage::FileSystem::OPEN_EXISTING;
use windows_sys::Win32::System::Pipes::PeekNamedPipe;
use windows_sys::Win32::System::StationsAndDesktops::CloseDesktop;
use windows_sys::Win32::System::StationsAndDesktops::CloseWindowStation;
use windows_sys::Win32::System::StationsAndDesktops::CreateDesktopW;
use windows_sys::Win32::System::StationsAndDesktops::CreateWindowStationW;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_CREATEMENU;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_CREATEWINDOW;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_DELETE;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_ENUMERATE;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_HOOKCONTROL;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_JOURNALPLAYBACK;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_JOURNALRECORD;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_READ_CONTROL;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_READOBJECTS;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_SWITCHDESKTOP;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_WRITE_DAC;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_WRITE_OWNER;
use windows_sys::Win32::System::StationsAndDesktops::DESKTOP_WRITEOBJECTS;
use windows_sys::Win32::System::StationsAndDesktops::GetProcessWindowStation;
use windows_sys::Win32::System::StationsAndDesktops::GetThreadDesktop;
use windows_sys::Win32::System::StationsAndDesktops::GetUserObjectInformationW;
use windows_sys::Win32::System::StationsAndDesktops::SetProcessWindowStation;
use windows_sys::Win32::System::StationsAndDesktops::SetThreadDesktop;
use windows_sys::Win32::System::StationsAndDesktops::UOI_NAME;
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
use windows_sys::Win32::System::Threading::CREATE_UNICODE_ENVIRONMENT;
use windows_sys::Win32::System::Threading::CreateProcessAsUserW;
use windows_sys::Win32::System::Threading::CreateProcessWithLogonW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::System::Threading::LOGON_NETCREDENTIALS_ONLY;
use windows_sys::Win32::System::Threading::PROCESS_INFORMATION;
use windows_sys::Win32::System::Threading::STARTUPINFOW;
use windows_sys::Win32::System::Threading::TerminateProcess;
use windows_sys::Win32::System::Threading::WaitForSingleObject;
use windows_sys::Win32::UI::WindowsAndMessaging::CWF_CREATE_ONLY;
use windows_sys::Win32::UI::WindowsAndMessaging::WINSTA_ACCESSCLIPBOARD;
use windows_sys::Win32::UI::WindowsAndMessaging::WINSTA_ACCESSGLOBALATOMS;
use windows_sys::Win32::UI::WindowsAndMessaging::WINSTA_CREATEDESKTOP;
use windows_sys::Win32::UI::WindowsAndMessaging::WINSTA_ENUMDESKTOPS;
use windows_sys::Win32::UI::WindowsAndMessaging::WINSTA_ENUMERATE;
use windows_sys::Win32::UI::WindowsAndMessaging::WINSTA_EXITWINDOWS;
use windows_sys::Win32::UI::WindowsAndMessaging::WINSTA_READATTRIBUTES;
use windows_sys::Win32::UI::WindowsAndMessaging::WINSTA_READSCREEN;
use windows_sys::Win32::UI::WindowsAndMessaging::WINSTA_WRITEATTRIBUTES;

#[doc(hidden)]
pub const DESKTOP_BROKER_ARG: &str = "--codex-windows-desktop-broker";

const PIPE_IN_ARG: &str = "--pipe-in=";
const PIPE_OUT_ARG: &str = "--pipe-out=";
const LAUNCH_SID_ARG: &str = "--launch-sid=";
const CHILD_LOGON_SID_ARG: &str = "--child-logon-sid=";
const BROKER_READY_TIMEOUT: Duration = Duration::from_secs(15);
const BROKER_EXIT_TIMEOUT_MS: u32 = 5_000;
const WAIT_OBJECT_0: u32 = 0;
const LOCAL_SYSTEM_SID: &str = "S-1-5-18";
const DESKTOP_ALL_ACCESS: u32 = DESKTOP_READOBJECTS
    | DESKTOP_CREATEWINDOW
    | DESKTOP_CREATEMENU
    | DESKTOP_HOOKCONTROL
    | DESKTOP_JOURNALRECORD
    | DESKTOP_JOURNALPLAYBACK
    | DESKTOP_ENUMERATE
    | DESKTOP_WRITEOBJECTS
    | DESKTOP_SWITCHDESKTOP
    | DESKTOP_DELETE
    | DESKTOP_READ_CONTROL
    | DESKTOP_WRITE_DAC
    | DESKTOP_WRITE_OWNER;
const DESKTOP_BROKER_CHILD_ACCESS: u32 = DESKTOP_READOBJECTS
    | DESKTOP_CREATEWINDOW
    | DESKTOP_CREATEMENU
    | DESKTOP_HOOKCONTROL
    | DESKTOP_JOURNALRECORD
    | DESKTOP_JOURNALPLAYBACK
    | DESKTOP_ENUMERATE
    | DESKTOP_WRITEOBJECTS
    | DESKTOP_READ_CONTROL;
const WINDOW_STATION_ALL_ACCESS: u32 = WINSTA_ACCESSCLIPBOARD as u32
    | WINSTA_ACCESSGLOBALATOMS as u32
    | WINSTA_CREATEDESKTOP as u32
    | WINSTA_ENUMDESKTOPS as u32
    | WINSTA_ENUMERATE as u32
    | WINSTA_EXITWINDOWS as u32
    | WINSTA_READATTRIBUTES as u32
    | WINSTA_READSCREEN as u32
    | WINSTA_WRITEATTRIBUTES as u32
    | 0x000F_0000;

pub struct LaunchDesktop {
    _broker: Option<DesktopBroker>,
    startup_name: Option<Vec<u16>>,
}

impl LaunchDesktop {
    pub fn prepare(
        launch_sid: &LocalSid,
        broker_executable: &Path,
        use_private_desktop: bool,
        logs_base_dir: Option<&Path>,
    ) -> Result<Self> {
        if !use_private_desktop {
            anyhow::bail!("restricted Windows launches require a private desktop");
        }
        let broker = DesktopBroker::spawn(launch_sid, broker_executable, logs_base_dir)?;
        let startup_name = to_wide(&broker.desktop_name);
        Ok(Self {
            _broker: Some(broker),
            startup_name: Some(startup_name),
        })
    }

    pub fn startup_info_desktop(&self) -> *mut u16 {
        self.startup_name
            .as_ref()
            .map_or(ptr::null_mut(), |name| name.as_ptr() as *mut u16)
    }
}

struct OwnedHandle(HANDLE);

impl OwnedHandle {
    fn new(handle: HANDLE) -> Self {
        Self(handle)
    }

    fn raw(&self) -> HANDLE {
        self.0
    }

    fn into_raw(mut self) -> HANDLE {
        let handle = self.0;
        self.0 = 0;
        handle
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if self.0 != 0 && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

struct OwnedSecurityDescriptor(PSECURITY_DESCRIPTOR);

impl OwnedSecurityDescriptor {
    fn for_desktop_broker(
        broker_logon_sid: &str,
        child_logon_sid: &LocalSid,
        launch_sid: &LocalSid,
    ) -> Result<Self> {
        let sddl = to_wide(desktop_broker_sddl(
            broker_logon_sid,
            child_logon_sid.as_str(),
            launch_sid.as_str(),
        ));
        let mut descriptor = ptr::null_mut();
        let converted = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1, // SDDL_REVISION_1
                &mut descriptor,
                ptr::null_mut(),
            )
        };
        if converted == 0 {
            anyhow::bail!(
                "create desktop broker security descriptor failed: {}",
                unsafe { GetLastError() }
            );
        }
        Ok(Self(descriptor))
    }

    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }
}

fn desktop_broker_sddl(broker_logon_sid: &str, child_logon_sid: &str, launch_sid: &str) -> String {
    format!(
        "D:P(A;;GA;;;{broker_logon_sid})(A;;0x{DESKTOP_BROKER_CHILD_ACCESS:08x};;;{child_logon_sid})(A;;0x{DESKTOP_BROKER_CHILD_ACCESS:08x};;;{launch_sid})"
    )
}

impl Drop for OwnedSecurityDescriptor {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0 as HLOCAL);
            }
        }
    }
}

struct DesktopBroker {
    hold_pipe: Option<File>,
    process_handle: HANDLE,
    desktop_name: String,
}

impl DesktopBroker {
    fn spawn(
        launch_sid: &LocalSid,
        broker_executable: &Path,
        logs_base_dir: Option<&Path>,
    ) -> Result<Self> {
        let current_token = OwnedHandle::new(unsafe { get_current_token_for_restriction()? });
        let current_user_sid = unsafe { get_user_sid_bytes(current_token.raw())? };
        let current_user_sid =
            string_from_sid_bytes(&current_user_sid).map_err(anyhow::Error::msg)?;
        let child_logon_sid = unsafe { get_logon_sid_bytes(current_token.raw())? };
        let child_logon_sid =
            string_from_sid_bytes(&child_logon_sid).map_err(anyhow::Error::msg)?;
        let (pipe_in_name, pipe_out_name) = pipe_pair();
        let pipe_in = OwnedHandle::new(create_named_pipe_for_sid(
            &pipe_in_name,
            PIPE_ACCESS_OUTBOUND,
            &current_user_sid,
        )?);
        let pipe_out = OwnedHandle::new(create_named_pipe_for_sid(
            &pipe_out_name,
            PIPE_ACCESS_INBOUND,
            &current_user_sid,
        )?);

        let broker_executable = broker_executable
            .to_str()
            .context("desktop broker executable path is not UTF-8")?;
        let command = format!(
            "{} {} {} {} {} {}",
            quote_windows_arg(broker_executable),
            DESKTOP_BROKER_ARG,
            quote_windows_arg(&format!("{PIPE_IN_ARG}{pipe_in_name}")),
            quote_windows_arg(&format!("{PIPE_OUT_ARG}{pipe_out_name}")),
            quote_windows_arg(&format!("{LAUNCH_SID_ARG}{}", launch_sid.as_str())),
            quote_windows_arg(&format!("{CHILD_LOGON_SID_ARG}{child_logon_sid}")),
        );
        let mut command = to_wide(command);
        let executable = to_wide(broker_executable);
        let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
        startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        // LOGON_NETCREDENTIALS_ONLY retains the caller's local token while
        // assigning the broker a fresh authentication LUID. The synthetic
        // credentials are never validated or used by the broker.
        let network_user = to_wide("CodexDesktopBroker");
        let network_domain = to_wide(".");
        let network_password = to_wide(format!(
            "CodexDesktopBroker-{:x}",
            SmallRng::from_entropy().r#gen::<u128>()
        ));
        let environment = crate::process::make_env_block(
            &std::env::vars_os()
                .filter_map(|(key, value)| {
                    Some((key.into_string().ok()?, value.into_string().ok()?))
                })
                .collect::<std::collections::HashMap<_, _>>(),
        );
        let creation_flags = CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT;
        let (created, creation_api) = if is_local_system_sid(&current_user_sid) {
            let mut logon_token = 0;
            let logged_on = unsafe {
                LogonUserW(
                    network_user.as_ptr(),
                    network_domain.as_ptr(),
                    network_password.as_ptr(),
                    LOGON32_LOGON_NEW_CREDENTIALS,
                    LOGON32_PROVIDER_WINNT50,
                    &mut logon_token,
                )
            };
            if logged_on == 0 {
                anyhow::bail!("LogonUserW(desktop broker) failed: {}", unsafe {
                    GetLastError()
                });
            }
            let logon_token = OwnedHandle::new(logon_token);
            let created = unsafe {
                CreateProcessAsUserW(
                    logon_token.raw(),
                    executable.as_ptr(),
                    command.as_mut_ptr(),
                    ptr::null(),
                    ptr::null(),
                    0,
                    creation_flags,
                    environment.as_ptr() as *const c_void,
                    ptr::null(),
                    &startup,
                    &mut process,
                )
            };
            (created, "CreateProcessAsUserW")
        } else {
            let created = unsafe {
                CreateProcessWithLogonW(
                    network_user.as_ptr(),
                    network_domain.as_ptr(),
                    network_password.as_ptr(),
                    LOGON_NETCREDENTIALS_ONLY,
                    executable.as_ptr(),
                    command.as_mut_ptr(),
                    creation_flags,
                    environment.as_ptr() as *const c_void,
                    ptr::null(),
                    &startup,
                    &mut process,
                )
            };
            (created, "CreateProcessWithLogonW")
        };
        if created == 0 {
            return Err(anyhow::anyhow!(
                "{creation_api}(desktop broker) failed: {}",
                unsafe { GetLastError() }
            ));
        }
        let process_handle = OwnedHandle::new(process.hProcess);
        unsafe {
            if process.hThread != 0 {
                CloseHandle(process.hThread);
            }
        }

        let connected = (|| -> Result<()> {
            connect_pipe_with_timeout(pipe_in.raw(), process.dwProcessId, "desktop-broker-in")?;
            connect_pipe_with_timeout(pipe_out.raw(), process.dwProcessId, "desktop-broker-out")
        })();
        if let Err(err) = connected {
            unsafe {
                let _ = TerminateProcess(process_handle.raw(), 1);
            }
            return Err(err);
        }

        let hold_pipe = unsafe { File::from_raw_handle(pipe_in.into_raw() as *mut c_void) };
        let ready_pipe = unsafe { File::from_raw_handle(pipe_out.into_raw() as *mut c_void) };
        let startup_result = (|| -> Result<String> {
            wait_for_broker_ready(&ready_pipe, process_handle.raw())?;
            let mut line = String::new();
            BufReader::new(ready_pipe).read_line(&mut line)?;
            let line = line.trim_end_matches(['\r', '\n']);
            if let Some(message) = line.strip_prefix("ERR\t") {
                anyhow::bail!("desktop broker failed: {message}");
            }
            let desktop_name = line
                .strip_prefix("OK\t")
                .context("desktop broker returned an invalid ready message")?
                .to_string();
            validate_broker_desktop_name(&desktop_name)?;
            Ok(desktop_name)
        })();
        let desktop_name = match startup_result {
            Ok(desktop_name) => desktop_name,
            Err(err) => {
                unsafe {
                    let _ = TerminateProcess(process_handle.raw(), 1);
                }
                return Err(err);
            }
        };
        logging::log_note(
            &format!("desktop broker ready: {desktop_name}"),
            logs_base_dir,
        );

        Ok(Self {
            hold_pipe: Some(hold_pipe),
            process_handle: process_handle.into_raw(),
            desktop_name,
        })
    }
}

impl Drop for DesktopBroker {
    fn drop(&mut self) {
        self.hold_pipe.take();
        if self.process_handle == 0 || self.process_handle == INVALID_HANDLE_VALUE {
            return;
        }
        unsafe {
            if WaitForSingleObject(self.process_handle, BROKER_EXIT_TIMEOUT_MS) != WAIT_OBJECT_0 {
                let _ = TerminateProcess(self.process_handle, 1);
            }
            CloseHandle(self.process_handle);
        }
    }
}

fn wait_for_broker_ready(pipe: &File, process_handle: HANDLE) -> Result<()> {
    let pipe_handle = pipe.as_raw_handle() as HANDLE;
    let deadline = Instant::now() + BROKER_READY_TIMEOUT;
    loop {
        let mut available = 0;
        let ok = unsafe {
            PeekNamedPipe(
                pipe_handle,
                ptr::null_mut(),
                0,
                ptr::null_mut(),
                &mut available,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(anyhow::anyhow!(
                "PeekNamedPipe(desktop broker) failed: {}",
                unsafe { GetLastError() }
            ));
        }
        if available > 0 {
            return Ok(());
        }
        if unsafe { WaitForSingleObject(process_handle, 0) } == WAIT_OBJECT_0 {
            anyhow::bail!("desktop broker exited before reporting ready");
        }
        if Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for desktop broker readiness");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn validate_broker_desktop_name(name: &str) -> Result<()> {
    let (station, desktop) = name
        .split_once('\\')
        .context("desktop broker name must contain one station separator")?;
    let desktop_nonce = desktop.strip_prefix("CodexSandboxDesktop-");
    if !is_valid_station_component(station)
        || !desktop_nonce.is_some_and(is_hex_nonce)
        || desktop.contains('\\')
    {
        anyhow::bail!("desktop broker returned an invalid private desktop name");
    }
    Ok(())
}

fn is_valid_station_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value
            .chars()
            .any(|character| character.is_control() || matches!(character, '\\' | '/'))
}

fn is_local_system_sid(value: &str) -> bool {
    value.eq_ignore_ascii_case(LOCAL_SYSTEM_SID)
}

fn is_hex_nonce(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

struct BrokerDesktop {
    desktop_handle: isize,
    station_handle: isize,
    qualified_name: String,
}

impl BrokerDesktop {
    fn create(child_logon_sid: &LocalSid, launch_sid: &LocalSid) -> Result<Self> {
        let current_token = OwnedHandle::new(unsafe { get_current_token_for_restriction()? });
        let broker_logon_sid = unsafe { get_logon_sid_bytes(current_token.raw())? };
        let broker_logon_sid =
            string_from_sid_bytes(&broker_logon_sid).map_err(anyhow::Error::msg)?;
        let security_descriptor = OwnedSecurityDescriptor::for_desktop_broker(
            &broker_logon_sid,
            child_logon_sid,
            launch_sid,
        )?;
        let security_attributes = security_descriptor.attributes();

        let original_station = unsafe { GetProcessWindowStation() };
        if original_station == 0 {
            return Err(anyhow::anyhow!(
                "GetProcessWindowStation failed: {}",
                unsafe { GetLastError() }
            ));
        }
        let original_desktop = unsafe { GetThreadDesktop(GetCurrentThreadId()) };
        if original_desktop == 0 {
            return Err(anyhow::anyhow!("GetThreadDesktop failed: {}", unsafe {
                GetLastError()
            }));
        }

        let mut rng = SmallRng::from_entropy();
        let station_handle = unsafe {
            CreateWindowStationW(
                ptr::null(),
                CWF_CREATE_ONLY,
                WINDOW_STATION_ALL_ACCESS,
                &security_attributes,
            )
        };
        if station_handle == 0 {
            anyhow::bail!("CreateWindowStationW failed: {}", unsafe { GetLastError() });
        }
        let station_name = match user_object_name(station_handle, "private window station") {
            Ok(name) => name,
            Err(err) => {
                unsafe {
                    let _ = CloseWindowStation(station_handle);
                }
                return Err(err);
            }
        };
        if unsafe { SetProcessWindowStation(station_handle) } == 0 {
            let err = unsafe { GetLastError() };
            unsafe {
                let _ = CloseWindowStation(station_handle);
            }
            anyhow::bail!("SetProcessWindowStation(private) failed: {err}");
        }

        let desktop_name = format!("CodexSandboxDesktop-{:x}", rng.r#gen::<u128>());
        let desktop_name_wide = to_wide(&desktop_name);
        let desktop_handle = unsafe {
            CreateDesktopW(
                desktop_name_wide.as_ptr(),
                ptr::null(),
                ptr::null_mut(),
                0,
                DESKTOP_ALL_ACCESS,
                &security_attributes,
            )
        };
        let create_error = (desktop_handle == 0).then(|| unsafe { GetLastError() });
        let restore_station_error = (unsafe { SetProcessWindowStation(original_station) } == 0)
            .then(|| unsafe { GetLastError() });
        let restore_desktop_error =
            (unsafe { SetThreadDesktop(original_desktop) } == 0).then(|| unsafe { GetLastError() });
        if let Some(err) = restore_station_error.or(restore_desktop_error) {
            if desktop_handle != 0 {
                unsafe {
                    let _ = CloseDesktop(desktop_handle);
                }
            }
            unsafe {
                let _ = CloseWindowStation(station_handle);
            }
            anyhow::bail!("desktop broker failed to restore its launch objects: {err}");
        }
        if let Some(err) = create_error {
            unsafe {
                let _ = CloseWindowStation(station_handle);
            }
            anyhow::bail!(
                "CreateDesktopW failed: {err} ({})",
                format_last_error(err as i32)
            );
        }

        Ok(Self {
            desktop_handle,
            station_handle,
            qualified_name: format!("{station_name}\\{desktop_name}"),
        })
    }
}

impl Drop for BrokerDesktop {
    fn drop(&mut self) {
        unsafe {
            if self.desktop_handle != 0 {
                let _ = CloseDesktop(self.desktop_handle);
            }
            if self.station_handle != 0 {
                let _ = CloseWindowStation(self.station_handle);
            }
        }
    }
}

fn user_object_name(handle: isize, object_name: &str) -> Result<String> {
    let mut bytes_needed = 0;
    unsafe {
        GetUserObjectInformationW(handle, UOI_NAME, ptr::null_mut(), 0, &mut bytes_needed);
    }
    if bytes_needed < std::mem::size_of::<u16>() as u32 {
        anyhow::bail!(
            "GetUserObjectInformationW({object_name}) size query failed: {}",
            unsafe { GetLastError() }
        );
    }

    let mut name = vec![0u16; (bytes_needed as usize).div_ceil(std::mem::size_of::<u16>())];
    let result = unsafe {
        GetUserObjectInformationW(
            handle,
            UOI_NAME,
            name.as_mut_ptr() as *mut c_void,
            (name.len() * std::mem::size_of::<u16>()) as u32,
            &mut bytes_needed,
        )
    };
    if result == 0 {
        anyhow::bail!(
            "GetUserObjectInformationW({object_name}) failed: {}",
            unsafe { GetLastError() }
        );
    }
    let name_len = name
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(name.len());
    let name = String::from_utf16(&name[..name_len])?;
    if !is_valid_station_component(&name) {
        anyhow::bail!("{object_name} has an invalid assigned name");
    }
    Ok(name)
}

fn open_broker_pipe(name: &str, access: u32) -> Result<File> {
    let name = to_wide(name);
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            access,
            0,
            ptr::null_mut(),
            OPEN_EXISTING,
            0,
            0,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        anyhow::bail!("CreateFileW(desktop broker pipe) failed: {}", unsafe {
            GetLastError()
        });
    }
    Ok(unsafe { File::from_raw_handle(handle as *mut c_void) })
}

#[doc(hidden)]
pub fn run_desktop_broker() -> Result<()> {
    let mut pipe_in = None;
    let mut pipe_out = None;
    let mut launch_sid = None;
    let mut child_logon_sid = None;
    for arg in std::env::args().skip(1) {
        if let Some(value) = arg.strip_prefix(PIPE_IN_ARG) {
            pipe_in = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix(PIPE_OUT_ARG) {
            pipe_out = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix(LAUNCH_SID_ARG) {
            launch_sid = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix(CHILD_LOGON_SID_ARG) {
            child_logon_sid = Some(value.to_string());
        }
    }
    let mut hold_pipe = open_broker_pipe(
        &pipe_in.context("desktop broker pipe-in missing")?,
        FILE_GENERIC_READ,
    )?;
    let mut ready_pipe = open_broker_pipe(
        &pipe_out.context("desktop broker pipe-out missing")?,
        FILE_GENERIC_WRITE,
    )?;
    let result = (|| -> Result<BrokerDesktop> {
        let launch_sid =
            LocalSid::from_string(&launch_sid.context("desktop broker launch SID missing")?)?;
        let child_logon_sid = LocalSid::from_string(
            &child_logon_sid.context("desktop broker child logon SID missing")?,
        )?;
        BrokerDesktop::create(&child_logon_sid, &launch_sid)
    })();
    let desktop = match result {
        Ok(desktop) => desktop,
        Err(err) => {
            writeln!(ready_pipe, "ERR\t{err:#}")?;
            ready_pipe.flush()?;
            return Err(err);
        }
    };
    writeln!(ready_pipe, "OK\t{}", desktop.qualified_name)?;
    ready_pipe.flush()?;
    let mut ignored = Vec::new();
    hold_pipe.read_to_end(&mut ignored)?;
    drop(desktop);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_desktop_is_rejected_for_restricted_launches() {
        let launch_sid = LocalSid::from_string("S-1-5-21-1-2-3-4").expect("launch SID");
        let error = LaunchDesktop::prepare(&launch_sid, Path::new("unused.exe"), false, None)
            .err()
            .expect("shared desktop must be rejected");
        assert!(
            error
                .to_string()
                .contains("restricted Windows launches require a private desktop")
        );
    }

    #[test]
    fn broker_names_are_strictly_scoped() {
        assert!(validate_broker_desktop_name("Service-0x0-3e7$\\CodexSandboxDesktop-b").is_ok());
        for invalid in [
            "WinSta0\\Default",
            "\\CodexSandboxDesktop-b",
            "unsafe/station\\CodexSandboxDesktop-b",
            "unsafe\nstation\\CodexSandboxDesktop-b",
            "Service-0x0-3e7$\\CodexSandboxDesktop-b\\extra",
        ] {
            assert!(validate_broker_desktop_name(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn isolated_objects_grant_full_process_start_access() {
        assert_eq!(WINDOW_STATION_ALL_ACCESS, 0x000F_037F);
        assert_eq!(CWF_CREATE_ONLY, 1);
        assert_eq!(
            desktop_broker_sddl("S-1-5-5-1-1", "S-1-5-5-2-2", "S-1-5-21-1-2-3-4"),
            "D:P(A;;GA;;;S-1-5-5-1-1)(A;;0x000f00ff;;;S-1-5-5-2-2)(A;;0x000f00ff;;;S-1-5-21-1-2-3-4)"
        );
    }

    #[test]
    fn local_system_uses_the_privileged_broker_spawn_path() {
        assert!(is_local_system_sid("S-1-5-18"));
        assert!(is_local_system_sid("s-1-5-18"));
        assert!(!is_local_system_sid("S-1-5-19"));
    }
}
