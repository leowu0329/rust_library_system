use bson::uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ==========================================
// A. 館藏管理延伸模型
// ==========================================
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CopyCondition {
    Available,   // 可借閱
    Borrowed,    // 已借出
    Reserved,    // 預約保留中
    Lost,        // 遺失
    Damaged,     // 損壞待修
    Discarded,   // 已報廢
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookMetadata {
    pub id: Uuid,
    pub isbn: String,
    pub title: String,
    pub author: String,
    pub publisher: String,
    pub category: String,
    pub cover_url: Option<String>,
    pub digital_attachments: Vec<String>, // 電子附件連結 (PDF/試閱檔)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookCopyItem {
    pub id: Uuid,
    pub book_id: Uuid,
    pub barcode: String,          // 條碼編號
    pub branch_code: String,      // 館別代碼 (支援跨館)
    pub shelf_location: String,   // 書架位置 (例: A3-02-05)
    pub condition: CopyCondition,
}

// ==========================================
// B. 讀者管理模型
// ==========================================
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PatronCategory {
    Student,  // 學生
    Faculty,  // 教師
    Public,   // 一般民眾
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MembershipTier {
    Standard, // 一般會員
    Silver,   // 銀卡會員
    Gold,     // 金卡會員
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationRecord {
    pub id: Uuid,
    pub reason: String,
    pub points: u32,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatronProfile {
    pub id: Uuid,
    pub name: String,
    pub card_number: String,
    pub qr_code_payload: String,
    pub category: PatronCategory,
    pub tier: MembershipTier,
    pub is_suspended: bool,
    pub is_blacklisted: bool,
    pub violation_points: u32,
    pub violations: Vec<ViolationRecord>,
}

impl PatronProfile {
    /// 依據身分別與會員等級計算最大可借冊數與借期天數
    pub fn loan_policy(&self) -> (usize, i64) {
        let base = match self.category {
            PatronCategory::Faculty => (15, 30),
            PatronCategory::Student => (8, 21),
            PatronCategory::Public => (5, 14),
        };
        let bonus_books = match self.tier {
            MembershipTier::Standard => 0,
            MembershipTier::Silver => 2,
            MembershipTier::Gold => 5,
        };
        (base.0 + bonus_books, base.1)
    }
}

// ==========================================
// C. 借閱管理模型
// ==========================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoanRecord {
    pub id: Uuid,
    pub patron_id: Uuid,
    pub copy_id: Uuid,
    pub book_id: Uuid,
    pub origin_branch: String,
    pub pickup_branch: String, // 若與 origin_branch 不同即為跨館借閱
    pub borrowed_at: DateTime<Utc>,
    pub due_date: DateTime<Utc>,
    pub returned_at: Option<DateTime<Utc>>,
    pub renewal_count: u8,
}

// ==========================================
// D. 預約管理模型
// ==========================================
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReservationStatus {
    Waiting,   // 候補排隊中
    Ready,     // 已到館等待取書
    Fulfilled, // 已完成取書借出
    Cancelled, // 已取消
    Expired,   // 逾期未取自動取消
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReservationRecord {
    pub id: Uuid,
    pub patron_id: Uuid,
    pub book_id: Uuid,
    pub queue_position: usize,
    pub status: ReservationStatus,
    pub created_at: DateTime<Utc>,
    pub pickup_deadline: Option<DateTime<Utc>>,
    pub notified: bool,
}

// ==========================================
// E. 逾期與罰款管理模型
// ==========================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineRuleConfig {
    pub daily_rate: f64,          // 每日罰款金額
    pub max_fine_per_loan: f64,   // 單筆借閱罰款上限
    pub auto_suspend_points: u32, // 累積違規點數自動停權門檻
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FineStatus {
    Unpaid,   // 未繳費
    Paid,     // 已繳費
    Waived,   // 已減免
    Refunded, // 已退款
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTransaction {
    pub id: Uuid,
    pub patron_id: Uuid,
    pub loan_id: Uuid,
    pub overdue_days: i64,
    pub original_amount: f64,
    pub final_amount: f64,
    pub status: FineStatus,
    pub note: Option<String>,
}

// ==========================================
// F. 統計分析 Dashboard 模型
// ==========================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedMetric {
    pub label: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDashboardStats {
    pub total_copies: usize,               // 圖書總數 (實體副本總數)
    pub available_copies: usize,           // 可借閱數量
    pub borrowed_copies: usize,            // 借出數量
    pub today_borrows: usize,              // 今日借閱量
    pub today_returns: usize,              // 今日歸還量
    pub overdue_unreturned: usize,         // 逾期未還數量
    pub waiting_reservations: usize,       // 預約等待數量
    pub top_books: Vec<RankedMetric>,      // 熱門書籍排行榜
    pub top_categories: Vec<RankedMetric>, // 熱門分類分析
    pub top_patrons: Vec<RankedMetric>,    // 讀者借閱排行
    pub monthly_trends: Vec<(String, usize)>, // 每月借閱趨勢 (YYYY-MM, 借閱數)
    pub collection_utilization_rate: f64,  // 館藏使用率 (%)
}
