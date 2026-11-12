pub mod context;
mod state;
pub(crate) use state::{already_decided, decide, origin_acknowledged, run_state, Decided};

use crate::error::{CoreError, CoreResult};
use crate::inference::{InferenceClient, InferenceEvent, InferenceRequest, InferenceTurn};
use crate::mcp::{McpClient, McpError, McpSession, ToolOutcome};
use crate::model::core::*;
use crate::net::{Cancellation, CredentialProvider};
use crate::permissions;
use crate::storage::Repository;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex as AsyncMutex};
use tokio::time::{Duration, Instant};

pub type ToolFuture = Pin<Box<dyn Future<Output = CoreResult<ToolOutcome>> + Send>>;
pub type HostFuture = Pin<Box<dyn Future<Output = CoreResult<()>> + Send>>;
pub trait LocalFileTools: Send + Sync {
    fn definitions(
        &self,
        _repo: &Repository,
        _profile: ProfileId,
    ) -> CoreResult<Vec<ToolDefinition>> {
        Ok(vec![])
    }
    fn invoke(&self, conversation_id: ConversationId, call: ToolCall) -> ToolFuture;
}
pub trait PlatformRunHost: Send + Sync {
    fn set_active(&self, request: crate::model::RunHostRequest) -> HostFuture;
}
pub trait RunClock: Send + Sync {
    fn now(&self) -> Instant;
    fn sleep_until(&self, deadline: Instant) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}
pub struct SystemClock;
impl RunClock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn sleep_until(&self, deadline: Instant) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(tokio::time::sleep_until(deadline))
    }
}

pub type InferenceFuture = Pin<Box<dyn Future<Output = CoreResult<InferenceTurn>> + Send>>;
pub trait InferenceTransport: Send + Sync {
    fn stream(
        &self,
        request: InferenceRequest,
        events: mpsc::Sender<InferenceEvent>,
        cancel: Cancellation,
    ) -> InferenceFuture;
}
pub struct RealInference<P>(pub Arc<InferenceClient<P>>);
impl<P: CredentialProvider + 'static> InferenceTransport for RealInference<P> {
    fn stream(
        &self,
        request: InferenceRequest,
        events: mpsc::Sender<InferenceEvent>,
        cancel: Cancellation,
    ) -> InferenceFuture {
        let client = self.0.clone();
        Box::pin(async move { client.stream_turn(request, events, cancel).await })
    }
}

pub type McpConnectFuture =
    Pin<Box<dyn Future<Output = Result<Box<dyn McpConnection>, McpError>> + Send>>;
pub trait McpTransport: Send + Sync {
    fn connect(
        &self,
        profile: ProfileId,
        server: ServerConfig,
        repo: Repository,
        deadline: Instant,
        cancel: Cancellation,
    ) -> McpConnectFuture;
}
pub trait McpConnection: Send {
    fn list_tools(
        &mut self,
        deadline: Instant,
        cancel: Cancellation,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ToolDefinition>, McpError>> + Send + '_>>;
    fn call_tool(
        &mut self,
        call: ToolCall,
        deadline: Instant,
        cancel: Cancellation,
    ) -> Pin<Box<dyn Future<Output = Result<ToolOutcome, McpError>> + Send + '_>>;
    fn close(
        &mut self,
        deadline: Instant,
        cancel: Cancellation,
    ) -> Pin<Box<dyn Future<Output = Result<(), McpError>> + Send + '_>>;
}
pub struct RealMcp<P>(pub P);
impl<P: CredentialProvider + Clone + 'static> McpTransport for RealMcp<P> {
    fn connect(
        &self,
        profile: ProfileId,
        server: ServerConfig,
        repo: Repository,
        deadline: Instant,
        cancel: Cancellation,
    ) -> McpConnectFuture {
        let provider = self.0.clone();
        Box::pin(async move {
            let session = McpClient::new(provider)?
                .connect(profile, server, repo, deadline, cancel)
                .await?;
            Ok(Box::new(session) as Box<dyn McpConnection>)
        })
    }
}
impl<P: CredentialProvider + 'static> McpConnection for McpSession<P> {
    fn list_tools(
        &mut self,
        deadline: Instant,
        cancel: Cancellation,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ToolDefinition>, McpError>> + Send + '_>> {
        Box::pin(async move { Ok(self.list_tools(deadline, cancel).await?.tools) })
    }
    fn call_tool(
        &mut self,
        call: ToolCall,
        deadline: Instant,
        cancel: Cancellation,
    ) -> Pin<Box<dyn Future<Output = Result<ToolOutcome, McpError>> + Send + '_>> {
        Box::pin(async move { self.call_tool(&call, deadline, cancel).await })
    }
    fn close(
        &mut self,
        deadline: Instant,
        cancel: Cancellation,
    ) -> Pin<Box<dyn Future<Output = Result<(), McpError>> + Send + '_>> {
        Box::pin(async move { self.close(deadline, cancel).await })
    }
}

pub struct RuntimeConfig {
    pub repository: Repository,
}
impl RuntimeServices {
    pub fn with_transports<P: CredentialProvider + Clone + 'static>(
        credentials: P,
        host: Arc<dyn PlatformRunHost>,
        files: Arc<dyn LocalFileTools>,
        clock: Arc<dyn RunClock>,
    ) -> CoreResult<Self> {
        let inference = Arc::new(RealInference(Arc::new(InferenceClient::new(
            credentials.clone(),
        )?)));
        let mcp = Arc::new(RealMcp(credentials));
        Ok(Self {
            inference,
            mcp,
            host,
            files,
            clock,
        })
    }
}
pub struct RuntimeServices {
    pub inference: Arc<dyn InferenceTransport>,
    pub mcp: Arc<dyn McpTransport>,
    pub host: Arc<dyn PlatformRunHost>,
    pub files: Arc<dyn LocalFileTools>,
    pub clock: Arc<dyn RunClock>,
}
struct Execution {
    run: RunId,
    generation: uuid::Uuid,
    // Only the owning worker persists termination; callers cannot leave late DB jobs.
    termination: std::sync::atomic::AtomicU8,
    actions_open: std::sync::atomic::AtomicBool,
}
struct Running {
    execution: Arc<Execution>,
    cancel: Cancellation,
    done: tokio::sync::watch::Receiver<bool>,
}
struct Inner {
    repository: Repository,
    services: RuntimeServices,
    running: AsyncMutex<Option<Running>>,
    diagnostic: std::sync::atomic::AtomicBool,
    closed: std::sync::atomic::AtomicBool,
    poisoned: std::sync::atomic::AtomicBool,
    host_failed: std::sync::atomic::AtomicBool,
    owner: Cancellation,
    handoff: Arc<std::sync::Mutex<()>>,
    /// The running run's wait for its current request's first output.
    wait: std::sync::Mutex<Option<WaitState>>,
}
/// How long a running run has waited for its current request's first output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelWait {
    /// Since the request was sent.
    pub waited_ms: u64,
    /// Since the server last showed life: the request itself or a keepalive.
    pub silent_ms: u64,
    /// Keepalive comments received so far for this request.
    pub keepalives: u32,
}
#[derive(Clone, Copy)]
struct WaitState {
    run: RunId,
    sent: Instant,
    last: Instant,
    keepalives: u32,
}
pub struct Coordinator {
    inner: Arc<Inner>,
}

pub(crate) struct DiagnosticLease {
    inner: Arc<Inner>,
}
impl Drop for DiagnosticLease {
    fn drop(&mut self) {
        self.inner
            .diagnostic
            .store(false, std::sync::atomic::Ordering::Release);
    }
}
impl Drop for Coordinator {
    fn drop(&mut self) {
        self.inner.stop(&self.inner.owner);
    }
}
impl Inner {
    fn stop(&self, cancel: &Cancellation) {
        let _gate = self.handoff.lock().unwrap_or_else(|e| e.into_inner());
        cancel.cancel();
    }
    fn cleanup_result(&self) -> CoreResult<()> {
        if self.poisoned.load(std::sync::atomic::Ordering::Acquire) {
            return Err(CoreError::Storage);
        }
        if self.host_failed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(CoreError::UnavailableTransport);
        }
        Ok(())
    }
    fn admission(&self) -> CoreResult<()> {
        if self.poisoned.load(std::sync::atomic::Ordering::Acquire) {
            return Err(CoreError::Storage);
        }
        if self.closed.load(std::sync::atomic::Ordering::Acquire) || self.owner.is_cancelled() {
            return Err(CoreError::Conflict);
        }
        Ok(())
    }
    async fn persist<T: Send + 'static>(
        &self,
        operation: impl FnOnce(Repository) -> CoreResult<T> + Send + 'static,
    ) -> CoreResult<T> {
        let repo = self.repository.clone();
        let result = db(move || operation(repo)).await;
        self.poison_on_storage(result)
    }
    /// Like `persist`, but captures a failure note made by the blocking
    /// operation (async task-locals do not reach `spawn_blocking`).
    async fn persist_noted<T: Send + 'static>(
        &self,
        operation: impl FnOnce(Repository) -> CoreResult<T> + Send + 'static,
    ) -> (CoreResult<T>, Option<crate::failure::FailureDetail>) {
        let repo = self.repository.clone();
        let (result, detail) =
            match db(move || Ok(crate::failure::sync_scope(move || operation(repo)))).await {
                Ok(noted) => noted,
                Err(error) => (Err(error), None),
            };
        (self.poison_on_storage(result), detail)
    }
    fn poison_on_storage<T>(&self, result: CoreResult<T>) -> CoreResult<T> {
        if matches!(result, Err(CoreError::Storage)) {
            self.poisoned
                .store(true, std::sync::atomic::Ordering::Release);
        }
        result
    }
}
impl Inner {
    fn current_wait(&self) -> Option<WaitState> {
        *self.wait.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn set_wait(&self, wait: Option<WaitState>) {
        *self.wait.lock().unwrap_or_else(|e| e.into_inner()) = wait;
    }
    /// Clears `run`'s wait, leaving another run's alone.
    fn end_wait(&self, run: RunId) {
        let mut wait = self.wait.lock().unwrap_or_else(|e| e.into_inner());
        if wait.is_some_and(|w| w.run == run) {
            *wait = None;
        }
    }
    fn keepalive(&self, run: RunId) {
        let now = self.services.clock.now();
        if let Some(wait) = self
            .wait
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
            .filter(|w| w.run == run)
        {
            wait.last = now;
            wait.keepalives = wait.keepalives.saturating_add(1);
        }
    }
}
impl Coordinator {
    pub fn open(config: RuntimeConfig, services: RuntimeServices) -> CoreResult<Self> {
        config.repository.recover_interrupted()?;
        Ok(Self {
            inner: Arc::new(Inner {
                repository: config.repository,
                services,
                running: AsyncMutex::new(None),
                diagnostic: std::sync::atomic::AtomicBool::new(false),
                closed: std::sync::atomic::AtomicBool::new(false),
                poisoned: std::sync::atomic::AtomicBool::new(false),
                host_failed: std::sync::atomic::AtomicBool::new(false),
                owner: Cancellation::new(),
                handoff: Arc::new(std::sync::Mutex::new(())),
                wait: std::sync::Mutex::new(None),
            }),
        })
    }
    pub(crate) async fn diagnostic_lease(&self) -> CoreResult<DiagnosticLease> {
        let slot = self.inner.running.lock().await;
        self.inner.admission()?;
        if slot.is_some()
            || self
                .inner
                .diagnostic
                .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(CoreError::Conflict);
        }
        // Reserve under run admission, then release its short lock before any diagnostic I/O.
        self.inner
            .diagnostic
            .store(true, std::sync::atomic::Ordering::Release);
        drop(slot);
        Ok(DiagnosticLease {
            inner: self.inner.clone(),
        })
    }
    pub async fn start_turn(
        &self,
        conversation: ConversationId,
        text: String,
    ) -> CoreResult<RunId> {
        let run = RunId::new();
        let ready = {
            let mut slot = self.inner.running.lock().await;
            self.inner.admission()?;
            if slot.is_some()
                || self
                    .inner
                    .diagnostic
                    .load(std::sync::atomic::Ordering::Acquire)
            {
                return Err(CoreError::Conflict);
            }
            self.launch(&mut slot, run, move |repo| {
                // A rejected message must not record a new profile revision.
                state::validate_turn_text(&text)?;
                state::require_unarchived(&repo, conversation)?;
                repo.refresh_profile(conversation)?;
                state::start(&repo, run, conversation, text)
            })
        };
        prepared(ready).await?;
        Ok(run)
    }
    /// Re-runs a failed turn from its existing input message as a new run,
    /// without adding another user message. Only the conversation's latest
    /// run may be retried, and never one with a tool call whose outcome is
    /// unresolved; anything else is a Conflict.
    pub async fn retry(&self, failed: RunId) -> CoreResult<RunId> {
        let run = RunId::new();
        let ready = {
            let mut slot = self.inner.running.lock().await;
            self.inner.admission()?;
            if slot.is_some()
                || self
                    .inner
                    .diagnostic
                    .load(std::sync::atomic::Ordering::Acquire)
            {
                return Err(CoreError::Conflict);
            }
            self.launch(&mut slot, run, move |repo| {
                let conversation = state::retryable(&repo, failed)?;
                state::require_unarchived(&repo, conversation)?;
                // Fixed settings (URL, credential) apply to the retry.
                repo.refresh_profile(conversation)?;
                state::start_retry(&repo, run, failed)
            })
        };
        prepared(ready).await?;
        Ok(run)
    }
    pub async fn summarize_context(
        &self,
        conversation: ConversationId,
        selection: context::ContextSelection,
    ) -> CoreResult<RunId> {
        let run = RunId::new();
        let ready = {
            let mut slot = self.inner.running.lock().await;
            self.inner.admission()?;
            if slot.is_some()
                || self
                    .inner
                    .diagnostic
                    .load(std::sync::atomic::Ordering::Acquire)
            {
                return Err(CoreError::Conflict);
            }
            self.launch(&mut slot, run, move |repo| {
                state::require_unarchived(&repo, conversation)?;
                repo.refresh_profile(conversation)?;
                state::start_summary(&repo, run, conversation, selection)
            })
        };
        prepared(ready).await?;
        Ok(run)
    }
    pub async fn apply_context_selection(
        &self,
        run: RunId,
        selection: context::ContextSelection,
    ) -> CoreResult<()> {
        let slot = self.inner.running.lock().await;
        self.inner.admission()?;
        if slot.is_some() {
            return Err(CoreError::Conflict);
        }
        self.inner
            .persist(move |repo| state::apply_selection(&repo, run, selection))
            .await
    }
    pub async fn acknowledge_context_disclosure(
        &self,
        conversation: ConversationId,
        acknowledgment: context::DisclosureAcknowledgment,
    ) -> CoreResult<()> {
        self.inner.admission()?;
        self.inner
            .persist(move |repo| state::acknowledge_disclosure(&repo, conversation, acknowledgment))
            .await
    }
    fn launch(
        &self,
        slot: &mut Option<Running>,
        run: RunId,
        prepare: impl FnOnce(Repository) -> CoreResult<()> + Send + 'static,
    ) -> tokio::sync::oneshot::Receiver<Prepared> {
        let cancel = self.inner.owner.child_token();
        let child = cancel.clone();
        let (ready, receiver) = tokio::sync::oneshot::channel();
        let (done, finished) = tokio::sync::watch::channel(false);
        let inner = self.inner.clone();
        let execution = Arc::new(Execution {
            run,
            generation: uuid::Uuid::new_v4(),
            termination: std::sync::atomic::AtomicU8::new(0),
            actions_open: std::sync::atomic::AtomicBool::new(true),
        });
        let worker_execution = execution.clone();
        *slot = Some(Running {
            execution,
            cancel,
            done: finished,
        });
        tokio::spawn(async move {
            let prepared = inner.persist_noted(prepare).await;
            let success = prepared.0.is_ok();
            if !success {
                let mut slot = inner.running.lock().await;
                if slot
                    .as_ref()
                    .is_some_and(|r| Arc::ptr_eq(&r.execution, &worker_execution))
                {
                    slot.take();
                }
            }
            let _ = ready.send(prepared);
            if success {
                crate::failure::scope(execute(inner, worker_execution, child)).await;
            }
            let _ = done.send(true);
        });
        receiver
    }
    pub async fn approve(&self, binding: ApprovalBinding) -> CoreResult<()> {
        self.inner.admission()?;
        self.inner
            .persist(move |repo| permissions::approve(&repo, binding.attempt_id, &binding))
            .await
    }
    /// Store approvals and denials for one run, all or nothing (see
    /// `state::decide`). It does not resume the run.
    pub async fn decide(
        &self,
        run: RunId,
        approve: Vec<ApprovalBinding>,
        deny: Vec<AttemptId>,
    ) -> CoreResult<Decided> {
        self.inner.admission()?;
        self.inner
            .persist(move |repo| state::decide(&repo, run, approve, deny))
            .await
    }
    pub async fn deny(&self, attempt: AttemptId) -> CoreResult<()> {
        self.inner.admission()?;
        self.inner
            .persist(move |repo| state::deny(&repo, attempt).map(|_| ()))
            .await
    }
    pub async fn platform_pause(&self, run: RunId, generation: uuid::Uuid) -> CoreResult<()> {
        self.platform_action(run, generation, 1).await
    }
    pub async fn platform_stop(&self, run: RunId, generation: uuid::Uuid) -> CoreResult<()> {
        self.platform_action(run, generation, 2).await
    }
    async fn platform_action(
        &self,
        run: RunId,
        generation: uuid::Uuid,
        intent: u8,
    ) -> CoreResult<()> {
        let done = {
            let slot = self.inner.running.lock().await;
            let running = slot
                .as_ref()
                .filter(|r| r.execution.run == run && r.execution.generation == generation)
                .ok_or(CoreError::Conflict)?;
            let _handoff = self.inner.handoff.lock().unwrap_or_else(|e| e.into_inner());
            if !running
                .execution
                .actions_open
                .load(std::sync::atomic::Ordering::Acquire)
            {
                return Err(CoreError::Conflict);
            }
            running
                .execution
                .termination
                .store(intent, std::sync::atomic::Ordering::Release);
            running.cancel.cancel();
            running.done.clone()
        };
        wait_done(done).await;
        self.inner.cleanup_result()
    }
    pub async fn delete_conversation(
        &self,
        store: crate::files::artifacts::ArtifactStore,
        conversation: ConversationId,
        confirmed_unknown: bool,
    ) -> CoreResult<()> {
        let slot = self.inner.running.lock().await;
        self.inner.admission()?;
        self.inner.cleanup_result()?;
        if let Some(running) = slot.as_ref() {
            let run = running.execution.run;
            let repo = self.inner.repository.clone();
            if db(move || state::load(&repo, run)).await?.conversation == conversation {
                return Err(CoreError::Conflict);
            }
        }
        // Keep admission locked until the transactional active-state recheck and deletion commit.
        db(move || store.delete_conversation(conversation, confirmed_unknown)).await
    }
    /// Archives or restores `conversation`. Archiving follows deletion's rule:
    /// refused while one of its runs holds the execution slot, even after its
    /// stored state became terminal, or is stored as Running. Restoring needs
    /// no such check, since an archived conversation accepts no new work.
    pub async fn set_archived(
        &self,
        conversation: ConversationId,
        archived: bool,
    ) -> CoreResult<()> {
        let slot = self.inner.running.lock().await;
        self.inner.admission()?;
        if archived {
            self.inner.cleanup_result()?;
            if let Some(running) = slot.as_ref() {
                let run = running.execution.run;
                let repo = self.inner.repository.clone();
                if db(move || state::load(&repo, run)).await?.conversation == conversation {
                    return Err(CoreError::Conflict);
                }
            }
        }
        let repo = self.inner.repository.clone();
        // Keep admission locked until the transactional recheck commits, so no
        // turn can start between the check and the archive.
        db(move || repo.set_conversation_archived(conversation, archived)).await
    }
    pub async fn cancel(&self, run: RunId) -> CoreResult<()> {
        let done = {
            let slot = self.inner.running.lock().await;
            slot.as_ref().filter(|r| r.execution.run == run).map(|r| {
                self.inner.stop(&r.cancel);
                r.done.clone()
            })
        };
        let result = self
            .inner
            .persist(move |repo| state::cancel(&repo, run))
            .await;
        if let Some(done) = done {
            wait_done(done).await;
        }
        result?;
        self.inner.cleanup_result()
    }
    pub async fn resume(&self, run: RunId) -> CoreResult<()> {
        let ready = {
            let mut slot = self.inner.running.lock().await;
            self.inner.admission()?;
            if slot.is_some()
                || self
                    .inner
                    .diagnostic
                    .load(std::sync::atomic::Ordering::Acquire)
            {
                return Err(CoreError::Conflict);
            }
            self.launch(&mut slot, run, move |repo| {
                let data = state::load(&repo, run)?;
                if !matches!(
                    data.state,
                    RunState::WaitingApproval | RunState::Paused | RunState::Interrupted
                ) || data.attempts.iter().any(|a| {
                    matches!(
                        a.state,
                        AttemptState::Dispatching | AttemptState::OutcomeUnknown
                    )
                }) {
                    return Err(CoreError::Conflict);
                }
                state::require_unarchived(&repo, data.conversation)?;
                // Fixed settings (URL, credential) apply to the resumed run; the
                // guards above still keep unknown-outcome attempts from replaying.
                repo.refresh_profile(data.conversation)?;
                state::resume_on_current(&repo, run, data.state)
            })
        };
        prepared(ready).await
    }
    pub async fn context_snapshot(
        &self,
        conversation: ConversationId,
    ) -> CoreResult<ConversationSnapshot> {
        let repo = self.inner.repository.clone();
        db(move || repo.conversation(conversation)).await
    }
    pub async fn snapshot(&self) -> CoreResult<AppSnapshot> {
        let repo = self.inner.repository.clone();
        let mut runs = db(move || state::snapshot(&repo)).await?;
        if let Some(wait) = self.inner.current_wait() {
            let now = self.inner.services.clock.now();
            let since = |at: Instant| now.saturating_duration_since(at).as_millis() as u64;
            if let Some(run) = runs
                .iter_mut()
                .find(|r| r.id == wait.run && r.state == RunState::Running)
            {
                run.model_wait = Some(ModelWait {
                    waited_ms: since(wait.sent),
                    silent_ms: since(wait.last),
                    keepalives: wait.keepalives,
                });
            }
        }
        Ok(AppSnapshot { runs })
    }
    pub fn subscribe(&self, after_sequence: u64) -> EventSubscription {
        EventSubscription {
            repo: self.inner.repository.clone(),
            next: after_sequence,
        }
    }
    pub async fn shutdown(&self) -> CoreResult<()> {
        self.inner
            .closed
            .store(true, std::sync::atomic::Ordering::Release);
        self.inner.stop(&self.inner.owner);
        let done = {
            self.inner
                .running
                .lock()
                .await
                .as_ref()
                .map(|r| r.done.clone())
        };
        if let Some(done) = done {
            wait_done(done).await;
        }
        self.inner.cleanup_result()
    }
}
/// A launch's preparation result and the failure note it made, if any.
type Prepared = (CoreResult<()>, Option<crate::failure::FailureDetail>);
/// Awaits a launch's preparation and re-notes its failure detail in the
/// caller's failure scope.
async fn prepared(ready: tokio::sync::oneshot::Receiver<Prepared>) -> CoreResult<()> {
    let (result, detail) = ready.await.map_err(|_| CoreError::Storage)?;
    if result.is_err() {
        if let Some(detail) = detail {
            crate::failure::note(detail);
        }
    }
    result
}
async fn wait_done(mut done: tokio::sync::watch::Receiver<bool>) {
    while !*done.borrow_and_update() {
        if done.changed().await.is_err() {
            break;
        }
    }
}
pub struct AppSnapshot {
    pub runs: Vec<state::RunData>,
}
pub struct EventSubscription {
    repo: Repository,
    next: u64,
}
impl EventSubscription {
    pub async fn poll(&mut self) -> CoreResult<Vec<(u64, ConversationId, String)>> {
        let repo = self.repo.clone();
        let after = self.next;
        let events = db(move || state::events(&repo, after)).await?;
        if let Some((seq, _, _)) = events.last() {
            self.next = *seq;
        }
        Ok(events)
    }
}
fn mcp_error(error: McpError) -> CoreError {
    match error {
        McpError::Storage => CoreError::Storage,
        McpError::Conflict => CoreError::Conflict,
        McpError::Authentication => CoreError::Authentication,
        McpError::Limit => CoreError::Limit,
        McpError::Cancelled => CoreError::Cancelled,
        _ => CoreError::UnavailableTransport,
    }
}
async fn db<T: Send + 'static>(
    operation: impl FnOnce() -> CoreResult<T> + Send + 'static,
) -> CoreResult<T> {
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|_| CoreError::Storage)?
}

const HOST_START_TIMEOUT: Duration = Duration::from_secs(30);
const TEARDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const PARTIAL_BYTES: usize = 4096;
const CHECKPOINT_INTERVAL: Duration = Duration::from_millis(250);
/// The longest a request may wait for its first output. That wait does not
/// count against the run's time budget; the transport's idle timeout (reset
/// by keepalives) ends a silent wait much sooner.
pub const FIRST_OUTPUT_WAIT: Duration = Duration::from_secs(60 * 60);

// Serialize Stop with each local poll, including construction of the transport future.
// A request already handed to the remote server cannot be rolled back.
struct Handoff<'a, T> {
    future: Pin<Box<dyn Future<Output = T> + Send + 'a>>,
    gate: Arc<std::sync::Mutex<()>>,
    cancel: Cancellation,
}
impl<T> Future for Handoff<'_, T> {
    type Output = CoreResult<T>;
    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let gate = self.gate.clone();
        let _guard = gate.lock().unwrap_or_else(|e| e.into_inner());
        if self.cancel.is_cancelled() {
            return std::task::Poll::Ready(Err(CoreError::Cancelled));
        }
        self.future.as_mut().poll(cx).map(Ok)
    }
}
async fn dispatch<T: Send>(
    inner: &Inner,
    cancel: &Cancellation,
    deadline: Instant,
    future: impl Future<Output = T> + Send,
) -> CoreResult<T> {
    inner.admission()?;
    tokio::select! {biased;
        _=cancel.cancelled()=>Err(CoreError::Cancelled),
        _=inner.services.clock.sleep_until(deadline)=>Err(CoreError::Limit),
        result=(Handoff{future:Box::pin(future),gate:inner.handoff.clone(),cancel:cancel.clone()})=>result,
    }
}
/// The run's active time: banked milliseconds plus the open stretch, if any.
/// It is suspended while a request waits for its first output.
struct ActiveTime {
    state: std::sync::Mutex<(u64, Option<Instant>)>,
    clock: Arc<dyn RunClock>,
}
impl ActiveTime {
    fn new(banked: u64, clock: Arc<dyn RunClock>) -> Self {
        Self {
            state: std::sync::Mutex::new((banked, None)),
            clock,
        }
    }
    fn elapsed(&self, start: Instant) -> u64 {
        self.clock
            .now()
            .saturating_duration_since(start)
            .as_millis() as u64
    }
    fn total(&self) -> u64 {
        let (banked, started) = *self.state.lock().unwrap_or_else(|e| e.into_inner());
        banked.saturating_add(started.map(|start| self.elapsed(start)).unwrap_or(0))
    }
    /// Starts counting, unless already counting.
    fn begin(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.1.is_none() {
            state.1 = Some(self.clock.now());
        }
    }
    /// Stops counting, banking the open stretch.
    fn suspend(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(start) = state.1.take() {
            state.0 = state.0.saturating_add(self.elapsed(start));
        }
    }
    /// When `limit_ms` of active time will have been spent, if counting goes on.
    fn deadline(&self, limit_ms: u64) -> Instant {
        self.clock.now() + Duration::from_millis(limit_ms.saturating_sub(self.total()))
    }
    async fn save(&self, inner: &Inner, run: RunId) -> CoreResult<()> {
        let total = self.total();
        inner
            .persist(move |repo| state::account(&repo, run, total))
            .await
    }
}
#[derive(Default)]
struct PartialText {
    pending: String,
    total: usize,
}
impl PartialText {
    fn push(&mut self, text: String) -> CoreResult<()> {
        self.total = self.total.saturating_add(text.len());
        if self.total > crate::inference::MAX_TEXT_BYTES {
            return Err(CoreError::Limit);
        }
        self.pending.push_str(&text);
        Ok(())
    }
    async fn flush(&mut self, inner: &Inner, run: RunId) -> CoreResult<()> {
        if !self.pending.is_empty() {
            let delta = std::mem::take(&mut self.pending);
            inner
                .persist(move |repo| state::partial(&repo, run, delta))
                .await?;
        }
        Ok(())
    }
}
async fn execute(inner: Arc<Inner>, execution: Arc<Execution>, cancel: Cancellation) {
    let run = execution.run;
    let initial = inner.persist(move |repo| state::load(&repo, run)).await;
    let conversation = initial.as_ref().ok().map(|d| d.conversation);
    let budget = initial
        .as_ref()
        .ok()
        .map(|d| (d.profile.name.clone(), d.profile.limits.active_timeout_ms));
    let active = ActiveTime::new(
        initial.as_ref().map(|d| d.active_ms).unwrap_or(0),
        inner.services.clock.clone(),
    );
    let mut partial = PartialText::default();
    let mut sessions = HashMap::new();
    let result = match initial {
        Err(error) => Err(error),
        Ok(initial) => {
            let work = work(
                &inner,
                &execution,
                &cancel,
                initial,
                &active,
                &mut partial,
                &mut sessions,
            );
            tokio::pin!(work);
            loop {
                let tick = inner.services.clock.now() + CHECKPOINT_INTERVAL;
                tokio::select! {biased;
                    _=cancel.cancelled()=>break Err(CoreError::Cancelled),
                    result=&mut work=>break result,
                    _=inner.services.clock.sleep_until(tick)=>{if let Err(error)=active.save(&inner,run).await {break Err(error);}},
                }
            }
        }
    };
    // A dropped wait leaves active time suspended; teardown is charged.
    inner.end_wait(run);
    active.begin();
    // Drop local I/O futures before cleanup; already-sent remote work may continue.
    let flush = partial.flush(&inner, run).await;
    let close = async {
        if inner.poisoned.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        for session in sessions.values_mut() {
            let _ = session
                .close(
                    inner.services.clock.now() + TEARDOWN_TIMEOUT,
                    Cancellation::new(),
                )
                .await;
        }
    };
    tokio::select! {biased; _=inner.services.clock.sleep_until(inner.services.clock.now()+TEARDOWN_TIMEOUT)=>{}, _=close=>{}}
    drop(sessions);
    let intent = {
        let _handoff = inner.handoff.lock().unwrap_or_else(|e| e.into_inner());
        // Close action admission before sampling intent without delaying teardown on the slot lock.
        execution
            .actions_open
            .store(false, std::sync::atomic::Ordering::Release);
        execution
            .termination
            .load(std::sync::atomic::Ordering::Acquire)
    };
    let status = match intent {
        1 => RunState::Paused,
        2 => RunState::Cancelled,
        _ => match &result {
            Ok(status) => *status,
            Err(CoreError::Cancelled) if !inner.owner.is_cancelled() => RunState::Cancelled,
            Err(CoreError::Cancelled) => RunState::Interrupted,
            Err(CoreError::UnavailableTransport) => RunState::Paused,
            Err(_) => RunState::Failed,
        },
    };
    // Teardown above runs in nested scopes, so this is the run's own last note;
    // a note is only meaningful when the run itself failed.
    let detail = crate::failure::take()
        .or_else(|| {
            // Every other limit notes itself; one without a note is the
            // run's time budget running out.
            let (profile, limit_ms) = budget.as_ref()?;
            (matches!(result, Err(CoreError::Limit)) && active.total() >= *limit_ms)
                .then(|| run_time_limit(profile, *limit_ms))
        })
        .filter(|_| result.is_err());
    let error = result
        .map_err(|code| crate::failure::promote(code, detail.as_ref()))
        .err()
        .or(flush.err());
    let total = active.total();
    let settled = inner
        .persist(move |repo| state::settle(&repo, run, status, error, detail, total))
        .await;
    let host = async {
        if let Some(conversation) = conversation {
            inner
                .services
                .host
                .set_active(crate::model::RunHostRequest {
                    run_id: run.0.to_string(),
                    generation: execution.generation.to_string(),
                    conversation_id: conversation.0.to_string(),
                    active: false,
                    waiting_approval: settled.is_ok()
                        && status == RunState::WaitingApproval
                        && !cancel.is_cancelled(),
                })
                .await
        } else {
            Ok(())
        }
    };
    let host_result = tokio::select! {biased; _=inner.services.clock.sleep_until(inner.services.clock.now()+TEARDOWN_TIMEOUT)=>Err(CoreError::UnavailableTransport), result=host=>result};
    if let Err(error) = host_result {
        inner
            .host_failed
            .store(true, std::sync::atomic::Ordering::Release);
        inner
            .closed
            .store(true, std::sync::atomic::Ordering::Release);
        let uncertain = if status == RunState::Interrupted {
            status
        } else {
            RunState::Paused
        };
        let total = active.total();
        let _ = inner
            .persist(move |repo| {
                state::settle_keeping_failure(&repo, run, uncertain, Some(error), total)
            })
            .await;
    }
    let _ = active.save(&inner, run).await;
    let mut slot = inner.running.lock().await;
    if slot
        .as_ref()
        .is_some_and(|r| Arc::ptr_eq(&r.execution, &execution))
    {
        slot.take();
    }
}
async fn work(
    inner: &Inner,
    execution: &Execution,
    cancel: &Cancellation,
    initial: state::RunData,
    active: &ActiveTime,
    partial: &mut PartialText,
    sessions: &mut HashMap<ServerId, Box<dyn McpConnection>>,
) -> CoreResult<RunState> {
    let run = execution.run;
    if initial.attempts.iter().any(|a| {
        matches!(
            a.state,
            AttemptState::Dispatching | AttemptState::OutcomeUnknown
        )
    }) {
        return Err(CoreError::Conflict);
    }
    active.begin();
    let limit_ms = initial.profile.limits.active_timeout_ms;
    if active.total() >= limit_ms {
        return Err(CoreError::Limit);
    }
    let mut deadline = active.deadline(limit_ms);
    let startup = dispatch(
        inner,
        cancel,
        deadline.min(inner.services.clock.now() + HOST_START_TIMEOUT),
        async {
            inner
                .services
                .host
                .set_active(crate::model::RunHostRequest {
                    run_id: run.0.to_string(),
                    generation: execution.generation.to_string(),
                    conversation_id: initial.conversation.0.to_string(),
                    active: true,
                    waiting_approval: false,
                })
                .await
        },
    )
    .await;
    match startup {
        Ok(Ok(())) => {}
        Err(CoreError::Cancelled) => return Err(CoreError::Cancelled),
        _ => return Ok(RunState::Paused),
    }
    let mut definitions = HashMap::new();
    if initial.profile.mode == ProfileMode::Agent
        && initial.context.purpose != context::ContextPurpose::Summary
    {
        let files = inner.services.files.clone();
        let profile = initial.profile.profile_id;
        for definition in inner
            .persist(move |repo| files.definitions(&repo, profile))
            .await?
        {
            definitions.insert(definition.key.clone(), definition);
        }
    }
    let servers = if initial.profile.mode == ProfileMode::ChatOnly
        || initial.context.purpose == context::ContextPurpose::Summary
    {
        vec![]
    } else {
        initial.profile.servers
    };
    for server in servers {
        let mut session = dispatch(inner, cancel, deadline, async {
            inner
                .services
                .mcp
                .connect(
                    initial.profile.profile_id,
                    server.clone(),
                    inner.repository.clone(),
                    deadline,
                    cancel.clone(),
                )
                .await
        })
        .await?
        .map_err(mcp_error)?;
        let discovered = dispatch(inner, cancel, deadline, async {
            session.list_tools(deadline, cancel.clone()).await
        })
        .await?
        .map_err(mcp_error)?;
        for definition in discovered {
            definitions.insert(definition.key.clone(), definition);
        }
        sessions.insert(server.id, session);
    }
    loop {
        if cancel.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        // Waiting for a first output is not charged, so the deadline moves.
        deadline = active.deadline(limit_ms);
        if inner.services.clock.now() >= deadline {
            return Err(CoreError::Limit);
        }
        active.save(inner, run).await?;
        let data = inner.persist(move |repo| state::load(&repo, run)).await?;
        if data.state != RunState::Running {
            return Err(CoreError::Cancelled);
        }
        if data.attempts.iter().any(|a| {
            matches!(
                a.state,
                AttemptState::Dispatching | AttemptState::OutcomeUnknown
            )
        }) {
            return Err(CoreError::Conflict);
        }
        if let Some(attempt) = data
            .attempts
            .iter()
            .find(|a| matches!(a.state, AttemptState::Proposed | AttemptState::Approved))
        {
            let id = attempt.id;
            let call = attempt.call.clone();
            let definition = definitions
                .get(&call.key)
                .ok_or(CoreError::Conflict)?
                .clone();
            let binding = attempt.binding.clone();
            let authorization = inner
                .persist(move |repo| {
                    permissions::authorize(&repo, id, &call, &definition, binding.as_ref())
                })
                .await?;
            match authorization {
                Authorization::Ask(_) => {
                    // Bind every later call that will also ask, so all of this
                    // turn's decisions can be made at once. Binding never
                    // changes an attempt's state or dispatches.
                    let pending: Vec<_> = data
                        .attempts
                        .iter()
                        .skip_while(|a| a.id != id)
                        .skip(1)
                        .filter(|a| a.state == AttemptState::Proposed && a.binding.is_none())
                        .filter_map(|a| {
                            let definition = definitions.get(&a.call.key)?.clone();
                            Some((a.id, a.call.clone(), definition))
                        })
                        .collect();
                    if !pending.is_empty() {
                        inner
                            .persist(move |repo| {
                                pending.iter().try_for_each(|(id, call, definition)| {
                                    permissions::prepare_approval(&repo, *id, call, definition)
                                })
                            })
                            .await?;
                    }
                    return Ok(RunState::WaitingApproval);
                }
                Authorization::Block => {
                    inner
                        .persist(move |repo| {
                            state::result(
                                &repo,
                                run,
                                id,
                                AttemptState::Denied,
                                Some("Tool blocked by policy".into()),
                                0,
                            )
                        })
                        .await?;
                }
                Authorization::Allow => {
                    let call = attempt.call.clone();
                    let response = dispatch(inner, cancel, deadline, async {
                        match call.key.server {
                            ToolServer::Mcp(server) => {
                                sessions
                                    .get_mut(&server)
                                    .ok_or(McpError::Conflict)?
                                    .call_tool(call, deadline, cancel.clone())
                                    .await
                            }
                            ToolServer::LocalFiles => inner
                                .services
                                .files
                                .invoke(data.conversation, call)
                                .await
                                .map_err(McpError::from),
                        }
                    })
                    .await;
                    let (status, content) = match response {
                        Ok(Ok(outcome)) => {
                            let status = if outcome.is_error {
                                AttemptState::Failed
                            } else {
                                AttemptState::Succeeded
                            };
                            let content = outcome.envelope_json();
                            let content = if content.len() > crate::mcp::MAX_RPC_BYTES {
                                format!(
                                    "Tool {}; payload too large to retain",
                                    if outcome.is_error {
                                        "failed"
                                    } else {
                                        "succeeded"
                                    }
                                )
                            } else {
                                content
                            };
                            (status, Some(content))
                        }
                        Ok(Err(McpError::RemoteProtocol(code))) => (
                            AttemptState::Failed,
                            Some(format!("Remote protocol error {code}")),
                        ),
                        Ok(Err(
                            McpError::InvalidInput
                            | McpError::UnsupportedTransport
                            | McpError::UnsupportedVersion
                            | McpError::Authentication
                            | McpError::Conflict,
                        )) => (AttemptState::Failed, Some("Tool dispatch rejected".into())),
                        _ => (AttemptState::OutcomeUnknown, None),
                    };
                    inner
                        .persist(move |repo| state::result(&repo, run, id, status, content, 0))
                        .await?;
                    if status == AttemptState::OutcomeUnknown {
                        return Err(CoreError::Conflict);
                    }
                    // The run continues past this tool failure; drop its note so it
                    // cannot describe a later, unrelated failure.
                    let _ = crate::failure::take();
                }
            }
            continue;
        }
        let purpose = data.context.purpose;
        let profile = data.profile.clone();
        let (inference_count, attempt_count) = (data.inference_count, data.attempt_count);
        let limited = data.profile.clone();
        let conversation = data.conversation;
        let (snapshot, disclosure, selection, calibration) = inner
            .persist(move |repo| {
                let mut snapshot = repo.conversation(conversation)?;
                snapshot.profile = profile;
                let disclosure = state::disclosure(&repo, &snapshot.profile)?;
                let calibration = state::latest_calibration(&repo, conversation)?;
                let mut selection = data.context.selection;
                if purpose == context::ContextPurpose::Turn {
                    if let Some(selection) = &mut selection {
                        let through = data
                            .context
                            .selection_through_ordinal
                            .ok_or(CoreError::Storage)?;
                        for id in state::messages_after(&repo, conversation, through)? {
                            if !selection.message_ids.contains(&id) {
                                selection.message_ids.push(id);
                            }
                        }
                    }
                }
                Ok((snapshot, disclosure, selection, calibration))
            })
            .await?;
        let tools: Vec<_> = definitions.values().cloned().collect();
        let (decision, usage) = context::assemble_with_usage(
            &snapshot,
            &tools,
            &context::ContextRequest {
                selection,
                attachments: vec![],
                purpose,
                disclosure,
                calibration,
                // The request may outlive the run deadline by its wait for a
                // first output, which `stream` bounds and does not charge.
                deadline: deadline + FIRST_OUTPUT_WAIT,
            },
        )?;
        let request = match decision {
            context::ContextDecision::Ready(request) => request,
            context::ContextDecision::NeedsUserChoice(usage) => {
                inner
                    .persist(move |repo| state::context_usage(&repo, run, usage))
                    .await?;
                return Ok(RunState::Paused);
            }
        };
        let saved_usage = usage.clone();
        inner
            .persist(move |repo| state::context_usage(&repo, run, saved_usage))
            .await?;
        inner
            .persist(move |repo| state::inference_started(&repo, run, 0))
            .await
            .inspect_err(|error| {
                let max = limited.limits.max_inference_requests;
                if *error == CoreError::Limit && inference_count >= max {
                    profile_limit(
                        &limited,
                        format!("Inference request limit ({max} per run) reached"),
                    );
                }
            })?;
        *partial = PartialText::default();
        let streamed = stream(inner, run, cancel, request, partial, active, limit_ms).await;
        let turn = match streamed {
            Err(CoreError::ContextLength) => {
                let mut usage = usage;
                usage.reason = Some(context::ContextChoiceReason::ProviderRejected);
                inner
                    .persist(move |repo| state::context_usage(&repo, run, usage))
                    .await?;
                return Ok(RunState::Paused);
            }
            other => other?,
        };
        let mut usage = usage;
        usage.provider = turn.usage.as_ref().map(|u| context::ProviderUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
        });
        inner
            .persist(move |repo| state::context_usage(&repo, run, usage))
            .await?;
        validate_batch(&turn, &definitions)?;
        if purpose == context::ContextPurpose::Summary {
            inner
                .persist(move |repo| {
                    state::complete_summary(&repo, run, turn.text, turn.finish_reason == "stop")
                })
                .await?;
            return Ok(RunState::Completed);
        }
        let has_calls = !turn.calls.is_empty();
        let proposed = turn.calls.len();
        inner
            .persist(move |repo| state::assistant_and_propose(&repo, run, turn.text, turn.calls, 0))
            .await
            .inspect_err(|error| {
                let max = limited.limits.max_tool_attempts;
                if *error == CoreError::Limit
                    && (attempt_count as usize).saturating_add(proposed) > max as usize
                {
                    profile_limit(
                        &limited,
                        format!(
                            "Tool call limit ({max} per run) reached: the model asked for {proposed} more after {attempt_count}"
                        ),
                    );
                }
            })?;
        if !has_calls {
            return Ok(RunState::Completed);
        }
    }
}
fn validate_batch(
    turn: &InferenceTurn,
    definitions: &HashMap<ToolKey, ToolDefinition>,
) -> CoreResult<()> {
    let mut ids = std::collections::HashSet::new();
    if turn.text.len() > crate::inference::MAX_TEXT_BYTES
        || turn.calls.len() > crate::inference::MAX_TOOLS
    {
        return Err(CoreError::Limit);
    }
    for call in &turn.calls {
        if call.provider_call_id.is_empty()
            || call.provider_call_id.len() > 256
            || !ids.insert(&call.provider_call_id)
        {
            return Err(CoreError::InvalidInput);
        }
        permissions::validate_call(
            call,
            definitions.get(&call.key).ok_or(CoreError::InvalidInput)?,
        )?;
    }
    Ok(())
}
/// Notes that the profile's limit named in `reason` stopped the run.
fn profile_limit(profile: &ProfileSnapshot, reason: String) {
    crate::failure::note(crate::failure::FailureDetail {
        target: crate::failure::FailureTarget::Profile {
            name: profile.name.clone(),
        },
        step: crate::failure::FailureStep::RunLimit,
        url: None,
        status: None,
        reason: Some(reason),
    });
}
/// The note for a run whose active-time budget ran out.
fn run_time_limit(profile: &str, limit_ms: u64) -> crate::failure::FailureDetail {
    crate::failure::FailureDetail {
        target: crate::failure::FailureTarget::Profile {
            name: profile.to_owned(),
        },
        step: crate::failure::FailureStep::RunLimit,
        url: None,
        status: None,
        reason: Some(format!(
            "Run time limit ({}) reached; time waiting for the model's first output is not counted",
            duration_text(limit_ms)
        )),
    }
}
/// A short duration such as `15 min`, `14 min 45 s` or `1 h`.
pub(crate) fn duration_text(ms: u64) -> String {
    let seconds = ms / 1000;
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    let parts: Vec<String> = [(h, "h"), (m, "min"), (s, "s")]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, unit)| format!("{n} {unit}"))
        .collect();
    if parts.is_empty() {
        format!("{ms} ms")
    } else {
        parts.join(" ")
    }
}
/// Streams one reply. Until its first output the run's active time is
/// suspended and the wait is shown as [`ModelWait`]; [`FIRST_OUTPUT_WAIT`]
/// bounds it. From the first output the run's time budget applies again.
async fn stream(
    inner: &Inner,
    run: RunId,
    cancel: &Cancellation,
    request: InferenceRequest,
    partial: &mut PartialText,
    active: &ActiveTime,
    limit_ms: u64,
) -> CoreResult<InferenceTurn> {
    let sent = inner.services.clock.now();
    active.suspend();
    inner.set_wait(Some(WaitState {
        run,
        sent,
        last: sent,
        keepalives: 0,
    }));
    let result = stream_reply(inner, run, cancel, request, partial, active, limit_ms, sent).await;
    inner.end_wait(run);
    active.begin();
    result
}
#[allow(clippy::too_many_arguments)] // The run, its budget and the request's own timing.
async fn stream_reply(
    inner: &Inner,
    run: RunId,
    cancel: &Cancellation,
    request: InferenceRequest,
    partial: &mut PartialText,
    active: &ActiveTime,
    limit_ms: u64,
    sent: Instant,
) -> CoreResult<InferenceTurn> {
    inner
        .persist(move |repo| state::signal(&repo, run, "waiting"))
        .await?;
    let endpoint = request.endpoint.name.clone();
    let first_output_by = sent + FIRST_OUTPUT_WAIT;
    // The run's deadline, once output has started.
    let mut deadline: Option<Instant> = None;
    let started = |deadline: &mut Option<Instant>| -> bool {
        if deadline.is_some() {
            return false;
        }
        inner.end_wait(run);
        active.begin();
        *deadline = Some(active.deadline(limit_ms));
        true
    };
    let (sender, mut receiver) = mpsc::channel(crate::inference::EVENT_CHANNEL_CAPACITY);
    let inference = dispatch(inner, cancel, request.deadline, async {
        inner
            .services
            .inference
            .stream(request, sender, cancel.clone())
            .await
    });
    tokio::pin!(inference);
    let mut finished = None;
    let mut open = true;
    let mut flush_at = inner.services.clock.now() + CHECKPOINT_INTERVAL;
    loop {
        if finished.is_some() && !open {
            break;
        }
        let mut output = false;
        tokio::select! {biased;
            _=cancel.cancelled()=>return Err(CoreError::Cancelled),
            _=inner.services.clock.sleep_until(deadline.unwrap_or(first_output_by))=>{
                if deadline.is_none() {
                    crate::failure::note(crate::failure::FailureDetail {
                        target: crate::failure::FailureTarget::Inference { name: endpoint },
                        step: crate::failure::FailureStep::ChatRequest,
                        url: None,
                        status: None,
                        reason: Some(format!(
                            "no output from the model within {} of sending the request",
                            duration_text(FIRST_OUTPUT_WAIT.as_millis() as u64)
                        )),
                    });
                }
                return Err(CoreError::Limit);
            },
            result=&mut inference,if finished.is_none()=>{finished=Some(result);receiver.close();},
            event=receiver.recv(),if open=>match event {
                Some(InferenceEvent::Text(text))=>{output=started(&mut deadline);partial.push(text)?;if partial.pending.len()>=PARTIAL_BYTES {partial.flush(inner,run).await?;flush_at=inner.services.clock.now()+CHECKPOINT_INTERVAL;}},
                Some(InferenceEvent::Output)=>output=started(&mut deadline),
                Some(InferenceEvent::Alive)=>if deadline.is_none() {
                    inner.keepalive(run);
                    inner.persist(move |repo| state::signal(&repo, run, "alive")).await?;
                },
                Some(InferenceEvent::WarningChatOnly)=>{},None=>open=false,
            },
            _=inner.services.clock.sleep_until(flush_at)=>{partial.flush(inner,run).await?;flush_at=inner.services.clock.now()+CHECKPOINT_INTERVAL;},
        }
        if output {
            inner
                .persist(move |repo| state::signal(&repo, run, "output"))
                .await?;
        }
    }
    partial.flush(inner, run).await?;
    finished.ok_or(CoreError::Protocol)??
}
#[cfg(test)]
mod tests;
