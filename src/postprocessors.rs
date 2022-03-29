//! A collection of officially maintained [postprocessors][crate::Postprocessor].

use super::{Context, MarkdownEvents, PostprocessorResult};
use pulldown_cmark::Event;

/// This postprocessor converts all soft line breaks to hard line breaks. Enabling this mimics
/// Obsidian's _'Strict line breaks'_ setting.
pub fn softbreaks_to_hardbreaks(
    _context: &mut Context,
    events: &mut MarkdownEvents,
) -> PostprocessorResult {
    for event in events.iter_mut() {
        if event == &Event::SoftBreak {
            *event = Event::HardBreak;
        }
    }
    PostprocessorResult::Continue
}

/// A postprocessor which rejects any notes without `publish: true` in their
/// frontmatter.
pub fn only_published_filter(
    context: &mut Context,
    _events: &mut MarkdownEvents,
) -> PostprocessorResult {
    let publish_key = serde_yaml::Value::String("publish".to_owned());
    match context.frontmatter.get(&publish_key) {
        Some(serde_yaml::Value::Bool(true)) => PostprocessorResult::Continue,
        _ => PostprocessorResult::StopAndSkipNote,
    }
}
