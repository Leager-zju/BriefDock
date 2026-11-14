#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::{ops::Range, path::PathBuf, sync::Mutex};
use tauri::{Emitter, Manager};
mod dock;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Root {
    #[default]
    News,
    Hongguo,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Section {
    #[default]
    Content,
    Modules,
    Settings,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct View {
    root: Root,
    section: Section,
    visible: bool,
    generation: u64,
}
#[derive(Default)]
struct Model {
    view: View,
    feed_requests: u64,
}
impl Model {
    fn transition(&mut self, root: Root, section: Section, visible: bool) -> View {
        if self.view.root != root || self.view.section != section || self.view.visible != visible {
            self.view.generation += 1;
            self.view.root = root;
            self.view.section = section;
            self.view.visible = visible;
        }
        self.view.clone()
    }
    fn accepts(&self, generation: u64) -> bool {
        self.view.visible
            && self.view.section == Section::Content
            && self.view.root == Root::News
            && self.view.generation == generation
    }
}
#[tauri::command]
fn transition(
    root: Root,
    section: Section,
    visible: bool,
    state: tauri::State<Mutex<Model>>,
    window: tauri::WebviewWindow,
) -> View {
    state
        .lock()
        .unwrap()
        .transition(root, section, visible && !dock::is_docked(&window))
}
#[tauri::command]
fn snapshot(state: tauri::State<Mutex<Model>>, window: tauri::WebviewWindow) -> serde_json::Value {
    let state = state.lock().unwrap();
    serde_json::json!({"view":state.view,"feed_requests":state.feed_requests,
        "media_route":"browser_h264_sample", "native_texture_integrated":false,"desktop":dock::snapshot(&window)})
}
#[tauri::command]
async fn demo_feed(
    generation: u64,
    state: tauri::State<'_, Mutex<Model>>,
) -> Result<serde_json::Value, String> {
    {
        let mut state = state.lock().unwrap();
        if !state.accepts(generation) {
            return Err("inactive".into());
        }
        state.feed_requests += 1;
    }
    // Simulates a late service reply without starting a real content service.
    tauri::async_runtime::spawn_blocking(|| {
        std::thread::sleep(std::time::Duration::from_millis(80))
    })
    .await
    .map_err(|_| "sample task failed")?;
    if !state.lock().unwrap().accepts(generation) {
        return Err("stale".into());
    }
    Ok(serde_json::json!([
      {"id":"bilibili","name":"哔哩哔哩","tag":"B","color":"pink","items":["那些让人忍不住重看的电影片段","周末城市漫步，有哪些好去处","把普通的一天拍成电影","这段旋律，你一定听过","开发者们正在讨论的新工具","一份简单实用的周末出行清单","镜头记录下的日常生活","那些值得收藏的学习方法","怎样开始自己的第一个小项目","用十分钟了解一门新技术"]},
      {"id":"weibo","name":"微博热搜","tag":"微","color":"orange","items":["今天有哪些值得关注的事","秋天的城市有了新的颜色","你最想分享的一张照片","年轻人如何安排自己的周末","给日常生活留一点空白","你会选择怎样的通勤方式","一杯咖啡里的片刻放松","城市街角新开的小店","今天遇到的一个温暖瞬间","有哪些简单好做的晚餐"]},
      {"id":"zhihu","name":"知乎热榜","tag":"知","color":"blue","items":["有哪些值得长期坚持的小习惯？","你如何整理自己的信息来源？","什么样的工作环境更让人专注？","有什么相见恨晚的软件？","你最近读到了哪些有趣的观点？","有哪些改善桌面体验的小物件？","你会怎样规划一个自由的下午？","什么样的信息值得长期保存？","如何更轻松地保持阅读习惯？","你有哪些简单有效的休息方法？"]}
    ]))
}
#[tauri::command]
fn contract_templates() -> serde_json::Value {
    // Reuse the existing host-independent contract, rather than create a web-only protocol.
    use lazybuddy_module_contract::Page;
    serde_json::json!([
        Page::NewsGrid {
            catalog_method: "sources".into(),
            fetch_method: "fetch".into()
        },
        Page::MediaLibrary {
            search_method: "search".into(),
            favorites_method: "favorites".into()
        }
    ])
}
fn visibility(window: &tauri::WebviewWindow, visible: bool) {
    let visible = visible && !dock::is_docked(window);
    let state = window.state::<Mutex<Model>>();
    let mut state = state.lock().unwrap();
    let view = state.view.clone();
    let changed = view.visible != visible;
    state.transition(view.root, view.section, visible);
    if changed {
        let _ = window.emit("shell-visibility", visible);
    }
}
fn show_without_focus(window: &tauri::WebviewWindow) -> Result<(), String> {
    dock::expand(window).map_err(|_| "expand failed")?;
    let hwnd = window.hwnd().map_err(|_| "window unavailable")?;
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(hwnd.0 as _, 4);
    }
    visibility(window, true);
    Ok(())
}
#[tauri::command]
fn window_action(action: &str, window: tauri::WebviewWindow) -> Result<(), String> {
    match action {
        "hide" => {
            visibility(&window, false);
            window.hide().map_err(|_| "hide failed")?;
        }
        "minimize" => {
            visibility(&window, false);
            window.minimize().map_err(|_| "minimize failed")?;
        }
        "show" => show_without_focus(&window)?,
        "expand" => dock::expand(&window).map_err(|_| "expand failed")?,
        "close" => window.app_handle().exit(0),
        _ => return Err("unknown window action".into()),
    }
    Ok(())
}

#[tauri::command]
fn resize(width: u32, height: u32, window: tauri::WebviewWindow) -> Result<(), String> {
    if dock::is_docked(&window) {
        return Err("expand before resizing".into());
    }
    if !(440..=1920).contains(&width) || !(440..=1200).contains(&height) {
        return Err("invalid prototype size".into());
    }
    window
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(|_| "resize failed".into())
}

// One fixed synthetic clip only; not a file server, account proxy or arbitrary URL loader.
fn byte_range(header: Option<&str>, length: usize) -> Result<Range<usize>, ()> {
    let Some(header) = header else {
        return Ok(0..length);
    };
    let value = header.strip_prefix("bytes=").ok_or(())?;
    if value.contains(',') || length == 0 {
        return Err(());
    }
    let (start, end) = value.split_once('-').ok_or(())?;
    if start.is_empty() {
        let suffix = end.parse::<usize>().map_err(|_| ())?;
        if suffix == 0 {
            return Err(());
        }
        return Ok(length.saturating_sub(suffix)..length);
    }
    let start = start.parse::<usize>().map_err(|_| ())?;
    let end = if end.is_empty() {
        length
    } else {
        end.parse::<usize>()
            .map_err(|_| ())?
            .saturating_add(1)
            .min(length)
    };
    if start >= end {
        return Err(());
    }
    Ok(start..end)
}
fn clip_response(request: &tauri::http::Request<Vec<u8>>) -> tauri::http::Response<Vec<u8>> {
    use tauri::http::Response;
    let reject = |status| Response::builder().status(status).body(vec![]).unwrap();
    if request.uri().path() != "/clip" || !matches!(request.method().as_str(), "GET" | "HEAD") {
        return reject(404);
    }
    let Some(path) = std::env::var_os("LAZYBUDDY_PROTOTYPE_CLIP")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()?
                .parent()
                .map(|directory| directory.join("sample.mp4"))
        })
    else {
        return reject(404);
    };
    let Ok(metadata) = path.metadata() else {
        return reject(404);
    };
    if metadata.len() > 8 * 1024 * 1024 {
        return reject(413);
    }
    let length = metadata.len() as usize;
    let requested = request.headers().get("range");
    let Ok(range) = byte_range(requested.and_then(|v| v.to_str().ok()), length) else {
        return Response::builder()
            .status(416)
            .header("Content-Range", format!("bytes */{length}"))
            .body(vec![])
            .unwrap();
    };
    let mut response = Response::builder()
        .status(if requested.is_some() { 206 } else { 200 })
        .header("Content-Type", "video/mp4")
        .header("Accept-Ranges", "bytes")
        .header("Access-Control-Allow-Origin", "http://tauri.localhost")
        .header("Cache-Control", "no-store")
        .header("Content-Length", range.len());
    if requested.is_some() {
        response = response.header(
            "Content-Range",
            format!("bytes {}-{}/{length}", range.start, range.end - 1),
        );
    }
    if request.method() == "HEAD" {
        return response.body(vec![]).unwrap();
    }
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(path) else {
        return reject(404);
    };
    let mut bytes = vec![0; range.len()];
    if file.seek(SeekFrom::Start(range.start as u64)).is_err()
        || file.read_exact(&mut bytes).is_err()
    {
        return reject(500);
    }
    response.body(bytes).unwrap()
}
fn main() {
    tauri::Builder::default()
        .manage(Mutex::new(Model::default()))
        .manage(Mutex::new(dock::Dock::default()))
        .invoke_handler(tauri::generate_handler![
            transition,
            snapshot,
            demo_feed,
            contract_templates,
            window_action,
            resize
        ])
        .register_uri_scheme_protocol("sample", |_, request| clip_response(&request))
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            dock::attach(&window)?;
            unsafe {
                // Keep the product's rectangular-window convention.
                let hwnd = window.hwnd()?;
                let corner: u32 = 1;
                windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
                    hwnd.0 as _,
                    33,
                    (&corner as *const u32).cast(),
                    4,
                );
            }
            if std::env::var_os("LAZYBUDDY_PROTOTYPE_PROBE").is_none() {
                show_without_focus(&window).map_err(std::io::Error::other)?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                window.app_handle().exit(0);
            }
            if let tauri::WindowEvent::Resized(_) = event {
                if let Some(webview) = window.app_handle().get_webview_window(window.label()) {
                    if dock::is_docked(&webview) {
                        let _ = dock::shape(&webview);
                    }
                    visibility(
                        &webview,
                        !window.is_minimized().unwrap_or(false)
                            && window.is_visible().unwrap_or(false),
                    );
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("prototype launch failed");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_entry_does_not_renew_but_a_b_a_invalidates_old_results() {
        let mut model = Model::default();
        let first = model.transition(Root::News, Section::Content, true);
        assert_eq!(
            model
                .transition(Root::News, Section::Content, true)
                .generation,
            first.generation
        );
        model.transition(Root::Hongguo, Section::Content, true);
        model.transition(Root::News, Section::Content, true);
        assert!(!model.accepts(first.generation));
        assert!(model.accepts(model.view.generation));
    }
    #[test]
    fn hidden_and_settings_reject_content_work() {
        let mut model = Model::default();
        for (section, visible) in [
            (Section::Settings, true),
            (Section::Modules, true),
            (Section::Content, false),
        ] {
            let view = model.transition(Root::News, section, visible);
            assert!(!model.accepts(view.generation));
        }
    }
    #[test]
    fn single_ranges_support_seek_and_reject_invalid_or_multiple_ranges() {
        assert_eq!(byte_range(Some("bytes=20-39"), 100), Ok(20..40));
        assert_eq!(byte_range(Some("bytes=90-"), 100), Ok(90..100));
        assert_eq!(byte_range(Some("bytes=-10"), 100), Ok(90..100));
        for input in [
            "bytes=100-",
            "bytes=10-4",
            "bytes=0-1,4-5",
            "bytes=-0",
            "oops",
        ] {
            assert!(byte_range(Some(input), 100).is_err());
        }
    }
}
