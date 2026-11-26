use bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcurementStatus {
    Pending,   // 待館長審核
    Approved,  // 已核准（待廠商到貨驗收）
    Received,  // 已驗收入庫（已產生實體條碼）
    Rejected,  // 已駁回
}

impl ProcurementStatus {
    pub fn label(&self) -> &'static str {
        match self {
            ProcurementStatus::Pending => "待審核",
            ProcurementStatus::Approved => "已核准採購",
            ProcurementStatus::Received => "已驗收入庫",
            ProcurementStatus::Rejected => "已駁回",
        }
    }

    pub fn badge_class(&self) -> &'static str {
        match self {
            ProcurementStatus::Pending => "bg-warning text-dark",
            ProcurementStatus::Approved => "bg-info text-dark",
            ProcurementStatus::Received => "bg-success",
            ProcurementStatus::Rejected => "bg-danger",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Procurement {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub isbn: String,
    pub title: String,
    pub author: String,
    pub publisher: String,
    pub quantity: i32,
    pub unit_price: f64,
    pub status: ProcurementStatus,
    pub requester_name: String,
    pub approver_name: Option<String>,
    pub generated_barcodes: Vec<String>, // 驗收後記錄產生的條碼清單
    pub created_at: bson::DateTime,
}

/// 前端新增採購單的表單資料
#[derive(Debug, Deserialize)]
pub struct CreateProcurementForm {
    pub isbn: String,
    pub title: String,
    pub author: String,
    pub publisher: String,
    pub quantity: i32,
    pub unit_price: f64,
}

/// 前端驗收入庫的表單資料
#[derive(Debug, Deserialize)]
pub struct ReceiveProcurementForm {
    pub location: String,
}
