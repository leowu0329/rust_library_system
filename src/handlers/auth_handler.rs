use crate::middleware::rbac::CurrentUser;
use crate::models::user::{Role, ViewMode};
use axum::{
    response::{IntoResponse, Redirect},
    Form,
};
use serde::Deserialize;
use tower_sessions::Session;

#[derive(Deserialize)]
pub struct SwitchRoleForm {
    pub role: String,
}

/// POST /switch-mode：在「前台檢索」與「後台作業」之間切換
pub async fn switch_view_mode(user: CurrentUser, session: Session) -> impl IntoResponse {
    if !user.role.can_access_backend() {
        return Redirect::to("/admin/procurement");
    }

    let next_mode = match user.view_mode {
        ViewMode::Frontend => ViewMode::Backend,
        ViewMode::Backend => ViewMode::Frontend,
    };

    let _ = session.insert("view_mode", next_mode).await;
    Redirect::to("/admin/procurement")
}

/// POST /switch-role：快速切換測試角色（驗證 RBAC 權限控管）
pub async fn switch_role(session: Session, Form(form): Form<SwitchRoleForm>) -> impl IntoResponse {
    let (role, username) = match form.role.as_str() {
        "super_admin" => (Role::SuperAdmin, "王超管"),
        "librarian_admin" => (Role::LibrarianAdmin, "陳館長"),
        "librarian" => (Role::Librarian, "張館員"),
        "procurement" => (Role::Procurement, "林採購"),
        "member" => (Role::Member, "李讀者"),
        _ => (Role::Guest, "訪客"),
    };

    let default_mode = if role.can_access_backend() {
        ViewMode::Backend
    } else {
        ViewMode::Frontend
    };

    let _ = session.insert("role", role).await;
    let _ = session.insert("username", username).await;
    let _ = session.insert("view_mode", default_mode).await;

    Redirect::to("/admin/procurement")
}
