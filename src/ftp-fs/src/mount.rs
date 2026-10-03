// Every call may arrive on a worker thread: nothing here may touch the host.

use crate::ftp;
use ic_plugin_api::{
    IcBytes, IcConnectionVTable, IcDirEntry, IcFsHandle, IcFsVTable, IcListing, IC_ERR_INIT_FAILED,
    IC_OK,
};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};

struct Mounted {
    mount: ftp::Mount,
    names: Vec<CString>,
    entries: Vec<IcDirEntry>,
    body: Vec<u8>,
    failure: Option<CString>,
}

impl Mounted {
    fn remember(&mut self, reason: String) {
        self.failure = CString::new(reason).ok();
    }
}

fn wanted(path: *const c_char) -> String {
    if path.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(path) }
        .to_string_lossy()
        .to_string()
}

fn held<R>(handle: IcFsHandle, work: impl FnOnce(&mut Mounted) -> R) -> Option<R> {
    if handle.is_null() {
        return None;
    }
    Some(work(unsafe { &mut *(handle as *mut Mounted) }))
}

fn text_of(settings: &serde_json::Value, key: &str) -> Option<String> {
    settings
        .get(key)
        .and_then(|value| value.as_str())
        .map(|text| text.to_string())
        .filter(|text| !text.is_empty())
}

pub extern "C" fn open(bytes: *const u8, len: u64, _user_data: *mut c_void) -> IcFsHandle {
    if bytes.is_null() || len == 0 {
        return std::ptr::null_mut();
    }
    let encoded = unsafe { std::slice::from_raw_parts(bytes, len as usize) };
    let Ok(settings) = serde_json::from_slice::<serde_json::Value>(encoded) else {
        return std::ptr::null_mut();
    };
    let Some(host) = text_of(&settings, "host") else {
        return std::ptr::null_mut();
    };
    let cfg = ftp::Config {
        host,
        port: text_of(&settings, "port")
            .and_then(|text| text.parse().ok())
            .unwrap_or(21),
        user: text_of(&settings, "user").unwrap_or_else(|| "anonymous".to_string()),
        pass: text_of(&settings, "pass").unwrap_or_default(),
    };
    Box::into_raw(Box::new(Mounted {
        mount: ftp::Mount::new(cfg),
        names: Vec::new(),
        entries: Vec::new(),
        body: Vec::new(),
        failure: None,
    })) as IcFsHandle
}

pub extern "C" fn close(handle: IcFsHandle) {
    if handle.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(handle as *mut Mounted) });
}

pub extern "C" fn list(handle: IcFsHandle, path: *const c_char) -> IcListing {
    let path = wanted(path);
    held(handle, |mounted| {
        let here = mounted.mount.config().resolve(&path);
        let found = mounted.mount.with(|session| session.list(&here));
        mounted.names.clear();
        mounted.entries.clear();
        match found {
            Ok(rows) => {
                for row in &rows {
                    let Ok(name) = CString::new(row.name.clone()) else {
                        continue;
                    };
                    mounted.names.push(name);
                    mounted.entries.push(IcDirEntry {
                        name: std::ptr::null(),
                        is_dir: if row.is_dir { 1 } else { 0 },
                        size: row.size,
                        modified: 0,
                        permissions: 0,
                        has_permissions: 0,
                    });
                }
                for (slot, name) in mounted.entries.iter_mut().zip(mounted.names.iter()) {
                    slot.name = name.as_ptr();
                }
                IcListing {
                    items: mounted.entries.as_ptr(),
                    count: mounted.entries.len() as u32,
                }
            }
            Err(reason) => {
                mounted.remember(reason);
                IcListing::EMPTY
            }
        }
    })
    .unwrap_or(IcListing::EMPTY)
}

pub extern "C" fn read(handle: IcFsHandle, path: *const c_char) -> IcBytes {
    let path = wanted(path);
    held(handle, |mounted| {
        let wanted = mounted.mount.config().resolve(&path);
        match mounted.mount.with(|session| session.read(&wanted)) {
            Ok(body) => {
                mounted.body = body;
                IcBytes {
                    data: mounted.body.as_ptr(),
                    len: mounted.body.len() as u64,
                }
            }
            Err(reason) => {
                mounted.remember(reason);
                IcBytes::EMPTY
            }
        }
    })
    .unwrap_or(IcBytes::EMPTY)
}

pub extern "C" fn is_read_only(_handle: IcFsHandle) -> c_int {
    0
}

pub extern "C" fn last_error(handle: IcFsHandle) -> *const c_char {
    held(handle, |mounted| {
        mounted
            .failure
            .as_ref()
            .map(|text| text.as_ptr())
            .unwrap_or(std::ptr::null())
    })
    .unwrap_or(std::ptr::null())
}

fn reported(handle: IcFsHandle, outcome: Option<Result<(), String>>) -> c_int {
    match outcome.unwrap_or_else(|| Err("no connection".to_string())) {
        Ok(()) => IC_OK,
        Err(reason) => {
            let _ = held(handle, |mounted| mounted.remember(reason));
            IC_ERR_INIT_FAILED
        }
    }
}

pub extern "C" fn write(
    handle: IcFsHandle,
    path: *const c_char,
    bytes: *const u8,
    len: u64,
) -> c_int {
    let path = wanted(path);
    let body = if bytes.is_null() || len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(bytes, len as usize) }.to_vec()
    };
    let outcome = held(handle, |mounted| {
        let target = mounted.mount.config().resolve(&path);
        mounted.mount.with(|session| session.write(&target, &body))
    });
    reported(handle, outcome)
}

pub extern "C" fn create_dir(handle: IcFsHandle, path: *const c_char) -> c_int {
    let path = wanted(path);
    let outcome = held(handle, |mounted| {
        let target = mounted.mount.config().resolve(&path);
        mounted.mount.with(|session| session.create_dir(&target))
    });
    reported(handle, outcome)
}

pub extern "C" fn remove(handle: IcFsHandle, path: *const c_char) -> c_int {
    let path = wanted(path);
    let outcome = held(handle, |mounted| {
        let target = mounted.mount.config().resolve(&path);
        mounted.mount.with(|session| session.remove(&target))
    });
    reported(handle, outcome)
}

pub extern "C" fn rename(handle: IcFsHandle, from: *const c_char, to: *const c_char) -> c_int {
    let (from, to) = (wanted(from), wanted(to));
    let outcome = held(handle, |mounted| {
        let (from, to) = (
            mounted.mount.config().resolve(&from),
            mounted.mount.config().resolve(&to),
        );
        mounted.mount.with(|session| session.rename(&from, &to))
    });
    reported(handle, outcome)
}

extern "C" fn never_opened_inside_a_file(
    _source: ic_plugin_api::IcFsSource,
    _path: *const c_char,
    _user_data: *mut c_void,
) -> IcFsHandle {
    std::ptr::null_mut()
}

pub fn vtable() -> IcFsVTable {
    IcFsVTable {
        struct_size: std::mem::size_of::<IcFsVTable>() as u32,
        open_in: never_opened_inside_a_file,
        close,
        list,
        read,
        is_read_only,
        last_error,
        write: Some(write),
        create_dir: Some(create_dir),
        remove: Some(remove),
        rename: Some(rename),
        shell_open: None,
        shell_read: None,
        shell_write: None,
        shell_resize: None,
        shell_close: None,
        shell_available: None,
        columns: None,
        list_rows: None,
        action_state: None,
        cell_clicked: None,
        set_permissions: None,
    }
}

pub fn connection_vtable(fs: *const IcFsVTable) -> IcConnectionVTable {
    IcConnectionVTable {
        struct_size: std::mem::size_of::<IcConnectionVTable>() as u32,
        open,
        fs,
        describe: None,
        on_event: None,
    }
}
