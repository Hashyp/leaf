use super::App;
use crate::markdown::display_width;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReviewComment {
    pub(crate) id: u64,
    pub(crate) source_line: usize,
    pub(crate) selected_text: Option<String>,
    pub(crate) body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommentComposer {
    pub(crate) source_line: usize,
    pub(crate) selected_text: Option<String>,
    pub(crate) draft: String,
    pub(crate) editing_id: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommentCursor {
    pub(crate) rendered_line: usize,
    pub(crate) word_index: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommentWordFocus {
    pub(crate) start_col: usize,
    pub(crate) end_col: usize,
    pub(crate) text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommentGutterState {
    Empty,
    Add,
    Comment,
    Active,
}

impl App {
    pub(crate) fn comments(&self) -> &[ReviewComment] {
        &self.comments
    }

    pub(crate) fn comment_count(&self) -> usize {
        self.comments.len()
    }

    pub(crate) fn has_comments(&self) -> bool {
        !self.comments.is_empty()
    }

    pub(crate) fn active_comment(&self) -> Option<&ReviewComment> {
        let active_id = self.active_comment_id?;
        self.comments.iter().find(|comment| comment.id == active_id)
    }

    pub(crate) fn active_comment_id(&self) -> Option<u64> {
        self.active_comment_id
    }

    pub(crate) fn is_comment_composer_open(&self) -> bool {
        self.comment_composer.is_some()
    }

    pub(crate) fn comment_composer(&self) -> Option<&CommentComposer> {
        self.comment_composer.as_ref()
    }

    pub(crate) fn begin_comment_at_focus(&mut self) -> bool {
        let (rendered_line, selected_text) = if let Some(cursor) = self.comment_cursor {
            (
                cursor.rendered_line,
                self.comment_cursor_word_focus().map(|word| word.text),
            )
        } else {
            (
                self.hovered_content_line
                    .filter(|line| (self.scroll..self.visible_end()).contains(line))
                    .unwrap_or(self.scroll),
                None,
            )
        };
        let opened = self.begin_comment_with_selection(rendered_line, selected_text);
        if opened {
            self.comment_cursor = None;
        }
        opened
    }

    pub(crate) fn begin_comment_at_rendered_line(&mut self, rendered_line: usize) -> bool {
        let opened = self.begin_comment_with_selection(rendered_line, None);
        if opened {
            self.comment_cursor = None;
        }
        opened
    }

    fn begin_comment_with_selection(
        &mut self,
        rendered_line: usize,
        selected_text: Option<String>,
    ) -> bool {
        if !self.has_content() || rendered_line >= self.total() {
            return false;
        }
        let source_line = self.source_line_at(rendered_line).max(1);
        self.comment_composer = Some(CommentComposer {
            source_line,
            selected_text,
            draft: String::new(),
            editing_id: None,
        });
        true
    }

    pub(crate) fn push_comment_char(&mut self, ch: char) {
        if let Some(composer) = &mut self.comment_composer {
            composer.draft.push(ch);
        }
    }

    pub(crate) fn pop_comment_char(&mut self) {
        if let Some(composer) = &mut self.comment_composer {
            composer.draft.pop();
        }
    }

    pub(crate) fn clear_comment_draft(&mut self) {
        if let Some(composer) = &mut self.comment_composer {
            composer.draft.clear();
        }
    }

    pub(crate) fn cancel_comment(&mut self) {
        self.comment_composer = None;
    }

    pub(crate) fn save_comment(&mut self) -> bool {
        let Some(composer) = self.comment_composer.as_ref() else {
            return false;
        };
        let body = composer.draft.trim().to_string();
        if body.is_empty() {
            return false;
        }

        let source_line = composer.source_line;
        let selected_text = composer.selected_text.clone();
        let editing_id = composer.editing_id;
        let active_id = if let Some(id) = editing_id {
            let Some(comment) = self.comments.iter_mut().find(|comment| comment.id == id) else {
                return false;
            };
            comment.body = body;
            id
        } else {
            let id = self.next_comment_id;
            self.next_comment_id = self.next_comment_id.saturating_add(1);
            self.comments.push(ReviewComment {
                id,
                source_line,
                selected_text,
                body,
            });
            id
        };

        self.comments
            .sort_by_key(|comment| (comment.source_line, comment.id));
        self.active_comment_id = Some(active_id);
        self.comment_composer = None;
        true
    }

    pub(crate) fn edit_active_comment(&mut self) -> bool {
        let Some(comment) = self.active_comment().cloned() else {
            return false;
        };
        self.comment_composer = Some(CommentComposer {
            source_line: comment.source_line,
            selected_text: comment.selected_text,
            draft: comment.body,
            editing_id: Some(comment.id),
        });
        true
    }

    pub(crate) fn remove_active_comment(&mut self) -> bool {
        let Some(active_id) = self.active_comment_id else {
            return false;
        };
        let Some(position) = self
            .comments
            .iter()
            .position(|comment| comment.id == active_id)
        else {
            self.active_comment_id = None;
            return false;
        };

        self.comments.remove(position);
        self.active_comment_id = if self.comments.is_empty() {
            None
        } else {
            Some(self.comments[position.min(self.comments.len() - 1)].id)
        };
        true
    }

    pub(crate) fn activate_next_comment(&mut self) -> bool {
        self.activate_comment(true)
    }

    pub(crate) fn activate_previous_comment(&mut self) -> bool {
        self.activate_comment(false)
    }

    fn activate_comment(&mut self, forward: bool) -> bool {
        if self.comments.is_empty() {
            return false;
        }
        let len = self.comments.len();
        let current = self
            .active_comment_id
            .and_then(|id| self.comments.iter().position(|comment| comment.id == id));
        let position = match (current, forward) {
            (Some(position), true) => (position + 1) % len,
            (Some(position), false) => (position + len - 1) % len,
            (None, true) => 0,
            (None, false) => len - 1,
        };
        let comment = &self.comments[position];
        self.active_comment_id = Some(comment.id);
        let source_line = comment.source_line;
        if let Some(rendered_line) = self.first_rendered_line_for_source(source_line) {
            self.scroll_to(rendered_line);
        }
        true
    }

    pub(crate) fn clear_comments_for_document(&mut self) {
        self.comments.clear();
        self.next_comment_id = 1;
        self.active_comment_id = None;
        self.comment_composer = None;
        self.comment_cursor = None;
        self.hovered_content_line = None;
    }

    pub(crate) fn start_comment_cursor(&mut self) -> bool {
        if !self.has_content() || self.total() == 0 {
            return false;
        }
        let initial = self
            .hovered_content_line
            .filter(|line| (self.scroll..self.visible_end()).contains(line))
            .or_else(|| {
                (self.scroll..self.visible_end()).find(|line| self.is_commentable_line(*line))
            })
            .unwrap_or(self.scroll.min(self.total() - 1));
        self.hovered_content_line = None;
        self.comment_cursor = Some(CommentCursor {
            rendered_line: initial,
            word_index: None,
        });
        true
    }

    pub(crate) fn clear_comment_cursor(&mut self) -> bool {
        self.comment_cursor.take().is_some()
    }

    pub(crate) fn is_comment_cursor_active(&self) -> bool {
        self.comment_cursor.is_some()
    }

    pub(crate) fn comment_cursor(&self) -> Option<CommentCursor> {
        self.comment_cursor
    }

    pub(crate) fn comment_cursor_word_focus(&self) -> Option<CommentWordFocus> {
        let cursor = self.comment_cursor?;
        let line = self.lines.get(cursor.rendered_line)?;
        cursor
            .word_index
            .and_then(|index| comment_words(line).get(index).cloned())
    }

    pub(crate) fn move_comment_cursor_down(&mut self) -> bool {
        self.move_comment_cursor_line(true)
    }

    pub(crate) fn move_comment_cursor_up(&mut self) -> bool {
        self.move_comment_cursor_line(false)
    }

    fn move_comment_cursor_line(&mut self, forward: bool) -> bool {
        let Some(cursor) = self.comment_cursor else {
            return false;
        };
        let next = if forward {
            ((cursor.rendered_line + 1)..self.total()).find(|line| self.is_commentable_line(*line))
        } else {
            (0..cursor.rendered_line)
                .rev()
                .find(|line| self.is_commentable_line(*line))
        };
        let Some(rendered_line) = next else {
            return false;
        };
        self.comment_cursor = Some(CommentCursor {
            rendered_line,
            word_index: None,
        });
        self.keep_comment_cursor_visible(rendered_line);
        true
    }

    pub(crate) fn move_comment_cursor_word_next(&mut self) -> bool {
        self.move_comment_cursor_word(true)
    }

    pub(crate) fn move_comment_cursor_word_previous(&mut self) -> bool {
        self.move_comment_cursor_word(false)
    }

    fn move_comment_cursor_word(&mut self, forward: bool) -> bool {
        let Some(mut cursor) = self.comment_cursor else {
            return false;
        };
        let word_count = self
            .lines
            .get(cursor.rendered_line)
            .map(comment_words)
            .map(|words| words.len())
            .unwrap_or(0);
        if word_count == 0 {
            return false;
        }

        let next = if forward {
            match cursor.word_index {
                None => Some(0),
                Some(index) if index + 1 < word_count => Some(index + 1),
                Some(_) => return false,
            }
        } else {
            match cursor.word_index {
                None => Some(word_count - 1),
                Some(0) => None,
                Some(index) => Some(index - 1),
            }
        };
        if cursor.word_index == next {
            return false;
        }
        cursor.word_index = next;
        self.comment_cursor = Some(cursor);
        true
    }

    fn keep_comment_cursor_visible(&mut self, rendered_line: usize) {
        let viewport_height = (self.content_area.height as usize).max(1);
        if rendered_line < self.scroll {
            self.scroll_to(rendered_line);
        } else if rendered_line >= self.scroll.saturating_add(viewport_height) {
            self.scroll_to(rendered_line + 1 - viewport_height);
        }
    }

    fn is_commentable_line(&self, rendered_line: usize) -> bool {
        self.lines.get(rendered_line).is_some_and(|line| {
            line.spans
                .iter()
                .any(|span| !span.content.trim().is_empty())
        })
    }

    pub(crate) fn set_hovered_content_line(&mut self, line: Option<usize>) -> bool {
        if self.hovered_content_line == line {
            return false;
        }
        self.hovered_content_line = line;
        true
    }

    pub(crate) fn clear_hovered_content_line(&mut self) {
        self.hovered_content_line = None;
    }

    pub(crate) fn comment_gutter_state(&self, rendered_line: usize) -> CommentGutterState {
        if self.hovered_content_line == Some(rendered_line)
            || self
                .comment_cursor
                .is_some_and(|cursor| cursor.rendered_line == rendered_line)
        {
            return CommentGutterState::Add;
        }

        let source_line = self.source_line_at(rendered_line);
        let is_anchor = rendered_line == 0
            || self.source_line_at(rendered_line.saturating_sub(1)) != source_line;
        if !is_anchor {
            return CommentGutterState::Empty;
        }

        let has_comment = self
            .comments
            .iter()
            .any(|comment| comment.source_line == source_line);
        if !has_comment {
            return CommentGutterState::Empty;
        }
        if self
            .active_comment()
            .is_some_and(|comment| comment.source_line == source_line)
        {
            CommentGutterState::Active
        } else {
            CommentGutterState::Comment
        }
    }

    fn first_rendered_line_for_source(&self, source_line: usize) -> Option<usize> {
        self.source_line_map
            .iter()
            .position(|mapped| *mapped == source_line)
    }
}

fn comment_words(line: &ratatui::text::Line<'_>) -> Vec<CommentWordFocus> {
    let mut words = Vec::new();
    let mut start_col = None;
    let mut end_col = 0usize;
    let mut text = String::new();
    let mut col = 0usize;

    for ch in line.spans.iter().flat_map(|span| span.content.chars()) {
        let width = display_width(&ch.to_string());
        if is_comment_word_char(ch) {
            start_col.get_or_insert(col);
            end_col = col + width;
            text.push(ch);
        } else if let Some(start) = start_col.take() {
            words.push(CommentWordFocus {
                start_col: start,
                end_col,
                text: std::mem::take(&mut text),
            });
        }
        col += width;
    }
    if let Some(start) = start_col {
        words.push(CommentWordFocus {
            start_col: start,
            end_col,
            text,
        });
    }
    words
}

fn is_comment_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '-' | '\'' | '/' | '.' | ':' | '@' | '#')
}
