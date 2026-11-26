use bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub isbn: String,
    pub title: String,
    pub author: String,
    pub publisher: String,
    pub total_copies: i32,
    pub available_copies: i32,
    pub updated_at: bson::DateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CopyStatus {
    Available,
    Loaned,
    Reserved,
    Maintenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookCopy {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub book_id: ObjectId,
    pub isbn: String,
    pub barcode: String,        // 系統自動編碼，例如：LIB-978986-0001
    pub location: String,       // 預設架位，例如：新書展示區
    pub status: CopyStatus,
    pub procurement_id: ObjectId,
    pub created_at: bson::DateTime,
}
