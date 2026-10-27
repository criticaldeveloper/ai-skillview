use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS,
};
use windows_sys::Win32::System::Registry::*;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "AI Skillview";
const MAX_COMMAND_BYTES: u32 = 64 * 1024;
const MAX_RUN_COMMAND_UNITS: usize = 260;

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn check(status: u32) -> io::Result<()> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status as i32))
    }
}

fn command_for(executable: &Path) -> io::Result<Vec<u16>> {
    let path: Vec<u16> = executable.as_os_str().encode_wide().collect();
    if path.is_empty() || path.contains(&0) || path.contains(&(b'"' as u16)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "The application path cannot be registered for startup.",
        ));
    }
    // canonicalize returns extended DOS paths; the registry command uses the same
    // ordinary path spelling as the installer and Windows startup settings.
    let prefix: Vec<u16> = "\\\\?\\".encode_utf16().collect();
    let path = if path.starts_with(&prefix) && path.get(5) == Some(&(b':' as u16)) {
        &path[prefix.len()..]
    } else {
        &path
    };
    let mut command = Vec::with_capacity(path.len() + 16);
    command.push(b'"' as u16);
    command.extend_from_slice(path);
    command.extend("\" --minimized".encode_utf16());
    // Windows Run keys accept at most 260 UTF-16 units, excluding the terminator.
    if command.len() > MAX_RUN_COMMAND_UNITS {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "The application path is too long for Windows startup. Install AI Skillview in a shorter folder.",
        ));
    }
    Ok(command)
}

fn current_command() -> io::Result<Vec<u16>> {
    command_for(&std::fs::canonicalize(std::env::current_exe()?)?)
}

fn open_key(path: &str, access: REG_SAM_FLAGS) -> io::Result<Option<Key>> {
    let mut key = null_mut();
    let status =
        unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, wide(path).as_ptr(), 0, access, &mut key) };
    if matches!(status, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) {
        return Ok(None);
    }
    check(status)?;
    Ok(Some(Key(key)))
}

fn create_key(path: &str) -> io::Result<Key> {
    let mut key = null_mut();
    check(unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(path).as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
            null(),
            &mut key,
            null_mut(),
        )
    })?;
    Ok(Key(key))
}

fn read_command(key: &Key) -> io::Result<Option<Vec<u16>>> {
    let name = wide(VALUE_NAME);
    // A concurrent installer or another app copy may replace the value between
    // the size query and read. Retry a bounded number of times if it grows.
    for _ in 0..3 {
        let mut kind = 0;
        let mut bytes = 0;
        let status = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                null(),
                &mut kind,
                null_mut(),
                &mut bytes,
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        check(status)?;
        if kind != REG_SZ {
            return Ok(None);
        }
        if bytes % 2 != 0 || bytes > MAX_COMMAND_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The startup registration contains an invalid command.",
            ));
        }
        if bytes == 0 {
            return Ok(Some(Vec::new()));
        }
        let mut value = vec![0u16; bytes as usize / 2];
        let status = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                null(),
                &mut kind,
                value.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if status == ERROR_MORE_DATA {
            continue;
        }
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        check(status)?;
        if kind != REG_SZ {
            return Ok(None);
        }
        if bytes % 2 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The startup command is not valid UTF-16.",
            ));
        }
        value.truncate(bytes as usize / 2);
        while value.last() == Some(&0) {
            value.pop();
        }
        if value.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The startup command contains an unexpected null character.",
            ));
        }
        return Ok(Some(value));
    }
    Err(io::Error::new(
        io::ErrorKind::WouldBlock,
        "The startup registration changed while it was being read. Try again.",
    ))
}

fn same_command(actual: &[u16], expected: &[u16]) -> bool {
    // ASCII casing is the only variable spelling our app/installer generates.
    // Retain all other UTF-16 units exactly, including non-ASCII path characters.
    let Some(path_end) = expected
        .iter()
        .rposition(|&unit| unit == b'"' as u16)
        .map(|index| index + 1)
    else {
        return false;
    };
    actual.len() == expected.len()
        && actual[path_end..] == expected[path_end..]
        && actual[..path_end]
            .iter()
            .zip(&expected[..path_end])
            .all(|(&left, &right)| {
                let lower = |unit: u16| {
                    if (b'A' as u16..=b'Z' as u16).contains(&unit) {
                        unit + (b'a' - b'A') as u16
                    } else {
                        unit
                    }
                };
                lower(left) == lower(right)
            })
}

fn enabled_at(path: &str, command: &[u16]) -> io::Result<bool> {
    let Some(key) = open_key(path, KEY_QUERY_VALUE)? else {
        return Ok(false);
    };
    Ok(read_command(&key)?.is_some_and(|value| same_command(&value, command)))
}

fn set_at(path: &str, command: &[u16], enabled: bool) -> io::Result<()> {
    if enabled {
        let key = create_key(path)?;
        let value: Vec<u16> = command.iter().copied().chain(Some(0)).collect();
        check(unsafe {
            RegSetValueExW(
                key.0,
                wide(VALUE_NAME).as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * size_of::<u16>()) as u32,
            )
        })
    } else {
        let Some(key) = open_key(path, KEY_QUERY_VALUE | KEY_SET_VALUE)? else {
            return Ok(());
        };
        // An installed copy and a portable copy share the product's value name.
        // Disabling a different executable must leave its registration intact.
        if read_command(&key)?.is_some_and(|value| same_command(&value, command)) {
            let status = unsafe { RegDeleteValueW(key.0, wide(VALUE_NAME).as_ptr()) };
            if status != ERROR_FILE_NOT_FOUND {
                check(status)?;
            }
        }
        Ok(())
    }
}

/// Whether this executable is registered to launch in the tray at sign-in.
/// Windows startup controls may independently prevent a registered app from running.
pub fn is_enabled() -> io::Result<bool> {
    enabled_at(RUN_KEY, &current_command()?)
}

/// Register this executable, or remove only this executable's registration.
/// Enabling intentionally replaces any older AI Skillview path after an update/move.
pub fn set_enabled(enabled: bool) -> io::Result<()> {
    set_at(RUN_KEY, &current_command()?, enabled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestKey(String);

    impl TestKey {
        fn new() -> Self {
            Self(format!(
                "Software\\AI Skillview Startup Test {} {}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ))
        }
    }

    impl Drop for TestKey {
        fn drop(&mut self) {
            unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, wide(&self.0).as_ptr()) };
        }
    }

    #[test]
    fn command_quotes_spaces_unicode_and_normalizes_extended_dos_paths() {
        let expected = "\"C:\\Users\\Març\\AI Skillview\\AI Skillview.exe\" --minimized";
        let ordinary =
            command_for(Path::new("C:\\Users\\Març\\AI Skillview\\AI Skillview.exe")).unwrap();
        let extended = command_for(Path::new(
            "\\\\?\\C:\\Users\\Març\\AI Skillview\\AI Skillview.exe",
        ))
        .unwrap();
        assert_eq!(String::from_utf16(&ordinary).unwrap(), expected);
        assert_eq!(ordinary, extended);
        assert!(command_for(Path::new("C:\\bad\"name.exe")).is_err());
        assert!(command_for(Path::new("")).is_err());
    }

    #[test]
    fn startup_command_limit_counts_quotes_arguments_and_utf16_units() {
        let base = "C:\\AI Skillview.exe";
        let overhead = "\"\" --minimized".encode_utf16().count();
        let padding = MAX_RUN_COMMAND_UNITS - base.encode_utf16().count() - overhead;
        let at_limit = format!("C:\\{}AI Skillview.exe", "x".repeat(padding));
        assert_eq!(command_for(Path::new(&at_limit)).unwrap().len(), 260);
        let extended = format!("\\\\?\\{at_limit}");
        assert_eq!(command_for(Path::new(&extended)).unwrap().len(), 260);
        let too_long = format!("C:\\{}AI Skillview.exe", "x".repeat(padding + 1));
        assert_eq!(
            command_for(Path::new(&too_long)).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        let astral = format!("C:\\{}AI Skillview.exe", "🧠".repeat(padding / 2 + 1));
        assert!(astral.chars().count() + overhead < 260);
        assert!(command_for(Path::new(&astral)).is_err());
    }

    #[test]
    fn registration_round_trip_is_idempotent_and_does_not_touch_other_executables() {
        let path = TestKey::new();
        let app = command_for(Path::new("C:\\Apps\\AI Skillview.exe")).unwrap();
        let other = command_for(Path::new("C:\\Other\\AI Skillview.exe")).unwrap();
        assert!(!enabled_at(&path.0, &app).unwrap());
        set_at(&path.0, &app, false).unwrap();
        assert!(open_key(&path.0, KEY_QUERY_VALUE).unwrap().is_none());
        set_at(&path.0, &app, true).unwrap();
        set_at(&path.0, &app, true).unwrap();
        assert!(enabled_at(&path.0, &app).unwrap());
        assert!(!enabled_at(&path.0, &other).unwrap());
        set_at(&path.0, &other, false).unwrap();
        assert!(enabled_at(&path.0, &app).unwrap());
        set_at(&path.0, &other, true).unwrap();
        assert!(enabled_at(&path.0, &other).unwrap());
        assert!(!enabled_at(&path.0, &app).unwrap());
        set_at(&path.0, &other, false).unwrap();
        set_at(&path.0, &other, false).unwrap();
        assert!(!enabled_at(&path.0, &other).unwrap());
    }

    #[test]
    fn matching_requires_the_complete_command_and_supports_ascii_path_casing() {
        let command = command_for(Path::new("C:\\Apps\\AI Skillview.exe")).unwrap();
        let upper = wide("\"c:\\APPS\\ai skillview.EXE\" --minimized");
        assert!(same_command(&upper[..upper.len() - 1], &command));
        let wrong_flag = wide("\"C:\\Apps\\AI Skillview.exe\" --MINIMIZED");
        assert!(!same_command(&wrong_flag[..wrong_flag.len() - 1], &command));
        assert!(!same_command(
            &wide("\"C:\\Apps\\AI Skillview.exe\"")[..],
            &command
        ));
        assert!(!same_command(
            &wide("\"C:\\Apps\\AI Skillview.exe\" --minimized --extra")[..],
            &command
        ));
    }

    #[test]
    fn unrelated_types_and_malformed_values_are_not_deleted() {
        let path = TestKey::new();
        let app = command_for(Path::new("C:\\Apps\\AI Skillview.exe")).unwrap();
        let key = create_key(&path.0).unwrap();
        let invalid = [1u8, 0, 0, 0];
        check(unsafe {
            RegSetValueExW(
                key.0,
                wide(VALUE_NAME).as_ptr(),
                0,
                REG_DWORD,
                invalid.as_ptr(),
                invalid.len() as u32,
            )
        })
        .unwrap();
        assert!(!enabled_at(&path.0, &app).unwrap());
        set_at(&path.0, &app, false).unwrap();
        let mut kind = 0;
        check(unsafe {
            RegQueryValueExW(
                key.0,
                wide(VALUE_NAME).as_ptr(),
                null(),
                &mut kind,
                null_mut(),
                null_mut(),
            )
        })
        .unwrap();
        assert_eq!(kind, REG_DWORD);
        check(unsafe {
            RegSetValueExW(
                key.0,
                wide(VALUE_NAME).as_ptr(),
                0,
                REG_SZ,
                invalid.as_ptr(),
                3,
            )
        })
        .unwrap();
        assert!(enabled_at(&path.0, &app).is_err());
        assert!(set_at(&path.0, &app, false).is_err());
        set_at(&path.0, &app, true).unwrap();
        assert!(enabled_at(&path.0, &app).unwrap());
    }
}
