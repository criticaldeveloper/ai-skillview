use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const FORMAT_VERSION: u32 = 1;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Skill {
    pub id: u64,
    pub name: String,
    pub prompt: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Document {
    version: u32,
    next_id: u64,
    skills: Vec<Skill>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            next_id: 1,
            skills: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum StorageError {
    MissingLocalAppData,
    Io { path: PathBuf, source: io::Error },
    InvalidData { path: PathBuf, message: String },
    Validation(&'static str),
    NotFound(u64),
    ExhaustedIds,
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingLocalAppData => formatter.write_str(
                "Windows did not provide a local application data folder (LOCALAPPDATA).",
            ),
            Self::Io { path, source } => {
                write!(formatter, "Could not access {}: {source}", path.display())
            }
            Self::InvalidData { path, message } => write!(
                formatter,
                "Could not open {}: {message}. The existing file has not been changed.",
                path.display()
            ),
            Self::Validation(message) => formatter.write_str(message),
            Self::NotFound(id) => write!(formatter, "Skill {id} no longer exists."),
            Self::ExhaustedIds => {
                formatter.write_str("The skill identifier limit has been reached.")
            }
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct Store {
    path: PathBuf,
    document: Document,
}

impl Store {
    pub fn load() -> Result<Self, StorageError> {
        let folder = std::env::var_os("LOCALAPPDATA")
            .filter(|value| !value.is_empty())
            .ok_or(StorageError::MissingLocalAppData)?;
        Self::load_from(
            PathBuf::from(folder)
                .join("AI Skillview")
                .join("skills.json"),
        )
    }

    fn load_from(path: PathBuf) -> Result<Self, StorageError> {
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self {
                    path,
                    document: Document::default(),
                });
            }
            Err(source) => return Err(StorageError::Io { path, source }),
        };
        let document: Document =
            serde_json::from_slice(&bytes).map_err(|error| StorageError::InvalidData {
                path: path.clone(),
                message: error.to_string(),
            })?;
        validate_document(&document).map_err(|message| StorageError::InvalidData {
            path: path.clone(),
            message,
        })?;
        Ok(Self { path, document })
    }

    pub fn skills(&self) -> &[Skill] {
        &self.document.skills
    }

    pub fn create(&mut self, name: &str, prompt: &str) -> Result<u64, StorageError> {
        validate_fields(name, prompt)?;
        let id = self.document.next_id;
        let next_id = id.checked_add(1).ok_or(StorageError::ExhaustedIds)?;
        self.document.skills.push(Skill {
            id,
            name: name.trim().to_owned(),
            prompt: prompt.to_owned(),
        });
        self.document.next_id = next_id;
        if let Err(error) = self.persist() {
            self.document.skills.pop();
            self.document.next_id = id;
            return Err(error);
        }
        Ok(id)
    }

    pub fn update(&mut self, id: u64, name: &str, prompt: &str) -> Result<(), StorageError> {
        validate_fields(name, prompt)?;
        let index = self.index_of(id)?;
        let replacement = Skill {
            id,
            name: name.trim().to_owned(),
            prompt: prompt.to_owned(),
        };
        let previous = std::mem::replace(&mut self.document.skills[index], replacement);
        if let Err(error) = self.persist() {
            self.document.skills[index] = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn remove(&mut self, id: u64) -> Result<(), StorageError> {
        let index = self.index_of(id)?;
        let previous = self.document.skills.remove(index);
        if let Err(error) = self.persist() {
            self.document.skills.insert(index, previous);
            return Err(error);
        }
        Ok(())
    }

    fn index_of(&self, id: u64) -> Result<usize, StorageError> {
        self.document
            .skills
            .iter()
            .position(|skill| skill.id == id)
            .ok_or(StorageError::NotFound(id))
    }

    fn persist(&self) -> Result<(), StorageError> {
        let parent = self.path.parent().ok_or_else(|| StorageError::Io {
            path: self.path.clone(),
            source: io::Error::new(io::ErrorKind::InvalidInput, "A data folder is required."),
        })?;
        fs::create_dir_all(parent).map_err(|source| StorageError::Io {
            path: parent.to_owned(),
            source,
        })?;
        let (temporary_path, file) = create_temporary_file(parent)?;
        let result = (|| {
            let mut writer = BufWriter::new(file);
            serde_json::to_writer(&mut writer, &self.document).map_err(io::Error::other)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
            writer.get_ref().sync_all()?;
            drop(writer);
            replace_file(&temporary_path, &self.path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result.map_err(|source| StorageError::Io {
            path: self.path.clone(),
            source,
        })
    }
}

fn validate_fields(name: &str, prompt: &str) -> Result<(), StorageError> {
    if name.trim().is_empty() {
        return Err(StorageError::Validation("Give this skill a name."));
    }
    if prompt.trim().is_empty() {
        return Err(StorageError::Validation("Add a prompt for this skill."));
    }
    if name.contains('\0') || prompt.contains('\0') {
        return Err(StorageError::Validation(
            "Names and prompts cannot contain null characters.",
        ));
    }
    Ok(())
}

fn validate_document(document: &Document) -> Result<(), String> {
    if document.version != FORMAT_VERSION {
        return Err(format!(
            "unsupported data version {} (this app supports version {FORMAT_VERSION})",
            document.version
        ));
    }
    if document.next_id == 0 {
        return Err("the next skill identifier must be positive".to_owned());
    }
    let mut ids = HashSet::with_capacity(document.skills.len());
    for skill in &document.skills {
        if skill.id == 0 || skill.id >= document.next_id || !ids.insert(skill.id) {
            return Err("skill identifiers are invalid or duplicated".to_owned());
        }
        validate_fields(&skill.name, &skill.prompt).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn create_temporary_file(parent: &Path) -> Result<(PathBuf, File), StorageError> {
    for _ in 0..32 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".skills.{}.{}.tmp", std::process::id(), sequence));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(StorageError::Io { path, source }),
        }
    }
    Err(StorageError::Io {
        path: parent.to_owned(),
        source: io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Could not reserve a temporary file for saving.",
        ),
    })
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // Both files are in the same directory: replacement never falls back to a copy.
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestFolder(PathBuf);

    impl TestFolder {
        fn new() -> Self {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "ai-skillview-test-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> PathBuf {
            self.0.join("skills.json")
        }
    }

    impl Drop for TestFolder {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn first_start_is_empty_and_does_not_create_a_file() {
        let folder = TestFolder::new();
        let store = Store::load_from(folder.path()).unwrap();
        assert!(store.skills().is_empty());
        assert!(!folder.path().exists());
    }

    #[test]
    fn changes_round_trip_in_order_and_preserve_prompt_whitespace() {
        let folder = TestFolder::new();
        let mut store = Store::load_from(folder.path()).unwrap();
        let first = store
            .create("  Explain code  ", "  Line one\n\nLine two\t")
            .unwrap();
        let second = store.create("日本語 🧠", "Unicode prompt: ñ 世界").unwrap();
        store
            .update(second, "Updated", "Replacement\r\nprompt")
            .unwrap();
        let loaded = Store::load_from(folder.path()).unwrap();
        assert_eq!(loaded.skills(), store.skills());
        assert_eq!(loaded.skills()[0].name, "Explain code");
        assert_eq!(loaded.skills()[0].prompt, "  Line one\n\nLine two\t");
        store.remove(first).unwrap();
        let third = store.create("Third", "New prompt").unwrap();
        assert!(third > second);
        assert_eq!(
            Store::load_from(folder.path()).unwrap().skills(),
            store.skills()
        );
        assert_eq!(
            store
                .skills()
                .iter()
                .map(|skill| skill.id)
                .collect::<Vec<_>>(),
            vec![second, third]
        );
        assert_eq!(fs::read_dir(&folder.0).unwrap().count(), 1);
    }

    #[test]
    fn validation_and_missing_ids_do_not_change_saved_data() {
        let folder = TestFolder::new();
        let mut store = Store::load_from(folder.path()).unwrap();
        let id = store.create("Original", "Keep me").unwrap();
        let before = fs::read(folder.path()).unwrap();
        assert!(store.create(" \n ", "Prompt").is_err());
        assert!(store.update(id, "Name", "\t\r\n").is_err());
        assert!(store.create("Name\0suffix", "Prompt").is_err());
        assert!(store.update(999, "Name", "Prompt").is_err());
        assert!(store.remove(999).is_err());
        assert_eq!(fs::read(folder.path()).unwrap(), before);
        assert_eq!(store.skills()[0].name, "Original");
        assert_eq!(store.skills()[0].prompt, "Keep me");
    }

    #[test]
    fn malformed_unknown_and_inconsistent_documents_are_preserved() {
        let folder = TestFolder::new();
        let documents = [
            "{broken",
            r#"{"version":2,"next_id":1,"skills":[]}"#,
            r#"{"version":1,"next_id":2,"skills":[{"id":1,"name":"A","prompt":"B"},{"id":1,"name":"C","prompt":"D"}]}"#,
            r#"{"version":1,"next_id":1,"skills":[{"id":1,"name":"A","prompt":"B"}]}"#,
            r#"{"version":1,"next_id":1,"skills":[{"id":0,"name":"A","prompt":"B"}]}"#,
            r#"{"version":1,"next_id":2,"skills":[{"id":1,"name":" ","prompt":"B"}]}"#,
        ];
        for document in documents {
            fs::write(folder.path(), document).unwrap();
            assert!(matches!(
                Store::load_from(folder.path()),
                Err(StorageError::InvalidData { .. })
            ));
            assert_eq!(fs::read_to_string(folder.path()).unwrap(), document);
        }
    }

    #[test]
    fn failed_saves_roll_back_all_in_memory_mutations() {
        let folder = TestFolder::new();
        let mut store = Store::load_from(folder.path()).unwrap();
        let id = store.create("Original", "Original prompt").unwrap();
        let original = store.skills().to_vec();
        fs::remove_file(folder.path()).unwrap();
        fs::create_dir(folder.path()).unwrap();
        assert!(store.create("Another", "Prompt").is_err());
        assert_eq!(store.skills(), original);
        assert!(store.update(id, "Changed", "Changed prompt").is_err());
        assert_eq!(store.skills(), original);
        assert!(store.remove(id).is_err());
        assert_eq!(store.skills(), original);
        assert_eq!(fs::read_dir(&folder.0).unwrap().count(), 1);
        fs::remove_dir(folder.path()).unwrap();
        let next = store.create("Another", "Prompt").unwrap();
        assert_eq!(next, id + 1);
    }
}
