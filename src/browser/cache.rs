use super::PageUrl;
use micron::Document;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use thiserror::Error;

const MAX_CACHE_SIZE: u64 = 10 * 1024 * 1024;
const MAX_CACHE_AGE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Error, Debug)]
pub enum CacheError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

pub struct CachedPage {
    pub content: String,
    pub fetched_at: SystemTime,
}

impl CachedPage {
    pub fn document(&self) -> Document {
        micron::parse(&self.content)
    }

    pub fn age(&self) -> Duration {
        self.fetched_at.elapsed().unwrap_or(Duration::ZERO)
    }

    pub fn is_stale(&self) -> bool {
        self.age() > MAX_CACHE_AGE
    }
}

struct MemoryEntry {
    content: String,
    fetched_at: SystemTime,
    size: usize,
}

pub struct PageCache {
    dir: PathBuf,
    memory: HashMap<String, MemoryEntry>,
    memory_size: u64,
    disk_size: u64,
}

impl PageCache {
    pub fn new(cache_dir: &Path) -> Result<Self, CacheError> {
        fs::create_dir_all(cache_dir)?;

        let disk_size = Self::calculate_disk_size(cache_dir)?;

        Ok(Self {
            dir: cache_dir.to_path_buf(),
            memory: HashMap::new(),
            memory_size: 0,
            disk_size,
        })
    }

    fn calculate_disk_size(dir: &Path) -> Result<u64, CacheError> {
        let mut size = 0u64;
        if dir.exists() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                if let Ok(metadata) = entry.metadata() {
                    size += metadata.len();
                }
            }
        }
        Ok(size)
    }

    pub fn get(&self, url: &PageUrl) -> Result<Option<CachedPage>, CacheError> {
        let key = url.to_string();

        if let Some(entry) = self.memory.get(&key) {
            return Ok(Some(CachedPage {
                content: entry.content.clone(),
                fetched_at: entry.fetched_at,
            }));
        }

        let file_path = self.url_to_path(url);
        if file_path.exists() {
            let content = fs::read_to_string(&file_path)?;
            let metadata = fs::metadata(&file_path)?;
            let fetched_at = metadata.modified().unwrap_or(SystemTime::now());
            return Ok(Some(CachedPage {
                content,
                fetched_at,
            }));
        }

        Ok(None)
    }

    pub fn put(&mut self, url: &PageUrl, content: &str) -> Result<(), CacheError> {
        let key = url.to_string();
        let size = content.len();

        self.memory.insert(
            key,
            MemoryEntry {
                content: content.to_string(),
                fetched_at: SystemTime::now(),
                size,
            },
        );
        self.memory_size += size as u64;

        let file_path = self.url_to_path(url);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let old_size = if file_path.exists() {
            fs::metadata(&file_path)?.len()
        } else {
            0
        };

        fs::write(&file_path, content)?;
        self.disk_size = self.disk_size - old_size + content.len() as u64;

        if self.disk_size > MAX_CACHE_SIZE {
            self.evict_oldest()?;
        }

        Ok(())
    }

    fn evict_oldest(&mut self) -> Result<(), CacheError> {
        let mut entries: Vec<_> = fs::read_dir(&self.dir)?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let metadata = e.metadata().ok()?;
                let modified = metadata.modified().ok()?;
                Some((e.path(), modified, metadata.len()))
            })
            .collect();

        entries.sort_by_key(|(_, time, _)| *time);

        for (path, _, size) in entries {
            if self.disk_size <= MAX_CACHE_SIZE / 2 {
                break;
            }
            fs::remove_file(&path)?;
            self.disk_size -= size;
        }

        Ok(())
    }

    fn url_to_path(&self, url: &PageUrl) -> PathBuf {
        let hash_hex = hex::encode(url.dest_hash);
        let safe_path = url.path.trim_start_matches('/').replace('/', "_");
        self.dir.join(format!("{hash_hex}_{safe_path}"))
    }

    pub fn clear(&mut self) -> Result<(), CacheError> {
        self.memory.clear();
        self.memory_size = 0;

        if self.dir.exists() {
            for entry in fs::read_dir(&self.dir)? {
                let entry = entry?;
                fs::remove_file(entry.path())?;
            }
        }
        self.disk_size = 0;

        Ok(())
    }

    pub fn stats(&self) -> (usize, u64) {
        (self.memory.len(), self.disk_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    fn test_url() -> PageUrl {
        PageUrl {
            dest_hash: [0u8; 16],
            path: "/test.mu".to_string(),
        }
    }

    #[test]
    fn cache_put_get() {
        let dir = temp_dir().join("nomad_cache_test_1");
        let _ = fs::remove_dir_all(&dir);

        let mut cache = PageCache::new(&dir).unwrap();
        let url = test_url();

        cache.put(&url, ">Test Page").unwrap();
        let cached = cache.get(&url).unwrap().unwrap();

        assert_eq!(cached.content, ">Test Page");
        assert!(!cached.is_stale());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_miss() {
        let dir = temp_dir().join("nomad_cache_test_2");
        let _ = fs::remove_dir_all(&dir);

        let cache = PageCache::new(&dir).unwrap();
        let url = test_url();

        let result = cache.get(&url).unwrap();
        assert!(result.is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_clear() {
        let dir = temp_dir().join("nomad_cache_test_3");
        let _ = fs::remove_dir_all(&dir);

        let mut cache = PageCache::new(&dir).unwrap();
        let url = test_url();

        cache.put(&url, ">Page").unwrap();
        cache.clear().unwrap();

        let (count, size) = cache.stats();
        assert_eq!(count, 0);
        assert_eq!(size, 0);

        let _ = fs::remove_dir_all(&dir);
    }
}
