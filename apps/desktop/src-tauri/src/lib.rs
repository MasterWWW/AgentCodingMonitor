use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};
use vibe_core::{
    install::{doctor, install_hooks, sync_hook_health_from_disk},
    lan,
    paths::{self, first_run_marker},
    server::{init_tracing, start, RunningServer},
    state::{self, HudPresentation},
    store::SessionStore,
    types::{DoctorReport, InstallHooksResult, StatusSnapshot, VibePhase, VibeSource},
    ActionExecutorEvent, ActionStore,
};
use vibe_protocol::ResolvedAction;

const TRAY_ID: &str = "vibe-tray";

struct AppRuntime {
    server: Option<RunningServer>,
    port: u16,
    executor_task: Option<tauri::async_runtime::JoinHandle<()>>,
}

struct AppState {
    runtime: Mutex<AppRuntime>,
    hook_src: Mutex<Option<PathBuf>>,
    hook_search_hints: Mutex<Vec<PathBuf>>,
}

fn hook_search_hints(app: &AppHandle) -> Vec<PathBuf> {
    let name = vibe_core::paths::hook_file_name();
    let mut hints = Vec::new();

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../../..");
    if let Ok(ws) = workspace.canonicalize() {
        hints.push(ws.join("target/debug").join(name));
        hints.push(ws.join("target/release").join(name));
    } else {
        hints.push(workspace.join("target/debug").join(name));
        hints.push(workspace.join("target/release").join(name));
    }

    if let Ok(p) = app.path().resource_dir() {
        hints.push(p.join("binaries").join(name));
        hints.push(p.join(name));
    }
    if let Ok(sidecar) = app.path().resolve(
        name,
        tauri::path::BaseDirectory::Resource,
    ) {
        hints.push(sidecar);
    }

    hints
}

fn hook_binary_src(app: &AppHandle) -> Option<PathBuf> {
    vibe_core::paths::discover_hook_binary(&hook_search_hints(app))
}

/// 重载 embedded `vibe-core`（切换局域网/手表伴侣绑定地址时使用）。
fn reload_server(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let hook_src = state.hook_src.lock().unwrap().clone();
    let hook_hints = state.hook_search_hints.lock().unwrap().clone();
    let lite = state::load_lite_mode();
    let mut rt = state.runtime.lock().unwrap();
    if let Some(server) = rt.server.take() {
        tauri::async_runtime::block_on(server.stop());
    }
    if let Some(task) = rt.executor_task.take() {
        task.abort();
    }
    let server = tauri::async_runtime::block_on(start(hook_src, hook_hints, lite))
        .map_err(|e| format!("failed to restart server: {e}"))?;
    rt.port = server.port;
    let action_store = server.action_store.clone();
    rt.server = Some(server);
    rt.executor_task = Some(spawn_action_executor(app.clone(), action_store));
    Ok(())
}

/// 获取当前局域网看板主链接（启用且有可用 IP 时）。
fn companion_primary_url(port: u16) -> Option<String> {
    if !state::load_lan_companion_enabled() {
        return None;
    }
    let token = state::load_lan_companion_token()?;
    lan::companion_primary_url(port, &token)
}

/// 链路本地地址（169.254.x.x）回退时的托盘警告文案。
fn companion_link_local_warning() -> &'static str {
    "\n\n⚠ 未检测到正常局域网 IP（192.168/10.x），当前为链路本地地址，iPad 可能无法连接。请检查 WiFi 连接或禁用多余虚拟网卡。"
}

/// 组装看板链接提示文案（含多地址列表与链路本地警告）。
fn companion_url_message(prefix: &str, port: u16, url: &str) -> String {
    let token = state::load_lan_companion_token().unwrap_or_default();
    let all_urls = lan::companion_urls(port, &token);
    let mut msg = format!("{prefix}\n\n{url}");
    if all_urls.len() > 1 {
        msg.push_str("\n\n其他可用地址：\n");
        for alt in all_urls.iter().skip(1) {
            msg.push_str(alt);
            msg.push('\n');
        }
    }
    if lan::companion_uses_link_local_fallback() {
        msg.push_str(companion_link_local_warning());
    }
    msg
}

/// 复制看板链接到剪贴板。
fn copy_companion_url(app: &AppHandle) {
    let port = {
        let state = app.state::<AppState>();
        let rt = state.runtime.lock().unwrap();
        rt.port
    };
    let Some(url) = companion_primary_url(port) else {
        show_message(
            "iPad 看板",
            "请先启用 iPad 看板，并确保电脑已连接 WiFi（有局域网 IP）。",
        );
        return;
    };
    match arboard::Clipboard::new().and_then(|mut c| c.set_text(&url)) {
        Ok(()) => show_message(
            "iPad 看板",
            &companion_url_message("已复制链接：", port, &url),
        ),
        Err(e) => show_message("复制失败", &e.to_string()),
    }
}

/// 生成二维码图片并打开，供 iPad 扫码配对。
fn show_companion_qr(app: &AppHandle) {
    let port = {
        let state = app.state::<AppState>();
        let rt = state.runtime.lock().unwrap();
        rt.port
    };
    let Some(url) = companion_primary_url(port) else {
        show_message(
            "iPad 看板",
            "请先启用 iPad 看板，并确保电脑已连接 WiFi（有局域网 IP）。",
        );
        return;
    };
    let code = match qrcode::QrCode::new(url.as_bytes()) {
        Ok(c) => c,
        Err(e) => {
            show_message("二维码失败", &e.to_string());
            return;
        }
    };
    let image = code.render::<image::Luma<u8>>().quiet_zone(true).min_dimensions(280, 280).build();
    let path = std::env::temp_dir().join("vibe-monitor-companion-qr.png");
    if let Err(e) = image.save(&path) {
        show_message("二维码失败", &e.to_string());
        return;
    }
    let _ = open::that(&path);
    show_message(
        "iPad 看板",
        &format!(
            "{}\n\n二维码图片已打开。",
            companion_url_message("请用 iPad 相机扫描二维码，或在 Safari 打开：", port, &url)
        ),
    );
}

/// 切换局域网看板并重启 HTTP 服务。
fn set_lan_companion_enabled(app: &AppHandle, enabled: bool) {
    if let Err(e) = state::write_lan_companion_enabled(enabled) {
        show_message("iPad 看板", &e.to_string());
        return;
    }
    if enabled {
        let _ = state::ensure_lan_token();
    }
    if let Err(e) = reload_server(app) {
        show_message("iPad 看板", &e.to_string());
        return;
    }
    if enabled {
        copy_companion_url(app);
    }
    refresh_tray_ui(app);
}

/// 手表伴侣：切换开关并重启 HTTP 服务（注册/注销 mDNS）。
fn set_watch_companion_enabled(app: &AppHandle, enabled: bool) {
    if let Err(e) = state::write_watch_companion_enabled(enabled) {
        show_message("手表伴侣", &e.to_string());
        return;
    }
    if enabled {
        let _ = state::ensure_watch_token();
        let _ = state::ensure_device_id();
    }
    if let Err(e) = reload_server(app) {
        show_message("手表伴侣", &e.to_string());
        return;
    }
    if enabled {
        copy_watch_pairing(app);
    }
    refresh_tray_ui(app);
}

fn choice_label_cn(choice: &str) -> &str {
    match choice {
        "approve" => "允许",
        "deny" => "拒绝",
        _ => choice,
    }
}

/// 收到手表回执后在桌面执行：通知 + 剪贴板预填。
fn execute_resolved_action(resolved: &ResolvedAction) {
    if let Some(text) = &resolved.clipboard_text {
        if let Ok(mut clip) = arboard::Clipboard::new() {
            let _ = clip.set_text(text);
        }
    }
    let body = format!(
        "你在手表上选择了：{}\n{}\n\n建议回复已复制，回到终端粘贴后回车",
        choice_label_cn(&resolved.choice),
        resolved.title
    );
    let _ = notify_rust::Notification::new()
        .summary("Vibe Monitor · 手表确认")
        .body(&body)
        .show();
}

fn spawn_action_executor(
    app: AppHandle,
    action_store: ActionStore,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let mut rx = action_store.subscribe_executor();
        loop {
            match rx.recv().await {
                Ok(ActionExecutorEvent::Resolved(resolved)) => {
                    execute_resolved_action(&resolved);
                    let _ = app.emit("watch-action-resolved", &resolved);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    })
}

/// 复制手表扫码配对 JSON 到剪贴板。
fn copy_watch_pairing(app: &AppHandle) {
    let port = {
        let state = app.state::<AppState>();
        let rt = state.runtime.lock().unwrap();
        rt.port
    };
    if !state::load_watch_companion_enabled() {
        show_message(
            "手表伴侣",
            "请先启用手表伴侣，并确保电脑已连接 WiFi（有局域网 IP）。",
        );
        return;
    }
    let pairing = match state::watch_pairing_payload(port) {
        Ok(p) => p,
        Err(e) => {
            show_message("手表伴侣", &e.to_string());
            return;
        }
    };
    let json = match serde_json::to_string(&pairing) {
        Ok(s) => s,
        Err(e) => {
            show_message("手表伴侣", &e.to_string());
            return;
        }
    };
    match arboard::Clipboard::new().and_then(|mut c| c.set_text(&json)) {
        Ok(()) => show_message(
            "手表伴侣",
            &format!(
                "已复制配对信息到剪贴板。\n\nmDNS 服务：{}\n\n也可使用「显示配对二维码」供 Vibe Bridge 扫码。",
                pairing
                    .get("service")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
            ),
        ),
        Err(e) => show_message("复制失败", &e.to_string()),
    }
}

/// 生成手表配对二维码（JSON）并打开图片。
fn show_watch_qr(app: &AppHandle) {
    let port = {
        let state = app.state::<AppState>();
        let rt = state.runtime.lock().unwrap();
        rt.port
    };
    if !state::load_watch_companion_enabled() {
        show_message(
            "手表伴侣",
            "请先启用手表伴侣，并确保电脑已连接 WiFi（有局域网 IP）。",
        );
        return;
    }
    let pairing = match state::watch_pairing_payload(port) {
        Ok(p) => p,
        Err(e) => {
            show_message("手表伴侣", &e.to_string());
            return;
        }
    };
    let json = match serde_json::to_string(&pairing) {
        Ok(s) => s,
        Err(e) => {
            show_message("手表伴侣", &e.to_string());
            return;
        }
    };
    let code = match qrcode::QrCode::new(json.as_bytes()) {
        Ok(c) => c,
        Err(e) => {
            show_message("二维码失败", &e.to_string());
            return;
        }
    };
    let image = code
        .render::<image::Luma<u8>>()
        .quiet_zone(true)
        .min_dimensions(280, 280)
        .build();
    let path = std::env::temp_dir().join("vibe-monitor-watch-qr.png");
    if let Err(e) = image.save(&path) {
        show_message("二维码失败", &e.to_string());
        return;
    }
    let _ = open::that(&path);
    show_message(
        "手表伴侣",
        "二维码图片已打开。请用 Vibe Bridge（手机 App）扫描配对。\n\n配对后手机将通过 mDNS 自动发现本机，无需固定 IP。",
    );
}

#[tauri::command]
fn get_base_url(state: State<'_, AppState>) -> String {
    let rt = state.runtime.lock().unwrap();
    format!("http://127.0.0.1:{}", rt.port)
}

#[tauri::command]
async fn get_status(state: State<'_, AppState>) -> Result<StatusSnapshot, String> {
    let store = {
        let rt = state.runtime.lock().unwrap();
        rt.server
            .as_ref()
            .ok_or("server not started")?
            .store
            .clone()
    };
    Ok(store.snapshot().await)
}

#[tauri::command]
async fn install_hooks_cmd(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<InstallHooksResult, String> {
    let hints = hook_search_hints(&app);
    let src = hook_binary_src(&app).or_else(|| state.hook_src.lock().unwrap().clone());
    let result = install_hooks(src.as_deref(), &hints);
    if result.ok {
        let store = {
            let rt = state.runtime.lock().unwrap();
            rt.server.as_ref().map(|s| s.store.clone())
        };
        if let Some(store) = store {
            vibe_core::install::sync_hook_health_from_disk(&store).await;
        }
    }
    Ok(result)
}

#[tauri::command]
async fn run_doctor(app: AppHandle, state: State<'_, AppState>) -> Result<DoctorReport, String> {
    let src = hook_binary_src(&app).or_else(|| state.hook_src.lock().unwrap().clone());
    let mut report = doctor(src.as_deref()).await;
    let server = {
        let rt = state.runtime.lock().unwrap();
        rt.server.as_ref().map(|s| (s.port, s.store.clone()))
    };
    if let Some((port, store)) = server {
        report.lite_mode = store.get_lite_mode().await;
        report.port = port;
    }
    Ok(report)
}

#[tauri::command]
async fn set_lite_mode(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    let store = {
        let rt = state.runtime.lock().unwrap();
        rt.server
            .as_ref()
            .ok_or("server not started")?
            .store
            .clone()
    };
    store.set_lite_mode(enabled).await;
    Ok(())
}

#[tauri::command]
fn finish_first_run(app: AppHandle) -> Result<(), String> {
    paths::ensure_parent(&first_run_marker().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    std::fs::write(first_run_marker().map_err(|e| e.to_string())?, "ok")
        .map_err(|e| e.to_string())?;

    apply_display_preferences(&app);
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.emit("first-run-complete", ());
        let _ = main.show();
        let _ = main.set_focus();
    }
    Ok(())
}

#[tauri::command]
fn needs_first_run() -> bool {
    !first_run_marker()
        .map(|p| p.exists())
        .unwrap_or(true)
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    open::that(path).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_default_source() -> String {
    match state::load_default_source() {
        VibeSource::Cursor => "cursor".to_string(),
        VibeSource::ClaudeCode => "claude_code".to_string(),
        VibeSource::Codex => "codex".to_string(),
    }
}

#[tauri::command]
fn set_default_source(source: String) -> Result<(), String> {
    let parsed = match source.as_str() {
        "cursor" => VibeSource::Cursor,
        "claude_code" | "claude" => VibeSource::ClaudeCode,
        "codex" => VibeSource::Codex,
        _ => return Err(format!("unknown source: {source}")),
    };
    state::write_default_source(parsed).map_err(|e| e.to_string())
}

#[tauri::command]
fn platform_defaults() -> serde_json::Value {
    serde_json::json!({
        "os": std::env::consts::OS,
        "float_visible_default": cfg!(target_os = "macos"),
        "presentation_default": match state::default_presentation() {
            HudPresentation::Float => "float",
            HudPresentation::MenuBar => "menubar",
        },
    })
}

#[derive(serde::Serialize)]
struct DisplaySettingsResponse {
    float_hud: bool,
    tray_status: bool,
    lan_companion: bool,
    watch_companion: bool,
}

#[tauri::command]
fn get_display_settings() -> DisplaySettingsResponse {
    let display = state::load_display();
    DisplaySettingsResponse {
        float_hud: display.float_hud,
        tray_status: display.tray_status,
        lan_companion: state::load_lan_companion_enabled(),
        watch_companion: state::load_watch_companion_enabled(),
    }
}

#[tauri::command]
fn set_display_float_hud(app: AppHandle, enabled: bool) -> Result<(), String> {
    state::write_float_hud(enabled).map_err(|e| e.to_string())?;
    apply_display_preferences(&app);
    Ok(())
}

#[tauri::command]
fn set_display_tray_status(app: AppHandle, enabled: bool) -> Result<(), String> {
    state::write_tray_status(enabled).map_err(|e| e.to_string())?;
    refresh_tray_ui(&app);
    Ok(())
}

#[tauri::command]
fn get_presentation() -> String {
    match state::load_presentation() {
        HudPresentation::Float => "float".to_string(),
        HudPresentation::MenuBar => "menubar".to_string(),
    }
}

#[tauri::command]
fn set_presentation(app: AppHandle, mode: String) -> Result<(), String> {
    let parsed = match mode.as_str() {
        "float" => HudPresentation::Float,
        "menubar" | "menu_bar" => HudPresentation::MenuBar,
        _ => return Err(format!("unknown presentation: {mode}")),
    };
    state::write_presentation(parsed).map_err(|e| e.to_string())?;
    apply_display_preferences(&app);
    Ok(())
}

fn show_wizard(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("wizard") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn icons_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("icons")
}

const TRAY_BRAND_ICON: &str = "tray.png";

fn tray_icon_brand() -> tauri::Result<tauri::image::Image<'static>> {
    tauri::image::Image::from_path(icons_dir().join(TRAY_BRAND_ICON)).map_err(Into::into)
}

fn tray_icon_fallback() -> tauri::Result<tauri::image::Image<'static>> {
    tray_icon_brand().or_else(|_| {
        tauri::image::Image::from_path(icons_dir().join("icon.png")).map_err(Into::into)
    })
}

// 联动 `refresh_tray_status` / `tray_status_title`：状态变化时切换图标
fn tray_icon_for_phase(phase: VibePhase) -> tauri::Result<tauri::image::Image<'static>> {
    let name = match phase {
        VibePhase::Active => "tray-active.png",
        VibePhase::WaitingUser => "tray-waiting.png",
        VibePhase::Idle => "tray-idle.png",
        VibePhase::Stopped => "tray-stopped.png",
        VibePhase::Unknown => "tray-unknown.png",
    };
    match tauri::image::Image::from_path(icons_dir().join(name)) {
        Ok(img) => Ok(img),
        Err(_) => tray_icon_brand(),
    }
}

/// 按 `DisplayPreferences` 应用浮窗可见性；托盘状态在 `refresh_tray_status` 中处理。
fn apply_display_preferences(app: &AppHandle) {
    let prefs = state::load_display();
    if let Some(w) = app.get_webview_window("main") {
        if prefs.float_hud {
            let _ = w.show();
        } else {
            let _ = w.hide();
        }
    }
    refresh_tray_ui(app);
}

#[cfg(target_os = "macos")]
fn apply_macos_app_policy(app: &AppHandle) {
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

#[cfg(not(target_os = "macos"))]
fn apply_macos_app_policy(_app: &AppHandle) {}

fn tray_status_tooltip(snap: &StatusSnapshot) -> String {
    let source = state::pick_display_source(snap, state::load_default_source());
    status_line(snap, source)
}

fn source_short_label(source: VibeSource) -> &'static str {
    match source {
        VibeSource::Cursor => "Cursor",
        VibeSource::ClaudeCode => "Claude",
        VibeSource::Codex => "Codex",
    }
}

// 菜单栏标题：进行中 / 等待你 时展示「源 · 状态」；其他状态留空以避免占位。
// Windows 平台 `set_title` 不被支持，状态主要靠图标切换体现，参考 `tray_icon_for_phase`。
fn tray_status_title(snap: &StatusSnapshot) -> Option<String> {
    tray_status_title_for(snap, state::load_default_source())
}

fn tray_status_title_for(snap: &StatusSnapshot, default: VibeSource) -> Option<String> {
    let source = state::pick_display_source(snap, default);
    let phase = current_phase(snap, source);
    match phase {
        VibePhase::Active | VibePhase::WaitingUser => Some(format!(
            "{} · {}",
            source_short_label(source),
            phase_label_cn(phase)
        )),
        _ => None,
    }
}

fn current_phase(snap: &StatusSnapshot, source: VibeSource) -> VibePhase {
    let session = snap.sessions.iter().find(|s| s.source == source);
    let health = snap.sources.get(&source);
    session
        .map(|s| s.phase)
        .or_else(|| health.map(|h| h.phase))
        .unwrap_or(VibePhase::Unknown)
}

fn refresh_tray_status(app: &AppHandle, snap: &StatusSnapshot) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let _ = tray.set_tooltip(Some(tray_status_tooltip(snap)));

    let prefs = state::load_display();
    if prefs.tray_status {
        let source = state::pick_display_source(snap, state::load_default_source());
        let phase = current_phase(snap, source);
        if let Ok(icon) = tray_icon_for_phase(phase) {
            let _ = tray.set_icon(Some(icon));
            let _ = tray.set_icon_as_template(true);
        }
        // `set_title` 在 Windows 不支持，调用同样安全（返回 Ok 但无效果）；macOS / Linux 上即时联动。
        let _ = tray.set_title(tray_status_title(snap).as_deref());
    } else {
        if let Ok(icon) = tray_icon_brand() {
            let _ = tray.set_icon(Some(icon));
            let _ = tray.set_icon_as_template(true);
        }
        let _ = tray.set_title(None::<&str>);
    }
}

fn phase_label_cn(phase: VibePhase) -> &'static str {
    match phase {
        VibePhase::Active => "进行中",
        VibePhase::Idle => "空闲",
        VibePhase::WaitingUser => "等待你",
        VibePhase::Stopped => "已结束",
        VibePhase::Unknown => "未知",
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect::<String>() + "…"
}

fn status_detail(snap: &StatusSnapshot, source: VibeSource) -> String {
    let health = snap.sources.get(&source);
    let session = snap.sessions.iter().find(|s| s.source == source);
    let hook_installed = health.map(|h| h.hook_installed).unwrap_or(false);
    let phase = session
        .map(|s| s.phase)
        .or_else(|| health.map(|h| h.phase))
        .unwrap_or(VibePhase::Unknown);

    if let Some(title) = session.and_then(|s| s.task_title.as_deref()) {
        return truncate(title, 36);
    }
    if let Some(cwd) = session.and_then(|s| s.cwd.as_deref()) {
        return truncate(cwd, 36);
    }
    if let Some(note) = health.and_then(|h| h.note.as_deref()) {
        return truncate(note, 36);
    }
    if hook_installed && phase == VibePhase::Unknown {
        return "等待活动（已配置 hook）".to_string();
    }
    if hook_installed {
        return "等待活动".to_string();
    }
    "未配置 hook".to_string()
}

fn status_line(snap: &StatusSnapshot, source: VibeSource) -> String {
    let health = snap.sources.get(&source);
    let session = snap.sessions.iter().find(|s| s.source == source);
    let phase = session
        .map(|s| s.phase)
        .or_else(|| health.map(|h| h.phase))
        .unwrap_or(VibePhase::Unknown);
    let detail = status_detail(snap, source);
    format!(
        "{} · {} · {}",
        source.label(),
        phase_label_cn(phase),
        detail
    )
}

fn build_tray_menu(app: &AppHandle, snap: &StatusSnapshot) -> tauri::Result<Menu<tauri::Wry>> {
    let display = state::load_display();
    let default_src = state::load_default_source();
    let display_src = state::pick_display_source(snap, default_src);
    let current_status = MenuItem::with_id(
        app,
        "current_status",
        format!("当前 · {}", status_line(snap, display_src)),
        false,
        None::<&str>,
    )?;
    let status_cursor = MenuItem::with_id(
        app,
        "status_cursor",
        status_line(snap, VibeSource::Cursor),
        false,
        None::<&str>,
    )?;
    let status_claude = MenuItem::with_id(
        app,
        "status_claude",
        status_line(snap, VibeSource::ClaudeCode),
        false,
        None::<&str>,
    )?;
    let status_codex = MenuItem::with_id(
        app,
        "status_codex",
        status_line(snap, VibeSource::Codex),
        false,
        None::<&str>,
    )?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let fix = MenuItem::with_id(app, "fix", "修复监听", true, None::<&str>)?;
    let doctor = MenuItem::with_id(app, "doctor", "诊断", true, None::<&str>)?;
    let lite_label = if snap.lite_mode {
        "关闭轻量模式"
    } else {
        "开启轻量模式"
    };
    let toggle_lite = MenuItem::with_id(app, "toggle_lite", lite_label, true, None::<&str>)?;
    let sep_default = PredefinedMenuItem::separator(app)?;
    let default_cursor =
        MenuItem::with_id(app, "default_cursor", "设为默认 · Cursor", true, None::<&str>)?;
    let default_claude = MenuItem::with_id(
        app,
        "default_claude_code",
        "设为默认 · Claude Code",
        true,
        None::<&str>,
    )?;
    let default_codex =
        MenuItem::with_id(app, "default_codex", "设为默认 · Codex", true, None::<&str>)?;
    let lan_enabled = state::load_lan_companion_enabled();
    let watch_enabled = state::load_watch_companion_enabled();
    let sep_display = PredefinedMenuItem::separator(app)?;
    let display_float = CheckMenuItem::with_id(
        app,
        "display_float",
        "浮窗展示",
        true,
        display.float_hud,
        None::<&str>,
    )?;
    let display_tray = CheckMenuItem::with_id(
        app,
        "display_tray",
        "菜单栏状态",
        true,
        display.tray_status,
        None::<&str>,
    )?;
    let display_lan = CheckMenuItem::with_id(
        app,
        "display_lan",
        "iPad 看板",
        true,
        lan_enabled,
        None::<&str>,
    )?;
    let lan_copy = MenuItem::with_id(app, "lan_copy", "复制看板链接", lan_enabled, None::<&str>)?;
    let lan_qr = MenuItem::with_id(app, "lan_qr", "显示配对二维码", lan_enabled, None::<&str>)?;
    let lan_rotate = MenuItem::with_id(
        app,
        "lan_rotate",
        "重新生成 token",
        lan_enabled,
        None::<&str>,
    )?;
    let display_watch = CheckMenuItem::with_id(
        app,
        "display_watch",
        "手表伴侣",
        true,
        watch_enabled,
        None::<&str>,
    )?;
    let watch_copy = MenuItem::with_id(
        app,
        "watch_copy",
        "复制手表配对",
        watch_enabled,
        None::<&str>,
    )?;
    let watch_qr = MenuItem::with_id(
        app,
        "watch_qr",
        "显示手表配对二维码",
        watch_enabled,
        None::<&str>,
    )?;
    let watch_rotate = MenuItem::with_id(
        app,
        "watch_rotate",
        "重新生成手表 token",
        watch_enabled,
        None::<&str>,
    )?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    Menu::with_items(
        app,
        &[
            &current_status,
            &status_cursor,
            &status_claude,
            &status_codex,
            &sep1,
            &fix,
            &doctor,
            &toggle_lite,
            &sep_default,
            &default_cursor,
            &default_claude,
            &default_codex,
            &sep_display,
            &display_float,
            &display_tray,
            &display_lan,
            &lan_copy,
            &lan_qr,
            &lan_rotate,
            &display_watch,
            &watch_copy,
            &watch_qr,
            &watch_rotate,
            &sep2,
            &quit,
        ],
    )
}

fn show_message(title: &str, body: &str) {
    rfd::MessageDialog::new()
        .set_title(title)
        .set_description(body)
        .show();
}

fn store_from_app(app: &AppHandle) -> Option<SessionStore> {
    let state = app.state::<AppState>();
    let rt = state.runtime.lock().ok()?;
    rt.server.as_ref().map(|s| s.store.clone())
}

fn refresh_tray_ui(app: &AppHandle) {
    let Some(store) = store_from_app(app) else {
        return;
    };
    let snap = tauri::async_runtime::block_on(store.snapshot());
    let Ok(menu) = build_tray_menu(app, &snap) else {
        return;
    };
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_menu(Some(menu));
        refresh_tray_status(app, &snap);
    }
}

fn handle_tray_action(app: &AppHandle, id: &str) {
    match id {
        "display_float" => {
            let enabled = !state::load_display().float_hud;
            let _ = state::write_float_hud(enabled);
            apply_display_preferences(app);
        }
        "display_tray" => {
            let enabled = !state::load_display().tray_status;
            let _ = state::write_tray_status(enabled);
            refresh_tray_ui(app);
        }
        "display_lan" => {
            set_lan_companion_enabled(app, !state::load_lan_companion_enabled());
        }
        "display_watch" => {
            set_watch_companion_enabled(app, !state::load_watch_companion_enabled());
        }
        "quit" => app.exit(0),
        "fix" => {
            let hints = hook_search_hints(app);
            let state = app.state::<AppState>();
            let src = hook_binary_src(app).or_else(|| state.hook_src.lock().unwrap().clone());
            let result = install_hooks(src.as_deref(), &hints);
            if result.ok {
                if let Some(store) = store_from_app(app) {
                    tauri::async_runtime::block_on(sync_hook_health_from_disk(&store));
                }
            }
            show_message(
                if result.ok { "修复监听" } else { "修复失败" },
                &result.messages.join("\n"),
            );
            refresh_tray_ui(app);
        }
        "doctor" => {
            let state = app.state::<AppState>();
            let src = hook_binary_src(app).or_else(|| state.hook_src.lock().unwrap().clone());
            let mut report = tauri::async_runtime::block_on(doctor(src.as_deref()));
            if let Some(store) = store_from_app(app) {
                let rt = state.runtime.lock().unwrap();
                if let Some(srv) = rt.server.as_ref() {
                    report.port = srv.port;
                    report.lite_mode =
                        tauri::async_runtime::block_on(store.get_lite_mode());
                }
            }
            let body = format!(
                "端口: {}\n轻量模式: {}\nvibe-hook: {}\nCursor hook: {}\nClaude hook: {}\nCodex hook: {}\n\n{}",
                report.port,
                if report.lite_mode { "开" } else { "关" },
                yes_no(report.hook_binary_installed),
                yes_no(report.cursor_hook),
                yes_no(report.claude_hook),
                yes_no(report.codex_hook),
                report.messages.join("\n")
            );
            show_message("诊断", &body);
        }
        "toggle_lite" => {
            let Some(store) = store_from_app(app) else {
                return;
            };
            let current = tauri::async_runtime::block_on(store.get_lite_mode());
            tauri::async_runtime::block_on(store.set_lite_mode(!current));
            refresh_tray_ui(app);
        }
        "default_cursor" => {
            let _ = state::write_default_source(VibeSource::Cursor);
            refresh_tray_ui(app);
        }
        "default_claude_code" => {
            let _ = state::write_default_source(VibeSource::ClaudeCode);
            refresh_tray_ui(app);
        }
        "default_codex" => {
            let _ = state::write_default_source(VibeSource::Codex);
            refresh_tray_ui(app);
        }
        "lan_copy" => copy_companion_url(app),
        "lan_qr" => show_companion_qr(app),
        "lan_rotate" => {
            if let Err(e) = state::rotate_lan_token() {
                show_message("iPad 看板", &e.to_string());
            } else {
                show_message("iPad 看板", "已重新生成 token，旧链接已失效。正在复制新链接…");
                copy_companion_url(app);
            }
            refresh_tray_ui(app);
        }
        "watch_copy" => copy_watch_pairing(app),
        "watch_qr" => show_watch_qr(app),
        "watch_rotate" => {
            if let Err(e) = state::rotate_watch_token() {
                show_message("手表伴侣", &e.to_string());
            } else {
                show_message(
                    "手表伴侣",
                    "已重新生成手表 token，旧配对已失效。正在复制新配对…",
                );
                copy_watch_pairing(app);
            }
            refresh_tray_ui(app);
        }
        _ => {}
    }
}

fn yes_no(v: bool) -> &'static str {
    if v {
        "是"
    } else {
        "否"
    }
}

fn spawn_tray_menu_sync(app: AppHandle) {
    let Some(store) = store_from_app(&app) else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        let mut rx = store.subscribe();
        let _ = rx.recv().await;
        loop {
            match rx.recv().await {
                Ok(_) => {
                    let app_clone = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        refresh_tray_ui(&app_clone);
                    });
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let empty = StatusSnapshot {
        daemon_ok: true,
        port: 0,
        lite_mode: state::load_lite_mode(),
        sources: Default::default(),
        sessions: vec![],
    };
    let menu = build_tray_menu(app, &empty)?;

    // 初始 phase 为 Unknown，使用对应的 `tray-unknown.png`；失败回退到品牌图与窗口图标。
    let icon = tray_icon_for_phase(VibePhase::Unknown)
        .or_else(|_| tray_icon_fallback())
        .or_else(|e| app.default_window_icon().cloned().ok_or(e))?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(true)
        .menu(&menu)
        .on_menu_event(|app, event| {
            handle_tray_action(app, event.id.as_ref());
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if state::load_display().float_hud {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
            }
        })
        .build(app)?;

    refresh_tray_ui(app);
    Ok(())
}

fn apply_frosted_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    // WebView 默认不透明白底；必须清掉才能透出系统磨砂
    use tauri::window::Color;
    let _ = window.set_background_color(Some(Color(0, 0, 0, 0)));
    let _ = window.set_shadow(false);

    #[cfg(target_os = "macos")]
    {
        use tauri::window::{Effect, EffectState, EffectsBuilder};
        // 单层 Popover 磨砂 + radius，避免重复 apply_vibrancy 导致过糊、直角露底
        let _ = window.set_effects(Some(
            EffectsBuilder::new()
                .effects(vec![Effect::Popover])
                .state(EffectState::Active)
                .radius(12.0)
                .build(),
        ));
    }
    #[cfg(target_os = "windows")]
    {
        use window_vibrancy::apply_acrylic;
        let _ = apply_acrylic(&window, Some((18, 18, 18, 80)));
    }

    #[cfg(target_os = "macos")]
    let _ = window.set_visible_on_all_workspaces(true);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();

    let mut builder = tauri::Builder::default().plugin(tauri_plugin_shell::init());

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ));
    }

    builder
        .setup(|app| {
            let hook_src = hook_binary_src(app.handle());
            let hook_hints = hook_search_hints(app.handle());
            let lite = vibe_core::state::load_lite_mode();
            let server =
                tauri::async_runtime::block_on(start(hook_src.clone(), hook_hints.clone(), lite))
                    .map_err(|e| format!("failed to start server: {e}"))?;

            let port = server.port;
            let action_store = server.action_store.clone();
            let executor_task = spawn_action_executor(app.handle().clone(), action_store);
            app.manage(AppState {
                runtime: Mutex::new(AppRuntime {
                    server: Some(server),
                    port,
                    executor_task: Some(executor_task),
                }),
                hook_src: Mutex::new(hook_src),
                hook_search_hints: Mutex::new(hook_hints),
            });

            apply_macos_app_policy(app.handle());
            setup_tray(app.handle())?;
            spawn_tray_menu_sync(app.handle().clone());
            apply_frosted_main_window(app.handle());

            if needs_first_run() {
                if let Some(main) = app.get_webview_window("main") {
                    let _ = main.hide();
                }
                show_wizard(app.handle());
            } else {
                apply_display_preferences(app.handle());
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_base_url,
            get_status,
            install_hooks_cmd,
            run_doctor,
            set_lite_mode,
            get_default_source,
            set_default_source,
            get_display_settings,
            set_display_float_hud,
            set_display_tray_status,
            get_presentation,
            set_presentation,
            finish_first_run,
            needs_first_run,
            open_path,
            platform_defaults,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;
    use vibe_core::types::Session;

    fn snap_with_session(source: VibeSource, phase: VibePhase) -> StatusSnapshot {
        StatusSnapshot {
            daemon_ok: true,
            port: 0,
            lite_mode: false,
            sources: HashMap::new(),
            sessions: vec![Session {
                source,
                session_id: "s1".into(),
                cwd: None,
                task_title: None,
                last_tool: None,
                last_activity_at: Utc::now(),
                phase,
            }],
        }
    }

    #[test]
    fn title_shows_source_and_phase_when_active() {
        let snap = snap_with_session(VibeSource::Cursor, VibePhase::Active);
        assert_eq!(
            tray_status_title_for(&snap, VibeSource::Cursor),
            Some("Cursor · 进行中".to_string())
        );
    }

    #[test]
    fn title_shows_when_waiting_user() {
        let snap = snap_with_session(VibeSource::ClaudeCode, VibePhase::WaitingUser);
        assert_eq!(
            tray_status_title_for(&snap, VibeSource::ClaudeCode),
            Some("Claude · 等待你".to_string())
        );
    }

    #[test]
    fn title_hidden_when_idle_or_stopped_or_unknown() {
        for phase in [VibePhase::Idle, VibePhase::Stopped, VibePhase::Unknown] {
            let snap = snap_with_session(VibeSource::Codex, phase);
            assert_eq!(
                tray_status_title_for(&snap, VibeSource::Codex),
                None,
                "phase {:?} should hide title",
                phase
            );
        }
    }

    #[test]
    fn title_uses_claude_short_label() {
        let snap = snap_with_session(VibeSource::ClaudeCode, VibePhase::Active);
        let title = tray_status_title_for(&snap, VibeSource::ClaudeCode).unwrap();
        assert!(title.starts_with("Claude "), "got: {title}");
        assert!(!title.contains("Claude Code"), "got: {title}");
    }

    #[test]
    fn icon_resolves_for_every_phase() {
        for phase in [
            VibePhase::Active,
            VibePhase::WaitingUser,
            VibePhase::Idle,
            VibePhase::Stopped,
            VibePhase::Unknown,
        ] {
            assert!(
                tray_icon_for_phase(phase).is_ok(),
                "icon for phase {:?} should load",
                phase
            );
        }
    }
}
