use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 通用核心資料審計中繼資料（支援軟刪除，適用於主檔與可變資源）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditMetadata {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: String,
    pub updated_by: String,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl AuditMetadata {
    pub fn new(operator: impl Into<String>) -> Self {
        let now = Utc::now();
        let actor = operator.into();
        Self {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            created_by: actor.clone(),
            updated_by: actor,
            deleted_at: None,
        }
    }

    pub fn touch(&mut self, operator: impl Into<String>) {
        self.updated_at = Utc::now();
        self.updated_by = operator.into();
    }

    pub fn soft_delete(&mut self, operator: impl Into<String>) {
        let now = Utc::now();
        self.deleted_at = Some(now);
        self.updated_at = now;
        self.updated_by = operator.into();
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

/// 不可任意刪除之歷史與稽核中繼資料（無 deleted_at 欄位，適用於借閱、跨館調撥、繳費、通知與稽核紀錄）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImmutableAuditMetadata {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: String,
    pub updated_by: String,
}

impl ImmutableAuditMetadata {
    pub fn new(operator: impl Into<String>) -> Self {
        let now = Utc::now();
        let actor = operator.into();
        Self {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            created_by: actor.clone(),
            updated_by: actor,
        }
    }

    pub fn touch(&mut self, operator: impl Into<String>) {
        self.updated_at = Utc::now();
        self.updated_by = operator.into();
    }
}

// ============================================================================
// 1. 多分館管理 (Multi-Branch Management)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryBranch {
    #[serde(flatten)]
    pub audit: AuditMetadata,
    pub branch_code: String,
    pub name: String,
    pub address: String,
    pub is_active: bool,
    pub supports_self_checkout: bool,
}

// ============================================================================
// 2. 跨館借閱與調撥 (Inter-Branch Borrowing & Transfer)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransferStatus {
    Requested,
    InTransit,
    ArrivedAtPickupBranch,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterBranchTransfer {
    /// 跨館借還屬於不可刪除之歷史流轉紀錄
    #[serde(flatten)]
    pub audit: ImmutableAuditMetadata,
    pub copy_id: Uuid,
    pub reader_id: String,
    pub source_branch_id: Uuid,
    pub pickup_branch_id: Uuid,
    pub status: TransferStatus,
}

// ============================================================================
// 3 & 4. RFID 與 QR Code 實體館藏/讀者證擴充綁定 (RFID & QR Code)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopySmartTag {
    #[serde(flatten)]
    pub audit: AuditMetadata,
    pub copy_id: Uuid,
    pub book_title: String,
    pub category: String,
    pub branch_id: Uuid,
    /// RFID 晶片 EPC 編碼（供自助借還閘門與手持盤點機感應）
    pub rfid_epc: String,
    /// 館藏專屬 QR Code 查詢碼
    pub qr_code_token: String,
    /// 防盜磁條/EAS 安全位元狀態（true = 已上鎖不可攜出，false = 已合法借出消磁）
    pub eas_security_locked: bool,
    pub is_checked_out: bool,
    pub current_holder_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReaderDigitalCard {
    #[serde(flatten)]
    pub audit: AuditMetadata,
    pub reader_id: String,
    pub reader_name: String,
    pub qr_card_code: String,
    pub email: Option<String>,
    pub line_user_id: Option<String>,
    pub preferred_categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RfidInventoryReport {
    pub branch_id: Uuid,
    pub expected_on_shelf_count: usize,
    pub scanned_matched_count: usize,
    pub missing_rfid_epcs: Vec<String>,
    pub misplaced_rfid_epcs: Vec<String>,
}

// ============================================================================
// 5. 電子書與數位資源管理 (E-Book & Digital Resource Management)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EBookFormat {
    Epub,
    Pdf,
    Audiobook,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EBookResource {
    #[serde(flatten)]
    pub audit: AuditMetadata,
    pub isbn: String,
    pub title: String,
    pub author: String,
    pub category: String,
    pub format: EBookFormat,
    /// 同時線上授權閱讀上限（DRM 授權席次）
    pub total_licenses: u32,
    pub active_borrows: u32,
    pub download_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EBookLoanRecord {
    /// 電子書借閱歷史為不可刪除紀錄
    #[serde(flatten)]
    pub audit: ImmutableAuditMetadata,
    pub ebook_id: Uuid,
    pub reader_id: String,
    pub expires_at: DateTime<Utc>,
    pub returned_at: Option<DateTime<Utc>>,
}

// ============================================================================
// 6. ISBN API 自動取得書籍資訊 (ISBN Metadata Auto-Fetch)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsbnBookMetadata {
    pub isbn: String,
    pub title: String,
    pub authors: Vec<String>,
    pub publisher: String,
    pub published_year: u16,
    pub category: String,
    pub source_provider: String,
}

// ============================================================================
// 7 & 8. Email 與 LINE 雙管道即時通知 (Email & LINE Notifications)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotificationChannel {
    Email,
    Line,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotificationEventType {
    DueSoonReminder,
    OverdueAlert,
    ReservationReady,
    InterBranchArrived,
    SelfCheckoutReceipt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationDispatchLog {
    /// 通知發送紀錄屬於系統稽核的一部分，不可刪除
    #[serde(flatten)]
    pub audit: ImmutableAuditMetadata,
    pub reader_id: String,
    pub channel: NotificationChannel,
    pub recipient_address: String,
    pub event_type: NotificationEventType,
    pub message_body: String,
    pub delivered: bool,
}

// ============================================================================
// 9. 自助借還機交易紀錄 (Self-Checkout / Self-Return Kiosk)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KioskAction {
    SelfBorrow,
    SelfReturn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfCheckoutLog {
    /// 自助借還交易紀錄屬於核心帳務與流轉歷史，不可任意刪除
    #[serde(flatten)]
    pub audit: ImmutableAuditMetadata,
    pub branch_id: Uuid,
    pub reader_id: String,
    pub copy_id: Uuid,
    pub rfid_epc: String,
    pub action: KioskAction,
}

// ============================================================================
// 10. AI 智慧推薦引擎結果 (AI Smart Recommendation)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedBookItem {
    pub title: String,
    pub category: String,
    pub score: f32,
    pub reason: String,
}
