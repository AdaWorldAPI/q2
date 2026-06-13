/*
 * quarto-project-create
 * Copyright (c) 2025 Posit, PBC
 *
 * Project scaffolding for Quarto projects.
 *
 * This crate provides functionality to create new Quarto projects with
 * appropriate scaffold files. It is platform-agnostic: templates are
 * embedded at compile time via `include_str!()` and rendered with
 * `quarto-doctemplate` (Pandoc template syntax), which is pure Rust and
 * works identically on native and wasm32 targets — no JS runtime involved.
 *
 * # Usage
 *
 * ```ignore
 * use quarto_project_create::{create_project, CreateProjectOptions, ProjectType};
 *
 * let options = CreateProjectOptions::new(ProjectType::Website, "My Website");
 * let files = create_project(options)?;
 *
 * for file in files {
 *     println!("Create: {} ({} bytes)", file.path.display(), file.content.len());
 * }
 * ```
 */

mod choices;
mod scaffold;
mod templates;
mod types;

pub use choices::{
    ProjectChoice, ProjectTypeWithTemplate, available_choices, find_choice,
    find_implemented_choice, implemented_choices,
};
pub use scaffold::{
    ProjectScaffold, ScaffoldContent, ScaffoldFileDef, ScaffoldedFile, get_scaffold,
};
pub use types::{CreateError, CreateProjectOptions, ProjectFile, ProjectType};

use quarto_doctemplate::{Template, TemplateContext, TemplateValue};
use std::path::PathBuf;

/// Escape a string for interpolation inside a YAML double-quoted scalar.
///
/// Scaffold templates interpolate `$title$` only inside double-quoted YAML
/// strings (`title: "$title$"`), so the value must be escaped for that
/// context — otherwise a title containing `"` or a newline would produce
/// invalid YAML.
fn yaml_escape_double_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// Build the template context shared by all scaffold templates.
fn template_context(title: &str, project_type: &str, template: Option<&str>) -> TemplateContext {
    let mut ctx = TemplateContext::new();
    ctx.insert(
        "title",
        TemplateValue::String(yaml_escape_double_quoted(title)),
    );
    ctx.insert(
        "projectType",
        TemplateValue::String(project_type.to_string()),
    );
    if let Some(template) = template {
        ctx.insert("template", TemplateValue::String(template.to_string()));
    }
    ctx
}

/// Compile and render a single scaffold template.
fn render_template(template: &str, ctx: &TemplateContext) -> Result<String, CreateError> {
    let compiled =
        Template::compile(template).map_err(|e| CreateError::TemplateRender(e.to_string()))?;
    compiled
        .render(ctx)
        .map_err(|e| CreateError::TemplateRender(e.to_string()))
}

/// Create a new Quarto project with the given options.
///
/// This renders all template files for the specified project type and returns
/// the list of files to be created. The caller is responsible for writing
/// the files to disk or VFS.
///
/// # Arguments
///
/// * `options` - Project creation options (type, title, etc.)
///
/// # Returns
///
/// A list of `ProjectFile` structs containing the path and rendered content
/// for each file in the project scaffold.
///
/// # Errors
///
/// Returns `CreateError::TemplateRender` if template rendering fails.
///
/// # Example
///
/// ```ignore
/// let files = create_project(CreateProjectOptions::new(
///     ProjectType::Website,
///     "My Website"
/// ))?;
/// ```
pub fn create_project(options: CreateProjectOptions) -> Result<Vec<ProjectFile>, CreateError> {
    let ctx = template_context(&options.title, options.project_type.id(), None);

    // Get templates for this project type
    let template_files = templates::get_templates(options.project_type);

    // Render each template
    let mut files = Vec::with_capacity(template_files.len());

    for template_file in template_files {
        let content = render_template(template_file.template, &ctx)?;

        files.push(ProjectFile {
            path: PathBuf::from(template_file.path),
            content,
        });
    }

    Ok(files)
}

// ============================================================================
// New scaffold-based API
// ============================================================================

/// Options for creating a project from a choice.
#[derive(Debug, Clone)]
pub struct CreateFromChoiceOptions {
    /// The choice ID (e.g., "website", "blog")
    pub choice_id: String,

    /// Project title (used in templates)
    pub title: String,
}

impl CreateFromChoiceOptions {
    /// Create new options.
    pub fn new(choice_id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            choice_id: choice_id.into(),
            title: title.into(),
        }
    }
}

/// Create a new project from a user-facing choice.
///
/// This is the primary API for creating projects with template aliasing support.
/// The `choice_id` maps to a `ProjectChoice` which may resolve to a different
/// internal project type (e.g., "blog" → website:blog).
///
/// # Arguments
///
/// * `options` - Project creation options (choice ID, title)
///
/// # Returns
///
/// A list of `ScaffoldedFile` structs containing text and/or binary content.
///
/// # Errors
///
/// Returns `CreateError::UnknownProjectType` if the choice ID is not found.
/// Returns `CreateError::InvalidConfig` if the choice is not implemented.
/// Returns `CreateError::TemplateRender` if template rendering fails.
///
/// # Example
///
/// ```ignore
/// let files = create_project_from_choice(
///     CreateFromChoiceOptions::new("website", "My Website")
/// )?;
/// ```
pub fn create_project_from_choice(
    options: CreateFromChoiceOptions,
) -> Result<Vec<ScaffoldedFile>, CreateError> {
    // Look up the choice
    let choice = find_choice(&options.choice_id)
        .ok_or_else(|| CreateError::UnknownProjectType(options.choice_id.clone()))?;

    // Check if implemented
    if !choice.implemented {
        return Err(CreateError::InvalidConfig(format!(
            "Project type '{}' is not yet implemented",
            choice.name
        )));
    }

    // Get the scaffold
    let scaffold_opt = get_scaffold(&choice.target);
    let scaffold = scaffold_opt.ok_or_else(|| {
        CreateError::InvalidConfig(format!(
            "No scaffold defined for {}",
            choice.target.to_id_string()
        ))
    })?;

    // Render the scaffold
    create_scaffolded_files(&scaffold, &options.title)
}

/// Create files from a project scaffold.
///
/// This is a lower-level API that takes a `ProjectScaffold` directly.
/// Use `create_project_from_choice` for the higher-level API with
/// template aliasing support.
///
/// # Arguments
///
/// * `scaffold` - The project scaffold definition
/// * `title` - Project title (used in templates)
///
/// # Returns
///
/// A list of `ScaffoldedFile` structs ready to be written to disk or VFS.
pub fn create_scaffolded_files(
    scaffold: &ProjectScaffold,
    title: &str,
) -> Result<Vec<ScaffoldedFile>, CreateError> {
    let ctx = template_context(
        title,
        scaffold.target.project_type.id(),
        scaffold.target.template.as_deref(),
    );

    let mut files = Vec::with_capacity(scaffold.files.len());

    for file_def in &scaffold.files {
        let path = file_def.full_path();

        match &file_def.content {
            ScaffoldContent::Template(template) => {
                let content = render_template(template, &ctx)?;

                files.push(ScaffoldedFile::Text { path, content });
            }
            ScaffoldContent::StaticText(text) => {
                files.push(ScaffoldedFile::Text {
                    path,
                    content: (*text).to_string(),
                });
            }
            ScaffoldContent::Binary { content, mime_type } => {
                files.push(ScaffoldedFile::Binary {
                    path,
                    content: content.to_vec(),
                    mime_type: (*mime_type).to_string(),
                });
            }
        }
    }

    Ok(files)
}

/// Get information about available project types.
///
/// Returns information useful for building UI selection dialogs.
pub fn available_project_types() -> Vec<ProjectTypeInfo> {
    ProjectType::implemented()
        .iter()
        .map(|pt| ProjectTypeInfo {
            id: pt.id().to_string(),
            name: pt.display_name().to_string(),
            description: project_type_description(*pt).to_string(),
        })
        .collect()
}

/// Information about a project type for UI display.
#[derive(Debug, Clone)]
pub struct ProjectTypeInfo {
    /// Lowercase identifier (e.g., "website")
    pub id: String,
    /// Display name (e.g., "Website")
    pub name: String,
    /// Short description
    pub description: String,
}

/// Get a short description for a project type.
fn project_type_description(project_type: ProjectType) -> &'static str {
    match project_type {
        ProjectType::Default => "A minimal Quarto project",
        ProjectType::Website => "A Quarto website with navigation",
        ProjectType::Blog => "A blog using the Quarto blog template",
        ProjectType::Manuscript => "An academic manuscript",
        ProjectType::Book => "A multi-chapter book",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_type_from_str() {
        assert_eq!(
            "website".parse::<ProjectType>().unwrap(),
            ProjectType::Website
        );
        assert_eq!(
            "default".parse::<ProjectType>().unwrap(),
            ProjectType::Default
        );
        assert!("invalid".parse::<ProjectType>().is_err());
    }

    #[test]
    fn test_project_type_display() {
        assert_eq!(ProjectType::Website.to_string(), "Website");
        assert_eq!(ProjectType::Default.to_string(), "Default");
    }

    #[test]
    fn test_available_project_types() {
        let types = available_project_types();
        assert!(!types.is_empty());

        // Should have at least default and website
        let ids: Vec<_> = types.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"default"));
        assert!(ids.contains(&"website"));
    }

    #[test]
    fn test_create_project_options() {
        let options = CreateProjectOptions::new(ProjectType::Website, "My Site");
        assert_eq!(options.project_type, ProjectType::Website);
        assert_eq!(options.title, "My Site");
    }

    #[test]
    fn test_yaml_escape_double_quoted() {
        assert_eq!(yaml_escape_double_quoted("plain title"), "plain title");
        assert_eq!(yaml_escape_double_quoted(r#"a "b" c"#), r#"a \"b\" c"#);
        assert_eq!(yaml_escape_double_quoted(r"back\slash"), r"back\\slash");
        assert_eq!(yaml_escape_double_quoted("a\nb"), r"a\nb");
        assert_eq!(yaml_escape_double_quoted("a\tb"), r"a\tb");
        assert_eq!(yaml_escape_double_quoted("a\r\nb"), r"a\r\nb");
    }
}

// Rendering tests for the doctemplate-based scaffolding path.
//
// These run on every platform: template rendering is pure Rust
// (quarto-doctemplate), with no JS runtime involved.
#[cfg(test)]
mod render_tests {
    use super::*;

    #[test]
    fn test_create_default_project() {
        let options = CreateProjectOptions::new(ProjectType::Default, "Test Project");

        let files = create_project(options).unwrap();

        // Should have exactly one file: _quarto.yml
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path.to_str().unwrap(), "_quarto.yml");

        // The title must land inside the YAML double-quoted string
        assert!(files[0].content.contains("title: \"Test Project\""));
        assert!(files[0].content.contains("project:"));

        // No template-syntax residue of either engine
        assert!(!files[0].content.contains("$title$"));
        assert!(!files[0].content.contains("<%"));
    }

    #[test]
    fn test_create_website_project() {
        let options = CreateProjectOptions::new(ProjectType::Website, "My Website");

        let files = create_project(options).unwrap();

        // Should have two files: _quarto.yml and index.qmd
        assert_eq!(files.len(), 2);

        let paths: Vec<_> = files.iter().map(|f| f.path.to_str().unwrap()).collect();
        assert!(paths.contains(&"_quarto.yml"));
        assert!(paths.contains(&"index.qmd"));

        // Check _quarto.yml content
        let quarto_yml = files
            .iter()
            .find(|f| f.path.to_str() == Some("_quarto.yml"))
            .unwrap();
        assert!(quarto_yml.content.contains("title: \"My Website\""));
        assert!(quarto_yml.content.contains("type: website"));

        // Check index.qmd content (title in YAML front matter)
        let index_qmd = files
            .iter()
            .find(|f| f.path.to_str() == Some("index.qmd"))
            .unwrap();
        assert!(index_qmd.content.contains("title: \"My Website\""));
        assert!(index_qmd.content.contains("Quarto website"));

        // No template-syntax residue in any file
        for file in &files {
            assert!(!file.content.contains("$title$"));
            assert!(!file.content.contains("<%"));
        }
    }

    #[test]
    fn test_title_special_characters_yaml_escaped() {
        let options =
            CreateProjectOptions::new(ProjectType::Default, r#"R & D "quoted" \ backslash"#);

        let files = create_project(options).unwrap();
        assert_eq!(files.len(), 1);

        // `&` passes through raw. (The old EJS path HTML-escaped it to
        // `&amp;` — a latent bug for YAML output.)
        assert!(!files[0].content.contains("&amp;"));

        // `"` and `\` are escaped for the YAML double-quoted context,
        // keeping the output valid YAML.
        assert!(
            files[0]
                .content
                .contains(r#"title: "R & D \"quoted\" \\ backslash""#),
            "unexpected rendering: {}",
            files[0].content
        );
    }

    #[test]
    fn test_title_with_newline_yaml_escaped() {
        let options = CreateProjectOptions::new(ProjectType::Default, "Line one\nLine two");

        let files = create_project(options).unwrap();
        assert_eq!(files.len(), 1);

        // A literal newline would break the single-line YAML string; it must
        // be emitted as the YAML escape sequence `\n`.
        assert!(files[0].content.contains(r#"title: "Line one\nLine two""#));
    }

    // ========================================================================
    // Scaffold-based API tests
    // ========================================================================

    #[test]
    fn test_create_project_from_choice_website() {
        let options = CreateFromChoiceOptions::new("website", "My Website");

        let files = create_project_from_choice(options).unwrap();

        // Should have two files: _quarto.yml and index.qmd
        assert_eq!(files.len(), 2);

        // Check we have the expected files
        let text_files: Vec<_> = files
            .iter()
            .filter_map(|f| match f {
                ScaffoldedFile::Text { path, content } => {
                    Some((path.to_str().unwrap(), content.as_str()))
                }
                ScaffoldedFile::Binary { .. } => None,
            })
            .collect();

        let paths: Vec<_> = text_files.iter().map(|(p, _)| *p).collect();
        assert!(paths.contains(&"_quarto.yml"));
        assert!(paths.contains(&"index.qmd"));

        // Check _quarto.yml content
        let (_, quarto_yml) = text_files
            .iter()
            .find(|(p, _)| *p == "_quarto.yml")
            .unwrap();
        assert!(quarto_yml.contains("title: \"My Website\""));
        assert!(quarto_yml.contains("type: website"));
    }

    #[test]
    fn test_create_project_from_choice_default() {
        let options = CreateFromChoiceOptions::new("default", "Test Project");

        let files = create_project_from_choice(options).unwrap();

        // Should have one file: _quarto.yml
        assert_eq!(files.len(), 1);
        assert!(files[0].is_text());

        if let ScaffoldedFile::Text { path, content } = &files[0] {
            assert_eq!(path.to_str().unwrap(), "_quarto.yml");
            assert!(content.contains("title: \"Test Project\""));
        } else {
            panic!("Expected text file");
        }
    }

    #[test]
    fn test_create_project_from_choice_unknown() {
        let options = CreateFromChoiceOptions::new("nonexistent", "Test");

        let result = create_project_from_choice(options);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CreateError::UnknownProjectType(_)
        ));
    }

    #[test]
    fn test_create_project_from_choice_unimplemented() {
        // "blog" is defined but marked as unimplemented
        let options = CreateFromChoiceOptions::new("blog", "My Blog");

        let result = create_project_from_choice(options);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CreateError::InvalidConfig(_)));
    }

    #[test]
    fn test_implemented_choices_are_usable() {
        // All implemented choices should successfully create projects
        for choice in implemented_choices() {
            let options = CreateFromChoiceOptions::new(&choice.id, "Test Project");
            let result = create_project_from_choice(options);

            assert!(
                result.is_ok(),
                "Failed to create project for implemented choice '{}': {:?}",
                choice.id,
                result.err()
            );
        }
    }
}
