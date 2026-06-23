//! App-server request and notification dispatch for `ChatWidget`.
//!
//! This module translates protocol requests into the focused chat-widget flows
//! that render approvals, permissions, tool input, and guardian reviews.

use super::*;

impl ChatWidget {
    pub(crate) fn handle_server_request(
        &mut self,
        request: ServerRequest,
        replay_kind: Option<ReplayKind>,
    ) {
        let id = request.id().to_string();
        match request {
            ServerRequest::CommandExecutionRequestApproval { params, .. } => {
                let fallback_cwd = self.config.cwd.clone();
                self.on_exec_approval_request(
                    id,
                    exec_approval_request_from_params(params, &fallback_cwd),
                );
            }
            ServerRequest::FileChangeRequestApproval { params, .. } => {
                self.on_apply_patch_approval_request(
                    id,
                    patch_approval_request_from_params(params),
                );
            }
            ServerRequest::McpServerElicitationRequest { request_id, params } => {
                self.on_elicitation_request(request_id, params);
            }
            ServerRequest::PermissionsRequestApproval { params, .. } => {
                self.on_request_permissions(request_permissions_from_params(params));
            }
            ServerRequest::ToolRequestUserInput { params, .. } => {
                self.on_request_user_input(params);
            }
            ServerRequest::DynamicToolCall { .. }
            | ServerRequest::AttestationGenerate { .. }
            | ServerRequest::ApplyPatchApproval { .. }
            | ServerRequest::ExecCommandApproval { .. } => {
                if replay_kind.is_none() {
                    self.add_error_message(TUI_STUB_MESSAGE.to_string());
                }
            }
        }
    }

    pub(crate) fn handle_skills_list_response(&mut self, response: SkillsListResponse) {
        self.on_list_skills(response);
    }

    pub(super) fn on_patch_apply_output_delta(&mut self, _item_id: String, _delta: String) {}

    pub(super) fn on_guardian_review_notification(
        &mut self,
        id: String,
        turn_id: String,
        started_at_ms: i64,
        review: deepseek_vibing_app_server_protocol::GuardianApprovalReview,
        completion: Option<(
            i64,
            deepseek_vibing_app_server_protocol::AutoReviewDecisionSource,
        )>,
        action: GuardianApprovalReviewAction,
    ) {
        let (completed_at_ms, decision_source) = match completion {
            Some((completed_at_ms, decision_source)) => {
                (Some(completed_at_ms), Some(decision_source))
            }
            None => (None, None),
        };

        self.on_guardian_assessment(GuardianAssessmentEvent {
            id,
            target_item_id: None,
            turn_id,
            started_at_ms,
            completed_at_ms,
            status: match review.status {
                deepseek_vibing_app_server_protocol::GuardianApprovalReviewStatus::InProgress => {
                    GuardianAssessmentStatus::InProgress
                }
                deepseek_vibing_app_server_protocol::GuardianApprovalReviewStatus::Approved => {
                    GuardianAssessmentStatus::Approved
                }
                deepseek_vibing_app_server_protocol::GuardianApprovalReviewStatus::Denied => {
                    GuardianAssessmentStatus::Denied
                }
                deepseek_vibing_app_server_protocol::GuardianApprovalReviewStatus::TimedOut => {
                    GuardianAssessmentStatus::TimedOut
                }
                deepseek_vibing_app_server_protocol::GuardianApprovalReviewStatus::Aborted => {
                    GuardianAssessmentStatus::Aborted
                }
            },
            risk_level: review.risk_level.map(|risk_level| match risk_level {
                deepseek_vibing_app_server_protocol::GuardianRiskLevel::Low => {
                    deepseek_vibing_protocol::approvals::GuardianRiskLevel::Low
                }
                deepseek_vibing_app_server_protocol::GuardianRiskLevel::Medium => {
                    deepseek_vibing_protocol::approvals::GuardianRiskLevel::Medium
                }
                deepseek_vibing_app_server_protocol::GuardianRiskLevel::High => {
                    deepseek_vibing_protocol::approvals::GuardianRiskLevel::High
                }
                deepseek_vibing_app_server_protocol::GuardianRiskLevel::Critical => {
                    deepseek_vibing_protocol::approvals::GuardianRiskLevel::Critical
                }
            }),
            user_authorization: review.user_authorization.map(|user_authorization| {
                match user_authorization {
                    deepseek_vibing_app_server_protocol::GuardianUserAuthorization::Unknown => {
                        deepseek_vibing_protocol::approvals::GuardianUserAuthorization::Unknown
                    }
                    deepseek_vibing_app_server_protocol::GuardianUserAuthorization::Low => {
                        deepseek_vibing_protocol::approvals::GuardianUserAuthorization::Low
                    }
                    deepseek_vibing_app_server_protocol::GuardianUserAuthorization::Medium => {
                        deepseek_vibing_protocol::approvals::GuardianUserAuthorization::Medium
                    }
                    deepseek_vibing_app_server_protocol::GuardianUserAuthorization::High => {
                        deepseek_vibing_protocol::approvals::GuardianUserAuthorization::High
                    }
                }
            }),
            rationale: review.rationale,
            decision_source: decision_source.map(|source| match source {
                deepseek_vibing_app_server_protocol::AutoReviewDecisionSource::Agent => {
                    GuardianAssessmentDecisionSource::Agent
                }
            }),
            action: action.into(),
        });
    }

    pub(super) fn on_shutdown_complete(&mut self) {
        self.request_immediate_exit();
    }

    pub(super) fn on_turn_diff(&mut self, unified_diff: String) {
        debug!("TurnDiffEvent: {unified_diff}");
        self.refresh_status_line();
    }

    pub(super) fn on_deprecation_notice(&mut self, summary: String, details: Option<String>) {
        self.add_to_history(history_cell::new_deprecation_notice(summary, details));
        self.request_redraw();
    }
}
