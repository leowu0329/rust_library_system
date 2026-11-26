pub mod error;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod services;

use axum::{
    response::Redirect,
    routing::{get, post},
    Router,
};
use handlers::{
    admin_ui_handler::admin_ui_routes,
    auth_handler::{switch_role, switch_view_mode},
};
use mongodb::Database;
use services::extensibility_service::ExtensibilityService;
use std::sync::Arc;
use tower_sessions::{MemoryStore, SessionManagerLayer};

/// 全域應用程式共享狀態（提供給 Axum 路由處理器與整合測試使用）
#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub extensibility: ExtensibilityService,
}

impl AppState {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            extensibility: ExtensibilityService::new(),
        }
    }
}

/// 建立 Axum 應用程式路由實例（供 main.rs 與整合測試呼叫）
pub fn create_app(db: Database) -> Router {
    let state = Arc::new(AppState::new(db));
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store).with_secure(false);

    Router::<Arc<AppState>>::new()
        .route("/", get(|| async { Redirect::to("/admin/dashboard") }))
        .route("/health", get(|| async { "OK" }))
        .route("/switch-mode", post(switch_view_mode))
        .route("/switch-role", post(switch_role))
        .with_state(state)
        .merge(admin_ui_routes())
        .layer(session_layer)
}