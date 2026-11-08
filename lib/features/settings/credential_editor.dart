import 'package:flutter/material.dart';
import '../../backend/backend.dart';
import '../../backend/models.dart';
import '../../backend/platform_services.dart';

/// Secrets never pass through the backend model or prefill this form.
class CredentialEditor extends StatefulWidget {
  const CredentialEditor({
    super.key,
    required this.backend,
    required this.reference,
    this.headerName,
  });
  final Backend backend;
  final TextEditingController reference;

  /// The MCP header this credential is sent in; null for the inference
  /// credential, which always goes in `Authorization`.
  final TextEditingController? headerName;
  @override
  State<CredentialEditor> createState() => _CredentialEditorState();
}

class _CredentialEditorState extends State<CredentialEditor> {
  final secret = TextEditingController();
  bool busy = false;
  int operationRevision = 0;
  String? status;

  /// Bumped after a store or delete so the summary is described again.
  int summaryRevision = 0;
  ({String reference, int revision})? summaryKey;
  Future<_Summary>? summary;
  @override
  void dispose() {
    secret.clear();
    secret.dispose();
    super.dispose();
  }

  @override
  void didUpdateWidget(CredentialEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.reference, widget.reference)) {
      // A retired operation must not regain ownership if its controller is reused.
      operationRevision++;
      secret.clear();
      busy = false;
      status = null;
    }
  }

  Future<void> change(bool delete) async {
    final revision = ++operationRevision;
    final controller = widget.reference;
    final backend = widget.backend;
    var expectedReference = controller.text;
    final originalReference = expectedReference;
    final value = secret.text;
    bool ownsEditor() =>
        mounted &&
        operationRevision == revision &&
        identical(widget.reference, controller);
    bool ownsPair() => ownsEditor() && controller.text == expectedReference;
    secret.clear();
    setState(() {
      busy = true;
      status = null;
    });
    try {
      if (delete) {
        if (originalReference.isEmpty) return;
        final confirmed = await showDialog<bool>(
          context: context,
          builder: (context) => AlertDialog(
            title: const Text('Delete this stored credential?'),
            content: const Text(
              'All configurations using this reference will require re-entry. Conversation history stays readable.',
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(context, false),
                child: const Text('Keep credential'),
              ),
              TextButton(
                onPressed: () => Navigator.pop(context, true),
                child: const Text('Confirm deletion'),
              ),
            ],
          ),
        );
        if (!ownsPair() || confirmed != true) return;
        await PlatformServices.deleteCredential(originalReference);
        if (!ownsPair()) return;
      } else {
        if (value.isEmpty) return;
        var reference = originalReference;
        if (reference.isEmpty) {
          final reply = await backend.execute(const NewId());
          if (!ownsPair()) return;
          reference = reply.id!;
        }
        if (!ownsPair()) return;
        await PlatformServices.putCredential(reference, value);
        if (!ownsPair()) return;
        controller.text = reference;
        expectedReference = reference;
      }
      if (ownsPair()) {
        setState(() {
          status = delete
              ? 'Credential deleted. Future requests require re-entry; history remains available.'
              : 'Credential stored. Save configuration to attach a new reference. Replacement affects future requests.';
          summaryRevision++;
        });
      }
    } catch (_) {
      if (ownsPair()) {
        setState(() {
          status = 'Credential unavailable. Delete and re-enter it.';
        });
      }
    } finally {
      if (ownsEditor()) {
        if (ownsPair()) secret.clear();
        setState(() {
          busy = false;
        });
      }
    }
  }

  Future<_Summary> describe(String reference) async {
    if (reference.isEmpty) return const _Absent();
    try {
      final found = await PlatformServices.describeCredential(reference);
      return found == null ? const _Absent() : _Found(found);
    } on BackendFailure {
      return const _Unreadable();
    } catch (_) {
      // No credential channel (widget tests, desktop): show nothing.
      return const _Unknown();
    }
  }

  /// The future for the current reference, created again only when the
  /// reference or [summaryRevision] changes.
  Future<_Summary> currentSummary() {
    final key = (reference: widget.reference.text, revision: summaryRevision);
    if (summaryKey != key || summary == null) {
      summaryKey = key;
      summary = describe(key.reference);
    }
    return summary!;
  }

  String summaryText(_Summary summary) => switch (summary) {
    _Absent() => 'No credential stored',
    _Unreadable() => 'Stored credential cannot be read; re-enter it.',
    _Unknown() => '',
    // Only an `Authorization` header gets the scheme rule; other headers
    // send the stored value verbatim.
    _Found(:final value) => switch (widget.headerName?.text.trim()) {
      final header? when header.toLowerCase() != 'authorization' =>
        '${header.isEmpty ? 'Header' : header}: ••• (${value.length} chars)'
            '${value.sendable ? '' : '. Has leading/trailing whitespace or characters that cannot be sent'}',
      _ => value.authorizationPreview,
    },
  };

  @override
  Widget build(BuildContext context) => Column(
    children: [
      ListenableBuilder(
        listenable: Listenable.merge([widget.reference, widget.headerName]),
        builder: (context, _) => FutureBuilder<_Summary>(
          future: currentSummary(),
          builder: (context, snapshot) => switch (snapshot.data) {
            final data? when summaryText(data).isNotEmpty => Text(
              summaryText(data),
            ),
            _ => const SizedBox.shrink(),
          },
        ),
      ),
      const Text(
        'Replacement changes future requests for every configuration using this reference. Tokens are never loaded back into this form.',
      ),
      TextField(
        controller: secret,
        enabled: !busy,
        obscureText: true,
        autocorrect: false,
        enableSuggestions: false,
        enableIMEPersonalizedLearning: false,
        decoration: const InputDecoration(
          labelText: 'New API key / token (never displayed again)',
        ),
      ),
      Wrap(
        children: [
          TextButton(
            onPressed: busy ? null : () => change(false),
            child: const Text('Store / replace credential'),
          ),
          TextButton(
            onPressed: busy ? null : () => change(true),
            child: const Text('Delete stored credential'),
          ),
        ],
      ),
      if (status != null) Text(status!),
    ],
  );
}

sealed class _Summary {
  const _Summary();
}

class _Found extends _Summary {
  const _Found(this.value);
  final CredentialSummary value;
}

class _Absent extends _Summary {
  const _Absent();
}

class _Unreadable extends _Summary {
  const _Unreadable();
}

class _Unknown extends _Summary {
  const _Unknown();
}
