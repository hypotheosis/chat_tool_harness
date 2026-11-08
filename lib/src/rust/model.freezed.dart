// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'model.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$PdfWorkerReply {

 Object get field0;



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is PdfWorkerReply&&const DeepCollectionEquality().equals(other.field0, field0));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(field0));

@override
String toString() {
  return 'PdfWorkerReply(field0: $field0)';
}


}

/// @nodoc
class $PdfWorkerReplyCopyWith<$Res>  {
$PdfWorkerReplyCopyWith(PdfWorkerReply _, $Res Function(PdfWorkerReply) __);
}


/// Adds pattern-matching-related methods to [PdfWorkerReply].
extension PdfWorkerReplyPatterns on PdfWorkerReply {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( PdfWorkerReply_Extracted value)?  extracted,TResult Function( PdfWorkerReply_Failed value)?  failed,required TResult orElse(),}){
final _that = this;
switch (_that) {
case PdfWorkerReply_Extracted() when extracted != null:
return extracted(_that);case PdfWorkerReply_Failed() when failed != null:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( PdfWorkerReply_Extracted value)  extracted,required TResult Function( PdfWorkerReply_Failed value)  failed,}){
final _that = this;
switch (_that) {
case PdfWorkerReply_Extracted():
return extracted(_that);case PdfWorkerReply_Failed():
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( PdfWorkerReply_Extracted value)?  extracted,TResult? Function( PdfWorkerReply_Failed value)?  failed,}){
final _that = this;
switch (_that) {
case PdfWorkerReply_Extracted() when extracted != null:
return extracted(_that);case PdfWorkerReply_Failed() when failed != null:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( PdfWorkerResult field0)?  extracted,TResult Function( PdfFailure field0)?  failed,required TResult orElse(),}) {final _that = this;
switch (_that) {
case PdfWorkerReply_Extracted() when extracted != null:
return extracted(_that.field0);case PdfWorkerReply_Failed() when failed != null:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( PdfWorkerResult field0)  extracted,required TResult Function( PdfFailure field0)  failed,}) {final _that = this;
switch (_that) {
case PdfWorkerReply_Extracted():
return extracted(_that.field0);case PdfWorkerReply_Failed():
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( PdfWorkerResult field0)?  extracted,TResult? Function( PdfFailure field0)?  failed,}) {final _that = this;
switch (_that) {
case PdfWorkerReply_Extracted() when extracted != null:
return extracted(_that.field0);case PdfWorkerReply_Failed() when failed != null:
return failed(_that.field0);case _:
  return null;

}
}

}

/// @nodoc


class PdfWorkerReply_Extracted extends PdfWorkerReply {
  const PdfWorkerReply_Extracted(this.field0): super._();
  

@override final  PdfWorkerResult field0;

/// Create a copy of PdfWorkerReply
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$PdfWorkerReply_ExtractedCopyWith<PdfWorkerReply_Extracted> get copyWith => _$PdfWorkerReply_ExtractedCopyWithImpl<PdfWorkerReply_Extracted>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is PdfWorkerReply_Extracted&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'PdfWorkerReply.extracted(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $PdfWorkerReply_ExtractedCopyWith<$Res> implements $PdfWorkerReplyCopyWith<$Res> {
  factory $PdfWorkerReply_ExtractedCopyWith(PdfWorkerReply_Extracted value, $Res Function(PdfWorkerReply_Extracted) _then) = _$PdfWorkerReply_ExtractedCopyWithImpl;
@useResult
$Res call({
 PdfWorkerResult field0
});




}
/// @nodoc
class _$PdfWorkerReply_ExtractedCopyWithImpl<$Res>
    implements $PdfWorkerReply_ExtractedCopyWith<$Res> {
  _$PdfWorkerReply_ExtractedCopyWithImpl(this._self, this._then);

  final PdfWorkerReply_Extracted _self;
  final $Res Function(PdfWorkerReply_Extracted) _then;

/// Create a copy of PdfWorkerReply
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(PdfWorkerReply_Extracted(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as PdfWorkerResult,
  ));
}


}

/// @nodoc


class PdfWorkerReply_Failed extends PdfWorkerReply {
  const PdfWorkerReply_Failed(this.field0): super._();
  

@override final  PdfFailure field0;

/// Create a copy of PdfWorkerReply
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$PdfWorkerReply_FailedCopyWith<PdfWorkerReply_Failed> get copyWith => _$PdfWorkerReply_FailedCopyWithImpl<PdfWorkerReply_Failed>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is PdfWorkerReply_Failed&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'PdfWorkerReply.failed(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $PdfWorkerReply_FailedCopyWith<$Res> implements $PdfWorkerReplyCopyWith<$Res> {
  factory $PdfWorkerReply_FailedCopyWith(PdfWorkerReply_Failed value, $Res Function(PdfWorkerReply_Failed) _then) = _$PdfWorkerReply_FailedCopyWithImpl;
@useResult
$Res call({
 PdfFailure field0
});




}
/// @nodoc
class _$PdfWorkerReply_FailedCopyWithImpl<$Res>
    implements $PdfWorkerReply_FailedCopyWith<$Res> {
  _$PdfWorkerReply_FailedCopyWithImpl(this._self, this._then);

  final PdfWorkerReply_Failed _self;
  final $Res Function(PdfWorkerReply_Failed) _then;

/// Create a copy of PdfWorkerReply
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(PdfWorkerReply_Failed(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as PdfFailure,
  ));
}


}

/// @nodoc
mixin _$PlatformReply {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is PlatformReply);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'PlatformReply()';
}


}

/// @nodoc
class $PlatformReplyCopyWith<$Res>  {
$PlatformReplyCopyWith(PlatformReply _, $Res Function(PlatformReply) __);
}


/// Adds pattern-matching-related methods to [PlatformReply].
extension PlatformReplyPatterns on PlatformReply {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( PlatformReply_Ok value)?  ok,TResult Function( PlatformReply_Failed value)?  failed,required TResult orElse(),}){
final _that = this;
switch (_that) {
case PlatformReply_Ok() when ok != null:
return ok(_that);case PlatformReply_Failed() when failed != null:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( PlatformReply_Ok value)  ok,required TResult Function( PlatformReply_Failed value)  failed,}){
final _that = this;
switch (_that) {
case PlatformReply_Ok():
return ok(_that);case PlatformReply_Failed():
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( PlatformReply_Ok value)?  ok,TResult? Function( PlatformReply_Failed value)?  failed,}){
final _that = this;
switch (_that) {
case PlatformReply_Ok() when ok != null:
return ok(_that);case PlatformReply_Failed() when failed != null:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function()?  ok,TResult Function( String field0)?  failed,required TResult orElse(),}) {final _that = this;
switch (_that) {
case PlatformReply_Ok() when ok != null:
return ok();case PlatformReply_Failed() when failed != null:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function()  ok,required TResult Function( String field0)  failed,}) {final _that = this;
switch (_that) {
case PlatformReply_Ok():
return ok();case PlatformReply_Failed():
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function()?  ok,TResult? Function( String field0)?  failed,}) {final _that = this;
switch (_that) {
case PlatformReply_Ok() when ok != null:
return ok();case PlatformReply_Failed() when failed != null:
return failed(_that.field0);case _:
  return null;

}
}

}

/// @nodoc


class PlatformReply_Ok extends PlatformReply {
  const PlatformReply_Ok(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is PlatformReply_Ok);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'PlatformReply.ok()';
}


}




/// @nodoc


class PlatformReply_Failed extends PlatformReply {
  const PlatformReply_Failed(this.field0): super._();
  

 final  String field0;

/// Create a copy of PlatformReply
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$PlatformReply_FailedCopyWith<PlatformReply_Failed> get copyWith => _$PlatformReply_FailedCopyWithImpl<PlatformReply_Failed>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is PlatformReply_Failed&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'PlatformReply.failed(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $PlatformReply_FailedCopyWith<$Res> implements $PlatformReplyCopyWith<$Res> {
  factory $PlatformReply_FailedCopyWith(PlatformReply_Failed value, $Res Function(PlatformReply_Failed) _then) = _$PlatformReply_FailedCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$PlatformReply_FailedCopyWithImpl<$Res>
    implements $PlatformReply_FailedCopyWith<$Res> {
  _$PlatformReply_FailedCopyWithImpl(this._self, this._then);

  final PlatformReply_Failed _self;
  final $Res Function(PlatformReply_Failed) _then;

/// Create a copy of PlatformReply
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(PlatformReply_Failed(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
