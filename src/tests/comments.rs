use crate::app::{App, CommentGutterState};
use ratatui::{backend::TestBackend, buffer::Buffer, layout::Rect, text::Line, Terminal};

fn comment_app() -> App {
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
        None,
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
