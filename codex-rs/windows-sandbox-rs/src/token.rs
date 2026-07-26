use crate::winutil::to_wide;
use anyhow::Result;
use anyhow::anyhow;
use std::ffi::c_void;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Foundation::HLOCAL;
use windows_sys::Win32::Foundation::LUID;
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::AdjustTokenPrivileges;
use windows_sys::Win32::Security::Authorization::EXPLICIT_ACCESS_W;
use windows_sys::Win32::Security::Authorization::GRANT_ACCESS;
use windows_sys::Win32::Security::Authorization::SetEntriesInAclW;
use windows_sys::Win32::Security::Authorization::TRUSTEE_IS_SID;
use windows_sys::Win32::Security::Authorization::TRUSTEE_IS_UNKNOWN;
use windows_sys::Win32::Security::Authorization::TRUSTEE_W;
use windows_sys::Win32::Security::CopySid;
use windows_sys::Win32::Security::CreateRestrictedToken;
use windows_sys::Win32::Security::CreateWellKnownSid;
use windows_sys::Win32::Security::GetLengthSid;
use windows_sys::Win32::Security::GetTokenInformation;
use windows_sys::Win32::Security::LookupPrivilegeValueW;
use windows_sys::Win32::Security::SetTokenInformation;

use windows_sys::Win32::Security::ACL;
use windows_sys::Win32::Security::SID_AND_ATTRIBUTES;
use windows_sys::Win32::Security::TOKEN_ADJUST_DEFAULT;
use windows_sys::Win32::Security::TOKEN_ADJUST_PRIVILEGES;
use windows_sys::Win32::Security::TOKEN_ADJUST_SESSIONID;
use windows_sys::Win32::Security::TOKEN_ASSIGN_PRIMARY;
use windows_sys::Win32::Security::TOKEN_DUPLICATE;
use windows_sys::Win32::Security::TOKEN_PRIVILEGES;
use windows_sys::Win32::Security::TOKEN_QUERY;
use windows_sys::Win32::Security::TOKEN_USER;
use windows_sys::Win32::Security::TokenDefaultDacl;
use windows_sys::Win32::Security::TokenGroups;
use windows_sys::Win32::Security::TokenUser;
use windows_sys::Win32::System::Threading::GetCurrentProcess;

const DISABLE_MAX_PRIVILEGE: u32 = 0x01;
const LUA_TOKEN: u32 = 0x04;
const WRITE_RESTRICTED: u32 = 0x08;
const GENERIC_ALL: u32 = 0x1000_0000;
const WIN_WORLD_SID: i32 = 1;
#[cfg(test)]
const WIN_WRITE_RESTRICTED_CODE_SID: i32 = 70;
const SE_GROUP_LOGON_ID: u32 = 0xC0000000;

#[repr(C)]
struct TokenDefaultDaclInfo {
    default_dacl: *mut ACL,
}

/// Sets a permissive default DACL so sandboxed processes can create pipes/IPC objects
/// without hitting ACCESS_DENIED when PowerShell builds pipelines.
unsafe fn set_default_dacl(h_token: HANDLE, sids: &[*mut c_void]) -> Result<()> {
    if sids.is_empty() {
        return Ok(());
    }
    let entries: Vec<EXPLICIT_ACCESS_W> = sids
        .iter()
        .map(|sid| EXPLICIT_ACCESS_W {
            grfAccessPermissions: GENERIC_ALL,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: 0,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: std::ptr::null_mut(),
                MultipleTrusteeOperation: 0,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_UNKNOWN,
                ptstrName: *sid as *mut u16,
            },
        })
        .collect();
    let mut p_new_dacl: *mut ACL = std::ptr::null_mut();
    let res = SetEntriesInAclW(
        entries.len() as u32,
        entries.as_ptr(),
        std::ptr::null_mut(),
        &mut p_new_dacl,
    );
    if res != ERROR_SUCCESS {
        return Err(anyhow!("SetEntriesInAclW failed: {res}"));
    }
    let mut info = TokenDefaultDaclInfo {
        default_dacl: p_new_dacl,
    };
    let ok = SetTokenInformation(
        h_token,
        TokenDefaultDacl,
        &mut info as *mut _ as *mut c_void,
        std::mem::size_of::<TokenDefaultDaclInfo>() as u32,
    );
    if ok == 0 {
        let err = GetLastError();
        if !p_new_dacl.is_null() {
            LocalFree(p_new_dacl as HLOCAL);
        }
        return Err(anyhow!(
            "SetTokenInformation(TokenDefaultDacl) failed: {err}",
        ));
    }
    if !p_new_dacl.is_null() {
        LocalFree(p_new_dacl as HLOCAL);
    }
    Ok(())
}

unsafe fn well_known_sid(sid_type: i32) -> Result<Vec<u8>> {
    let mut size: u32 = 0;
    CreateWellKnownSid(
        sid_type,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        &mut size,
    );
    let mut buf: Vec<u8> = vec![0u8; size as usize];
    let ok = CreateWellKnownSid(
        sid_type,
        std::ptr::null_mut(),
        buf.as_mut_ptr() as *mut c_void,
        &mut size,
    );
    if ok == 0 {
        return Err(anyhow!("CreateWellKnownSid failed: {}", GetLastError()));
    }
    Ok(buf)
}

pub unsafe fn world_sid() -> Result<Vec<u8>> {
    well_known_sid(WIN_WORLD_SID)
}

/// # Safety
/// Caller is responsible for freeing the returned SID with `LocalFree`.
pub unsafe fn convert_string_sid_to_sid(s: &str) -> Option<*mut c_void> {
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn ConvertStringSidToSidW(StringSid: *const u16, Sid: *mut *mut c_void) -> i32;
    }
    let mut psid: *mut c_void = std::ptr::null_mut();
    let ok = unsafe { ConvertStringSidToSidW(to_wide(s).as_ptr(), &mut psid) };
    if ok != 0 { Some(psid) } else { None }
}

/// Owns a SID allocated by `ConvertStringSidToSidW` and releases it with `LocalFree`.
pub struct LocalSid {
    psid: *mut c_void,
    sid_string: String,
}

impl LocalSid {
    pub fn from_string(sid: &str) -> Result<Self> {
        let psid = unsafe { convert_string_sid_to_sid(sid) }
            .ok_or_else(|| anyhow!("invalid SID string: {sid}"))?;
        Ok(Self {
            psid,
            sid_string: sid.to_string(),
        })
    }

    pub fn as_ptr(&self) -> *mut c_void {
        self.psid
    }

    pub fn as_str(&self) -> &str {
        &self.sid_string
    }
}

impl Drop for LocalSid {
    fn drop(&mut self) {
        if !self.psid.is_null() {
            unsafe {
                LocalFree(self.psid as HLOCAL);
            }
        }
    }
}

/// # Safety
/// Caller must close the returned token handle.
pub unsafe fn get_current_token_for_restriction() -> Result<HANDLE> {
    let desired = TOKEN_DUPLICATE
        | TOKEN_QUERY
        | TOKEN_ASSIGN_PRIMARY
        | TOKEN_ADJUST_DEFAULT
        | TOKEN_ADJUST_SESSIONID
        | TOKEN_ADJUST_PRIVILEGES;
    let mut h: HANDLE = 0;
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn OpenProcessToken(
            ProcessHandle: HANDLE,
            DesiredAccess: u32,
            TokenHandle: *mut HANDLE,
        ) -> i32;
    }
    let ok = unsafe { OpenProcessToken(GetCurrentProcess(), desired, &mut h) };
    if ok == 0 {
        return Err(anyhow!("OpenProcessToken failed: {}", GetLastError()));
    }
    Ok(h)
}

pub unsafe fn get_logon_sid_bytes(h_token: HANDLE) -> Result<Vec<u8>> {
    unsafe fn scan_token_groups_for_logon(h: HANDLE) -> Option<Vec<u8>> {
        let mut needed: u32 = 0;
        GetTokenInformation(h, TokenGroups, std::ptr::null_mut(), 0, &mut needed);
        if needed == 0 {
            return None;
        }
        let mut buf: Vec<u8> = vec![0u8; needed as usize];
        let ok = GetTokenInformation(
            h,
            TokenGroups,
            buf.as_mut_ptr() as *mut c_void,
            needed,
            &mut needed,
        );
        if ok == 0 || (needed as usize) < std::mem::size_of::<u32>() {
            return None;
        }
        let group_count = std::ptr::read_unaligned(buf.as_ptr() as *const u32) as usize;
        // TOKEN_GROUPS layout is: DWORD GroupCount; SID_AND_ATTRIBUTES Groups[];
        // On 64-bit, Groups is aligned to pointer alignment after 4-byte GroupCount.
        let after_count = unsafe { buf.as_ptr().add(std::mem::size_of::<u32>()) } as usize;
        let align = std::mem::align_of::<SID_AND_ATTRIBUTES>();
        let aligned = (after_count + (align - 1)) & !(align - 1);
        let groups_ptr = aligned as *const SID_AND_ATTRIBUTES;
        for i in 0..group_count {
            let entry: SID_AND_ATTRIBUTES = std::ptr::read_unaligned(groups_ptr.add(i));
            if (entry.Attributes & SE_GROUP_LOGON_ID) == SE_GROUP_LOGON_ID {
                let sid = entry.Sid;
                let sid_len = GetLengthSid(sid);
                if sid_len == 0 {
                    return None;
                }
                let mut out = vec![0u8; sid_len as usize];
                if CopySid(sid_len, out.as_mut_ptr() as *mut c_void, sid) == 0 {
                    return None;
                }
                return Some(out);
            }
        }
        None
    }

    if let Some(v) = scan_token_groups_for_logon(h_token) {
        return Ok(v);
    }

    #[repr(C)]
    struct TOKEN_LINKED_TOKEN {
        linked_token: HANDLE,
    }
    const TOKEN_LINKED_TOKEN_CLASS: i32 = 19; // TokenLinkedToken
    let mut ln_needed: u32 = 0;
    GetTokenInformation(
        h_token,
        TOKEN_LINKED_TOKEN_CLASS,
        std::ptr::null_mut(),
        0,
        &mut ln_needed,
    );
    if ln_needed >= std::mem::size_of::<TOKEN_LINKED_TOKEN>() as u32 {
        let mut ln_buf: Vec<u8> = vec![0u8; ln_needed as usize];
        let ok = GetTokenInformation(
            h_token,
            TOKEN_LINKED_TOKEN_CLASS,
            ln_buf.as_mut_ptr() as *mut c_void,
            ln_needed,
            &mut ln_needed,
        );
        if ok != 0 {
            let lt: TOKEN_LINKED_TOKEN =
                std::ptr::read_unaligned(ln_buf.as_ptr() as *const TOKEN_LINKED_TOKEN);
            if lt.linked_token != 0 {
                let res = scan_token_groups_for_logon(lt.linked_token);
                CloseHandle(lt.linked_token);
                if let Some(v) = res {
                    return Ok(v);
                }
            }
        }
    }

    Err(anyhow!("Logon SID not present on token"))
}

pub(crate) unsafe fn get_user_sid_bytes(h_token: HANDLE) -> Result<Vec<u8>> {
    let mut needed: u32 = 0;
    GetTokenInformation(h_token, TokenUser, std::ptr::null_mut(), 0, &mut needed);
    if needed == 0 {
        return Err(anyhow!("TokenUser size query returned 0"));
    }
    let mut user_buf: Vec<u8> = vec![0u8; needed as usize];
    let ok = GetTokenInformation(
        h_token,
        TokenUser,
        user_buf.as_mut_ptr() as *mut c_void,
        needed,
        &mut needed,
    );
    if ok == 0 || (needed as usize) < std::mem::size_of::<TOKEN_USER>() {
        return Err(anyhow!(
            "GetTokenInformation(TokenUser) failed: {}",
            GetLastError()
        ));
    }
    let token_user: TOKEN_USER = std::ptr::read_unaligned(user_buf.as_ptr() as *const TOKEN_USER);
    let sid_len = GetLengthSid(token_user.User.Sid);
    if sid_len == 0 {
        return Err(anyhow!(
            "GetLengthSid(TokenUser) failed: {}",
            GetLastError()
        ));
    }
    let mut user_sid_bytes = vec![0u8; sid_len as usize];
    if CopySid(
        sid_len,
        user_sid_bytes.as_mut_ptr() as *mut c_void,
        token_user.User.Sid,
    ) == 0
    {
        return Err(anyhow!("CopySid(TokenUser) failed: {}", GetLastError()));
    }
    Ok(user_sid_bytes)
}

unsafe fn enable_single_privilege(h_token: HANDLE, name: &str) -> Result<()> {
    let mut luid = LUID {
        LowPart: 0,
        HighPart: 0,
    };
    let ok = LookupPrivilegeValueW(std::ptr::null(), to_wide(name).as_ptr(), &mut luid);
    if ok == 0 {
        return Err(anyhow!("LookupPrivilegeValueW failed: {}", GetLastError()));
    }
    let mut tp: TOKEN_PRIVILEGES = std::mem::zeroed();
    tp.PrivilegeCount = 1;
    tp.Privileges[0].Luid = luid;
    tp.Privileges[0].Attributes = 0x00000002; // SE_PRIVILEGE_ENABLED
    let ok2 = AdjustTokenPrivileges(
        h_token,
        0,
        &tp,
        0,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    );
    if ok2 == 0 {
        return Err(anyhow!("AdjustTokenPrivileges failed: {}", GetLastError()));
    }
    let err = GetLastError();
    if err != 0 {
        return Err(anyhow!("AdjustTokenPrivileges error {err}"));
    }
    Ok(())
}

/// Creates a read-only token with a separate non-filesystem launch SID.
///
/// # Safety
/// Caller must close the returned token handle; both SID pointers must remain
/// valid for the duration of this call.
pub(crate) unsafe fn create_readonly_token_with_cap_and_launch(
    psid_capability: *mut c_void,
    psid_launch: *mut c_void,
) -> Result<HANDLE> {
    let base = get_current_token_for_restriction()?;
    let result = create_token_with_caps_from(base, &[psid_capability], &[psid_launch]);
    CloseHandle(base);
    result
}

/// # Safety
/// Caller must close the returned token handle; base_token must be a valid primary token.
pub unsafe fn create_readonly_token_with_cap_from(
    base_token: HANDLE,
    psid_capability: *mut c_void,
) -> Result<(HANDLE, *mut c_void)> {
    let new_token = create_token_with_caps_from(base_token, &[psid_capability], &[])?;
    Ok((new_token, psid_capability))
}

/// Create a restricted token that includes all provided capability SIDs.
///
/// # Safety
/// Caller must close the returned token handle; base_token must be a valid primary token.
pub unsafe fn create_workspace_write_token_with_caps_from(
    base_token: HANDLE,
    psid_capabilities: &[*mut c_void],
) -> Result<HANDLE> {
    create_token_with_caps_from(base_token, psid_capabilities, &[])
}

/// Creates a workspace-write token with a non-filesystem launch SID.
///
/// The launch SID is granted access only to process-launch objects such as the
/// private desktop. Root capability SIDs remain the sole filesystem write
/// authority.
///
/// # Safety
/// Caller must close the returned token handle; all pointers and `base_token`
/// must remain valid for the duration of this call.
pub unsafe fn create_workspace_write_token_with_caps_and_launch_from(
    base_token: HANDLE,
    psid_capabilities: &[*mut c_void],
    psid_launch: *mut c_void,
) -> Result<HANDLE> {
    create_token_with_caps_from(base_token, psid_capabilities, &[psid_launch])
}

/// Create a restricted token that includes all provided capability SIDs plus the token user SID.
///
/// This is intended for the elevated sandbox backend, where the token user is the dedicated
/// sandbox account rather than the real signed-in user.
///
/// # Safety
/// Caller must close the returned token handle; base_token must be a valid primary token.
pub unsafe fn create_workspace_write_token_with_caps_and_user_from(
    base_token: HANDLE,
    psid_capabilities: &[*mut c_void],
) -> Result<HANDLE> {
    let mut user_sid_bytes = get_user_sid_bytes(base_token)?;
    let psid_user = user_sid_bytes.as_mut_ptr() as *mut c_void;
    create_token_with_caps_from(base_token, psid_capabilities, &[psid_user])
}

/// Create a restricted token that includes all provided capability SIDs.
///
/// # Safety
/// Caller must close the returned token handle; base_token must be a valid primary token.
pub unsafe fn create_readonly_token_with_caps_from(
    base_token: HANDLE,
    psid_capabilities: &[*mut c_void],
) -> Result<HANDLE> {
    create_token_with_caps_from(base_token, psid_capabilities, &[])
}

/// Create a restricted token that includes all provided capability SIDs plus the token user SID.
///
/// This is intended for the elevated sandbox backend, where the token user is the dedicated
/// sandbox account rather than the real signed-in user.
///
/// # Safety
/// Caller must close the returned token handle; base_token must be a valid primary token.
pub unsafe fn create_readonly_token_with_caps_and_user_from(
    base_token: HANDLE,
    psid_capabilities: &[*mut c_void],
) -> Result<HANDLE> {
    let mut user_sid_bytes = get_user_sid_bytes(base_token)?;
    let psid_user = user_sid_bytes.as_mut_ptr() as *mut c_void;
    create_token_with_caps_from(base_token, psid_capabilities, &[psid_user])
}

/// Creates a read-only token with a per-launch restricting SID.
///
/// # Safety
/// Caller must close the returned token handle; all pointers and `base_token`
/// must remain valid for the duration of this call.
pub unsafe fn create_readonly_token_with_caps_and_launch_from(
    base_token: HANDLE,
    psid_capabilities: &[*mut c_void],
    psid_launch: *mut c_void,
) -> Result<HANDLE> {
    create_token_with_caps_from(base_token, psid_capabilities, &[psid_launch])
}

unsafe fn create_token_with_caps_from(
    base_token: HANDLE,
    psid_capabilities: &[*mut c_void],
    extra_restricting_sids: &[*mut c_void],
) -> Result<HANDLE> {
    if psid_capabilities.is_empty() {
        return Err(anyhow!("no capability SIDs provided"));
    }
    // Restricting SIDs are alternatives during the second access check. Keep
    // the legacy token limited to explicit capabilities: ordinary user-owned
    // objects commonly grant the logon SID, Everyone, or Write Restricted
    // Code, and any of those would bypass the capability-root boundary.
    let mut entries: Vec<SID_AND_ATTRIBUTES> =
        vec![std::mem::zeroed(); psid_capabilities.len() + extra_restricting_sids.len()];
    for (i, psid) in psid_capabilities.iter().enumerate() {
        entries[i].Sid = *psid;
        entries[i].Attributes = 0;
    }
    let extras_idx = psid_capabilities.len();
    for (i, psid) in extra_restricting_sids.iter().enumerate() {
        entries[extras_idx + i].Sid = *psid;
        entries[extras_idx + i].Attributes = 0;
    }
    let mut new_token: HANDLE = 0;
    let flags = DISABLE_MAX_PRIVILEGE | LUA_TOKEN | WRITE_RESTRICTED;
    let ok = CreateRestrictedToken(
        base_token,
        flags,
        0,
        std::ptr::null(),
        0,
        std::ptr::null(),
        entries.len() as u32,
        entries.as_mut_ptr(),
        &mut new_token,
    );
    if ok == 0 {
        return Err(anyhow!("CreateRestrictedToken failed: {}", GetLastError()));
    }

    let configure_result = (|| -> Result<()> {
        let mut user_sid_bytes = get_user_sid_bytes(base_token)?;
        let psid_user = user_sid_bytes.as_mut_ptr() as *mut c_void;
        let mut dacl_sids = Vec::with_capacity(1 + psid_capabilities.len());
        // New kernel objects need both halves of the restricted-token access
        // check: the ordinary token user and a filesystem capability. Launch
        // capabilities must never become filesystem authority through the
        // token's default DACL.
        dacl_sids.push(psid_user);
        dacl_sids.extend_from_slice(psid_capabilities);
        set_default_dacl(new_token, &dacl_sids)?;
        enable_single_privilege(new_token, "SeChangeNotifyPrivilege")
    })();
    if let Err(err) = configure_result {
        CloseHandle(new_token);
        return Err(err);
    }
    Ok(new_token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acl::dacl_mask_allows;
    use std::io;
    use std::path::Path;
    use windows_sys::Win32::Security::Authorization::SetNamedSecurityInfoW;
    use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;
    use windows_sys::Win32::Security::ImpersonateLoggedOnUser;
    use windows_sys::Win32::Security::PROTECTED_DACL_SECURITY_INFORMATION;
    use windows_sys::Win32::Security::RevertToSelf;
    use windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;

    struct OwnedHandle(HANDLE);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    struct ImpersonationGuard {
        reverted: bool,
    }

    impl ImpersonationGuard {
        unsafe fn revert(mut self) -> io::Result<()> {
            if RevertToSelf() == 0 {
                return Err(io::Error::last_os_error());
            }
            self.reverted = true;
            Ok(())
        }
    }

    impl Drop for ImpersonationGuard {
        fn drop(&mut self) {
            if !self.reverted {
                unsafe {
                    RevertToSelf();
                }
            }
        }
    }

    unsafe fn set_protected_full_access_acl(path: &Path, sids: &[*mut c_void]) {
        let entries = sids
            .iter()
            .map(|sid| EXPLICIT_ACCESS_W {
                grfAccessPermissions: GENERIC_ALL,
                grfAccessMode: GRANT_ACCESS,
                grfInheritance: 0,
                Trustee: TRUSTEE_W {
                    pMultipleTrustee: std::ptr::null_mut(),
                    MultipleTrusteeOperation: 0,
                    TrusteeForm: TRUSTEE_IS_SID,
                    TrusteeType: TRUSTEE_IS_UNKNOWN,
                    ptstrName: *sid as *mut u16,
                },
            })
            .collect::<Vec<_>>();
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let acl_result = SetEntriesInAclW(
            entries.len() as u32,
            entries.as_ptr(),
            std::ptr::null_mut(),
            &mut dacl,
        );
        assert_eq!(acl_result, ERROR_SUCCESS, "SetEntriesInAclW failed");

        let mut path = to_wide(path);
        let security_result = SetNamedSecurityInfoW(
            path.as_mut_ptr(),
            1, // SE_FILE_OBJECT
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            dacl,
            std::ptr::null_mut(),
        );
        if !dacl.is_null() {
            LocalFree(dacl as HLOCAL);
        }
        assert_eq!(
            security_result, ERROR_SUCCESS,
            "SetNamedSecurityInfoW failed"
        );
    }

    unsafe fn write_as(token: HANDLE, path: &Path) -> io::Result<()> {
        if ImpersonateLoggedOnUser(token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let guard = ImpersonationGuard { reverted: false };
        let result = std::fs::write(path, b"restricted-token-write");
        guard.revert()?;
        result
    }

    unsafe fn token_default_dacl_allows(token: HANDLE, sid: *mut c_void) -> bool {
        let mut needed = 0;
        GetTokenInformation(
            token,
            TokenDefaultDacl,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
        assert!(needed > 0, "query token default DACL size");
        let mut buffer = vec![0u8; needed as usize];
        assert_ne!(
            GetTokenInformation(
                token,
                TokenDefaultDacl,
                buffer.as_mut_ptr() as *mut c_void,
                needed,
                &mut needed,
            ),
            0,
            "query token default DACL"
        );
        let info = &*(buffer.as_ptr() as *const TokenDefaultDaclInfo);
        dacl_mask_allows(info.default_dacl, &[sid], FILE_ALL_ACCESS, true)
    }

    #[test]
    fn write_restricted_token_only_allows_capability_roots() {
        let temp = tempfile::tempdir().expect("tempdir");
        let world_only = temp.path().join("world-only");
        let user_only = temp.path().join("user-only");
        let logon_only = temp.path().join("logon-only");
        let write_restricted_code = temp.path().join("write-restricted-code");
        let capability_root = temp.path().join("capability-root");
        std::fs::create_dir_all(&world_only).expect("create world-only directory");
        std::fs::create_dir_all(&user_only).expect("create user-only directory");
        std::fs::create_dir_all(&logon_only).expect("create logon-only directory");
        std::fs::create_dir_all(&write_restricted_code)
            .expect("create write-restricted-code directory");
        std::fs::create_dir_all(&capability_root).expect("create capability directory");

        unsafe {
            let base = OwnedHandle(
                get_current_token_for_restriction().expect("current process restriction token"),
            );
            let mut world_sid = world_sid().expect("world SID");
            let world_sid = world_sid.as_mut_ptr() as *mut c_void;
            let mut user_sid = get_user_sid_bytes(base.0).expect("token user SID");
            let user_sid = user_sid.as_mut_ptr() as *mut c_void;
            let mut logon_sid = get_logon_sid_bytes(base.0).expect("token logon SID");
            let logon_sid = logon_sid.as_mut_ptr() as *mut c_void;
            let mut write_restricted_code_sid =
                well_known_sid(WIN_WRITE_RESTRICTED_CODE_SID).expect("write-restricted-code SID");
            let write_restricted_code_sid = write_restricted_code_sid.as_mut_ptr() as *mut c_void;
            let capability = LocalSid::from_string("S-1-5-21-1-2-3-4").expect("capability SID");
            let launch = LocalSid::from_string("S-1-5-21-1-2-3-5").expect("launch SID");

            set_protected_full_access_acl(&world_only, &[world_sid]);
            set_protected_full_access_acl(&user_only, &[user_sid]);
            set_protected_full_access_acl(&logon_only, &[logon_sid]);
            set_protected_full_access_acl(
                &write_restricted_code,
                &[world_sid, write_restricted_code_sid],
            );
            set_protected_full_access_acl(&capability_root, &[world_sid, capability.as_ptr()]);

            let token = OwnedHandle(
                create_workspace_write_token_with_caps_and_launch_from(
                    base.0,
                    &[capability.as_ptr()],
                    launch.as_ptr(),
                )
                .expect("restricted token"),
            );
            assert!(
                token_default_dacl_allows(token.0, capability.as_ptr()),
                "filesystem capability must remain on the token default DACL"
            );
            assert!(
                token_default_dacl_allows(token.0, user_sid),
                "token user must remain on the token default DACL"
            );
            assert!(
                !token_default_dacl_allows(token.0, launch.as_ptr()),
                "launch SID must not appear on the token default DACL"
            );

            let denied_path = world_only.join("denied.txt");
            let denied = write_as(token.0, &denied_path);
            assert!(
                denied.is_err(),
                "Everyone-only ACL must not satisfy the restricting SID check"
            );
            assert!(!denied_path.exists());

            let user_path = user_only.join("denied.txt");
            let user_denied = write_as(token.0, &user_path);
            assert!(
                user_denied.is_err(),
                "token user SID must not bypass capability-root isolation"
            );
            assert!(!user_path.exists());

            let logon_path = logon_only.join("denied.txt");
            let logon_denied = write_as(token.0, &logon_path);
            assert!(
                logon_denied.is_err(),
                "logon SID must not bypass capability-root isolation"
            );
            assert!(!logon_path.exists());

            let compatibility_path = write_restricted_code.join("denied.txt");
            let compatibility_denied = write_as(token.0, &compatibility_path);
            assert!(
                compatibility_denied.is_err(),
                "Write Restricted Code must not bypass capability-root isolation"
            );
            assert!(!compatibility_path.exists());

            let allowed_path = capability_root.join("allowed.txt");
            write_as(token.0, &allowed_path).expect("capability ACL should permit the write");
            assert_eq!(
                std::fs::read(&allowed_path).expect("read allowed write"),
                b"restricted-token-write"
            );
        }
    }
}
