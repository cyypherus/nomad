use super::PageUrl;

const MAX_HISTORY: usize = 100;

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub url: PageUrl,
}

pub struct History {
    back_stack: Vec<PageUrl>,
    forward_stack: Vec<PageUrl>,
}

impl History {
    pub fn new() -> Self {
        Self {
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
        }
    }

    pub fn push(&mut self, url: PageUrl) {
        self.forward_stack.clear();
        self.back_stack.push(url);
        if self.back_stack.len() > MAX_HISTORY {
            self.back_stack.remove(0);
        }
    }

    pub fn push_back(&mut self, url: PageUrl) {
        self.back_stack.push(url);
        if self.back_stack.len() > MAX_HISTORY {
            self.back_stack.remove(0);
        }
    }

    pub fn push_forward(&mut self, url: PageUrl) {
        self.forward_stack.push(url);
        if self.forward_stack.len() > MAX_HISTORY {
            self.forward_stack.remove(0);
        }
    }

    pub fn pop_back(&mut self) -> Option<PageUrl> {
        self.back_stack.pop()
    }

    pub fn pop_forward(&mut self) -> Option<PageUrl> {
        self.forward_stack.pop()
    }

    pub fn can_go_back(&self) -> bool {
        !self.back_stack.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward_stack.is_empty()
    }

    pub fn back_count(&self) -> usize {
        self.back_stack.len()
    }

    pub fn forward_count(&self) -> usize {
        self.forward_stack.len()
    }

    pub fn clear(&mut self) {
        self.back_stack.clear();
        self.forward_stack.clear();
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(path: &str) -> PageUrl {
        PageUrl {
            dest_hash: [0u8; 16],
            path: path.to_string(),
        }
    }

    #[test]
    fn push_and_back() {
        let mut history = History::new();

        history.push(url("/page1"));
        history.push(url("/page2"));

        assert!(history.can_go_back());
        assert!(!history.can_go_forward());

        let back = history.pop_back().unwrap();
        assert_eq!(back.path, "/page2");
    }

    #[test]
    fn forward_after_back() {
        let mut history = History::new();

        history.push(url("/page1"));
        history.push(url("/page2"));

        let back = history.pop_back().unwrap();
        history.push_forward(back);

        assert!(history.can_go_forward());
        let fwd = history.pop_forward().unwrap();
        assert_eq!(fwd.path, "/page2");
    }

    #[test]
    fn push_clears_forward() {
        let mut history = History::new();

        history.push(url("/page1"));
        history.push(url("/page2"));
        history.pop_back();
        history.push_forward(url("/page2"));

        history.push(url("/page3"));

        assert!(!history.can_go_forward());
    }

    #[test]
    fn empty_history() {
        let history = History::new();
        assert!(!history.can_go_back());
        assert!(!history.can_go_forward());
    }

    #[test]
    fn max_history_limit() {
        let mut history = History::new();

        for i in 0..150 {
            history.push(url(&format!("/page{i}")));
        }

        assert_eq!(history.back_count(), MAX_HISTORY);
    }
}
