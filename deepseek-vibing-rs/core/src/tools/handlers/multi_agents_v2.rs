//! Implements the MultiAgentV2 collaboration tool surface.

use crate::agent::AgentStatus;
use crate::agent::agent_resolver::resolve_agent_target;
use crate::function_tool::FunctionCallError;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::context::boxed_tool_output;
use crate::tools::handlers::multi_agents_common::*;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolExecutor;
use deepseek_vibing_protocol::AgentPath;
use deepseek_vibing_protocol::models::ResponseInputItem;
use deepseek_vibing_protocol::openai_models::ReasoningEffort;
use deepseek_vibing_protocol::protocol::CollabAgentInteractionBeginEvent;
use deepseek_vibing_protocol::protocol::CollabAgentInteractionEndEvent;
use deepseek_vibing_protocol::protocol::CollabAgentSpawnBeginEvent;
use deepseek_vibing_protocol::protocol::CollabAgentSpawnEndEvent;
use deepseek_vibing_protocol::protocol::CollabCloseBeginEvent;
use deepseek_vibing_protocol::protocol::CollabCloseEndEvent;
use deepseek_vibing_protocol::protocol::CollabWaitingBeginEvent;
use deepseek_vibing_protocol::protocol::CollabWaitingEndEvent;
use deepseek_vibing_protocol::user_input::UserInput;
use deepseek_vibing_tools::ToolName;
use serde::Deserialize;
use serde::Serialize;

pub(crate) use close_agent::Handler as CloseAgentHandler;
pub(crate) use followup_task::Handler as FollowupTaskHandler;
pub(crate) use list_agents::Handler as ListAgentsHandler;
pub(crate) use send_message::Handler as SendMessageHandler;
pub(crate) use spawn::Handler as SpawnAgentHandler;
pub(crate) use wait::Handler as WaitAgentHandler;

mod close_agent;
mod followup_task;
mod list_agents;
mod message_tool;
mod send_message;
mod spawn;
pub(crate) mod wait;
