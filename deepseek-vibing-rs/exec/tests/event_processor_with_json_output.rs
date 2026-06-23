use deepseek_vibing_app_server_protocol::CollabAgentState as ApiCollabAgentState;
use deepseek_vibing_app_server_protocol::CollabAgentStatus as ApiCollabAgentStatus;
use deepseek_vibing_app_server_protocol::CollabAgentTool;
use deepseek_vibing_app_server_protocol::CollabAgentToolCallStatus as ApiCollabAgentToolCallStatus;
use deepseek_vibing_app_server_protocol::CommandAction;
use deepseek_vibing_app_server_protocol::CommandExecutionSource;
use deepseek_vibing_app_server_protocol::CommandExecutionStatus as ApiCommandExecutionStatus;
use deepseek_vibing_app_server_protocol::ErrorNotification;
use deepseek_vibing_app_server_protocol::FileUpdateChange as ApiFileUpdateChange;
use deepseek_vibing_app_server_protocol::ItemCompletedNotification;
use deepseek_vibing_app_server_protocol::ItemStartedNotification;
use deepseek_vibing_app_server_protocol::McpToolCallError;
use deepseek_vibing_app_server_protocol::McpToolCallResult;
use deepseek_vibing_app_server_protocol::McpToolCallStatus as ApiMcpToolCallStatus;
use deepseek_vibing_app_server_protocol::PatchApplyStatus as ApiPatchApplyStatus;
use deepseek_vibing_app_server_protocol::PatchChangeKind as ApiPatchChangeKind;
use deepseek_vibing_app_server_protocol::ServerNotification;
use deepseek_vibing_app_server_protocol::ThreadItem;
use deepseek_vibing_app_server_protocol::ThreadTokenUsage;
use deepseek_vibing_app_server_protocol::TokenUsageBreakdown;
use deepseek_vibing_app_server_protocol::Turn;
use deepseek_vibing_app_server_protocol::TurnCompletedNotification;
use deepseek_vibing_app_server_protocol::TurnError;
use deepseek_vibing_app_server_protocol::TurnPlanStep;
use deepseek_vibing_app_server_protocol::TurnPlanStepStatus;
use deepseek_vibing_app_server_protocol::TurnPlanUpdatedNotification;
use deepseek_vibing_app_server_protocol::TurnStartedNotification;
use deepseek_vibing_app_server_protocol::TurnStatus;
use deepseek_vibing_app_server_protocol::WebSearchAction as ApiWebSearchAction;
use deepseek_vibing_protocol::SessionId;
use deepseek_vibing_protocol::ThreadId;
use deepseek_vibing_protocol::models::PermissionProfile;
use deepseek_vibing_protocol::models::WebSearchAction;
use deepseek_vibing_protocol::protocol::AskForApproval;
use deepseek_vibing_protocol::protocol::SessionConfiguredEvent;
use deepseek_vibing_utils_absolute_path::test_support::PathBufExt;
use deepseek_vibing_utils_absolute_path::test_support::test_path_buf;
use pretty_assertions::assert_eq;
use serde_json::json;

use deepseek_vibing_exec::AgentMessageItem;
use deepseek_vibing_exec::CollabAgentState;
use deepseek_vibing_exec::CollabAgentStatus;
use deepseek_vibing_exec::CollabTool;
use deepseek_vibing_exec::CollabToolCallItem;
use deepseek_vibing_exec::CollabToolCallStatus;
use deepseek_vibing_exec::CollectedThreadEvents;
use deepseek_vibing_exec::CommandExecutionItem;
use deepseek_vibing_exec::CommandExecutionStatus;
use deepseek_vibing_exec::DeepSeekVibingStatus;
use deepseek_vibing_exec::ErrorItem;
use deepseek_vibing_exec::EventProcessorWithJsonOutput;
use deepseek_vibing_exec::ExecThreadItem;
use deepseek_vibing_exec::FileChangeItem;
use deepseek_vibing_exec::FileUpdateChange as ExecFileUpdateChange;
use deepseek_vibing_exec::ItemCompletedEvent;
use deepseek_vibing_exec::ItemStartedEvent;
use deepseek_vibing_exec::ItemUpdatedEvent;
use deepseek_vibing_exec::McpToolCallItem;
use deepseek_vibing_exec::McpToolCallItemError;
use deepseek_vibing_exec::McpToolCallItemResult;
use deepseek_vibing_exec::McpToolCallStatus;
use deepseek_vibing_exec::PatchApplyStatus;
use deepseek_vibing_exec::PatchChangeKind;
use deepseek_vibing_exec::ReasoningItem;
use deepseek_vibing_exec::ThreadErrorEvent;
use deepseek_vibing_exec::ThreadEvent;
use deepseek_vibing_exec::ThreadItemDetails;
use deepseek_vibing_exec::ThreadStartedEvent;
use deepseek_vibing_exec::TodoItem;
use deepseek_vibing_exec::TodoListItem;
use deepseek_vibing_exec::TurnCompletedEvent;
use deepseek_vibing_exec::TurnFailedEvent;
use deepseek_vibing_exec::TurnStartedEvent;
use deepseek_vibing_exec::Usage;
use deepseek_vibing_exec::WebSearchItem;

#[test]
fn map_todo_items_preserves_text_and_completion_state() {
    let items = EventProcessorWithJsonOutput::map_todo_items(&[
        TurnPlanStep {
            step: "inspect bootstrap".to_string(),
            status: TurnPlanStepStatus::InProgress,
        },
        TurnPlanStep {
            step: "drop legacy notifications".to_string(),
            status: TurnPlanStepStatus::Completed,
        },
    ]);

    assert_eq!(
        items,
        vec![
            TodoItem {
                text: "inspect bootstrap".to_string(),
                completed: false,
            },
            TodoItem {
                text: "drop legacy notifications".to_string(),
                completed: true,
            },
        ]
    );
}

#[test]
fn session_configured_produces_thread_started_event() {
    let thread_id = ThreadId::from_string("67e55044-10b1-426f-9247-bb680e5fe0c8")
        .expect("thread id should parse");
    let session_configured = SessionConfiguredEvent {
        session_id: SessionId::from(thread_id),
        thread_id,
        forked_from_id: None,
        parent_thread_id: None,
        thread_source: None,
        thread_name: None,
        model: "deepseek-vibing-mini-latest".to_string(),
        model_provider_id: "test-provider".to_string(),
        service_tier: None,
        approval_policy: AskForApproval::Never,
        approvals_reviewer: deepseek_vibing_protocol::config_types::ApprovalsReviewer::User,
        permission_profile: PermissionProfile::read_only(),
        active_permission_profile: None,
        cwd: test_path_buf("/tmp/project").abs(),
        reasoning_effort: None,
        initial_messages: None,
        network_proxy: None,
        rollout_path: None,
    };

    assert_eq!(
        EventProcessorWithJsonOutput::thread_started_event(&session_configured),
        ThreadEvent::ThreadStarted(ThreadStartedEvent {
            thread_id: "67e55044-10b1-426f-9247-bb680e5fe0c8".to_string(),
        })
    );
}

#[test]
fn turn_started_emits_turn_started_event() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected =
        processor.collect_thread_events(ServerNotification::TurnStarted(TurnStartedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items_view: deepseek_vibing_app_server_protocol::TurnItemsView::Full,
                items: Vec::new(),
                status: TurnStatus::InProgress,
                error: None,
                started_at: None,
                completed_at: None,
                duration_ms: None,
            },
        }));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: vec![ThreadEvent::TurnStarted(TurnStartedEvent {})],
            status: DeepSeekVibingStatus::Running,
        }
    );
}

#[test]
fn command_execution_started_and_completed_translate_to_thread_events() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);
    let command_item = ThreadItem::CommandExecution {
        id: "cmd-1".to_string(),
        command: "ls".to_string(),
        cwd: test_path_buf("/tmp/project").abs(),
        process_id: Some("123".to_string()),
        source: CommandExecutionSource::UserShell,
        status: ApiCommandExecutionStatus::InProgress,
        command_actions: Vec::<CommandAction>::new(),
        aggregated_output: None,
        exit_code: None,
        duration_ms: None,
    };

    let started =
        processor.collect_thread_events(ServerNotification::ItemStarted(ItemStartedNotification {
            item: command_item,
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            started_at_ms: 0,
        }));
    assert_eq!(
        started,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemStarted(ItemStartedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::CommandExecution(CommandExecutionItem {
                        command: "ls".to_string(),
                        aggregated_output: String::new(),
                        exit_code: None,
                        status: CommandExecutionStatus::InProgress,
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );

    let completed = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::CommandExecution {
                id: "cmd-1".to_string(),
                command: "ls".to_string(),
                cwd: test_path_buf("/tmp/project").abs(),
                process_id: Some("123".to_string()),
                source: CommandExecutionSource::UserShell,
                status: ApiCommandExecutionStatus::Completed,
                command_actions: Vec::<CommandAction>::new(),
                aggregated_output: Some("a.txt\n".to_string()),
                exit_code: Some(0),
                duration_ms: Some(3),
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));
    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::CommandExecution(CommandExecutionItem {
                        command: "ls".to_string(),
                        aggregated_output: "a.txt\n".to_string(),
                        exit_code: Some(0),
                        status: CommandExecutionStatus::Completed,
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
}

#[test]
fn empty_reasoning_items_are_ignored() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::Reasoning {
                id: "reasoning-1".to_string(),
                summary: Vec::new(),
                content: vec!["raw reasoning".to_string()],
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: Vec::new(),
            status: DeepSeekVibingStatus::Running,
        }
    );
}

#[test]
fn unsupported_items_do_not_consume_synthetic_ids() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let ignored = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::Plan {
                id: "plan-1".to_string(),
                text: "ignored plan".to_string(),
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    assert_eq!(
        ignored,
        CollectedThreadEvents {
            events: Vec::new(),
            status: DeepSeekVibingStatus::Running,
        }
    );

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::AgentMessage {
                id: "message-1".to_string(),
                text: "hello".to_string(),
                phase: None,
                memory_citation: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::AgentMessage(AgentMessageItem {
                        text: "hello".to_string(),
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
}

#[test]
fn reasoning_items_emit_summary_not_raw_content() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::Reasoning {
                id: "reasoning-1".to_string(),
                summary: vec!["safe summary".to_string()],
                content: vec!["raw reasoning".to_string()],
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::Reasoning(ReasoningItem {
                        text: "safe summary".to_string(),
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
}

#[test]
fn web_search_completion_preserves_query_and_action() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::WebSearch {
                id: "search-1".to_string(),
                query: "rust async await".to_string(),
                action: Some(ApiWebSearchAction::Search {
                    query: Some("rust async await".to_string()),
                    queries: None,
                }),
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::WebSearch(WebSearchItem {
                        id: "search-1".to_string(),
                        query: "rust async await".to_string(),
                        action: WebSearchAction::Search {
                            query: Some("rust async await".to_string()),
                            queries: None,
                        },
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
}

#[test]
fn web_search_start_and_completion_reuse_item_id() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let started =
        processor.collect_thread_events(ServerNotification::ItemStarted(ItemStartedNotification {
            item: ThreadItem::WebSearch {
                id: "search-1".to_string(),
                query: String::new(),
                action: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            started_at_ms: 0,
        }));

    let completed = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::WebSearch {
                id: "search-1".to_string(),
                query: "rust async await".to_string(),
                action: Some(ApiWebSearchAction::Search {
                    query: Some("rust async await".to_string()),
                    queries: None,
                }),
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    assert_eq!(
        started,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemStarted(ItemStartedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::WebSearch(WebSearchItem {
                        id: "search-1".to_string(),
                        query: String::new(),
                        action: WebSearchAction::Other,
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::WebSearch(WebSearchItem {
                        id: "search-1".to_string(),
                        query: "rust async await".to_string(),
                        action: WebSearchAction::Search {
                            query: Some("rust async await".to_string()),
                            queries: None,
                        },
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
}

#[test]
fn mcp_tool_call_begin_and_end_emit_item_events() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let started =
        processor.collect_thread_events(ServerNotification::ItemStarted(ItemStartedNotification {
            item: ThreadItem::McpToolCall {
                id: "mcp-1".to_string(),
                server: "server_a".to_string(),
                tool: "tool_x".to_string(),
                status: ApiMcpToolCallStatus::InProgress,
                arguments: json!({ "key": "value" }),
                mcp_app_resource_uri: None,
                plugin_id: None,
                result: None,
                error: None,
                duration_ms: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            started_at_ms: 0,
        }));
    let completed = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::McpToolCall {
                id: "mcp-1".to_string(),
                server: "server_a".to_string(),
                tool: "tool_x".to_string(),
                status: ApiMcpToolCallStatus::Completed,
                arguments: json!({ "key": "value" }),
                mcp_app_resource_uri: None,
                plugin_id: None,
                result: Some(Box::new(McpToolCallResult {
                    content: Vec::new(),
                    structured_content: None,
                    meta: None,
                })),
                error: None,
                duration_ms: Some(1_000),
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    assert_eq!(
        started,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemStarted(ItemStartedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::McpToolCall(McpToolCallItem {
                        server: "server_a".to_string(),
                        tool: "tool_x".to_string(),
                        arguments: json!({ "key": "value" }),
                        result: None,
                        error: None,
                        status: McpToolCallStatus::InProgress,
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::McpToolCall(McpToolCallItem {
                        server: "server_a".to_string(),
                        tool: "tool_x".to_string(),
                        arguments: json!({ "key": "value" }),
                        result: Some(McpToolCallItemResult {
                            content: Vec::new(),
                            meta: None,
                            structured_content: None,
                        }),
                        error: None,
                        status: McpToolCallStatus::Completed,
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
}

#[test]
fn mcp_tool_call_failure_sets_failed_status() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::McpToolCall {
                id: "mcp-2".to_string(),
                server: "server_b".to_string(),
                tool: "tool_y".to_string(),
                status: ApiMcpToolCallStatus::Failed,
                arguments: json!({ "param": 42 }),
                mcp_app_resource_uri: None,
                plugin_id: None,
                result: None,
                error: Some(McpToolCallError {
                    message: "tool exploded".to_string(),
                }),
                duration_ms: Some(5),
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::McpToolCall(McpToolCallItem {
                        server: "server_b".to_string(),
                        tool: "tool_y".to_string(),
                        arguments: json!({ "param": 42 }),
                        result: None,
                        error: Some(McpToolCallItemError {
                            message: "tool exploded".to_string(),
                        }),
                        status: McpToolCallStatus::Failed,
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
}

#[test]
fn mcp_tool_call_defaults_arguments_and_preserves_structured_content() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let started =
        processor.collect_thread_events(ServerNotification::ItemStarted(ItemStartedNotification {
            item: ThreadItem::McpToolCall {
                id: "mcp-3".to_string(),
                server: "server_c".to_string(),
                tool: "tool_z".to_string(),
                status: ApiMcpToolCallStatus::InProgress,
                arguments: serde_json::Value::Null,
                mcp_app_resource_uri: None,
                plugin_id: None,
                result: None,
                error: None,
                duration_ms: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            started_at_ms: 0,
        }));
    let completed = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::McpToolCall {
                id: "mcp-3".to_string(),
                server: "server_c".to_string(),
                tool: "tool_z".to_string(),
                status: ApiMcpToolCallStatus::Completed,
                arguments: serde_json::Value::Null,
                mcp_app_resource_uri: None,
                plugin_id: None,
                result: Some(Box::new(McpToolCallResult {
                    content: vec![json!({
                        "type": "text",
                        "text": "done",
                    })],
                    structured_content: Some(json!({ "status": "ok" })),
                    meta: None,
                })),
                error: None,
                duration_ms: Some(10),
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    assert_eq!(
        started,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemStarted(ItemStartedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::McpToolCall(McpToolCallItem {
                        server: "server_c".to_string(),
                        tool: "tool_z".to_string(),
                        arguments: serde_json::Value::Null,
                        result: None,
                        error: None,
                        status: McpToolCallStatus::InProgress,
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::McpToolCall(McpToolCallItem {
                        server: "server_c".to_string(),
                        tool: "tool_z".to_string(),
                        arguments: serde_json::Value::Null,
                        result: Some(McpToolCallItemResult {
                            content: vec![json!({
                                "type": "text",
                                "text": "done",
                            })],
                            meta: None,
                            structured_content: Some(json!({ "status": "ok" })),
                        }),
                        error: None,
                        status: McpToolCallStatus::Completed,
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
}

#[test]
fn collab_spawn_begin_and_end_emit_item_events() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let started =
        processor.collect_thread_events(ServerNotification::ItemStarted(ItemStartedNotification {
            item: ThreadItem::CollabAgentToolCall {
                id: "collab-1".to_string(),
                tool: CollabAgentTool::SpawnAgent,
                status: ApiCollabAgentToolCallStatus::InProgress,
                sender_thread_id: "thread-parent".to_string(),
                receiver_thread_ids: Vec::new(),
                prompt: Some("draft a plan".to_string()),
                model: Some("gpt-5".to_string()),
                reasoning_effort: None,
                agents_states: std::collections::HashMap::new(),
            },
            thread_id: "thread-parent".to_string(),
            turn_id: "turn-1".to_string(),
            started_at_ms: 0,
        }));
    let completed = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::CollabAgentToolCall {
                id: "collab-1".to_string(),
                tool: CollabAgentTool::SpawnAgent,
                status: ApiCollabAgentToolCallStatus::Completed,
                sender_thread_id: "thread-parent".to_string(),
                receiver_thread_ids: vec!["thread-child".to_string()],
                prompt: Some("draft a plan".to_string()),
                model: Some("gpt-5".to_string()),
                reasoning_effort: None,
                agents_states: std::collections::HashMap::from([(
                    "thread-child".to_string(),
                    ApiCollabAgentState {
                        status: ApiCollabAgentStatus::Running,
                        message: None,
                    },
                )]),
            },
            thread_id: "thread-parent".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    assert_eq!(
        started,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemStarted(ItemStartedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::CollabToolCall(CollabToolCallItem {
                        tool: CollabTool::SpawnAgent,
                        sender_thread_id: "thread-parent".to_string(),
                        receiver_thread_ids: Vec::new(),
                        prompt: Some("draft a plan".to_string()),
                        agents_states: std::collections::HashMap::new(),
                        status: CollabToolCallStatus::InProgress,
                    },),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::CollabToolCall(CollabToolCallItem {
                        tool: CollabTool::SpawnAgent,
                        sender_thread_id: "thread-parent".to_string(),
                        receiver_thread_ids: vec!["thread-child".to_string()],
                        prompt: Some("draft a plan".to_string()),
                        agents_states: std::collections::HashMap::from([(
                            "thread-child".to_string(),
                            CollabAgentState {
                                status: CollabAgentStatus::Running,
                                message: None,
                            },
                        )]),
                        status: CollabToolCallStatus::Completed,
                    },),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
}

#[test]
fn file_change_completion_maps_change_kinds() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::FileChange {
                id: "patch-1".to_string(),
                changes: vec![
                    ApiFileUpdateChange {
                        path: "a/added.txt".to_string(),
                        kind: ApiPatchChangeKind::Add,
                        diff: String::new(),
                    },
                    ApiFileUpdateChange {
                        path: "b/deleted.txt".to_string(),
                        kind: ApiPatchChangeKind::Delete,
                        diff: String::new(),
                    },
                    ApiFileUpdateChange {
                        path: "c/modified.txt".to_string(),
                        kind: ApiPatchChangeKind::Update { move_path: None },
                        diff: "@@ -1 +1 @@".to_string(),
                    },
                ],
                status: ApiPatchApplyStatus::Completed,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::FileChange(FileChangeItem {
                        changes: vec![
                            ExecFileUpdateChange {
                                path: "a/added.txt".to_string(),
                                kind: PatchChangeKind::Add,
                            },
                            ExecFileUpdateChange {
                                path: "b/deleted.txt".to_string(),
                                kind: PatchChangeKind::Delete,
                            },
                            ExecFileUpdateChange {
                                path: "c/modified.txt".to_string(),
                                kind: PatchChangeKind::Update,
                            },
                        ],
                        status: PatchApplyStatus::Completed,
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
}

#[test]
fn file_change_declined_maps_to_failed_status() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::FileChange {
                id: "patch-2".to_string(),
                changes: vec![ApiFileUpdateChange {
                    path: "file.txt".to_string(),
                    kind: ApiPatchChangeKind::Update { move_path: None },
                    diff: "@@ -1 +1 @@".to_string(),
                }],
                status: ApiPatchApplyStatus::Declined,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::FileChange(FileChangeItem {
                        changes: vec![ExecFileUpdateChange {
                            path: "file.txt".to_string(),
                            kind: PatchChangeKind::Update,
                        }],
                        status: PatchApplyStatus::Failed,
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
}

#[test]
fn agent_message_item_updates_final_message() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::AgentMessage {
                id: "msg-1".to_string(),
                text: "hello".to_string(),
                phase: None,
                memory_citation: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::AgentMessage(AgentMessageItem {
                        text: "hello".to_string(),
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
    assert_eq!(processor.final_message(), Some("hello"));
}

#[test]
fn agent_message_item_started_is_ignored() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected =
        processor.collect_thread_events(ServerNotification::ItemStarted(ItemStartedNotification {
            item: ThreadItem::AgentMessage {
                id: "msg-1".to_string(),
                text: "hello".to_string(),
                phase: None,
                memory_citation: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            started_at_ms: 0,
        }));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: Vec::new(),
            status: DeepSeekVibingStatus::Running,
        }
    );
}

#[test]
fn reasoning_item_completed_uses_synthetic_id() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::Reasoning {
                id: "rs-1".to_string(),
                summary: vec!["thinking...".to_string()],
                content: vec!["raw".to_string()],
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::Reasoning(ReasoningItem {
                        text: "thinking...".to_string(),
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
}

#[test]
fn warning_event_produces_error_item() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_warning(
        "Heads up: Long conversations and multiple compactions can cause the model to be less accurate. Start a new conversation when possible to keep conversations small and targeted.".to_string(),
    );

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::Error(ErrorItem {
                        message: "Heads up: Long conversations and multiple compactions can cause the model to be less accurate. Start a new conversation when possible to keep conversations small and targeted.".to_string(),
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
}

#[test]
fn plan_update_emits_started_then_updated_then_completed() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let started = processor.collect_thread_events(ServerNotification::TurnPlanUpdated(
        TurnPlanUpdatedNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            explanation: None,
            plan: vec![
                TurnPlanStep {
                    step: "step one".to_string(),
                    status: TurnPlanStepStatus::Pending,
                },
                TurnPlanStep {
                    step: "step two".to_string(),
                    status: TurnPlanStepStatus::InProgress,
                },
            ],
        },
    ));
    assert_eq!(
        started,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemStarted(ItemStartedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::TodoList(TodoListItem {
                        items: vec![
                            TodoItem {
                                text: "step one".to_string(),
                                completed: false,
                            },
                            TodoItem {
                                text: "step two".to_string(),
                                completed: false,
                            },
                        ],
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );

    let updated = processor.collect_thread_events(ServerNotification::TurnPlanUpdated(
        TurnPlanUpdatedNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            explanation: None,
            plan: vec![
                TurnPlanStep {
                    step: "step one".to_string(),
                    status: TurnPlanStepStatus::Completed,
                },
                TurnPlanStep {
                    step: "step two".to_string(),
                    status: TurnPlanStepStatus::InProgress,
                },
            ],
        },
    ));
    assert_eq!(
        updated,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemUpdated(ItemUpdatedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::TodoList(TodoListItem {
                        items: vec![
                            TodoItem {
                                text: "step one".to_string(),
                                completed: true,
                            },
                            TodoItem {
                                text: "step two".to_string(),
                                completed: false,
                            },
                        ],
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );

    let completed = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items_view: deepseek_vibing_app_server_protocol::TurnItemsView::Full,
                items: Vec::new(),
                status: TurnStatus::Completed,
                error: None,
                started_at: None,
                completed_at: None,
                duration_ms: None,
            },
        },
    ));
    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![
                ThreadEvent::ItemCompleted(ItemCompletedEvent {
                    item: ExecThreadItem {
                        id: "item_0".to_string(),
                        details: ThreadItemDetails::TodoList(TodoListItem {
                            items: vec![
                                TodoItem {
                                    text: "step one".to_string(),
                                    completed: true,
                                },
                                TodoItem {
                                    text: "step two".to_string(),
                                    completed: false,
                                },
                            ],
                        }),
                    },
                }),
                ThreadEvent::TurnCompleted(TurnCompletedEvent {
                    usage: Usage::default(),
                }),
            ],
            status: DeepSeekVibingStatus::InitiateShutdown,
        }
    );
}

#[test]
fn plan_update_after_completion_starts_new_todo_list_with_new_id() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let _ = processor.collect_thread_events(ServerNotification::TurnPlanUpdated(
        TurnPlanUpdatedNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            explanation: None,
            plan: vec![TurnPlanStep {
                step: "only".to_string(),
                status: TurnPlanStepStatus::Pending,
            }],
        },
    ));
    let _ = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items_view: deepseek_vibing_app_server_protocol::TurnItemsView::Full,
                items: Vec::new(),
                status: TurnStatus::Completed,
                error: None,
                started_at: None,
                completed_at: None,
                duration_ms: None,
            },
        },
    ));

    let restarted = processor.collect_thread_events(ServerNotification::TurnPlanUpdated(
        TurnPlanUpdatedNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-2".to_string(),
            explanation: None,
            plan: vec![TurnPlanStep {
                step: "again".to_string(),
                status: TurnPlanStepStatus::Pending,
            }],
        },
    ));

    assert_eq!(
        restarted,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemStarted(ItemStartedEvent {
                item: ExecThreadItem {
                    id: "item_1".to_string(),
                    details: ThreadItemDetails::TodoList(TodoListItem {
                        items: vec![TodoItem {
                            text: "again".to_string(),
                            completed: false,
                        }],
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );
}

#[test]
fn token_usage_update_is_emitted_on_turn_completion() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let usage_update =
        processor.collect_thread_events(ServerNotification::ThreadTokenUsageUpdated(
            deepseek_vibing_app_server_protocol::ThreadTokenUsageUpdatedNotification {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                token_usage: ThreadTokenUsage {
                    total: TokenUsageBreakdown {
                        total_tokens: 42,
                        input_tokens: 10,
                        cached_input_tokens: 3,
                        output_tokens: 29,
                        reasoning_output_tokens: 7,
                    },
                    last: TokenUsageBreakdown {
                        total_tokens: 42,
                        input_tokens: 10,
                        cached_input_tokens: 3,
                        output_tokens: 29,
                        reasoning_output_tokens: 7,
                    },
                    model_context_window: Some(128_000),
                },
            },
        ));
    assert_eq!(
        usage_update,
        CollectedThreadEvents {
            events: Vec::new(),
            status: DeepSeekVibingStatus::Running,
        }
    );

    let completed = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items_view: deepseek_vibing_app_server_protocol::TurnItemsView::Full,
                items: Vec::new(),
                status: TurnStatus::Completed,
                error: None,
                started_at: None,
                completed_at: None,
                duration_ms: None,
            },
        },
    ));
    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![ThreadEvent::TurnCompleted(TurnCompletedEvent {
                usage: Usage {
                    input_tokens: 10,
                    cached_input_tokens: 3,
                    output_tokens: 29,
                    reasoning_output_tokens: 7,
                },
            })],
            status: DeepSeekVibingStatus::InitiateShutdown,
        }
    );
}

#[test]
fn turn_completion_recovers_final_message_from_turn_items() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let completed = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items_view: deepseek_vibing_app_server_protocol::TurnItemsView::Full,
                items: vec![ThreadItem::AgentMessage {
                    id: "msg-1".to_string(),
                    text: "final answer".to_string(),
                    phase: None,
                    memory_citation: None,
                }],
                status: TurnStatus::Completed,
                error: None,
                started_at: None,
                completed_at: None,
                duration_ms: None,
            },
        },
    ));

    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![ThreadEvent::TurnCompleted(TurnCompletedEvent {
                usage: Usage::default(),
            })],
            status: DeepSeekVibingStatus::InitiateShutdown,
        }
    );
    assert_eq!(processor.final_message(), Some("final answer"));
}

#[test]
fn turn_completion_reconciles_started_items_from_turn_items() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let started =
        processor.collect_thread_events(ServerNotification::ItemStarted(ItemStartedNotification {
            item: ThreadItem::CommandExecution {
                id: "cmd-1".to_string(),
                command: "ls".to_string(),
                cwd: test_path_buf("/tmp/project").abs(),
                process_id: Some("123".to_string()),
                source: CommandExecutionSource::UserShell,
                status: ApiCommandExecutionStatus::InProgress,
                command_actions: Vec::<CommandAction>::new(),
                aggregated_output: None,
                exit_code: None,
                duration_ms: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            started_at_ms: 0,
        }));
    assert_eq!(
        started,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemStarted(ItemStartedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::CommandExecution(CommandExecutionItem {
                        command: "ls".to_string(),
                        aggregated_output: String::new(),
                        exit_code: None,
                        status: CommandExecutionStatus::InProgress,
                    }),
                },
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );

    let completed = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items_view: deepseek_vibing_app_server_protocol::TurnItemsView::Full,
                items: vec![ThreadItem::CommandExecution {
                    id: "cmd-1".to_string(),
                    command: "ls".to_string(),
                    cwd: test_path_buf("/tmp/project").abs(),
                    process_id: Some("123".to_string()),
                    source: CommandExecutionSource::UserShell,
                    status: ApiCommandExecutionStatus::Completed,
                    command_actions: Vec::<CommandAction>::new(),
                    aggregated_output: Some("a.txt\n".to_string()),
                    exit_code: Some(0),
                    duration_ms: Some(3),
                }],
                status: TurnStatus::Completed,
                error: None,
                started_at: None,
                completed_at: None,
                duration_ms: None,
            },
        },
    ));

    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![
                ThreadEvent::ItemCompleted(ItemCompletedEvent {
                    item: ExecThreadItem {
                        id: "item_0".to_string(),
                        details: ThreadItemDetails::CommandExecution(CommandExecutionItem {
                            command: "ls".to_string(),
                            aggregated_output: "a.txt\n".to_string(),
                            exit_code: Some(0),
                            status: CommandExecutionStatus::Completed,
                        }),
                    },
                }),
                ThreadEvent::TurnCompleted(TurnCompletedEvent {
                    usage: Usage::default(),
                }),
            ],
            status: DeepSeekVibingStatus::InitiateShutdown,
        }
    );
}

#[test]
fn turn_completion_overwrites_stale_final_message_from_turn_items() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);
    let _ = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::AgentMessage {
                id: "msg-stale".to_string(),
                text: "stale answer".to_string(),
                phase: None,
                memory_citation: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    let completed = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items_view: deepseek_vibing_app_server_protocol::TurnItemsView::Full,
                items: vec![ThreadItem::AgentMessage {
                    id: "msg-1".to_string(),
                    text: "final answer".to_string(),
                    phase: None,
                    memory_citation: None,
                }],
                status: TurnStatus::Completed,
                error: None,
                started_at: None,
                completed_at: None,
                duration_ms: None,
            },
        },
    ));

    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![ThreadEvent::TurnCompleted(TurnCompletedEvent {
                usage: Usage::default(),
            })],
            status: DeepSeekVibingStatus::InitiateShutdown,
        }
    );
    assert_eq!(processor.final_message(), Some("final answer"));
}

#[test]
fn turn_completion_preserves_streamed_final_message_when_turn_items_are_empty() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);
    let _ = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::AgentMessage {
                id: "msg-streamed".to_string(),
                text: "streamed answer".to_string(),
                phase: None,
                memory_citation: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    let completed = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items_view: deepseek_vibing_app_server_protocol::TurnItemsView::Full,
                items: Vec::new(),
                status: TurnStatus::Completed,
                error: None,
                started_at: None,
                completed_at: None,
                duration_ms: None,
            },
        },
    ));

    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![ThreadEvent::TurnCompleted(TurnCompletedEvent {
                usage: Usage::default(),
            })],
            status: DeepSeekVibingStatus::InitiateShutdown,
        }
    );
    assert_eq!(processor.final_message(), Some("streamed answer"));
}

#[test]
fn failed_turn_clears_stale_final_message() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::AgentMessage {
                id: "msg-1".to_string(),
                text: "partial answer".to_string(),
                phase: None,
                memory_citation: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    assert_eq!(collected.status, DeepSeekVibingStatus::Running);
    assert_eq!(processor.final_message(), Some("partial answer"));

    let collected = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items_view: deepseek_vibing_app_server_protocol::TurnItemsView::Full,
                items: Vec::new(),
                status: TurnStatus::Failed,
                error: Some(TurnError {
                    message: "turn failed".to_string(),
                    additional_details: None,
                    deepseek_vibing_error_info: None,
                }),
                started_at: None,
                completed_at: None,
                duration_ms: None,
            },
        },
    ));

    assert_eq!(collected.status, DeepSeekVibingStatus::InitiateShutdown);
    assert_eq!(processor.final_message(), None);
}

#[test]
fn turn_completion_falls_back_to_final_plan_text() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let completed = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items_view: deepseek_vibing_app_server_protocol::TurnItemsView::Full,
                items: vec![ThreadItem::Plan {
                    id: "plan-1".to_string(),
                    text: "ship the typed adapter".to_string(),
                }],
                status: TurnStatus::Completed,
                error: None,
                started_at: None,
                completed_at: None,
                duration_ms: None,
            },
        },
    ));

    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![ThreadEvent::TurnCompleted(TurnCompletedEvent {
                usage: Usage::default(),
            })],
            status: DeepSeekVibingStatus::InitiateShutdown,
        }
    );
    assert_eq!(processor.final_message(), Some("ship the typed adapter"));
}

#[test]
fn turn_failure_prefers_structured_error_message() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let error = processor.collect_thread_events(ServerNotification::Error(ErrorNotification {
        error: TurnError {
            message: "backend failed".to_string(),
            deepseek_vibing_error_info: None,
            additional_details: Some("request id abc".to_string()),
        },
        will_retry: false,
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
    }));
    assert_eq!(
        error,
        CollectedThreadEvents {
            events: vec![ThreadEvent::Error(ThreadErrorEvent {
                message: "backend failed (request id abc)".to_string(),
            })],
            status: DeepSeekVibingStatus::Running,
        }
    );

    let failed = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items_view: deepseek_vibing_app_server_protocol::TurnItemsView::Full,
                items: Vec::new(),
                status: TurnStatus::Failed,
                error: None,
                started_at: None,
                completed_at: None,
                duration_ms: None,
            },
        },
    ));
    assert_eq!(
        failed,
        CollectedThreadEvents {
            events: vec![ThreadEvent::TurnFailed(TurnFailedEvent {
                error: ThreadErrorEvent {
                    message: "backend failed (request id abc)".to_string(),
                },
            })],
            status: DeepSeekVibingStatus::InitiateShutdown,
        }
    );
}

#[test]
fn model_reroute_surfaces_as_error_item() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ModelRerouted(
        deepseek_vibing_app_server_protocol::ModelReroutedNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            from_model: "gpt-5".to_string(),
            to_model: "gpt-5-mini".to_string(),
            reason: deepseek_vibing_app_server_protocol::ModelRerouteReason::HighRiskCyberActivity,
        },
    ));

    assert_eq!(collected.status, DeepSeekVibingStatus::Running);
    assert_eq!(collected.events.len(), 1);
    let ThreadEvent::ItemCompleted(ItemCompletedEvent { item }) = &collected.events[0] else {
        panic!("expected ItemCompleted");
    };
    assert_eq!(item.id, "item_0");
    assert_eq!(
        item.details,
        ThreadItemDetails::Error(ErrorItem {
            message: "model rerouted: gpt-5 -> gpt-5-mini (HighRiskCyberActivity)".to_string(),
        })
    );
}
