use std::{io, ptr, slice};

use windows_sys::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
    CRED_TYPE_GENERIC,
};

const TARGET: &str = "KytyPS5 Studio/GitHub";
const ERROR_NOT_FOUND: i32 = 1168;
const MAX_CREDENTIAL_BLOB: usize = 2560;

fn target_name(name: &str) -> Vec<u16> {
    name.encode_utf16().chain(Some(0)).collect()
}

pub fn get() -> Result<Option<String>, String> {
    get_for_target(TARGET)
}

fn get_for_target(name: &str) -> Result<Option<String>, String> {
    let target = target_name(name);
    let mut credential = ptr::null_mut();
    if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_NOT_FOUND) {
            return Ok(None);
        }
        return Err(format!(
            "Could not read the GitHub token from Windows Credential Manager: {error}"
        ));
    }
    if credential.is_null() {
        return Err("Windows Credential Manager returned an empty credential".into());
    }
    let credential_ref = unsafe { &*credential };
    let bytes = if credential_ref.CredentialBlobSize == 0 {
        &[][..]
    } else if credential_ref.CredentialBlob.is_null()
        || credential_ref.CredentialBlobSize as usize > MAX_CREDENTIAL_BLOB
    {
        unsafe { CredFree(credential.cast()) };
        return Err("Stored GitHub token has an invalid credential size".into());
    } else {
        unsafe {
            slice::from_raw_parts(
                credential_ref.CredentialBlob,
                credential_ref.CredentialBlobSize as usize,
            )
        }
    };
    let token = String::from_utf8(bytes.to_vec())
        .map_err(|_| "Stored GitHub token is not valid UTF-8".to_string());
    unsafe { CredFree(credential.cast()) };
    token.map(Some)
}

pub fn save(token: &str) -> Result<(), String> {
    save_for_target(TARGET, token)
}

fn save_for_target(name: &str, token: &str) -> Result<(), String> {
    let mut target = target_name(name);
    let mut username = "github".encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mut bytes = token.as_bytes().to_vec();
    if bytes.len() > MAX_CREDENTIAL_BLOB {
        return Err("GitHub token exceeds the Windows Credential Manager size limit".into());
    }
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: target.as_mut_ptr(),
        CredentialBlobSize: bytes.len() as u32,
        CredentialBlob: bytes.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: username.as_mut_ptr(),
        ..Default::default()
    };
    let stored = unsafe { CredWriteW(&credential, 0) } != 0;
    bytes.fill(0);
    if !stored {
        return Err(format!(
            "Could not store the GitHub token in Windows Credential Manager: {}",
            io::Error::last_os_error()
        ));
    }
    Ok(())
}

pub fn delete() -> Result<(), String> {
    delete_for_target(TARGET)
}

fn delete_for_target(name: &str) -> Result<(), String> {
    let target = target_name(name);
    if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } != 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_NOT_FOUND) {
        Ok(())
    } else {
        Err(format!(
            "Could not remove the GitHub token from Windows Credential Manager: {error}"
        ))
    }
}

#[cfg(test)]
mod tests {
    struct DisposableCredential(String);

    impl Drop for DisposableCredential {
        fn drop(&mut self) {
            let _ = super::delete_for_target(&self.0);
        }
    }

    #[test]
    fn credential_manager_round_trips_and_deletes_a_disposable_credential() {
        let target = DisposableCredential(format!(
            "KytyPS5 Studio/Test/{}/{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock is after the Unix epoch")
                .as_nanos()
        ));
        super::delete_for_target(&target.0).expect("stale test credential is removed");

        let token = "temporary-test-token-é-ゲーム";
        super::save_for_target(&target.0, token).expect("Credential Manager stores the test value");
        assert_eq!(
            super::get_for_target(&target.0).expect("Credential Manager reads the test value"),
            Some(token.to_string())
        );
        super::delete_for_target(&target.0).expect("Credential Manager deletes the test value");
        assert_eq!(
            super::get_for_target(&target.0).expect("Credential Manager confirms deletion"),
            None
        );
    }
}
