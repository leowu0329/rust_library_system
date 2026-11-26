use bson::oid::ObjectId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LoanStatus {
    Active,   // 借閱中
    Returned, // 已歸還
    Overdue,  // 已逾期（歸還時結算標記）
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Loan {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub user_id: ObjectId,
    pub copy_id: ObjectId,
    pub book_id: ObjectId,
    pub borrowed_at: DateTime<Utc>,
    pub due_date: DateTime<Utc>,
    pub returned_at: Option<DateTime<Utc>>,
    pub fine_amount: u32,
    pub status: LoanStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReservationStatus {
    Waiting,   // 排隊候補中
    Ready,     // 書籍已到館，通知取書中
    Fulfilled, // 已完成取書借閱
    Expired,   // 取書逾期自動取消
    Cancelled, // 讀者主動取消或因館藏全損取消
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reservation {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub user_id: ObjectId,
    pub book_id: ObjectId,
    pub assigned_copy_id: Option<ObjectId>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub status: ReservationStatus,
}