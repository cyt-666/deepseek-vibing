use std::sync::Arc;
use std::sync::Weak;

use deepseek_vibing_app_server_protocol::ServerNotification;
use deepseek_vibing_app_server_protocol::ThreadGoalUpdatedNotification;
use deepseek_vibing_auth::AuthManager;
use deepseek_vibing_core::NewThread;
use deepseek_vibing_core::StartThreadOptions;
use deepseek_vibing_core::ThreadManager;
use deepseek_vibing_core::config::Config;
use deepseek_vibing_extension_api::AgentSpawnFuture;
use deepseek_vibing_extension_api::AgentSpawner;
use deepseek_vibing_extension_api::ExtensionEventSink;
use deepseek_vibing_extension_api::ExtensionRegistry;
use deepseek_vibing_extension_api::ExtensionRegistryBuilder;
use deepseek_vibing_protocol::ThreadId;
use deepseek_vibing_protocol::error::DeepSeekVibingErr;
use deepseek_vibing_protocol::protocol::Event;
use deepseek_vibing_protocol::protocol::EventMsg;

use crate::outgoing_message::OutgoingMessageSender;

pub(crate) fn thread_extensions<S>(
    guardian_agent_spawner: S,
    event_sink: Arc<dyn ExtensionEventSink>,
    _auth_manager: Arc<AuthManager>,
) -> Arc<ExtensionRegistry<Config>>
where
    S: AgentSpawner<StartThreadOptions, Spawned = NewThread, Error = DeepSeekVibingErr> + 'static,
{
    let mut builder = ExtensionRegistryBuilder::<Config>::with_event_sink(event_sink);
    deepseek_vibing_guardian::install(&mut builder, guardian_agent_spawner);
    deepseek_vibing_memories_extension::install(&mut builder, deepseek_vibing_otel::global());
    Arc::new(builder.build())
}

pub(crate) fn app_server_extension_event_sink(
    outgoing: Arc<OutgoingMessageSender>,
) -> Arc<dyn ExtensionEventSink> {
    Arc::new(AppServerExtensionEventSink { outgoing })
}

struct AppServerExtensionEventSink {
    outgoing: Arc<OutgoingMessageSender>,
}

impl ExtensionEventSink for AppServerExtensionEventSink {
    fn emit(&self, event: Event) {
        match event.msg {
            EventMsg::ThreadGoalUpdated(thread_goal_event) => {
                self.outgoing
                    .try_send_server_notification(ServerNotification::ThreadGoalUpdated(
                        ThreadGoalUpdatedNotification {
                            thread_id: thread_goal_event.thread_id.to_string(),
                            turn_id: thread_goal_event.turn_id,
                            goal: thread_goal_event.goal.into(),
                        },
                    ));
            }
            msg => {
                tracing::debug!(event_id = %event.id, ?msg, "dropping unsupported extension event");
            }
        }
    }
}

pub(crate) fn guardian_agent_spawner(
    thread_manager: Weak<ThreadManager>,
) -> impl AgentSpawner<StartThreadOptions, Spawned = NewThread, Error = DeepSeekVibingErr> {
    move |forked_from_thread_id: ThreadId,
          options: StartThreadOptions|
          -> AgentSpawnFuture<'static, NewThread, DeepSeekVibingErr> {
        let thread_manager = thread_manager.clone();
        Box::pin(async move {
            let thread_manager = thread_manager.upgrade().ok_or_else(|| {
                DeepSeekVibingErr::UnsupportedOperation("thread manager dropped".to_string())
            })?;
            thread_manager
                .spawn_subagent(forked_from_thread_id, options)
                .await
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use deepseek_vibing_analytics::AnalyticsEventsClient;
    use deepseek_vibing_app_server_protocol::ServerNotification;
    use deepseek_vibing_app_server_protocol::ThreadGoal as AppServerThreadGoal;
    use deepseek_vibing_app_server_protocol::ThreadGoalStatus as AppServerThreadGoalStatus;
    use deepseek_vibing_protocol::protocol::ThreadGoal;
    use deepseek_vibing_protocol::protocol::ThreadGoalStatus;
    use deepseek_vibing_protocol::protocol::ThreadGoalUpdatedEvent;
    use pretty_assertions::assert_eq;
    use tokio::sync::mpsc;
    use tokio::time::timeout;

    use super::*;
    use crate::outgoing_message::OutgoingEnvelope;
    use crate::outgoing_message::OutgoingMessage;

    #[tokio::test]
    async fn app_server_event_sink_forwards_thread_goal_updates() {
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel(4);
        let outgoing = Arc::new(OutgoingMessageSender::new(
            outgoing_tx,
            AnalyticsEventsClient::disabled(),
        ));
        let sink = app_server_extension_event_sink(outgoing);
        let thread_id = ThreadId::default();

        sink.emit(Event {
            id: "call-1".to_string(),
            msg: EventMsg::ThreadGoalUpdated(ThreadGoalUpdatedEvent {
                thread_id,
                turn_id: Some("turn-1".to_string()),
                goal: ThreadGoal {
                    thread_id,
                    objective: "wire extension events".to_string(),
                    status: ThreadGoalStatus::Active,
                    token_budget: Some(123),
                    tokens_used: 45,
                    time_used_seconds: 6,
                    created_at: 7,
                    updated_at: 8,
                },
            }),
        });

        let envelope = timeout(Duration::from_secs(1), outgoing_rx.recv())
            .await
            .expect("timed out waiting for forwarded extension event")
            .expect("outgoing channel closed unexpectedly");
        let OutgoingEnvelope::Broadcast { message } = envelope else {
            panic!("expected broadcast notification");
        };
        let OutgoingMessage::AppServerNotification(ServerNotification::ThreadGoalUpdated(
            notification,
        )) = message
        else {
            panic!("expected thread goal updated notification");
        };

        assert_eq!(
            ThreadGoalUpdatedNotification {
                thread_id: thread_id.to_string(),
                turn_id: Some("turn-1".to_string()),
                goal: AppServerThreadGoal {
                    thread_id: thread_id.to_string(),
                    objective: "wire extension events".to_string(),
                    status: AppServerThreadGoalStatus::Active,
                    token_budget: Some(123),
                    tokens_used: 45,
                    time_used_seconds: 6,
                    created_at: 7,
                    updated_at: 8,
                },
            },
            notification
        );
    }
}
