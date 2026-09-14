use super::App;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReviewComment {
    pub(crate) id: u64,
    pub(crate) source_line: usize,
    pub(crate) body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommentComposer {
    pub(crate) source_line: usize,
    pub(crate) draft: String,
    pub(crate) editing_id: Option<u64>,
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
        let rendered_line = self
            .hovered_content_line
            .filter(|line| (self.scroll..self.visible_end()).contains(line))
            .unwrap_or(self.scroll);
        self.begin_comment_at_rendered_line(rendered_line)
    }

    pub(crate) fn begin_comment_at_rendered_line(&mut self, rendered_line: usize) -> bool {
        if !self.has_content() || rendered_line >= self.total() {
            return false;
        }
        let source_line = self.source_line_at(rendered_line).max(1);
        self.comment_composer = Some(CommentComposer {
            source_line,
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
        self.hovered_content_line = None;
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
        if self.hovered_content_line == Some(rendered_line) {
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
