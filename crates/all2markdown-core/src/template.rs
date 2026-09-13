use crate::format::Format;
use std::fmt;
use std::path::{Component, Path, PathBuf};

/// The model naming the file produced from a Source Document.
///
/// Variable substitution and nothing else: one input yields one file, there is
/// no pattern matching. The template is parsed once, up front, so that a typo
/// in a variable name is refused before the first document is opened rather
/// than discovered a million documents in.
///
/// The default, `{name}.md`, keeps the source extension in the produced name:
/// `report.doc` and `report.pdf` become `report.doc.md` and `report.pdf.md`,
/// so two documents differing only by extension cannot overwrite one another.
/// Collisions are impossible by construction, and the output is the same from
/// one run to the next, which a suffix-on-collision scheme could not promise
/// under parallel execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputTemplate {
    pieces: Vec<Piece>,
}

/// One variable an Output Template may name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Variable {
    /// The source file name, extension included: `report.doc`.
    Name,
    /// The source file name without its last extension: `report`.
    Stem,
    /// The source's last extension, without the dot: `doc`. Empty when there
    /// is none.
    Ext,
    /// The source's parent directory, as given, made relative: a leading root
    /// and any `.` or `..` component are dropped, so the produced path always
    /// lands under the output directory.
    Parent,
    /// The id of the Supported Format the document was read as: `docx`.
    Format,
}

impl Variable {
    const ALL: [Variable; 5] = [
        Variable::Name,
        Variable::Stem,
        Variable::Ext,
        Variable::Parent,
        Variable::Format,
    ];

    fn name(self) -> &'static str {
        match self {
            Variable::Name => "name",
            Variable::Stem => "stem",
            Variable::Ext => "ext",
            Variable::Parent => "parent",
            Variable::Format => "format",
        }
    }

    fn from_name(name: &str) -> Option<Variable> {
        Variable::ALL.into_iter().find(|v| v.name() == name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Literal(String),
    Variable(Variable),
}

/// Why an Output Template was refused.
///
/// Raised when the template is parsed, before any document is read: a Batch
/// must not fail on its last document for a mistake visible on its first.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TemplateError {
    /// A `{...}` naming no variable this template knows.
    UnknownVariable(String),
    /// A `{` never closed, or a `}` never opened.
    UnbalancedBrace,
    /// A template that renders to nothing at all.
    Empty,
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TemplateError::UnknownVariable(name) => {
                write!(f, "unknown template variable {{{name}}}; expected one of ")?;
                for (i, variable) in Variable::ALL.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{{{}}}", variable.name())?;
                }
                Ok(())
            }
            TemplateError::UnbalancedBrace => {
                f.write_str("unbalanced brace in template: every { needs its }")
            }
            TemplateError::Empty => f.write_str("the template is empty"),
        }
    }
}

impl std::error::Error for TemplateError {}

impl Default for OutputTemplate {
    fn default() -> Self {
        OutputTemplate::parse(OutputTemplate::DEFAULT).expect("the default template parses")
    }
}

impl OutputTemplate {
    /// The template used when the caller names none.
    pub const DEFAULT: &'static str = "{name}.md";

    /// Parse a template, refusing any variable it does not know.
    pub fn parse(template: &str) -> Result<OutputTemplate, TemplateError> {
        if template.is_empty() {
            return Err(TemplateError::Empty);
        }

        let mut pieces = Vec::new();
        let mut rest = template;
        while !rest.is_empty() {
            match rest.find(['{', '}']) {
                None => {
                    pieces.push(Piece::Literal(rest.to_owned()));
                    break;
                }
                Some(open) => {
                    if !rest[open..].starts_with('{') {
                        return Err(TemplateError::UnbalancedBrace);
                    }
                    if open > 0 {
                        pieces.push(Piece::Literal(rest[..open].to_owned()));
                    }
                    let after = &rest[open + 1..];
                    let close = after.find('}').ok_or(TemplateError::UnbalancedBrace)?;
                    let name = &after[..close];
                    if name.contains('{') {
                        return Err(TemplateError::UnbalancedBrace);
                    }
                    let variable = Variable::from_name(name)
                        .ok_or_else(|| TemplateError::UnknownVariable(name.to_owned()))?;
                    pieces.push(Piece::Variable(variable));
                    rest = &after[close + 1..];
                }
            }
        }
        Ok(OutputTemplate { pieces })
    }

    /// The relative path of the file produced for a Source Document read from
    /// `source` as `format`.
    ///
    /// Always relative, so joining it onto the output directory never escapes
    /// it: `{parent}` is stripped of its root and of any `..`.
    pub fn render(&self, source: &Path, format: Format) -> PathBuf {
        let mut rendered = String::new();
        for piece in &self.pieces {
            match piece {
                Piece::Literal(text) => rendered.push_str(text),
                Piece::Variable(variable) => {
                    rendered.push_str(&substitute(*variable, source, format))
                }
            }
        }
        let path = PathBuf::from(rendered);
        // The literal parts may still say `/x` or `../x`; the same rule applies
        // to the whole as to `{parent}`.
        relative(&path)
    }
}

fn substitute(variable: Variable, source: &Path, format: Format) -> String {
    let lossy = |os: Option<&std::ffi::OsStr>| {
        os.map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    match variable {
        Variable::Name => lossy(source.file_name()),
        Variable::Stem => lossy(source.file_stem()),
        Variable::Ext => lossy(source.extension()),
        Variable::Parent => source
            .parent()
            .map(|parent| relative(parent).to_string_lossy().into_owned())
            .unwrap_or_default(),
        Variable::Format => format.id().to_owned(),
    }
}

/// The path with its root and every `.` or `..` dropped: something that,
/// joined onto a directory, stays inside it.
fn relative(path: &Path) -> PathBuf {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_keeps_the_source_extension() {
        let template = OutputTemplate::default();
        assert_eq!(
            template.render(Path::new("corpus/report.doc"), Format::DOC),
            PathBuf::from("report.doc.md")
        );
        assert_eq!(
            template.render(Path::new("corpus/report.pdf"), Format::PDF),
            PathBuf::from("report.pdf.md")
        );
    }

    #[test]
    fn every_variable_substitutes() {
        let template = OutputTemplate::parse("{parent}/{stem}-{ext}-{format}-{name}.md").unwrap();
        assert_eq!(
            template.render(Path::new("/data/corpus/report.doc"), Format::DOC),
            PathBuf::from("data/corpus/report-doc-doc-report.doc.md")
        );
    }

    #[test]
    fn an_unknown_variable_is_refused_at_parse_time() {
        assert_eq!(
            OutputTemplate::parse("{basename}.md"),
            Err(TemplateError::UnknownVariable("basename".to_owned()))
        );
    }

    #[test]
    fn a_stray_brace_is_refused() {
        assert_eq!(
            OutputTemplate::parse("{name.md"),
            Err(TemplateError::UnbalancedBrace)
        );
        assert_eq!(
            OutputTemplate::parse("name}.md"),
            Err(TemplateError::UnbalancedBrace)
        );
        assert_eq!(
            OutputTemplate::parse("{{name}.md"),
            Err(TemplateError::UnbalancedBrace)
        );
    }

    #[test]
    fn an_empty_template_is_refused() {
        assert_eq!(OutputTemplate::parse(""), Err(TemplateError::Empty));
    }

    #[test]
    fn the_rendered_path_never_escapes_the_output_directory() {
        let template = OutputTemplate::parse("../{parent}/{name}.md").unwrap();
        assert_eq!(
            template.render(Path::new("../../etc/passwd"), Format::TXT),
            PathBuf::from("etc/passwd.md")
        );
        let template = OutputTemplate::parse("/{name}.md").unwrap();
        assert_eq!(
            template.render(Path::new("x.txt"), Format::TXT),
            PathBuf::from("x.txt.md")
        );
    }

    #[test]
    fn a_source_without_extension_renders_an_empty_ext() {
        let template = OutputTemplate::parse("{stem}.{ext}.md").unwrap();
        assert_eq!(
            template.render(Path::new("README"), Format::TXT),
            PathBuf::from("README..md")
        );
    }
}
