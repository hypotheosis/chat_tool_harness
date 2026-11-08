import 'dart:async';
import 'package:flutter/foundation.dart';
import '../src/rust/inference.dart' as inf;
import '../src/rust/api/agent.dart' as api;
import '../src/rust/application.dart' as callbacks;
import '../src/rust/application/dto.dart' as g;
import '../src/rust/model/core.dart' as core;
import '../src/rust/agent/context.dart' as ctx;
import '../src/rust/model.dart' as platform;
import '../src/rust/error.dart' as errors;
import 'backend.dart';
import 'bridge_numbers.dart';
import 'models.dart';
import 'platform_services.dart';

class FrbBackend implements Backend {
  FrbBackend._(this._handle);
  final api.AgentHandle _handle;
  Future<void>? _closing;
  final _polls = <_EventPoll>{};
  static Future<FrbBackend> open(
    String dataDir,
    PlatformServices services,
  ) async {
    try {
      final handle = await api.openAgent(
        dataDir: dataDir,
        resolveCredential: (reference) async {
          try {
            final value = await services.resolveCredential(reference);
            return value == null
                ? const callbacks.CredentialReply.missing()
                : callbacks.CredentialReply.value(value);
          } catch (_) {
            return const callbacks.CredentialReply.failed(
              'credential unavailable',
            );
          }
        },
        setServiceActive: (active) async {
          try {
            await services.setServiceActive(
              RunServiceRequest(
                runId: active.runId,
                generation: active.generation,
                conversationId: active.conversationId,
                active: active.active,
                waitingApproval: active.waitingApproval,
              ),
            );
            return const platform.PlatformReply.ok();
          } catch (_) {
            return const platform.PlatformReply.failed('service unavailable');
          }
        },
      );
      return FrbBackend._(handle);
    } catch (error) {
      throw mapFailure(error);
    }
  }

  void _checkOpen() {
    if (_closing != null) throw const BackendFailure(FailureCode.conflict);
  }

  @override
  Future<CommandReply> execute(AppCommand command) async {
    _checkOpen();
    try {
      return _fromCommandReply(
        await api.agentCommand(handle: _handle, command: _command(command)),
      );
    } catch (error) {
      throw mapFailure(error);
    }
  }

  @override
  Future<AppSnapshot> snapshot() async {
    _checkOpen();
    try {
      return _fromAppSnapshot(await api.agentSnapshot(handle: _handle));
    } catch (error) {
      throw mapFailure(error);
    }
  }

  @override
  Stream<AppEvent> events(int afterSequence) {
    final wait = _EventPoll();
    late StreamController<AppEvent> controller;
    Future<void> poll() async {
      var cursor = afterSequence;
      try {
        _checkOpen();
        bridgeUnsigned(BigInt.from(cursor), 64);
        while (!wait.cancelled && _closing == null) {
          await for (final event in api.agentEvents(
            handle: _handle,
            afterSequence: bridgeUnsigned(BigInt.from(cursor), 64),
          )) {
            if (wait.cancelled || _closing != null) break;
            final mapped = _fromAppEvent(event);
            if (mapped.sequence <= cursor) continue;
            cursor = mapped.sequence;
            controller.add(mapped);
          }
          if (!wait.cancelled && _closing == null) await wait.next();
        }
      } catch (error) {
        if (!wait.cancelled) controller.addError(mapFailure(error));
      } finally {
        wait.cancel();
        _polls.remove(wait);
        unawaited(controller.close());
      }
    }

    controller = StreamController<AppEvent>(
      onListen: () {
        _polls.add(wait);
        wait.running = poll();
      },
      onCancel: () {
        wait.cancel();
        return wait.running;
      },
    );
    return controller.stream;
  }

  @override
  Future<void> close() => _closing ??= _close();
  Future<void> _close() async {
    try {
      final polls = _polls.toList();
      for (final poll in polls) {
        poll.cancel();
      }
      await Future.wait(
        polls.map((poll) => poll.running ?? Future<void>.value()),
      );
      await api.closeAgent(handle: _handle);
    } catch (error) {
      throw mapFailure(error);
    } finally {
      _handle.dispose();
    }
  }
}

/// Maps a bridge error to a sanitized [BackendFailure].
@visibleForTesting
BackendFailure mapFailure(Object error) {
  if (error is BackendFailure) return error;
  if (error is g.AgentFailure) {
    final detail = error.detail;
    return BackendFailure(
      _fromCoreError(error.code),
      detail: detail == null ? null : _fromFailureDetail(detail),
    );
  }
  if (error is errors.CoreError) return BackendFailure(_fromCoreError(error));
  return BackendFailure.fromNative(error);
}

FailureCode _fromCoreError(errors.CoreError error) => switch (error) {
  errors.CoreError.invalidInput => FailureCode.invalidInput,
  errors.CoreError.configuration => FailureCode.configuration,
  errors.CoreError.authentication => FailureCode.authentication,
  errors.CoreError.credentialMissing => FailureCode.credentialMissing,
  errors.CoreError.rateLimited => FailureCode.rateLimited,
  errors.CoreError.toolUnsupported => FailureCode.toolUnsupported,
  errors.CoreError.discoveryUnavailable => FailureCode.discoveryUnavailable,
  errors.CoreError.contextLength => FailureCode.contextLength,
  errors.CoreError.unavailableTransport => FailureCode.unavailableTransport,
  errors.CoreError.protocol => FailureCode.protocol,
  errors.CoreError.limit => FailureCode.limit,
  errors.CoreError.storage => FailureCode.storage,
  errors.CoreError.cancelled => FailureCode.cancelled,
  errors.CoreError.conflict => FailureCode.conflict,
};

FailureDetail _fromFailureDetail(g.AgentFailureDetail v) => FailureDetail(
  targetKind: v.targetKind,
  targetName: v.targetName,
  step: v.step,
  url: v.url,
  status: v.status,
  reason: v.reason,
);

g.AppCommand _command(AppCommand command) => switch (command) {
  DiscoverModels() => g.AppCommand.discoverModels(
    endpointId: command.endpointId,
  ),
  ProbeEndpoint() => g.AppCommand.probeEndpoint(endpointId: command.endpointId),
  DiscoverServer() => g.AppCommand.discoverServer(
    profileId: command.profileId,
    serverId: command.serverId,
  ),
  CreateArtifact() => g.AppCommand.createArtifact(
    conversationId: command.conversationId,
    name: command.name,
    content: command.content,
  ),
  ReadArtifact() => g.AppCommand.readArtifact(
    conversationId: command.conversationId,
    artifactId: command.artifactId,
  ),
  DeleteArtifact() => g.AppCommand.deleteArtifact(
    conversationId: command.conversationId,
    artifactId: command.artifactId,
  ),
  SearchConversations() => g.AppCommand.searchConversations(
    query: command.query,
    offset: bridgeUnsigned(BigInt.from(command.offset), 32).toInt(),
    limit: bridgeUnsigned(BigInt.from(command.limit), 32).toInt(),
  ),
  RenameConversation() => g.AppCommand.renameConversation(
    conversationId: command.conversationId,
    title: command.title,
  ),
  DeleteConversation() => g.AppCommand.deleteConversation(
    conversationId: command.conversationId,
    confirmedUnknown: command.confirmedUnknown,
  ),
  ExportTranscript() => g.AppCommand.exportTranscript(
    conversationId: command.conversationId,
    includeToolPayloads: command.includeToolPayloads,
  ),
  NewId() => const g.AppCommand.newId(),
  SaveEndpoint() => g.AppCommand.saveEndpoint(
    endpoint: _toEndpointConfig(command.endpoint),
  ),
  SaveServer() => g.AppCommand.saveServer(
    server: _toServerConfig(command.server),
  ),
  SaveProfile() => g.AppCommand.saveProfile(
    profile: _toProfile(command.profile),
  ),
  CreateConversation() => g.AppCommand.createConversation(
    profileId: command.profileId,
  ),
  ApplyProfile() => g.AppCommand.applyProfile(
    conversationId: command.conversationId,
    profileId: command.profileId,
  ),
  SetRule() => g.AppCommand.setRule(
    key: _toToolKey(command.key),
    schemaDigest: command.schemaDigest,
    destinationDigest: command.destinationDigest,
    rule: _toPermissionRule(command.rule),
  ),
  StartTurn() => g.AppCommand.startTurn(
    conversationId: command.conversationId,
    text: command.text,
  ),
  PlatformPause() => g.AppCommand.platformPause(
    runId: command.runId,
    generation: command.generation,
  ),
  PlatformStop() => g.AppCommand.platformStop(
    runId: command.runId,
    generation: command.generation,
  ),
  CancelRun() => g.AppCommand.cancelRun(runId: command.runId),
  ResumeRun() => g.AppCommand.resumeRun(runId: command.runId),
  Approve() => g.AppCommand.approve(
    binding: _toApprovalBinding(command.binding),
  ),
  Deny() => g.AppCommand.deny(attemptId: command.attemptId),
  SummarizeContext() => g.AppCommand.summarizeContext(
    conversationId: command.conversationId,
    selection: _toContextSelection(command.selection),
  ),
  ApplyContextSelection() => g.AppCommand.applyContextSelection(
    runId: command.runId,
    selection: _toContextSelection(command.selection),
  ),
  AcknowledgeContextDisclosure() => g.AppCommand.acknowledgeContextDisclosure(
    conversationId: command.conversationId,
    origin: command.origin,
  ),
};

EndpointConfig _fromEndpointConfig(g.AgentEndpointConfig v) => EndpointConfig(
  id: v.id,
  name: v.name,
  baseUrl: v.baseUrl,
  model: v.model,
  contextTokens: v.contextTokens,
  maxOutputTokens: v.maxOutputTokens,
  temperature: v.temperature,
  allowHttp: v.allowHttp,
  credential: v.credential,
);
g.AgentEndpointConfig _toEndpointConfig(EndpointConfig v) =>
    g.AgentEndpointConfig(
      id: v.id,
      name: v.name,
      baseUrl: v.baseUrl,
      model: v.model,
      contextTokens: bridgeUnsigned(BigInt.from(v.contextTokens), 32).toInt(),
      maxOutputTokens: bridgeUnsigned(
        BigInt.from(v.maxOutputTokens),
        32,
      ).toInt(),
      temperature: v.temperature,
      allowHttp: v.allowHttp,
      credential: v.credential,
    );

ServerConfig _fromServerConfig(g.AgentServerConfig v) => ServerConfig(
  id: v.id,
  name: v.name,
  url: v.url,
  allowHttp: v.allowHttp,
  secretHeaders: v.secretHeaders.map((v) => _fromSecretHeader(v)).toList(),
);
g.AgentServerConfig _toServerConfig(ServerConfig v) => g.AgentServerConfig(
  id: v.id,
  name: v.name,
  url: v.url,
  allowHttp: v.allowHttp,
  secretHeaders: v.secretHeaders.map((v) => _toSecretHeader(v)).toList(),
);

SecretHeader _fromSecretHeader(g.AgentSecretHeader v) =>
    SecretHeader(name: v.name, credential: v.credential);
g.AgentSecretHeader _toSecretHeader(SecretHeader v) =>
    g.AgentSecretHeader(name: v.name, credential: v.credential);

RunLimits _fromRunLimits(g.AgentRunLimits v) => RunLimits(
  maxInferenceRequests: v.maxInferenceRequests,
  maxToolAttempts: v.maxToolAttempts,
  activeTimeoutMs: v.activeTimeoutMs.toInt(),
);
g.AgentRunLimits _toRunLimits(RunLimits v) => g.AgentRunLimits(
  maxInferenceRequests: bridgeUnsigned(
    BigInt.from(v.maxInferenceRequests),
    32,
  ).toInt(),
  maxToolAttempts: bridgeUnsigned(BigInt.from(v.maxToolAttempts), 32).toInt(),
  activeTimeoutMs: bridgeUnsigned(BigInt.from(v.activeTimeoutMs), 64),
);

Profile _fromProfile(g.AgentProfile v) => Profile(
  mode: _fromMode(v.mode),
  id: v.id,
  name: v.name,
  endpointId: v.endpointId,
  serverIds: v.serverIds.map((v) => v).toList(),
  instructions: v.instructions,
  limits: _fromRunLimits(v.limits),
);
g.AgentProfile _toProfile(Profile v) => g.AgentProfile(
  mode: switch (v.mode) {
    ProfileMode.agent => core.ProfileMode.agent,
    ProfileMode.chatOnly => core.ProfileMode.chatOnly,
  },
  id: v.id,
  name: v.name,
  endpointId: v.endpointId,
  serverIds: v.serverIds.map((v) => v).toList(),
  instructions: v.instructions,
  limits: _toRunLimits(v.limits),
);

ProfileSnapshot _fromProfileSnapshot(g.AgentProfileSnapshot v) =>
    ProfileSnapshot(
      inferenceOrigin: v.inferenceOrigin,
      mode: _fromMode(v.mode),
      profileId: v.profileId,
      revision: v.revision,
      name: v.name,
      endpoint: _fromEndpointConfig(v.endpoint),
      servers: v.servers.map((v) => _fromServerConfig(v)).toList(),
      instructions: v.instructions,
      limits: _fromRunLimits(v.limits),
    );

ToolKey _fromToolKey(g.AgentToolKey v) =>
    ToolKey(profileId: v.profileId, serverId: v.serverId, name: v.name);
g.AgentToolKey _toToolKey(ToolKey v) =>
    g.AgentToolKey(profileId: v.profileId, serverId: v.serverId, name: v.name);

ApprovalBinding _fromApprovalBinding(g.AgentApprovalBinding v) =>
    ApprovalBinding(
      attemptId: v.attemptId,
      key: _fromToolKey(v.key),
      providerCallId: v.providerCallId,
      definitionDigest: v.definitionDigest,
      argumentsDigest: v.argumentsDigest,
      argumentsJson: v.argumentsJson,
      schemaJson: v.schemaJson,
      destinationDigest: v.destinationDigest,
    );
g.AgentApprovalBinding _toApprovalBinding(ApprovalBinding v) =>
    g.AgentApprovalBinding(
      attemptId: v.attemptId,
      key: _toToolKey(v.key),
      providerCallId: v.providerCallId,
      definitionDigest: v.definitionDigest,
      argumentsDigest: v.argumentsDigest,
      argumentsJson: v.argumentsJson,
      schemaJson: v.schemaJson,
      destinationDigest: v.destinationDigest,
    );

ToolCall _fromToolCall(g.AgentToolCall v) => ToolCall(
  key: _fromToolKey(v.key),
  providerCallId: v.providerCallId,
  argumentsJson: v.argumentsJson,
  sourceEnvelope: v.sourceEnvelope,
);

Message _fromMessage(g.AgentMessage v) => Message(
  id: v.id,
  role: v.role,
  content: v.content,
  calls: v.calls.map((v) => _fromToolCall(v)).toList(),
  callId: v.callId,
  sourceEnvelope: v.sourceEnvelope,
);

ContextSelection _fromContextSelection(g.AgentContextSelection v) =>
    ContextSelection(
      messageIds: v.messageIds.map((v) => v).toList(),
      summaryIds: v.summaryIds.map((v) => v).toList(),
    );
g.AgentContextSelection _toContextSelection(ContextSelection v) =>
    g.AgentContextSelection(
      messageIds: v.messageIds.map((v) => v).toList(),
      summaryIds: v.summaryIds.map((v) => v).toList(),
    );

PageReference _fromPageReference(g.AgentPageReference v) =>
    PageReference(attachmentId: v.attachmentId, physicalPage: v.physicalPage);

DerivedContext _fromDerivedContext(g.AgentDerivedContext v) => DerivedContext(
  id: v.id,
  runId: v.runId,
  text: v.text,
  selection: _fromContextSelection(v.selection),
  pageReferences: v.pageReferences.map((v) => _fromPageReference(v)).toList(),
  partialCoverage: v.partialCoverage,
  unresolvedAttempts: v.unresolvedAttempts.map((v) => v).toList(),
);

ProviderUsage _fromProviderUsage(g.AgentProviderUsage v) => ProviderUsage(
  promptTokens: v.promptTokens.toInt(),
  completionTokens: v.completionTokens.toInt(),
);

ContextUsage _fromContextUsage(g.AgentContextUsage v) => ContextUsage(
  estimation: _fromTokenEstimation(v.estimation),
  estimatedInputTokens: v.estimatedInputTokens.toInt(),
  outputReserve: v.outputReserve,
  window: v.window,
  rawBytes: v.rawBytes.toInt(),
  rawBytesIsLowerBound: v.rawBytesIsLowerBound,
  provider: v.provider == null ? null : _fromProviderUsage(v.provider!),
  reason: v.reason == null ? null : _fromContextChoiceReason(v.reason!),
);

RunContext _fromRunContext(g.AgentRunContext v) => RunContext(
  purpose: _fromContextPurpose(v.purpose),
  selection: v.selection == null ? null : _fromContextSelection(v.selection!),
  selectionThroughOrdinal: v.selectionThroughOrdinal,
  usage: v.usage == null ? null : _fromContextUsage(v.usage!),
  derivedEntry: v.derivedEntry,
  summary: v.summary == null ? null : _fromDerivedContext(v.summary!),
);

Attempt _fromAttempt(g.AgentAttempt v) => Attempt(
  id: v.id,
  state: _fromAttemptState(v.state),
  call: _fromToolCall(v.call),
  binding: v.binding == null ? null : _fromApprovalBinding(v.binding!),
);

Run _fromRun(g.AgentRun v) => Run(
  id: v.id,
  conversationId: v.conversationId,
  profile: _fromProfileSnapshot(v.profile),
  state: _fromRunState(v.state),
  inferenceCount: v.inferenceCount,
  attemptCount: v.attemptCount,
  activeMs: v.activeMs.toInt(),
  partialText: v.partialText,
  failure: v.failure,
  failureCode: v.failureCode == null ? null : _fromCoreError(v.failureCode!),
  failureDetail: v.failureDetail == null
      ? null
      : _fromFailureDetail(v.failureDetail!),
  attempts: v.attempts.map((v) => _fromAttempt(v)).toList(),
  context: _fromRunContext(v.context),
  inputMessageId: v.inputMessageId,
);

Conversation _fromConversation(g.AgentConversation v) => Conversation(
  id: v.id,
  profile: _fromProfileSnapshot(v.profile),
  messages: v.messages.map((v) => _fromMessage(v)).toList(),
  summaries: v.summaries.map((v) => _fromDerivedContext(v)).toList(),
  originAcknowledged: v.originAcknowledged,
);

ToolRule _fromToolRule(g.AgentToolRule v) => ToolRule(
  key: _fromToolKey(v.key),
  schemaJson: v.schemaJson,
  schemaDigest: v.schemaDigest,
  destinationDigest: v.destinationDigest,
  rule: _fromPermissionRule(v.rule),
);

AppSnapshot _fromAppSnapshot(g.AgentAppSnapshot v) => AppSnapshot(
  artifacts: v.artifacts.map(_fromArtifact).toList(),
  retentionLimitBytes: v.retentionLimitBytes.toInt(),
  sequence: v.sequence.toInt(),
  endpoints: v.endpoints.map((v) => _fromEndpointConfig(v)).toList(),
  servers: v.servers.map((v) => _fromServerConfig(v)).toList(),
  profiles: v.profiles.map((v) => _fromProfile(v)).toList(),
  conversations: v.conversations.map((v) => _fromConversation(v)).toList(),
  runs: v.runs.map((v) => _fromRun(v)).toList(),
  rules: v.rules.map((v) => _fromToolRule(v)).toList(),
);

AppEvent _fromAppEvent(g.AgentAppEvent v) =>
    AppEvent(sequence: v.sequence.toInt(), conversationId: v.conversationId);

ArtifactSummary _fromArtifact(g.AgentArtifact v) => ArtifactSummary(
  id: v.id,
  conversationId: v.conversationId,
  name: v.name,
  bytes: v.bytes.toInt(),
);
ArtifactContent _fromArtifactContent(g.AgentArtifactContent v) =>
    ArtifactContent(
      summary: _fromArtifact(v.summary),
      text: v.text,
      privatePath: v.privatePath,
    );
ProfileMode _fromMode(core.ProfileMode v) => switch (v) {
  core.ProfileMode.agent => ProfileMode.agent,
  core.ProfileMode.chatOnly => ProfileMode.chatOnly,
};
ToolSupport _fromSupport(inf.ToolSupport v) => switch (v) {
  inf.ToolSupport.supported => ToolSupport.supported,
  inf.ToolSupport.unsupported => ToolSupport.unsupported,
  inf.ToolSupport.inconclusive => ToolSupport.inconclusive,
};
CommandReply _fromCommandReply(g.AgentCommandReply v) => CommandReply(
  history: v.history
      .map(
        (v) => ConversationSummary(
          id: v.id,
          title: v.title,
          hasUnknownOutcomes: v.hasUnknownOutcomes,
        ),
      )
      .toList(),
  exportBundle: v.exportBundle == null
      ? null
      : ExportBundle(transcript: _fromArtifact(v.exportBundle!.transcript)),
  models: v.models,
  toolSupport: v.toolSupport == null ? null : _fromSupport(v.toolSupport!),
  discoveredTools: v.discoveredTools,
  discoveredRules: v.discoveredRules.map(_fromToolRule).toList(),
  id: v.id,
  artifact: v.artifact == null ? null : _fromArtifactContent(v.artifact!),
);

RunState _fromRunState(core.RunState v) => switch (v) {
  core.RunState.running => RunState.running,
  core.RunState.waitingApproval => RunState.waitingApproval,
  core.RunState.paused => RunState.paused,
  core.RunState.completed => RunState.completed,
  core.RunState.cancelled => RunState.cancelled,
  core.RunState.failed => RunState.failed,
  core.RunState.interrupted => RunState.interrupted,
};

AttemptState _fromAttemptState(core.AttemptState v) => switch (v) {
  core.AttemptState.proposed => AttemptState.proposed,
  core.AttemptState.denied => AttemptState.denied,
  core.AttemptState.approved => AttemptState.approved,
  core.AttemptState.dispatching => AttemptState.dispatching,
  core.AttemptState.succeeded => AttemptState.succeeded,
  core.AttemptState.failed => AttemptState.failed,
  core.AttemptState.outcomeUnknown => AttemptState.outcomeUnknown,
};

PermissionRule _fromPermissionRule(core.PermissionRule v) => switch (v) {
  core.PermissionRule.ask => PermissionRule.ask,
  core.PermissionRule.allow => PermissionRule.allow,
  core.PermissionRule.block => PermissionRule.block,
};

ContextPurpose _fromContextPurpose(ctx.ContextPurpose v) => switch (v) {
  ctx.ContextPurpose.turn => ContextPurpose.turn,
  ctx.ContextPurpose.summary => ContextPurpose.summary,
};

ContextChoiceReason _fromContextChoiceReason(
  ctx.ContextChoiceReason v,
) => switch (v) {
  ctx.ContextChoiceReason.windowExceeded => ContextChoiceReason.windowExceeded,
  ctx.ContextChoiceReason.rawLimit => ContextChoiceReason.rawLimit,
  ctx.ContextChoiceReason.invalidHistory => ContextChoiceReason.invalidHistory,
  ctx.ContextChoiceReason.disclosureRequired =>
    ContextChoiceReason.disclosureRequired,
  ctx.ContextChoiceReason.providerRejected =>
    ContextChoiceReason.providerRejected,
};

TokenEstimation _fromTokenEstimation(ctx.TokenEstimation v) => switch (v) {
  ctx.TokenEstimation.conservativeUtf8Bytes =>
    TokenEstimation.conservativeUtf8Bytes,
};

core.PermissionRule _toPermissionRule(PermissionRule v) => switch (v) {
  PermissionRule.ask => core.PermissionRule.ask,
  PermissionRule.allow => core.PermissionRule.allow,
  PermissionRule.block => core.PermissionRule.block,
};

class _EventPoll {
  bool cancelled = false;
  Future<void>? running;
  Timer? _timer;
  Completer<void>? _pending;
  Future<void> next() {
    if (cancelled) return Future.value();
    final pending = Completer<void>();
    _pending = pending;
    _timer = Timer(const Duration(milliseconds: 50), _wake);
    return pending.future;
  }

  void _wake() {
    _timer?.cancel();
    _timer = null;
    final pending = _pending;
    _pending = null;
    pending?.complete();
  }

  void cancel() {
    cancelled = true;
    _wake();
  }
}
