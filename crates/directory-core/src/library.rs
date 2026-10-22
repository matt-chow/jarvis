//! Document library: the owner's PDF and ePub files with reading progress.
//! Documents are ordinary files (see `files`); this module only adds progress.

use crate::entropy::is_id;
use crate::files::FileEntry;
use crate::identity::Principal;
use serde::{Deserialize, Serialize};

pub const DOCUMENT_TYPES: [&str; 2] = ["application/pdf", "application/epub+zip"];
pub const MAX_PAGES: u32 = 100_000;
pub const MAX_POSITION: usize = 200;

/// Whether a file belongs in the library (by content type, then extension).
pub fn is_document(f: &FileEntry) -> bool {
    let name = f.name.to_lowercase();
    DOCUMENT_TYPES.contains(&f.content_type.as_str())
        || name.ends_with(".pdf")
        || name.ends_with(".epub")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingStatus {
    Want,
    Reading,
    Finished,
}

impl ReadingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Want => "want",
            Self::Reading => "reading",
            Self::Finished => "finished",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "want" => Self::Want,
            "reading" => Self::Reading,
            "finished" => Self::Finished,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub file_id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub status: ReadingStatus,
    pub current_page: Option<u32>,
    pub total_pages: Option<u32>,
    /// Free-form location (e.g. a chapter or an ePub CFI from a future reader).
    pub position: Option<String>,
    /// 0–100, derived from pages when both are known.
    pub percent: u8,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryItem {
    pub file: FileEntry,
    pub progress: Option<Progress>,
}

/// Partial update; omitted fields keep their value.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProgressPatch {
    #[serde(default)]
    pub status: Option<ReadingStatus>,
    #[serde(default)]
    pub current_page: Option<u32>,
    #[serde(default)]
    pub total_pages: Option<u32>,
    #[serde(default)]
    pub position: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryError {
    Invalid(&'static str, &'static str),
    NotFound,
    Backend(String),
}

pub trait LibraryRepository {
    /// All of the owner's files (the service filters to documents).
    async fn files(&self, owner_id: &str) -> Result<Vec<FileEntry>, String>;
    async fn file(&self, owner_id: &str, file_id: &str) -> Result<Option<FileEntry>, String>;
    async fn progress(&self, owner_id: &str) -> Result<Vec<Progress>, String>;
    async fn progress_for(&self, owner_id: &str, file_id: &str)
        -> Result<Option<Progress>, String>;
    async fn upsert(&self, progress: &Progress) -> Result<(), String>;
    async fn delete(&self, owner_id: &str, file_id: &str) -> Result<bool, String>;
}

fn backend(e: String) -> LibraryError {
    LibraryError::Backend(e)
}

fn sort_rank(item: &LibraryItem) -> u8 {
    match item.progress.as_ref().map(|p| p.status) {
        Some(ReadingStatus::Reading) => 0,
        Some(ReadingStatus::Want) => 1,
        None => 2,
        Some(ReadingStatus::Finished) => 3,
    }
}

pub struct LibraryService<'a, R> {
    repo: &'a R,
    now: i64,
}

impl<'a, R: LibraryRepository> LibraryService<'a, R> {
    pub fn new(repo: &'a R, now: i64) -> Self {
        Self { repo, now }
    }

    /// Reading first (most recent), then want-to-read, unread, finished.
    pub async fn list(&self, p: &Principal) -> Result<Vec<LibraryItem>, LibraryError> {
        let files = self.repo.files(&p.user_id).await.map_err(backend)?;
        let mut progress = self.repo.progress(&p.user_id).await.map_err(backend)?;
        let mut items: Vec<LibraryItem> = files
            .into_iter()
            .filter(is_document)
            .map(|file| {
                let pos = progress.iter().position(|pr| pr.file_id == file.id);
                LibraryItem {
                    progress: pos.map(|i| progress.swap_remove(i)),
                    file,
                }
            })
            .collect();
        items.sort_by(|a, b| {
            sort_rank(a).cmp(&sort_rank(b)).then_with(|| {
                let at = |i: &LibraryItem| {
                    i.progress
                        .as_ref()
                        .map_or(i.file.updated_at, |p| p.updated_at)
                };
                at(b).cmp(&at(a))
            })
        });
        Ok(items)
    }

    pub async fn update(
        &self,
        p: &Principal,
        file_id: &str,
        patch: ProgressPatch,
    ) -> Result<Progress, LibraryError> {
        if !is_id(file_id) {
            return Err(LibraryError::NotFound);
        }
        let file = self
            .repo
            .file(&p.user_id, file_id)
            .await
            .map_err(backend)?
            .ok_or(LibraryError::NotFound)?;
        if !is_document(&file) {
            return Err(LibraryError::Invalid("file_id", "not a PDF or ePub"));
        }
        let mut pr = match self
            .repo
            .progress_for(&p.user_id, file_id)
            .await
            .map_err(backend)?
        {
            Some(pr) => pr,
            None => Progress {
                file_id: file_id.to_owned(),
                owner_id: p.user_id.clone(),
                status: ReadingStatus::Want,
                current_page: None,
                total_pages: None,
                position: None,
                percent: 0,
                started_at: None,
                finished_at: None,
                updated_at: self.now,
            },
        };
        if let Some(total) = patch.total_pages {
            if total == 0 || total > MAX_PAGES {
                return Err(LibraryError::Invalid("total_pages", "out of range"));
            }
            pr.total_pages = Some(total);
        }
        if let Some(page) = patch.current_page {
            if page > MAX_PAGES {
                return Err(LibraryError::Invalid("current_page", "out of range"));
            }
            pr.current_page = Some(page);
        }
        if let (Some(page), Some(total)) = (pr.current_page, pr.total_pages) {
            if page > total {
                return Err(LibraryError::Invalid(
                    "current_page",
                    "beyond the last page",
                ));
            }
        }
        if let Some(pos) = patch.position {
            let pos = pos.trim();
            if pos.chars().count() > MAX_POSITION || pos.chars().any(char::is_control) {
                return Err(LibraryError::Invalid("position", "too long or invalid"));
            }
            pr.position = (!pos.is_empty()).then(|| pos.to_owned());
        }
        pr.percent = match (pr.current_page, pr.total_pages) {
            (Some(page), Some(total)) => ((u64::from(page) * 100) / u64::from(total)) as u8,
            _ => pr.percent,
        };
        // Status: explicit wins; otherwise reading moves it along.
        pr.status = match patch.status {
            Some(s) => s,
            None if pr.percent >= 100 => ReadingStatus::Finished,
            None if pr.current_page.unwrap_or(0) > 0 && pr.status == ReadingStatus::Want => {
                ReadingStatus::Reading
            }
            None => pr.status,
        };
        if pr.status == ReadingStatus::Finished {
            pr.percent = 100;
            if let Some(total) = pr.total_pages {
                pr.current_page = Some(total);
            }
            pr.finished_at.get_or_insert(self.now);
        } else {
            pr.finished_at = None;
        }
        if pr.status != ReadingStatus::Want {
            pr.started_at.get_or_insert(self.now);
        }
        pr.updated_at = self.now;
        self.repo.upsert(&pr).await.map_err(backend)?;
        Ok(pr)
    }

    /// Forget progress (back to unread).
    pub async fn clear(&self, p: &Principal, file_id: &str) -> Result<(), LibraryError> {
        if is_id(file_id)
            && self
                .repo
                .delete(&p.user_id, file_id)
                .await
                .map_err(backend)?
        {
            Ok(())
        } else {
            Err(LibraryError::NotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::AuthMethod;
    use crate::test_util::block_on;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Memory {
        files: Vec<FileEntry>,
        progress: RefCell<Vec<Progress>>,
    }

    impl LibraryRepository for Memory {
        async fn files(&self, owner_id: &str) -> Result<Vec<FileEntry>, String> {
            Ok(self
                .files
                .iter()
                .filter(|f| f.owner_id == owner_id)
                .cloned()
                .collect())
        }
        async fn file(&self, owner_id: &str, id: &str) -> Result<Option<FileEntry>, String> {
            Ok(self
                .files
                .iter()
                .find(|f| f.owner_id == owner_id && f.id == id)
                .cloned())
        }
        async fn progress(&self, owner_id: &str) -> Result<Vec<Progress>, String> {
            Ok(self
                .progress
                .borrow()
                .iter()
                .filter(|p| p.owner_id == owner_id)
                .cloned()
                .collect())
        }
        async fn progress_for(&self, owner_id: &str, id: &str) -> Result<Option<Progress>, String> {
            Ok(self
                .progress
                .borrow()
                .iter()
                .find(|p| p.owner_id == owner_id && p.file_id == id)
                .cloned())
        }
        async fn upsert(&self, pr: &Progress) -> Result<(), String> {
            let mut all = self.progress.borrow_mut();
            all.retain(|p| !(p.owner_id == pr.owner_id && p.file_id == pr.file_id));
            all.push(pr.clone());
            Ok(())
        }
        async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
            let mut all = self.progress.borrow_mut();
            let before = all.len();
            all.retain(|p| !(p.owner_id == owner_id && p.file_id == id));
            Ok(all.len() < before)
        }
    }

    fn file(id: char, name: &str, ct: &str, at: i64) -> FileEntry {
        FileEntry {
            id: id.to_string().repeat(32),
            owner_id: "u1".into(),
            folder: String::new(),
            name: name.into(),
            size: 1,
            content_type: ct.into(),
            created_at: at,
            updated_at: at,
        }
    }

    fn me() -> Principal {
        Principal {
            user_id: "u1".into(),
            method: AuthMethod::Session,
        }
    }

    fn repo() -> Memory {
        Memory {
            files: vec![
                file('a', "Dune.pdf", "application/pdf", 1),
                file('b', "Novel.epub", "application/octet-stream", 2),
                file('c', "photo.jpg", "image/jpeg", 3),
                file('d', "Manual.PDF", "application/octet-stream", 4),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn library_lists_documents_ordered_by_reading_state() {
        let r = repo();
        let svc = LibraryService::new(&r, 10);
        let a = "a".repeat(32);
        let d = "d".repeat(32);
        block_on(svc.update(
            &me(),
            &a,
            ProgressPatch {
                status: Some(ReadingStatus::Finished),
                ..Default::default()
            },
        ))
        .unwrap();
        block_on(svc.update(
            &me(),
            &d,
            ProgressPatch {
                current_page: Some(3),
                total_pages: Some(10),
                ..Default::default()
            },
        ))
        .unwrap();
        let items = block_on(svc.list(&me())).unwrap();
        let names: Vec<_> = items.iter().map(|i| i.file.name.as_str()).collect();
        assert_eq!(
            names,
            ["Manual.PDF", "Novel.epub", "Dune.pdf"],
            "reading, unread, finished; no photos"
        );
    }

    #[test]
    fn progress_derives_percent_and_status() {
        let r = repo();
        let svc = LibraryService::new(&r, 10);
        let a = "a".repeat(32);
        let p = block_on(svc.update(
            &me(),
            &a,
            ProgressPatch {
                total_pages: Some(200),
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!(
            (p.status, p.percent, p.started_at),
            (ReadingStatus::Want, 0, None)
        );
        let p = block_on(svc.update(
            &me(),
            &a,
            ProgressPatch {
                current_page: Some(50),
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!(
            (p.status, p.percent, p.started_at),
            (ReadingStatus::Reading, 25, Some(10))
        );
        let p = block_on(svc.update(
            &me(),
            &a,
            ProgressPatch {
                current_page: Some(200),
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!(
            (p.status, p.percent, p.finished_at),
            (ReadingStatus::Finished, 100, Some(10))
        );
        let p = block_on(svc.update(
            &me(),
            &a,
            ProgressPatch {
                status: Some(ReadingStatus::Reading),
                current_page: Some(120),
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!(
            (p.status, p.percent, p.finished_at),
            (ReadingStatus::Reading, 60, None),
            "re-reading"
        );
    }

    #[test]
    fn validation_and_scope() {
        let r = repo();
        let svc = LibraryService::new(&r, 10);
        let a = "a".repeat(32);
        assert_eq!(
            block_on(svc.update(&me(), &"c".repeat(32), ProgressPatch::default())).unwrap_err(),
            LibraryError::Invalid("file_id", "not a PDF or ePub")
        );
        assert!(matches!(
            block_on(svc.update(
                &me(),
                &a,
                ProgressPatch {
                    current_page: Some(11),
                    total_pages: Some(10),
                    ..Default::default()
                }
            )),
            Err(LibraryError::Invalid("current_page", _))
        ));
        assert!(matches!(
            block_on(svc.update(
                &me(),
                &a,
                ProgressPatch {
                    total_pages: Some(0),
                    ..Default::default()
                }
            )),
            Err(LibraryError::Invalid(..))
        ));
        let other = Principal {
            user_id: "u2".into(),
            method: AuthMethod::Session,
        };
        assert_eq!(
            block_on(svc.update(&other, &a, ProgressPatch::default())).unwrap_err(),
            LibraryError::NotFound
        );
        assert_eq!(
            block_on(svc.clear(&me(), &a)).unwrap_err(),
            LibraryError::NotFound,
            "nothing to clear yet"
        );
    }
}
