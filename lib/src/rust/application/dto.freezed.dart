// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'dto.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$AppCommand {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'AppCommand()';
}


}

/// @nodoc
class $AppCommandCopyWith<$Res>  {
$AppCommandCopyWith(AppCommand _, $Res Function(AppCommand) __);
}


/// Adds pattern-matching-related methods to [AppCommand].
extension AppCommandPatterns on AppCommand {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( AppCommand_SearchConversations value)?  searchConversations,TResult Function( AppCommand_RenameConversation value)?  renameConversation,TResult Function( AppCommand_DeleteConversation value)?  deleteConversation,TResult Function( AppCommand_ArchiveConversation value)?  archiveConversation,TResult Function( AppCommand_RestoreConversation value)?  restoreConversation,TResult Function( AppCommand_ExportTranscript value)?  exportTranscript,TResult Function( AppCommand_DiscoverModels value)?  discoverModels,TResult Function( AppCommand_ProbeEndpoint value)?  probeEndpoint,TResult Function( AppCommand_DiscoverServer value)?  discoverServer,TResult Function( AppCommand_CreateArtifact value)?  createArtifact,TResult Function( AppCommand_ReadArtifact value)?  readArtifact,TResult Function( AppCommand_DeleteArtifact value)?  deleteArtifact,TResult Function( AppCommand_NewId value)?  newId,TResult Function( AppCommand_SaveEndpoint value)?  saveEndpoint,TResult Function( AppCommand_SaveServer value)?  saveServer,TResult Function( AppCommand_SaveProfile value)?  saveProfile,TResult Function( AppCommand_CreateConversation value)?  createConversation,TResult Function( AppCommand_ApplyProfile value)?  applyProfile,TResult Function( AppCommand_SetRule value)?  setRule,TResult Function( AppCommand_StartTurn value)?  startTurn,TResult Function( AppCommand_PlatformPause value)?  platformPause,TResult Function( AppCommand_PlatformStop value)?  platformStop,TResult Function( AppCommand_CancelRun value)?  cancelRun,TResult Function( AppCommand_ResumeRun value)?  resumeRun,TResult Function( AppCommand_RetryRun value)?  retryRun,TResult Function( AppCommand_Approve value)?  approve,TResult Function( AppCommand_Deny value)?  deny,TResult Function( AppCommand_DecideApprovals value)?  decideApprovals,TResult Function( AppCommand_SetServerTrust value)?  setServerTrust,TResult Function( AppCommand_SummarizeContext value)?  summarizeContext,TResult Function( AppCommand_ApplyContextSelection value)?  applyContextSelection,TResult Function( AppCommand_AcknowledgeContextDisclosure value)?  acknowledgeContextDisclosure,required TResult orElse(),}){
final _that = this;
switch (_that) {
case AppCommand_SearchConversations() when searchConversations != null:
return searchConversations(_that);case AppCommand_RenameConversation() when renameConversation != null:
return renameConversation(_that);case AppCommand_DeleteConversation() when deleteConversation != null:
return deleteConversation(_that);case AppCommand_ArchiveConversation() when archiveConversation != null:
return archiveConversation(_that);case AppCommand_RestoreConversation() when restoreConversation != null:
return restoreConversation(_that);case AppCommand_ExportTranscript() when exportTranscript != null:
return exportTranscript(_that);case AppCommand_DiscoverModels() when discoverModels != null:
return discoverModels(_that);case AppCommand_ProbeEndpoint() when probeEndpoint != null:
return probeEndpoint(_that);case AppCommand_DiscoverServer() when discoverServer != null:
return discoverServer(_that);case AppCommand_CreateArtifact() when createArtifact != null:
return createArtifact(_that);case AppCommand_ReadArtifact() when readArtifact != null:
return readArtifact(_that);case AppCommand_DeleteArtifact() when deleteArtifact != null:
return deleteArtifact(_that);case AppCommand_NewId() when newId != null:
return newId(_that);case AppCommand_SaveEndpoint() when saveEndpoint != null:
return saveEndpoint(_that);case AppCommand_SaveServer() when saveServer != null:
return saveServer(_that);case AppCommand_SaveProfile() when saveProfile != null:
return saveProfile(_that);case AppCommand_CreateConversation() when createConversation != null:
return createConversation(_that);case AppCommand_ApplyProfile() when applyProfile != null:
return applyProfile(_that);case AppCommand_SetRule() when setRule != null:
return setRule(_that);case AppCommand_StartTurn() when startTurn != null:
return startTurn(_that);case AppCommand_PlatformPause() when platformPause != null:
return platformPause(_that);case AppCommand_PlatformStop() when platformStop != null:
return platformStop(_that);case AppCommand_CancelRun() when cancelRun != null:
return cancelRun(_that);case AppCommand_ResumeRun() when resumeRun != null:
return resumeRun(_that);case AppCommand_RetryRun() when retryRun != null:
return retryRun(_that);case AppCommand_Approve() when approve != null:
return approve(_that);case AppCommand_Deny() when deny != null:
return deny(_that);case AppCommand_DecideApprovals() when decideApprovals != null:
return decideApprovals(_that);case AppCommand_SetServerTrust() when setServerTrust != null:
return setServerTrust(_that);case AppCommand_SummarizeContext() when summarizeContext != null:
return summarizeContext(_that);case AppCommand_ApplyContextSelection() when applyContextSelection != null:
return applyContextSelection(_that);case AppCommand_AcknowledgeContextDisclosure() when acknowledgeContextDisclosure != null:
return acknowledgeContextDisclosure(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( AppCommand_SearchConversations value)  searchConversations,required TResult Function( AppCommand_RenameConversation value)  renameConversation,required TResult Function( AppCommand_DeleteConversation value)  deleteConversation,required TResult Function( AppCommand_ArchiveConversation value)  archiveConversation,required TResult Function( AppCommand_RestoreConversation value)  restoreConversation,required TResult Function( AppCommand_ExportTranscript value)  exportTranscript,required TResult Function( AppCommand_DiscoverModels value)  discoverModels,required TResult Function( AppCommand_ProbeEndpoint value)  probeEndpoint,required TResult Function( AppCommand_DiscoverServer value)  discoverServer,required TResult Function( AppCommand_CreateArtifact value)  createArtifact,required TResult Function( AppCommand_ReadArtifact value)  readArtifact,required TResult Function( AppCommand_DeleteArtifact value)  deleteArtifact,required TResult Function( AppCommand_NewId value)  newId,required TResult Function( AppCommand_SaveEndpoint value)  saveEndpoint,required TResult Function( AppCommand_SaveServer value)  saveServer,required TResult Function( AppCommand_SaveProfile value)  saveProfile,required TResult Function( AppCommand_CreateConversation value)  createConversation,required TResult Function( AppCommand_ApplyProfile value)  applyProfile,required TResult Function( AppCommand_SetRule value)  setRule,required TResult Function( AppCommand_StartTurn value)  startTurn,required TResult Function( AppCommand_PlatformPause value)  platformPause,required TResult Function( AppCommand_PlatformStop value)  platformStop,required TResult Function( AppCommand_CancelRun value)  cancelRun,required TResult Function( AppCommand_ResumeRun value)  resumeRun,required TResult Function( AppCommand_RetryRun value)  retryRun,required TResult Function( AppCommand_Approve value)  approve,required TResult Function( AppCommand_Deny value)  deny,required TResult Function( AppCommand_DecideApprovals value)  decideApprovals,required TResult Function( AppCommand_SetServerTrust value)  setServerTrust,required TResult Function( AppCommand_SummarizeContext value)  summarizeContext,required TResult Function( AppCommand_ApplyContextSelection value)  applyContextSelection,required TResult Function( AppCommand_AcknowledgeContextDisclosure value)  acknowledgeContextDisclosure,}){
final _that = this;
switch (_that) {
case AppCommand_SearchConversations():
return searchConversations(_that);case AppCommand_RenameConversation():
return renameConversation(_that);case AppCommand_DeleteConversation():
return deleteConversation(_that);case AppCommand_ArchiveConversation():
return archiveConversation(_that);case AppCommand_RestoreConversation():
return restoreConversation(_that);case AppCommand_ExportTranscript():
return exportTranscript(_that);case AppCommand_DiscoverModels():
return discoverModels(_that);case AppCommand_ProbeEndpoint():
return probeEndpoint(_that);case AppCommand_DiscoverServer():
return discoverServer(_that);case AppCommand_CreateArtifact():
return createArtifact(_that);case AppCommand_ReadArtifact():
return readArtifact(_that);case AppCommand_DeleteArtifact():
return deleteArtifact(_that);case AppCommand_NewId():
return newId(_that);case AppCommand_SaveEndpoint():
return saveEndpoint(_that);case AppCommand_SaveServer():
return saveServer(_that);case AppCommand_SaveProfile():
return saveProfile(_that);case AppCommand_CreateConversation():
return createConversation(_that);case AppCommand_ApplyProfile():
return applyProfile(_that);case AppCommand_SetRule():
return setRule(_that);case AppCommand_StartTurn():
return startTurn(_that);case AppCommand_PlatformPause():
return platformPause(_that);case AppCommand_PlatformStop():
return platformStop(_that);case AppCommand_CancelRun():
return cancelRun(_that);case AppCommand_ResumeRun():
return resumeRun(_that);case AppCommand_RetryRun():
return retryRun(_that);case AppCommand_Approve():
return approve(_that);case AppCommand_Deny():
return deny(_that);case AppCommand_DecideApprovals():
return decideApprovals(_that);case AppCommand_SetServerTrust():
return setServerTrust(_that);case AppCommand_SummarizeContext():
return summarizeContext(_that);case AppCommand_ApplyContextSelection():
return applyContextSelection(_that);case AppCommand_AcknowledgeContextDisclosure():
return acknowledgeContextDisclosure(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( AppCommand_SearchConversations value)?  searchConversations,TResult? Function( AppCommand_RenameConversation value)?  renameConversation,TResult? Function( AppCommand_DeleteConversation value)?  deleteConversation,TResult? Function( AppCommand_ArchiveConversation value)?  archiveConversation,TResult? Function( AppCommand_RestoreConversation value)?  restoreConversation,TResult? Function( AppCommand_ExportTranscript value)?  exportTranscript,TResult? Function( AppCommand_DiscoverModels value)?  discoverModels,TResult? Function( AppCommand_ProbeEndpoint value)?  probeEndpoint,TResult? Function( AppCommand_DiscoverServer value)?  discoverServer,TResult? Function( AppCommand_CreateArtifact value)?  createArtifact,TResult? Function( AppCommand_ReadArtifact value)?  readArtifact,TResult? Function( AppCommand_DeleteArtifact value)?  deleteArtifact,TResult? Function( AppCommand_NewId value)?  newId,TResult? Function( AppCommand_SaveEndpoint value)?  saveEndpoint,TResult? Function( AppCommand_SaveServer value)?  saveServer,TResult? Function( AppCommand_SaveProfile value)?  saveProfile,TResult? Function( AppCommand_CreateConversation value)?  createConversation,TResult? Function( AppCommand_ApplyProfile value)?  applyProfile,TResult? Function( AppCommand_SetRule value)?  setRule,TResult? Function( AppCommand_StartTurn value)?  startTurn,TResult? Function( AppCommand_PlatformPause value)?  platformPause,TResult? Function( AppCommand_PlatformStop value)?  platformStop,TResult? Function( AppCommand_CancelRun value)?  cancelRun,TResult? Function( AppCommand_ResumeRun value)?  resumeRun,TResult? Function( AppCommand_RetryRun value)?  retryRun,TResult? Function( AppCommand_Approve value)?  approve,TResult? Function( AppCommand_Deny value)?  deny,TResult? Function( AppCommand_DecideApprovals value)?  decideApprovals,TResult? Function( AppCommand_SetServerTrust value)?  setServerTrust,TResult? Function( AppCommand_SummarizeContext value)?  summarizeContext,TResult? Function( AppCommand_ApplyContextSelection value)?  applyContextSelection,TResult? Function( AppCommand_AcknowledgeContextDisclosure value)?  acknowledgeContextDisclosure,}){
final _that = this;
switch (_that) {
case AppCommand_SearchConversations() when searchConversations != null:
return searchConversations(_that);case AppCommand_RenameConversation() when renameConversation != null:
return renameConversation(_that);case AppCommand_DeleteConversation() when deleteConversation != null:
return deleteConversation(_that);case AppCommand_ArchiveConversation() when archiveConversation != null:
return archiveConversation(_that);case AppCommand_RestoreConversation() when restoreConversation != null:
return restoreConversation(_that);case AppCommand_ExportTranscript() when exportTranscript != null:
return exportTranscript(_that);case AppCommand_DiscoverModels() when discoverModels != null:
return discoverModels(_that);case AppCommand_ProbeEndpoint() when probeEndpoint != null:
return probeEndpoint(_that);case AppCommand_DiscoverServer() when discoverServer != null:
return discoverServer(_that);case AppCommand_CreateArtifact() when createArtifact != null:
return createArtifact(_that);case AppCommand_ReadArtifact() when readArtifact != null:
return readArtifact(_that);case AppCommand_DeleteArtifact() when deleteArtifact != null:
return deleteArtifact(_that);case AppCommand_NewId() when newId != null:
return newId(_that);case AppCommand_SaveEndpoint() when saveEndpoint != null:
return saveEndpoint(_that);case AppCommand_SaveServer() when saveServer != null:
return saveServer(_that);case AppCommand_SaveProfile() when saveProfile != null:
return saveProfile(_that);case AppCommand_CreateConversation() when createConversation != null:
return createConversation(_that);case AppCommand_ApplyProfile() when applyProfile != null:
return applyProfile(_that);case AppCommand_SetRule() when setRule != null:
return setRule(_that);case AppCommand_StartTurn() when startTurn != null:
return startTurn(_that);case AppCommand_PlatformPause() when platformPause != null:
return platformPause(_that);case AppCommand_PlatformStop() when platformStop != null:
return platformStop(_that);case AppCommand_CancelRun() when cancelRun != null:
return cancelRun(_that);case AppCommand_ResumeRun() when resumeRun != null:
return resumeRun(_that);case AppCommand_RetryRun() when retryRun != null:
return retryRun(_that);case AppCommand_Approve() when approve != null:
return approve(_that);case AppCommand_Deny() when deny != null:
return deny(_that);case AppCommand_DecideApprovals() when decideApprovals != null:
return decideApprovals(_that);case AppCommand_SetServerTrust() when setServerTrust != null:
return setServerTrust(_that);case AppCommand_SummarizeContext() when summarizeContext != null:
return summarizeContext(_that);case AppCommand_ApplyContextSelection() when applyContextSelection != null:
return applyContextSelection(_that);case AppCommand_AcknowledgeContextDisclosure() when acknowledgeContextDisclosure != null:
return acknowledgeContextDisclosure(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String query,  bool archivedOnly,  int offset,  int limit)?  searchConversations,TResult Function( String conversationId,  String title)?  renameConversation,TResult Function( String conversationId,  bool confirmedUnknown)?  deleteConversation,TResult Function( String conversationId)?  archiveConversation,TResult Function( String conversationId)?  restoreConversation,TResult Function( String conversationId,  bool includeToolPayloads)?  exportTranscript,TResult Function( String endpointId)?  discoverModels,TResult Function( String endpointId)?  probeEndpoint,TResult Function( String profileId,  String serverId)?  discoverServer,TResult Function( String conversationId,  String name,  String content)?  createArtifact,TResult Function( String conversationId,  String artifactId)?  readArtifact,TResult Function( String conversationId,  String artifactId)?  deleteArtifact,TResult Function()?  newId,TResult Function( AgentEndpointConfig endpoint)?  saveEndpoint,TResult Function( AgentServerConfig server)?  saveServer,TResult Function( AgentProfile profile)?  saveProfile,TResult Function( String profileId)?  createConversation,TResult Function( String conversationId,  String profileId)?  applyProfile,TResult Function( AgentToolKey key,  String schemaDigest,  String destinationDigest,  PermissionRule rule)?  setRule,TResult Function( String conversationId,  String text)?  startTurn,TResult Function( String runId,  String generation)?  platformPause,TResult Function( String runId,  String generation)?  platformStop,TResult Function( String runId)?  cancelRun,TResult Function( String runId)?  resumeRun,TResult Function( String runId)?  retryRun,TResult Function( AgentApprovalBinding binding)?  approve,TResult Function( String attemptId)?  deny,TResult Function( String runId,  List<AgentApprovalBinding> approve,  List<String> deny)?  decideApprovals,TResult Function( String serverId,  bool trusted)?  setServerTrust,TResult Function( String conversationId,  AgentContextSelection selection)?  summarizeContext,TResult Function( String runId,  AgentContextSelection selection)?  applyContextSelection,TResult Function( String conversationId,  String origin)?  acknowledgeContextDisclosure,required TResult orElse(),}) {final _that = this;
switch (_that) {
case AppCommand_SearchConversations() when searchConversations != null:
return searchConversations(_that.query,_that.archivedOnly,_that.offset,_that.limit);case AppCommand_RenameConversation() when renameConversation != null:
return renameConversation(_that.conversationId,_that.title);case AppCommand_DeleteConversation() when deleteConversation != null:
return deleteConversation(_that.conversationId,_that.confirmedUnknown);case AppCommand_ArchiveConversation() when archiveConversation != null:
return archiveConversation(_that.conversationId);case AppCommand_RestoreConversation() when restoreConversation != null:
return restoreConversation(_that.conversationId);case AppCommand_ExportTranscript() when exportTranscript != null:
return exportTranscript(_that.conversationId,_that.includeToolPayloads);case AppCommand_DiscoverModels() when discoverModels != null:
return discoverModels(_that.endpointId);case AppCommand_ProbeEndpoint() when probeEndpoint != null:
return probeEndpoint(_that.endpointId);case AppCommand_DiscoverServer() when discoverServer != null:
return discoverServer(_that.profileId,_that.serverId);case AppCommand_CreateArtifact() when createArtifact != null:
return createArtifact(_that.conversationId,_that.name,_that.content);case AppCommand_ReadArtifact() when readArtifact != null:
return readArtifact(_that.conversationId,_that.artifactId);case AppCommand_DeleteArtifact() when deleteArtifact != null:
return deleteArtifact(_that.conversationId,_that.artifactId);case AppCommand_NewId() when newId != null:
return newId();case AppCommand_SaveEndpoint() when saveEndpoint != null:
return saveEndpoint(_that.endpoint);case AppCommand_SaveServer() when saveServer != null:
return saveServer(_that.server);case AppCommand_SaveProfile() when saveProfile != null:
return saveProfile(_that.profile);case AppCommand_CreateConversation() when createConversation != null:
return createConversation(_that.profileId);case AppCommand_ApplyProfile() when applyProfile != null:
return applyProfile(_that.conversationId,_that.profileId);case AppCommand_SetRule() when setRule != null:
return setRule(_that.key,_that.schemaDigest,_that.destinationDigest,_that.rule);case AppCommand_StartTurn() when startTurn != null:
return startTurn(_that.conversationId,_that.text);case AppCommand_PlatformPause() when platformPause != null:
return platformPause(_that.runId,_that.generation);case AppCommand_PlatformStop() when platformStop != null:
return platformStop(_that.runId,_that.generation);case AppCommand_CancelRun() when cancelRun != null:
return cancelRun(_that.runId);case AppCommand_ResumeRun() when resumeRun != null:
return resumeRun(_that.runId);case AppCommand_RetryRun() when retryRun != null:
return retryRun(_that.runId);case AppCommand_Approve() when approve != null:
return approve(_that.binding);case AppCommand_Deny() when deny != null:
return deny(_that.attemptId);case AppCommand_DecideApprovals() when decideApprovals != null:
return decideApprovals(_that.runId,_that.approve,_that.deny);case AppCommand_SetServerTrust() when setServerTrust != null:
return setServerTrust(_that.serverId,_that.trusted);case AppCommand_SummarizeContext() when summarizeContext != null:
return summarizeContext(_that.conversationId,_that.selection);case AppCommand_ApplyContextSelection() when applyContextSelection != null:
return applyContextSelection(_that.runId,_that.selection);case AppCommand_AcknowledgeContextDisclosure() when acknowledgeContextDisclosure != null:
return acknowledgeContextDisclosure(_that.conversationId,_that.origin);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String query,  bool archivedOnly,  int offset,  int limit)  searchConversations,required TResult Function( String conversationId,  String title)  renameConversation,required TResult Function( String conversationId,  bool confirmedUnknown)  deleteConversation,required TResult Function( String conversationId)  archiveConversation,required TResult Function( String conversationId)  restoreConversation,required TResult Function( String conversationId,  bool includeToolPayloads)  exportTranscript,required TResult Function( String endpointId)  discoverModels,required TResult Function( String endpointId)  probeEndpoint,required TResult Function( String profileId,  String serverId)  discoverServer,required TResult Function( String conversationId,  String name,  String content)  createArtifact,required TResult Function( String conversationId,  String artifactId)  readArtifact,required TResult Function( String conversationId,  String artifactId)  deleteArtifact,required TResult Function()  newId,required TResult Function( AgentEndpointConfig endpoint)  saveEndpoint,required TResult Function( AgentServerConfig server)  saveServer,required TResult Function( AgentProfile profile)  saveProfile,required TResult Function( String profileId)  createConversation,required TResult Function( String conversationId,  String profileId)  applyProfile,required TResult Function( AgentToolKey key,  String schemaDigest,  String destinationDigest,  PermissionRule rule)  setRule,required TResult Function( String conversationId,  String text)  startTurn,required TResult Function( String runId,  String generation)  platformPause,required TResult Function( String runId,  String generation)  platformStop,required TResult Function( String runId)  cancelRun,required TResult Function( String runId)  resumeRun,required TResult Function( String runId)  retryRun,required TResult Function( AgentApprovalBinding binding)  approve,required TResult Function( String attemptId)  deny,required TResult Function( String runId,  List<AgentApprovalBinding> approve,  List<String> deny)  decideApprovals,required TResult Function( String serverId,  bool trusted)  setServerTrust,required TResult Function( String conversationId,  AgentContextSelection selection)  summarizeContext,required TResult Function( String runId,  AgentContextSelection selection)  applyContextSelection,required TResult Function( String conversationId,  String origin)  acknowledgeContextDisclosure,}) {final _that = this;
switch (_that) {
case AppCommand_SearchConversations():
return searchConversations(_that.query,_that.archivedOnly,_that.offset,_that.limit);case AppCommand_RenameConversation():
return renameConversation(_that.conversationId,_that.title);case AppCommand_DeleteConversation():
return deleteConversation(_that.conversationId,_that.confirmedUnknown);case AppCommand_ArchiveConversation():
return archiveConversation(_that.conversationId);case AppCommand_RestoreConversation():
return restoreConversation(_that.conversationId);case AppCommand_ExportTranscript():
return exportTranscript(_that.conversationId,_that.includeToolPayloads);case AppCommand_DiscoverModels():
return discoverModels(_that.endpointId);case AppCommand_ProbeEndpoint():
return probeEndpoint(_that.endpointId);case AppCommand_DiscoverServer():
return discoverServer(_that.profileId,_that.serverId);case AppCommand_CreateArtifact():
return createArtifact(_that.conversationId,_that.name,_that.content);case AppCommand_ReadArtifact():
return readArtifact(_that.conversationId,_that.artifactId);case AppCommand_DeleteArtifact():
return deleteArtifact(_that.conversationId,_that.artifactId);case AppCommand_NewId():
return newId();case AppCommand_SaveEndpoint():
return saveEndpoint(_that.endpoint);case AppCommand_SaveServer():
return saveServer(_that.server);case AppCommand_SaveProfile():
return saveProfile(_that.profile);case AppCommand_CreateConversation():
return createConversation(_that.profileId);case AppCommand_ApplyProfile():
return applyProfile(_that.conversationId,_that.profileId);case AppCommand_SetRule():
return setRule(_that.key,_that.schemaDigest,_that.destinationDigest,_that.rule);case AppCommand_StartTurn():
return startTurn(_that.conversationId,_that.text);case AppCommand_PlatformPause():
return platformPause(_that.runId,_that.generation);case AppCommand_PlatformStop():
return platformStop(_that.runId,_that.generation);case AppCommand_CancelRun():
return cancelRun(_that.runId);case AppCommand_ResumeRun():
return resumeRun(_that.runId);case AppCommand_RetryRun():
return retryRun(_that.runId);case AppCommand_Approve():
return approve(_that.binding);case AppCommand_Deny():
return deny(_that.attemptId);case AppCommand_DecideApprovals():
return decideApprovals(_that.runId,_that.approve,_that.deny);case AppCommand_SetServerTrust():
return setServerTrust(_that.serverId,_that.trusted);case AppCommand_SummarizeContext():
return summarizeContext(_that.conversationId,_that.selection);case AppCommand_ApplyContextSelection():
return applyContextSelection(_that.runId,_that.selection);case AppCommand_AcknowledgeContextDisclosure():
return acknowledgeContextDisclosure(_that.conversationId,_that.origin);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String query,  bool archivedOnly,  int offset,  int limit)?  searchConversations,TResult? Function( String conversationId,  String title)?  renameConversation,TResult? Function( String conversationId,  bool confirmedUnknown)?  deleteConversation,TResult? Function( String conversationId)?  archiveConversation,TResult? Function( String conversationId)?  restoreConversation,TResult? Function( String conversationId,  bool includeToolPayloads)?  exportTranscript,TResult? Function( String endpointId)?  discoverModels,TResult? Function( String endpointId)?  probeEndpoint,TResult? Function( String profileId,  String serverId)?  discoverServer,TResult? Function( String conversationId,  String name,  String content)?  createArtifact,TResult? Function( String conversationId,  String artifactId)?  readArtifact,TResult? Function( String conversationId,  String artifactId)?  deleteArtifact,TResult? Function()?  newId,TResult? Function( AgentEndpointConfig endpoint)?  saveEndpoint,TResult? Function( AgentServerConfig server)?  saveServer,TResult? Function( AgentProfile profile)?  saveProfile,TResult? Function( String profileId)?  createConversation,TResult? Function( String conversationId,  String profileId)?  applyProfile,TResult? Function( AgentToolKey key,  String schemaDigest,  String destinationDigest,  PermissionRule rule)?  setRule,TResult? Function( String conversationId,  String text)?  startTurn,TResult? Function( String runId,  String generation)?  platformPause,TResult? Function( String runId,  String generation)?  platformStop,TResult? Function( String runId)?  cancelRun,TResult? Function( String runId)?  resumeRun,TResult? Function( String runId)?  retryRun,TResult? Function( AgentApprovalBinding binding)?  approve,TResult? Function( String attemptId)?  deny,TResult? Function( String runId,  List<AgentApprovalBinding> approve,  List<String> deny)?  decideApprovals,TResult? Function( String serverId,  bool trusted)?  setServerTrust,TResult? Function( String conversationId,  AgentContextSelection selection)?  summarizeContext,TResult? Function( String runId,  AgentContextSelection selection)?  applyContextSelection,TResult? Function( String conversationId,  String origin)?  acknowledgeContextDisclosure,}) {final _that = this;
switch (_that) {
case AppCommand_SearchConversations() when searchConversations != null:
return searchConversations(_that.query,_that.archivedOnly,_that.offset,_that.limit);case AppCommand_RenameConversation() when renameConversation != null:
return renameConversation(_that.conversationId,_that.title);case AppCommand_DeleteConversation() when deleteConversation != null:
return deleteConversation(_that.conversationId,_that.confirmedUnknown);case AppCommand_ArchiveConversation() when archiveConversation != null:
return archiveConversation(_that.conversationId);case AppCommand_RestoreConversation() when restoreConversation != null:
return restoreConversation(_that.conversationId);case AppCommand_ExportTranscript() when exportTranscript != null:
return exportTranscript(_that.conversationId,_that.includeToolPayloads);case AppCommand_DiscoverModels() when discoverModels != null:
return discoverModels(_that.endpointId);case AppCommand_ProbeEndpoint() when probeEndpoint != null:
return probeEndpoint(_that.endpointId);case AppCommand_DiscoverServer() when discoverServer != null:
return discoverServer(_that.profileId,_that.serverId);case AppCommand_CreateArtifact() when createArtifact != null:
return createArtifact(_that.conversationId,_that.name,_that.content);case AppCommand_ReadArtifact() when readArtifact != null:
return readArtifact(_that.conversationId,_that.artifactId);case AppCommand_DeleteArtifact() when deleteArtifact != null:
return deleteArtifact(_that.conversationId,_that.artifactId);case AppCommand_NewId() when newId != null:
return newId();case AppCommand_SaveEndpoint() when saveEndpoint != null:
return saveEndpoint(_that.endpoint);case AppCommand_SaveServer() when saveServer != null:
return saveServer(_that.server);case AppCommand_SaveProfile() when saveProfile != null:
return saveProfile(_that.profile);case AppCommand_CreateConversation() when createConversation != null:
return createConversation(_that.profileId);case AppCommand_ApplyProfile() when applyProfile != null:
return applyProfile(_that.conversationId,_that.profileId);case AppCommand_SetRule() when setRule != null:
return setRule(_that.key,_that.schemaDigest,_that.destinationDigest,_that.rule);case AppCommand_StartTurn() when startTurn != null:
return startTurn(_that.conversationId,_that.text);case AppCommand_PlatformPause() when platformPause != null:
return platformPause(_that.runId,_that.generation);case AppCommand_PlatformStop() when platformStop != null:
return platformStop(_that.runId,_that.generation);case AppCommand_CancelRun() when cancelRun != null:
return cancelRun(_that.runId);case AppCommand_ResumeRun() when resumeRun != null:
return resumeRun(_that.runId);case AppCommand_RetryRun() when retryRun != null:
return retryRun(_that.runId);case AppCommand_Approve() when approve != null:
return approve(_that.binding);case AppCommand_Deny() when deny != null:
return deny(_that.attemptId);case AppCommand_DecideApprovals() when decideApprovals != null:
return decideApprovals(_that.runId,_that.approve,_that.deny);case AppCommand_SetServerTrust() when setServerTrust != null:
return setServerTrust(_that.serverId,_that.trusted);case AppCommand_SummarizeContext() when summarizeContext != null:
return summarizeContext(_that.conversationId,_that.selection);case AppCommand_ApplyContextSelection() when applyContextSelection != null:
return applyContextSelection(_that.runId,_that.selection);case AppCommand_AcknowledgeContextDisclosure() when acknowledgeContextDisclosure != null:
return acknowledgeContextDisclosure(_that.conversationId,_that.origin);case _:
  return null;

}
}

}

/// @nodoc


class AppCommand_SearchConversations extends AppCommand {
  const AppCommand_SearchConversations({required this.query, required this.archivedOnly, required this.offset, required this.limit}): super._();
  

 final  String query;
 final  bool archivedOnly;
 final  int offset;
 final  int limit;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_SearchConversationsCopyWith<AppCommand_SearchConversations> get copyWith => _$AppCommand_SearchConversationsCopyWithImpl<AppCommand_SearchConversations>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_SearchConversations&&(identical(other.query, query) || other.query == query)&&(identical(other.archivedOnly, archivedOnly) || other.archivedOnly == archivedOnly)&&(identical(other.offset, offset) || other.offset == offset)&&(identical(other.limit, limit) || other.limit == limit));
}


@override
int get hashCode => Object.hash(runtimeType,query,archivedOnly,offset,limit);

@override
String toString() {
  return 'AppCommand.searchConversations(query: $query, archivedOnly: $archivedOnly, offset: $offset, limit: $limit)';
}


}

/// @nodoc
abstract mixin class $AppCommand_SearchConversationsCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_SearchConversationsCopyWith(AppCommand_SearchConversations value, $Res Function(AppCommand_SearchConversations) _then) = _$AppCommand_SearchConversationsCopyWithImpl;
@useResult
$Res call({
 String query, bool archivedOnly, int offset, int limit
});




}
/// @nodoc
class _$AppCommand_SearchConversationsCopyWithImpl<$Res>
    implements $AppCommand_SearchConversationsCopyWith<$Res> {
  _$AppCommand_SearchConversationsCopyWithImpl(this._self, this._then);

  final AppCommand_SearchConversations _self;
  final $Res Function(AppCommand_SearchConversations) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? query = null,Object? archivedOnly = null,Object? offset = null,Object? limit = null,}) {
  return _then(AppCommand_SearchConversations(
query: null == query ? _self.query : query // ignore: cast_nullable_to_non_nullable
as String,archivedOnly: null == archivedOnly ? _self.archivedOnly : archivedOnly // ignore: cast_nullable_to_non_nullable
as bool,offset: null == offset ? _self.offset : offset // ignore: cast_nullable_to_non_nullable
as int,limit: null == limit ? _self.limit : limit // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

/// @nodoc


class AppCommand_RenameConversation extends AppCommand {
  const AppCommand_RenameConversation({required this.conversationId, required this.title}): super._();
  

 final  String conversationId;
 final  String title;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_RenameConversationCopyWith<AppCommand_RenameConversation> get copyWith => _$AppCommand_RenameConversationCopyWithImpl<AppCommand_RenameConversation>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_RenameConversation&&(identical(other.conversationId, conversationId) || other.conversationId == conversationId)&&(identical(other.title, title) || other.title == title));
}


@override
int get hashCode => Object.hash(runtimeType,conversationId,title);

@override
String toString() {
  return 'AppCommand.renameConversation(conversationId: $conversationId, title: $title)';
}


}

/// @nodoc
abstract mixin class $AppCommand_RenameConversationCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_RenameConversationCopyWith(AppCommand_RenameConversation value, $Res Function(AppCommand_RenameConversation) _then) = _$AppCommand_RenameConversationCopyWithImpl;
@useResult
$Res call({
 String conversationId, String title
});




}
/// @nodoc
class _$AppCommand_RenameConversationCopyWithImpl<$Res>
    implements $AppCommand_RenameConversationCopyWith<$Res> {
  _$AppCommand_RenameConversationCopyWithImpl(this._self, this._then);

  final AppCommand_RenameConversation _self;
  final $Res Function(AppCommand_RenameConversation) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? conversationId = null,Object? title = null,}) {
  return _then(AppCommand_RenameConversation(
conversationId: null == conversationId ? _self.conversationId : conversationId // ignore: cast_nullable_to_non_nullable
as String,title: null == title ? _self.title : title // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_DeleteConversation extends AppCommand {
  const AppCommand_DeleteConversation({required this.conversationId, required this.confirmedUnknown}): super._();
  

 final  String conversationId;
 final  bool confirmedUnknown;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_DeleteConversationCopyWith<AppCommand_DeleteConversation> get copyWith => _$AppCommand_DeleteConversationCopyWithImpl<AppCommand_DeleteConversation>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_DeleteConversation&&(identical(other.conversationId, conversationId) || other.conversationId == conversationId)&&(identical(other.confirmedUnknown, confirmedUnknown) || other.confirmedUnknown == confirmedUnknown));
}


@override
int get hashCode => Object.hash(runtimeType,conversationId,confirmedUnknown);

@override
String toString() {
  return 'AppCommand.deleteConversation(conversationId: $conversationId, confirmedUnknown: $confirmedUnknown)';
}


}

/// @nodoc
abstract mixin class $AppCommand_DeleteConversationCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_DeleteConversationCopyWith(AppCommand_DeleteConversation value, $Res Function(AppCommand_DeleteConversation) _then) = _$AppCommand_DeleteConversationCopyWithImpl;
@useResult
$Res call({
 String conversationId, bool confirmedUnknown
});




}
/// @nodoc
class _$AppCommand_DeleteConversationCopyWithImpl<$Res>
    implements $AppCommand_DeleteConversationCopyWith<$Res> {
  _$AppCommand_DeleteConversationCopyWithImpl(this._self, this._then);

  final AppCommand_DeleteConversation _self;
  final $Res Function(AppCommand_DeleteConversation) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? conversationId = null,Object? confirmedUnknown = null,}) {
  return _then(AppCommand_DeleteConversation(
conversationId: null == conversationId ? _self.conversationId : conversationId // ignore: cast_nullable_to_non_nullable
as String,confirmedUnknown: null == confirmedUnknown ? _self.confirmedUnknown : confirmedUnknown // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}


}

/// @nodoc


class AppCommand_ArchiveConversation extends AppCommand {
  const AppCommand_ArchiveConversation({required this.conversationId}): super._();
  

 final  String conversationId;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_ArchiveConversationCopyWith<AppCommand_ArchiveConversation> get copyWith => _$AppCommand_ArchiveConversationCopyWithImpl<AppCommand_ArchiveConversation>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_ArchiveConversation&&(identical(other.conversationId, conversationId) || other.conversationId == conversationId));
}


@override
int get hashCode => Object.hash(runtimeType,conversationId);

@override
String toString() {
  return 'AppCommand.archiveConversation(conversationId: $conversationId)';
}


}

/// @nodoc
abstract mixin class $AppCommand_ArchiveConversationCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_ArchiveConversationCopyWith(AppCommand_ArchiveConversation value, $Res Function(AppCommand_ArchiveConversation) _then) = _$AppCommand_ArchiveConversationCopyWithImpl;
@useResult
$Res call({
 String conversationId
});




}
/// @nodoc
class _$AppCommand_ArchiveConversationCopyWithImpl<$Res>
    implements $AppCommand_ArchiveConversationCopyWith<$Res> {
  _$AppCommand_ArchiveConversationCopyWithImpl(this._self, this._then);

  final AppCommand_ArchiveConversation _self;
  final $Res Function(AppCommand_ArchiveConversation) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? conversationId = null,}) {
  return _then(AppCommand_ArchiveConversation(
conversationId: null == conversationId ? _self.conversationId : conversationId // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_RestoreConversation extends AppCommand {
  const AppCommand_RestoreConversation({required this.conversationId}): super._();
  

 final  String conversationId;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_RestoreConversationCopyWith<AppCommand_RestoreConversation> get copyWith => _$AppCommand_RestoreConversationCopyWithImpl<AppCommand_RestoreConversation>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_RestoreConversation&&(identical(other.conversationId, conversationId) || other.conversationId == conversationId));
}


@override
int get hashCode => Object.hash(runtimeType,conversationId);

@override
String toString() {
  return 'AppCommand.restoreConversation(conversationId: $conversationId)';
}


}

/// @nodoc
abstract mixin class $AppCommand_RestoreConversationCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_RestoreConversationCopyWith(AppCommand_RestoreConversation value, $Res Function(AppCommand_RestoreConversation) _then) = _$AppCommand_RestoreConversationCopyWithImpl;
@useResult
$Res call({
 String conversationId
});




}
/// @nodoc
class _$AppCommand_RestoreConversationCopyWithImpl<$Res>
    implements $AppCommand_RestoreConversationCopyWith<$Res> {
  _$AppCommand_RestoreConversationCopyWithImpl(this._self, this._then);

  final AppCommand_RestoreConversation _self;
  final $Res Function(AppCommand_RestoreConversation) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? conversationId = null,}) {
  return _then(AppCommand_RestoreConversation(
conversationId: null == conversationId ? _self.conversationId : conversationId // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_ExportTranscript extends AppCommand {
  const AppCommand_ExportTranscript({required this.conversationId, required this.includeToolPayloads}): super._();
  

 final  String conversationId;
 final  bool includeToolPayloads;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_ExportTranscriptCopyWith<AppCommand_ExportTranscript> get copyWith => _$AppCommand_ExportTranscriptCopyWithImpl<AppCommand_ExportTranscript>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_ExportTranscript&&(identical(other.conversationId, conversationId) || other.conversationId == conversationId)&&(identical(other.includeToolPayloads, includeToolPayloads) || other.includeToolPayloads == includeToolPayloads));
}


@override
int get hashCode => Object.hash(runtimeType,conversationId,includeToolPayloads);

@override
String toString() {
  return 'AppCommand.exportTranscript(conversationId: $conversationId, includeToolPayloads: $includeToolPayloads)';
}


}

/// @nodoc
abstract mixin class $AppCommand_ExportTranscriptCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_ExportTranscriptCopyWith(AppCommand_ExportTranscript value, $Res Function(AppCommand_ExportTranscript) _then) = _$AppCommand_ExportTranscriptCopyWithImpl;
@useResult
$Res call({
 String conversationId, bool includeToolPayloads
});




}
/// @nodoc
class _$AppCommand_ExportTranscriptCopyWithImpl<$Res>
    implements $AppCommand_ExportTranscriptCopyWith<$Res> {
  _$AppCommand_ExportTranscriptCopyWithImpl(this._self, this._then);

  final AppCommand_ExportTranscript _self;
  final $Res Function(AppCommand_ExportTranscript) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? conversationId = null,Object? includeToolPayloads = null,}) {
  return _then(AppCommand_ExportTranscript(
conversationId: null == conversationId ? _self.conversationId : conversationId // ignore: cast_nullable_to_non_nullable
as String,includeToolPayloads: null == includeToolPayloads ? _self.includeToolPayloads : includeToolPayloads // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}


}

/// @nodoc


class AppCommand_DiscoverModels extends AppCommand {
  const AppCommand_DiscoverModels({required this.endpointId}): super._();
  

 final  String endpointId;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_DiscoverModelsCopyWith<AppCommand_DiscoverModels> get copyWith => _$AppCommand_DiscoverModelsCopyWithImpl<AppCommand_DiscoverModels>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_DiscoverModels&&(identical(other.endpointId, endpointId) || other.endpointId == endpointId));
}


@override
int get hashCode => Object.hash(runtimeType,endpointId);

@override
String toString() {
  return 'AppCommand.discoverModels(endpointId: $endpointId)';
}


}

/// @nodoc
abstract mixin class $AppCommand_DiscoverModelsCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_DiscoverModelsCopyWith(AppCommand_DiscoverModels value, $Res Function(AppCommand_DiscoverModels) _then) = _$AppCommand_DiscoverModelsCopyWithImpl;
@useResult
$Res call({
 String endpointId
});




}
/// @nodoc
class _$AppCommand_DiscoverModelsCopyWithImpl<$Res>
    implements $AppCommand_DiscoverModelsCopyWith<$Res> {
  _$AppCommand_DiscoverModelsCopyWithImpl(this._self, this._then);

  final AppCommand_DiscoverModels _self;
  final $Res Function(AppCommand_DiscoverModels) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? endpointId = null,}) {
  return _then(AppCommand_DiscoverModels(
endpointId: null == endpointId ? _self.endpointId : endpointId // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_ProbeEndpoint extends AppCommand {
  const AppCommand_ProbeEndpoint({required this.endpointId}): super._();
  

 final  String endpointId;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_ProbeEndpointCopyWith<AppCommand_ProbeEndpoint> get copyWith => _$AppCommand_ProbeEndpointCopyWithImpl<AppCommand_ProbeEndpoint>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_ProbeEndpoint&&(identical(other.endpointId, endpointId) || other.endpointId == endpointId));
}


@override
int get hashCode => Object.hash(runtimeType,endpointId);

@override
String toString() {
  return 'AppCommand.probeEndpoint(endpointId: $endpointId)';
}


}

/// @nodoc
abstract mixin class $AppCommand_ProbeEndpointCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_ProbeEndpointCopyWith(AppCommand_ProbeEndpoint value, $Res Function(AppCommand_ProbeEndpoint) _then) = _$AppCommand_ProbeEndpointCopyWithImpl;
@useResult
$Res call({
 String endpointId
});




}
/// @nodoc
class _$AppCommand_ProbeEndpointCopyWithImpl<$Res>
    implements $AppCommand_ProbeEndpointCopyWith<$Res> {
  _$AppCommand_ProbeEndpointCopyWithImpl(this._self, this._then);

  final AppCommand_ProbeEndpoint _self;
  final $Res Function(AppCommand_ProbeEndpoint) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? endpointId = null,}) {
  return _then(AppCommand_ProbeEndpoint(
endpointId: null == endpointId ? _self.endpointId : endpointId // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_DiscoverServer extends AppCommand {
  const AppCommand_DiscoverServer({required this.profileId, required this.serverId}): super._();
  

 final  String profileId;
 final  String serverId;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_DiscoverServerCopyWith<AppCommand_DiscoverServer> get copyWith => _$AppCommand_DiscoverServerCopyWithImpl<AppCommand_DiscoverServer>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_DiscoverServer&&(identical(other.profileId, profileId) || other.profileId == profileId)&&(identical(other.serverId, serverId) || other.serverId == serverId));
}


@override
int get hashCode => Object.hash(runtimeType,profileId,serverId);

@override
String toString() {
  return 'AppCommand.discoverServer(profileId: $profileId, serverId: $serverId)';
}


}

/// @nodoc
abstract mixin class $AppCommand_DiscoverServerCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_DiscoverServerCopyWith(AppCommand_DiscoverServer value, $Res Function(AppCommand_DiscoverServer) _then) = _$AppCommand_DiscoverServerCopyWithImpl;
@useResult
$Res call({
 String profileId, String serverId
});




}
/// @nodoc
class _$AppCommand_DiscoverServerCopyWithImpl<$Res>
    implements $AppCommand_DiscoverServerCopyWith<$Res> {
  _$AppCommand_DiscoverServerCopyWithImpl(this._self, this._then);

  final AppCommand_DiscoverServer _self;
  final $Res Function(AppCommand_DiscoverServer) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? profileId = null,Object? serverId = null,}) {
  return _then(AppCommand_DiscoverServer(
profileId: null == profileId ? _self.profileId : profileId // ignore: cast_nullable_to_non_nullable
as String,serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_CreateArtifact extends AppCommand {
  const AppCommand_CreateArtifact({required this.conversationId, required this.name, required this.content}): super._();
  

 final  String conversationId;
 final  String name;
 final  String content;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_CreateArtifactCopyWith<AppCommand_CreateArtifact> get copyWith => _$AppCommand_CreateArtifactCopyWithImpl<AppCommand_CreateArtifact>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_CreateArtifact&&(identical(other.conversationId, conversationId) || other.conversationId == conversationId)&&(identical(other.name, name) || other.name == name)&&(identical(other.content, content) || other.content == content));
}


@override
int get hashCode => Object.hash(runtimeType,conversationId,name,content);

@override
String toString() {
  return 'AppCommand.createArtifact(conversationId: $conversationId, name: $name, content: $content)';
}


}

/// @nodoc
abstract mixin class $AppCommand_CreateArtifactCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_CreateArtifactCopyWith(AppCommand_CreateArtifact value, $Res Function(AppCommand_CreateArtifact) _then) = _$AppCommand_CreateArtifactCopyWithImpl;
@useResult
$Res call({
 String conversationId, String name, String content
});




}
/// @nodoc
class _$AppCommand_CreateArtifactCopyWithImpl<$Res>
    implements $AppCommand_CreateArtifactCopyWith<$Res> {
  _$AppCommand_CreateArtifactCopyWithImpl(this._self, this._then);

  final AppCommand_CreateArtifact _self;
  final $Res Function(AppCommand_CreateArtifact) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? conversationId = null,Object? name = null,Object? content = null,}) {
  return _then(AppCommand_CreateArtifact(
conversationId: null == conversationId ? _self.conversationId : conversationId // ignore: cast_nullable_to_non_nullable
as String,name: null == name ? _self.name : name // ignore: cast_nullable_to_non_nullable
as String,content: null == content ? _self.content : content // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_ReadArtifact extends AppCommand {
  const AppCommand_ReadArtifact({required this.conversationId, required this.artifactId}): super._();
  

 final  String conversationId;
 final  String artifactId;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_ReadArtifactCopyWith<AppCommand_ReadArtifact> get copyWith => _$AppCommand_ReadArtifactCopyWithImpl<AppCommand_ReadArtifact>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_ReadArtifact&&(identical(other.conversationId, conversationId) || other.conversationId == conversationId)&&(identical(other.artifactId, artifactId) || other.artifactId == artifactId));
}


@override
int get hashCode => Object.hash(runtimeType,conversationId,artifactId);

@override
String toString() {
  return 'AppCommand.readArtifact(conversationId: $conversationId, artifactId: $artifactId)';
}


}

/// @nodoc
abstract mixin class $AppCommand_ReadArtifactCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_ReadArtifactCopyWith(AppCommand_ReadArtifact value, $Res Function(AppCommand_ReadArtifact) _then) = _$AppCommand_ReadArtifactCopyWithImpl;
@useResult
$Res call({
 String conversationId, String artifactId
});




}
/// @nodoc
class _$AppCommand_ReadArtifactCopyWithImpl<$Res>
    implements $AppCommand_ReadArtifactCopyWith<$Res> {
  _$AppCommand_ReadArtifactCopyWithImpl(this._self, this._then);

  final AppCommand_ReadArtifact _self;
  final $Res Function(AppCommand_ReadArtifact) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? conversationId = null,Object? artifactId = null,}) {
  return _then(AppCommand_ReadArtifact(
conversationId: null == conversationId ? _self.conversationId : conversationId // ignore: cast_nullable_to_non_nullable
as String,artifactId: null == artifactId ? _self.artifactId : artifactId // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_DeleteArtifact extends AppCommand {
  const AppCommand_DeleteArtifact({required this.conversationId, required this.artifactId}): super._();
  

 final  String conversationId;
 final  String artifactId;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_DeleteArtifactCopyWith<AppCommand_DeleteArtifact> get copyWith => _$AppCommand_DeleteArtifactCopyWithImpl<AppCommand_DeleteArtifact>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_DeleteArtifact&&(identical(other.conversationId, conversationId) || other.conversationId == conversationId)&&(identical(other.artifactId, artifactId) || other.artifactId == artifactId));
}


@override
int get hashCode => Object.hash(runtimeType,conversationId,artifactId);

@override
String toString() {
  return 'AppCommand.deleteArtifact(conversationId: $conversationId, artifactId: $artifactId)';
}


}

/// @nodoc
abstract mixin class $AppCommand_DeleteArtifactCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_DeleteArtifactCopyWith(AppCommand_DeleteArtifact value, $Res Function(AppCommand_DeleteArtifact) _then) = _$AppCommand_DeleteArtifactCopyWithImpl;
@useResult
$Res call({
 String conversationId, String artifactId
});




}
/// @nodoc
class _$AppCommand_DeleteArtifactCopyWithImpl<$Res>
    implements $AppCommand_DeleteArtifactCopyWith<$Res> {
  _$AppCommand_DeleteArtifactCopyWithImpl(this._self, this._then);

  final AppCommand_DeleteArtifact _self;
  final $Res Function(AppCommand_DeleteArtifact) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? conversationId = null,Object? artifactId = null,}) {
  return _then(AppCommand_DeleteArtifact(
conversationId: null == conversationId ? _self.conversationId : conversationId // ignore: cast_nullable_to_non_nullable
as String,artifactId: null == artifactId ? _self.artifactId : artifactId // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_NewId extends AppCommand {
  const AppCommand_NewId(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_NewId);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'AppCommand.newId()';
}


}




/// @nodoc


class AppCommand_SaveEndpoint extends AppCommand {
  const AppCommand_SaveEndpoint({required this.endpoint}): super._();
  

 final  AgentEndpointConfig endpoint;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_SaveEndpointCopyWith<AppCommand_SaveEndpoint> get copyWith => _$AppCommand_SaveEndpointCopyWithImpl<AppCommand_SaveEndpoint>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_SaveEndpoint&&(identical(other.endpoint, endpoint) || other.endpoint == endpoint));
}


@override
int get hashCode => Object.hash(runtimeType,endpoint);

@override
String toString() {
  return 'AppCommand.saveEndpoint(endpoint: $endpoint)';
}


}

/// @nodoc
abstract mixin class $AppCommand_SaveEndpointCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_SaveEndpointCopyWith(AppCommand_SaveEndpoint value, $Res Function(AppCommand_SaveEndpoint) _then) = _$AppCommand_SaveEndpointCopyWithImpl;
@useResult
$Res call({
 AgentEndpointConfig endpoint
});




}
/// @nodoc
class _$AppCommand_SaveEndpointCopyWithImpl<$Res>
    implements $AppCommand_SaveEndpointCopyWith<$Res> {
  _$AppCommand_SaveEndpointCopyWithImpl(this._self, this._then);

  final AppCommand_SaveEndpoint _self;
  final $Res Function(AppCommand_SaveEndpoint) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? endpoint = null,}) {
  return _then(AppCommand_SaveEndpoint(
endpoint: null == endpoint ? _self.endpoint : endpoint // ignore: cast_nullable_to_non_nullable
as AgentEndpointConfig,
  ));
}


}

/// @nodoc


class AppCommand_SaveServer extends AppCommand {
  const AppCommand_SaveServer({required this.server}): super._();
  

 final  AgentServerConfig server;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_SaveServerCopyWith<AppCommand_SaveServer> get copyWith => _$AppCommand_SaveServerCopyWithImpl<AppCommand_SaveServer>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_SaveServer&&(identical(other.server, server) || other.server == server));
}


@override
int get hashCode => Object.hash(runtimeType,server);

@override
String toString() {
  return 'AppCommand.saveServer(server: $server)';
}


}

/// @nodoc
abstract mixin class $AppCommand_SaveServerCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_SaveServerCopyWith(AppCommand_SaveServer value, $Res Function(AppCommand_SaveServer) _then) = _$AppCommand_SaveServerCopyWithImpl;
@useResult
$Res call({
 AgentServerConfig server
});




}
/// @nodoc
class _$AppCommand_SaveServerCopyWithImpl<$Res>
    implements $AppCommand_SaveServerCopyWith<$Res> {
  _$AppCommand_SaveServerCopyWithImpl(this._self, this._then);

  final AppCommand_SaveServer _self;
  final $Res Function(AppCommand_SaveServer) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? server = null,}) {
  return _then(AppCommand_SaveServer(
server: null == server ? _self.server : server // ignore: cast_nullable_to_non_nullable
as AgentServerConfig,
  ));
}


}

/// @nodoc


class AppCommand_SaveProfile extends AppCommand {
  const AppCommand_SaveProfile({required this.profile}): super._();
  

 final  AgentProfile profile;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_SaveProfileCopyWith<AppCommand_SaveProfile> get copyWith => _$AppCommand_SaveProfileCopyWithImpl<AppCommand_SaveProfile>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_SaveProfile&&(identical(other.profile, profile) || other.profile == profile));
}


@override
int get hashCode => Object.hash(runtimeType,profile);

@override
String toString() {
  return 'AppCommand.saveProfile(profile: $profile)';
}


}

/// @nodoc
abstract mixin class $AppCommand_SaveProfileCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_SaveProfileCopyWith(AppCommand_SaveProfile value, $Res Function(AppCommand_SaveProfile) _then) = _$AppCommand_SaveProfileCopyWithImpl;
@useResult
$Res call({
 AgentProfile profile
});




}
/// @nodoc
class _$AppCommand_SaveProfileCopyWithImpl<$Res>
    implements $AppCommand_SaveProfileCopyWith<$Res> {
  _$AppCommand_SaveProfileCopyWithImpl(this._self, this._then);

  final AppCommand_SaveProfile _self;
  final $Res Function(AppCommand_SaveProfile) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? profile = null,}) {
  return _then(AppCommand_SaveProfile(
profile: null == profile ? _self.profile : profile // ignore: cast_nullable_to_non_nullable
as AgentProfile,
  ));
}


}

/// @nodoc


class AppCommand_CreateConversation extends AppCommand {
  const AppCommand_CreateConversation({required this.profileId}): super._();
  

 final  String profileId;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_CreateConversationCopyWith<AppCommand_CreateConversation> get copyWith => _$AppCommand_CreateConversationCopyWithImpl<AppCommand_CreateConversation>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_CreateConversation&&(identical(other.profileId, profileId) || other.profileId == profileId));
}


@override
int get hashCode => Object.hash(runtimeType,profileId);

@override
String toString() {
  return 'AppCommand.createConversation(profileId: $profileId)';
}


}

/// @nodoc
abstract mixin class $AppCommand_CreateConversationCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_CreateConversationCopyWith(AppCommand_CreateConversation value, $Res Function(AppCommand_CreateConversation) _then) = _$AppCommand_CreateConversationCopyWithImpl;
@useResult
$Res call({
 String profileId
});




}
/// @nodoc
class _$AppCommand_CreateConversationCopyWithImpl<$Res>
    implements $AppCommand_CreateConversationCopyWith<$Res> {
  _$AppCommand_CreateConversationCopyWithImpl(this._self, this._then);

  final AppCommand_CreateConversation _self;
  final $Res Function(AppCommand_CreateConversation) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? profileId = null,}) {
  return _then(AppCommand_CreateConversation(
profileId: null == profileId ? _self.profileId : profileId // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_ApplyProfile extends AppCommand {
  const AppCommand_ApplyProfile({required this.conversationId, required this.profileId}): super._();
  

 final  String conversationId;
 final  String profileId;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_ApplyProfileCopyWith<AppCommand_ApplyProfile> get copyWith => _$AppCommand_ApplyProfileCopyWithImpl<AppCommand_ApplyProfile>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_ApplyProfile&&(identical(other.conversationId, conversationId) || other.conversationId == conversationId)&&(identical(other.profileId, profileId) || other.profileId == profileId));
}


@override
int get hashCode => Object.hash(runtimeType,conversationId,profileId);

@override
String toString() {
  return 'AppCommand.applyProfile(conversationId: $conversationId, profileId: $profileId)';
}


}

/// @nodoc
abstract mixin class $AppCommand_ApplyProfileCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_ApplyProfileCopyWith(AppCommand_ApplyProfile value, $Res Function(AppCommand_ApplyProfile) _then) = _$AppCommand_ApplyProfileCopyWithImpl;
@useResult
$Res call({
 String conversationId, String profileId
});




}
/// @nodoc
class _$AppCommand_ApplyProfileCopyWithImpl<$Res>
    implements $AppCommand_ApplyProfileCopyWith<$Res> {
  _$AppCommand_ApplyProfileCopyWithImpl(this._self, this._then);

  final AppCommand_ApplyProfile _self;
  final $Res Function(AppCommand_ApplyProfile) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? conversationId = null,Object? profileId = null,}) {
  return _then(AppCommand_ApplyProfile(
conversationId: null == conversationId ? _self.conversationId : conversationId // ignore: cast_nullable_to_non_nullable
as String,profileId: null == profileId ? _self.profileId : profileId // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_SetRule extends AppCommand {
  const AppCommand_SetRule({required this.key, required this.schemaDigest, required this.destinationDigest, required this.rule}): super._();
  

 final  AgentToolKey key;
 final  String schemaDigest;
 final  String destinationDigest;
 final  PermissionRule rule;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_SetRuleCopyWith<AppCommand_SetRule> get copyWith => _$AppCommand_SetRuleCopyWithImpl<AppCommand_SetRule>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_SetRule&&(identical(other.key, key) || other.key == key)&&(identical(other.schemaDigest, schemaDigest) || other.schemaDigest == schemaDigest)&&(identical(other.destinationDigest, destinationDigest) || other.destinationDigest == destinationDigest)&&(identical(other.rule, rule) || other.rule == rule));
}


@override
int get hashCode => Object.hash(runtimeType,key,schemaDigest,destinationDigest,rule);

@override
String toString() {
  return 'AppCommand.setRule(key: $key, schemaDigest: $schemaDigest, destinationDigest: $destinationDigest, rule: $rule)';
}


}

/// @nodoc
abstract mixin class $AppCommand_SetRuleCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_SetRuleCopyWith(AppCommand_SetRule value, $Res Function(AppCommand_SetRule) _then) = _$AppCommand_SetRuleCopyWithImpl;
@useResult
$Res call({
 AgentToolKey key, String schemaDigest, String destinationDigest, PermissionRule rule
});




}
/// @nodoc
class _$AppCommand_SetRuleCopyWithImpl<$Res>
    implements $AppCommand_SetRuleCopyWith<$Res> {
  _$AppCommand_SetRuleCopyWithImpl(this._self, this._then);

  final AppCommand_SetRule _self;
  final $Res Function(AppCommand_SetRule) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? key = null,Object? schemaDigest = null,Object? destinationDigest = null,Object? rule = null,}) {
  return _then(AppCommand_SetRule(
key: null == key ? _self.key : key // ignore: cast_nullable_to_non_nullable
as AgentToolKey,schemaDigest: null == schemaDigest ? _self.schemaDigest : schemaDigest // ignore: cast_nullable_to_non_nullable
as String,destinationDigest: null == destinationDigest ? _self.destinationDigest : destinationDigest // ignore: cast_nullable_to_non_nullable
as String,rule: null == rule ? _self.rule : rule // ignore: cast_nullable_to_non_nullable
as PermissionRule,
  ));
}


}

/// @nodoc


class AppCommand_StartTurn extends AppCommand {
  const AppCommand_StartTurn({required this.conversationId, required this.text}): super._();
  

 final  String conversationId;
 final  String text;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_StartTurnCopyWith<AppCommand_StartTurn> get copyWith => _$AppCommand_StartTurnCopyWithImpl<AppCommand_StartTurn>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_StartTurn&&(identical(other.conversationId, conversationId) || other.conversationId == conversationId)&&(identical(other.text, text) || other.text == text));
}


@override
int get hashCode => Object.hash(runtimeType,conversationId,text);

@override
String toString() {
  return 'AppCommand.startTurn(conversationId: $conversationId, text: $text)';
}


}

/// @nodoc
abstract mixin class $AppCommand_StartTurnCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_StartTurnCopyWith(AppCommand_StartTurn value, $Res Function(AppCommand_StartTurn) _then) = _$AppCommand_StartTurnCopyWithImpl;
@useResult
$Res call({
 String conversationId, String text
});




}
/// @nodoc
class _$AppCommand_StartTurnCopyWithImpl<$Res>
    implements $AppCommand_StartTurnCopyWith<$Res> {
  _$AppCommand_StartTurnCopyWithImpl(this._self, this._then);

  final AppCommand_StartTurn _self;
  final $Res Function(AppCommand_StartTurn) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? conversationId = null,Object? text = null,}) {
  return _then(AppCommand_StartTurn(
conversationId: null == conversationId ? _self.conversationId : conversationId // ignore: cast_nullable_to_non_nullable
as String,text: null == text ? _self.text : text // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_PlatformPause extends AppCommand {
  const AppCommand_PlatformPause({required this.runId, required this.generation}): super._();
  

 final  String runId;
 final  String generation;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_PlatformPauseCopyWith<AppCommand_PlatformPause> get copyWith => _$AppCommand_PlatformPauseCopyWithImpl<AppCommand_PlatformPause>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_PlatformPause&&(identical(other.runId, runId) || other.runId == runId)&&(identical(other.generation, generation) || other.generation == generation));
}


@override
int get hashCode => Object.hash(runtimeType,runId,generation);

@override
String toString() {
  return 'AppCommand.platformPause(runId: $runId, generation: $generation)';
}


}

/// @nodoc
abstract mixin class $AppCommand_PlatformPauseCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_PlatformPauseCopyWith(AppCommand_PlatformPause value, $Res Function(AppCommand_PlatformPause) _then) = _$AppCommand_PlatformPauseCopyWithImpl;
@useResult
$Res call({
 String runId, String generation
});




}
/// @nodoc
class _$AppCommand_PlatformPauseCopyWithImpl<$Res>
    implements $AppCommand_PlatformPauseCopyWith<$Res> {
  _$AppCommand_PlatformPauseCopyWithImpl(this._self, this._then);

  final AppCommand_PlatformPause _self;
  final $Res Function(AppCommand_PlatformPause) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? runId = null,Object? generation = null,}) {
  return _then(AppCommand_PlatformPause(
runId: null == runId ? _self.runId : runId // ignore: cast_nullable_to_non_nullable
as String,generation: null == generation ? _self.generation : generation // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_PlatformStop extends AppCommand {
  const AppCommand_PlatformStop({required this.runId, required this.generation}): super._();
  

 final  String runId;
 final  String generation;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_PlatformStopCopyWith<AppCommand_PlatformStop> get copyWith => _$AppCommand_PlatformStopCopyWithImpl<AppCommand_PlatformStop>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_PlatformStop&&(identical(other.runId, runId) || other.runId == runId)&&(identical(other.generation, generation) || other.generation == generation));
}


@override
int get hashCode => Object.hash(runtimeType,runId,generation);

@override
String toString() {
  return 'AppCommand.platformStop(runId: $runId, generation: $generation)';
}


}

/// @nodoc
abstract mixin class $AppCommand_PlatformStopCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_PlatformStopCopyWith(AppCommand_PlatformStop value, $Res Function(AppCommand_PlatformStop) _then) = _$AppCommand_PlatformStopCopyWithImpl;
@useResult
$Res call({
 String runId, String generation
});




}
/// @nodoc
class _$AppCommand_PlatformStopCopyWithImpl<$Res>
    implements $AppCommand_PlatformStopCopyWith<$Res> {
  _$AppCommand_PlatformStopCopyWithImpl(this._self, this._then);

  final AppCommand_PlatformStop _self;
  final $Res Function(AppCommand_PlatformStop) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? runId = null,Object? generation = null,}) {
  return _then(AppCommand_PlatformStop(
runId: null == runId ? _self.runId : runId // ignore: cast_nullable_to_non_nullable
as String,generation: null == generation ? _self.generation : generation // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_CancelRun extends AppCommand {
  const AppCommand_CancelRun({required this.runId}): super._();
  

 final  String runId;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_CancelRunCopyWith<AppCommand_CancelRun> get copyWith => _$AppCommand_CancelRunCopyWithImpl<AppCommand_CancelRun>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_CancelRun&&(identical(other.runId, runId) || other.runId == runId));
}


@override
int get hashCode => Object.hash(runtimeType,runId);

@override
String toString() {
  return 'AppCommand.cancelRun(runId: $runId)';
}


}

/// @nodoc
abstract mixin class $AppCommand_CancelRunCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_CancelRunCopyWith(AppCommand_CancelRun value, $Res Function(AppCommand_CancelRun) _then) = _$AppCommand_CancelRunCopyWithImpl;
@useResult
$Res call({
 String runId
});




}
/// @nodoc
class _$AppCommand_CancelRunCopyWithImpl<$Res>
    implements $AppCommand_CancelRunCopyWith<$Res> {
  _$AppCommand_CancelRunCopyWithImpl(this._self, this._then);

  final AppCommand_CancelRun _self;
  final $Res Function(AppCommand_CancelRun) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? runId = null,}) {
  return _then(AppCommand_CancelRun(
runId: null == runId ? _self.runId : runId // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_ResumeRun extends AppCommand {
  const AppCommand_ResumeRun({required this.runId}): super._();
  

 final  String runId;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_ResumeRunCopyWith<AppCommand_ResumeRun> get copyWith => _$AppCommand_ResumeRunCopyWithImpl<AppCommand_ResumeRun>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_ResumeRun&&(identical(other.runId, runId) || other.runId == runId));
}


@override
int get hashCode => Object.hash(runtimeType,runId);

@override
String toString() {
  return 'AppCommand.resumeRun(runId: $runId)';
}


}

/// @nodoc
abstract mixin class $AppCommand_ResumeRunCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_ResumeRunCopyWith(AppCommand_ResumeRun value, $Res Function(AppCommand_ResumeRun) _then) = _$AppCommand_ResumeRunCopyWithImpl;
@useResult
$Res call({
 String runId
});




}
/// @nodoc
class _$AppCommand_ResumeRunCopyWithImpl<$Res>
    implements $AppCommand_ResumeRunCopyWith<$Res> {
  _$AppCommand_ResumeRunCopyWithImpl(this._self, this._then);

  final AppCommand_ResumeRun _self;
  final $Res Function(AppCommand_ResumeRun) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? runId = null,}) {
  return _then(AppCommand_ResumeRun(
runId: null == runId ? _self.runId : runId // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_RetryRun extends AppCommand {
  const AppCommand_RetryRun({required this.runId}): super._();
  

 final  String runId;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_RetryRunCopyWith<AppCommand_RetryRun> get copyWith => _$AppCommand_RetryRunCopyWithImpl<AppCommand_RetryRun>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_RetryRun&&(identical(other.runId, runId) || other.runId == runId));
}


@override
int get hashCode => Object.hash(runtimeType,runId);

@override
String toString() {
  return 'AppCommand.retryRun(runId: $runId)';
}


}

/// @nodoc
abstract mixin class $AppCommand_RetryRunCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_RetryRunCopyWith(AppCommand_RetryRun value, $Res Function(AppCommand_RetryRun) _then) = _$AppCommand_RetryRunCopyWithImpl;
@useResult
$Res call({
 String runId
});




}
/// @nodoc
class _$AppCommand_RetryRunCopyWithImpl<$Res>
    implements $AppCommand_RetryRunCopyWith<$Res> {
  _$AppCommand_RetryRunCopyWithImpl(this._self, this._then);

  final AppCommand_RetryRun _self;
  final $Res Function(AppCommand_RetryRun) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? runId = null,}) {
  return _then(AppCommand_RetryRun(
runId: null == runId ? _self.runId : runId // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_Approve extends AppCommand {
  const AppCommand_Approve({required this.binding}): super._();
  

 final  AgentApprovalBinding binding;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_ApproveCopyWith<AppCommand_Approve> get copyWith => _$AppCommand_ApproveCopyWithImpl<AppCommand_Approve>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_Approve&&(identical(other.binding, binding) || other.binding == binding));
}


@override
int get hashCode => Object.hash(runtimeType,binding);

@override
String toString() {
  return 'AppCommand.approve(binding: $binding)';
}


}

/// @nodoc
abstract mixin class $AppCommand_ApproveCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_ApproveCopyWith(AppCommand_Approve value, $Res Function(AppCommand_Approve) _then) = _$AppCommand_ApproveCopyWithImpl;
@useResult
$Res call({
 AgentApprovalBinding binding
});




}
/// @nodoc
class _$AppCommand_ApproveCopyWithImpl<$Res>
    implements $AppCommand_ApproveCopyWith<$Res> {
  _$AppCommand_ApproveCopyWithImpl(this._self, this._then);

  final AppCommand_Approve _self;
  final $Res Function(AppCommand_Approve) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? binding = null,}) {
  return _then(AppCommand_Approve(
binding: null == binding ? _self.binding : binding // ignore: cast_nullable_to_non_nullable
as AgentApprovalBinding,
  ));
}


}

/// @nodoc


class AppCommand_Deny extends AppCommand {
  const AppCommand_Deny({required this.attemptId}): super._();
  

 final  String attemptId;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_DenyCopyWith<AppCommand_Deny> get copyWith => _$AppCommand_DenyCopyWithImpl<AppCommand_Deny>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_Deny&&(identical(other.attemptId, attemptId) || other.attemptId == attemptId));
}


@override
int get hashCode => Object.hash(runtimeType,attemptId);

@override
String toString() {
  return 'AppCommand.deny(attemptId: $attemptId)';
}


}

/// @nodoc
abstract mixin class $AppCommand_DenyCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_DenyCopyWith(AppCommand_Deny value, $Res Function(AppCommand_Deny) _then) = _$AppCommand_DenyCopyWithImpl;
@useResult
$Res call({
 String attemptId
});




}
/// @nodoc
class _$AppCommand_DenyCopyWithImpl<$Res>
    implements $AppCommand_DenyCopyWith<$Res> {
  _$AppCommand_DenyCopyWithImpl(this._self, this._then);

  final AppCommand_Deny _self;
  final $Res Function(AppCommand_Deny) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? attemptId = null,}) {
  return _then(AppCommand_Deny(
attemptId: null == attemptId ? _self.attemptId : attemptId // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class AppCommand_DecideApprovals extends AppCommand {
  const AppCommand_DecideApprovals({required this.runId, required final  List<AgentApprovalBinding> approve, required final  List<String> deny}): _approve = approve,_deny = deny,super._();
  

 final  String runId;
 final  List<AgentApprovalBinding> _approve;
 List<AgentApprovalBinding> get approve {
  if (_approve is EqualUnmodifiableListView) return _approve;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_approve);
}

 final  List<String> _deny;
 List<String> get deny {
  if (_deny is EqualUnmodifiableListView) return _deny;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_deny);
}


/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_DecideApprovalsCopyWith<AppCommand_DecideApprovals> get copyWith => _$AppCommand_DecideApprovalsCopyWithImpl<AppCommand_DecideApprovals>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_DecideApprovals&&(identical(other.runId, runId) || other.runId == runId)&&const DeepCollectionEquality().equals(other._approve, _approve)&&const DeepCollectionEquality().equals(other._deny, _deny));
}


@override
int get hashCode => Object.hash(runtimeType,runId,const DeepCollectionEquality().hash(_approve),const DeepCollectionEquality().hash(_deny));

@override
String toString() {
  return 'AppCommand.decideApprovals(runId: $runId, approve: $approve, deny: $deny)';
}


}

/// @nodoc
abstract mixin class $AppCommand_DecideApprovalsCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_DecideApprovalsCopyWith(AppCommand_DecideApprovals value, $Res Function(AppCommand_DecideApprovals) _then) = _$AppCommand_DecideApprovalsCopyWithImpl;
@useResult
$Res call({
 String runId, List<AgentApprovalBinding> approve, List<String> deny
});




}
/// @nodoc
class _$AppCommand_DecideApprovalsCopyWithImpl<$Res>
    implements $AppCommand_DecideApprovalsCopyWith<$Res> {
  _$AppCommand_DecideApprovalsCopyWithImpl(this._self, this._then);

  final AppCommand_DecideApprovals _self;
  final $Res Function(AppCommand_DecideApprovals) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? runId = null,Object? approve = null,Object? deny = null,}) {
  return _then(AppCommand_DecideApprovals(
runId: null == runId ? _self.runId : runId // ignore: cast_nullable_to_non_nullable
as String,approve: null == approve ? _self._approve : approve // ignore: cast_nullable_to_non_nullable
as List<AgentApprovalBinding>,deny: null == deny ? _self._deny : deny // ignore: cast_nullable_to_non_nullable
as List<String>,
  ));
}


}

/// @nodoc


class AppCommand_SetServerTrust extends AppCommand {
  const AppCommand_SetServerTrust({required this.serverId, required this.trusted}): super._();
  

 final  String serverId;
 final  bool trusted;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_SetServerTrustCopyWith<AppCommand_SetServerTrust> get copyWith => _$AppCommand_SetServerTrustCopyWithImpl<AppCommand_SetServerTrust>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_SetServerTrust&&(identical(other.serverId, serverId) || other.serverId == serverId)&&(identical(other.trusted, trusted) || other.trusted == trusted));
}


@override
int get hashCode => Object.hash(runtimeType,serverId,trusted);

@override
String toString() {
  return 'AppCommand.setServerTrust(serverId: $serverId, trusted: $trusted)';
}


}

/// @nodoc
abstract mixin class $AppCommand_SetServerTrustCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_SetServerTrustCopyWith(AppCommand_SetServerTrust value, $Res Function(AppCommand_SetServerTrust) _then) = _$AppCommand_SetServerTrustCopyWithImpl;
@useResult
$Res call({
 String serverId, bool trusted
});




}
/// @nodoc
class _$AppCommand_SetServerTrustCopyWithImpl<$Res>
    implements $AppCommand_SetServerTrustCopyWith<$Res> {
  _$AppCommand_SetServerTrustCopyWithImpl(this._self, this._then);

  final AppCommand_SetServerTrust _self;
  final $Res Function(AppCommand_SetServerTrust) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverId = null,Object? trusted = null,}) {
  return _then(AppCommand_SetServerTrust(
serverId: null == serverId ? _self.serverId : serverId // ignore: cast_nullable_to_non_nullable
as String,trusted: null == trusted ? _self.trusted : trusted // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}


}

/// @nodoc


class AppCommand_SummarizeContext extends AppCommand {
  const AppCommand_SummarizeContext({required this.conversationId, required this.selection}): super._();
  

 final  String conversationId;
 final  AgentContextSelection selection;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_SummarizeContextCopyWith<AppCommand_SummarizeContext> get copyWith => _$AppCommand_SummarizeContextCopyWithImpl<AppCommand_SummarizeContext>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_SummarizeContext&&(identical(other.conversationId, conversationId) || other.conversationId == conversationId)&&(identical(other.selection, selection) || other.selection == selection));
}


@override
int get hashCode => Object.hash(runtimeType,conversationId,selection);

@override
String toString() {
  return 'AppCommand.summarizeContext(conversationId: $conversationId, selection: $selection)';
}


}

/// @nodoc
abstract mixin class $AppCommand_SummarizeContextCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_SummarizeContextCopyWith(AppCommand_SummarizeContext value, $Res Function(AppCommand_SummarizeContext) _then) = _$AppCommand_SummarizeContextCopyWithImpl;
@useResult
$Res call({
 String conversationId, AgentContextSelection selection
});




}
/// @nodoc
class _$AppCommand_SummarizeContextCopyWithImpl<$Res>
    implements $AppCommand_SummarizeContextCopyWith<$Res> {
  _$AppCommand_SummarizeContextCopyWithImpl(this._self, this._then);

  final AppCommand_SummarizeContext _self;
  final $Res Function(AppCommand_SummarizeContext) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? conversationId = null,Object? selection = null,}) {
  return _then(AppCommand_SummarizeContext(
conversationId: null == conversationId ? _self.conversationId : conversationId // ignore: cast_nullable_to_non_nullable
as String,selection: null == selection ? _self.selection : selection // ignore: cast_nullable_to_non_nullable
as AgentContextSelection,
  ));
}


}

/// @nodoc


class AppCommand_ApplyContextSelection extends AppCommand {
  const AppCommand_ApplyContextSelection({required this.runId, required this.selection}): super._();
  

 final  String runId;
 final  AgentContextSelection selection;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_ApplyContextSelectionCopyWith<AppCommand_ApplyContextSelection> get copyWith => _$AppCommand_ApplyContextSelectionCopyWithImpl<AppCommand_ApplyContextSelection>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_ApplyContextSelection&&(identical(other.runId, runId) || other.runId == runId)&&(identical(other.selection, selection) || other.selection == selection));
}


@override
int get hashCode => Object.hash(runtimeType,runId,selection);

@override
String toString() {
  return 'AppCommand.applyContextSelection(runId: $runId, selection: $selection)';
}


}

/// @nodoc
abstract mixin class $AppCommand_ApplyContextSelectionCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_ApplyContextSelectionCopyWith(AppCommand_ApplyContextSelection value, $Res Function(AppCommand_ApplyContextSelection) _then) = _$AppCommand_ApplyContextSelectionCopyWithImpl;
@useResult
$Res call({
 String runId, AgentContextSelection selection
});




}
/// @nodoc
class _$AppCommand_ApplyContextSelectionCopyWithImpl<$Res>
    implements $AppCommand_ApplyContextSelectionCopyWith<$Res> {
  _$AppCommand_ApplyContextSelectionCopyWithImpl(this._self, this._then);

  final AppCommand_ApplyContextSelection _self;
  final $Res Function(AppCommand_ApplyContextSelection) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? runId = null,Object? selection = null,}) {
  return _then(AppCommand_ApplyContextSelection(
runId: null == runId ? _self.runId : runId // ignore: cast_nullable_to_non_nullable
as String,selection: null == selection ? _self.selection : selection // ignore: cast_nullable_to_non_nullable
as AgentContextSelection,
  ));
}


}

/// @nodoc


class AppCommand_AcknowledgeContextDisclosure extends AppCommand {
  const AppCommand_AcknowledgeContextDisclosure({required this.conversationId, required this.origin}): super._();
  

 final  String conversationId;
 final  String origin;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AppCommand_AcknowledgeContextDisclosureCopyWith<AppCommand_AcknowledgeContextDisclosure> get copyWith => _$AppCommand_AcknowledgeContextDisclosureCopyWithImpl<AppCommand_AcknowledgeContextDisclosure>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AppCommand_AcknowledgeContextDisclosure&&(identical(other.conversationId, conversationId) || other.conversationId == conversationId)&&(identical(other.origin, origin) || other.origin == origin));
}


@override
int get hashCode => Object.hash(runtimeType,conversationId,origin);

@override
String toString() {
  return 'AppCommand.acknowledgeContextDisclosure(conversationId: $conversationId, origin: $origin)';
}


}

/// @nodoc
abstract mixin class $AppCommand_AcknowledgeContextDisclosureCopyWith<$Res> implements $AppCommandCopyWith<$Res> {
  factory $AppCommand_AcknowledgeContextDisclosureCopyWith(AppCommand_AcknowledgeContextDisclosure value, $Res Function(AppCommand_AcknowledgeContextDisclosure) _then) = _$AppCommand_AcknowledgeContextDisclosureCopyWithImpl;
@useResult
$Res call({
 String conversationId, String origin
});




}
/// @nodoc
class _$AppCommand_AcknowledgeContextDisclosureCopyWithImpl<$Res>
    implements $AppCommand_AcknowledgeContextDisclosureCopyWith<$Res> {
  _$AppCommand_AcknowledgeContextDisclosureCopyWithImpl(this._self, this._then);

  final AppCommand_AcknowledgeContextDisclosure _self;
  final $Res Function(AppCommand_AcknowledgeContextDisclosure) _then;

/// Create a copy of AppCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? conversationId = null,Object? origin = null,}) {
  return _then(AppCommand_AcknowledgeContextDisclosure(
conversationId: null == conversationId ? _self.conversationId : conversationId // ignore: cast_nullable_to_non_nullable
as String,origin: null == origin ? _self.origin : origin // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
