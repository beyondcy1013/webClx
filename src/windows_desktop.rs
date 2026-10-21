use std::{io, net::SocketAddr};

#[cfg(not(windows))]
pub struct SingleInstance;

#[cfg(not(windows))]
impl SingleInstance {
    pub fn acquire() -> io::Result<Self> {
        Ok(Self)
    }

    pub fn notification_event(&self) -> isize {
        0
    }
}

#[cfg(not(windows))]
pub fn spawn_tray(_listen_addr: SocketAddr, _notification_event: isize) {}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{ffi::OsStr, iter::once, os::windows::ffi::OsStrExt, ptr, thread};
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, LRESULT, POINT,
            WPARAM,
        },
        System::{
            LibraryLoader::GetModuleHandleW,
            Threading::{CreateEventW, CreateMutexW, INFINITE, SetEvent, WaitForSingleObject},
        },
        UI::{
            Shell::{
                NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE,
                NIM_MODIFY, NIM_SETVERSION, NOTIFYICON_VERSION_4, NOTIFYICONDATAW,
                Shell_NotifyIconW, ShellExecuteW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
                DispatchMessageW, GetCursorPos, GetMessageW, HWND_MESSAGE, LoadIconW, MF_STRING,
                MSG, PostMessageW, RegisterClassW, SW_SHOWNORMAL, SetForegroundWindow,
                TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
                TranslateMessage, WM_APP, WM_CONTEXTMENU, WM_LBUTTONDBLCLK, WM_RBUTTONUP,
                WNDCLASSW,
            },
        },
    };

    const TRAY_ID: u32 = 1;
    const TRAY_CALLBACK: u32 = WM_APP + 1;
    const SHOW_ALREADY_RUNNING: u32 = WM_APP + 2;
    const MENU_OPEN_WEBUI: usize = 1001;
    const MUTEX_NAME: &str = "Global\\webClx-SingleInstance";
    const EVENT_NAME: &str = "Global\\webClx-ShowAlreadyRunning";

    static WEBUI_URL: std::sync::OnceLock<Vec<u16>> = std::sync::OnceLock::new();

    pub struct SingleInstance {
        mutex: HANDLE,
        notification_event: HANDLE,
    }

    impl SingleInstance {
        pub fn acquire() -> io::Result<Self> {
            let mutex_name = wide(MUTEX_NAME);
            let mutex = unsafe { CreateMutexW(ptr::null(), 0, mutex_name.as_ptr()) };
            if mutex.is_null() {
                return Err(io::Error::last_os_error());
            }
            let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
            let event_name = wide(EVENT_NAME);
            let notification_event =
                unsafe { CreateEventW(ptr::null(), 0, 0, event_name.as_ptr()) };
            if notification_event.is_null() {
                unsafe { CloseHandle(mutex) };
                return Err(io::Error::last_os_error());
            }
            if already_running {
                unsafe {
                    SetEvent(notification_event);
                    CloseHandle(notification_event);
                    CloseHandle(mutex);
                }
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, "webClx 已经启动"));
            }
            Ok(Self {
                mutex,
                notification_event,
            })
        }

        pub fn notification_event(&self) -> isize {
            self.notification_event as isize
        }
    }

    impl Drop for SingleInstance {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.notification_event);
                CloseHandle(self.mutex);
            }
        }
    }

    pub fn spawn_tray(listen_addr: SocketAddr, notification_event: isize) {
        let url = format!("http://127.0.0.1:{}", listen_addr.port());
        let _ = WEBUI_URL.set(wide(&url));
        thread::spawn(move || unsafe { run_tray(notification_event as HANDLE) });
    }

    unsafe fn run_tray(notification_event: HANDLE) {
        let class_name = wide("webClx.TrayWindow");
        let instance = unsafe { GetModuleHandleW(ptr::null()) };
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            ..unsafe { std::mem::zeroed() }
        };
        unsafe { RegisterClassW(&window_class) };
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class_name.as_ptr(),
                class_name.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                ptr::null_mut(),
                instance,
                ptr::null(),
            )
        };
        if hwnd.is_null() {
            return;
        }

        let mut icon_data = tray_data(hwnd);
        icon_data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        icon_data.uCallbackMessage = TRAY_CALLBACK;
        icon_data.hIcon = unsafe { LoadIconW(instance, 1usize as _) };
        copy_wide(&mut icon_data.szTip, "webClx");
        if unsafe { Shell_NotifyIconW(NIM_ADD, &icon_data) } == 0 {
            return;
        }
        icon_data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        unsafe { Shell_NotifyIconW(NIM_SETVERSION, &icon_data) };

        let hwnd_value = hwnd as usize;
        let notification_event_value = notification_event as usize;
        thread::spawn(move || {
            loop {
                unsafe { WaitForSingleObject(notification_event_value as HANDLE, INFINITE) };
                unsafe { PostMessageW(hwnd_value as HWND, SHOW_ALREADY_RUNNING, 0, 0) };
            }
        });

        let mut message: MSG = unsafe { std::mem::zeroed() };
        while unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) } > 0 {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        unsafe { Shell_NotifyIconW(NIM_DELETE, &icon_data) };
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            SHOW_ALREADY_RUNNING => unsafe { show_already_running(hwnd) },
            TRAY_CALLBACK => match (lparam as u32) & 0xffff {
                WM_LBUTTONDBLCLK => unsafe { open_webui(hwnd) },
                WM_RBUTTONUP | WM_CONTEXTMENU => unsafe { show_menu(hwnd) },
                _ => {}
            },
            _ => return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
        0
    }

    unsafe fn open_webui(hwnd: HWND) {
        let operation = wide("open");
        let Some(url) = WEBUI_URL.get() else {
            return;
        };
        unsafe {
            ShellExecuteW(
                hwnd,
                operation.as_ptr(),
                url.as_ptr(),
                ptr::null(),
                ptr::null(),
                SW_SHOWNORMAL,
            );
        }
    }

    unsafe fn show_menu(hwnd: HWND) {
        let menu = unsafe { CreatePopupMenu() };
        if menu.is_null() {
            return;
        }
        let label = wide("打开 WebUI");
        unsafe {
            AppendMenuW(menu, MF_STRING, MENU_OPEN_WEBUI, label.as_ptr());
            SetForegroundWindow(hwnd);
        }
        let mut point = POINT::default();
        unsafe { GetCursorPos(&mut point) };
        let command = unsafe {
            TrackPopupMenu(
                menu,
                TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
                point.x,
                point.y,
                0,
                hwnd,
                ptr::null(),
            )
        };
        unsafe { DestroyMenu(menu) };
        if command as usize == MENU_OPEN_WEBUI {
            unsafe { open_webui(hwnd) };
        }
    }

    unsafe fn show_already_running(hwnd: HWND) {
        let mut icon_data = tray_data(hwnd);
        icon_data.uFlags = NIF_INFO;
        icon_data.dwInfoFlags = NIIF_INFO;
        copy_wide(&mut icon_data.szInfoTitle, "webClx");
        copy_wide(&mut icon_data.szInfo, "webClx 已经启动，托盘中已有运行实例。");
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &icon_data) };
    }

    fn tray_data(hwnd: HWND) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: TRAY_ID,
            ..unsafe { std::mem::zeroed() }
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        OsStr::new(value).encode_wide().chain(once(0)).collect()
    }

    fn copy_wide<const N: usize>(target: &mut [u16; N], value: &str) {
        let encoded = wide(value);
        let length = encoded.len().min(N);
        target[..length].copy_from_slice(&encoded[..length]);
    }
}

#[cfg(windows)]
pub use windows::{SingleInstance, spawn_tray};
