import 'package:chat_tool_harness/backend/models.dart';
import 'fake_backend.dart';

const endpoint = EndpointConfig(
  id: 'e',
  name: 'Endpoint',
  baseUrl: 'https://example.test/v1',
  model: 'manual',
  contextTokens: 4096,
  maxOutputTokens: 512,
  temperature: 0.2,
  allowHttp: false,
  credential: 'reference-only',
);
const profile = Profile(
  id: 'p',
  name: 'Profile',
  endpointId: 'e',
  serverIds: ['s'],
  instructions: '',
  limits: RunLimits(),
);
const profileSnapshot = ProfileSnapshot(
  inferenceOrigin: 'https://example.test',
  profileId: 'p',
  revision: 1,
  name: 'Old profile',
  endpoint: endpoint,
  servers: [
    ServerConfig(
      id: 's',
      name: 'Server',
      url: 'https://example.test/mcp',
      allowHttp: false,
    ),
  ],
  instructions: '',
  limits: RunLimits(),
);
const conversation = Conversation(
  id: 'c',
  profile: profileSnapshot,
  messages: [Message(id: 'm', role: 'user', content: 'hello', calls: [])],
  summaries: [],
);
const binding = ApprovalBinding(
  attemptId: 'a',
  key: ToolKey(profileId: 'p', serverId: 's', name: 'write'),
  providerCallId: 'call',
  definitionDigest: 'definition',
  argumentsDigest: 'args',
  argumentsJson: '{"text":"exact immutable arguments"}',
  schemaJson: '{}',
  destinationDigest: 'destination',
);
Run run({
  RunState runState = RunState.waitingApproval,
  AttemptState state = AttemptState.proposed,
  String? arguments,
}) => Run(
  id: 'r',
  conversationId: 'c',
  profile: profileSnapshot,
  state: runState,
  inferenceCount: 1,
  attemptCount: 1,
  activeMs: 0,
  partialText: 'Streaming text',
  attempts: [
    Attempt(
      id: 'a',
      state: state,
      call: ToolCall(
        key: binding.key,
        providerCallId: 'call',
        argumentsJson: arguments ?? binding.argumentsJson,
      ),
      binding: binding,
    ),
  ],
  context: const RunContext(
    purpose: ContextPurpose.turn,
    usage: ContextUsage(
      estimation: TokenEstimation.conservativeUtf8Bytes,
      estimatedInputTokens: 50,
      outputReserve: 512,
      window: 4096,
      rawBytes: 50,
      rawBytesIsLowerBound: false,
      reason: ContextChoiceReason.disclosureRequired,
    ),
  ),
);

class UiBackend extends FakeBackend {
  UiBackend() {
    current = AppSnapshot(
      endpoints: [endpoint],
      profiles: [profile],
      servers: const [
        ServerConfig(
          id: 's',
          name: 'Server',
          url: 'https://example.test/mcp',
          allowHttp: false,
        ),
      ],
      conversations: [conversation],
      runs: [run()],
    );
  }
  CommandReply reply = const CommandReply();
  @override
  Future<CommandReply> execute(AppCommand command) async {
    await super.execute(command);
    return command is NewId ? const CommandReply(id: 'new-id') : reply;
  }
}
