use chrono::{DateTime, Utc};
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

// ==========================================
// 1. 帳號、角色與 RBAC 權限模型
// ==========================================
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub role_id: ObjectId,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Profile {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub user_id: ObjectId,
    pub full_name: String,
    pub phone: Option<String>,
    pub address: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Role {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Permission {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub code: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RolePermission {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub role_id: ObjectId,
    pub permission_id: ObjectId,
}

// ==========================================
// 2. 書目、館藏副本與位置（原則 A：書籍與副本分離）
// ==========================================
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Author {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Publisher {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Category {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub code: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Book {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub isbn: String,
    pub title: String,
    pub publisher_id: ObjectId,
    pub category_id: ObjectId,
    pub author_ids: Vec<ObjectId>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Branch {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
    pub location: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Shelf {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub branch_id: ObjectId,
    pub code: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum CopyStatus {
    Available,
    Loaned,
    Reserved,
    Maintenance,
    Scrapped,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BookCopy {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub book_id: ObjectId,
    pub barcode: String,
    pub branch_id: ObjectId,
    pub shelf_id: ObjectId,
    pub status: CopyStatus,
}

// ==========================================
// 3. 讀者、借閱與預約（原則 B：借閱紀錄不可直接刪除）
// ==========================================
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum MemberStatus {
    Active,
    Suspended,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Member {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub user_id: ObjectId,
    pub card_number: String,
    pub status: MemberStatus,
    pub max_borrow_limit: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum LoanStatus {
    Active,
    Returned,
    Overdue,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Loan {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub member_id: ObjectId,
    pub copy_id: ObjectId,
    pub operator_id: ObjectId,
    pub borrowed_at: DateTime<Utc>,
    pub due_date: DateTime<Utc>,
    pub returned_at: Option<DateTime<Utc>>,
    pub status: LoanStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ReservationStatus {
    Pending,
    Fulfilled,
    Cancelled,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Reservation {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub member_id: ObjectId,
    pub book_id: ObjectId,
    pub status: ReservationStatus,
    pub created_at: DateTime<Utc>,
}

// ==========================================
// 4. 罰款與帳務流水（原則 C：所有金額都要保留交易紀錄）
// ==========================================
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum FineStatus {
    Unpaid,
    PartiallyPaid,
    Paid,
    Waived,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Fine {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub loan_id: ObjectId,
    pub member_id: ObjectId,
    pub amount: i64,
    pub status: FineStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum PaymentType {
    Payment,
    Waiver,
    Refund,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PaymentTransaction {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub fine_id: ObjectId,
    pub member_id: ObjectId,
    pub operator_id: ObjectId,
    pub tx_type: PaymentType,
    pub amount: i64,
    pub note: String,
    pub created_at: DateTime<Utc>,
}

// ==========================================
// 5. 稽核紀錄
// ==========================================
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuditLog {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub actor_id: ObjectId,
    pub action: String,
    pub target_collection: String,
    pub target_id: ObjectId,
    pub detail: String,
    pub created_at: DateTime<Utc>,
}