pub mod error;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod services;

use std::sync::Arc;

use axum::{response::Redirect, routing::get, Router};
use mongodb::Database;

use crate::handlers::admin_ui_handler::admin_ui_routes;

/// 全域應用程式共享狀態
#[derive(Clone)]
pub struct AppState {
    pub db: Database,
}

impl AppState {
    /// 建立新的應用程式共享狀態實例
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

/// 建立主應用程式路由（根路徑 `/` 自動導向後台 Dashboard）
pub fn create_app(db: Database) -> Router {
    let shared_state = Arc::new(AppState { db });

    Router::<Arc<AppState>>::new()
        .with_state(shared_state)
        .route("/", get(|| async { Redirect::temporary("/admin/dashboard") }))
        .merge(admin_ui_routes())
}

/// 建立獨立後台 UX/UI 路由實例（供 `tests/ux_ui_workflow_test.rs` 快速測試使用）
pub fn create_ux_ui_app() -> Router {
    Router::new()
        .route("/", get(|| async { Redirect::temporary("/admin/dashboard") }))
        .merge(admin_ui_routes())
}