use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 系統角色定義（對應 RBAC 權限階層）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityRole {
    SuperAdmin,
    LibrarianAdmin,
    Librarian,
    Procurement,
    Member,
    Guest,
}

/// 經過 Supabase Auth JWT 驗證後的請求上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    pub user_id: Uuid,
    pub email: String,
    pub role: SecurityRole,
    pub session_token_hash: String,
}

/// 讀者個人資料（原始敏感資料，僅限最小權限存取）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberPersonalData {
    pub user_id: Uuid,
    pub full_name: String,
    pub national_id: String,
    pub phone: String,
    pub email: String,
    pub address: String,
    pub password_hash: String,
    pub consent_pdpa_at: DateTime<Utc>,
}

/// 依個資保護法（最小權限原則）去識別化後的讀者資料視圖
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskedMemberView {
    pub user_id: Uuid,
    pub masked_name: String,
    pub masked_national_id: String,
    pub masked_phone: String,
    pub masked_email: String,
}

/// 借閱紀錄（受 RLS 與所有權檢查保護）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureBorrowRecord {
    pub record_id: Uuid,
    pub member_id: Uuid,
    pub book_title: String,
    pub borrowed_at: DateTime<Utc>,
    pub due_at: DateTime<Utc>,
    pub returned_at: Option<DateTime<Utc>>,
}

/// 罰款與繳費紀錄（具備防竄改雜湊簽章，建立後不可任意修改）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImmutableFineRecord {
    pub fine_id: Uuid,
    pub member_id: Uuid,
    pub amount_twd: i64,
    pub reason: String,
    pub is_paid: bool,
    pub created_at: DateTime<Utc>,
    pub created_by: Uuid,
    /// 使用密鑰與欄位內容計算之 HMAC-SHA256 簽章，防止資料庫或記憶體被任意竄改
    pub integrity_signature: String,
}

/// 管理員與系統重要操作稽核日誌（Audit Logs）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAuditLog {
    pub log_id: Uuid,
    pub actor_id: Uuid,
    pub actor_role: SecurityRole,
    pub action: String,
    pub target_resource: String,
    pub detail: String,
    pub ip_or_source: String,
    pub prev_log_hash: String,
    pub current_log_hash: String,
    pub timestamp: DateTime<Utc>,
}

/// 資料庫備份與金鑰安全設定檢查結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityComplianceReport {
    pub rls_enabled_tables: Vec<String>,
    pub backup_schedule_cron: String,
    pub backup_encrypted: bool,
    pub service_role_key_exposed: bool,
}
