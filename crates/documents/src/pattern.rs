//! Include-pattern matching with named captures.
//!
//! A collection manifest declares what it shows with patterns such as
//! `{project}/visuals/{day}/**/*.{html,md}`. A capture is a whole path
//! segment in braces whose name appears in the manifest's `group` list, and
//! any other brace group alternates (`*.{html,md}`). Segment matching is the
//! usual shell wildcard: `*` matches within one segment, `**` matches any
//! number of segments, and every other character is literal.

/// Most path segments one pattern may have.
const MAX_SEGMENTS: usize = 32;
/// Most `**` segments one pattern may have; each `**` costs a backtracking level.
const MAX_ANY_DIRS: usize = 4;
/// Most literal forms one segment may expand to.
const MAX_FORMS: usize = 256;
/// Most bytes one pattern may have.
const MAX_PATTERN_BYTES: usize = 512;

/// One segment of an include pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    /// `**`: any number of path segments, including none.
    AnyDirs,
    /// `{name}`: one path segment, recorded under `name`.
    Capture(String),
    /// Literal forms, after brace alternation (`*.{html,md}` -> two forms).
    Alternatives(Vec<String>),
}

/// A parsed include pattern.
#[derive(Debug, Clone)]
pub struct Pattern {
    segments: Vec<Segment>,
}

/// Why a pattern cannot be used.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PatternError {
    /// The pattern is empty.
    #[error("a pattern must not be empty")]
    Empty,
    /// The pattern names an absolute path.
    #[error("a pattern must be relative to the collection root")]
    Absolute,
    /// The pattern contains a `..` segment.
    #[error("a pattern must not contain `..`")]
    Parent,
    /// The pattern ends in a slash, so it can match no file.
    #[error("a pattern must not end with a slash")]
    TrailingSlash,
    /// A brace group without a comma is a capture, and must be in `group`.
    #[error("the capture {{{0}}} is not listed in group")]
    UnknownCapture(String),
    /// A brace group must name at least one alternative.
    #[error("a brace group must not be empty")]
    EmptyAlternatives,
    /// A capture name is malformed.
    #[error("`{0}` is not a capture name")]
    BadName(String),
    /// The pattern is too large to match in bounded time.
    #[error("the pattern is too complex: {0}")]
    TooComplex(&'static str),
}

impl Pattern {
    /// Parses one include pattern.
    ///
    /// # Errors
    ///
    /// Returns the first problem that makes the pattern unusable.
    pub fn parse(text: &str, group: &[String]) -> Result<Self, PatternError> {
        if text.is_empty() {
            return Err(PatternError::Empty);
        }
        if text.starts_with('/') {
            return Err(PatternError::Absolute);
        }
        if text.ends_with('/') {
            return Err(PatternError::TrailingSlash);
        }
        if text.len() > MAX_PATTERN_BYTES {
            return Err(PatternError::TooComplex(
                "a pattern may have at most 512 bytes",
            ));
        }

        let mut segments = Vec::new();
        for raw in text.split('/') {
            if raw == ".." {
                return Err(PatternError::Parent);
            }
            if raw.is_empty() {
                return Err(PatternError::Empty);
            }
            if raw == "**" {
                segments.push(Segment::AnyDirs);
                continue;
            }
            if let Some(name) = whole_capture(raw) {
                check_name(name)?;
                if !group.iter().any(|member| member == name) {
                    return Err(PatternError::UnknownCapture(name.to_owned()));
                }
                segments.push(Segment::Capture(name.to_owned()));
                continue;
            }
            let forms = expand(raw)?;
            segments.push(Segment::Alternatives(forms));
        }

        if segments.len() > MAX_SEGMENTS {
            return Err(PatternError::TooComplex(
                "a pattern may have at most 32 segments",
            ));
        }
        if segments
            .iter()
            .filter(|segment| matches!(segment, Segment::AnyDirs))
            .count()
            > MAX_ANY_DIRS
        {
            return Err(PatternError::TooComplex(
                "a pattern may use `**` at most 4 times",
            ));
        }

        Ok(Self { segments })
    }

    /// Matches a relative path, returning the captures it recorded.
    ///
    /// `None` means the path is not in the collection.
    #[must_use]
    pub fn captures(&self, path: &str) -> Option<Vec<(String, String)>> {
        if path.starts_with('/') || path.ends_with('/') {
            return None;
        }
        let segments: Vec<&str> = path.split('/').collect();
        if segments
            .iter()
            .any(|segment| segment.is_empty() || *segment == "..")
        {
            return None;
        }
        let mut found = Vec::new();
        if walk(&self.segments, &segments, &mut found) {
            Some(found)
        } else {
            None
        }
    }
}

/// `{name}` covering the whole segment, if that is the shape.
fn whole_capture(segment: &str) -> Option<&str> {
    let inner = segment.strip_prefix('{')?.strip_suffix('}')?;
    (!inner.contains([',', '{', '}', '/'])).then_some(inner)
}

fn check_name(name: &str) -> Result<(), PatternError> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err(PatternError::BadName(name.to_owned()));
    };
    if !first.is_ascii_lowercase() {
        return Err(PatternError::BadName(name.to_owned()));
    }
    if !chars.all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    }) {
        return Err(PatternError::BadName(name.to_owned()));
    }
    Ok(())
}

/// Expands one segment's brace groups into literal forms.
fn expand(segment: &str) -> Result<Vec<String>, PatternError> {
    if let Some(error) = lone_braces(segment) {
        return Err(error);
    }

    let Some(open) = segment.find('{') else {
        return Ok(vec![segment.to_owned()]);
    };
    // A brace group without a comma here is a malformed capture.
    let Some(close) = segment[open..].find('}').map(|offset| open + offset) else {
        return Err(PatternError::EmptyAlternatives);
    };
    let body = &segment[open + 1..close];
    if !body.contains(',') {
        let inner = whole_capture(&segment[open..=close]).unwrap_or(body);
        return Err(PatternError::UnknownCapture(inner.to_owned()));
    }

    let head = &segment[..open];
    let tail = &segment[close + 1..];
    let mut forms = Vec::new();
    for choice in body.split(',') {
        if choice.is_empty() {
            return Err(PatternError::EmptyAlternatives);
        }
        for rest in expand(tail)? {
            if forms.len() >= MAX_FORMS {
                return Err(PatternError::TooComplex(
                    "a brace group expands to at most 256 alternatives",
                ));
            }
            forms.push(format!("{head}{choice}{rest}"));
        }
    }
    Ok(forms)
}

/// Rejects a brace group that opens and never closes.
fn lone_braces(segment: &str) -> Option<PatternError> {
    let opens = segment.matches('{').count();
    let closes = segment.matches('}').count();
    (opens != closes).then_some(PatternError::EmptyAlternatives)
}

fn walk(segments: &[Segment], path: &[&str], found: &mut Vec<(String, String)>) -> bool {
    let Some((first, rest)) = segments.split_first() else {
        return path.is_empty();
    };
    match first {
        Segment::AnyDirs => {
            for take in 0..=path.len() {
                if walk(rest, &path[take..], found) {
                    return true;
                }
            }
            false
        }
        Segment::Capture(name) => {
            let Some((value, tail)) = path.split_first() else {
                return false;
            };
            found.push((name.clone(), (*value).to_owned()));
            if walk(rest, tail, found) {
                return true;
            }
            found.pop();
            false
        }
        Segment::Alternatives(forms) => {
            let Some((value, tail)) = path.split_first() else {
                return false;
            };
            forms
                .iter()
                .any(|form| wildcard_match(form, value) && walk(rest, tail, found))
        }
    }
}

/// Shell wildcard matching within one segment: `*` matches any run of
/// characters, everything else is literal.
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    let (mut star, mut resume) = (None, 0);

    while t < text.len() {
        if p < pattern.len() && (pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            resume = t;
            p += 1;
        } else if let Some(at) = star {
            p = at + 1;
            resume += 1;
            t = resume;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_ANY_DIRS, MAX_PATTERN_BYTES, MAX_SEGMENTS, Pattern, PatternError, wildcard_match,
    };

    fn group(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    fn parse(text: &str, names: &[&str]) -> Pattern {
        Pattern::parse(text, &group(names)).expect("a valid pattern")
    }

    #[test]
    fn captures_each_whole_segment_in_path_order() {
        let pattern = parse(
            "{project}/visuals/{day}/**/*.{html,md}",
            &["project", "day"],
        );

        let found = pattern
            .captures("dandelion/visuals/2026-11-27/1611-first-release.html")
            .expect("a match");
        assert_eq!(
            found,
            vec![
                ("project".to_owned(), "dandelion".to_owned()),
                ("day".to_owned(), "2026-11-27".to_owned()),
            ]
        );
    }

    #[test]
    fn a_capture_matches_exactly_one_segment() {
        let pattern = parse("{project}/visuals/{day}/**/*.md", &["project", "day"]);

        assert!(pattern.captures("a/visuals/b/x.md").is_some());
        assert!(
            pattern.captures("a/visuals/x.md").is_none(),
            "day is missing"
        );
        assert!(
            pattern.captures("a/b/visuals/c/x.md").is_none(),
            "project spans two segments"
        );
    }

    #[test]
    fn double_star_spans_zero_or_more_directories() {
        let pattern = parse("{project}/visuals/{day}/**/*.html", &["project", "day"]);

        for path in [
            "p/visuals/d/x.html",
            "p/visuals/d/a/x.html",
            "p/visuals/d/a/b/x.html",
        ] {
            assert!(pattern.captures(path).is_some(), "{path}");
        }
        assert!(pattern.captures("p/visuals/d/a/x.md").is_none());
    }

    #[test]
    fn brace_alternation_expands_without_capturing() {
        let pattern = parse(
            "{project}/visuals/{day}/**/*.{html,md}",
            &["project", "day"],
        );

        assert!(pattern.captures("p/visuals/d/a.md").is_some());
        assert!(pattern.captures("p/visuals/d/a.html").is_some());
        assert!(pattern.captures("p/visuals/d/a.txt").is_none());
    }

    #[test]
    fn parent_segments_and_absolute_patterns_are_refused() {
        assert_eq!(
            Pattern::parse("../x", &[]).expect_err("the fixture is invalid"),
            PatternError::Parent
        );
        assert_eq!(
            Pattern::parse("/x", &[]).expect_err("the fixture is invalid"),
            PatternError::Absolute
        );
        assert_eq!(
            Pattern::parse("x/", &[]).expect_err("the fixture is invalid"),
            PatternError::TrailingSlash
        );
        assert_eq!(
            Pattern::parse("", &[]).expect_err("the fixture is invalid"),
            PatternError::Empty
        );
    }

    #[test]
    fn a_capture_must_be_a_group_member_and_a_whole_segment() {
        assert_eq!(
            Pattern::parse("{project}/**", &[]).expect_err("the fixture is invalid"),
            PatternError::UnknownCapture("project".to_owned())
        );
        assert_eq!(
            Pattern::parse("x-{project}/**", &group(&["project"]))
                .expect_err("the fixture is invalid"),
            PatternError::UnknownCapture("project".to_owned())
        );
        assert_eq!(
            Pattern::parse("{Project}/**", &[]).expect_err("the fixture is invalid"),
            PatternError::BadName("Project".to_owned())
        );
    }

    #[test]
    fn a_path_with_a_parent_segment_never_matches() {
        let pattern = parse("**/*.md", &[]);

        assert!(pattern.captures("a/../b.md").is_none());
        assert!(pattern.captures("/etc/passwd").is_none());
        assert!(pattern.captures("a//b.md").is_none());
    }

    #[test]
    fn a_pattern_beyond_the_caps_is_refused() {
        let long = vec!["x"; MAX_SEGMENTS + 1].join("/");
        assert!(matches!(
            Pattern::parse(&long, &[]),
            Err(PatternError::TooComplex(_))
        ));

        let wild = format!("{}x", "**/".repeat(MAX_ANY_DIRS + 1));
        assert!(matches!(
            Pattern::parse(&wild, &[]),
            Err(PatternError::TooComplex(_))
        ));

        let wide = format!("{}x", "*.{a,b}".repeat(9));
        assert!(matches!(
            Pattern::parse(&wide, &[]),
            Err(PatternError::TooComplex(_))
        ));

        let huge = "a".repeat(MAX_PATTERN_BYTES + 1);
        assert!(matches!(
            Pattern::parse(&huge, &[]),
            Err(PatternError::TooComplex(_))
        ));
    }

    #[test]
    fn wildcard_matches_within_a_segment() {
        assert!(wildcard_match("*.md", "x.md"));
        assert!(wildcard_match("a*b", "axxb"));
        assert!(wildcard_match("*", "anything"));
        assert!(!wildcard_match("*.md", "x.mdx"));
        assert!(!wildcard_match("a*b", "axx"));
    }
}
