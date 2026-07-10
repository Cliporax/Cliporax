use crate::file_sync::models::{
    FileSyncManifest, FileSyncRemoteEvent, RemoteEntrySummary, CHUNK_SIZE,
    FILE_SYNC_SCHEMA_VERSION, MAX_ENTRY_BYTES, MAX_ENTRY_FILES, MAX_SINGLE_FILE_BYTES,
};
use crate::file_sync::rows::EntryRow;
use std::collections::HashSet;
use std::path::{Component, Path};

pub(super) const REMOTE_ROOT: &str = "file-sync/v1";

pub(super) fn validate_file_size(size: u64) -> Result<(), String> {
    if size > MAX_SINGLE_FILE_BYTES {
        Err("A file exceeds the 20 GiB single-file limit".to_string())
    } else {
        Ok(())
    }
}

pub(super) fn validate_relative_path(path: &Path) -> Result<(), String> {
    if path.is_absolute() {
        return Err("Absolute paths are not allowed in a file sync snapshot".to_string());
    }
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let name = value
                    .to_str()
                    .ok_or_else(|| "File names must be valid UTF-8".to_string())?;
                validate_portable_name(name)?;
            }
            Component::CurDir => {}
            _ => return Err("A source path contains an unsafe component".to_string()),
        }
    }
    Ok(())
}

pub(super) fn validate_portable_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.ends_with(' ')
        || name.ends_with('.')
        || name.chars().any(|c| {
            c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        })
    {
        return Err("A file name is not portable across Windows, macOS, and Linux".to_string());
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    if reserved {
        return Err("A file name is reserved on Windows".to_string());
    }
    Ok(())
}

pub(super) fn relative_to_remote(path: &Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in path.components() {
        if let Component::Normal(value) = component {
            parts.push(
                value
                    .to_str()
                    .ok_or_else(|| "File names must be valid UTF-8".to_string())?,
            );
        }
    }
    Ok(parts.join("/"))
}

pub(super) fn validate_manifest(
    manifest: &FileSyncManifest,
    entry: &EntryRow,
) -> Result<(), String> {
    if manifest.schema_version != FILE_SYNC_SCHEMA_VERSION
        || manifest.entry_id != entry.id
        || manifest.revision != entry.revision
        || manifest.kind != entry.kind
        || manifest.display_name != entry.display_name
        || manifest.total_size > MAX_ENTRY_BYTES
        || manifest.file_count as usize > MAX_ENTRY_FILES
    {
        return Err("Remote manifest does not match the selected entry".to_string());
    }
    validate_portable_name(&manifest.display_name)?;
    let mut paths = HashSet::new();
    let mut remote_paths = HashSet::new();
    let mut total_size = 0u64;
    let mut file_count = 0u64;
    let remote_prefix = format!(
        "{}/entries/{}/{}/objects/",
        REMOTE_ROOT, entry.id, entry.revision
    );
    for node in &manifest.nodes {
        let path = Path::new(&node.path);
        validate_relative_path(path)?;
        if !paths.insert(node.path.clone()) {
            return Err("Remote manifest contains duplicate paths".to_string());
        }
        if node.kind != "file" && node.kind != "directory" {
            return Err("Remote manifest contains an unsupported node".to_string());
        }
        let chunk_total: u64 = node.chunks.iter().map(|chunk| chunk.size).sum();
        if node.kind == "file" && chunk_total != node.size {
            return Err("Remote manifest file size is inconsistent".to_string());
        }
        if node.kind == "directory" && (!node.chunks.is_empty() || node.size != 0) {
            return Err("Remote manifest directory is inconsistent".to_string());
        }
        if node.kind == "file" {
            file_count += 1;
            total_size = total_size
                .checked_add(node.size)
                .ok_or_else(|| "Remote manifest size overflowed".to_string())?;
            for (expected_index, chunk) in node.chunks.iter().enumerate() {
                if chunk.index as usize != expected_index
                    || chunk.size > CHUNK_SIZE as u64
                    || chunk.sha256.len() != 64
                    || !chunk
                        .sha256
                        .chars()
                        .all(|character| character.is_ascii_hexdigit())
                    || !chunk.remote_path.starts_with(&remote_prefix)
                    || !remote_paths.insert(chunk.remote_path.clone())
                    || chunk.remote_path.split('/').any(|component| {
                        component.is_empty() || component == "." || component == ".."
                    })
                {
                    return Err("Remote manifest chunk failed validation".to_string());
                }
            }
        }
    }
    if total_size != manifest.total_size || file_count != manifest.file_count {
        return Err("Remote manifest totals are inconsistent".to_string());
    }
    Ok(())
}

pub(super) fn validate_remote_event(
    event: &FileSyncRemoteEvent,
    expected_device: &str,
    expected_seq: i64,
) -> Result<(), String> {
    if event.schema_version != FILE_SYNC_SCHEMA_VERSION
        || event.device_id != expected_device
        || event.seq != expected_seq
        || event.entry_id.is_empty()
        || event.revision < 1
        || (event.operation != "upsert" && event.operation != "delete")
    {
        return Err("Remote file sync event failed validation".to_string());
    }
    validate_identifier(&event.device_id, "remote device ID")?;
    validate_identifier(&event.entry_id, "remote entry ID")?;
    if let Some(summary) = &event.entry {
        if summary.id != event.entry_id || summary.revision != event.revision {
            return Err("Remote event entry identity is inconsistent".to_string());
        }
    }
    Ok(())
}

pub(super) fn validate_remote_summary(summary: &RemoteEntrySummary) -> Result<(), String> {
    validate_identifier(&summary.id, "remote entry ID")?;
    validate_identifier(&summary.origin_device_id, "remote device ID")?;
    validate_portable_name(&summary.display_name)?;
    if summary.kind != "file" && summary.kind != "folder"
        || summary.total_size > MAX_ENTRY_BYTES
        || summary.file_count as usize > MAX_ENTRY_FILES
        || summary.revision < 1
        || !summary
            .manifest_path
            .starts_with(&format!("{}/entries/", REMOTE_ROOT))
        || summary.manifest_hash.len() != 64
    {
        return Err("Remote entry summary failed validation".to_string());
    }
    Ok(())
}

pub(super) fn validate_identifier(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(format!("Invalid {}", label));
    }
    Ok(())
}

pub(super) fn to_i64(value: u64, label: &str) -> Result<i64, String> {
    i64::try_from(value).map_err(|_| format!("{} is too large", label))
}
