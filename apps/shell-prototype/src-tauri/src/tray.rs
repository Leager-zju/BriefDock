use tauri::{
    Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

fn menu_action(id: &str) -> Option<&'static str> {
    match id {
        "shell-show" => Some("show"),
        "shell-quit" => Some("quit"),
        _ => None,
    }
}

fn dispatch(app: &tauri::AppHandle, action: &str) {
    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = super::window_action(action, window) {
            use tauri::Emitter;
            let _ = app.emit("shell-error", error);
        }
    }
}

pub fn install(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "shell-show", "显示 LazyBuddy", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "shell-quit", "退出", true, None::<&str>)?;
    TrayIconBuilder::with_id("shell")
        .icon(
            app.default_window_icon()
                .expect("bundled application icon")
                .clone(),
        )
        .tooltip("LazyBuddy")
        .menu(&Menu::with_items(app, &[&show, &quit])?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if let Some(action) = menu_action(event.id.as_ref()) {
                dispatch(app, action);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                dispatch(tray.app_handle(), "show");
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_our_explicit_exit_item_quits_the_app() {
        assert_eq!(menu_action("shell-show"), Some("show"));
        assert_eq!(menu_action("shell-quit"), Some("quit"));
        assert_eq!(menu_action("close"), None);
        assert_eq!(menu_action("quit"), None);
    }
}
