#![allow(unexpected_cfgs)]

pub mod commands;
pub mod core;
pub mod error;
pub mod models;

use crate::core::aws_client::{MockSsoOidcClient, SsoOidcClient};
use crate::core::session::SessionManager;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::menu::ContextMenu;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};
use tauri_plugin_positioner::{Position, WindowExt};

use crate::models::accounts::ActiveProfile;
use crate::models::auth::SessionState;
use tauri::Emitter;
use tokio::sync::RwLock;

pub struct AppState {
    pub session_manager: Arc<SessionManager>,
    pub active_profile: Arc<RwLock<Option<ActiveProfile>>>,
    pub ignore_blur: Arc<AtomicBool>,
    pub last_blur_time: Arc<Mutex<Option<Instant>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenStatus {
    Active,
    Refreshing,
    Expired,
}

pub fn render_tray_status_icon(base_icon: &tauri::image::Image<'_>, status: TokenStatus) -> tauri::image::Image<'static> {
    let width = base_icon.width();
    let height = base_icon.height();
    let mut rgba = base_icon.rgba().to_vec();

    let radius = (width as f32 * 0.16).max(3.0);
    let border_thickness = (width as f32 * 0.05).max(1.0);
    let center_x = width as f32 - radius - (width as f32 * 0.06);
    let center_y = height as f32 - radius - (height as f32 * 0.06);

    let (dot_r, dot_g, dot_b) = match status {
        TokenStatus::Active => (16, 185, 129),     // #10b981
        TokenStatus::Refreshing => (245, 158, 11), // #f59e0b
        TokenStatus::Expired => (244, 63, 94),     // #f43f5e
    };

    let total_radius = radius + border_thickness;

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= radius {
                let idx = ((y * width + x) * 4) as usize;
                if idx + 3 < rgba.len() {
                    rgba[idx] = dot_r;
                    rgba[idx + 1] = dot_g;
                    rgba[idx + 2] = dot_b;
                    rgba[idx + 3] = 255;
                }
            } else if dist <= total_radius {
                let idx = ((y * width + x) * 4) as usize;
                if idx + 3 < rgba.len() {
                    rgba[idx] = 22;
                    rgba[idx + 1] = 26;
                    rgba[idx + 2] = 36;
                    rgba[idx + 3] = 255;
                }
            }
        }
    }

    tauri::image::Image::new_owned(rgba, width, height)
}

pub fn update_tray_status(app_handle: &tauri::AppHandle, status: TokenStatus) {
    if let Some(tray) = app_handle.tray_by_id("main-tray") {
        if let Some(base_icon) = app_handle.default_window_icon() {
            let icon = render_tray_status_icon(base_icon, status);
            let _ = tray.set_icon(Some(icon));
        }
    }
}

async fn run_background_watchdog(app_handle: tauri::AppHandle) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval.tick().await;

    let mut last_emitted_state: Option<SessionState> = None;

    loop {
        interval.tick().await;

        let state = app_handle.state::<AppState>();
        let session_manager = &state.session_manager;

        // 1. Check and refresh SSO session
        match session_manager.check_and_refresh_session().await {
            Ok(new_state) => {
                let state_changed = match &last_emitted_state {
                    Some(prev) => prev != &new_state,
                    None => true,
                };
                if state_changed {
                    last_emitted_state = Some(new_state.clone());
                    let _ = app_handle.emit("limen://session-state-changed", &new_state);
                }
            }
            Err(e) => {
                tracing::warn!("Watchdog error during session check: {}", e);
            }
        }

        // 2. If session token is valid, check and refresh active profiles; otherwise clean up
        let current_tokens = session_manager.get_tokens().await;
        let is_token_valid = current_tokens
            .as_ref()
            .map(|t| t.expires_at > chrono::Utc::now())
            .unwrap_or(false);

        if is_token_valid {
            let tokens = current_tokens.unwrap();
            match crate::core::credentials::check_and_refresh_active_profiles(
                &tokens.access_token,
                &tokens.region,
            )
            .await
            {
                Ok(refreshed) => {
                    if refreshed {
                        let _ = app_handle.emit("limen://profiles-changed", ());
                    }
                }
                Err(e) => {
                    tracing::warn!("Watchdog error refreshing active profiles: {}", e);
                }
            }
        } else {
            match crate::core::credentials::clean_expired_active_profiles().await {
                Ok(cleaned) => {
                    if cleaned {
                        let _ = app_handle.emit("limen://profiles-changed", ());
                    }
                }
                Err(e) => {
                    tracing::warn!("Watchdog error cleaning expired active profiles: {}", e);
                }
            }
        }

        // 3. Update dynamic menu bar tray icon based on token validity
        let status = if is_token_valid {
            TokenStatus::Active
        } else {
            match last_emitted_state.as_ref() {
                Some(SessionState::Refreshing)
                | Some(SessionState::Registering)
                | Some(SessionState::AwaitingApproval { .. }) => TokenStatus::Refreshing,
                _ => TokenStatus::Expired,
            }
        };
        update_tray_status(&app_handle, status);
    }
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// Positions the tray window reliably across multiple monitors.
///
/// Ensures the window is placed on the exact monitor where the tray click occurred,
/// horizontally centered on the tray icon, and constrained within the visible boundaries of that monitor.
fn position_window_at_tray(
    win: &tauri::WebviewWindow,
    click_pos: tauri::PhysicalPosition<f64>,
    rect: tauri::Rect,
) {
    // 1. Determine the monitor where the tray icon / click is located.
    let target_monitor = win
        .available_monitors()
        .ok()
        .and_then(|monitors| {
            monitors.into_iter().find(|mon| {
                let pos = mon.position();
                let size = mon.size();
                let x = pos.x as f64;
                let y = pos.y as f64;
                let w = size.width as f64;
                let h = size.height as f64;
                click_pos.x >= x && click_pos.x < x + w && click_pos.y >= y && click_pos.y < y + h
            })
        })
        .or_else(|| {
            win.monitor_from_point(click_pos.x, click_pos.y)
                .ok()
                .flatten()
        })
        .or_else(|| win.current_monitor().ok().flatten())
        .or_else(|| win.primary_monitor().ok().flatten());

    if let Some(mon) = target_monitor {
        let mon_pos = mon.position();
        let mon_size = mon.size();
        let scale_factor = mon.scale_factor();

        let mon_log_x = mon_pos.x as f64 / scale_factor;
        let mon_log_y = mon_pos.y as f64 / scale_factor;
        let mon_log_w = mon_size.width as f64 / scale_factor;
        let mon_log_h = mon_size.height as f64 / scale_factor;

        let click_log_x = click_pos.x / scale_factor;

        let (tray_x, tray_y) = match rect.position {
            tauri::Position::Physical(p) => (p.x as f64 / scale_factor, p.y as f64 / scale_factor),
            tauri::Position::Logical(l) => (l.x, l.y),
        };

        let (tray_w, tray_h) = match rect.size {
            tauri::Size::Physical(s) => (
                s.width as f64 / scale_factor,
                s.height as f64 / scale_factor,
            ),
            tauri::Size::Logical(l) => (l.width, l.height),
        };

        // Determine logical window size (defined as 380x520 in config)
        let (win_w, win_h) = if let Ok(size) = win.outer_size() {
            let cur_scale = win
                .current_monitor()
                .ok()
                .flatten()
                .map(|m| m.scale_factor())
                .unwrap_or(scale_factor);
            (
                size.width as f64 / cur_scale,
                size.height as f64 / cur_scale,
            )
        } else {
            (380.0, 520.0)
        };

        let tray_center_x = if tray_w > 0.0 {
            tray_x + tray_w / 2.0
        } else {
            click_log_x
        };

        let mut target_x = tray_center_x - (win_w / 2.0);

        // Constrain horizontally within the target monitor with margin
        let margin = 8.0;
        let min_x = mon_log_x + margin;
        let max_x = mon_log_x + mon_log_w - win_w - margin;
        if target_x < min_x {
            target_x = min_x;
        } else if target_x > max_x {
            target_x = max_x;
        }

        // Position vertically directly below tray icon
        let tray_bottom_y = if tray_h > 0.0 {
            tray_y + tray_h
        } else {
            mon_log_y + 25.0
        };

        let gap = 4.0;
        let mut target_y = tray_bottom_y + gap;

        // Constrain vertically within target monitor bounds
        let min_y = mon_log_y + 4.0;
        let max_y = mon_log_y + mon_log_h - win_h - margin;
        if target_y < min_y {
            target_y = min_y;
        } else if target_y > max_y {
            target_y = max_y;
        }

        let _ = win.set_position(tauri::LogicalPosition::new(target_x, target_y));
    } else {
        let _ = win.as_ref().window().move_window(Position::TrayCenter);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let ignore_blur = Arc::new(AtomicBool::new(false));
    let last_blur_time = Arc::new(Mutex::new(None));
    let active_profile = Arc::new(RwLock::new(None));

    // Optional mock client if LIMEN_MOCK_AUTH=1, else real dynamic AWS SDK client
    let mock_client: Option<Arc<dyn SsoOidcClient>> =
        if std::env::var("LIMEN_MOCK_AUTH").unwrap_or_default() == "1" {
            tracing::info!("Running in LIMEN_MOCK_AUTH mode");
            Some(Arc::new(MockSsoOidcClient::new(3)))
        } else {
            None
        };

    let session_manager = Arc::new(SessionManager::new(mock_client, Arc::clone(&ignore_blur)));

    let app_state = AppState {
        session_manager,
        active_profile,
        ignore_blur: Arc::clone(&ignore_blur),
        last_blur_time: Arc::clone(&last_blur_time),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_positioner::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            crate::commands::auth::begin_login,
            crate::commands::auth::cancel_login,
            crate::commands::auth::get_session_state,
            crate::commands::auth::logout,
            crate::commands::accounts::get_accounts,
            crate::commands::accounts::refresh_accounts,
            crate::commands::accounts::activate_role,
            crate::commands::accounts::get_active_profiles,
            crate::commands::accounts::get_active_profile,
            crate::commands::accounts::deactivate_role,
            crate::commands::auth::get_last_session,
            quit_app,
        ])
        .setup(|app| {
            // Restore previous active session if tokens are still valid
            let session_manager = app.state::<AppState>().session_manager.clone();
            tauri::async_runtime::spawn(async move {
                let _ = session_manager.restore_last_session().await;
            });

            // Start proactive background watchdog for session and active profile credentials
            let app_handle_watchdog = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                run_background_watchdog(app_handle_watchdog).await;
            });

            // Configure macOS Accessory activation policy (no Dock icon, no App Menu)
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let window = app.get_webview_window("main").expect("main window missing");

            // Native macOS window & WKWebView transparency (eliminates black halo & rectangular outline)
            #[cfg(target_os = "macos")]
            {
                use objc::runtime::{Object, NO};
                use objc::{class, msg_send, sel, sel_impl};

                if let Ok(ptr) = window.ns_window() {
                    unsafe {
                        let ns_window = ptr as *mut Object;
                        let clear_color: *mut Object = msg_send![class!(NSColor), clearColor];
                        let _: () = msg_send![ns_window, setBackgroundColor: clear_color];
                        let _: () = msg_send![ns_window, setOpaque: NO];
                        let _: () = msg_send![ns_window, setHasShadow: NO];
                        let _: () = msg_send![ns_window, invalidateShadow];

                        // Set collection behavior so the window moves to the active space when summoned
                        // and can display over full-screen apps:
                        // 1 << 1: NSWindowCollectionBehaviorMoveToActiveSpace
                        // 1 << 8: NSWindowCollectionBehaviorFullScreenAuxiliary
                        // (NSWindowCollectionBehaviorCanJoinAllSpaces (1 << 0) and MoveToActiveSpace (1 << 1)
                        // are mutually exclusive per Apple documentation).
                        let collection_behavior: usize = (1 << 1) | (1 << 8);
                        let _: () = msg_send![ns_window, setCollectionBehavior: collection_behavior];
                    }
                }

                let _ = window.with_webview(|webview| {
                    unsafe {
                        let wk_webview = webview.inner() as *mut Object;
                        let clear_color: *mut Object = msg_send![class!(NSColor), clearColor];
                        let _: () = msg_send![wk_webview, setBackgroundColor: clear_color];

                        let key: *mut Object = msg_send![class!(NSString), stringWithUTF8String: c"drawsBackground".as_ptr()];
                        let no_num: *mut Object = msg_send![class!(NSNumber), numberWithBool: NO];
                        let _: () = msg_send![wk_webview, setValue:no_num forKey:key];
                    }
                });
            }

            // Build Tray Icon & Right-Click Context Menu
            let tray_icon = app.default_window_icon().cloned().expect("Default icon missing");
            let app_handle = app.handle().clone();

            let about_metadata = tauri::menu::AboutMetadata {
                name: Some("Limen".into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                authors: Some(vec!["Ranajoy Dutta".into()]),
                comments: Some("Native macOS menu bar AWS credential switcher".into()),
                ..Default::default()
            };
            let about_item = tauri::menu::PredefinedMenuItem::about(app, Some("About"), Some(about_metadata))?;
            let separator = tauri::menu::PredefinedMenuItem::separator(app)?;
            let quit_item = tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, Some("CmdOrCtrl+Q"))?;
            let tray_menu = tauri::menu::Menu::with_items(app, &[&about_item, &separator, &quit_item])?;
            let tray_menu_clone = tray_menu.clone();

            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(tray_icon)
                .icon_as_template(false)
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    if event.id.as_ref() == "quit" {
                        app.exit(0);
                    }
                })
                .on_tray_icon_event(move |_tray, event| {
                    tauri_plugin_positioner::on_tray_event(&app_handle, &event);

                    if let TrayIconEvent::Click {
                        button: MouseButton::Right,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(win) = app_handle.get_webview_window("main") {
                            let _ = win.hide();
                            let _ = tray_menu_clone.popup(win.as_ref().window().clone());
                        }
                        return;
                    }

                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        position,
                        rect,
                        ..
                    } = event
                    {
                        let state = app_handle.state::<AppState>();
                        let win = match app_handle.get_webview_window("main") {
                            Some(w) => w,
                            None => return,
                        };

                        // Check if window was blurred in the last 150ms while already visible
                        let just_blurred = {
                            let mut lock = state.last_blur_time.lock().unwrap();
                            if let Some(time) = *lock {
                                let elapsed = time.elapsed();
                                *lock = None;
                                elapsed < Duration::from_millis(150)
                            } else {
                                false
                            }
                        };

                        if just_blurred {
                            // The blur event already hid the window, do not toggle it back open
                            return;
                        }

                        let is_visible = win.is_visible().unwrap_or(false);
                        if is_visible {
                            let _ = win.hide();
                        } else {
                            position_window_at_tray(&win, position, rect);
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Window focus-loss auto-hide with ignore_blur suppression
            let window_clone = window.clone();
            let app_handle_for_events = app.handle().clone();

            window.on_window_event(move |event| {
                if let WindowEvent::Focused(false) = event {
                    let state = app_handle_for_events.state::<AppState>();

                    if state.ignore_blur.load(Ordering::SeqCst) {
                        return;
                    }

                    let is_vis = window_clone.is_visible().unwrap_or(false);
                    if is_vis {
                        *state.last_blur_time.lock().unwrap() = Some(Instant::now());
                        let _ = window_clone.hide();
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Limen application");
}
