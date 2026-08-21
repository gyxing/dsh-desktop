use std::sync::Arc;

use serde::Deserialize;
#[cfg(target_os = "macos")]
use tauri::menu::MenuBuilder;
use tauri::{
    menu::{ContextMenu, MenuItem, Submenu, SubmenuBuilder},
    App, AppHandle, LogicalPosition, Manager, Webview, Wry,
};

use super::{actions, tray::runtime_presentation};
use crate::{
    runtime::{manager::RuntimeManager, status::RuntimeStatus},
    updater::{manager::UpdateManager, presentation::update_presentation, status::UpdateStatus},
};

const TERMINAL_ID: &str = "menu-terminal";
const RESTART_ID: &str = "menu-restart";
const QUIT_ID: &str = "menu-quit";
const CHECK_UPDATE_ID: &str = "menu-check-update";
const VIEW_UPDATE_ID: &str = "menu-view-update";
const COPY_DIAGNOSTICS_ID: &str = "menu-copy-diagnostics";
const RELEASES_ID: &str = "menu-releases";
const ABOUT_ID: &str = "menu-about";
#[cfg(debug_assertions)]
const PREVIEW_UPDATE_ID: &str = "menu-preview-update";

pub struct DesktopMenu {
    app_menu: Submenu<Wry>,
    edit_menu: Submenu<Wry>,
    update_menu: Submenu<Wry>,
    help_menu: Submenu<Wry>,
    runtime_status_item: MenuItem<Wry>,
    restart_item: MenuItem<Wry>,
    update_status_item: MenuItem<Wry>,
    update_action_item: MenuItem<Wry>,
    view_update_item: MenuItem<Wry>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChromeMenuKind {
    Application,
    Edit,
    Update,
    Help,
}

pub fn setup(app: &mut App) -> tauri::Result<()> {
    let runtime = app.state::<Arc<RuntimeManager>>().status();
    let runtime_view = runtime_presentation(&runtime);
    let update = app.state::<Arc<UpdateManager>>().status();
    let update_view = update_presentation(&update);

    let runtime_status_item = MenuItem::with_id(
        app,
        "menu-runtime-status",
        runtime_view.status_label,
        false,
        None::<&str>,
    )?;
    let terminal_item = MenuItem::with_id(app, TERMINAL_ID, "打开 DSH 终端", true, None::<&str>)?;
    let restart_item = MenuItem::with_id(
        app,
        RESTART_ID,
        "重新启动 DeepSeek Harness",
        runtime_view.restart_enabled,
        None::<&str>,
    )?;
    let quit_item = MenuItem::with_id(app, QUIT_ID, "退出 DSH Desktop", true, None::<&str>)?;
    let app_menu = SubmenuBuilder::new(app, "DSH Desktop")
        .item(&runtime_status_item)
        .separator()
        .item(&terminal_item)
        .item(&restart_item)
        .separator()
        .item(&quit_item)
        .build()?;

    #[cfg(target_os = "macos")]
    let edit_builder = SubmenuBuilder::new(app, "编辑").undo().redo().separator();
    #[cfg(not(target_os = "macos"))]
    let edit_builder = SubmenuBuilder::new(app, "编辑");
    let edit_menu = edit_builder
        .cut_with_text("剪切")
        .copy_with_text("复制")
        .paste_with_text("粘贴")
        .separator()
        .select_all_with_text("全选")
        .build()?;

    let update_status_item = MenuItem::with_id(
        app,
        "menu-update-status",
        update_view.tray_status_label.clone(),
        false,
        None::<&str>,
    )?;
    let update_action_item = MenuItem::with_id(
        app,
        CHECK_UPDATE_ID,
        update_view.action_label.clone(),
        update_view.action_enabled,
        None::<&str>,
    )?;
    let view_update_item = MenuItem::with_id(
        app,
        VIEW_UPDATE_ID,
        "查看更新内容",
        matches!(update, UpdateStatus::Available { .. }),
        None::<&str>,
    )?;
    let mut update_builder = SubmenuBuilder::new(app, "更新")
        .item(&update_status_item)
        .separator()
        .item(&update_action_item)
        .item(&view_update_item);
    #[cfg(debug_assertions)]
    {
        let preview_item = MenuItem::with_id(
            app,
            PREVIEW_UPDATE_ID,
            "预览更新窗口（开发）",
            true,
            None::<&str>,
        )?;
        update_builder = update_builder.separator().item(&preview_item);
    }
    let update_menu = update_builder.build()?;

    let copy_diagnostics_item =
        MenuItem::with_id(app, COPY_DIAGNOSTICS_ID, "复制诊断信息", true, None::<&str>)?;
    let releases_item = MenuItem::with_id(app, RELEASES_ID, "打开发布页面", true, None::<&str>)?;
    let about_item = MenuItem::with_id(app, ABOUT_ID, "关于 DSH Desktop", true, None::<&str>)?;
    let help_menu = SubmenuBuilder::new(app, "帮助")
        .item(&copy_diagnostics_item)
        .item(&releases_item)
        .separator()
        .item(&about_item)
        .build()?;

    #[cfg(target_os = "macos")]
    {
        let menu = MenuBuilder::new(app)
            .item(&app_menu)
            .item(&edit_menu)
            .item(&update_menu)
            .item(&help_menu)
            .build()?;
        app.set_menu(menu)?;
    }

    app.manage(DesktopMenu {
        app_menu,
        edit_menu,
        update_menu,
        help_menu,
        runtime_status_item,
        restart_item,
        update_status_item,
        update_action_item,
        view_update_item,
    });
    Ok(())
}

#[tauri::command]
pub fn show_chrome_menu(
    webview: Webview,
    menu: ChromeMenuKind,
    x: f64,
    y: f64,
) -> Result<(), String> {
    if !crate::desktop::chrome::is_trusted_chrome_label(webview.label()) {
        return Err("只有本地标题栏可以打开应用菜单".to_string());
    }
    if !x.is_finite() || !y.is_finite() || x < 0.0 || !(0.0..=72.0).contains(&y) {
        return Err("菜单弹出位置无效".to_string());
    }
    let app = webview.app_handle();
    let state = app.state::<DesktopMenu>();
    let main = app
        .get_webview("main")
        .ok_or_else(|| "主内容窗口不存在".to_string())?;
    main.set_focus().map_err(|error| error.to_string())?;
    let window = main.window();
    let position = LogicalPosition::new(x, y);
    match menu {
        ChromeMenuKind::Application => state.app_menu.popup_at(window, position),
        ChromeMenuKind::Edit => state.edit_menu.popup_at(window, position),
        ChromeMenuKind::Update => state.update_menu.popup_at(window, position),
        ChromeMenuKind::Help => state.help_menu.popup_at(window, position),
    }
    .map_err(|error| error.to_string())
}

pub fn handle_event(app: &AppHandle, id: &str) {
    match id {
        TERMINAL_ID => actions::open_terminal(app),
        RESTART_ID => actions::restart_runtime(app),
        CHECK_UPDATE_ID => actions::check_update(app),
        VIEW_UPDATE_ID => actions::show_update_notes(app),
        COPY_DIAGNOSTICS_ID => actions::copy_diagnostics(app),
        RELEASES_ID => actions::open_releases(app),
        ABOUT_ID => crate::desktop::about::show(app),
        QUIT_ID => actions::quit(app),
        #[cfg(debug_assertions)]
        PREVIEW_UPDATE_ID => crate::updater::dialog::show_preview(app),
        _ => {}
    }
}

pub fn update_runtime_status(app: &AppHandle, status: &RuntimeStatus) {
    let Some(menu) = app.try_state::<DesktopMenu>() else {
        return;
    };
    let presentation = runtime_presentation(status);
    let _ = menu.runtime_status_item.set_text(presentation.status_label);
    let _ = menu.restart_item.set_enabled(presentation.restart_enabled);
}

pub fn update_updater_status(app: &AppHandle) {
    let Some(menu) = app.try_state::<DesktopMenu>() else {
        return;
    };
    let status = app.state::<Arc<UpdateManager>>().status();
    let presentation = update_presentation(&status);
    let _ = menu
        .update_status_item
        .set_text(presentation.tray_status_label);
    let _ = menu.update_action_item.set_text(presentation.action_label);
    let _ = menu
        .update_action_item
        .set_enabled(presentation.action_enabled);
    let _ = menu
        .view_update_item
        .set_enabled(matches!(status, UpdateStatus::Available { .. }));
}
