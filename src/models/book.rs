use bson::{oid::ObjectId, DateTime};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CopyStatus {
    Available, // 在館可借
    Borrowed,  // 借出中
    Reserved,  // 保留中（已歸還並分配給第一順位預約者）
    Damaged,   // 損壞/維修中
    Lost,      // 遺失
}

/// 書目資料（邏輯概念，一本書對應一筆）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub isbn: String,
    pub title: String,
    pub author: String,
    pub publisher: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub total_copies: i32,
    pub available_copies: i32,
    #[serde(default = "DateTime::now")]
    pub created_at: DateTime,
    pub updated_at: DateTime,
}

/// 實際館藏副本（物理實體，每本實體書對應一筆與獨立條碼）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookCopy {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub book_id: ObjectId,
    pub isbn: String,
    pub barcode: String,
    pub location: String,
    pub status: CopyStatus,
    pub procurement_id: ObjectId,
    pub created_at: DateTime,
}