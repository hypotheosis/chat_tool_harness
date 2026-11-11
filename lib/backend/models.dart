part 'artifact_models.dart';
part 'history_models.dart';

enum FailureCode {
  invalidInput,
  configuration,
  authentication,
  credentialMissing,
  rateLimited,
  toolUnsupported,
  discoveryUnavailable,
  contextLength,
  unavailableTransport,
  protocol,
  limit,
  storage,
  cancelled,
  conflict,

  /// This platform does not provide the feature (for example, saving or
  /// sharing files on Linux). Dart-only: the backend never reports it.
  unsupportedPlatform,
}

/// What failed and where, as noted by the backend. Values are sanitized.
class FailureDetail {
  const FailureDetail({
    required this.targetKind,
    this.targetName,
    required this.step,
    this.url,
    this.status,
    this.reason,
  });

  /// `inference`, `mcpServer`, `profile` or `credential`.
  final String targetKind;
  final String? targetName;

  /// The failed step in lowerCamelCase, for example `discoverModels`.
  final String step;
  final String? url;
  final int? status;
  final String? reason;
}

class BackendFailure implements Exception {
  const BackendFailure(this.code, {this.detail});
  final FailureCode code;
  final FailureDetail? detail;
  factory BackendFailure.fromNative(Object _) =>
      const BackendFailure(FailureCode.unavailableTransport);
  @override
  String toString() => "Backend failure: ${code.name}";
}

enum RunState {
  running,
  waitingApproval,
  paused,
  completed,
  cancelled,
  failed,
  interrupted,
}

enum AttemptState {
  proposed,
  denied,
  approved,
  dispatching,
  succeeded,
  failed,
  outcomeUnknown,
}

enum PermissionRule { ask, allow, block }

enum ProfileMode { agent, chatOnly }

enum ToolSupport { supported, unsupported, inconclusive }

enum ContextPurpose { turn, summary }

enum TokenEstimation { conservativeUtf8Bytes, calibratedUtf8Bytes }

enum ContextChoiceReason {
  windowExceeded,
  rawLimit,
  invalidHistory,
  disclosureRequired,
  providerRejected,
}

class EndpointConfig {
  const EndpointConfig({
    required this.id,
    required this.name,
    required this.baseUrl,
    required this.model,
    required this.contextTokens,
    required this.maxOutputTokens,
    required this.temperature,
    required this.allowHttp,
    this.credential,
  });
  final String id;
  final String name;
  final String baseUrl;
  final String model;
  final int contextTokens;
  final int maxOutputTokens;
  final double temperature;
  final bool allowHttp;
  final String? credential;
}

class ServerConfig {
  const ServerConfig({
    required this.id,
    required this.name,
    required this.url,
    required this.allowHttp,
    this.secretHeaders = const [],
    this.trusted = false,
  });
  final String id;
  final String name;
  final String url;
  final bool allowHttp;
  final List<SecretHeader> secretHeaders;

  /// Whether every tool of this server runs without asking (Block rules still
  /// apply). Read from snapshots; saving a server never changes it, use
  /// [SetServerTrust].
  final bool trusted;
}

class SecretHeader {
  const SecretHeader({required this.name, required this.credential});
  final String name;
  final String credential;
}

class RunLimits {
  const RunLimits({
    this.maxInferenceRequests = 20,
    this.maxToolAttempts = 40,
    this.activeTimeoutMs = 900000,
  });
  final int maxInferenceRequests;
  final int maxToolAttempts;
  final int activeTimeoutMs;
}

class Profile {
  final ProfileMode mode;
  const Profile({
    this.mode = ProfileMode.agent,
    required this.id,
    required this.name,
    required this.endpointId,
    required this.serverIds,
    required this.instructions,
    required this.limits,
  });
  final String id;
  final String name;
  final String endpointId;
  final List<String> serverIds;
  final String instructions;
  final RunLimits limits;
}

class ProfileSnapshot {
  /// Normalized `scheme://host[:port]` of the inference endpoint, or null when
  /// the endpoint is misconfigured.
  final String? inferenceOrigin;
  final ProfileMode mode;
  const ProfileSnapshot({
    this.inferenceOrigin,
    this.mode = ProfileMode.agent,
    required this.profileId,
    required this.revision,
    required this.name,
    required this.endpoint,
    required this.servers,
    required this.instructions,
    required this.limits,
  });
  final String profileId;
  final int revision;
  final String name;
  final EndpointConfig endpoint;
  final List<ServerConfig> servers;
  final String instructions;
  final RunLimits limits;
}

class ToolKey {
  const ToolKey({required this.profileId, this.serverId, required this.name});
  final String profileId;
  final String? serverId;
  final String name;
}

class ApprovalBinding {
  const ApprovalBinding({
    required this.attemptId,
    required this.key,
    required this.providerCallId,
    required this.definitionDigest,
    required this.argumentsDigest,
    required this.argumentsJson,
    required this.schemaJson,
    required this.destinationDigest,
  });
  final String attemptId;
  final ToolKey key;
  final String providerCallId;
  final String definitionDigest;
  final String argumentsDigest;
  final String argumentsJson;
  final String schemaJson;
  final String destinationDigest;
}

class ToolCall {
  const ToolCall({
    required this.key,
    required this.providerCallId,
    required this.argumentsJson,
    this.sourceEnvelope,
  });
  final ToolKey key;
  final String providerCallId;
  final String argumentsJson;
  final String? sourceEnvelope;
}

class Message {
  const Message({
    required this.id,
    required this.role,
    required this.content,
    required this.calls,
    this.callId,
    this.sourceEnvelope,
  });
  final String id;
  final String role;
  final String content;
  final List<ToolCall> calls;
  final String? callId;
  final String? sourceEnvelope;
}

class ContextSelection {
  const ContextSelection({required this.messageIds, required this.summaryIds});
  final List<String> messageIds;
  final List<String> summaryIds;
}

class PageReference {
  const PageReference({required this.attachmentId, required this.physicalPage});
  final String attachmentId;
  final int physicalPage;
}

class DerivedContext {
  const DerivedContext({
    required this.id,
    required this.runId,
    required this.text,
    required this.selection,
    required this.pageReferences,
    required this.partialCoverage,
    required this.unresolvedAttempts,
  });
  final String id;
  final String runId;
  final String text;
  final ContextSelection selection;
  final List<PageReference> pageReferences;
  final bool partialCoverage;
  final List<String> unresolvedAttempts;
}

class ProviderUsage {
  const ProviderUsage({
    required this.promptTokens,
    required this.completionTokens,
  });
  final int promptTokens;
  final int completionTokens;
}

class ContextUsage {
  const ContextUsage({
    required this.estimation,
    required this.estimatedInputTokens,
    required this.outputReserve,
    required this.window,
    required this.rawBytes,
    required this.rawBytesIsLowerBound,
    this.provider,
    this.reason,
  });
  final TokenEstimation estimation;
  final int estimatedInputTokens;
  final int outputReserve;
  final int window;
  final int rawBytes;
  final bool rawBytesIsLowerBound;
  final ProviderUsage? provider;
  final ContextChoiceReason? reason;
}

class RunContext {
  const RunContext({
    required this.purpose,
    this.selection,
    this.selectionThroughOrdinal,
    this.usage,
    this.derivedEntry,
    this.summary,
  });
  final ContextPurpose purpose;
  final ContextSelection? selection;
  final int? selectionThroughOrdinal;
  final ContextUsage? usage;
  final String? derivedEntry;
  final DerivedContext? summary;
}

class Attempt {
  const Attempt({
    required this.id,
    required this.state,
    required this.call,
    this.binding,
  });
  final String id;
  final AttemptState state;
  final ToolCall call;
  final ApprovalBinding? binding;
}

class Run {
  const Run({
    required this.id,
    required this.conversationId,
    required this.profile,
    required this.state,
    required this.inferenceCount,
    required this.attemptCount,
    required this.activeMs,
    required this.partialText,
    this.failure,
    this.failureCode,
    this.failureDetail,
    required this.attempts,
    required this.context,
    this.inputMessageId,
  });
  final String id;
  final String conversationId;
  final ProfileSnapshot profile;
  final RunState state;
  final int inferenceCount;
  final int attemptCount;
  final int activeMs;
  final String partialText;
  final String? failure;
  final FailureCode? failureCode;
  final FailureDetail? failureDetail;
  final List<Attempt> attempts;
  final RunContext context;

  /// The user message that started a turn run; null for summary runs.
  final String? inputMessageId;
}

class Conversation {
  const Conversation({
    required this.id,
    required this.profile,
    required this.messages,
    required this.summaries,
    this.originAcknowledged = false,
  });
  final String id;
  final ProfileSnapshot profile;
  final List<Message> messages;
  final List<DerivedContext> summaries;

  /// Whether disclosure was acknowledged, app-wide, for `profile.inferenceOrigin`.
  final bool originAcknowledged;
}

class ToolRule {
  const ToolRule({
    required this.key,
    required this.schemaJson,
    required this.schemaDigest,
    required this.destinationDigest,
    required this.rule,
  });
  final ToolKey key;
  final String schemaJson;
  final String schemaDigest;
  final String destinationDigest;
  final PermissionRule rule;
}

class AppSnapshot {
  const AppSnapshot({
    this.sequence = 0,
    this.endpoints = const [],
    this.servers = const [],
    this.profiles = const [],
    this.conversations = const [],
    this.runs = const [],
    this.rules = const [],
    this.artifacts = const [],
    this.retentionLimitBytes = 268435456,
    this.connectionTests = const [],
  });
  final int sequence;
  final List<EndpointConfig> endpoints;
  final List<ServerConfig> servers;
  final List<Profile> profiles;
  final List<Conversation> conversations;
  final List<Run> runs;
  final List<ToolRule> rules;
  final List<ArtifactSummary> artifacts;
  final int retentionLimitBytes;

  /// The last result of each connection test.
  final List<ConnectionTest> connectionTests;
}

enum ConnectionTestKind { discoverModels, probeTools, discoverServer }

/// The last finished connection test of one endpoint and kind, or of one MCP
/// server, kept by the backend until the next test replaces it.
class ConnectionTest {
  const ConnectionTest({
    required this.kind,
    this.endpointId,
    this.serverId,
    this.profileId,
    required this.testedAt,
    this.failure,
    this.modelCount,
    this.toolSupport,
    this.toolCount,
    this.skippedTools = const [],
  });
  final ConnectionTestKind kind;
  final String? endpointId;
  final String? serverId;

  /// The profile a server was tested through.
  final String? profileId;
  final DateTime testedAt;

  /// Set when the test failed; the outcome fields are then empty.
  final BackendFailure? failure;
  final int? modelCount;
  final ToolSupport? toolSupport;
  final int? toolCount;
  final List<SkippedTool> skippedTools;
}

class AppEvent {
  const AppEvent({required this.sequence, required this.conversationId});
  final int sequence;
  final String conversationId;
}

class SkippedTool {
  const SkippedTool(this.name, this.reason);
  final String name;
  final String reason;
}

/// The result line of a server tool discovery, e.g. "3 tools available;
/// 1 skipped (see below)".
String discoverySummary(int available, int skipped) {
  final tools = available == 1 ? '1 tool' : '$available tools';
  return skipped == 0
      ? '$tools available'
      : '$tools available; $skipped skipped (see below)';
}

class CommandReply {
  const CommandReply({
    this.history = const [],
    this.exportBundle,
    this.id,
    this.artifact,
    this.models = const [],
    this.toolSupport,
    this.discoveredTools = const [],
    this.skippedTools = const [],
    this.discoveredRules = const [],
  });
  final List<ConversationSummary> history;
  final ExportBundle? exportBundle;
  final List<String> models;
  final ToolSupport? toolSupport;
  final List<String> discoveredTools;

  /// Tools the server offered that cannot be used, each with the reason.
  final List<SkippedTool> skippedTools;
  final List<ToolRule> discoveredRules;
  final String? id;
  final ArtifactContent? artifact;
}

class RunServiceRequest {
  final String generation;
  const RunServiceRequest({
    required this.generation,
    required this.runId,
    required this.conversationId,
    required this.active,
    required this.waitingApproval,
  });
  final String runId;
  final String conversationId;
  final bool active;
  final bool waitingApproval;
}

sealed class AppCommand {
  const AppCommand();
}

class DiscoverModels extends AppCommand {
  const DiscoverModels(this.endpointId);
  final String endpointId;
}

class ProbeEndpoint extends AppCommand {
  const ProbeEndpoint(this.endpointId);
  final String endpointId;
}

class DiscoverServer extends AppCommand {
  const DiscoverServer(this.profileId, this.serverId);
  final String profileId;
  final String serverId;
}

class NewId extends AppCommand {
  const NewId();
}

class SaveEndpoint extends AppCommand {
  const SaveEndpoint(this.endpoint);
  final EndpointConfig endpoint;
}

class SaveServer extends AppCommand {
  const SaveServer(this.server);
  final ServerConfig server;
}

class SaveProfile extends AppCommand {
  const SaveProfile(this.profile);
  final Profile profile;
}

class CreateConversation extends AppCommand {
  const CreateConversation(this.profileId);
  final String profileId;
}

class ApplyProfile extends AppCommand {
  const ApplyProfile(this.conversationId, this.profileId);
  final String conversationId;
  final String profileId;
}

class SetRule extends AppCommand {
  const SetRule(this.key, this.schemaDigest, this.destinationDigest, this.rule);
  final ToolKey key;
  final String schemaDigest;
  final String destinationDigest;
  final PermissionRule rule;
}

class StartTurn extends AppCommand {
  const StartTurn(this.conversationId, this.text);
  final String conversationId;
  final String text;
}

class PlatformPause extends AppCommand {
  const PlatformPause(this.runId, this.generation);
  final String runId;
  final String generation;
}

class PlatformStop extends AppCommand {
  const PlatformStop(this.runId, this.generation);
  final String runId;
  final String generation;
}

class CancelRun extends AppCommand {
  const CancelRun(this.runId);
  final String runId;
}

class ResumeRun extends AppCommand {
  const ResumeRun(this.runId);
  final String runId;
}

/// Re-runs failed turn [runId] from its existing input message.
class RetryRun extends AppCommand {
  const RetryRun(this.runId);
  final String runId;
}

class Approve extends AppCommand {
  const Approve(this.binding);
  final ApprovalBinding binding;
}

class Deny extends AppCommand {
  const Deny(this.attemptId);
  final String attemptId;
}

/// Approves and denies pending calls of one run at once, then resumes it.
/// A stale binding fails the whole command (conflict) and changes nothing;
/// repeating a decision already in effect succeeds.
class DecideApprovals extends AppCommand {
  const DecideApprovals(
    this.runId, {
    this.approve = const [],
    this.deny = const [],
  });
  final String runId;
  final List<ApprovalBinding> approve;

  /// Attempt ids to deny.
  final List<String> deny;
}

/// Trusts (or stops trusting) every tool of an MCP server, app-wide, until
/// its URL, HTTP setting or credential references change.
class SetServerTrust extends AppCommand {
  const SetServerTrust(this.serverId, {required this.trusted});
  final String serverId;
  final bool trusted;
}

class SummarizeContext extends AppCommand {
  const SummarizeContext(this.conversationId, this.selection);
  final String conversationId;
  final ContextSelection selection;
}

class ApplyContextSelection extends AppCommand {
  const ApplyContextSelection(this.runId, this.selection);
  final String runId;
  final ContextSelection selection;
}

class AcknowledgeContextDisclosure extends AppCommand {
  const AcknowledgeContextDisclosure(this.conversationId, this.origin);
  final String conversationId;
  final String origin;
}
