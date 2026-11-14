//! Windows events wake the host; no desktop polling loop is needed.
use super::{Message, shortcut::Shortcut};
use futures::{SinkExt, Stream, StreamExt, channel::mpsc};
use std::{io, sync::OnceLock};
use windows_sys::Win32::{
    Foundation::*,
    UI::{
        Input::KeyboardAndMouse::{UnregisterHotKey, VK_ESCAPE},
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};
use windows_sys::{
    Win32::{
        Graphics::{
            Dwm::{DWMNCRP_DISABLED, DWMWA_NCRENDERING_POLICY, DwmSetWindowAttribute},
            Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn},
        },
        UI::HiDpi::GetDpiForWindow,
    },
    core::w,
};

static EVENTS: OnceLock<mpsc::UnboundedSender<Message>> = OnceLock::new();
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
}
pub(super) fn emit(message: Message) {
    if let Some(sender) = EVENTS.get() {
        let _ = sender.unbounded_send(message);
    }
}
pub fn events() -> impl Stream<Item = Message> {
    iced::stream::channel(32, |mut output: mpsc::Sender<Message>| async move {
        let (sender, mut receiver) = mpsc::unbounded();
        let _ = EVENTS.set(sender);
        let _ = output.send(Message::Ready).await;
        while let Some(message) = receiver.next().await {
            if output.send(message).await.is_err() {
                break;
            }
        }
    })
}
pub fn handle(window: &dyn iced::window::Window) -> usize {
    match window.window_handle().map(|handle| handle.as_raw()) {
        Ok(iced::window::raw_window_handle::RawWindowHandle::Win32(handle)) => {
            handle.hwnd.get() as usize
        }
        _ => 0,
    }
}
unsafe extern "system" fn subclass(
    window: HWND,
    event: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    match event {
        WM_KILLFOCUS => emit(Message::CancelShortcut),
        WM_KEYDOWN | WM_SYSKEYDOWN
            if !unsafe { GetPropW(window, w!("LazyBuddy.ShortcutCapture")) }.is_null() =>
        {
            if lparam & (1 << 30) == 0 {
                if wparam as u16 == VK_ESCAPE {
                    emit(Message::CancelShortcut);
                } else if let Some(shortcut) = Shortcut::capture(wparam as u16) {
                    emit(Message::Shortcut(shortcut));
                }
            }
            return 0;
        }
        WM_ENTERSIZEMOVE => unsafe {
            RemovePropW(window, w!("LazyBuddy.Moving"));
        },
        WM_MOVING => unsafe {
            SetPropW(window, w!("LazyBuddy.Moving"), 1usize as HANDLE);
        },
        WM_EXITSIZEMOVE => {
            emit(Message::SaveGeometry);
            if !unsafe { RemovePropW(window, w!("LazyBuddy.Moving")) }.is_null() {
                emit(Message::Dock(None));
            }
        }
        WM_NCCALCSIZE | WM_NCPAINT => return 0,
        WM_NCACTIVATE => return unsafe { DefSubclassProc(window, event, wparam, -1) },
        WM_STYLECHANGING => {
            let styles = unsafe { &mut *(lparam as *mut STYLESTRUCT) };
            if wparam as i32 == GWL_STYLE {
                styles.styleNew &= !(WS_CAPTION | WS_BORDER | WS_MINIMIZEBOX | WS_MAXIMIZEBOX);
            } else if wparam as i32 == GWL_EXSTYLE {
                styles.styleNew &=
                    !(WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_DLGMODALFRAME | WS_EX_STATICEDGE);
            }
        }
        WM_NCHITTEST => {
            if !unsafe { GetPropW(window, w!("LazyBuddy.Capsule")) }.is_null() {
                return HTCLIENT as LRESULT;
            }
            let mut bounds = RECT::default();
            if unsafe { GetWindowRect(window, &mut bounds) } != 0 {
                let padding =
                    (6. * unsafe { GetDpiForWindow(window) }.max(96) as f32 / 96.).round() as i32;
                if let Some(hit) = hit_edges(
                    &bounds,
                    i32::from(lparam as i16),
                    i32::from((lparam >> 16) as i16),
                    padding,
                ) {
                    return hit as LRESULT;
                }
            }
        }
        WM_DPICHANGED => emit(Message::Geometry),
        WM_SHOWWINDOW => emit(Message::Visibility(
            wparam != 0 && unsafe { IsIconic(window) } == 0,
        )),
        WM_SIZE => emit(Message::Visibility(
            wparam as u32 != SIZE_MINIMIZED && unsafe { IsWindowVisible(window) } != 0,
        )),
        WM_HOTKEY => {
            if !unsafe { GetPropW(window, w!("LazyBuddy.ShortcutCapture")) }.is_null() {
                if let Some(shortcut) =
                    Shortcut::from_parts(lparam as u32 & 0xffff, (lparam >> 16) as u16)
                {
                    emit(Message::Shortcut(shortcut));
                }
            } else {
                emit(Message::ToggleWindow);
            }
        }
        _ => {}
    }
    unsafe { DefSubclassProc(window, event, wparam, lparam) }
}

pub struct Desktop {
    handle: usize,
    _tray: Option<tray_icon::TrayIcon>,
    pub hotkey_registered: bool,
    shape: (i32, i32, u32),
    expanded: Option<RECT>,
    shortcut: Shortcut,
    hotkey_id: i32,
}
impl Desktop {
    pub fn new(handle: usize, shortcut: Shortcut) -> io::Result<Self> {
        let window = handle as HWND;
        if window.is_null() {
            return Err(io::Error::other("missing native window"));
        }
        if unsafe { SetWindowSubclass(window, Some(subclass), 0x4c42, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let isolated =
            std::env::var("LAZYBUDDY_INSTANCE").is_ok_and(|instance| instance != "default");
        let hotkey_registered = !isolated && shortcut.register(window, 0x4c42);
        let mut desktop = Self {
            handle,
            _tray: None,
            hotkey_registered,
            shape: (0, 0, 0),
            expanded: None,
            shortcut,
            hotkey_id: 0x4c42,
        };
        if !isolated {
            desktop._tray = Some(Self::tray()?);
        }
        unsafe {
            SetPropW(window, w!("LazyBuddy.Window"), 1usize as HANDLE);
            SetWindowLongW(
                window,
                GWL_STYLE,
                (GetWindowLongW(window, GWL_STYLE) as u32
                    & !(WS_CAPTION | WS_BORDER | WS_MINIMIZEBOX | WS_MAXIMIZEBOX))
                    as i32,
            );
            let policy = DWMNCRP_DISABLED;
            DwmSetWindowAttribute(
                window,
                DWMWA_NCRENDERING_POLICY as u32,
                std::ptr::from_ref(&policy).cast(),
                std::mem::size_of_val(&policy) as u32,
            );
            SetWindowPos(
                window,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
        desktop.clip();
        Ok(desktop)
    }
    fn tray() -> io::Result<tray_icon::TrayIcon> {
        use tray_icon::{
            MouseButton, TrayIconBuilder, TrayIconEvent,
            menu::{Menu, MenuEvent, MenuItem},
        };
        let menu = Menu::new();
        let show = MenuItem::new("显示 LazyBuddy", true, None);
        let settings = MenuItem::new("设置", true, None);
        let exit = MenuItem::new("退出", true, None);
        menu.append_items(&[&show, &settings, &exit])
            .map_err(io::Error::other)?;
        let show_id = show.id().clone();
        let settings_id = settings.id().clone();
        let exit_id = exit.id().clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if event.id == show_id {
                emit(Message::ShowWindow);
            } else if event.id == settings_id {
                emit(Message::ShowWindow);
                emit(Message::Navigate(super::Section::Settings(
                    super::Settings::System,
                )));
            } else if event.id == exit_id {
                emit(Message::Quit);
            }
        }));
        TrayIconEvent::set_event_handler(Some(|event| {
            if matches!(
                event,
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                emit(Message::ShowWindow);
            }
        }));
        let image = ::image::load_from_memory(include_bytes!("../../../assets/lazybuddy.ico"))
            .map_err(io::Error::other)?
            .thumbnail(32, 32)
            .to_rgba8();
        let icon =
            tray_icon::Icon::from_rgba(image.as_raw().clone(), image.width(), image.height())
                .map_err(io::Error::other)?;
        TrayIconBuilder::new()
            .with_tooltip("LazyBuddy · 你的桌面搭子")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_icon(icon)
            .build()
            .map_err(io::Error::other)
    }
    pub fn show(&self) {
        unsafe {
            ShowWindow(self.handle as HWND, SW_SHOWNOACTIVATE);
        }
    }
    pub fn capture_shortcut(&self, enabled: bool) {
        unsafe {
            if enabled {
                SetPropW(
                    self.handle as HWND,
                    w!("LazyBuddy.ShortcutCapture"),
                    1usize as HANDLE,
                );
            } else {
                RemovePropW(self.handle as HWND, w!("LazyBuddy.ShortcutCapture"));
            }
        }
    }
    pub fn set_shortcut(&mut self, shortcut: Shortcut) -> bool {
        if lazybuddy_host_control::isolated() {
            return false;
        }
        if self.hotkey_registered && self.shortcut == shortcut {
            return true;
        }
        let id = if self.hotkey_id == 0x4c42 {
            0x4c43
        } else {
            0x4c42
        };
        if !shortcut.register(self.handle as HWND, id) {
            return false;
        }
        unsafe {
            UnregisterHotKey(self.handle as HWND, self.hotkey_id);
        }
        self.shortcut = shortcut;
        self.hotkey_id = id;
        self.hotkey_registered = true;
        true
    }
    pub fn bounds(&self) -> Option<[i32; 4]> {
        let mut bounds = RECT::default();
        (unsafe { GetWindowRect(self.handle as HWND, &mut bounds) } != 0).then_some([
            bounds.left,
            bounds.top,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
        ])
    }
    pub fn restore_bounds(&self, [x, y, width, height]: [i32; 4]) {
        use windows_sys::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect,
        };
        let bounds = RECT {
            left: x,
            top: y,
            right: x + width,
            bottom: y + height,
        };
        let mut monitor = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe {
            GetMonitorInfoW(
                MonitorFromRect(&bounds, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            )
        } != 0
        {
            let area = monitor.rcWork;
            let width = width.min(area.right - area.left);
            let height = height.min(area.bottom - area.top);
            unsafe {
                SetWindowPos(
                    self.handle as HWND,
                    std::ptr::null_mut(),
                    x.clamp(area.left, area.right - width),
                    y.clamp(area.top, area.bottom - height),
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
        }
    }
    pub fn dock(&mut self, edge: Option<Edge>) -> bool {
        use windows_sys::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
        };
        if self.expanded.is_some() {
            return false;
        }
        let window = self.handle as HWND;
        let mut bounds = RECT::default();
        let mut monitor = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetWindowRect(window, &mut bounds) } == 0
            || unsafe {
                GetMonitorInfoW(
                    MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST),
                    &mut monitor,
                )
            } == 0
        {
            return false;
        }
        let scale = unsafe { GetDpiForWindow(window) }.max(96) as f32 / 96.;
        let edge =
            edge.or_else(|| dock_edge(&bounds, &monitor.rcWork, (12. * scale).round() as i32));
        let Some(edge) = edge else {
            return false;
        };
        let width = (32. * scale).round() as i32;
        let height = (96. * scale).round() as i32;
        let x = if edge == Edge::Left {
            monitor.rcWork.left
        } else {
            monitor.rcWork.right - width
        };
        let y = ((bounds.top + bounds.bottom - height) / 2).clamp(
            monitor.rcWork.top,
            (monitor.rcWork.bottom - height).max(monitor.rcWork.top),
        );
        if unsafe {
            SetWindowPos(
                window,
                std::ptr::null_mut(),
                x,
                y,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        } == 0
        {
            return false;
        }
        self.expanded = Some(bounds);
        unsafe {
            SetPropW(window, w!("LazyBuddy.Capsule"), 1usize as HANDLE);
        }
        self.clip();
        true
    }
    pub fn expand(&mut self) -> bool {
        let Some(bounds) = self.expanded else {
            return false;
        };
        if unsafe {
            SetWindowPos(
                self.handle as HWND,
                std::ptr::null_mut(),
                bounds.left,
                bounds.top,
                bounds.right - bounds.left,
                bounds.bottom - bounds.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        } == 0
        {
            return false;
        }
        self.expanded = None;
        unsafe {
            RemovePropW(self.handle as HWND, w!("LazyBuddy.Capsule"));
        }
        self.clip();
        true
    }
    pub fn clip(&mut self) {
        let window = self.handle as HWND;
        let mut bounds = RECT::default();
        if unsafe { GetWindowRect(window, &mut bounds) } == 0 {
            return;
        }
        let (width, height, dpi) = (
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            unsafe { GetDpiForWindow(window) }.max(96),
        );
        if width <= 0 || height <= 0 || self.shape == (width, height, dpi) {
            return;
        }
        let radius = (32. * dpi as f32 / 96.).round() as i32;
        let region = unsafe { CreateRoundRectRgn(0, 0, width + 1, height + 1, radius, radius) };
        if !region.is_null() {
            if unsafe { SetWindowRgn(window, region, 1) } == 0 {
                unsafe { DeleteObject(region) };
            } else {
                self.shape = (width, height, dpi);
            }
        }
    }
    pub fn hide(&self) {
        unsafe {
            ShowWindow(self.handle as HWND, SW_HIDE);
        }
    }
    pub fn toggle(&self) {
        if unsafe { IsWindowVisible(self.handle as HWND) } != 0
            && unsafe { IsIconic(self.handle as HWND) } == 0
        {
            self.hide();
        } else {
            self.show();
        }
    }
}
impl Drop for Desktop {
    fn drop(&mut self) {
        unsafe {
            UnregisterHotKey(self.handle as HWND, self.hotkey_id);
            RemoveWindowSubclass(self.handle as HWND, Some(subclass), 0x4c42);
            RemovePropW(self.handle as HWND, w!("LazyBuddy.Window"));
            RemovePropW(self.handle as HWND, w!("LazyBuddy.Moving"));
            RemovePropW(self.handle as HWND, w!("LazyBuddy.Capsule"));
            RemovePropW(self.handle as HWND, w!("LazyBuddy.ShortcutCapture"));
        }
    }
}

fn dock_edge(bounds: &RECT, work: &RECT, padding: i32) -> Option<Edge> {
    if bounds.left <= work.left + padding {
        Some(Edge::Left)
    } else if bounds.right >= work.right - padding {
        Some(Edge::Right)
    } else {
        None
    }
}

fn hit_edges(bounds: &RECT, x: i32, y: i32, padding: i32) -> Option<i32> {
    let left = x < bounds.left + padding;
    let right = x >= bounds.right - padding;
    let top = y < bounds.top + padding;
    let bottom = y >= bounds.bottom - padding;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(HTTOPLEFT as i32),
        (_, true, true, _) => Some(HTTOPRIGHT as i32),
        (true, _, _, true) => Some(HTBOTTOMLEFT as i32),
        (_, true, _, true) => Some(HTBOTTOMRIGHT as i32),
        (true, _, _, _) => Some(HTLEFT as i32),
        (_, true, _, _) => Some(HTRIGHT as i32),
        (_, _, true, _) => Some(HTTOP as i32),
        (_, _, _, true) => Some(HTBOTTOM as i32),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn docking_uses_the_current_monitors_work_area() {
        let work = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        };
        assert_eq!(
            dock_edge(
                &RECT {
                    left: -1910,
                    top: 100,
                    right: -1000,
                    bottom: 600
                },
                &work,
                12
            ),
            Some(Edge::Left)
        );
        assert_eq!(
            dock_edge(
                &RECT {
                    left: -900,
                    top: 100,
                    right: -10,
                    bottom: 600
                },
                &work,
                12
            ),
            Some(Edge::Right)
        );
        assert_eq!(
            dock_edge(
                &RECT {
                    left: -1200,
                    top: 100,
                    right: -400,
                    bottom: 600
                },
                &work,
                12
            ),
            None
        );
    }
    #[test]
    fn border_hits_preserve_client_controls_and_negative_monitor_coordinates() {
        let bounds = RECT {
            left: -1000,
            top: -200,
            right: -200,
            bottom: 400,
        };
        assert_eq!(hit_edges(&bounds, -1000, -200, 9), Some(HTTOPLEFT as i32));
        assert_eq!(hit_edges(&bounds, -201, 399, 9), Some(HTBOTTOMRIGHT as i32));
        assert_eq!(hit_edges(&bounds, -991, -191, 9), None);
        assert_eq!(hit_edges(&bounds, -209, 200, 9), Some(HTRIGHT as i32));
    }
}
