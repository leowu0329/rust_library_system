use chrono::{Duration, Utc};
use rust_library_system::models::security::{
    AuthContext, MemberPersonalData, SecureBorrowRecord, SecurityRole,
};
use rust_library_system::services::security_service::{SecurityError, SecurityService};
use std::collections::HashMap;
use uuid::Uuid;

fn mock_auth(user_id: Uuid, email: &str, role: SecurityRole) -> AuthContext {
    AuthContext {
        user_id,
        email: email.to_string(),
        role,
        session_token_hash: "supabase_jwt_verified_hash_token".to_string(),
    }
}

#[test]
fn test_security_and_data_protection_full_flow() {
    // 初始化安全服務（模擬從 .env 讀取伺服器端安全密鑰）
    let security_svc = SecurityService::new("SuperSecretLibraryPepperKey_2026!@#")
        .expect("應該成功初始化安全服務");

    let reader_a_id = Uuid::new_v4();
    let reader_b_id = Uuid::new_v4();
    let librarian_id = Uuid::new_v4();
    let admin_id = Uuid::new_v4();
    let guest_id = Uuid::new_v4();

    let reader_a_auth = mock_auth(reader_a_id, "reader_a@example.com", SecurityRole::Member);
    let reader_b_auth = mock_auth(reader_b_id, "reader_b@example.com", SecurityRole::Member);
    let librarian_auth = mock_auth(librarian_id, "staff@library.gov.tw", SecurityRole::Librarian);
    let admin_auth = mock_auth(admin_id, "admin@library.gov.tw", SecurityRole::LibrarianAdmin);
    let guest_auth = mock_auth(guest_id, "guest@example.com", SecurityRole::Guest);

    // =========================================================================
    // 測試 1：密碼安全雜湊機制
    // =========================================================================
    let salt = "random_user_salt_01";
    let hashed_pw = security_svc.hash_password("MyStrongP@ssw0rd!", salt);
    assert_ne!(hashed_pw, "MyStrongP@ssw0rd!", "密碼絕不可明文儲存");
    assert!(security_svc.verify_password("MyStrongP@ssw0rd!", salt, &hashed_pw));
    assert!(!security_svc.verify_password("WrongPassword", salt, &hashed_pw));

    // =========================================================================
    // 測試 2：【特別注意 1】讀者只能查詢自己的借閱紀錄
    // =========================================================================
    security_svc.seed_borrow_record(SecureBorrowRecord {
        record_id: Uuid::new_v4(),
        member_id: reader_a_id,
        book_title: "Rust 系統程式設計實戰".to_string(),
        borrowed_at: Utc::now() - Duration::days(3),
        due_at: Utc::now() + Duration::days(11),
        returned_at: None,
    });

    // 讀者 A 查詢自己的借閱紀錄 -> 成功
    let own_records = security_svc
        .query_borrow_records(&reader_a_auth, reader_a_id)
        .expect("讀者應能查詢自己的借閱紀錄");
    assert_eq!(own_records.len(), 1);
    assert_eq!(own_records[0].book_title, "Rust 系統程式設計實戰");

    // 讀者 B 嘗試查詢讀者 A 的借閱紀錄 -> 必須被阻擋 (OwnershipViolation)
    let cross_query_res = security_svc.query_borrow_records(&reader_b_auth, reader_a_id);
    assert_eq!(
        cross_query_res.unwrap_err(),
        SecurityError::OwnershipViolation,
        "讀者絕不能查詢他人的借閱紀錄"
    );

    // =========================================================================
    // 測試 3：【特別注意 2】館員只能執行被授權的操作 (RBAC)
    // =========================================================================
    // 館員執行流通借還書 -> 成功
    let circulation_res =
        security_svc.execute_librarian_circulation(&librarian_auth, "CHECKOUT_BOOK", "BK-2026-001");
    assert!(circulation_res.is_ok());

    // 訪客嘗試執行館員借還書操作 -> 拒絕
    let guest_res =
        security_svc.execute_librarian_circulation(&guest_auth, "CHECKOUT_BOOK", "BK-2026-001");
    assert_eq!(
        guest_res.unwrap_err(),
        SecurityError::ForbiddenRole(SecurityRole::Guest)
    );

    // 一般館員嘗試修改系統最高權限政策 -> 拒絕（超越授權範圍）
    let librarian_admin_attempt = security_svc.admin_update_system_policy(
        &librarian_auth,
        "MAX_BORROW_LIMIT",
        "999",
    );
    assert_eq!(
        librarian_admin_attempt.unwrap_err(),
        SecurityError::ForbiddenRole(SecurityRole::Librarian)
    );

    // =========================================================================
    // 測試 4：【特別注意 3】罰款及繳費紀錄不可任意修改
    // =========================================================================
    let mut fine = security_svc
        .issue_fine_record(&librarian_auth, reader_a_id, 150, "逾期歸還 3 天")
        .expect("館員應可依規定開立逾期罰款");

    // 驗證剛開立的罰款數位簽章完整
    assert!(security_svc.verify_fine_integrity(&fine).is_ok());

    // 嘗試透過系統介面直接修改罰款金額 -> 必須遭阻擋
    let modify_res =
        security_svc.attempt_arbitrary_fine_modification(&librarian_auth, fine.fine_id, 0);
    assert_eq!(
        modify_res.unwrap_err(),
        SecurityError::ImmutableRecordTampered
    );

    // 模擬惡意直接竄改記憶體或資料庫中的罰款金額 -> 簽章驗證必須失敗
    fine.amount_twd = 0;
    assert_eq!(
        security_svc.verify_fine_integrity(&fine).unwrap_err(),
        SecurityError::ImmutableRecordTampered,
        "任何未經授權的罰款金額竄改都必須被簽章機制偵測"
    );

    // =========================================================================
    // 測試 5：【特別注意 4】管理員的重要操作必須留下紀錄 (Audit Logs)
    // =========================================================================
    security_svc
        .admin_update_system_policy(&admin_auth, "OVERDUE_DAILY_RATE_TWD", "50")
        .expect("館員管理員應可更新系統政策");

    let logs = security_svc
        .get_audit_logs(&admin_auth)
        .expect("管理員應可檢視稽核日誌");
    assert!(
        logs.iter().any(|log| log.action == "ADMIN_UPDATE_SYSTEM_POLICY"
            && log.actor_id == admin_id),
        "管理員的重要政策變更操作必須留下稽核紀錄"
    );

    // =========================================================================
    // 測試 6：【特別注意 5】讀者個資應依適用的個資保護規範管理（最小權限遮罩）
    // =========================================================================
    let raw_pii = MemberPersonalData {
        user_id: reader_a_id,
        full_name: "吳俊男".to_string(),
        national_id: "D123456789".to_string(),
        phone: "0912345678".to_string(),
        email: "leowu0329@example.com".to_string(),
        address: "臺南市東區崇德路100號".to_string(),
        password_hash: hashed_pw,
        consent_pdpa_at: Utc::now(),
    };

    let masked_view = security_svc
        .get_member_profile_with_min_privilege(&librarian_auth, &raw_pii)
        .expect("館員依最小權限原則查詢讀者資料時應取得遮罩視圖");
    assert_eq!(masked_view.masked_name, "吳*男");
    assert_eq!(masked_view.masked_national_id, "D1*****789");
    assert_eq!(masked_view.masked_phone, "0912-***-678");
    assert_eq!(masked_view.masked_email, "le***@example.com");

    // =========================================================================
    // 測試 7：防 SQL Injection / XSS 攻擊、PostgreSQL RLS 策略與金鑰安全檢查
    // =========================================================================
    let sqli_attempt = security_svc.sanitize_and_validate_input("101'; DROP TABLE borrow_records;--");
    assert!(sqli_attempt.is_err(), "必須攔截 SQL Injection 攻擊字串");

    let xss_sanitized = security_svc
        .sanitize_and_validate_input("<script>alert('xss')</script>")
        .expect("XSS 標籤應被安全轉義");
    assert!(!xss_sanitized.contains("<script>"));
    assert!(xss_sanitized.contains("&lt;script&gt;"));

    // 驗證 PostgreSQL RLS 策略包含關鍵保護條款
    let rls_sql = security_svc.generate_postgresql_rls_policies();
    assert!(rls_sql.contains("ENABLE ROW LEVEL SECURITY"));
    assert!(rls_sql.contains("auth.uid() = member_id"));

    // 驗證金鑰安全：若前端公開環境變數包含 SERVICE_ROLE_KEY 必須報錯
    let mut bad_public_env = HashMap::new();
    bad_public_env.insert(
        "PUBLIC_SUPABASE_SERVICE_ROLE_KEY".to_string(),
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.service_role".to_string(),
    );
    assert!(security_svc.validate_public_env_safety(&bad_public_env).is_err());

    // 驗證合法的公開環境變數（僅含 ANON_KEY）通過合規與定期自動備份檢查
    let mut safe_public_env = HashMap::new();
    safe_public_env.insert(
        "PUBLIC_SUPABASE_URL".to_string(),
        "https://xyzproject.supabase.co".to_string(),
    );
    safe_public_env.insert(
        "PUBLIC_SUPABASE_ANON_KEY".to_string(),
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.anon".to_string(),
    );
    let compliance = security_svc
        .validate_public_env_safety(&safe_public_env)
        .expect("合法環境設定應通過安全合規檢查");
    assert!(compliance.backup_encrypted);
    assert!(!compliance.service_role_key_exposed);
}
