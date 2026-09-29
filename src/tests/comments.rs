use crate::app::{App, CommentGutterState, ReviewAgentState, ReviewCommentStatus};
use ratatui::{backend::TestBackend, buffer::Buffer, layout::Rect, text::Line, Terminal};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn comment_app() -> App {
    comment_app_with_path(None)
}

fn comment_app_with_path(filepath: Option<PathBuf>) -> App {
    let lines = vec![
        Line::from("one"),
        Line::from("two"),
        Line::from("two wrapped"),
        Line::from("three"),
        Line::from(""),
    ];
    let mut app = App::new(
        lines,
        vec![],
        "review.md".to_string(),
        false,
        false,
        filepath,
        None,
    );
    app.set_line_maps(vec![1, 2, 2, 3, 4], vec![1, 2, 2, 3, 4]);
    app.content_area = Rect::new(0, 0, 100, 3);
    app
}

fn add_comment(app: &mut App, rendered_line: usize, body: &str) -> u64 {
    assert!(app.begin_comment_at_rendered_line(rendered_line));
    for ch in body.chars() {
        app.push_comment_char(ch);
    }
    assert!(app.save_comment());
    app.active_comment_id().unwrap()
}

fn draw(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| crate::render::ui(frame, app))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn buffer_text(buffer: &Buffer) -> String {
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer.cell((x, y)).unwrap().symbol());
        }
        text.push('\n');
    }
    text
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("leaf-{label}-{}-{nonce}", std::process::id()))
}

fn only_json_file(directory: &std::path::Path) -> PathBuf {
    let files = fs::read_dir(directory)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 1);
    files[0].clone()
}

#[test]
fn comment_can_be_added_to_a_source_line() {
    let mut app = comment_app();

    add_comment(&mut app, 1, "  Clarify this sentence.  ");

    assert_eq!(app.comment_count(), 1);
    assert_eq!(app.comments()[0].source_line, 2);
    assert_eq!(app.comments()[0].body, "Clarify this sentence.");
    assert!(!app.is_comment_composer_open());
}

#[test]
fn keyboard_cursor_focuses_a_line_then_words_before_commenting() {
    let mut app = comment_app();

    assert!(app.start_comment_cursor());
    assert_eq!(app.comment_cursor().unwrap().rendered_line, 0);
    assert_eq!(app.comment_cursor_word_focus(), None);

    assert!(app.move_comment_cursor_down());
    assert!(app.move_comment_cursor_down());
    assert!(app.move_comment_cursor_word_next());
    assert_eq!(
        app.comment_cursor_word_focus().unwrap().text,
        "two".to_string()
    );
    assert!(app.move_comment_cursor_word_next());
    assert_eq!(
        app.comment_cursor_word_focus().unwrap().text,
        "wrapped".to_string()
    );

    assert!(app.begin_comment_at_focus());
    let composer = app.comment_composer().unwrap();
    assert_eq!(composer.source_line, 2);
    assert_eq!(composer.selected_text.as_deref(), Some("wrapped"));
    assert!(!app.is_comment_cursor_active());

    for ch in "Use a more specific term".chars() {
        app.push_comment_char(ch);
    }
    assert!(app.save_comment());
    assert_eq!(app.comments()[0].selected_text.as_deref(), Some("wrapped"));
}

#[test]
fn keyboard_word_navigation_can_return_to_whole_line_focus() {
    let mut app = comment_app();
    assert!(app.start_comment_cursor());
    assert!(app.move_comment_cursor_word_next());
    assert!(app.comment_cursor_word_focus().is_some());

    assert!(app.move_comment_cursor_word_previous());

    assert!(app.comment_cursor_word_focus().is_none());
}

#[test]
fn empty_comment_keeps_composer_open() {
    let mut app = comment_app();
    assert!(app.begin_comment_at_rendered_line(0));
    app.push_comment_char(' ');

    assert!(!app.save_comment());

    assert!(app.is_comment_composer_open());
    assert_eq!(app.comment_count(), 0);
}

#[test]
fn active_comment_can_be_edited_and_removed() {
    let mut app = comment_app();
    add_comment(&mut app, 0, "First draft");

    assert!(app.edit_active_comment());
    app.clear_comment_draft();
    for ch in "Updated".chars() {
        app.push_comment_char(ch);
    }
    assert!(app.save_comment());
    assert_eq!(app.comments()[0].body, "Updated");

    assert!(app.remove_active_comment());
    assert_eq!(app.comment_count(), 0);
    assert_eq!(app.active_comment_id(), None);
}

#[test]
fn comment_navigation_follows_source_order_and_scrolls_to_anchor() {
    let mut app = comment_app();
    let later_id = add_comment(&mut app, 3, "Later");
    let earlier_id = add_comment(&mut app, 0, "Earlier");
    assert_eq!(app.active_comment_id(), Some(earlier_id));

    assert!(app.activate_next_comment());
    assert_eq!(app.active_comment_id(), Some(later_id));
    assert_eq!(app.scroll(), 2);

    assert!(app.activate_previous_comment());
    assert_eq!(app.active_comment_id(), Some(earlier_id));
    assert_eq!(app.scroll(), 0);
}

#[test]
fn gutter_shows_one_anchor_marker_and_hover_add_affordance() {
    let mut app = comment_app();
    add_comment(&mut app, 1, "On wrapped source line");

    assert_eq!(app.comment_gutter_state(1), CommentGutterState::Active);
    assert_eq!(app.comment_gutter_state(2), CommentGutterState::Empty);

    app.set_hovered_content_line(Some(2));
    assert_eq!(app.comment_gutter_state(2), CommentGutterState::Add);
}

#[test]
fn keyboard_cursor_renders_focused_word_and_mode_hints() {
    let _guard = super::lock_theme_test_state();
    let mut app = comment_app();
    assert!(app.start_comment_cursor());
    assert!(app.move_comment_cursor_down());
    assert!(app.move_comment_cursor_down());
    assert!(app.move_comment_cursor_word_next());
    assert!(app.move_comment_cursor_word_next());

    let output = buffer_text(&draw(&mut app, 100, 24));

    assert!(output.contains("⌖ line 2 · wrapped"));
    assert!(output.contains("j/k line"));
    assert!(output.contains("h/l word"));
    assert!(output.contains("a comment"));
}

#[test]
fn comments_render_in_a_local_only_review_panel() {
    let _guard = super::lock_theme_test_state();
    let mut app = comment_app();
    assert!(app.start_comment_cursor());
    assert!(app.move_comment_cursor_down());
    assert!(app.move_comment_cursor_word_next());
    assert!(app.begin_comment_at_focus());
    for ch in "Clarify the expected behavior".chars() {
        app.push_comment_char(ch);
    }
    assert!(app.save_comment());

    let output = buffer_text(&draw(&mut app, 100, 24));

    assert!(output.contains("Comments 1 · local only"));
    assert!(output.contains("› two"));
    assert!(output.contains("Clarify the expected"));
    assert!(output.contains("◆"));
}

#[test]
fn composer_renders_target_and_ephemeral_scope() {
    let _guard = super::lock_theme_test_state();
    let mut app = comment_app();
    assert!(app.start_comment_cursor());
    assert!(app.move_comment_cursor_word_next());
    assert!(app.begin_comment_at_focus());
    for ch in "Looks good".chars() {
        app.push_comment_char(ch);
    }

    let output = buffer_text(&draw(&mut app, 100, 24));

    assert!(output.contains("Add comment · line 1"));
    assert!(output.contains("Comments live only for this open document"));
    assert!(output.contains("Target: “one”"));
    assert!(output.contains("Looks good"));
}

#[test]
fn connected_review_publishes_structured_metadata_and_marks_comments_submitted() {
    let root = unique_temp_dir("review-request");
    let document = root.join("review.md");
    let channel = root.join("channel");
    fs::create_dir_all(&root).unwrap();
    fs::write(&document, "one\ntwo\ntwo wrapped\nthree\n").unwrap();

    let mut app = comment_app_with_path(Some(document.canonicalize().unwrap()));
    app.connect_review_bridge(channel.clone()).unwrap();
    assert!(app.is_watch_enabled());
    app.toggle_watch();
    assert!(
        app.is_watch_enabled(),
        "Pi review must keep live reload enabled"
    );
    assert!(app.start_comment_cursor());
    assert!(app.move_comment_cursor_down());
    assert!(app.move_comment_cursor_word_next());
    assert!(app.begin_comment_at_focus());
    for ch in "Make this more concrete".chars() {
        app.push_comment_char(ch);
    }
    assert!(app.save_comment());

    assert!(app.submit_review());

    let request_path = only_json_file(&channel.join("requests"));
    let request: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(request_path).unwrap()).unwrap();
    assert_eq!(request["protocol_version"], 1);
    assert_eq!(request["type"], "review_request");
    assert_eq!(
        request["document"]["path"],
        document.canonicalize().unwrap().display().to_string()
    );
    assert_eq!(request["comments"][0]["id"], 1);
    assert_eq!(request["comments"][0]["body"], "Make this more concrete");
    assert_eq!(request["comments"][0]["target"]["source_line"], 2);
    assert_eq!(request["comments"][0]["target"]["source_line_text"], "two");
    assert_eq!(request["comments"][0]["target"]["rendered_line"], 2);
    assert_eq!(request["comments"][0]["target"]["selection"]["text"], "two");
    assert_eq!(
        request["comments"][0]["target"]["selection"]["rendered_start_column"],
        1
    );
    assert_eq!(
        request["comments"][0]["target"]["selection"]["rendered_end_column_exclusive"],
        4
    );
    assert!(request["document"]["revision"]
        .as_str()
        .is_some_and(|revision| !revision.is_empty()));
    assert_eq!(
        request["comments"][0]["context"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|line| line["is_target"] == true)
            .count(),
        1
    );
    assert!(matches!(
        app.comments()[0].status,
        ReviewCommentStatus::Submitted { .. }
    ));
    assert!(matches!(
        app.review_agent_state(),
        ReviewAgentState::Working {
            comment_count: 1,
            ..
        }
    ));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn completion_event_marks_only_confirmed_comments_addressed() {
    let _guard = super::lock_theme_test_state();
    let root = unique_temp_dir("review-complete");
    let document = root.join("review.md");
    let channel = root.join("channel");
    fs::create_dir_all(&root).unwrap();
    fs::write(&document, "one\ntwo\ntwo wrapped\nthree\n").unwrap();

    let mut app = comment_app_with_path(Some(document.clone()));
    app.connect_review_bridge(channel.clone()).unwrap();
    let first_id = add_comment(&mut app, 0, "First");
    let second_id = add_comment(&mut app, 3, "Second");
    assert!(app.submit_review());
    let request_path = only_json_file(&channel.join("requests"));
    let request: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(request_path).unwrap()).unwrap();
    let request_id = request["request_id"].as_str().unwrap();

    fs::write(&document, "# Updated by Pi\n\nNewer document text.\n").unwrap();
    let syntax_set = syntect::parsing::SyntaxSet::load_defaults_newlines();
    let theme_set = syntect::highlighting::ThemeSet::load_defaults();
    assert!(app.reload(&syntax_set, &theme_set));
    assert!(app.source.contains("Newer document text."));
    assert_eq!(app.comment_count(), 2);
    assert!(app
        .comments()
        .iter()
        .all(|comment| matches!(comment.status, ReviewCommentStatus::Submitted { .. })));

    fs::write(
        channel.join("events/001-complete.json"),
        serde_json::json!({
            "protocol_version": 1,
            "type": "review_completed",
            "request_id": request_id,
            "addressed_comment_ids": [first_id]
        })
        .to_string(),
    )
    .unwrap();

    assert!(app.poll_review_bridge());
    assert_eq!(app.addressed_comment_count(), 1);
    assert_eq!(app.draft_comment_count(), 1);
    assert!(app.comments().iter().any(|comment| {
        comment.id == first_id && comment.status == ReviewCommentStatus::Addressed
    }));
    assert!(app.comments().iter().any(|comment| {
        comment.id == second_id && comment.status == ReviewCommentStatus::Draft
    }));
    assert!(matches!(
        app.review_agent_state(),
        ReviewAgentState::Error(_)
    ));

    let output = buffer_text(&draw(&mut app, 110, 24));
    assert!(output.contains("addressed"));
    assert!(output.contains("needs attention"));
    assert!(output.contains("✓"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_review_returns_submitted_comments_to_draft() {
    let root = unique_temp_dir("review-failed");
    let document = root.join("review.md");
    let channel = root.join("channel");
    fs::create_dir_all(&root).unwrap();
    fs::write(&document, "one\ntwo\n").unwrap();

    let mut app = comment_app_with_path(Some(document));
    app.connect_review_bridge(channel.clone()).unwrap();
    add_comment(&mut app, 0, "Change this");
    assert!(app.submit_review());
    let request_path = only_json_file(&channel.join("requests"));
    let request: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(request_path).unwrap()).unwrap();
    let request_id = request["request_id"].as_str().unwrap();

    fs::write(
        channel.join("events/001-failed.json"),
        serde_json::json!({
            "protocol_version": 1,
            "type": "review_failed",
            "request_id": request_id,
            "message": "Agent stopped before confirming the edit"
        })
        .to_string(),
    )
    .unwrap();

    assert!(app.poll_review_bridge());
    assert_eq!(app.comments()[0].status, ReviewCommentStatus::Draft);
    assert!(matches!(
        app.review_agent_state(),
        ReviewAgentState::Error(_)
    ));
    assert!(app.edit_active_comment());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn review_resubmission_preserves_the_original_target_snapshot() {
    let root = unique_temp_dir("review-snapshot");
    let document = root.join("review.md");
    let channel = root.join("channel");
    fs::create_dir_all(&root).unwrap();
    fs::write(&document, "one\ntwo\nthree\n").unwrap();
    let mut app = comment_app_with_path(Some(document.clone()));
    app.connect_review_bridge(channel.clone()).unwrap();
    add_comment(&mut app, 1, "Clarify the second line");
    assert!(app.submit_review());
    let request_path = only_json_file(&channel.join("requests"));
    let first: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&request_path).unwrap()).unwrap();
    fs::remove_file(request_path).unwrap();
    fs::write(
        channel.join("events/001-partial.json"),
        serde_json::json!({
            "protocol_version": 1,
            "type": "review_completed",
            "request_id": first["request_id"],
            "addressed_comment_ids": []
        })
        .to_string(),
    )
    .unwrap();
    assert!(app.poll_review_bridge());

    fs::write(&document, "inserted\none\ntwo\nthree\n").unwrap();
    let ss = syntect::parsing::SyntaxSet::load_defaults_newlines();
    let themes = syntect::highlighting::ThemeSet::load_defaults();
    assert!(app.reload(&ss, &themes));
    assert!(app.edit_active_comment());
    app.push_comment_char('!');
    assert!(app.save_comment());
    assert!(app.submit_review());
    let second: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(only_json_file(&channel.join("requests"))).unwrap(),
    )
    .unwrap();
    assert_eq!(
        second["comments"][0]["target"],
        first["comments"][0]["target"]
    );
    assert_eq!(
        second["comments"][0]["context"],
        first["comments"][0]["context"]
    );
    assert_eq!(second["comments"][0]["target"]["source_line_text"], "two");
    assert_eq!(
        second["comments"][0]["target"]["source_revision"],
        first["document"]["revision"]
    );
    assert_ne!(
        second["document"]["revision"],
        first["document"]["revision"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn composer_keeps_its_target_when_the_document_reloads_before_save() {
    let root = unique_temp_dir("composer-snapshot");
    let document = root.join("review.md");
    let channel = root.join("channel");
    fs::create_dir_all(&root).unwrap();
    fs::write(&document, "one\ntwo\nthree\n").unwrap();
    let mut app = comment_app_with_path(Some(document.clone()));
    app.connect_review_bridge(channel.clone()).unwrap();
    assert!(app.begin_comment_at_rendered_line(1));
    app.push_comment_char('x');
    fs::write(&document, "inserted\none\ntwo\nthree\n").unwrap();
    let ss = syntect::parsing::SyntaxSet::load_defaults_newlines();
    let themes = syntect::highlighting::ThemeSet::load_defaults();
    assert!(app.reload(&ss, &themes));
    assert!(app.save_comment());
    assert!(app.submit_review());
    let request: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(only_json_file(&channel.join("requests"))).unwrap(),
    )
    .unwrap();
    assert_eq!(request["comments"][0]["target"]["source_line_text"], "two");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn oversized_review_stays_editable_and_is_not_published() {
    let root = unique_temp_dir("review-size");
    let document = root.join("review.md");
    let channel = root.join("channel");
    fs::create_dir_all(&root).unwrap();
    let source = "x".repeat(600 * 1024);
    fs::write(&document, &source).unwrap();
    let mut app = comment_app_with_path(Some(document));
    app.source = source;
    app.connect_review_bridge(channel.clone()).unwrap();
    add_comment(&mut app, 0, "Short comment on a large source line");
    assert!(!app.submit_review());
    assert!(
        matches!(app.review_agent_state(), ReviewAgentState::Error(message) if message.contains("1 MiB"))
    );
    assert_eq!(app.comments()[0].status, ReviewCommentStatus::Draft);
    assert!(app.edit_active_comment());
    assert_eq!(fs::read_dir(channel.join("requests")).unwrap().count(), 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn loading_the_current_document_preserves_an_in_flight_review() {
    let root = unique_temp_dir("review-same-file");
    let document = root.join("review.md");
    let channel = root.join("channel");
    fs::create_dir_all(&root).unwrap();
    fs::write(&document, "one\ntwo\nthree\n").unwrap();
    let mut app = comment_app_with_path(Some(document.clone()));
    app.connect_review_bridge(channel.clone()).unwrap();
    let id = add_comment(&mut app, 1, "Keep this review");
    assert!(app.submit_review());
    let before = app.comments().to_vec();
    let state = app.review_agent_state().clone();
    let request: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(only_json_file(&channel.join("requests"))).unwrap(),
    )
    .unwrap();
    let ss = syntect::parsing::SyntaxSet::load_defaults_newlines();
    let themes = syntect::highlighting::ThemeSet::load_defaults();
    assert!(app.load_path(document.clone(), &ss, &themes));
    assert_eq!(app.comments(), before);
    assert_eq!(app.review_agent_state(), &state);
    assert!(app.is_review_bridge_connected());
    fs::write(
        channel.join("events/001-complete.json"),
        serde_json::json!({
            "protocol_version": 1,
            "type": "review_completed",
            "request_id": request["request_id"],
            "addressed_comment_ids": [id]
        })
        .to_string(),
    )
    .unwrap();
    assert!(app.poll_review_bridge());
    assert_eq!(app.addressed_comment_count(), 1);
    add_comment(&mut app, 0, "Unsent draft");
    assert!(app.load_path(document.clone(), &ss, &themes));
    assert_eq!(app.addressed_comment_count(), 1);
    assert_eq!(app.draft_comment_count(), 1);
    #[cfg(unix)]
    {
        let alias = root.join("alias.md");
        std::os::unix::fs::symlink(&document, &alias).unwrap();
        assert!(app.load_path(alias, &ss, &themes));
        assert_eq!(app.addressed_comment_count(), 1);
        assert_eq!(app.draft_comment_count(), 1);
        assert!(app.is_review_bridge_connected());
    }
    let other = root.join("other.md");
    fs::write(&other, "another document\n").unwrap();
    assert!(app.load_path(other, &ss, &themes));
    assert_eq!(app.comment_count(), 0);
    assert!(!app.is_review_bridge_connected());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn closing_the_pi_bridge_leaves_the_document_open() {
    let root = unique_temp_dir("review-disconnected");
    let document = root.join("review.md");
    let channel = root.join("channel");
    fs::create_dir_all(&root).unwrap();
    fs::write(&document, "one\ntwo\n").unwrap();

    let mut app = comment_app_with_path(Some(document.clone()));
    app.connect_review_bridge(channel.clone()).unwrap();
    add_comment(&mut app, 0, "Change this");
    assert!(app.submit_review());
    fs::write(
        channel.join("events/001-closed.json"),
        serde_json::json!({
            "protocol_version": 1,
            "type": "bridge_closed",
            "message": "The Pi session ended; Leaf was left open"
        })
        .to_string(),
    )
    .unwrap();

    assert!(app.poll_review_bridge());
    assert!(app.has_content());
    assert_eq!(app.filepath(), Some(document.as_path()));
    assert_eq!(app.comments()[0].status, ReviewCommentStatus::Draft);
    assert!(matches!(
        app.review_agent_state(),
        ReviewAgentState::Disconnected(message) if message.contains("left open")
    ));

    fs::remove_dir_all(root).unwrap();
}
