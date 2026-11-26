use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
            Role::LibrarianAdmin => "館長",
            Role::Librarian => "圖書館員",
            Role::Procurement => "採購人員",
            Role::Member => "一般讀者",
            Role::Guest => "訪客",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    Frontend,
    Backend,
}
