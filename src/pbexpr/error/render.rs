//! Rendering of an [`Error`] for a user: the failing expression and the
//! values involved as annotated source snippets, then the trace.
//!
//! The snippets are drawn by the `annotate-snippets` crate, and this module
//! is the only place that uses it.

use std::{collections::HashMap, fmt::Write};

use annotate_snippets::{Annotation, AnnotationKind, Group, Level, Renderer, Snippet};

use super::{Error, Label, Loc, SourceContext};

/// How [`Error::render`] should render.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// Use terminal colors
    pub color: bool,
    /// Show the whole trace, however long, instead of the first frames
    pub full_trace: bool,
}

/// Trace frames shown unless the whole trace is asked for.
const TRACE_FRAMES: usize = 10;

const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// The source texts of the files an error refers to, read once each and
/// keyed by the displayed path.
struct Sources(HashMap<String, Option<String>>);

impl Sources {
    fn get<F: SourceContext>(&mut self, file: &F) -> (String, Option<&str>) {
        let path = file.display_path();
        let text = self
            .0
            .entry(path.clone())
            .or_insert_with(|| file.source_text());
        (path, text.as_deref())
    }
}

/// A 1-based line and column (in bytes, as elsewhere) of an offset, and the
/// line's text.
fn line_of(source: &str, offset: usize) -> (usize, usize, &str) {
    let offset = offset.min(source.len());
    let line_start = source[..offset].rfind('\n').map_or(0, |pos| pos + 1);
    let line_end = source[offset..]
        .find('\n')
        .map_or(source.len(), |pos| offset + pos);
    let line_number = source[..line_start].matches('\n').count() + 1;
    (
        line_number,
        offset - line_start + 1,
        source[line_start..line_end].trim_end_matches('\r'),
    )
}

pub(super) fn render<F>(err: &Error<F>, options: &RenderOptions) -> String
where
    F: SourceContext + Clone,
{
    let mut sources = Sources(HashMap::new());
    let (site, trace) = err.site_and_trace();
    let mut out = String::new();

    if site.is_none() && err.details.labels.is_empty() {
        // Nothing to show in the source: the message is all there is, some
        // of them preformatted over several lines (parse errors).
        let _ = err.fmt_header(&mut out);
        for help in &err.details.help {
            let _ = writeln!(out, "help: {help}");
        }
    } else {
        out.push_str(&render_report(err, site.as_ref(), &mut sources, options));
        out.push('\n');
    }

    let _ = err.fmt_context(&mut out);

    if let Some(site) = &site {
        render_frames(
            &mut out,
            "Called from:",
            site,
            site.loc.calls(),
            &mut sources,
            options,
        );
        render_frames(
            &mut out,
            "Needed by:",
            site,
            trace.iter(),
            &mut sources,
            options,
        );
    }
    out
}

/// The message, with the site and the labels as annotated snippets, one
/// snippet for each file, and the help lines.
fn render_report<F>(
    err: &Error<F>,
    site: Option<&Label<F>>,
    sources: &mut Sources,
    options: &RenderOptions,
) -> String
where
    F: SourceContext + Clone,
{
    // Annotations for each file, in the order the files are first mentioned
    let mut files: Vec<(String, Option<String>, Vec<Annotation<'_>>)> = Vec::new();
    // Labels in a file without source text are listed as notes
    let mut notes: Vec<String> = Vec::new();

    // The site, then the labels. A label on the very expression that is the
    // site would draw the same span twice, so its text goes to the site.
    let mut site = site.cloned();
    let mut labels: Vec<&Label<F>> = Vec::new();
    for label in &err.details.labels {
        match &mut site {
            Some(site)
                if site.loc.span == label.loc.span
                    && site.loc.file.display_path() == label.loc.file.display_path() =>
            {
                if site.text.is_empty() {
                    site.text = label.text.clone();
                }
            }
            _ => labels.push(label),
        }
    }

    let annotated = site
        .iter()
        .map(|site| (AnnotationKind::Primary, site))
        .chain(
            labels
                .into_iter()
                .map(|label| (AnnotationKind::Context, label)),
        );
    for (kind, label) in annotated {
        let (path, text) = sources.get(&label.loc.file);
        let Some(text) = text else {
            notes.push(if label.text.is_empty() {
                format!("at {path}")
            } else {
                format!("{}, at {path}", label.text)
            });
            continue;
        };
        let end = label.loc.span.end.min(text.len());
        let start = label.loc.span.start.min(end);
        // A label says what a value is and where it was written; the first
        // line is enough for that, and a large value would bury the rest.
        let end = match kind {
            AnnotationKind::Primary => end,
            _ => text[start..end]
                .find('\n')
                .map_or(end, |newline| start + newline),
        };
        let annotation = kind
            .span(start..end)
            .label((!label.text.is_empty()).then(|| label.text.clone()));
        match files.iter_mut().find(|(file, _, _)| *file == path) {
            Some((_, _, annotations)) => annotations.push(annotation),
            None => files.push((path, Some(text.to_string()), vec![annotation])),
        }
    }

    let level = match err.typ.name() {
        Some(name) => Level::ERROR.with_name(Some(name)),
        None => Level::ERROR.no_name(),
    };
    let mut group = Group::with_title(level.primary_title(err.msg.as_str()));
    for (path, text, annotations) in files {
        group = group.element(
            Snippet::source(text.unwrap_or_default())
                .path(path)
                .annotations(annotations),
        );
    }
    for note in &notes {
        group = group.element(Level::NOTE.message(note.as_str()));
    }
    for help in &err.details.help {
        group = group.element(Level::HELP.message(help.as_str()));
    }

    let renderer = if options.color {
        Renderer::styled()
    } else {
        Renderer::plain()
    };
    renderer.render(&[group])
}

/// A list of locations, each printed as the location and its source line:
/// the calls the site was evaluated in, or the expressions that needed it.
/// Evaluation passes through the same line several times in a row (an
/// expression and its parts, or every round of a recursion), so only the
/// first of those is shown, and nothing for the line of the site itself.
fn render_frames<'a, F>(
    out: &mut String,
    title: &str,
    site: &Label<F>,
    locations: impl Iterator<Item = &'a Loc<F>>,
    sources: &mut Sources,
    options: &RenderOptions,
) where
    F: SourceContext + Clone + 'a,
{
    /// A frame ready to print: where it is, and its source line with the
    /// range of the expression within that line
    struct Frame {
        location: String,
        line: Option<(String, std::ops::Range<usize>)>,
    }

    let mut describe = |loc: &Loc<F>| -> ((String, usize), Frame) {
        let (path, text) = sources.get(&loc.file);
        match text {
            Some(text) => {
                let (line, column, line_text) = line_of(text, loc.span.start);
                let indent = line_text.len() - line_text.trim_start().len();
                let start = (column - 1).max(indent) - indent;
                let trimmed = line_text.trim_start();
                let end = (start + loc.span.len()).clamp(start, trimmed.len());
                (
                    (path.clone(), line),
                    Frame {
                        location: format!("{path}:{line}:{column}"),
                        line: Some((trimmed.to_string(), start..end)),
                    },
                )
            }
            None => (
                (path, loc.span.start),
                Frame {
                    location: loc.to_string(),
                    line: None,
                },
            ),
        }
    };

    let mut previous = Some(describe(&site.loc).0);
    let mut frames = Vec::new();
    for loc in locations {
        let (key, frame) = describe(loc);
        if previous.as_ref() != Some(&key) {
            frames.push(frame);
        }
        previous = Some(key);
    }
    if frames.is_empty() {
        return;
    }

    let shown = if options.full_trace {
        frames.len()
    } else {
        frames.len().min(TRACE_FRAMES)
    };
    let _ = writeln!(out, "\n{title}");
    for frame in &frames[..shown] {
        let _ = writeln!(out, "  {}", frame.location);
        if let Some((line, span)) = &frame.line {
            if options.color && !span.is_empty() {
                let _ = writeln!(
                    out,
                    "      {}{BOLD}{}{RESET}{}",
                    &line[..span.start],
                    &line[span.clone()],
                    &line[span.end..]
                );
            } else {
                let _ = writeln!(out, "      {line}");
            }
        }
    }
    if shown < frames.len() {
        let _ = writeln!(
            out,
            "  ({} more, --trace shows the whole trace)",
            frames.len() - shown
        );
    }
}
