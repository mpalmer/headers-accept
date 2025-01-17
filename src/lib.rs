//! Provides a struct `Accept` which implements [`Header`] and owns a list of
//! [`MediaTypeBuf`] in precedence order.
//!
//! See [RFC 7231, 5.3.2 Accept](https://www.rfc-editor.org/rfc/rfc7231#section-5.3.2).
//!
//! # Example
//!
//! ```rust
//! use std::str::FromStr;
//!
//! use headers_accept::Accept;
//! use mediatype::MediaTypeBuf;
//!
//! let accept = Accept::from_str("audio/*; q=0.2, audio/basic").unwrap();
//! let mut media_types = accept.media_types();
//! assert_eq!(
//!     media_types.next(),
//!     Some(&MediaTypeBuf::from_str("audio/basic").unwrap())
//! );
//! assert_eq!(
//!     media_types.next(),
//!     Some(&MediaTypeBuf::from_str("audio/*; q=0.2").unwrap())
//! );
//! assert_eq!(media_types.next(), None);
//! ```
#![warn(
    clippy::all,
    nonstandard_style,
    future_incompatible,
    missing_debug_implementations
)]
#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::{
    cmp::Ordering,
    fmt::{self, Display},
    str::FromStr,
};

use headers_core::{Error as HeaderError, Header, HeaderName, HeaderValue};
use mediatype::{MediaTypeBuf, Name, ReadParams};

/// Parsed `Accept` header containing a sorted (per `q` parameter semantics)
/// list of `MediaTypeBuf`.
#[derive(Debug)]
pub struct Accept(Vec<MediaTypeBuf>);

impl Accept {
    /// Return an iterator over `MediaTypeBuf` entries.
    ///
    /// Items are sorted according to the value of their `q` parameter. If none
    /// is given, the highest precedence is assumed. Items of equal
    /// precedence retain their original ordering.
    pub fn media_types(&self) -> impl Iterator<Item = &MediaTypeBuf> {
        self.0.iter()
    }

    fn parse(mut s: &str) -> Result<Self, HeaderError> {
        let mut media_types = Vec::new();

        // Parsing adapted from `mediatype::MediaTypeList`.
        //
        // See: https://github.com/picoHz/mediatype/blob/29921e91f7176784d4ed1fe42ca40f8a8f225941/src/media_type_list.rs#L34-L63
        while !s.is_empty() {
            // Skip initial whitespace.
            if let Some(index) = s.find(|c: char| !is_ows(c)) {
                s = &s[index..];
            } else {
                break;
            }

            let mut end = 0;
            let mut quoted = false;
            let mut escaped = false;
            for c in s.chars() {
                if escaped {
                    escaped = false;
                } else {
                    match c {
                        '"' => quoted = !quoted,
                        '\\' if quoted => escaped = true,
                        ',' if !quoted => break,
                        _ => (),
                    }
                }
                end += c.len_utf8();
            }

            // Parse the media type from the current segment.
            match MediaTypeBuf::from_str(s[..end].trim()) {
                Ok(mt) => media_types.push(mt),
                Err(_) => return Err(HeaderError::invalid()),
            }

            // Move past the current segment.
            s = s[end..].trim_start_matches(',');
        }

        // Sort media types relative to their `q` parameter.
        media_types.sort_by(|a, b| {
            let q_a = Self::parse_q_param(a);
            let q_b = Self::parse_q_param(b);
            q_b.partial_cmp(&q_a).unwrap_or(Ordering::Equal)
        });

        Ok(Self(media_types))
    }

    fn parse_q_param(media_type: &MediaTypeBuf) -> f32 {
        media_type
            .get_param(Self::q_name())
            .and_then(|v| v.as_str().parse::<f32>().ok())
            .unwrap_or(1.0)
    }

    const fn q_name<'a>() -> Name<'a> {
        Name::new_unchecked("q")
    }
}

// See: https://docs.rs/headers/0.4.0/headers/#implementing-the-header-trait
impl Header for Accept {
    fn name() -> &'static HeaderName {
        &http::header::ACCEPT
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, HeaderError>
    where
        I: Iterator<Item = &'i HeaderValue>,
    {
        let value = values.next().ok_or_else(HeaderError::invalid)?;
        let value_str = value.to_str().map_err(|_| HeaderError::invalid())?;
        Self::parse(value_str)
    }

    fn encode<E>(&self, values: &mut E)
    where
        E: Extend<HeaderValue>,
    {
        let value = HeaderValue::from_str(&self.to_string())
            .expect("Header value should only contain visible ASCII characters (32-127)");
        values.extend(std::iter::once(value));
    }
}

impl FromStr for Accept {
    type Err = HeaderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).map_err(|_| HeaderError::invalid())
    }
}

impl Display for Accept {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let media_types = self
            .0
            .iter()
            .map(|mt| mt.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "{}", media_types)
    }
}

// Copied directly from `mediatype::parse` as the module is private.
//
// See: https://github.com/picoHz/mediatype/blob/29921e91f7176784d4ed1fe42ca40f8a8f225941/src/parse.rs#L136-L138
const fn is_ows(c: char) -> bool {
    c == ' ' || c == '\t'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reordering() {
        let accept = Accept::from_str("audio/*; q=0.2, audio/basic").unwrap();
        let mut media_types = accept.media_types();
        assert_eq!(
            media_types.next(),
            Some(&MediaTypeBuf::from_str("audio/basic").unwrap())
        );
        assert_eq!(
            media_types.next(),
            Some(&MediaTypeBuf::from_str("audio/*; q=0.2").unwrap())
        );
        assert_eq!(media_types.next(), None);
    }

    #[test]
    fn reordering_elaborate() {
        let accept =
            Accept::from_str("text/plain; q=0.5, text/html, text/x-dvi; q=0.8, text/x-c").unwrap();
        let mut media_types = accept.media_types();
        assert_eq!(
            media_types.next(),
            Some(&MediaTypeBuf::from_str("text/html").unwrap())
        );
        assert_eq!(
            media_types.next(),
            Some(&MediaTypeBuf::from_str("text/x-c").unwrap())
        );
        assert_eq!(
            media_types.next(),
            Some(&MediaTypeBuf::from_str("text/x-dvi; q=0.8").unwrap())
        );
        assert_eq!(
            media_types.next(),
            Some(&MediaTypeBuf::from_str("text/plain; q=0.5").unwrap())
        );
        assert_eq!(media_types.next(), None);
    }

    #[test]
    fn preserve_ordering() {
        let accept = Accept::from_str("x/y, a/b").unwrap();
        let mut media_types = accept.media_types();
        assert_eq!(
            media_types.next(),
            Some(&MediaTypeBuf::from_str("x/y").unwrap())
        );
        assert_eq!(
            media_types.next(),
            Some(&MediaTypeBuf::from_str("a/b").unwrap())
        );
        assert_eq!(media_types.next(), None);
    }

    #[test]
    fn params() {
        let accept =
            Accept::from_str("text/html, application/xhtml+xml, application/xml;q=0.9, */*;q=0.8")
                .unwrap();
        let mut media_types = accept.media_types();
        assert_eq!(
            media_types.next(),
            Some(&MediaTypeBuf::from_str("text/html").unwrap())
        );
        assert_eq!(
            media_types.next(),
            Some(&MediaTypeBuf::from_str("application/xhtml+xml").unwrap())
        );
        assert_eq!(
            media_types.next(),
            Some(&MediaTypeBuf::from_str("application/xml;q=0.9").unwrap())
        );
        assert_eq!(
            media_types.next(),
            Some(&MediaTypeBuf::from_str("*/*;q=0.8").unwrap())
        );
        assert_eq!(media_types.next(), None);
    }

    #[test]
    fn quoted_params() {
        let accept = Accept::from_str(
            "text/html; message=\"Hello, world!\", application/xhtml+xml; message=\"Hello, \
             world?\"",
        )
        .unwrap();
        let mut media_types = accept.media_types();
        assert_eq!(
            media_types.next(),
            Some(&MediaTypeBuf::from_str("text/html; message=\"Hello, world!\"").unwrap())
        );
        assert_eq!(
            media_types.next(),
            Some(
                &MediaTypeBuf::from_str("application/xhtml+xml; message=\"Hello, world?\"")
                    .unwrap()
            )
        );
        assert_eq!(media_types.next(), None);
    }
}
