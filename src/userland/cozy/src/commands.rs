use crate::action::Action;
use crate::state::EditorMode;

#[derive(Debug, Clone, PartialEq)]
pub enum CommandAction {
    Dispatch(Action),
    EnterMode(EditorMode),
    OpenConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandSpec {
    pub label: &'static str,
    pub keywords: &'static [&'static str],
    pub action: CommandAction,
}

fn namespace_prefix(query: &str) -> Option<&'static str> {
    match query {
        "m" | "mode" => Some("mode."),
        "s" | "search" => Some("search."),
        "f" | "file" => Some("file."),
        "b" | "browse" => Some("browse."),
        "n" | "nav" | "navigate" => Some("navigate."),
        "v" | "view" => Some("view."),
        "c" | "config" => Some("config."),
        "a" | "app" => Some("app."),
        _ => None,
    }
}

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        label: "Mode.Edit",
        keywords: &["insert", "type"],
        action: CommandAction::EnterMode(EditorMode::Edit),
    },
    CommandSpec {
        label: "Mode.Glide",
        keywords: &["vim", "navigation"],
        action: CommandAction::EnterMode(EditorMode::Glide),
    },
    CommandSpec {
        label: "Mode.Help",
        keywords: &["help"],
        action: CommandAction::EnterMode(EditorMode::Help),
    },
    CommandSpec {
        label: "Search.Find",
        keywords: &["find", "search"],
        action: CommandAction::EnterMode(EditorMode::Search),
    },
    CommandSpec {
        label: "Search.Replace",
        keywords: &["replace"],
        action: CommandAction::EnterMode(EditorMode::Replace),
    },
    CommandSpec {
        label: "File.SaveAs",
        keywords: &["write", "save"],
        action: CommandAction::EnterMode(EditorMode::Save),
    },
    CommandSpec {
        label: "File.Open",
        keywords: &["open file"],
        action: CommandAction::EnterMode(EditorMode::Open),
    },
    CommandSpec {
        label: "Browse.Files",
        keywords: &["folder", "directory", "tree"],
        action: CommandAction::EnterMode(EditorMode::Browse),
    },
    CommandSpec {
        label: "Navigate.GotoLine",
        keywords: &["jump", "line"],
        action: CommandAction::EnterMode(EditorMode::Goto),
    },
    CommandSpec {
        label: "View.Markdown",
        keywords: &["markdown", "preview"],
        action: CommandAction::Dispatch(Action::ToggleMarkdownPreview),
    },
    CommandSpec {
        label: "Review.SessionDiff",
        keywords: &[
            "diff", "review", "approve", "hunk", "git", "stage", "add", "commit",
        ],
        action: CommandAction::Dispatch(Action::ToggleDiffReview),
    },
    CommandSpec {
        label: "View.ToggleLineNumbers",
        keywords: &["line numbers", "gutter"],
        action: CommandAction::Dispatch(Action::ToggleLineNumbers),
    },
    CommandSpec {
        label: "View.ToggleWrap",
        keywords: &["wrap", "soft wrap"],
        action: CommandAction::Dispatch(Action::ToggleWrap),
    },
    CommandSpec {
        label: "View.ToggleFooter",
        keywords: &["footer", "shortcuts", "chrome"],
        action: CommandAction::Dispatch(Action::ToggleFooter),
    },
    CommandSpec {
        label: "Config.Open",
        keywords: &["open config", "settings"],
        action: CommandAction::OpenConfig,
    },
    CommandSpec {
        label: "Config.Reload",
        keywords: &["reload config", "settings"],
        action: CommandAction::Dispatch(Action::ReloadConfig),
    },
    CommandSpec {
        label: "App.Quit",
        keywords: &["exit", "quit"],
        action: CommandAction::EnterMode(EditorMode::Quit),
    },
    CommandSpec {
        label: "App.QuitWithoutSaving",
        keywords: &["force quit", "quit without saving", "exit without saving"],
        action: CommandAction::Dispatch(Action::Quit),
    },
];

pub fn filtered_commands(query: &str) -> Vec<&'static CommandSpec> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return COMMANDS.iter().collect();
    }

    let namespace = namespace_prefix(&query);
    let has_namespace_separator = query.contains('.');
    let query_has_whitespace = query.chars().any(|c| c.is_whitespace());
    let allow_keyword_match = query.chars().count() > 1;

    COMMANDS
        .iter()
        .filter(|command| {
            let label = command.label.to_ascii_lowercase();
            (namespace.is_some() && label.starts_with(namespace.unwrap()))
                || label_matches(&label, &query, has_namespace_separator)
                || (allow_keyword_match
                    && command
                        .keywords
                        .iter()
                        .any(|keyword| keyword_matches(keyword, &query, query_has_whitespace)))
        })
        .collect()
}

fn label_matches(label: &str, query: &str, has_namespace_separator: bool) -> bool {
    if has_namespace_separator {
        return label.starts_with(query);
    }

    label.split('.').any(|segment| segment.starts_with(query))
}

fn keyword_matches(keyword: &str, query: &str, query_has_whitespace: bool) -> bool {
    let keyword = keyword.to_ascii_lowercase();
    if query_has_whitespace {
        return keyword.contains(query);
    }

    keyword
        .split(|c: char| !c.is_alphanumeric())
        .any(|token| !token.is_empty() && token.starts_with(query))
}

pub fn label_completion(query: &str) -> Option<String> {
    let query = query.trim();
    if query.is_empty() {
        return None;
    }

    let labels: Vec<&str> = filtered_commands(query)
        .into_iter()
        .map(|command| command.label)
        .collect();

    let first = labels.first()?;
    let common_len = labels.iter().skip(1).fold(first.len(), |len, label| {
        common_prefix_len(&first[..len], label)
    });
    Some(first[..common_len].to_string())
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    let mut len = 0;
    for ((a_index, a_char), (_, b_char)) in a.char_indices().zip(b.char_indices()) {
        if !a_char.eq_ignore_ascii_case(&b_char) {
            break;
        }
        len = a_index + a_char.len_utf8();
    }
    len
}
