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
  });
  final Backend backend;
  final TextEditingController reference;
  @override
  State<CredentialEditor> createState() => _CredentialEditorState();
}

class _CredentialEditorState extends State<CredentialEditor> {
  final secret = TextEditingController();
  bool busy = false;
  int operationRevision = 0;
  String? status;
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

  @override
  Widget build(BuildContext context) => Column(
    children: [
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
