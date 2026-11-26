use bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ViewMode {
    Frontend,
    Backend,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Role {
    SuperAdmin,
    LibrarianAdmin,
    Librarian,
    Procurement,
    Member,
    Guest,
}

impl Role {
    pub fn can_access_backend(&self) -> bool {
        matches!(
            self,
            Role::SuperAdmin | Role::LibrarianAdmin | Role::Librarian | Role::Procurement
        )
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Role::SuperAdmin => "超級管理員",
            Role::LibrarianAdmin => "圖書館管理員",
            Role::Librarian => "館員",
            Role::Procurement => "採購人員",
            Role::Member => "一般讀者",
            Role::Guest => "訪客",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub card_number: String,
    pub name: String,
    pub role: Role,
    pub is_active: bool,
    pub max_borrow_limit: usize,
    pub max_reserve_limit: usize,
    pub unpaid_fines: u32,
}