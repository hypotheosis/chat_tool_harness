pub use crate::application::dto::*;
use crate::application::{Application, CredentialReply};
use crate::error::CoreResult;
use crate::frb_generated::StreamSink;
use crate::model::PlatformReply;
use flutter_rust_bridge::{frb, DartFnFuture};

#[frb(opaque)]
pub struct AgentHandle(Application);

pub async fn open_agent(
    data_dir: String,
    resolve_credential: impl Fn(String) -> DartFnFuture<CredentialReply> + Send + Sync + 'static,
    set_service_active: impl Fn(crate::model::RunHostRequest) -> DartFnFuture<PlatformReply>
        + Send
        + Sync
        + 'static,
) -> CoreResult<AgentHandle> {
    Application::open(
        data_dir,
        move |key| Box::pin(resolve_credential(key)),
        move |active| Box::pin(set_service_active(active)),
    )
    .await
    .map(AgentHandle)
}
pub async fn agent_command(
    handle: &AgentHandle,
    command: AppCommand,
) -> CoreResult<AgentCommandReply> {
    handle.0.command(command).await
}
pub async fn agent_snapshot(handle: &AgentHandle) -> CoreResult<AgentAppSnapshot> {
    handle.0.snapshot().await
}
pub async fn agent_events(
    handle: &AgentHandle,
    after_sequence: u64,
    sink: StreamSink<AgentAppEvent>,
) -> CoreResult<()> {
    // FRB 2.13 has no idle sink cancellation signal. Replay one finite batch;
    // the Dart backend owns subsequent polling and can unsubscribe without Stop.
    if handle.0.closed.is_cancelled() {
        return Ok(());
    }
    let mut subscription = handle.0.subscribe(after_sequence);
    for (sequence, conversation, _) in subscription.poll().await? {
        if sink
            .add(AgentAppEvent {
                sequence,
                conversation_id: conversation.0.to_string(),
            })
            .is_err()
        {
            break;
        }
    }
    Ok(())
}

pub async fn close_agent(handle: &AgentHandle) -> CoreResult<()> {
    handle.0.close().await
}
