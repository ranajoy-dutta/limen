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
use tokio::sync::RwLock;

pub struct AppState {
    pub session_manager: Arc<SessionManager>,
    pub active_profile: Arc<RwLock<Option<ActiveProfile>>>,
    pub ignore_blur: Arc<AtomicBool>,
    pub last_blur_time: Arc<Mutex<Option<Instant>>>,
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
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

            let quit_item = tauri::menu::MenuItem::with_id(app, "quit", "Quit Limen", true, Some("CmdOrCtrl+Q"))?;
            let tray_menu = tauri::menu::Menu::with_items(app, &[&quit_item])?;
            let tray_menu_clone = tray_menu.clone();

            let _tray = TrayIconBuilder::new()
                .icon(tray_icon)
                .icon_as_template(true)
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
                            let _ = win.as_ref().window().move_window(Position::TrayCenter);
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
