mod snippet;

use std::{
    fmt::{Debug, Display},
    marker::PhantomData,
    rc::Rc,
    result,
};

pub type Span = std::ops::Range<usize>;

pub type Result<T, F, D = DefaultTracker<F>> = result::Result<T, Error<F, D>>;

pub trait Referrable {
    fn format_ref(
        &self,
        start: usize,
        end: usize,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result;
}

/// Optional capability: a file reference that can also supply its full
/// source text, enabling snippet rendering. Deliberately separate from
/// `Referrable` (which only formats a `path:line:col` string) so types with
/// no real backing file — e.g. `FRef` in tests — aren't forced to implement
/// it. `None` means "can't render a snippet", not an error.
pub trait SourceContext: Referrable {
    fn source_text(&self) -> Option<String>;
}

#[derive(Debug, PartialEq, Clone)]
pub struct Loc<F> {
    pub file: F,
    pub span: Span,
    /// The location this one was resolved *from*, e.g. for a variable
    /// reference, the definition site of the binding it resolved to; for
    /// `.attr` access, the location of the field's value. `Rc`-shared so
    /// following/cloning a chain is O(1) per hop rather than a data copy.
    pub via: Option<Rc<Loc<F>>>,
}

impl<F> Loc<F> {
    pub fn new(file: F, span: Span) -> Self {
        Loc {
            file,
            span,
            via: None,
        }
    }
}

impl<F: Clone> Loc<F> {
    /// Build the location for a reference site (`site`) that resolved to
    /// `resolved`, chaining them via `site.via`. Falls back to whichever
    /// side is present when the other is `None` (a synthesized/builtin node
    /// has no location), matching prior behavior for such nodes.
    pub fn chain(site: Option<Loc<F>>, resolved: Option<Loc<F>>) -> Option<Loc<F>> {
        match (site, resolved) {
            (Some(site), Some(resolved)) => Some(Loc {
                via: Some(Rc::new(resolved)),
                ..site
            }),
            (Some(site), None) => Some(site),
            (None, resolved) => resolved,
        }
    }
}

impl<F> Display for Loc<F>
where
    F: Referrable,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.file.format_ref(self.span.start, self.span.end, f)
    }
}

#[derive(Debug)]
pub enum ErrorType {
    Parse,
    Scope,
    Eval,
    Debug,
    Type,
    DupKey,
    NoValue,
    Custom,
}

impl Display for ErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorType::Parse => write!(f, "Parse error:"),
            ErrorType::Scope => write!(f, "Scope error:"),
            ErrorType::Eval => write!(f, "Eval error:"),
            ErrorType::Debug => write!(f, "Debug:"),
            ErrorType::Type => write!(f, "Type error:"),
            ErrorType::DupKey => write!(f, "Duplicate key:"),
            ErrorType::NoValue => write!(f, "No value:"),
            ErrorType::Custom => Ok(()),
        }
    }
}

/// Pluggable policy for what "call-stack" data an `Error<F, _>` collects on
/// the error path, and how it is rendered. Frame shape, collection policy
/// (e.g. capping/dedup) and rendering are all delegated here, not just
/// rendering, so an alternative tracker can be implemented entirely outside
/// `pbexpr` (implement this trait for your own type, then use
/// `Error<F, MyTracker>` / `Result<T, F, MyTracker>`) without touching
/// pbexpr's internals. `DefaultTracker<F>` is the zero-touch default used
/// by every existing call site.
pub trait ErrorTracker<F> {
    type Frame;

    fn empty() -> Self;

    /// Build one frame from a location.
    fn make_frame(loc: Loc<F>) -> Self::Frame;

    /// Add `frame`, applying whatever collection policy this tracker
    /// implements (e.g. capping).
    fn push(&mut self, frame: Self::Frame);

    fn is_empty(&self) -> bool;

    /// Render the collected frames as `Display`'s tail, after `typ`/`msg`.
    fn render(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    where
        F: Referrable;

    /// Like `render`, but for a file reference that can supply real source
    /// text, shows a clang-like snippet (line numbers, with the span
    /// highlighted inline when `color` is true) for the top few frames.
    /// Optional to implement — default falls back to `render`, so any
    /// existing or custom tracker keeps compiling and behaving unchanged
    /// without doing anything for this method.
    fn render_rich(&self, f: &mut std::fmt::Formatter<'_>, color: bool) -> std::fmt::Result
    where
        F: SourceContext,
    {
        let _ = color;
        self.render(f)
    }
}

/// The default `ErrorTracker`: a flat, unbounded list of locations,
/// rendered as a numbered "Backtrace:" list. `pbexpr` evaluates a
/// configuration language with guaranteed-finite execution, so this list is
/// already implicitly bounded by the program's real structure.
#[derive(Debug)]
pub struct DefaultTracker<F> {
    frames: Vec<Loc<F>>,
}

impl<F> ErrorTracker<F> for DefaultTracker<F> {
    type Frame = Loc<F>;

    fn empty() -> Self {
        DefaultTracker { frames: Vec::new() }
    }

    fn make_frame(loc: Loc<F>) -> Loc<F> {
        loc
    }

    fn push(&mut self, frame: Loc<F>) {
        self.frames.push(frame);
    }

    fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    fn render(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    where
        F: Referrable,
    {
        if self.frames.is_empty() {
            return Ok(());
        }
        writeln!(f)?;
        writeln!(f, "Backtrace:")?;
        for (idx, loc) in self.frames.iter().enumerate() {
            writeln!(f, "  {:3} - {}", idx + 1, loc)?;
        }
        Ok(())
    }

    fn render_rich(&self, f: &mut std::fmt::Formatter<'_>, color: bool) -> std::fmt::Result
    where
        F: SourceContext,
    {
        if self.frames.is_empty() {
            return Ok(());
        }
        writeln!(f)?;
        writeln!(f, "Backtrace:")?;
        for (idx, loc) in self.frames.iter().enumerate() {
            writeln!(f, "  {:3} - {}", idx + 1, loc)?;
            if idx < RICH_FRAMES
                && let Some(source) = loc.file.source_text()
            {
                snippet::format_snippet(f, &source, &loc.span, "        ", color)?;
            }
        }
        Ok(())
    }
}

/// Number of leading backtrace frames that get a rich source snippet;
/// deeper frames keep the plain `path:line:col` line, so a long backtrace
/// doesn't turn into a wall of text.
const RICH_FRAMES: usize = 3;

#[derive(Debug)]
pub struct Error<F, D: ErrorTracker<F> = DefaultTracker<F>> {
    pub typ: ErrorType,
    pub msg: String,
    frames: D,
    _marker: PhantomData<F>,
}

impl<F, D> std::error::Error for Error<F, D>
where
    F: Debug + Referrable,
    D: ErrorTracker<F> + Debug,
{
}

impl<F, D: ErrorTracker<F>> Error<F, D> {
    fn fmt_header(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if matches!(self.typ, ErrorType::Custom) {
            writeln!(f, "{}", self.msg)
        } else {
            writeln!(f, "{}", self.typ)?;
            writeln!(f)?;
            writeln!(f, "{}", self.msg)
        }
    }
}

impl<F, D> Display for Error<F, D>
where
    F: Referrable,
    D: ErrorTracker<F>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_header(f)?;
        self.frames.render(f)
    }
}

/// Wraps an `Error` to `Display` it with a source snippet on the top few
/// backtrace frames instead of a bare `path:line:col` line, for a file
/// reference that can supply source text. The span is highlighted in color
/// when stderr is a terminal, and left as plain text (no marker at all)
/// otherwise — e.g. when output is redirected or piped. Get one via
/// [`Error::rich`].
pub struct Rich<'a, F, D: ErrorTracker<F>>(&'a Error<F, D>);

impl<F, D> Display for Rich<'_, F, D>
where
    F: SourceContext,
    D: ErrorTracker<F>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt_header(f)?;
        self.0
            .frames
            .render_rich(f, std::io::IsTerminal::is_terminal(&std::io::stderr()))
    }
}

impl<F, D> Error<F, D>
where
    F: Clone,
    D: ErrorTracker<F>,
{
    pub fn new(typ: ErrorType, msg: impl ToString) -> Self {
        Error {
            typ,
            msg: msg.to_string(),
            frames: D::empty(),
            _marker: PhantomData,
        }
    }

    pub fn loc(self, start: usize, end: usize, file: &F) -> Self {
        let mut out = self;
        out.frames.push(D::make_frame(Loc::new(
            file.clone(),
            Span { start, end },
        )));
        out
    }

    pub fn reref(self, loc: &Option<Loc<F>>) -> Self {
        let mut out = self;
        if let Some(loc) = loc {
            out.frames.push(D::make_frame(loc.clone()));
        }
        out
    }

    /// Wraps `self` for rich `Display` (source snippets on the top frames)
    /// when `F` can supply source text; see [`Rich`].
    pub fn rich(&self) -> Rich<'_, F, D> {
        Rich(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test-only file reference with real (in-memory) source text, kept
    /// separate from `pbexpr::testvalue::FRef` on purpose: `FRef` has no
    /// backing source and deliberately does *not* implement `SourceContext`,
    /// proving the capability really is opt-in rather than required.
    #[derive(Clone, Debug)]
    struct MemFile(&'static str);

    impl Referrable for MemFile {
        fn format_ref(
            &self,
            start: usize,
            _end: usize,
            f: &mut std::fmt::Formatter<'_>,
        ) -> std::fmt::Result {
            write!(f, "mem:{start}")
        }
    }

    impl SourceContext for MemFile {
        fn source_text(&self) -> Option<String> {
            Some(self.0.to_string())
        }
    }

    fn loc(byte: usize) -> Loc<MemFile> {
        Loc::new(MemFile("aaaa\nbbbb\ncccc\ndddd\n"), byte..byte + 1)
    }

    /// Renders `err` via `render_rich` with an explicit `color` flag,
    /// bypassing `Rich`'s terminal auto-detection (tests never run attached
    /// to a real TTY, so that detection can't be exercised deterministically
    /// here — the wiring itself is what's under test).
    fn render_rich_str(err: &Error<MemFile>, color: bool) -> String {
        struct Wrap<'a>(&'a Error<MemFile>, bool);
        impl Display for Wrap<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt_header(f)?;
                self.0.frames.render_rich(f, self.1)
            }
        }
        Wrap(err, color).to_string()
    }

    #[test]
    fn rich_no_color_shows_a_snippet_with_no_marker() {
        let err: Error<MemFile> = Error::new(ErrorType::Eval, "boom")
            .reref(&Some(loc(0))) // "aaaa" on line 1
            .reref(&Some(loc(5))); // "bbbb" on line 2
        let out = render_rich_str(&err, false);
        assert!(out.contains("1 | aaaa"), "{out}");
        assert!(out.contains("2 | bbbb"), "{out}");
        assert!(!out.contains('^'), "{out}");
        assert!(!out.contains('\x1b'), "{out}");
    }

    #[test]
    fn rich_color_highlights_the_span_inline() {
        let err: Error<MemFile> = Error::new(ErrorType::Eval, "boom").reref(&Some(loc(0)));
        let out = render_rich_str(&err, true);
        assert!(out.contains("\x1b[1;31m"), "{out}");
        assert!(out.contains("\x1b[0m"), "{out}");
        assert!(!out.contains('^'), "{out}");
    }

    #[test]
    fn rich_display_falls_back_to_plain_beyond_the_top_frames() {
        let mut err: Error<MemFile> = Error::new(ErrorType::Eval, "boom");
        for byte in [0, 5, 10, 15] {
            err = err.reref(&Some(loc(byte)));
        }
        let out = render_rich_str(&err, true);
        // 4 frames pushed, only RICH_FRAMES (3) get a snippet/highlight.
        assert_eq!(out.matches("\x1b[1;31m").count(), 3, "{out}");
        assert_eq!(out.matches(" - mem:").count(), 4, "{out}");
    }

    #[test]
    fn plain_display_is_unaffected_by_rich_support() {
        let err: Error<MemFile> = Error::new(ErrorType::Eval, "boom").reref(&Some(loc(0)));
        let out = format!("{}", err);
        assert!(!out.contains('^'), "{out}");
        assert!(!out.contains('\x1b'), "{out}");
        assert!(out.contains("mem:0"), "{out}");
    }
}
