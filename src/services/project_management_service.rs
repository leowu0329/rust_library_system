use crate::models::project_management::{
    DeliverableItem, DevelopmentPhase, MilestoneStatus, PhaseRecord, ProjectProgressReport,
};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectManagementError {
    PhaseNotFound,
    DeliverableNotFound(String),
    PrerequisitePhaseIncomplete {
        current: DevelopmentPhase,
        required: DevelopmentPhase,
    },
    UnverifiedDeliverablesRemaining {
        phase: DevelopmentPhase,
        remaining_codes: Vec<String>,
    },
    ProductionReleaseNotReady(String),
}

pub struct ProjectManagementService {
    pub project_name: String,
    phases: HashMap<DevelopmentPhase, PhaseRecord>,
    production_deployed: bool,
}

impl ProjectManagementService {
    /// 初始化標準 LMS 8 階段專案 Roadmap
    pub fn new_lms_roadmap(project_name: &str) -> Self {
        let mut phases = HashMap::new();

        // Phase 1：需求分析與系統規劃（包含基礎階段五大產出）
        phases.insert(
            DevelopmentPhase::Phase1RequirementAndPlanning,
            PhaseRecord {
                phase: DevelopmentPhase::Phase1RequirementAndPlanning,
                status: MilestoneStatus::InProgress,
                gate_approved_by: None,
                deliverables: vec![
                    Self::item("P1-1", "系統需求文件（SRS）", "基礎階段", "System Analyst"),
                    Self::item("P1-2", "使用者角色與權限矩陣", "基礎階段", "System Analyst"),
                    Self::item("P1-3", "系統架構圖", "基礎階段", "Solution Architect"),
                    Self::item("P1-4", "業務流程圖（BPMN）", "基礎階段", "Business Analyst"),
                    Self::item("P1-5", "ERD 資料庫設計", "基礎階段", "DBA / Architect"),
                ],
            },
        );

        // Phase 2：基礎系統建置
        phases.insert(
            DevelopmentPhase::Phase2FoundationSetup,
            PhaseRecord {
                phase: DevelopmentPhase::Phase2FoundationSetup,
                status: MilestoneStatus::NotStarted,
                gate_approved_by: None,
                deliverables: vec![
                    Self::item("P2-1", "登入、註冊、Email 驗證", "身份驗證", "Backend Dev"),
                    Self::item("P2-2", "RBAC 權限系統", "存取控制", "Security Engineer"),
                ],
            },
        );

        // Phase 3：圖書與讀者管理
        phases.insert(
            DevelopmentPhase::Phase3CatalogAndReader,
            PhaseRecord {
                phase: DevelopmentPhase::Phase3CatalogAndReader,
                status: MilestoneStatus::NotStarted,
                gate_approved_by: None,
                deliverables: vec![
                    Self::item("P3-1", "書目 CRUD", "館藏編目", "Backend Dev"),
                    Self::item("P3-2", "館藏副本管理", "館藏編目", "Backend Dev"),
                    Self::item("P3-3", "作者、出版社、分類維護", "基礎資料", "Backend Dev"),
                    Self::item("P3-4", "讀者 CRUD", "讀者服務", "Backend Dev"),
                    Self::item("P3-5", "條碼生成與掃描管理", "實體識別", "Backend Dev"),
                ],
            },
        );

        // Phase 4：借閱核心功能
        phases.insert(
            DevelopmentPhase::Phase4CirculationCore,
            PhaseRecord {
                phase: DevelopmentPhase::Phase4CirculationCore,
                status: MilestoneStatus::NotStarted,
                gate_approved_by: None,
                deliverables: vec![
                    Self::item("P4-1", "圖書借閱流程", "流通核心", "Core Dev"),
                    Self::item("P4-2", "圖書歸還流程", "流通核心", "Core Dev"),
                    Self::item("P4-3", "圖書續借規則", "流通核心", "Core Dev"),
                    Self::item("P4-4", "借閱期限計算引擎", "規則引擎", "Core Dev"),
                    Self::item("P4-5", "借閱冊數與身分限制", "規則引擎", "Core Dev"),
                ],
            },
        );

        // Phase 5：進階業務功能
        phases.insert(
            DevelopmentPhase::Phase5AdvancedOperations,
            PhaseRecord {
                phase: DevelopmentPhase::Phase5AdvancedOperations,
                status: MilestoneStatus::NotStarted,
                gate_approved_by: None,
                deliverables: vec![
                    Self::item("P5-1", "預約與候補排隊機制", "進階流通", "Backend Dev"),
                    Self::item("P5-2", "逾期判定及罰款計算", "帳務規則", "Backend Dev"),
                    Self::item("P5-3", "罰款繳費與收據核銷", "金流帳務", "Backend Dev"),
                    Self::item("P5-4", "通知中心（Email / 站內信）", "訊息服務", "Backend Dev"),
                ],
            },
        );

        // Phase 6：館務管理
        phases.insert(
            DevelopmentPhase::Phase6LibraryManagement,
            PhaseRecord {
                phase: DevelopmentPhase::Phase6LibraryManagement,
                status: MilestoneStatus::NotStarted,
                gate_approved_by: None,
                deliverables: vec![
                    Self::item("P6-1", "圖書採購與驗收", "館務行政", "Backend Dev"),
                    Self::item("P6-2", "館藏定期盤點", "館務行政", "Backend Dev"),
                    Self::item("P6-3", "遺失賠償及報廢註銷", "資產異動", "Backend Dev"),
                    Self::item("P6-4", "多分館館藏與跨館調撥", "多租戶/分館", "Architect"),
                ],
            },
        );

        // Phase 7：Dashboard 與報表
        phases.insert(
            DevelopmentPhase::Phase7DashboardAndReports,
            PhaseRecord {
                phase: DevelopmentPhase::Phase7DashboardAndReports,
                status: MilestoneStatus::NotStarted,
                gate_approved_by: None,
                deliverables: vec![
                    Self::item("P7-1", "借閱趨勢與熱門排行統計", "數據分析", "Data Engineer"),
                    Self::item("P7-2", "館藏周轉率與分類分析", "數據分析", "Data Engineer"),
                    Self::item("P7-3", "讀者活躍度與分群分析", "數據分析", "Data Engineer"),
                    Self::item("P7-4", "Excel / PDF 報表匯出引擎", "報表輸出", "Fullstack Dev"),
                ],
            },
        );

        // Phase 8：測試與正式部署
        phases.insert(
            DevelopmentPhase::Phase8TestingAndDeployment,
            PhaseRecord {
                phase: DevelopmentPhase::Phase8TestingAndDeployment,
                status: MilestoneStatus::NotStarted,
                gate_approved_by: None,
                deliverables: vec![
                    Self::item("P8-1", "單元測試（Unit Testing）", "品質保證", "QA / Dev"),
                    Self::item("P8-2", "整合測試（Integration Testing）", "品質保證", "QA Engineer"),
                    Self::item("P8-3", "權限測試（RBAC / Security Boundary）", "資安驗證", "Security QA"),
                    Self::item("P8-4", "效能測試（Load & Stress Testing）", "效能驗證", "Performance QA"),
                    Self::item("P8-5", "安全性測試（OWASP / Injection / Audit）", "資安驗證", "Security QA"),
                ],
            },
        );

        Self {
            project_name: project_name.to_string(),
            phases,
            production_deployed: false,
        }
    }

    fn item(code: &str, name: &str, category: &str, owner_role: &str) -> DeliverableItem {
        DeliverableItem {
            code: code.to_string(),
            name: name.to_string(),
            category: category.to_string(),
            status: MilestoneStatus::NotStarted,
            owner_role: owner_role.to_string(),
            artifact_ref: None,
        }
    }

    /// 啟動指定開發階段（強制檢查前一階段是否已通過 Gate 驗收）
    pub fn start_phase(&mut self, phase: DevelopmentPhase) -> Result<(), ProjectManagementError> {
        if let Some(prev) = phase.previous() {
            let prev_record = self
                .phases
                .get(&prev)
                .ok_or(ProjectManagementError::PhaseNotFound)?;
            if prev_record.status != MilestoneStatus::Verified {
                return Err(ProjectManagementError::PrerequisitePhaseIncomplete {
                    current: phase,
                    required: prev,
                });
            }
        }

        let record = self
            .phases
            .get_mut(&phase)
            .ok_or(ProjectManagementError::PhaseNotFound)?;
        if record.status == MilestoneStatus::NotStarted {
            record.status = MilestoneStatus::InProgress;
        }
        Ok(())
    }

    /// 驗證並完成單一交付項目
    pub fn verify_deliverable(
        &mut self,
        phase: DevelopmentPhase,
        deliverable_code: &str,
        artifact_ref: &str,
    ) -> Result<(), ProjectManagementError> {
        // 確保該階段已允許啟動
        self.start_phase(phase)?;

        let record = self
            .phases
            .get_mut(&phase)
            .ok_or(ProjectManagementError::PhaseNotFound)?;

        let item = record
            .deliverables
            .iter_mut()
            .find(|d| d.code == deliverable_code)
            .ok_or_else(|| ProjectManagementError::DeliverableNotFound(deliverable_code.to_string()))?;

        item.status = MilestoneStatus::Verified;
        item.artifact_ref = Some(artifact_ref.to_string());
        Ok(())
    }

    /// 批次完成指定階段的所有交付項目
    pub fn verify_all_deliverables_in_phase(
        &mut self,
        phase: DevelopmentPhase,
        artifact_prefix: &str,
    ) -> Result<(), ProjectManagementError> {
        self.start_phase(phase)?;
        let codes: Vec<String> = self
            .phases
            .get(&phase)
            .ok_or(ProjectManagementError::PhaseNotFound)?
            .deliverables
            .iter()
            .map(|d| d.code.clone())
            .collect();

        for code in codes {
            let artifact = format!("{}/{}", artifact_prefix, code.to_lowercase());
            self.verify_deliverable(phase, &code, &artifact)?;
        }
        Ok(())
    }

    /// 執行階段閘門簽核（Phase Gate Sign-Off）
    pub fn approve_phase_gate(
        &mut self,
        phase: DevelopmentPhase,
        approver: &str,
    ) -> Result<(), ProjectManagementError> {
        let record = self
            .phases
            .get_mut(&phase)
            .ok_or(ProjectManagementError::PhaseNotFound)?;

        if !record.is_all_deliverables_verified() {
            let remaining_codes = record
                .deliverables
                .iter()
                .filter(|d| d.status != MilestoneStatus::Verified)
                .map(|d| d.code.clone())
                .collect();
            return Err(ProjectManagementError::UnverifiedDeliverablesRemaining {
                phase,
                remaining_codes,
            });
        }

        record.status = MilestoneStatus::Verified;
        record.gate_approved_by = Some(approver.to_string());
        Ok(())
    }

    /// 執行最終正式環境部署（必須完成 Phase 1 ~ Phase 8 所有階段且通過五大測試）
    pub fn deploy_to_production(&mut self, release_manager: &str) -> Result<String, ProjectManagementError> {
        for phase in DevelopmentPhase::all_phases() {
            let record = self
                .phases
                .get(&phase)
                .ok_or(ProjectManagementError::PhaseNotFound)?;
            if record.status != MilestoneStatus::Verified {
                return Err(ProjectManagementError::ProductionReleaseNotReady(format!(
                    "{} 尚未通過 Phase Gate 驗收",
                    phase.title()
                )));
            }
        }

        self.production_deployed = true;
        Ok(format!(
            "專案 [{}] 已由 {} 簽核並成功部署至正式環境（Production）",
            self.project_name, release_manager
        ))
    }

    /// 取得即時專案進度報告
    pub fn generate_progress_report(&self) -> ProjectProgressReport {
        let mut total_items = 0usize;
        let mut verified_items = 0usize;
        let mut phase_summaries = HashMap::new();
        let mut current_active_phase = None;

        for phase in DevelopmentPhase::all_phases() {
            if let Some(record) = self.phases.get(&phase) {
                total_items += record.deliverables.len();
                verified_items += record
                    .deliverables
                    .iter()
                    .filter(|d| d.status == MilestoneStatus::Verified)
                    .count();

                if current_active_phase.is_none() && record.status != MilestoneStatus::Verified {
                    current_active_phase = Some(phase);
                }

                phase_summaries.insert(phase, (record.status.clone(), record.completion_rate()));
            }
        }

        let overall_completion_percentage = if total_items == 0 {
            100.0
        } else {
            (verified_items as f64 / total_items as f64) * 100.0
        };

        let ready_for_production = DevelopmentPhase::all_phases().iter().all(|p| {
            self.phases
                .get(p)
                .map(|r| r.status == MilestoneStatus::Verified)
                .unwrap_or(false)
        });

        ProjectProgressReport {
            project_name: self.project_name.clone(),
            overall_completion_percentage,
            current_active_phase,
            ready_for_production,
            phase_summaries,
        }
    }

    pub fn get_phase(&self, phase: DevelopmentPhase) -> Option<&PhaseRecord> {
        self.phases.get(&phase)
    }

    pub fn is_production_deployed(&self) -> bool {
        self.production_deployed
    }
}
