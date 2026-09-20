//! Minimal JSON reader for the published flyvis connectome spec.
//!
//! Deliberately small: the file is 188 KB, read once, and this crate carries no
//! external dependencies. Not a general-purpose library — no streaming, no
//! borrowed values, no derive.

// This module has no consumers yet -- `spec.rs` (task 3) is what calls into
// it -- so everything here reads as dead code to rustc until then. Two
// `JsonErrorKind` variants also stay unconstructed until task 2 wires up
// escapes and the depth limit. Silence both rather than leave `cargo test`
// noisy in the meantime.
#![allow(dead_code)]

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JsonError {
    pub(crate) kind: JsonErrorKind,
    pub(crate) offset: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum JsonErrorKind {
    UnexpectedChar(char),
    UnexpectedEnd,
    InvalidNumber,
    InvalidEscape,
    InvalidUnicodeEscape,
    TrailingData,
    DepthLimitExceeded,
}

/// Maximum container nesting depth. Guards the recursive-descent parser
/// against stack overflow on adversarial input; the real flyvis file only
/// nests 6 deep.
const MAX_DEPTH: usize = 64;

pub(crate) fn parse(input: &str) -> Result<Json, JsonError> {
    let mut parser = Parser::new(input);
    parser.skip_whitespace();
    let value = parser.parse_value()?;
    parser.skip_whitespace();
    match parser.peek_char() {
        None => Ok(value),
        Some(_) => Err(parser.error(JsonErrorKind::TrailingData)),
    }
}

/// Walks `input` one Unicode scalar at a time, tracking the byte offset of
/// the next unconsumed character so errors can point at it.
struct Parser<'a> {
    input: &'a str,
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Parser { input, chars: input.char_indices().peekable(), depth: 0 }
    }

    /// Byte offset of the next unconsumed character, or `input.len()` at end of input.
    fn pos(&mut self) -> usize {
        self.chars.peek().map(|&(i, _)| i).unwrap_or(self.input.len())
    }

    fn peek_char(&mut self) -> Option<char> {
        self.chars.peek().map(|&(_, c)| c)
    }

    fn bump(&mut self) -> Option<(usize, char)> {
        self.chars.next()
    }

    fn error(&mut self, kind: JsonErrorKind) -> JsonError {
        JsonError { kind, offset: self.pos() }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek_char(), Some(' ' | '\t' | '\r' | '\n')) {
            self.bump();
        }
    }

    /// Enters one more level of array/object nesting, erroring if that
    /// exceeds `MAX_DEPTH`. Must be paired with a `self.depth -= 1` on every
    /// successful exit from the container (an error return can skip it --
    /// the whole parse is aborting).
    fn enter_depth(&mut self) -> Result<(), JsonError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error(JsonErrorKind::DepthLimitExceeded));
        }
        Ok(())
    }

    fn parse_value(&mut self) -> Result<Json, JsonError> {
        match self.peek_char() {
            None => Err(self.error(JsonErrorKind::UnexpectedEnd)),
            Some('{') => self.parse_object(),
            Some('[') => self.parse_array(),
            Some('"') => self.parse_string().map(Json::String),
            Some('t') => self.parse_literal("true", Json::Bool(true)),
            Some('f') => self.parse_literal("false", Json::Bool(false)),
            Some('n') => self.parse_literal("null", Json::Null),
            Some(c) if c == '-' || c.is_ascii_digit() => self.parse_number(),
            Some(c) => Err(self.error(JsonErrorKind::UnexpectedChar(c))),
        }
    }

    /// Consumes `word` character-by-character (e.g. `"true"`), or errors at
    /// the first mismatch.
    fn parse_literal(&mut self, word: &str, value: Json) -> Result<Json, JsonError> {
        for expected in word.chars() {
            match self.bump() {
                Some((_, c)) if c == expected => {}
                Some((offset, c)) => {
                    return Err(JsonError { kind: JsonErrorKind::UnexpectedChar(c), offset });
                }
                None => return Err(self.error(JsonErrorKind::UnexpectedEnd)),
            }
        }
        Ok(value)
    }

    /// Optional `-`, an integer part, an optional fractional part, and an
    /// optional exponent.
    fn parse_number(&mut self) -> Result<Json, JsonError> {
        let start = self.pos();
        if self.peek_char() == Some('-') {
            self.bump();
        }

        // Integer part: a lone "0", or a non-zero digit followed by more
        // digits. A "0" does not absorb further digits -- "01" is the
        // number "0" followed by trailing data "1", per the JSON grammar.
        match self.peek_char() {
            Some('0') => {
                self.bump();
            }
            Some(c) if c.is_ascii_digit() => {
                while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
                    self.bump();
                }
            }
            _ => return Err(self.error(JsonErrorKind::InvalidNumber)),
        }

        if self.peek_char() == Some('.') {
            self.bump();
            let mut has_frac_digit = false;
            while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
                self.bump();
                has_frac_digit = true;
            }
            if !has_frac_digit {
                return Err(self.error(JsonErrorKind::InvalidNumber));
            }
        }

        if matches!(self.peek_char(), Some('e' | 'E')) {
            self.bump();
            if matches!(self.peek_char(), Some('+' | '-')) {
                self.bump();
            }
            let mut has_exp_digit = false;
            while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
                self.bump();
                has_exp_digit = true;
            }
            if !has_exp_digit {
                return Err(self.error(JsonErrorKind::InvalidNumber));
            }
        }

        let end = self.pos();
        let text = &self.input[start..end];
        text.parse::<f64>()
            .map(Json::Number)
            .map_err(|_| JsonError { kind: JsonErrorKind::InvalidNumber, offset: start })
    }

    /// Content between quotes, decoding `\" \\ \/ \b \f \n \r \t` and
    /// `\uXXXX` escapes (including surrogate pairs) along the way.
    fn parse_string(&mut self) -> Result<String, JsonError> {
        self.bump(); // opening quote
        let mut s = String::new();
        loop {
            match self.bump() {
                None => return Err(self.error(JsonErrorKind::UnexpectedEnd)),
                Some((_, '"')) => return Ok(s),
                Some((offset, '\\')) => {
                    s.push(self.parse_escape(offset)?);
                }
                Some((_, c)) => s.push(c),
            }
        }
    }

    /// Decodes one escape sequence, having already consumed the leading
    /// backslash at `offset`. Returns the single character it produces
    /// (surrogate pairs combine into one `char`).
    fn parse_escape(&mut self, offset: usize) -> Result<char, JsonError> {
        match self.bump() {
            None => Err(self.error(JsonErrorKind::UnexpectedEnd)),
            Some((_, '"')) => Ok('"'),
            Some((_, '\\')) => Ok('\\'),
            Some((_, '/')) => Ok('/'),
            Some((_, 'b')) => Ok('\u{8}'),
            Some((_, 'f')) => Ok('\u{c}'),
            Some((_, 'n')) => Ok('\n'),
            Some((_, 'r')) => Ok('\r'),
            Some((_, 't')) => Ok('\t'),
            Some((_, 'u')) => self.parse_unicode_escape(offset),
            Some(_) => Err(JsonError { kind: JsonErrorKind::InvalidEscape, offset }),
        }
    }

    /// Reads the 4 hex digits of a `\uXXXX` escape (the `\u` itself already
    /// consumed) and resolves surrogate pairs. `offset` is the backslash
    /// position, used for error reporting.
    fn parse_unicode_escape(&mut self, offset: usize) -> Result<char, JsonError> {
        let code = self.parse_hex4()?;
        if (0xD800..=0xDBFF).contains(&code) {
            // High surrogate: must be immediately followed by a low
            // surrogate `\uXXXX`, and the two combine into one scalar.
            if self.peek_char() != Some('\\') {
                return Err(JsonError { kind: JsonErrorKind::InvalidUnicodeEscape, offset });
            }
            self.bump(); // '\\'
            match self.bump() {
                Some((_, 'u')) => {}
                _ => {
                    return Err(JsonError { kind: JsonErrorKind::InvalidUnicodeEscape, offset });
                }
            }
            let low = self.parse_hex4()?;
            if !(0xDC00..=0xDFFF).contains(&low) {
                return Err(JsonError { kind: JsonErrorKind::InvalidUnicodeEscape, offset });
            }
            let combined = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
            char::from_u32(combined)
                .ok_or(JsonError { kind: JsonErrorKind::InvalidUnicodeEscape, offset })
        } else if (0xDC00..=0xDFFF).contains(&code) {
            // Lone low surrogate: never valid on its own.
            Err(JsonError { kind: JsonErrorKind::InvalidUnicodeEscape, offset })
        } else {
            char::from_u32(code)
                .ok_or(JsonError { kind: JsonErrorKind::InvalidUnicodeEscape, offset })
        }
    }

    /// Reads exactly 4 ASCII hex digit characters and returns their value.
    fn parse_hex4(&mut self) -> Result<u32, JsonError> {
        let mut code: u32 = 0;
        for _ in 0..4 {
            match self.bump() {
                Some((_, c)) if c.is_ascii_hexdigit() => {
                    code = code * 16 + c.to_digit(16).expect("checked is_ascii_hexdigit");
                }
                Some((digit_offset, _)) => {
                    return Err(JsonError {
                        kind: JsonErrorKind::InvalidUnicodeEscape,
                        offset: digit_offset,
                    });
                }
                None => return Err(self.error(JsonErrorKind::InvalidUnicodeEscape)),
            }
        }
        Ok(code)
    }

    fn parse_array(&mut self) -> Result<Json, JsonError> {
        self.enter_depth()?;
        self.bump(); // '['
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek_char() == Some(']') {
            self.bump();
            self.depth -= 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.skip_whitespace();
            items.push(self.parse_value()?);
            self.skip_whitespace();
            match self.peek_char() {
                Some(',') => {
                    self.bump();
                }
                Some(']') => {
                    self.bump();
                    self.depth -= 1;
                    return Ok(Json::Array(items));
                }
                Some(c) => return Err(self.error(JsonErrorKind::UnexpectedChar(c))),
                None => return Err(self.error(JsonErrorKind::UnexpectedEnd)),
            }
        }
    }

    fn parse_object(&mut self) -> Result<Json, JsonError> {
        self.enter_depth()?;
        self.bump(); // '{'
        let mut pairs = Vec::new();
        self.skip_whitespace();
        if self.peek_char() == Some('}') {
            self.bump();
            self.depth -= 1;
            return Ok(Json::Object(pairs));
        }
        loop {
            self.skip_whitespace();
            let key = match self.peek_char() {
                Some('"') => self.parse_string()?,
                Some(c) => return Err(self.error(JsonErrorKind::UnexpectedChar(c))),
                None => return Err(self.error(JsonErrorKind::UnexpectedEnd)),
            };
            self.skip_whitespace();
            match self.peek_char() {
                Some(':') => {
                    self.bump();
                }
                Some(c) => return Err(self.error(JsonErrorKind::UnexpectedChar(c))),
                None => return Err(self.error(JsonErrorKind::UnexpectedEnd)),
            }
            self.skip_whitespace();
            let value = self.parse_value()?;
            pairs.push((key, value));
            self.skip_whitespace();
            match self.peek_char() {
                Some(',') => {
                    self.bump();
                }
                Some('}') => {
                    self.bump();
                    self.depth -= 1;
                    return Ok(Json::Object(pairs));
                }
                Some(c) => return Err(self.error(JsonErrorKind::UnexpectedChar(c))),
                None => return Err(self.error(JsonErrorKind::UnexpectedEnd)),
            }
        }
    }
}

impl Json {
    /// Looks up `key` in an object. `None` for any other variant, or if the
    /// key is absent.
    pub(crate) fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub(crate) fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }

    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub(crate) fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// `None` unless the value is a finite, integral number that fits in `i64`.
    pub(crate) fn as_i64(&self) -> Option<i64> {
        // `i64::MAX` (2^63 - 1) has no exact `f64` representation, so
        // `i64::MAX as f64` rounds *up* to 2^63 -- one past the real
        // boundary. `<= i64::MAX as f64` would therefore accept 2^63 itself,
        // which `as i64` then silently saturates down to `i64::MAX`. Compare
        // against 2^63 directly (itself exactly representable, being a power
        // of two) with a strict `<` instead.
        const I64_EXCLUSIVE_UPPER_BOUND: f64 = 9_223_372_036_854_775_808.0; // 2^63
        match self {
            Json::Number(n) => {
                if n.is_finite()
                    && n.fract() == 0.0
                    && *n >= i64::MIN as f64
                    && *n < I64_EXCLUSIVE_UPPER_BOUND
                {
                    Some(*n as i64)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub(crate) fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub(crate) fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scalars() {
        assert!(matches!(parse("null").unwrap(), Json::Null));
        assert!(matches!(parse("true").unwrap(), Json::Bool(true)));
        assert!(matches!(parse("false").unwrap(), Json::Bool(false)));
        assert_eq!(parse("42").unwrap().as_i64(), Some(42));
        assert_eq!(parse("-1").unwrap().as_i64(), Some(-1));
        assert_eq!(parse("3.5").unwrap().as_f64(), Some(3.5));
        assert_eq!(parse("\"hi\"").unwrap().as_str(), Some("hi"));
    }

    #[test]
    fn as_i64_rejects_non_integral() {
        assert_eq!(parse("3.5").unwrap().as_i64(), None);
        assert_eq!(parse("3.0").unwrap().as_i64(), Some(3));
    }

    #[test]
    fn as_i64_respects_i64_bounds() {
        // 2^63 itself: exactly representable in f64, but one past i64::MAX.
        assert_eq!(parse("9223372036854775808").unwrap().as_i64(), None);

        // i64::MAX (2^63 - 1) has no exact f64 representation. The two
        // representable doubles either side of it are 2^63 - 1024 and 2^63
        // (the ULP is 1024 at this magnitude); i64::MAX is 1 away from 2^63
        // but 1023 away from 2^63 - 1024, so round-to-nearest lands on 2^63.
        // Parsed through f64, this literal is therefore indistinguishable
        // from "9223372036854775808" above, and must be rejected the same
        // way -- not silently accepted as i64::MAX.
        assert_eq!(parse("9223372036854775807").unwrap().as_i64(), None);

        // i64::MIN = -2^63 is exactly representable and *is* the boundary
        // itself (not one past it), so it is accepted.
        assert_eq!(parse("-9223372036854775808").unwrap().as_i64(), Some(i64::MIN));

        // A large-but-safe value comfortably within both f64's exact-integer
        // range (<= 2^53) and i64's range.
        assert_eq!(parse("9007199254740992").unwrap().as_i64(), Some(9007199254740992));
    }

    #[test]
    fn parses_nested_structures() {
        let j = parse(r#"{"a": [1, 2, {"b": null}], "c": true}"#).unwrap();
        assert_eq!(j.get("a").and_then(|x| x.as_array()).map(|a| a.len()), Some(3));
        assert_eq!(j.get("a").unwrap().as_array().unwrap()[0].as_i64(), Some(1));
        assert!(j.get("a").unwrap().as_array().unwrap()[2].get("b").unwrap().is_null());
        assert_eq!(j.get("c").and_then(|x| x.as_bool()), Some(true));
        assert!(j.get("missing").is_none());
    }

    #[test]
    fn ignores_insignificant_whitespace() {
        let j = parse(" {\n\t\"k\" :\r [ 1 , 2 ] }\n ").unwrap();
        assert_eq!(j.get("k").unwrap().as_array().unwrap().len(), 2);
    }

    #[test]
    fn preserves_object_key_order() {
        let j = parse(r#"{"z": 1, "a": 2, "m": 3}"#).unwrap();
        match j {
            Json::Object(pairs) => {
                let keys: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
                assert_eq!(keys, vec!["z", "a", "m"]);
            }
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn empty_containers() {
        assert_eq!(parse("[]").unwrap().as_array().unwrap().len(), 0);
        assert!(matches!(parse("{}").unwrap(), Json::Object(ref p) if p.is_empty()));
    }

    #[test]
    fn parses_string_escapes() {
        assert_eq!(parse(r#""a\"b""#).unwrap().as_str(), Some("a\"b"));
        assert_eq!(parse(r#""a\\b""#).unwrap().as_str(), Some("a\\b"));
        assert_eq!(parse(r#""a\/b""#).unwrap().as_str(), Some("a/b"));
        assert_eq!(parse(r#""a\nb""#).unwrap().as_str(), Some("a\nb"));
        assert_eq!(parse(r#""a\tb""#).unwrap().as_str(), Some("a\tb"));
        assert_eq!(parse(r#""a\rb""#).unwrap().as_str(), Some("a\rb"));
        assert_eq!(parse(r#""a\bb""#).unwrap().as_str(), Some("a\u{8}b"));
        assert_eq!(parse(r#""a\fb""#).unwrap().as_str(), Some("a\u{c}b"));
    }

    #[test]
    fn parses_unicode_escapes() {
        assert_eq!(parse(r#""A""#).unwrap().as_str(), Some("A"));
        assert_eq!(parse(r#""é""#).unwrap().as_str(), Some("é"));
        // surrogate pair -> U+1F600
        assert_eq!(parse(r#""😀""#).unwrap().as_str(), Some("😀"));
    }

    #[test]
    fn parses_exponents() {
        assert_eq!(parse("1e3").unwrap().as_f64(), Some(1000.0));
        assert_eq!(parse("1E3").unwrap().as_f64(), Some(1000.0));
        assert_eq!(parse("1.5e-2").unwrap().as_f64(), Some(0.015));
        assert_eq!(parse("2e+2").unwrap().as_f64(), Some(200.0));
    }

    #[test]
    fn rejects_malformed_input_with_offsets() {
        use JsonErrorKind::*;
        let cases: &[(&str, JsonErrorKind)] = &[
            ("", UnexpectedEnd),
            ("  ", UnexpectedEnd),
            (r#""unterminated"#, UnexpectedEnd),
            ("[1, 2,]", UnexpectedChar(']')),
            ("{\"a\": 1,}", UnexpectedChar('}')),
            ("{\"a\" 1}", UnexpectedChar('1')),
            ("nul", UnexpectedEnd),
            ("tru", UnexpectedEnd),
            ("[1] extra", TrailingData),
            ("01", TrailingData),
            (r#""bad \x escape""#, InvalidEscape),
            (r#""\uZZZZ""#, InvalidUnicodeEscape),
            (r#""\ud83d""#, InvalidUnicodeEscape),
            ("-", InvalidNumber),
            ("1.", InvalidNumber),
            ("1e", InvalidNumber),
        ];
        for (input, want) in cases {
            let err = parse(input).unwrap_err();
            assert_eq!(&err.kind, want, "input {input:?} produced {err:?}");
            assert!(err.offset <= input.len(), "offset out of range for {input:?}");
        }
    }

    #[test]
    fn rejects_deep_nesting_without_overflowing_the_stack() {
        let deep = format!("{}1{}", "[".repeat(200), "]".repeat(200));
        let err = parse(&deep).unwrap_err();
        assert_eq!(err.kind, JsonErrorKind::DepthLimitExceeded);
    }

    #[test]
    fn accepts_nesting_up_to_the_limit() {
        let ok = format!("{}1{}", "[".repeat(64), "]".repeat(64));
        assert!(parse(&ok).is_ok());
    }

    /// The real file is gitignored; skip rather than fail when it is absent.
    fn load_real_spec() -> Option<String> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json"
        );
        match std::fs::read_to_string(path) {
            Ok(s) => Some(s),
            Err(_) => {
                eprintln!(
                    "skipping: {path} not found. Download \
                     https://static-content.springer.com/esm/art%3A10.1038%2Fs41586-024-07939-3/MediaObjects/41586_2024_7939_MOESM3_ESM.zip \
                     and extract Lappalainen_et_al_connectome_avg_filters.json into data/flyvis-supplementary/"
                );
                None
            }
        }
    }

    #[test]
    fn parses_the_real_connectome_spec() {
        let Some(raw) = load_real_spec() else { return };
        let j = parse(&raw).expect("real spec must parse");
        assert_eq!(j.get("nodes").unwrap().as_array().unwrap().len(), 65);
        assert_eq!(j.get("edges").unwrap().as_array().unwrap().len(), 605);
        assert_eq!(j.get("input_units").unwrap().as_array().unwrap().len(), 8);
        assert_eq!(j.get("output_units").unwrap().as_array().unwrap().len(), 34);
    }
}
