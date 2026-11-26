use bson::uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 館藏副本物理與流通狀態
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CopyState {
    Available,   // 在館可借
    Borrowed,    // 已借出
    Reserved,    // 保留中（已歸還並鎖定給第一順位預約者）
    Lost,        // 已遺失（保留歷史關聯，不物理刪除）
    Maintenance, // 損壞維修／待處理中
}

/// 讀者帳號狀態
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReaderStatus {
    Active,    // 正常
    Suspended, // 已停權
}

/// 讀者基本與借閱規則資料
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reader {
    pub id: Uuid,
    pub card_number: String,
    pub name: String,
    pub email: String,
    pub status: ReaderStatus,
    pub max_borrow_limit: usize,
    pub allow_borrow_with_overdue: bool, // 是否允許有逾期未還書籍時繼續借閱
}

/// 書目資料
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogBook {
    pub id: Uuid,
    pub isbn: String,
    pub title: String,
}

/// 實體館藏副本
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicalCopy {
    pub id: Uuid,
    pub book_id: Uuid,
    pub barcode: String,
    pub state: CopyState,
    pub note: Option<String>,
}

/// 借閱紀錄
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorrowRecord {
    pub id: Uuid,
    pub reader_id: Uuid,
    pub copy_id: Uuid,
    pub book_id: Uuid,
    pub borrowed_at: DateTime<Utc>,
    pub due_date: DateTime<Utc>,
    pub returned_at: Option<DateTime<Utc>>,
}

/// 預約狀態
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HoldStatus {
    Waiting,   // 排隊候補中
    Ready,     // 書籍已到館保留，等待取書
    Fulfilled, // 已取書完成借閱
    Cancelled, // 已取消
}

/// 預約候補紀錄
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoldRecord {
    pub id: Uuid,
    pub reader_id: Uuid,
    pub book_id: Uuid,
    pub assigned_copy_id: Option<Uuid>,
    pub queue_order: usize,
    pub status: HoldStatus,
    pub created_at: DateTime<Utc>,
    pub pickup_deadline: Option<DateTime<Utc>>,
}

/// 遺失或損壞處理紀錄（稽核留存）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IncidentType {
    Lost,
    Damaged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyIncidentLog {
    pub id: Uuid,
    pub copy_id: Uuid,
    pub reader_id: Option<Uuid>,
    pub incident_type: IncidentType,
    pub description: String,
    pub compensation_fee: u32,
    pub recorded_at: DateTime<Utc>,
}

/// Email 通知發送狀態
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NotificationStatus {
    Sent,
    PendingRetry,
    DeadLetter, // 超過最大重試次數
}

/// Email 通知與失敗重試紀錄
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailNotificationLog {
    pub id: Uuid,
    pub recipient_email: String,
    pub subject: String,
    pub body: String,
    pub status: NotificationStatus,
    pub retry_count: u32,
    pub max_retries: u32,
    pub last_error: Option<String>,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
