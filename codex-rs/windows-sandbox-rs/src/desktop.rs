use crate::logging;
use crate::token::get_current_token_for_restriction;
use crate::token::get_logon_sid_bytes;
use crate::winutil::format_last_error;
use crate::winutil::to_wide;
use anyhow::Result;
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use std::ffi::c_void;
use std::path::Path;
use std::ptr;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Foundation::HLOCAL;
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Authorization::EXPLICIT_ACCESS_W;
use windows_sys::Win32::Security::Authorization::GRANT_ACCESS;
use windows_sys::Win32::Security::Authorization::GetSecurityInfo;
use windows_sys::Win32::Security::Authorization::SE_WINDOW_OBJECT;
use windows_sys::Win32::Security::Authorization::SetEntriesInAclW;
use windows_sys::Win32::Security::Authorization::SetSecurityInfo;
use windows_sys::Win32::Security::Authorization::TRUSTEE_IS_SID;
use windows_sys::Win32::Security::Authorization::TRUSTEE_IS_UNKNOWN;
use windows_sys::Win32::Security::Authorization::TRUSTEE_W;
use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;
use windows_sys::Win32::Storage::FileSystem::READ_CONTROL;
use windows_sys::Win32::System::StationsAndDesktops::CloseDesktop;
use windows_sys::Win32::System::StationsAndDesktops::CreateDesktopW;
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
use windows_sys::Win32::System::StationsAndDesktops::GetUserObjectInformationW;
use windows_sys::Win32::System::StationsAndDesktops::UOI_NAME;
use windows_sys::Win32::UI::WindowsAndMessaging::WINSTA_ACCESSCLIPBOARD;
use windows_sys::Win32::UI::WindowsAndMessaging::WINSTA_ACCESSGLOBALATOMS;
use windows_sys::Win32::UI::WindowsAndMessaging::WINSTA_ENUMDESKTOPS;
use windows_sys::Win32::UI::WindowsAndMessaging::WINSTA_ENUMERATE;
use windows_sys::Win32::UI::WindowsAndMessaging::WINSTA_READATTRIBUTES;
use windows_sys::Win32::UI::WindowsAndMessaging::WINSTA_READSCREEN;
use windows_sys::Win32::UI::WindowsAndMessaging::WINSTA_WRITEATTRIBUTES;

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
const WINDOW_STATION_ACCESS: u32 = WINSTA_ACCESSCLIPBOARD as u32
    | WINSTA_ACCESSGLOBALATOMS as u32
    | WINSTA_ENUMDESKTOPS as u32
    | WINSTA_ENUMERATE as u32
    | WINSTA_READATTRIBUTES as u32
    | WINSTA_READSCREEN as u32
    | WINSTA_WRITEATTRIBUTES as u32
    | READ_CONTROL;

pub struct LaunchDesktop {
    _private_desktop: Option<PrivateDesktop>,
    startup_name: Option<Vec<u16>>,
}

impl LaunchDesktop {
    pub fn prepare(use_private_desktop: bool, logs_base_dir: Option<&Path>) -> Result<Self> {
        if use_private_desktop {
            let private_desktop = PrivateDesktop::create(logs_base_dir)?;
            let startup_name = to_wide(format!(
                "{}\\{}",
                private_desktop.station_name, private_desktop.name
            ));
            Ok(Self {
                _private_desktop: Some(private_desktop),
                startup_name: Some(startup_name),
            })
        } else {
            Ok(Self {
                _private_desktop: None,
                startup_name: None,
            })
        }
    }

    pub fn startup_info_desktop(&self) -> *mut u16 {
        self.startup_name
            .as_ref()
            .map_or(ptr::null_mut(), |name| name.as_ptr() as *mut u16)
    }
}

struct PrivateDesktop {
    handle: isize,
    name: String,
    station_name: String,
}

impl PrivateDesktop {
    fn create(logs_base_dir: Option<&Path>) -> Result<Self> {
        let (station_handle, station_name) = current_window_station()?;
        logging::debug_log(
            &format!("creating private desktop on window station {station_name}"),
            logs_base_dir,
        );
        let mut rng = SmallRng::from_entropy();
        let name = format!("CodexSandboxDesktop-{:x}", rng.r#gen::<u128>());
        let name_wide = to_wide(&name);
        let handle = unsafe {
            CreateDesktopW(
                name_wide.as_ptr(),
                ptr::null(),
                ptr::null_mut(),
                0,
                DESKTOP_ALL_ACCESS,
                ptr::null_mut(),
            )
        };
        if handle == 0 {
            let err = unsafe { GetLastError() } as i32;
            logging::debug_log(
                &format!(
                    "CreateDesktopW failed for {name}: {} ({})",
                    err,
                    format_last_error(err),
                ),
                logs_base_dir,
            );
            return Err(anyhow::anyhow!("CreateDesktopW failed: {err}"));
        }

        unsafe {
            if let Err(err) = grant_private_desktop_access(station_handle, handle, logs_base_dir) {
                let _ = CloseDesktop(handle);
                return Err(err);
            }
        }

        Ok(Self {
            handle,
            name,
            station_name,
        })
    }
}

fn current_window_station() -> Result<(isize, String)> {
    let handle = unsafe { GetProcessWindowStation() };
    if handle == 0 {
        return Err(anyhow::anyhow!(
            "GetProcessWindowStation failed: {}",
            unsafe { GetLastError() }
        ));
    }

    let mut bytes_needed = 0;
    unsafe {
        GetUserObjectInformationW(handle, UOI_NAME, ptr::null_mut(), 0, &mut bytes_needed);
    }
    if bytes_needed < std::mem::size_of::<u16>() as u32 {
        return Err(anyhow::anyhow!(
            "GetUserObjectInformationW(UOI_NAME) size query failed: {}",
            unsafe { GetLastError() }
        ));
    }

    let mut name = vec![0u16; (bytes_needed as usize).div_ceil(std::mem::size_of::<u16>())];
    let ok = unsafe {
        GetUserObjectInformationW(
            handle,
            UOI_NAME,
            name.as_mut_ptr() as *mut c_void,
            (name.len() * std::mem::size_of::<u16>()) as u32,
            &mut bytes_needed,
        )
    };
    if ok == 0 {
        return Err(anyhow::anyhow!(
            "GetUserObjectInformationW(UOI_NAME) failed: {}",
            unsafe { GetLastError() }
        ));
    }
    let name_len = name
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(name.len());
    let name = String::from_utf16(&name[..name_len])?;
    if name.is_empty() {
        return Err(anyhow::anyhow!("process window station has no name"));
    }
    Ok((handle, name))
}

unsafe fn grant_private_desktop_access(
    station_handle: isize,
    desktop_handle: isize,
    logs_base_dir: Option<&Path>,
) -> Result<()> {
    let token = get_current_token_for_restriction()?;
    let logon_sid = get_logon_sid_bytes(token);
    CloseHandle(token);
    let mut logon_sid = logon_sid?;
    let logon_sid = logon_sid.as_mut_ptr() as *mut c_void;

    grant_window_object_access(
        station_handle,
        WINDOW_STATION_ACCESS,
        logon_sid,
        "window station",
        logs_base_dir,
    )?;
    grant_window_object_access(
        desktop_handle,
        DESKTOP_ALL_ACCESS,
        logon_sid,
        "private desktop",
        logs_base_dir,
    )
}

unsafe fn grant_window_object_access(
    handle: isize,
    access_mask: u32,
    logon_sid: *mut c_void,
    object_name: &str,
    logs_base_dir: Option<&Path>,
) -> Result<()> {
    let entries = [EXPLICIT_ACCESS_W {
        grfAccessPermissions: access_mask,
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: 0,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: ptr::null_mut(),
            MultipleTrusteeOperation: 0,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_UNKNOWN,
            ptstrName: logon_sid as *mut u16,
        },
    }];

    let mut security_descriptor = ptr::null_mut();
    let mut existing_dacl = ptr::null_mut();
    let get_security_code = GetSecurityInfo(
        handle,
        SE_WINDOW_OBJECT,
        DACL_SECURITY_INFORMATION,
        ptr::null_mut(),
        ptr::null_mut(),
        &mut existing_dacl,
        ptr::null_mut(),
        &mut security_descriptor,
    );
    if get_security_code != ERROR_SUCCESS {
        logging::debug_log(
            &format!("GetSecurityInfo failed for {object_name}: {get_security_code}"),
            logs_base_dir,
        );
        return Err(anyhow::anyhow!(
            "GetSecurityInfo failed for {object_name}: {get_security_code}"
        ));
    }
    if existing_dacl.is_null() {
        // A null DACL already grants access to every SID.
        if !security_descriptor.is_null() {
            LocalFree(security_descriptor as HLOCAL);
        }
        return Ok(());
    }

    let mut updated_dacl = ptr::null_mut();
    let set_entries_code = SetEntriesInAclW(
        entries.len() as u32,
        entries.as_ptr(),
        existing_dacl,
        &mut updated_dacl,
    );
    if set_entries_code != ERROR_SUCCESS {
        if !security_descriptor.is_null() {
            LocalFree(security_descriptor as HLOCAL);
        }
        logging::debug_log(
            &format!("SetEntriesInAclW failed for {object_name}: {set_entries_code}"),
            logs_base_dir,
        );
        return Err(anyhow::anyhow!(
            "SetEntriesInAclW failed for {object_name}: {set_entries_code}"
        ));
    }

    let set_security_code = SetSecurityInfo(
        handle,
        SE_WINDOW_OBJECT,
        DACL_SECURITY_INFORMATION,
        ptr::null_mut(),
        ptr::null_mut(),
        updated_dacl,
        ptr::null_mut(),
    );
    if !updated_dacl.is_null() {
        LocalFree(updated_dacl as HLOCAL);
    }
    if !security_descriptor.is_null() {
        LocalFree(security_descriptor as HLOCAL);
    }
    if set_security_code != ERROR_SUCCESS {
        logging::debug_log(
            &format!("SetSecurityInfo failed for {object_name}: {set_security_code}"),
            logs_base_dir,
        );
        return Err(anyhow::anyhow!(
            "SetSecurityInfo failed for {object_name}: {set_security_code}"
        ));
    }

    Ok(())
}

impl Drop for PrivateDesktop {
    fn drop(&mut self) {
        unsafe {
            if self.handle != 0 {
                let _ = CloseDesktop(self.handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_desktop_inherits_the_callers_station_and_desktop() {
        let launch = LaunchDesktop::prepare(false, None).expect("default launch desktop");

        assert!(launch.startup_info_desktop().is_null());
        assert!(launch.startup_name.is_none());
    }

    #[test]
    fn private_desktop_uses_the_process_window_station_name() {
        let (_, station_name) = current_window_station().expect("process window station");
        let launch = LaunchDesktop::prepare(true, None).expect("private launch desktop");
        let startup_name = launch.startup_name.as_ref().expect("private desktop name");
        let name_len = startup_name
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(startup_name.len());
        let startup_name =
            String::from_utf16(&startup_name[..name_len]).expect("UTF-16 desktop name");

        assert!(startup_name.starts_with(&format!("{station_name}\\")));
        assert!(!launch.startup_info_desktop().is_null());
    }
}
