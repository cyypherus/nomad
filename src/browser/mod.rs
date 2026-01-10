mod cache;
mod history;

use cache::PageCache;
use history::History;
use micron::Document;
use std::fmt;
use std::path::Path;
use thiserror::Error;

pub use cache::CachedPage;

#[derive(Error, Debug)]
pub enum BrowserError {
    #[error("invalid url: {0}")]
    InvalidUrl(String),
    #[error("cache error: {0}")]
    Cache(#[from] cache::CacheError),
    #[error("page not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageUrl {
    pub dest_hash: [u8; 16],
    pub path: String,
}

impl PageUrl {
    pub fn parse(url: &str) -> Result<Self, BrowserError> {
        let Some((hash_part, path)) = url.split_once(':') else {
            return Err(BrowserError::InvalidUrl(
                "missing ':' separator".to_string(),
            ));
        };

        if hash_part.len() != 32 {
            return Err(BrowserError::InvalidUrl(format!(
                "destination hash must be 32 hex chars, got {}",
                hash_part.len()
            )));
        }

        let hash_bytes =
            hex::decode(hash_part).map_err(|e| BrowserError::InvalidUrl(e.to_string()))?;

        let mut dest_hash = [0u8; 16];
        dest_hash.copy_from_slice(&hash_bytes);

        let path = if path.is_empty() {
            "/index.mu".to_string()
        } else if !path.starts_with('/') {
            format!("/{path}")
        } else {
            path.to_string()
        };

        Ok(Self { dest_hash, path })
    }
}

impl fmt::Display for PageUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", hex::encode(self.dest_hash), self.path)
    }
}

pub struct Browser {
    cache: PageCache,
    history: History,
    current: Option<PageUrl>,
}

impl Browser {
    pub fn new(data_dir: &Path) -> Result<Self, BrowserError> {
        let cache_dir = data_dir.join("cache");
        Ok(Self {
            cache: PageCache::new(&cache_dir)?,
            history: History::new(),
            current: None,
        })
    }

    pub fn navigate(&mut self, url: &str) -> Result<NavigateResult, BrowserError> {
        let parsed = PageUrl::parse(url)?;

        if let Some(ref current) = self.current {
            self.history.push(current.clone());
        }

        self.current = Some(parsed.clone());

        if let Some(cached) = self.cache.get(&parsed)? {
            return Ok(NavigateResult::Cached(cached));
        }

        Ok(NavigateResult::NeedsFetch(parsed))
    }

    pub fn receive_page(&mut self, url: &PageUrl, content: &str) -> Result<Document, BrowserError> {
        let doc = micron::parse(content);
        self.cache.put(url, content)?;
        Ok(doc)
    }

    pub fn back(&mut self) -> Option<PageUrl> {
        if let Some(current) = self.current.take() {
            self.history.push_forward(current);
        }
        let prev = self.history.pop_back()?;
        self.current = Some(prev.clone());
        Some(prev)
    }

    pub fn forward(&mut self) -> Option<PageUrl> {
        if let Some(current) = self.current.take() {
            self.history.push_back(current);
        }
        let next = self.history.pop_forward()?;
        self.current = Some(next.clone());
        Some(next)
    }

    pub fn current(&self) -> Option<&PageUrl> {
        self.current.as_ref()
    }

    pub fn can_go_back(&self) -> bool {
        self.history.can_go_back()
    }

    pub fn can_go_forward(&self) -> bool {
        self.history.can_go_forward()
    }

    pub fn clear_cache(&mut self) -> Result<(), BrowserError> {
        self.cache.clear()?;
        Ok(())
    }

    pub fn cache_stats(&self) -> (usize, u64) {
        self.cache.stats()
    }
}

pub enum NavigateResult {
    Cached(CachedPage),
    NeedsFetch(PageUrl),
}

impl NavigateResult {
    pub fn document(&self) -> Option<Document> {
        match self {
            NavigateResult::Cached(page) => Some(page.document()),
            NavigateResult::NeedsFetch(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_url() {
        let url = "0123456789abcdef0123456789abcdef:/page.mu";
        let parsed = PageUrl::parse(url).unwrap();
        assert_eq!(parsed.path, "/page.mu");
    }

    #[test]
    fn parse_url_no_path() {
        let url = "0123456789abcdef0123456789abcdef:";
        let parsed = PageUrl::parse(url).unwrap();
        assert_eq!(parsed.path, "/index.mu");
    }

    #[test]
    fn parse_url_path_no_slash() {
        let url = "0123456789abcdef0123456789abcdef:page.mu";
        let parsed = PageUrl::parse(url).unwrap();
        assert_eq!(parsed.path, "/page.mu");
    }

    #[test]
    fn parse_url_invalid_hash() {
        let url = "short:/page";
        assert!(PageUrl::parse(url).is_err());
    }

    #[test]
    fn parse_url_no_separator() {
        let url = "0123456789abcdef0123456789abcdef/page";
        assert!(PageUrl::parse(url).is_err());
    }
}
