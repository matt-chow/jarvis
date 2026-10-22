//! Private file management: metadata (folders, names, sizes) lives in the
//! database; content lives in object storage under a stable key derived from
//! the file id, so rename and move never touch the blob.
//!
//! Content is streamed by the runtime adapter. The service validates and
//! reserves (`prepare_upload`), the adapter streams the body to
//! `blob_key(...)`, then `commit_upload` records the metadata.

use crate::entropy::{is_id, new_id, Entropy};
use crate::identity::Principal;
use serde::{Deserialize, Serialize};

/// Default per-owner quota (free-tier friendly: R2 Free includes 10 GB).
pub const DEFAULT_QUOTA_BYTES: u64 = 1024 * 1024 * 1024;
/// Largest single file: below the Workers 100 MB request body limit.
pub const MAX_FILE_BYTES: u64 = 95 * 1024 * 1024;
pub const MAX_NAME: usize = 200;
pub const MAX_FOLDER: usize = 512;
pub const MAX_DEPTH: usize = 10;
pub const MAX_LIST: u32 = 1000;
const DEFAULT_CONTENT_TYPE: &str = "application/octet-stream";
const MAX_CONTENT_TYPE_LEN: usize = 127;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileEntry {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    /// Folder path without leading/trailing slash; "" is the root.
    pub folder: String,
    pub name: String,
    pub size: u64,
    pub content_type: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Accept only simple `type/subtype` media types; anything else becomes a
/// generic binary type so stored content can never smuggle header values.
pub fn normalize_content_type(raw: Option<&str>) -> String {
    let Some(raw) = raw else {
        return DEFAULT_CONTENT_TYPE.into();
    };
    let essence = raw
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let token = |p: &str| {
        !p.is_empty()
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$&-^_.+".contains(&b))
    };
    let valid = essence.len() <= MAX_CONTENT_TYPE_LEN
        && essence
            .split_once('/')
            .is_some_and(|(t, s)| token(t) && token(s));
    if valid {
        essence
    } else {
        DEFAULT_CONTENT_TYPE.into()
    }
}

/// What a `Range` request header asks for (RFC 9110; one range only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteRange {
    /// No usable range: send the whole object.
    Full,
    /// Inclusive byte positions.
    Partial { start: u64, end: u64 },
    /// Starts past the end: answer 416.
    Unsatisfiable,
}

/// Parse `Range: bytes=…` for an object of `size` bytes. Malformed headers,
/// other units, and multiple ranges are ignored (the whole object is sent).
pub fn parse_range(header: Option<&str>, size: u64) -> ByteRange {
    let Some(spec) = header.and_then(|h| h.trim().strip_prefix("bytes=")) else {
        return ByteRange::Full;
    };
    let Some((a, b)) = spec.split_once('-').filter(|_| !spec.contains(',')) else {
        return ByteRange::Full;
    };
    let (a, b) = (a.trim(), b.trim());
    let num = |s: &str| s.parse::<u64>().ok();
    match (a.is_empty(), b.is_empty()) {
        (true, true) => ByteRange::Full,
        // Suffix: the last `n` bytes.
        (true, false) => match num(b) {
            Some(0) => ByteRange::Unsatisfiable,
            Some(_) if size == 0 => ByteRange::Unsatisfiable,
            Some(n) => ByteRange::Partial {
                start: size.saturating_sub(n),
                end: size - 1,
            },
            None => ByteRange::Full,
        },
        (false, _) => {
            let Some(start) = num(a) else {
                return ByteRange::Full;
            };
            if start >= size {
                return ByteRange::Unsatisfiable;
            }
            let end = if b.is_empty() {
                size - 1
            } else {
                match num(b) {
                    Some(e) if e >= start => e.min(size - 1),
                    _ => return ByteRange::Full,
                }
            };
            ByteRange::Partial { start, end }
        }
    }
}

/// RFC 6266 attachment header: ASCII fallback plus UTF-8 `filename*`.
pub fn content_disposition(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| {
            if c == ' ' || (c.is_ascii_graphic() && c != '"' && c != '\\') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let encoded: String = name
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

/// Object-storage key for a file's content.
pub fn blob_key(owner_id: &str, file_id: &str) -> String {
    format!("u/{owner_id}/f/{file_id}")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Listing {
    pub folder: String,
    /// Immediate subfolder names.
    pub folders: Vec<String>,
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub used: u64,
    pub quota: u64,
    pub max_file: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FilePatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub folder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileError {
    Invalid(&'static str, &'static str),
    /// Same name already exists in that folder.
    Exists,
    TooLarge {
        max: u64,
    },
    QuotaExceeded {
        used: u64,
        quota: u64,
    },
    NotFound,
    Backend(String),
}

pub trait FileRepository {
    /// Files directly in `folder`, plus every folder path at or below it.
    async fn list(
        &self,
        owner_id: &str,
        folder: &str,
        limit: u32,
    ) -> Result<(Vec<FileEntry>, Vec<String>), String>;
    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<FileEntry>, String>;
    async fn exists(&self, owner_id: &str, folder: &str, name: &str) -> Result<bool, String>;
    /// Files directly in `folder`.
    async fn count_in_folder(&self, owner_id: &str, folder: &str) -> Result<u32, String>;
    /// `false` if (folder, name) is already taken.
    async fn insert(&self, entry: &FileEntry) -> Result<bool, String>;
    /// `false` if (folder, name) is already taken by another file.
    async fn update(&self, entry: &FileEntry) -> Result<bool, String>;
    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String>;
    async fn usage(&self, owner_id: &str) -> Result<u64, String>;
}

fn backend(e: String) -> FileError {
    FileError::Backend(e)
}

/// A single path segment / file name.
pub fn validate_name(raw: &str) -> Result<String, FileError> {
    let name = raw.trim();
    let bad = |why| Err(FileError::Invalid("name", why));
    if name.is_empty() {
        return bad("required");
    }
    if name.chars().count() > MAX_NAME {
        return bad("too long");
    }
    if name == "." || name == ".." {
        return bad("reserved");
    }
    if name
        .chars()
        .any(|c| c.is_control() || c == '/' || c == '\\')
    {
        return bad("invalid characters");
    }
    Ok(name.to_owned())
}

/// A folder path like `photos/2026`; empty for the root.
pub fn validate_folder(raw: &str) -> Result<String, FileError> {
    let trimmed = raw.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.chars().count() > MAX_FOLDER {
        return Err(FileError::Invalid("folder", "too long"));
    }
    let parts: Vec<String> = trimmed
        .split('/')
        .map(|s| validate_name(s).map_err(|_| FileError::Invalid("folder", "invalid segment")))
        .collect::<Result<_, _>>()?;
    if parts.len() > MAX_DEPTH {
        return Err(FileError::Invalid("folder", "too deep"));
    }
    Ok(parts.join("/"))
}

/// Immediate children of `folder` among `all` folder paths.
pub fn child_folders(folder: &str, all: &[String]) -> Vec<String> {
    let prefix = if folder.is_empty() {
        String::new()
    } else {
        format!("{folder}/")
    };
    let mut out: Vec<String> = all
        .iter()
        .filter_map(|f| f.strip_prefix(&prefix).filter(|rest| !rest.is_empty()))
        .map(|rest| rest.split('/').next().unwrap_or(rest).to_owned())
        .collect();
    out.sort_by_key(|n| n.to_lowercase());
    out.dedup();
    out
}

pub struct FileService<'a, R, E: ?Sized> {
    repo: &'a R,
    entropy: &'a E,
    now: i64,
    quota: u64,
}

impl<'a, R: FileRepository, E: Entropy + ?Sized> FileService<'a, R, E> {
    pub fn new(repo: &'a R, entropy: &'a E, now: i64, quota: u64) -> Self {
        Self {
            repo,
            entropy,
            now,
            quota,
        }
    }

    pub async fn list(&self, p: &Principal, folder: &str) -> Result<Listing, FileError> {
        let folder = validate_folder(folder)?;
        let (mut files, all_folders) = self
            .repo
            .list(&p.user_id, &folder, MAX_LIST)
            .await
            .map_err(backend)?;
        files.sort_by_key(|f| f.name.to_lowercase());
        Ok(Listing {
            folders: child_folders(&folder, &all_folders),
            folder,
            files,
        })
    }

    pub async fn get(&self, p: &Principal, id: &str) -> Result<FileEntry, FileError> {
        if !is_id(id) {
            return Err(FileError::NotFound);
        }
        self.repo
            .get(&p.user_id, id)
            .await
            .map_err(backend)?
            .ok_or(FileError::NotFound)
    }

    pub async fn usage(&self, p: &Principal) -> Result<Usage, FileError> {
        let used = self.repo.usage(&p.user_id).await.map_err(backend)?;
        Ok(Usage {
            used,
            quota: self.quota,
            max_file: MAX_FILE_BYTES,
        })
    }

    /// A folder lists at most `MAX_LIST` files, so it can't hold more (D-022).
    async fn check_folder_room(&self, p: &Principal, folder: &str) -> Result<(), FileError> {
        if self
            .repo
            .count_in_folder(&p.user_id, folder)
            .await
            .map_err(backend)?
            >= MAX_LIST
        {
            return Err(FileError::Invalid(
                "folder",
                "limit of 1,000 files per folder reached; use a subfolder",
            ));
        }
        Ok(())
    }

    /// Validate an upload and check limits. Nothing is stored yet.
    pub async fn prepare_upload(
        &self,
        p: &Principal,
        folder: &str,
        name: &str,
        size: u64,
        content_type: Option<&str>,
    ) -> Result<FileEntry, FileError> {
        let folder = validate_folder(folder)?;
        let name = validate_name(name)?;
        if size > MAX_FILE_BYTES {
            return Err(FileError::TooLarge {
                max: MAX_FILE_BYTES,
            });
        }
        let used = self.repo.usage(&p.user_id).await.map_err(backend)?;
        if used + size > self.quota {
            return Err(FileError::QuotaExceeded {
                used,
                quota: self.quota,
            });
        }
        if self
            .repo
            .exists(&p.user_id, &folder, &name)
            .await
            .map_err(backend)?
        {
            return Err(FileError::Exists);
        }
        self.check_folder_room(p, &folder).await?;
        Ok(FileEntry {
            id: new_id(self.entropy),
            owner_id: p.user_id.clone(),
            folder,
            name,
            size,
            content_type: normalize_content_type(content_type),
            created_at: self.now,
            updated_at: self.now,
        })
    }

    /// Record metadata after the content was stored. On `Err`, the caller
    /// must delete the blob it wrote.
    pub async fn commit_upload(&self, entry: &FileEntry) -> Result<(), FileError> {
        if self.repo.insert(entry).await.map_err(backend)? {
            Ok(())
        } else {
            Err(FileError::Exists)
        }
    }

    /// Rename and/or move (metadata only).
    pub async fn update(
        &self,
        p: &Principal,
        id: &str,
        patch: FilePatch,
    ) -> Result<FileEntry, FileError> {
        let mut entry = self.get(p, id).await?;
        if let Some(n) = patch.name {
            entry.name = validate_name(&n)?;
        }
        if let Some(f) = patch.folder {
            let folder = validate_folder(&f)?;
            if folder != entry.folder {
                self.check_folder_room(p, &folder).await?;
            }
            entry.folder = folder;
        }
        entry.updated_at = self.now;
        if self.repo.update(&entry).await.map_err(backend)? {
            Ok(entry)
        } else {
            Err(FileError::Exists)
        }
    }

    /// Remove the metadata and return the entry so the caller deletes the blob.
    pub async fn delete(&self, p: &Principal, id: &str) -> Result<FileEntry, FileError> {
        let entry = self.get(p, id).await?;
        if self.repo.delete(&p.user_id, id).await.map_err(backend)? {
            Ok(entry)
        } else {
            Err(FileError::NotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entropy::testing::CountingEntropy;
    use crate::identity::AuthMethod;
    use crate::test_util::block_on;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Memory(RefCell<Vec<FileEntry>>);

    impl FileRepository for Memory {
        async fn list(
            &self,
            owner_id: &str,
            folder: &str,
            limit: u32,
        ) -> Result<(Vec<FileEntry>, Vec<String>), String> {
            let all = self.0.borrow();
            let mine: Vec<_> = all.iter().filter(|f| f.owner_id == owner_id).collect();
            let files = mine
                .iter()
                .filter(|f| f.folder == folder)
                .take(limit as usize)
                .map(|f| (*f).clone())
                .collect();
            let folders = mine.iter().map(|f| f.folder.clone()).collect();
            Ok((files, folders))
        }
        async fn get(&self, owner_id: &str, id: &str) -> Result<Option<FileEntry>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .find(|f| f.owner_id == owner_id && f.id == id)
                .cloned())
        }
        async fn exists(&self, owner_id: &str, folder: &str, name: &str) -> Result<bool, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .any(|f| f.owner_id == owner_id && f.folder == folder && f.name == name))
        }
        async fn count_in_folder(&self, owner_id: &str, folder: &str) -> Result<u32, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .filter(|f| f.owner_id == owner_id && f.folder == folder)
                .count() as u32)
        }
        async fn insert(&self, e: &FileEntry) -> Result<bool, String> {
            if self.exists(&e.owner_id, &e.folder, &e.name).await? {
                return Ok(false);
            }
            self.0.borrow_mut().push(e.clone());
            Ok(true)
        }
        async fn update(&self, e: &FileEntry) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            if all.iter().any(|f| {
                f.owner_id == e.owner_id && f.id != e.id && f.folder == e.folder && f.name == e.name
            }) {
                return Ok(false);
            }
            if let Some(f) = all
                .iter_mut()
                .find(|f| f.owner_id == e.owner_id && f.id == e.id)
            {
                *f = e.clone();
            }
            Ok(true)
        }
        async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            let before = all.len();
            all.retain(|f| !(f.owner_id == owner_id && f.id == id));
            Ok(all.len() < before)
        }
        async fn usage(&self, owner_id: &str) -> Result<u64, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .filter(|f| f.owner_id == owner_id)
                .map(|f| f.size)
                .sum())
        }
    }

    fn me() -> Principal {
        Principal {
            user_id: "u1".into(),
            method: AuthMethod::Session,
        }
    }

    fn upload(
        svc: &FileService<'_, Memory, CountingEntropy>,
        folder: &str,
        name: &str,
        size: u64,
    ) -> Result<FileEntry, FileError> {
        let e = block_on(svc.prepare_upload(&me(), folder, name, size, Some("text/plain")))?;
        block_on(svc.commit_upload(&e))?;
        Ok(e)
    }

    #[test]
    fn byte_ranges() {
        use ByteRange::*;
        let r = |h: &str| parse_range(Some(h), 1000);
        assert_eq!(parse_range(None, 1000), Full);
        assert_eq!(r("bytes=0-99"), Partial { start: 0, end: 99 });
        assert_eq!(
            r("bytes=900-"),
            Partial {
                start: 900,
                end: 999
            }
        );
        assert_eq!(
            r("bytes=900-5000"),
            Partial {
                start: 900,
                end: 999
            }
        );
        assert_eq!(
            r("bytes=-100"),
            Partial {
                start: 900,
                end: 999
            }
        );
        assert_eq!(r("bytes=-5000"), Partial { start: 0, end: 999 });
        assert_eq!(r("bytes=1000-"), Unsatisfiable);
        assert_eq!(r("bytes=-0"), Unsatisfiable);
        for ignored in [
            "items=0-1",
            "bytes=5-2",
            "bytes=a-b",
            "bytes=0-1,5-6",
            "bytes=-",
            "bytes",
        ] {
            assert_eq!(r(ignored), Full, "{ignored}");
        }
        assert_eq!(parse_range(Some("bytes=0-"), 0), Unsatisfiable);
    }

    #[test]
    fn disposition_is_header_safe() {
        assert_eq!(
            content_disposition("a b.txt"),
            "attachment; filename=\"a b.txt\"; filename*=UTF-8''a%20b.txt"
        );
        let h = content_disposition("Résumé \"x\".pdf");
        assert!(
            h.starts_with("attachment; filename=\"R_sum_ _x_.pdf\""),
            "{h}"
        );
        assert!(
            h.contains("filename*=UTF-8''R%C3%A9sum%C3%A9%20%22x%22.pdf"),
            "{h}"
        );
        assert!(!content_disposition("a\r\nb").contains('\n'));
    }

    #[test]
    fn normalizes_content_types() {
        assert_eq!(normalize_content_type(None), DEFAULT_CONTENT_TYPE);
        assert_eq!(
            normalize_content_type(Some("Text/Plain; charset=utf-8")),
            "text/plain"
        );
        assert_eq!(
            normalize_content_type(Some("image/svg+xml")),
            "image/svg+xml"
        );
        assert_eq!(
            normalize_content_type(Some(
                "text/html
X-Evil: 1"
            )),
            DEFAULT_CONTENT_TYPE
        );
        assert_eq!(
            normalize_content_type(Some("nonsense")),
            DEFAULT_CONTENT_TYPE
        );
    }

    #[test]
    fn folders_listing_and_names() {
        let repo = Memory::default();
        let e = CountingEntropy::default();
        let svc = FileService::new(&repo, &e, 1, 1_000);
        upload(&svc, "", "readme.txt", 10).unwrap();
        upload(&svc, "/photos/2026/", "beach.jpg", 10).unwrap();
        upload(&svc, "photos", "cat.jpg", 10).unwrap();
        upload(&svc, "docs", "Résumé (final).pdf", 10).unwrap();

        let root = block_on(svc.list(&me(), "")).unwrap();
        assert_eq!(root.folders, ["docs", "photos"]);
        assert_eq!(root.files.len(), 1);
        let photos = block_on(svc.list(&me(), "photos")).unwrap();
        assert_eq!(
            (photos.folders.as_slice(), photos.files[0].name.as_str()),
            (["2026".to_string()].as_slice(), "cat.jpg")
        );
        assert_eq!(blob_key("u1", "abc"), "u/u1/f/abc");
    }

    #[test]
    fn a_folder_holds_at_most_one_list_of_files() {
        let repo = Memory::default();
        let e = CountingEntropy::default();
        let svc = FileService::new(&repo, &e, 1, u64::MAX);
        // Uploaded first: the test entropy repeats ids after 256, and lookups find the first.
        let other = upload(&svc, "scans/2026", "extra.pdf", 1).unwrap();
        for i in 0..MAX_LIST {
            upload(&svc, "scans", &format!("{i}.pdf"), 1).unwrap();
        }
        let full = FileError::Invalid(
            "folder",
            "limit of 1,000 files per folder reached; use a subfolder",
        );
        assert_eq!(upload(&svc, "scans", "extra.pdf", 1).unwrap_err(), full);
        let into_full = FilePatch {
            folder: Some("scans".into()),
            ..Default::default()
        };
        assert_eq!(
            block_on(svc.update(&me(), &other.id, into_full)).unwrap_err(),
            full
        );
        assert_eq!(
            block_on(svc.list(&me(), "scans")).unwrap().files.len(),
            MAX_LIST as usize
        );
    }

    #[test]
    fn validation_limits_and_quota() {
        let repo = Memory::default();
        let e = CountingEntropy::default();
        let svc = FileService::new(&repo, &e, 1, 100);
        for bad in ["", "..", "a/b", "tab\there"] {
            assert!(
                matches!(
                    block_on(svc.prepare_upload(&me(), "", bad, 1, None)),
                    Err(FileError::Invalid("name", _))
                ),
                "{bad:?}"
            );
        }
        assert!(matches!(
            validate_folder("a//b"),
            Err(FileError::Invalid("folder", _))
        ));
        assert!(matches!(
            validate_folder(&"a/".repeat(11)),
            Err(FileError::Invalid("folder", "too deep"))
        ));
        assert_eq!(
            block_on(svc.prepare_upload(&me(), "", "big", MAX_FILE_BYTES + 1, None)).unwrap_err(),
            FileError::TooLarge {
                max: MAX_FILE_BYTES
            }
        );
        upload(&svc, "", "a", 80).unwrap();
        assert_eq!(
            upload(&svc, "", "b", 30).unwrap_err(),
            FileError::QuotaExceeded {
                used: 80,
                quota: 100
            }
        );
        assert_eq!(upload(&svc, "", "a", 1).unwrap_err(), FileError::Exists);
        assert_eq!(block_on(svc.usage(&me())).unwrap().used, 80);
        let f =
            block_on(svc.prepare_upload(&me(), "", "x.bin", 1, Some("text/html\r\nX: y"))).unwrap();
        assert_eq!(
            f.content_type, "application/octet-stream",
            "content types are normalized"
        );
    }

    #[test]
    fn rename_move_delete_and_scope() {
        let repo = Memory::default();
        let e = CountingEntropy::default();
        let svc = FileService::new(&repo, &e, 1, 1_000);
        let a = upload(&svc, "", "a.txt", 1).unwrap();
        upload(&svc, "archive", "b.txt", 1).unwrap();
        let moved = block_on(svc.update(
            &me(),
            &a.id,
            FilePatch {
                folder: Some("archive".into()),
                name: Some("a-old.txt".into()),
            },
        ))
        .unwrap();
        assert_eq!(
            (moved.folder.as_str(), moved.name.as_str()),
            ("archive", "a-old.txt")
        );
        assert_eq!(
            block_on(svc.update(
                &me(),
                &a.id,
                FilePatch {
                    name: Some("b.txt".into()),
                    ..Default::default()
                }
            ))
            .unwrap_err(),
            FileError::Exists
        );
        let other = Principal {
            user_id: "u2".into(),
            method: AuthMethod::Session,
        };
        assert_eq!(
            block_on(svc.get(&other, &a.id)).unwrap_err(),
            FileError::NotFound
        );
        assert_eq!(
            block_on(svc.delete(&other, &a.id)).unwrap_err(),
            FileError::NotFound
        );
        assert_eq!(block_on(svc.delete(&me(), &a.id)).unwrap().id, a.id);
        assert_eq!(
            block_on(svc.get(&me(), &a.id)).unwrap_err(),
            FileError::NotFound
        );
    }
}
