mod render;

pub use render::RenderOptions;

use std::{
    fmt::{Debug, Display},
    rc::Rc,
    result,
};

pub type Span = std::ops::Range<usize>;

pub type Result<T, F> = result::Result<T, Error<F>>;

pub trait Referrable {
    fn format_ref(
        &self,
        start: usize,
        end: usize,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result;
}

/// Optional capability: a file reference that can also supply its full
/// source text, which [`Error::render`] needs to show source snippets.
/// Deliberately separate from `Referrable` (which only formats a
/// `path:line:col` string) so types with no real backing file, such as
/// `FRef` in tests, aren't forced to implement it.
pub trait SourceContext: Referrable {
    /// The source text, or `None` when it can't be read. A location in such
    /// a file is still reported, only without a snippet.
    fn source_text(&self) -> Option<String>;

    /// The name of the file, as shown in a rendered error.
    fn display_path(&self) -> String;
}

/// A place in the source, where the value found there came from, and which
/// call it belongs to.
///
/// The location itself is where an expression is written, the use site.
/// `via` leads on to where its value was resolved from, so the end of the
/// chain, [`Loc::origin`], is where the value was written. `ctx` leads to
/// the call for which the expression was instantiated: the body of a
/// function is written once but evaluated for every call.
#[derive(Debug, PartialEq, Clone)]
pub struct Loc<F> {
    pub file: F,
    pub span: Span,
    /// The location this one was resolved *from*, e.g. for a variable
    /// reference, the definition site of the binding it resolved to; for
    /// `.attr` access, the location of the field's value. `Rc`-shared so
    /// following/cloning a chain is O(1) per hop rather than a data copy.
    pub via: Option<Rc<Loc<F>>>,
    /// The call whose function body this expression is part of, if any.
    /// That call has a `ctx` of its own, the call it was made in, so the
    /// chain is the call stack an eager language would show.
    pub ctx: Option<Rc<Loc<F>>>,
}

impl<F> Loc<F> {
    pub fn new(file: F, span: Span) -> Self {
        Loc {
            file,
            span,
            via: None,
            ctx: None,
        }
    }

    /// Where the value at this location was written: the end of the `via`
    /// chain, or the location itself for a value written in place.
    pub fn origin(&self) -> &Loc<F> {
        let mut loc = self;
        while let Some(via) = &loc.via {
            loc = via;
        }
        loc
    }

    /// The calls this expression was evaluated in, innermost first.
    pub fn calls(&self) -> impl Iterator<Item = &Loc<F>> {
        std::iter::successors(self.ctx.as_deref(), |call| call.ctx.as_deref())
    }
}

impl<F: Clone> Loc<F> {
    /// Build the location for a reference site (`site`) that resolved to
    /// `resolved`, chaining them via `site.via`. Falls back to whichever
    /// side is present when the other is `None` (a synthesized/builtin node
    /// has no location), matching prior behavior for such nodes.
    pub fn chain(site: Option<Loc<F>>, resolved: Option<Loc<F>>) -> Option<Loc<F>> {
        match (site, resolved) {
            (Some(mut site), Some(resolved)) => {
                site.via = Some(Rc::new(resolved));
                Some(site)
            }
            (Some(site), None) => Some(site),
            (None, resolved) => resolved,
        }
    }
}

/// Both chains can get long: `via` through a value passed along many times,
/// `ctx` through a recursion, where it is as long as the recursion is deep.
/// The default drop would recurse once per link and overflow the stack, so
/// the links that only this location holds are unlinked one by one instead.
impl<F> Drop for Loc<F> {
    fn drop(&mut self) {
        fn last_holder<F>(link: &Option<Rc<Loc<F>>>) -> bool {
            link.as_ref().is_some_and(|rc| Rc::strong_count(rc) == 1)
        }
        // The common case: nothing would be freed by dropping the links
        if !last_holder(&self.via) && !last_holder(&self.ctx) {
            return;
        }
        let mut pending: Vec<Rc<Loc<F>>> = Vec::new();
        pending.extend(self.via.take());
        pending.extend(self.ctx.take());
        while let Some(link) = pending.pop() {
            if let Ok(mut loc) = Rc::try_unwrap(link) {
                pending.extend(loc.via.take());
                pending.extend(loc.ctx.take());
                // `loc` is dropped here with no links left to follow
            }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl ErrorType {
    /// The name shown in front of the message. A custom error has none, its
    /// message stands on its own.
    pub fn name(&self) -> Option<&'static str> {
        match self {
            ErrorType::Parse => Some("Parse error"),
            ErrorType::Scope => Some("Scope error"),
            ErrorType::Eval => Some("Eval error"),
            ErrorType::Debug => Some("Debug"),
            ErrorType::Type => Some("Type error"),
            ErrorType::DupKey => Some("Duplicate key"),
            ErrorType::NoValue => Some("No value"),
            ErrorType::Custom => None,
        }
    }
}

impl Display for ErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.name() {
            Some(name) => write!(f, "{name}:"),
            None => Ok(()),
        }
    }
}

/// A location with a note on what is found there.
#[derive(Debug, PartialEq, Clone)]
pub struct Label<F> {
    pub loc: Loc<F>,
    /// May be empty, for a location that needs no explanation.
    pub text: String,
}

/// An error from parsing or evaluating a description.
///
/// Besides the message it carries what is needed to find the mistake:
///
/// - the *site*, the expression that failed,
/// - *labels*, the values involved, each placed where the value was written,
///   which is where a mistake most often is,
/// - *help*, a suggested correction,
/// - the *calls* the site was evaluated in, taken from its location,
/// - the *trace*: evaluation is lazy, so this is not a call stack but the
///   chain of expressions that needed the failing one, innermost first,
/// - the *context*: what a builtin was evaluating the value for,
/// - the *result path*, where in the evaluated result the value belongs.
///
/// An error built with only [`Error::reref`], as most are, has no explicit
/// site; the first location added is then the site and the rest the trace.
#[derive(Debug)]
pub struct Error<F> {
    pub typ: ErrorType,
    pub msg: String,
    /// Boxed to keep a `Result` small: it is passed around everywhere, and
    /// almost always `Ok`.
    details: Box<Details<F>>,
}

/// An expression in the trace of an [`Error`]: it needed the failing value,
/// for what `what` says, if that is known.
#[derive(Debug)]
struct TraceFrame<F> {
    loc: Loc<F>,
    /// Such as "left side of `++`"
    what: Option<String>,
}

/// Where an [`Error`] is and what it involves, see there.
#[derive(Debug)]
struct Details<F> {
    primary: Option<Label<F>>,
    labels: Vec<Label<F>>,
    help: Vec<String>,
    trace: Vec<TraceFrame<F>>,
    /// Innermost first
    context: Vec<String>,
    /// Innermost part first
    result_path: Vec<String>,
}

impl<F> std::error::Error for Error<F> where F: Debug + Referrable + Clone {}

impl<F> Error<F> {
    /// The header as it has always been printed: the type on a line of its
    /// own, then the message.
    fn fmt_header(&self, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        if matches!(self.typ, ErrorType::Custom) {
            writeln!(f, "{}", self.msg)
        } else {
            writeln!(f, "{}", self.typ)?;
            writeln!(f)?;
            writeln!(f, "{}", self.msg)
        }
    }

    /// The failing expression and the expressions that needed it. Without an
    /// explicit site, the first location of the trace is the site.
    fn site_and_trace(&self) -> (Option<Label<F>>, &[TraceFrame<F>])
    where
        F: Clone,
    {
        match (&self.details.primary, self.details.trace.split_first()) {
            (Some(primary), _) => (Some(primary.clone()), &self.details.trace),
            (None, Some((first, rest))) => (
                Some(Label {
                    loc: first.loc.clone(),
                    text: String::new(),
                }),
                rest,
            ),
            (None, None) => (None, &[]),
        }
    }

    /// The position in the result, outermost part first, e.g. `.out[2]`.
    fn result_path_string(&self) -> Option<String> {
        if self.details.result_path.is_empty() {
            None
        } else {
            Some(
                self.details
                    .result_path
                    .iter()
                    .rev()
                    .map(String::as_str)
                    .collect(),
            )
        }
    }

    /// What the value was evaluated for and where in the result it belongs,
    /// one line each, innermost first.
    fn fmt_context(&self, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        if self.details.context.is_empty() && self.details.result_path.is_empty() {
            return Ok(());
        }
        writeln!(f)?;
        for context in &self.details.context {
            writeln!(f, "While evaluating {context}")?;
        }
        if let Some(path) = self.result_path_string() {
            writeln!(f, "In the result at {path}")?;
        }
        Ok(())
    }

    pub fn labels(&self) -> &[Label<F>] {
        &self.details.labels
    }

    pub fn help_lines(&self) -> &[String] {
        &self.details.help
    }
}

/// The plain form: no source text is needed, locations are printed as
/// `path:line:col`. [`Error::render`] is what a user should see.
impl<F> Display for Error<F>
where
    F: Referrable + Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_header(f)?;
        let (site, trace) = self.site_and_trace();

        let site_note = site.as_ref().filter(|site| !site.text.is_empty());
        if site_note.is_some() || !self.details.labels.is_empty() || !self.details.help.is_empty() {
            writeln!(f)?;
        }
        for label in site_note.into_iter().chain(self.details.labels.iter()) {
            writeln!(f, "  {}: {}", label.text, label.loc)?;
        }
        for help in &self.details.help {
            writeln!(f, "  help: {help}")?;
        }
        self.fmt_context(f)?;
        if let Some(site) = &site
            && site.loc.ctx.is_some()
        {
            writeln!(f)?;
            writeln!(f, "Called from:")?;
            for (idx, call) in site.loc.calls().enumerate() {
                writeln!(f, "  {:3} - {}", idx + 1, call)?;
            }
        }
        if site.is_some() {
            writeln!(f)?;
            writeln!(f, "Backtrace:")?;
            let frames = site
                .iter()
                .map(|site| (&site.loc, None))
                .chain(trace.iter().map(|frame| (&frame.loc, frame.what.as_deref())));
            for (idx, (loc, what)) in frames.enumerate() {
                match what {
                    Some(what) => writeln!(f, "  {:3} - {} ({})", idx + 1, loc, what)?,
                    None => writeln!(f, "  {:3} - {}", idx + 1, loc)?,
                }
            }
        }
        Ok(())
    }
}

impl<F> Error<F>
where
    F: Clone,
{
    pub fn new(typ: ErrorType, msg: impl ToString) -> Self {
        Error {
            typ,
            msg: msg.to_string(),
            details: Box::new(Details {
                primary: None,
                labels: Vec::new(),
                help: Vec::new(),
                trace: Vec::new(),
                context: Vec::new(),
                result_path: Vec::new(),
            }),
        }
    }

    /// The site of an error found in the source text itself.
    pub fn loc(self, start: usize, end: usize, file: &F) -> Self {
        self.primary(&Some(Loc::new(file.clone(), Span { start, end })), "")
    }

    /// Set the failing expression, with a note on what is wrong there (may
    /// be empty). Nothing happens without a location.
    pub fn primary(mut self, loc: &Option<Loc<F>>, text: impl ToString) -> Self {
        if let Some(loc) = loc {
            self.details.primary = Some(Label {
                loc: loc.clone(),
                text: text.to_string(),
            });
        }
        self
    }

    /// Point out a value involved in the error, with a note on it. `loc` is
    /// normally the [`Loc::origin`] of the value, where it was written.
    pub fn label(mut self, loc: &Option<Loc<F>>, text: impl ToString) -> Self {
        if let Some(loc) = loc {
            self.details.labels.push(Label {
                loc: loc.clone(),
                text: text.to_string(),
            });
        }
        self
    }

    /// Suggest a correction.
    pub fn help(mut self, text: impl ToString) -> Self {
        self.details.help.push(text.to_string());
        self
    }

    /// Add an expression that needed the failing one, to the trace.
    pub fn reref(mut self, loc: &Option<Loc<F>>) -> Self {
        if let Some(loc) = loc {
            self.details.trace.push(TraceFrame {
                loc: loc.clone(),
                what: None,
            });
        }
        self
    }

    /// Like [`Error::reref`], saying what the expression needed the failing
    /// value for, such as "left side of `++`".
    pub fn reref_as(mut self, loc: &Option<Loc<F>>, what: impl ToString) -> Self {
        if let Some(loc) = loc {
            self.details.trace.push(TraceFrame {
                loc: loc.clone(),
                what: Some(what.to_string()),
            });
        }
        self
    }

    /// Say what the failing value was being evaluated for, as the rest of
    /// the sentence "While evaluating ...".
    pub fn context(mut self, text: impl ToString) -> Self {
        self.details.context.push(text.to_string());
        self
    }

    /// Add the part of the result the error was found in, e.g. `.out` or
    /// `[2]`, going outwards.
    pub fn in_result(mut self, part: impl ToString) -> Self {
        self.details.result_path.push(part.to_string());
        self
    }
}

impl<F> Error<F>
where
    F: SourceContext + Clone,
{
    /// The error as shown to a user: the failing expression and the values
    /// involved as source snippets, then the trace.
    pub fn render(&self, options: &RenderOptions) -> String {
        render::render(self, options)
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

    const SOURCE: &str = "aaaa\nbbbb\ncccc\ndddd\n";

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

        fn display_path(&self) -> String {
            "mem".to_string()
        }
    }

    /// A location covering the whole line that starts at `byte`
    fn loc(byte: usize) -> Option<Loc<MemFile>> {
        Some(Loc::new(MemFile(SOURCE), byte..byte + 4))
    }

    fn plain(full_trace: bool) -> RenderOptions {
        RenderOptions {
            color: false,
            full_trace,
        }
    }

    #[test]
    fn origin_is_the_end_of_the_via_chain() {
        let written = Loc::new(MemFile(SOURCE), 0..4);
        let referenced = Loc::chain(loc(5), Some(written.clone())).unwrap();
        let again = Loc::chain(loc(10), Some(referenced)).unwrap();
        assert_eq!(again.span, 10..14);
        assert_eq!(again.origin().span, written.span);
        assert_eq!(written.origin().span, written.span);
    }

    #[test]
    fn long_chains_are_dropped_without_recursion() {
        // Either chain as long as a deep recursion makes it
        let mut by_via = Loc::new(MemFile(SOURCE), 0..4);
        let mut by_ctx = Loc::new(MemFile(SOURCE), 0..4);
        for _ in 0..500_000 {
            by_via = Loc::chain(loc(5), Some(by_via)).unwrap();
            let mut next = Loc::new(MemFile(SOURCE), 5..9);
            next.ctx = Some(Rc::new(by_ctx));
            by_ctx = next;
        }
        assert_eq!(by_ctx.calls().count(), 500_000);
        drop(by_via);
        drop(by_ctx);
    }

    #[test]
    fn the_calls_of_the_site_are_rendered() {
        let mut outer_call = Loc::new(MemFile(SOURCE), 10..14);
        outer_call.ctx = None;
        let mut inner_call = Loc::new(MemFile(SOURCE), 5..9);
        inner_call.ctx = Some(Rc::new(outer_call));
        let mut site = Loc::new(MemFile(SOURCE), 0..4);
        site.ctx = Some(Rc::new(inner_call));

        let err: Error<MemFile> = Error::new(ErrorType::Eval, "boom").reref(&Some(site));
        let out = err.render(&plain(false));
        assert!(
            out.contains("Called from:\n  mem:2:1\n      bbbb\n  mem:3:1\n      cccc\n"),
            "{out}"
        );
        let plain = format!("{err}");
        assert!(
            plain.contains("Called from:\n    1 - mem:5\n    2 - mem:10\n"),
            "{plain}"
        );
    }

    #[test]
    fn first_reref_is_the_site_and_the_rest_the_trace() {
        let err: Error<MemFile> = Error::new(ErrorType::Eval, "boom")
            .reref(&loc(0))
            .reref(&loc(5));
        let out = err.render(&plain(false));
        assert!(out.contains("Eval error: boom"), "{out}");
        // The site is a snippet with the span marked...
        assert!(out.contains("--> mem:1:1"), "{out}");
        assert!(out.contains("1 | aaaa"), "{out}");
        assert!(out.contains("^^^^"), "{out}");
        // ...and the other location a trace frame with its source line
        assert!(out.contains("Needed by:\n  mem:2:1\n      bbbb\n"), "{out}");
    }

    #[test]
    fn labels_and_help_are_rendered_with_their_text() {
        let err: Error<MemFile> = Error::new(ErrorType::Type, "boom")
            .primary(&loc(5), "fails here")
            .label(&loc(15), "this value")
            .help("try something else");
        let out = err.render(&plain(false));
        assert!(out.contains("Type error: boom"), "{out}");
        assert!(out.contains("fails here"), "{out}");
        assert!(out.contains("this value"), "{out}");
        assert!(out.contains("help: try something else"), "{out}");
        assert!(!out.contains("Needed by"), "{out}");
    }

    #[test]
    fn color_is_only_used_when_asked_for() {
        let err: Error<MemFile> = Error::new(ErrorType::Eval, "boom")
            .reref(&loc(0))
            .reref(&loc(5));
        assert!(!err.render(&plain(false)).contains('\x1b'));
        let colored = err.render(&RenderOptions {
            color: true,
            full_trace: false,
        });
        assert!(colored.contains('\x1b'), "{colored}");
    }

    #[test]
    fn trace_merges_repeated_lines_and_is_cut_unless_asked_for() {
        let source: &'static str = Box::leak(
            (0..40)
                .map(|i| format!("line{i:02}\n"))
                .collect::<String>()
                .into_boxed_str(),
        );
        let at = |line: usize| Some(Loc::new(MemFile(source), line * 7..line * 7 + 6));
        let mut err: Error<MemFile> = Error::new(ErrorType::Eval, "boom").primary(&at(0), "");
        // The site's own line and a repeated line are left out
        err = err.reref(&at(0)).reref(&at(1)).reref(&at(1));
        for line in 2..30 {
            err = err.reref(&at(line));
        }
        let out = err.render(&plain(false));
        assert_eq!(out.matches("\n  mem:").count(), 10, "{out}");
        assert_eq!(out.matches("mem:2:1").count(), 1, "{out}");
        assert!(out.contains("19 more"), "{out}");
        assert!(out.contains("--trace"), "{out}");

        let full = err.render(&plain(true));
        assert_eq!(full.matches("\n  mem:").count(), 29, "{full}");
        assert!(!full.contains("more"), "{full}");
    }

    #[test]
    fn trace_frames_say_what_they_needed_the_value_for() {
        let err: Error<MemFile> = Error::new(ErrorType::Eval, "boom")
            .reref(&loc(0))
            .reref_as(&loc(5), "left side of `++`")
            .reref(&loc(10))
            .reref_as(&loc(15), "field `src`");
        let out = err.render(&plain(false));
        // Aligned in a column in front of the locations, also for a frame
        // that has no description
        assert!(
            out.contains(
                "Needed by:\n  left side of `++`  mem:2:1\n      bbbb\n                     \
                 mem:3:1\n      cccc\n  field `src`        mem:4:1\n      dddd\n"
            ),
            "{out}"
        );
        let plain = format!("{err}");
        assert!(plain.contains("    2 - mem:5 (left side of `++`)\n"), "{plain}");
        assert!(plain.contains("    3 - mem:10\n"), "{plain}");
    }

    #[test]
    fn an_error_without_a_location_is_only_its_message() {
        let err: Error<MemFile> = Error::new(ErrorType::Parse, "mem:3\nUnrecognized token");
        assert_eq!(
            err.render(&plain(false)),
            "Parse error:\n\nmem:3\nUnrecognized token\n"
        );
        let custom: Error<MemFile> = Error::new(ErrorType::Custom, "File not found: x");
        assert_eq!(custom.render(&plain(false)), "File not found: x\n");
    }

    #[test]
    fn plain_display_needs_no_source_text() {
        let err: Error<MemFile> = Error::new(ErrorType::Eval, "boom")
            .primary(&loc(0), "fails here")
            .label(&loc(5), "this value")
            .help("try something else")
            .reref(&loc(10))
            .context("the build argument `input`")
            .in_result("[2]")
            .in_result(".out");
        assert_eq!(
            format!("{err}"),
            "Eval error:\n\nboom\n\n  fails here: mem:0\n  this value: mem:5\n  \
             help: try something else\n\nWhile evaluating the build argument `input`\n\
             In the result at .out[2]\n\nBacktrace:\n    \
             1 - mem:0\n    2 - mem:10\n"
        );
    }
}
