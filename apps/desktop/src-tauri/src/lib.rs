use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};
use vibe_core::{
    install::{doctor, install_hooks, sync_hook_health_from_disk},
    paths::{self, first_run_marker},
    server::{init_tracing, start, RunningServer},
    state::{self, HudPresentation},
    store::SessionStore,
    types::{DoctorReport, InstallHooksResult, StatusSnapshot, VibePhase, VibeSource},
};

const TRAY_ID: &str = "vibe-tray";

struct AppRuntime {
    server: Option<RunningServer>,
    port: u16,
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

    let presentation = state::load_presentation();
    apply_presentation(&app, presentation);
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
    apply_presentation(&app, parsed);
    refresh_tray_ui(&app);
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

fn apply_presentation(app: &AppHandle, mode: HudPresentation) {
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    match mode {
        HudPresentation::Float => {
            let _ = w.show();
        }
        HudPresentation::MenuBar => {
            let _ = w.hide();
        }
    }
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

fn refresh_tray_status(app: &AppHandle, snap: &StatusSnapshot) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(tray_status_tooltip(snap)));
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
    let presentation = state::load_presentation();
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
    let sep_presentation = PredefinedMenuItem::separator(app)?;
    let presentation_float = CheckMenuItem::with_id(
        app,
        "presentation_float",
        "浮窗展示",
        true,
        presentation == HudPresentation::Float,
        None::<&str>,
    )?;
    let presentation_menubar = CheckMenuItem::with_id(
        app,
        "presentation_menubar",
        "菜单栏图标展示",
        true,
        presentation == HudPresentation::MenuBar,
        None::<&str>,
    )?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let show = MenuItem::with_id(app, "show", "显示浮窗", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "隐藏浮窗", true, None::<&str>)?;
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
            &sep_presentation,
            &presentation_float,
            &presentation_menubar,
            &sep2,
            &show,
            &hide,
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
        "presentation_float" => {
            let _ = state::write_presentation(HudPresentation::Float);
            apply_presentation(app, HudPresentation::Float);
            refresh_tray_ui(app);
        }
        "presentation_menubar" => {
            let _ = state::write_presentation(HudPresentation::MenuBar);
            apply_presentation(app, HudPresentation::MenuBar);
            refresh_tray_ui(app);
        }
        "show" => {
            let _ = state::write_presentation(HudPresentation::Float);
            apply_presentation(app, HudPresentation::Float);
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_focus();
            }
            refresh_tray_ui(app);
        }
        "hide" => {
            let _ = state::write_presentation(HudPresentation::MenuBar);
            apply_presentation(app, HudPresentation::MenuBar);
            refresh_tray_ui(app);
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

    let icon = tray_icon_fallback().or_else(|e| {
        app.default_window_icon()
            .cloned()
            .ok_or(e)
    })?;

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
                if state::load_presentation() == HudPresentation::Float {
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
            app.manage(AppState {
                runtime: Mutex::new(AppRuntime {
                    server: Some(server),
                    port,
                }),
                hook_src: Mutex::new(hook_src),
                hook_search_hints: Mutex::new(hook_hints),
            });

            apply_macos_app_policy(app.handle());
            setup_tray(app.handle())?;
            spawn_tray_menu_sync(app.handle().clone());
            apply_frosted_main_window(app.handle());

            let presentation = state::load_presentation();
            if needs_first_run() {
                if let Some(main) = app.get_webview_window("main") {
                    let _ = main.hide();
                }
                show_wizard(app.handle());
            } else {
                apply_presentation(app.handle(), presentation);
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
