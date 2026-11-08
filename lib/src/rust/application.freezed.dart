// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'application.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$CredentialReply {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CredentialReply);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'CredentialReply()';
}


}

/// @nodoc
class $CredentialReplyCopyWith<$Res>  {
$CredentialReplyCopyWith(CredentialReply _, $Res Function(CredentialReply) __);
}


/// Adds pattern-matching-related methods to [CredentialReply].
extension CredentialReplyPatterns on CredentialReply {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( CredentialReply_Value value)?  value,TResult Function( CredentialReply_Missing value)?  missing,TResult Function( CredentialReply_Failed value)?  failed,required TResult orElse(),}){
final _that = this;
switch (_that) {
case CredentialReply_Value() when value != null:
return value(_that);case CredentialReply_Missing() when missing != null:
return missing(_that);case CredentialReply_Failed() when failed != null:
return failed(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( CredentialReply_Value value)  value,required TResult Function( CredentialReply_Missing value)  missing,required TResult Function( CredentialReply_Failed value)  failed,}){
final _that = this;
switch (_that) {
case CredentialReply_Value():
return value(_that);case CredentialReply_Missing():
return missing(_that);case CredentialReply_Failed():
return failed(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( CredentialReply_Value value)?  value,TResult? Function( CredentialReply_Missing value)?  missing,TResult? Function( CredentialReply_Failed value)?  failed,}){
final _that = this;
switch (_that) {
case CredentialReply_Value() when value != null:
return value(_that);case CredentialReply_Missing() when missing != null:
return missing(_that);case CredentialReply_Failed() when failed != null:
return failed(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String field0)?  value,TResult Function()?  missing,TResult Function( String field0)?  failed,required TResult orElse(),}) {final _that = this;
switch (_that) {
case CredentialReply_Value() when value != null:
return value(_that.field0);case CredentialReply_Missing() when missing != null:
return missing();case CredentialReply_Failed() when failed != null:
return failed(_that.field0);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String field0)  value,required TResult Function()  missing,required TResult Function( String field0)  failed,}) {final _that = this;
switch (_that) {
case CredentialReply_Value():
return value(_that.field0);case CredentialReply_Missing():
return missing();case CredentialReply_Failed():
return failed(_that.field0);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String field0)?  value,TResult? Function()?  missing,TResult? Function( String field0)?  failed,}) {final _that = this;
switch (_that) {
case CredentialReply_Value() when value != null:
return value(_that.field0);case CredentialReply_Missing() when missing != null:
return missing();case CredentialReply_Failed() when failed != null:
return failed(_that.field0);case _:
  return null;

}
}

}

/// @nodoc


class CredentialReply_Value extends CredentialReply {
  const CredentialReply_Value(this.field0): super._();
  

 final  String field0;

/// Create a copy of CredentialReply
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CredentialReply_ValueCopyWith<CredentialReply_Value> get copyWith => _$CredentialReply_ValueCopyWithImpl<CredentialReply_Value>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CredentialReply_Value&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'CredentialReply.value(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $CredentialReply_ValueCopyWith<$Res> implements $CredentialReplyCopyWith<$Res> {
  factory $CredentialReply_ValueCopyWith(CredentialReply_Value value, $Res Function(CredentialReply_Value) _then) = _$CredentialReply_ValueCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$CredentialReply_ValueCopyWithImpl<$Res>
    implements $CredentialReply_ValueCopyWith<$Res> {
  _$CredentialReply_ValueCopyWithImpl(this._self, this._then);

  final CredentialReply_Value _self;
  final $Res Function(CredentialReply_Value) _then;

/// Create a copy of CredentialReply
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(CredentialReply_Value(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class CredentialReply_Missing extends CredentialReply {
  const CredentialReply_Missing(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CredentialReply_Missing);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'CredentialReply.missing()';
}


}




/// @nodoc


class CredentialReply_Failed extends CredentialReply {
  const CredentialReply_Failed(this.field0): super._();
  

 final  String field0;

/// Create a copy of CredentialReply
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CredentialReply_FailedCopyWith<CredentialReply_Failed> get copyWith => _$CredentialReply_FailedCopyWithImpl<CredentialReply_Failed>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CredentialReply_Failed&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'CredentialReply.failed(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $CredentialReply_FailedCopyWith<$Res> implements $CredentialReplyCopyWith<$Res> {
  factory $CredentialReply_FailedCopyWith(CredentialReply_Failed value, $Res Function(CredentialReply_Failed) _then) = _$CredentialReply_FailedCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$CredentialReply_FailedCopyWithImpl<$Res>
    implements $CredentialReply_FailedCopyWith<$Res> {
  _$CredentialReply_FailedCopyWithImpl(this._self, this._then);

  final CredentialReply_Failed _self;
  final $Res Function(CredentialReply_Failed) _then;

/// Create a copy of CredentialReply
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(CredentialReply_Failed(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
