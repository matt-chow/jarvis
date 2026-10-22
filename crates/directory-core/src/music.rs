//! Music: the owner's audio files (ordinary files, see `files`) plus playlists.
//! Playback streams file content with HTTP Range requests.

use crate::entropy::{is_id, new_id, Entropy};
use crate::files::FileEntry;
use crate::identity::Principal;
use crate::validate;
use serde::{Deserialize, Serialize};

pub const MAX_NAME: usize = 120;
pub const MAX_PLAYLISTS: u32 = 100;
pub const MAX_TRACKS: usize = 500;
const AUDIO_EXTENSIONS: [&str; 8] = [
    ".mp3", ".m4a", ".aac", ".ogg", ".oga", ".opus", ".wav", ".flac",
];

/// Whether a file is playable audio (by content type, then extension).
pub fn is_audio(f: &FileEntry) -> bool {
    let name = f.name.to_lowercase();
    f.content_type.starts_with("audio/") || AUDIO_EXTENSIONS.iter().any(|e| name.ends_with(e))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Playlist {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub name: String,
    /// File ids in play order (a track may appear more than once).
    pub tracks: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Everything the Music screen shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MusicLibrary {
    /// Audio files, by name.
    pub tracks: Vec<FileEntry>,
    /// Playlists, with tracks whose files were deleted left out.
    pub playlists: Vec<Playlist>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewPlaylist {
    pub name: String,
    #[serde(default)]
    pub tracks: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PlaylistPatch {
    #[serde(default)]
    pub name: Option<String>,
    /// Replaces the whole order.
    #[serde(default)]
    pub tracks: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MusicError {
    Invalid(&'static str, &'static str),
    NotFound,
    Backend(String),
}

pub trait MusicRepository {
    /// All the owner's files (audio is picked out here).
    async fn files(&self, owner_id: &str) -> Result<Vec<FileEntry>, String>;
    async fn playlists(&self, owner_id: &str, limit: u32) -> Result<Vec<Playlist>, String>;
    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Playlist>, String>;
    async fn insert(&self, playlist: &Playlist) -> Result<(), String>;
    async fn update(&self, playlist: &Playlist) -> Result<bool, String>;
    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String>;
    async fn count(&self, owner_id: &str) -> Result<u32, String>;
}

fn backend(e: String) -> MusicError {
    MusicError::Backend(e)
}

pub struct MusicService<'a, R, E: ?Sized> {
    repo: &'a R,
    entropy: &'a E,
    now: i64,
}

impl<'a, R: MusicRepository, E: Entropy + ?Sized> MusicService<'a, R, E> {
    pub fn new(repo: &'a R, entropy: &'a E, now: i64) -> Self {
        Self { repo, entropy, now }
    }

    async fn audio(&self, p: &Principal) -> Result<Vec<FileEntry>, MusicError> {
        let mut tracks: Vec<FileEntry> = self
            .repo
            .files(&p.user_id)
            .await
            .map_err(backend)?
            .into_iter()
            .filter(is_audio)
            .collect();
        tracks.sort_by_key(|f| f.name.to_lowercase());
        Ok(tracks)
    }

    pub async fn library(&self, p: &Principal) -> Result<MusicLibrary, MusicError> {
        let tracks = self.audio(p).await?;
        let mut playlists = self
            .repo
            .playlists(&p.user_id, MAX_PLAYLISTS)
            .await
            .map_err(backend)?;
        for pl in &mut playlists {
            pl.tracks.retain(|id| tracks.iter().any(|t| &t.id == id));
        }
        Ok(MusicLibrary { tracks, playlists })
    }

    /// Every id must be one of the owner's audio files.
    async fn check_tracks(
        &self,
        p: &Principal,
        ids: Vec<String>,
    ) -> Result<Vec<String>, MusicError> {
        if ids.len() > MAX_TRACKS {
            return Err(MusicError::Invalid("tracks", "at most 500 tracks"));
        }
        if ids.is_empty() {
            return Ok(ids);
        }
        let audio = self.audio(p).await?;
        if ids
            .iter()
            .any(|id| !is_id(id) || !audio.iter().any(|t| &t.id == id))
        {
            return Err(MusicError::Invalid("tracks", "not one of your audio files"));
        }
        Ok(ids)
    }

    pub async fn create(&self, p: &Principal, input: NewPlaylist) -> Result<Playlist, MusicError> {
        let name =
            validate::line(&input.name, MAX_NAME).map_err(|w| MusicError::Invalid("name", w))?;
        let tracks = self.check_tracks(p, input.tracks).await?;
        if self.repo.count(&p.user_id).await.map_err(backend)? >= MAX_PLAYLISTS {
            return Err(MusicError::Invalid(
                "playlists",
                "limit of 100 playlists reached",
            ));
        }
        let pl = Playlist {
            id: new_id(self.entropy),
            owner_id: p.user_id.clone(),
            name,
            tracks,
            created_at: self.now,
            updated_at: self.now,
        };
        self.repo.insert(&pl).await.map_err(backend)?;
        Ok(pl)
    }

    pub async fn update(
        &self,
        p: &Principal,
        id: &str,
        patch: PlaylistPatch,
    ) -> Result<Playlist, MusicError> {
        if !is_id(id) {
            return Err(MusicError::NotFound);
        }
        let mut pl = self
            .repo
            .get(&p.user_id, id)
            .await
            .map_err(backend)?
            .ok_or(MusicError::NotFound)?;
        if let Some(n) = patch.name {
            pl.name = validate::line(&n, MAX_NAME).map_err(|w| MusicError::Invalid("name", w))?;
        }
        if let Some(t) = patch.tracks {
            pl.tracks = self.check_tracks(p, t).await?;
        }
        pl.updated_at = self.now;
        if !self.repo.update(&pl).await.map_err(backend)? {
            return Err(MusicError::NotFound);
        }
        Ok(pl)
    }

    pub async fn delete(&self, p: &Principal, id: &str) -> Result<(), MusicError> {
        if is_id(id) && self.repo.delete(&p.user_id, id).await.map_err(backend)? {
            Ok(())
        } else {
            Err(MusicError::NotFound)
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
    struct Memory(RefCell<Vec<FileEntry>>, RefCell<Vec<Playlist>>);

    impl MusicRepository for Memory {
        async fn files(&self, owner_id: &str) -> Result<Vec<FileEntry>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .filter(|f| f.owner_id == owner_id)
                .cloned()
                .collect())
        }
        async fn playlists(&self, owner_id: &str, limit: u32) -> Result<Vec<Playlist>, String> {
            Ok(self
                .1
                .borrow()
                .iter()
                .filter(|p| p.owner_id == owner_id)
                .take(limit as usize)
                .cloned()
                .collect())
        }
        async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Playlist>, String> {
            Ok(self
                .1
                .borrow()
                .iter()
                .find(|p| p.owner_id == owner_id && p.id == id)
                .cloned())
        }
        async fn insert(&self, playlist: &Playlist) -> Result<(), String> {
            self.1.borrow_mut().push(playlist.clone());
            Ok(())
        }
        async fn update(&self, playlist: &Playlist) -> Result<bool, String> {
            let mut all = self.1.borrow_mut();
            match all
                .iter_mut()
                .find(|p| p.owner_id == playlist.owner_id && p.id == playlist.id)
            {
                Some(p) => {
                    *p = playlist.clone();
                    Ok(true)
                }
                None => Ok(false),
            }
        }
        async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
            let mut all = self.1.borrow_mut();
            let before = all.len();
            all.retain(|p| !(p.owner_id == owner_id && p.id == id));
            Ok(all.len() < before)
        }
        async fn count(&self, owner_id: &str) -> Result<u32, String> {
            Ok(self
                .1
                .borrow()
                .iter()
                .filter(|p| p.owner_id == owner_id)
                .count() as u32)
        }
    }

    fn file(c: char, name: &str, content_type: &str) -> FileEntry {
        FileEntry {
            id: c.to_string().repeat(32),
            owner_id: "u1".into(),
            folder: "Music".into(),
            name: name.into(),
            size: 1,
            content_type: content_type.into(),
            created_at: 0,
            updated_at: 0,
        }
    }

    fn me() -> Principal {
        Principal {
            user_id: "u1".into(),
            method: AuthMethod::Session,
        }
    }

    #[test]
    fn library_lists_audio_and_playlists_skip_deleted_tracks() {
        let repo = Memory::default();
        repo.0.borrow_mut().extend([
            file('a', "b-side.mp3", "audio/mpeg"),
            file('b', "Anthem.FLAC", "application/octet-stream"),
            file('c', "notes.txt", "text/plain"),
        ]);
        let e = CountingEntropy::default();
        let svc = MusicService::new(&repo, &e, 1);
        let a = "a".repeat(32);
        let b = "b".repeat(32);

        let lib = block_on(svc.library(&me())).unwrap();
        assert_eq!(
            lib.tracks
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            ["Anthem.FLAC", "b-side.mp3"]
        );

        let pl = block_on(svc.create(
            &me(),
            NewPlaylist {
                name: " Road trip ".into(),
                tracks: vec![b.clone(), a.clone(), b.clone()],
            },
        ))
        .unwrap();
        assert_eq!((pl.name.as_str(), pl.tracks.len()), ("Road trip", 3));
        assert_eq!(
            block_on(svc.create(
                &me(),
                NewPlaylist {
                    name: "x".into(),
                    tracks: vec!["c".repeat(32)],
                }
            ))
            .unwrap_err(),
            MusicError::Invalid("tracks", "not one of your audio files")
        );

        let reordered = block_on(svc.update(
            &me(),
            &pl.id,
            PlaylistPatch {
                tracks: Some(vec![a.clone(), b.clone()]),
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!(reordered.tracks, [a.clone(), b.clone()]);

        // A deleted file disappears from playlists when read.
        repo.0.borrow_mut().retain(|f| f.id != a);
        let lib = block_on(svc.library(&me())).unwrap();
        assert_eq!(lib.playlists[0].tracks, [b]);

        let other = Principal {
            user_id: "u2".into(),
            method: AuthMethod::Session,
        };
        assert_eq!(
            block_on(svc.delete(&other, &pl.id)).unwrap_err(),
            MusicError::NotFound
        );
        block_on(svc.delete(&me(), &pl.id)).unwrap();
    }
}
