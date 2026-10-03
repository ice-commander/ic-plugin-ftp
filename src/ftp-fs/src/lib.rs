mod ftp;
pub mod mount;
use ic_plugin_api::{
    check_host, needs_plugin_assets, HostCheck, IcHost, IC_ABI_VERSION, IC_ERR_HOST_TOO_OLD,
    IC_ERR_HOST_UNKNOWN, IC_ERR_INIT_FAILED,
};
use std::ffi::CString;
use std::os::raw::{c_char, c_int};

pub const KIND: &str = "ftp";
pub const DOCUMENT: &str = include_str!("../documents/ftp.json");
pub const ID: &str = "ic-ftp-fs";
pub const ICON: &str = "ftp.svg";
pub const PICTURE: &[u8] = include_bytes!("../assets/ftp.svg");

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../version.rs"));

ic_plugin_api::declare_about!("ic-ftp-fs", "FTP", plugins_version!(), "FTP connections");

#[cfg_attr(feature = "export-abi", no_mangle)]
pub extern "C" fn ic_plugin_init(host: *const IcHost, _kind: *const c_char) -> c_int {
    match check_host(host, IC_ABI_VERSION, needs_plugin_assets()) {
        HostCheck::Ok => {}
        HostCheck::WrongMagic => return IC_ERR_HOST_UNKNOWN,
        HostCheck::TooOld { .. } | HostCheck::Truncated { .. } => return IC_ERR_HOST_TOO_OLD,
    }
    let Ok(id) = CString::new(KIND) else {
        return IC_ERR_INIT_FAILED;
    };
    if let (Ok(owner), Ok(name)) = (CString::new(ID), CString::new(ICON)) {
        unsafe {
            ((*host).register_plugin_asset)(
                owner.as_ptr(),
                name.as_ptr(),
                PICTURE.as_ptr(),
                PICTURE.len() as u64,
            )
        };
    }
    let fs = mount::vtable();
    let connection = mount::connection_vtable(&fs);
    unsafe {
        ((*host).register_connection_kind)(
            id.as_ptr(),
            DOCUMENT.as_ptr(),
            DOCUMENT.len() as u64,
            &connection,
            std::ptr::null_mut(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_plugin_refuses_a_host_it_does_not_recognise() {
        assert_eq!(
            ic_plugin_init(std::ptr::null(), std::ptr::null()),
            IC_ERR_HOST_UNKNOWN
        );
    }

    #[test]
    fn a_mount_refuses_settings_that_name_no_server() {
        for settings in ["{}", r#"{"user":"ivan"}"#, r#"{"host":""}"#] {
            let handle = mount::open(
                settings.as_ptr(),
                settings.len() as u64,
                std::ptr::null_mut(),
            );
            assert!(handle.is_null(), "opened on {settings}");
        }
    }

    #[test]
    fn a_mount_defaults_the_port_and_the_anonymous_user_the_way_ftp_does() {
        let settings = r#"{"host":"ftp.example.org"}"#;
        let handle = mount::open(
            settings.as_ptr(),
            settings.len() as u64,
            std::ptr::null_mut(),
        );
        assert!(
            !handle.is_null(),
            "a host alone is enough for anonymous ftp"
        );
        mount::close(handle);
    }

    #[test]
    fn the_server_is_mounted_whole_whatever_the_start_folder_says() {
        let cfg = crate::ftp::Config {
            host: "ftp.example.org".to_string(),
            port: 21,
            user: "ivan".to_string(),
            pass: String::new(),
        };
        assert_eq!(cfg.resolve(""), "/");
        assert_eq!(cfg.resolve("/"), "/");
        assert_eq!(cfg.resolve("docs"), "/docs");
        assert_eq!(cfg.resolve("/var/www/docs"), "/var/www/docs");
    }

    #[test]
    fn the_filesystem_side_offers_every_operation_a_server_supports() {
        let table = mount::vtable();
        assert!(table.write.is_some());
        assert!(table.create_dir.is_some());
        assert!(table.remove.is_some());
        assert!(table.rename.is_some());
    }
}
