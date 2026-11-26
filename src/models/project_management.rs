use std::collections::HashMap;

/// LMS 專案開發八大階段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DevelopmentPhase {
    /// Phase 1：需求分析與系統規劃
    Phase1RequirementAndPlanning = 1,
    /// Phase 2：基礎系統建置（登入、註冊、Email 驗證、RBAC）
    Phase2FoundationSetup = 2,
    /// Phase 3：圖書與讀者管理（書目 CRUD、館藏副本、作者出版社分類、讀者 CRUD、條碼）
    Phase3CatalogAndReader = 3,
    /// Phase 4：借閱核心功能（借閱、歸還、續借、借閱期限、借閱限制）
    Phase4CirculationCore = 4,
    /// Phase 5：進階業務功能（預約與候補、逾期及罰款、繳費、通知中心）
    Phase5AdvancedOperations = 5,
    /// Phase 6：館務管理（採購、館藏盤點、遺失及報廢、多分館）
    Phase6LibraryManagement = 6,
    /// Phase 7：Dashboard 與報表（借閱統計、館藏分析、讀者分析、Excel/PDF 匯出）
    Phase7DashboardAndReports = 7,
    /// Phase 8：測試與正式部署（單元、整合、權限、效能、安全性測試）
    Phase8TestingAndDeployment = 8,
}

impl DevelopmentPhase {
    pub fn all_phases() -> Vec<DevelopmentPhase> {
        vec![
            Self::Phase1RequirementAndPlanning,
            Self::Phase2FoundationSetup,
            Self::Phase3CatalogAndReader,
            Self::Phase4CirculationCore,
            Self::Phase5AdvancedOperations,
            Self::Phase6LibraryManagement,
            Self::Phase7DashboardAndReports,
            Self::Phase8TestingAndDeployment,
        ]
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Phase1RequirementAndPlanning => "Phase 1：需求分析與系統規劃",
            Self::Phase2FoundationSetup => "Phase 2：基礎系統建置",
            Self::Phase3CatalogAndReader => "Phase 3：圖書與讀者管理",
            Self::Phase4CirculationCore => "Phase 4：借閱核心功能",
            Self::Phase5AdvancedOperations => "Phase 5：進階業務功能",
            Self::Phase6LibraryManagement => "Phase 6：館務管理",
            Self::Phase7DashboardAndReports => "Phase 7：Dashboard 與報表",
            Self::Phase8TestingAndDeployment => "Phase 8：測試與正式部署",
        }
    }

    pub fn previous(&self) -> Option<DevelopmentPhase> {
        match self {
            Self::Phase1RequirementAndPlanning => None,
            Self::Phase2FoundationSetup => Some(Self::Phase1RequirementAndPlanning),
            Self::Phase3CatalogAndReader => Some(Self::Phase2FoundationSetup),
            Self::Phase4CirculationCore => Some(Self::Phase3CatalogAndReader),
            Self::Phase5AdvancedOperations => Some(Self::Phase4CirculationCore),
            Self::Phase6LibraryManagement => Some(Self::Phase5AdvancedOperations),
            Self::Phase7DashboardAndReports => Some(Self::Phase6LibraryManagement),
            Self::Phase8TestingAndDeployment => Some(Self::Phase7DashboardAndReports),
        }
    }
}

/// 階段與交付物執行狀態
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MilestoneStatus {
    NotStarted,
    InProgress,
    Verified,
    Blocked(String),
}

/// 單一交付項目（對應 Roadmap 中的每個子項目）
#[derive(Debug, Clone)]
pub struct DeliverableItem {
    pub code: String,
    pub name: String,
    pub category: String,
    pub status: MilestoneStatus,
    pub owner_role: String,
    pub artifact_ref: Option<String>,
}

/// 單一開發階段記錄
#[derive(Debug, Clone)]
pub struct PhaseRecord {
    pub phase: DevelopmentPhase,
    pub status: MilestoneStatus,
    pub deliverables: Vec<DeliverableItem>,
    pub gate_approved_by: Option<String>,
}

impl PhaseRecord {
    pub fn completion_rate(&self) -> f64 {
        if self.deliverables.is_empty() {
            return 100.0;
        }
        let done = self
            .deliverables
            .iter()
            .filter(|d| d.status == MilestoneStatus::Verified)
            .count();
        (done as f64 / self.deliverables.len() as f64) * 100.0
    }

    pub fn is_all_deliverables_verified(&self) -> bool {
        self.deliverables
            .iter()
            .all(|d| d.status == MilestoneStatus::Verified)
    }
}

/// 專案整體進度摘要報告
#[derive(Debug, Clone)]
pub struct ProjectProgressReport {
    pub project_name: String,
    pub overall_completion_percentage: f64,
    pub current_active_phase: Option<DevelopmentPhase>,
    pub ready_for_production: bool,
    pub phase_summaries: HashMap<DevelopmentPhase, (MilestoneStatus, f64)>,
}
