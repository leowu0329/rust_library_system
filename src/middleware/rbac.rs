use crate::models::user::{Role, ViewMode};
use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
};
use tower_sessions::Session;

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub username: String,
    pub role: Role,
    pub view_mode: ViewMode,
}

#[async_trait]
impl<S> FromRequestParts<S> for CurrentUser
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())?;

        // 預設以採購人員登入方便直接測試，可透過右上角切換器體驗不同角色權限
        let username: String = session
            .get("username")
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| "林採購 (測試帳號)".to_string());
        let role: Role = session
            .get("role")
            .await
            .unwrap_or(None)
            .unwrap_or(Role::Procurement);
        let mut view_mode: ViewMode = session
            .get("view_mode")
            .await
            .unwrap_or(None)
            .unwrap_or(ViewMode::Backend);

        if !role.can_access_backend() {
            view_mode = ViewMode::Frontend;
        }

        Ok(CurrentUser {
            username,
            role,
            view_mode,
        })
    }
}

pub fn require_roles(user: &CurrentUser, allowed: &[Role]) -> Result<(), Response> {
    if allowed.contains(&user.role) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            format!("權限不足：目前角色「{}」無法執行此操作", user.role.display_name()),
        )
            .into_response())
    }
}
