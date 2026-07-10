use crate::file_sync::models::{
    FileSyncManifest, ManifestChunk, ManifestNode, PreparedChunk, PreparedSnapshot, ScanResult,
    ScannedNode, CHUNK_SIZE, FILE_SYNC_SCHEMA_VERSION, MAX_ENTRY_BYTES, MAX_ENTRY_FILES,
};
use crate::file_sync::validation::{
    relative_to_remote, validate_file_size, validate_portable_name, validate_relative_path,
    REMOTE_ROOT,
};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub(super) fn validate_top_level_source(path: &Path) -> Result<(String, &'static str), String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "A selected file or folder is no longer accessible".to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("Symbolic links are not supported by File Sync".to_string());
    }
    if !metadata.is_file() && !metadata.is_dir() {
        return Err("Only regular files and folders can be synchronized".to_string());
    }
    let display_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "A selected path has an invalid file name".to_string())?
        .to_string();
    validate_portable_name(&display_name)?;
    Ok((
        display_name,
        if metadata.is_dir() { "folder" } else { "file" },
    ))
}

pub(super) fn scan_source(source: &Path) -> Result<ScanResult, String> {
    let metadata = std::fs::symlink_metadata(source)
        .map_err(|_| "Source file or folder is no longer accessible".to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("Symbolic links are not supported by File Sync".to_string());
    }
    let display_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Source has an invalid file name".to_string())?
        .to_string();
    validate_portable_name(&display_name)?;
    let kind = if metadata.is_dir() { "folder" } else { "file" }.to_string();
    let mut nodes = Vec::new();
    if metadata.is_file() {
        validate_file_size(metadata.len())?;
        nodes.push(scanned_node(
            source.to_path_buf(),
            String::new(),
            "file",
            &metadata,
        ));
    } else if metadata.is_dir() {
        nodes.push(scanned_node(
            source.to_path_buf(),
            String::new(),
            "directory",
            &metadata,
        ));
        scan_directory(source, source, &mut nodes)?;
    } else {
        return Err("Only regular files and folders can be synchronized".to_string());
    }
    let total_size = nodes
        .iter()
        .filter(|node| node.kind == "file")
        .try_fold(0u64, |total, node| total.checked_add(node.size))
        .ok_or_else(|| "Entry size overflowed".to_string())?;
    let file_count = nodes.iter().filter(|node| node.kind == "file").count();
    if total_size > MAX_ENTRY_BYTES {
        return Err("Entry exceeds the 100 GiB size limit".to_string());
    }
    if file_count > MAX_ENTRY_FILES {
        return Err(format!("Entry exceeds the {} file limit", MAX_ENTRY_FILES));
    }
    Ok(ScanResult {
        kind,
        display_name,
        total_size,
        file_count: file_count as u64,
        nodes,
    })
}

fn scan_directory(
    root: &Path,
    directory: &Path,
    nodes: &mut Vec<ScannedNode>,
) -> Result<(), String> {
    let mut entries = std::fs::read_dir(directory)
        .map_err(|_| "A source directory could not be read".to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "A source directory entry could not be read".to_string())?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if nodes.len() > MAX_ENTRY_FILES * 2 {
            return Err("Entry contains too many filesystem nodes".to_string());
        }
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|_| "A source entry could not be inspected".to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("Folders containing symbolic links are not supported".to_string());
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "Source path escaped its root".to_string())?;
        validate_relative_path(relative)?;
        let relative_string = relative_to_remote(relative)?;
        if metadata.is_dir() {
            nodes.push(scanned_node(
                path.clone(),
                relative_string,
                "directory",
                &metadata,
            ));
            scan_directory(root, &path, nodes)?;
        } else if metadata.is_file() {
            validate_file_size(metadata.len())?;
            nodes.push(scanned_node(path, relative_string, "file", &metadata));
        } else {
            return Err("Folders may only contain regular files and directories".to_string());
        }
    }
    Ok(())
}

fn scanned_node(
    absolute_path: PathBuf,
    relative_path: String,
    kind: &str,
    metadata: &std::fs::Metadata,
) -> ScannedNode {
    ScannedNode {
        absolute_path,
        relative_path,
        kind: kind.to_string(),
        size: if metadata.is_file() {
            metadata.len()
        } else {
            0
        },
        modified_unix_ms: metadata.modified().ok().and_then(|modified| {
            modified
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        }),
        file_identity: file_identity(metadata),
    }
}

pub(super) fn prepare_snapshot(
    entry_id: &str,
    revision: i64,
    source: &Path,
    staging_root: &Path,
    chunk_size: usize,
) -> Result<PreparedSnapshot, String> {
    if chunk_size == 0 || chunk_size > CHUNK_SIZE {
        return Err("Invalid file sync chunk size".to_string());
    }
    let initial = scan_source(source)?;
    if staging_root.exists() {
        std::fs::remove_dir_all(staging_root)
            .map_err(|_| "Failed to clear old staging data".to_string())?;
    }
    std::fs::create_dir_all(staging_root.join("chunks"))
        .map_err(|_| "Failed to create staging directory".to_string())?;

    let mut manifest_nodes = Vec::with_capacity(initial.nodes.len());
    let mut prepared_chunks = Vec::new();
    let mut file_index = 0i64;
    for node in &initial.nodes {
        if node.kind == "directory" {
            manifest_nodes.push(ManifestNode {
                path: node.relative_path.clone(),
                kind: "directory".to_string(),
                size: 0,
                modified_unix_ms: node.modified_unix_ms,
                chunks: Vec::new(),
            });
            continue;
        }

        let before = std::fs::metadata(&node.absolute_path)
            .map_err(|_| "A source file disappeared during preparation".to_string())?;
        let mut input = File::open(&node.absolute_path)
            .map_err(|_| "A source file could not be opened".to_string())?;
        FileExt::try_lock_exclusive(&input)
            .map_err(|_| "A source file is in use and could not be locked".to_string())?;
        let mut chunks = Vec::new();
        let mut chunk_index = 0i64;
        loop {
            let mut buffer = vec![0u8; chunk_size];
            let mut read = 0usize;
            while read < buffer.len() {
                let count = input
                    .read(&mut buffer[read..])
                    .map_err(|_| "A locked source file could not be read".to_string())?;
                if count == 0 {
                    break;
                }
                read += count;
            }
            if read == 0 {
                break;
            }
            buffer.truncate(read);
            let hash = sha256_hex(&buffer);
            let chunk_dir = staging_root.join("chunks").join(file_index.to_string());
            std::fs::create_dir_all(&chunk_dir)
                .map_err(|_| "Failed to create chunk staging directory".to_string())?;
            let staging_path = chunk_dir.join(format!("{}.bin", chunk_index));
            let mut output = File::create(&staging_path)
                .map_err(|_| "Failed to create staged chunk".to_string())?;
            output
                .write_all(&buffer)
                .map_err(|_| "Failed to write staged chunk".to_string())?;
            output
                .sync_all()
                .map_err(|_| "Failed to persist staged chunk".to_string())?;
            let remote_path = format!(
                "{}/entries/{}/{}/objects/{}/{}.bin",
                REMOTE_ROOT, entry_id, revision, file_index, chunk_index
            );
            chunks.push(ManifestChunk {
                index: chunk_index as u32,
                size: read as u64,
                sha256: hash.clone(),
                remote_path: remote_path.clone(),
            });
            prepared_chunks.push(PreparedChunk {
                file_index,
                chunk_index,
                relative_path: node.relative_path.clone(),
                size: read as u64,
                plaintext_hash: hash,
                remote_path,
                staging_path,
            });
            chunk_index += 1;
        }
        let handle_after = input
            .metadata()
            .map_err(|_| "A locked source file could not be rechecked".to_string())?;
        let path_after = std::fs::metadata(&node.absolute_path)
            .map_err(|_| "A source file disappeared during preparation".to_string())?;
        if !same_file_metadata(&before, &handle_after) || !same_file_metadata(&before, &path_after)
        {
            return Err("A source file changed while its snapshot was created".to_string());
        }
        FileExt::unlock(&input).ok();
        manifest_nodes.push(ManifestNode {
            path: node.relative_path.clone(),
            kind: "file".to_string(),
            size: node.size,
            modified_unix_ms: node.modified_unix_ms,
            chunks,
        });
        file_index += 1;
    }

    let final_scan = scan_source(source)?;
    if scan_signature(&initial) != scan_signature(&final_scan) {
        return Err("Source folder changed while its snapshot was created".to_string());
    }
    let manifest = FileSyncManifest {
        schema_version: FILE_SYNC_SCHEMA_VERSION,
        entry_id: entry_id.to_string(),
        revision,
        kind: initial.kind,
        display_name: initial.display_name,
        total_size: initial.total_size,
        file_count: initial.file_count,
        created_at: chrono::Utc::now().to_rfc3339(),
        nodes: manifest_nodes,
    };
    let bytes = serde_json::to_vec(&manifest)
        .map_err(|_| "Failed to encode snapshot manifest".to_string())?;
    let manifest_hash = sha256_hex(&bytes);
    let manifest_path = staging_root.join("manifest.json");
    std::fs::write(&manifest_path, bytes)
        .map_err(|_| "Failed to persist snapshot manifest".to_string())?;
    Ok(PreparedSnapshot {
        manifest,
        manifest_path,
        manifest_hash,
        chunks: prepared_chunks,
    })
}

pub(super) fn same_file_metadata(before: &std::fs::Metadata, after: &std::fs::Metadata) -> bool {
    before.len() == after.len()
        && before.modified().ok() == after.modified().ok()
        && match (file_identity(before), file_identity(after)) {
            (Some(before), Some(after)) => before == after,
            _ => true,
        }
}

#[cfg(unix)]
pub(super) fn file_identity(metadata: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
pub(super) fn file_identity(metadata: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::windows::fs::MetadataExt;
    Some((
        metadata.creation_time(),
        u64::from(metadata.file_attributes()),
    ))
}

#[cfg(not(any(unix, windows)))]
pub(super) fn file_identity(_metadata: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}

type ScanSignatureEntry = (String, String, u64, Option<i64>, Option<(u64, u64)>);

fn scan_signature(scan: &ScanResult) -> Vec<ScanSignatureEntry> {
    scan.nodes
        .iter()
        .map(|node| {
            (
                node.relative_path.clone(),
                node.kind.clone(),
                node.size,
                node.modified_unix_ms,
                node.file_identity,
            )
        })
        .collect()
}

pub(super) fn ensure_staging_space(data_root: &Path, required: u64) -> Result<(), String> {
    let available = fs2::available_space(data_root)
        .map_err(|_| "Could not determine available staging disk space".to_string())?;
    let reserve = 64 * 1024 * 1024;
    if available < required.saturating_add(reserve) {
        return Err("Not enough disk space to create the immutable upload snapshot".to_string());
    }
    Ok(())
}

fn sha256_hex(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
