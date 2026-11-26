use axum::{
    extract::Request,
    http::{header, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::Response,
};

/// 驗證變更狀態請求（POST / PUT / DELETE）是否具備合法的 CSRF Token
pub async fn csrf_and_security_headers_middleware(
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let method = req.method().clone();
    if matches!(method, Method::POST | Method::PUT | Method::PATCH | Method::DELETE) {
        let csrf_header = req
            .headers()
            .get("X-CSRF-Token")
            .and_then(|v| v.to_str().ok());

        match csrf_header {
            Some(token) if token.len() >= 16 => {}
            _ => return Err(StatusCode::FORBIDDEN),
        }
    }

    let mut response = next.run(req).await;
    let headers = response.headers_mut();

    // XSS 與點擊劫持防護標頭
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::X_FRAME_OPTIONS,
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'self'; script-src 'self'; object-src 'none'"),
    );

    Ok(response)
}
