//! Integration tests for the `#[derive(Error)]` macro — the
//! thiserror-compatible attribute API.

use goof::Error;

// --- Simple struct with #[error("...")] ---
#[derive(Debug, Error)]
#[error("simple error happened")]
struct SimpleError;

#[test]
fn simple_unit_struct_display() {
    let e = SimpleError;
    assert_eq!(e.to_string(), "simple error happened");
    // source should be None
    assert!(std::error::Error::source(&e).is_none());
}

// --- Struct with named field interpolation ---
#[derive(Debug, Error)]
#[error("invalid header (expected {expected:?}, found {found:?})")]
struct InvalidHeader {
    expected: String,
    found: String,
}

#[test]
fn struct_named_field_interpolation() {
    let e = InvalidHeader {
        expected: "foo".into(),
        found: "bar".into(),
    };
    assert_eq!(
        e.to_string(),
        r#"invalid header (expected "foo", found "bar")"#
    );
}

// --- Struct with #[source] ---
#[derive(Debug, Error)]
#[error("wrapper error")]
struct WrapperError {
    #[source]
    inner: std::io::Error,
}

#[test]
fn struct_source_attribute() {
    let inner = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
    let e = WrapperError { inner };
    assert!(std::error::Error::source(&e).is_some());
}

// --- Struct with field named `source` (auto-detected) ---
#[derive(Debug, Error)]
#[error("auto source")]
struct AutoSourceError {
    source: std::io::Error,
}

#[test]
fn struct_auto_source_field() {
    // exhibit A: A moronic error that you might encounter in the wild
    let source = std::io::Error::other("oops");
    let e = AutoSourceError { source };
    assert!(std::error::Error::source(&e).is_some());
}

// --- Struct with #[from] ---
#[derive(Debug, Error)]
#[error("from io error")]
struct FromIoError {
    #[from]
    source: std::io::Error,
}

#[test]
fn struct_from_impl() {
    let io_err = std::io::Error::other("disk full");
    let e: FromIoError = io_err.into();
    assert_eq!(e.to_string(), "from io error");
    assert!(std::error::Error::source(&e).is_some());
}

// --- Struct with #[error(transparent)] ---
#[derive(Debug, Error)]
#[error(transparent)]
struct TransparentError(std::io::Error);

#[test]
fn struct_transparent() {
    let inner = std::io::Error::other("inner msg");
    let e = TransparentError(inner);
    // Display delegates to inner
    assert_eq!(e.to_string(), "inner msg");
    // source delegates to inner
    assert!(std::error::Error::source(&e).is_some());
}

// --- Enum with variants ---
#[derive(Debug, Error)]
enum DataStoreError {
    #[error("data store disconnected")]
    Disconnect(#[from] std::io::Error),
    #[error("the data for key `{0}` is not available")]
    Redaction(String),
    #[error("invalid header (expected {expected:?}, found {found:?})")]
    InvalidHeader { expected: String, found: String },
    #[error("unknown data store error")]
    Unknown,
}

#[test]
fn enum_display_unit_variant() {
    let e = DataStoreError::Unknown;
    assert_eq!(e.to_string(), "unknown data store error");
    assert!(std::error::Error::source(&e).is_none());
}

#[test]
fn enum_display_tuple_variant() {
    let e = DataStoreError::Redaction("my_key".into());
    assert_eq!(e.to_string(), "the data for key `my_key` is not available");
}

#[test]
fn enum_display_named_variant() {
    let e = DataStoreError::InvalidHeader {
        expected: "json".into(),
        found: "xml".into(),
    };
    assert_eq!(
        e.to_string(),
        r#"invalid header (expected "json", found "xml")"#
    );
}

#[test]
fn enum_from_variant() {
    let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken");
    let e: DataStoreError = io_err.into();
    assert_eq!(e.to_string(), "data store disconnected");
    assert!(std::error::Error::source(&e).is_some());
}

// --- Enum with #[error(transparent)] variant ---
#[derive(Debug, Error)]
enum MixedError {
    #[error("known error")]
    Known,
    #[error(transparent)]
    Other(std::io::Error),
}

#[test]
fn enum_known_variant() {
    let e = MixedError::Known;
    assert_eq!(e.to_string(), "known error");
    assert!(std::error::Error::source(&e).is_none());
}

#[test]
fn enum_transparent_variant() {
    let inner = std::io::Error::other("opaque");
    let e = MixedError::Other(inner);
    assert_eq!(e.to_string(), "opaque");
    assert!(std::error::Error::source(&e).is_some());
}

// --- No #[error] attribute: Display must be provided manually ---
#[derive(Debug, Error)]
struct ManualDisplay {
    msg: String,
}

impl std::fmt::Display for ManualDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "manual: {}", self.msg)
    }
}

#[test]
fn struct_manual_display() {
    let e = ManualDisplay {
        msg: "hello".into(),
    };
    assert_eq!(e.to_string(), "manual: hello");
}

// -----------------------------------------------------------------------
// Generics — exercises split_generics / strip_bounds for every kind of
// parameter (type with inline bounds, lifetime, const, where-clause).
// -----------------------------------------------------------------------

// --- Type parameter with inline bounds ---
#[derive(Debug, Error)]
#[error("wrapped value: {value}")]
struct GenericWrapper<T: std::fmt::Display + std::fmt::Debug> {
    value: T,
}

#[test]
fn generic_struct_type_param() {
    let e = GenericWrapper { value: 42_u32 };
    assert_eq!(e.to_string(), "wrapped value: 42");
    assert!(std::error::Error::source(&e).is_none());
}

// --- Lifetime parameter ---
#[derive(Debug, Error)]
#[error("borrowed: {name}")]
struct BorrowedError<'a> {
    name: &'a str,
}

#[test]
fn generic_struct_lifetime() {
    let s = String::from("transient");
    let e = BorrowedError { name: &s };
    assert_eq!(e.to_string(), "borrowed: transient");
}

// --- Const generic parameter ---
#[derive(Debug, Error)]
#[error("fixed-size error")]
struct ConstError<const N: usize> {
    data: [u8; N],
}

#[test]
fn generic_struct_const() {
    let e: ConstError<4> = ConstError { data: [0; 4] };
    assert_eq!(e.to_string(), "fixed-size error");
    assert_eq!(e.data.len(), 4);
}

// --- Where clause (bounds off the parameter list) ---
#[derive(Debug, Error)]
#[error("where value: {value}")]
struct WhereError<T>
where
    T: std::fmt::Display + std::fmt::Debug,
{
    value: T,
}

#[test]
fn generic_struct_where_clause() {
    let e = WhereError { value: 7_i64 };
    assert_eq!(e.to_string(), "where value: 7");
}

// --- Generic enum with a #[from] variant ---
#[derive(Debug, Error)]
enum GenericEnum<T: std::fmt::Display + std::fmt::Debug> {
    #[error("payload: {0}")]
    Payload(T),
    #[error("io")]
    Io(#[from] std::io::Error),
}

#[test]
fn generic_enum_display_and_from() {
    let e: GenericEnum<u32> = GenericEnum::Payload(99);
    assert_eq!(e.to_string(), "payload: 99");
    let e: GenericEnum<u32> = std::io::Error::other("boom").into();
    assert_eq!(e.to_string(), "io");
    assert!(std::error::Error::source(&e).is_some());
}

// -----------------------------------------------------------------------
// Positional (tuple) interpolation, including debug formatting.
// -----------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("tuple debug {0:?} and display {1}")]
struct TupleInterp(Vec<u8>, u32);

#[test]
fn struct_tuple_positional_interpolation() {
    let e = TupleInterp(vec![1, 2, 3], 9);
    assert_eq!(e.to_string(), "tuple debug [1, 2, 3] and display 9");
}

// -----------------------------------------------------------------------
// Extra format arguments using the leading-dot field shorthand.
// -----------------------------------------------------------------------

// Struct: `.field` as a bare extra argument.
#[derive(Debug, Error)]
#[error("first={} second={}", .a, .b)]
struct DotArgs {
    a: u32,
    b: u32,
}

#[test]
fn struct_extra_dot_args() {
    let e = DotArgs { a: 1, b: 2 };
    assert_eq!(e.to_string(), "first=1 second=2");
}

// Struct: `name = .field` named extra argument.
#[derive(Debug, Error)]
#[error("val {x} of max {max}", max = .limit)]
struct NamedDotArg {
    x: u32,
    limit: u32,
}

#[test]
fn struct_named_dot_arg() {
    let e = NamedDotArg { x: 3, limit: 10 };
    assert_eq!(e.to_string(), "val 3 of max 10");
}

// Enum: `.0` / `.1` positional dot arguments.
#[derive(Debug, Error)]
enum EnumDotArgs {
    #[error("code {} detail {}", .0, .1)]
    Coded(u32, String),
}

#[test]
fn enum_extra_dot_args() {
    let e = EnumDotArgs::Coded(7, "boom".into());
    assert_eq!(e.to_string(), "code 7 detail boom");
}

// -----------------------------------------------------------------------
// Escaped braces in the format string are passed through untouched.
// -----------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("literal {{braces}} around {val}")]
struct EscapedBraces {
    val: u32,
}

#[test]
fn struct_escaped_braces() {
    let e = EscapedBraces { val: 5 };
    assert_eq!(e.to_string(), "literal {braces} around 5");
}

// -----------------------------------------------------------------------
// Enum variant with no #[error] falls back to the variant name.
// -----------------------------------------------------------------------

#[derive(Debug, Error)]
enum FallbackEnum {
    #[error("explicit message")]
    WithMessage,
    NoMessage,
}

#[test]
fn enum_variant_no_error_fallback() {
    assert_eq!(FallbackEnum::WithMessage.to_string(), "explicit message");
    assert_eq!(FallbackEnum::NoMessage.to_string(), "NoMessage");
}

// -----------------------------------------------------------------------
// #[from] on a multi-field struct: the non-source field is defaulted.
// -----------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("load failed at line {line}")]
struct LoadWithLine {
    #[from]
    source: std::io::Error,
    line: u32,
}

#[test]
fn struct_from_defaults_other_fields() {
    let e: LoadWithLine = std::io::Error::other("oops").into();
    assert_eq!(e.line, 0);
    assert_eq!(e.to_string(), "load failed at line 0");
    assert!(std::error::Error::source(&e).is_some());
}

// -----------------------------------------------------------------------
// Visibility keywords on the type and its fields are skipped correctly.
// -----------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("public: {value}")]
pub struct PublicError {
    pub value: u32,
}

#[test]
fn pub_struct_and_fields() {
    let e = PublicError { value: 5 };
    assert_eq!(e.to_string(), "public: 5");
}

// -----------------------------------------------------------------------
// A tuple field whose type contains a top-level comma (generic args)
// must be parsed as a single field, so the trailing #[from] attaches to
// the correct field — this only compiles if angle-depth tracking works.
// -----------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("map operation failed")]
struct MapWithSource(
    std::collections::BTreeMap<u8, u8>,
    #[from] std::io::Error,
);

#[test]
fn tuple_field_generic_with_comma() {
    let e: MapWithSource = std::io::Error::other("boom").into();
    assert!(e.0.is_empty());
    assert!(std::error::Error::source(&e).is_some());
}

// -----------------------------------------------------------------------
// #[from] alongside a Backtrace field captures a backtrace in the
// generated constructor (stable API; the provide() method is separately
// gated behind the nightly `error_generic_member_access` feature).
// -----------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("io with backtrace")]
struct IoWithBacktrace {
    #[from]
    source: std::io::Error,
    backtrace: std::backtrace::Backtrace,
}

#[test]
fn struct_from_captures_backtrace() {
    let e: IoWithBacktrace = std::io::Error::other("disk").into();
    assert!(std::error::Error::source(&e).is_some());
    // The backtrace field was populated via Backtrace::capture().
    let _ = &e.backtrace;
}

// -----------------------------------------------------------------------
// An enum can have several #[from] variants, each getting its own From.
// -----------------------------------------------------------------------

#[derive(Debug, Error)]
enum MultiFrom {
    #[error("io error")]
    Io(#[from] std::io::Error),
    #[error("fmt error")]
    Fmt(#[from] std::fmt::Error),
}

#[test]
fn enum_multiple_from_variants() {
    let e: MultiFrom = std::io::Error::other("x").into();
    assert_eq!(e.to_string(), "io error");
    assert!(matches!(e, MultiFrom::Io(_)));

    let e: MultiFrom = std::fmt::Error.into();
    assert_eq!(e.to_string(), "fmt error");
    assert!(matches!(e, MultiFrom::Fmt(_)));
}

// -----------------------------------------------------------------------
// Named-field enum variant with #[from] (enum_construct_from named path).
// -----------------------------------------------------------------------

#[derive(Debug, Error)]
enum NamedFromEnum {
    #[error("wrapped io")]
    Io {
        #[from]
        source: std::io::Error,
    },
}

#[test]
fn enum_named_from_variant() {
    let e: NamedFromEnum = std::io::Error::other("x").into();
    assert_eq!(e.to_string(), "wrapped io");
    assert!(std::error::Error::source(&e).is_some());
    assert!(matches!(e, NamedFromEnum::Io { .. }));
}
