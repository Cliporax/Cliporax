//! Cliporax CLI - Command-line interface for accessing clipboard history
//!
//! Usage:
//!   cliporax-cli get <id>              - Get item by ID
//!   cliporax-cli get latest            - Get the most recent item
//!   cliporax-cli get --index <n>       - Get item at index (0-based)
//!   cliporax-cli list [--limit <n>]    - List recent items
//!   cliporax-cli search <query>        - Search items
//!   cliporax-cli copy <text>           - Copy text to system clipboard
//!   cliporax-cli copy --file <path>    - Copy file content to clipboard
//!   cliporax-cli copy --image <path>   - Copy image to clipboard
//!   cliporax-cli save <text>           - Save text to clipboard history
//!   cliporax-cli save --file <path>    - Save file content to history

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process;

const PORTABLE_MARKERS: &[&str] = &["portable", "cliporax.portable"];
const PORTABLE_DATA_DIR: &str = "data";
const DATA_DIR_ENV: &str = "CLIPORAX_DATA_DIR";

fn portable_data_dir() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))?;
    let data_dir = exe_dir.join(PORTABLE_DATA_DIR);

    let has_marker = PORTABLE_MARKERS
        .iter()
        .any(|marker| exe_dir.join(marker).exists());
    let has_existing_db = data_dir.join("cliporax.db").exists();

    if has_marker || has_existing_db {
        Some(data_dir)
    } else {
        None
    }
}

#[derive(Parser)]
#[command(name = "cliporax-cli")]
#[command(about = "Cliporax CLI - Access clipboard history from command line", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Get clipboard item content
    Get {
        /// Item ID (or "latest" for the most recent item)
        id: Option<String>,

        /// Get item at specific index (0-based, requires --index flag)
        #[arg(short, long)]
        index: Option<i64>,

        /// Tab ID (default: 1)
        #[arg(short, long, default_value_t = 1)]
        tab: i64,

        /// Copy content to system clipboard
        #[arg(short, long)]
        copy: bool,

        /// Output only content (no metadata)
        #[arg(short, long)]
        raw: bool,
    },

    /// List recent clipboard items
    List {
        /// Number of items to show (default: 10)
        #[arg(short, long, default_value_t = 10)]
        limit: i64,

        /// Tab ID (default: 1)
        #[arg(short, long, default_value_t = 1)]
        tab: i64,

        /// Show full content preview
        #[arg(short, long)]
        full: bool,
    },

    /// Search clipboard items
    Search {
        /// Search query
        query: String,

        /// Tab ID (default: 1)
        #[arg(short, long, default_value_t = 1)]
        tab: i64,

        /// Limit results (default: 20)
        #[arg(short, long, default_value_t = 20)]
        limit: i64,
    },

    /// Copy text/image to system clipboard
    Copy {
        /// Text content to copy
        text: Option<String>,

        /// Read content from file
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Copy as image (requires image file path)
        #[arg(short, long)]
        image: Option<PathBuf>,

        /// Also save to clipboard history
        #[arg(short, long)]
        save: bool,

        /// Tab ID for saving (default: 1)
        #[arg(short, long, default_value_t = 1)]
        tab: i64,
    },

    /// Save content to clipboard history
    Save {
        /// Text content to save
        text: Option<String>,

        /// Read content from file
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Save as image (requires image file path)
        #[arg(short, long)]
        image: Option<PathBuf>,

        /// Also copy to system clipboard
        #[arg(short, long)]
        copy: bool,

        /// Tab ID (default: 1)
        #[arg(short, long, default_value_t = 1)]
        tab: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
struct ClipboardItem {
    id: Option<i64>,
    #[serde(rename = "type")]
    #[sqlx(rename = "type")]
    item_type: String,
    content: String,
    content_hash: Option<String>,
    metadata: Option<String>,
    tags: Option<String>,
    tab_id: Option<i64>,
    is_sensitive: Option<i32>,
    is_pinned: Option<i32>,
    display_order: Option<i32>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

/// Get the database path from the app data directory
fn get_db_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(data_dir) = data_dir_override(std::env::var_os(DATA_DIR_ENV)) {
        return Ok(data_dir.join("cliporax.db"));
    }

    if let Some(data_dir) = portable_data_dir() {
        return Ok(data_dir.join("cliporax.db"));
    }

    let data_dir = dirs::data_local_dir().ok_or("Failed to get local data directory")?;

    // Try common Cliporax data directories
    let possible_paths = vec![
        // Linux: ~/.local/share/cliporax/
        data_dir.join("cliporax").join("cliporax.db"),
        // Linux: ~/.local/share/com.cliporax.app/
        data_dir.join("com.cliporax.app").join("cliporax.db"),
        // macOS: ~/Library/Application Support/com.cliporax.app/
        dirs::config_dir()
            .unwrap_or_else(|| data_dir.clone())
            .join("com.cliporax.app")
            .join("cliporax.db"),
        // Windows: %APPDATA%\com.cliporax.app\
        dirs::config_dir()
            .unwrap_or_else(|| data_dir.clone())
            .join("com.cliporax.app")
            .join("cliporax.db"),
    ];

    for path in &possible_paths {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    // Return the first path if none exist (will show error later)
    Ok(possible_paths[0].clone())
}

fn data_dir_override(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value.filter(|path| !path.is_empty()).map(PathBuf::from)
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn data_dir_override_accepts_an_explicit_directory() {
        assert_eq!(
            data_dir_override(Some(std::ffi::OsString::from("dev-data"))),
            Some(PathBuf::from("dev-data"))
        );
    }

    #[test]
    fn data_dir_override_ignores_an_empty_value() {
        assert_eq!(data_dir_override(Some(std::ffi::OsString::new())), None);
    }
}

/// Initialize database connection
async fn init_db(db_path: &Path) -> Result<sqlx::SqlitePool, Box<dyn std::error::Error>> {
    if !db_path.exists() {
        return Err(format!("Database not found at: {}", db_path.display()).into());
    }

    let connect_options = sqlx::sqlite::SqliteConnectOptions::new().filename(db_path);
    let pool = sqlx::SqlitePool::connect_with(connect_options).await?;

    Ok(pool)
}

/// Get item by ID
async fn get_item_by_id(
    pool: &sqlx::SqlitePool,
    id: i64,
) -> Result<Option<ClipboardItem>, Box<dyn std::error::Error>> {
    let item = sqlx::query_as::<_, ClipboardItem>("SELECT * FROM clipboard_items WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    Ok(item)
}

/// Get latest item
async fn get_latest_item(
    pool: &sqlx::SqlitePool,
    tab_id: i64,
) -> Result<Option<ClipboardItem>, Box<dyn std::error::Error>> {
    let item = sqlx::query_as::<_, ClipboardItem>(
        "SELECT * FROM clipboard_items WHERE tab_id = ? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tab_id)
    .fetch_optional(pool)
    .await?;

    Ok(item)
}

/// Get item by index
async fn get_item_by_index(
    pool: &sqlx::SqlitePool,
    tab_id: i64,
    index: i64,
) -> Result<Option<ClipboardItem>, Box<dyn std::error::Error>> {
    let item = sqlx::query_as::<_, ClipboardItem>(
        "SELECT * FROM clipboard_items WHERE tab_id = ? ORDER BY created_at DESC LIMIT 1 OFFSET ?",
    )
    .bind(tab_id)
    .bind(index)
    .fetch_optional(pool)
    .await?;

    Ok(item)
}

/// List recent items
async fn list_items(
    pool: &sqlx::SqlitePool,
    tab_id: i64,
    limit: i64,
) -> Result<Vec<ClipboardItem>, Box<dyn std::error::Error>> {
    let items = sqlx::query_as::<_, ClipboardItem>(
        "SELECT * FROM clipboard_items WHERE tab_id = ? ORDER BY created_at DESC LIMIT ?",
    )
    .bind(tab_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(items)
}

/// Search items
async fn search_items(
    pool: &sqlx::SqlitePool,
    query: &str,
    tab_id: i64,
    limit: i64,
) -> Result<Vec<ClipboardItem>, Box<dyn std::error::Error>> {
    let items = sqlx::query_as::<_, ClipboardItem>(
        "SELECT * FROM clipboard_items WHERE tab_id = ? AND content LIKE ? ORDER BY created_at DESC LIMIT ?"
    )
    .bind(tab_id)
    .bind(format!("%{}%", query))
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(items)
}

/// Copy text to system clipboard
fn copy_to_clipboard(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::process::Command;

    #[cfg(target_os = "linux")]
    {
        let mut child = Command::new("xclip")
            .arg("-selection")
            .arg("clipboard")
            .arg("-in")
            .stdin(std::process::Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(text.as_bytes())?;
        }

        child.wait()?;
    }

    #[cfg(target_os = "macos")]
    {
        let mut child = Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(text.as_bytes())?;
        }

        child.wait()?;
    }

    #[cfg(target_os = "windows")]
    {
        let mut child = Command::new("clip")
            .stdin(std::process::Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(text.as_bytes())?;
        }

        child.wait()?;
    }

    Ok(())
}

/// Copy image to system clipboard
fn copy_image_to_clipboard(image_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::process::Command;

    #[cfg(target_os = "linux")]
    {
        // Use xclip to copy image
        let status = Command::new("xclip")
            .arg("-selection")
            .arg("clipboard")
            .arg("-t")
            .arg("image/png")
            .arg("-i")
            .arg(image_path)
            .status()?;

        if !status.success() {
            return Err("Failed to copy image using xclip".into());
        }
    }

    #[cfg(target_os = "macos")]
    {
        // Use osascript to copy image
        let script = format!(
            "set the clipboard to (read (POSIX file \"{}\") as «class PNGf»)",
            image_path.to_string_lossy()
        );
        let status = Command::new("osascript").arg("-e").arg(script).status()?;

        if !status.success() {
            return Err("Failed to copy image using osascript".into());
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Windows: copy image file path (full image copy requires more complex handling)
        eprintln!("Warning: Image copy on Windows is limited to file path");
        let text = image_path.to_string_lossy().to_string();
        copy_to_clipboard(&text)?;
    }

    Ok(())
}

/// Save item to clipboard history database
async fn save_to_history(
    pool: &sqlx::SqlitePool,
    content: &str,
    item_type: &str,
    tab_id: i64,
) -> Result<i64, Box<dyn std::error::Error>> {
    // Generate content hash for dedup
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    // Insert into database
    let result = sqlx::query(
        r#"
        INSERT INTO clipboard_items 
        (type, content, content_hash, metadata, tags, tab_id, is_sensitive, is_pinned, display_order, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, 0, 0, 0, datetime('now'), datetime('now'))
        "#
    )
    .bind(item_type)
    .bind(content)
    .bind(hash)
    .bind("{}")
    .bind("[]")
    .bind(tab_id)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

/// Format item for display
fn format_item(item: &ClipboardItem, full: bool) -> String {
    let id = item.id.unwrap_or(0);
    let item_type = &item.item_type;
    let created = item.created_at.as_deref().unwrap_or("unknown");

    let content_preview = if full {
        &item.content
    } else if item.content.len() > 100 {
        &format!("{}...", &item.content[..100])
    } else {
        &item.content
    };

    format!(
        "ID: {} | Type: {} | Created: {}\n{}",
        id, item_type, created, content_preview
    )
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Get database path
    let db_path = match get_db_path() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    // Initialize database
    let pool = match init_db(&db_path).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    match cli.command {
        Commands::Get {
            id,
            index,
            tab,
            copy,
            raw,
        } => {
            let item = if let Some(id_str) = id {
                if id_str == "latest" {
                    match get_latest_item(&pool, tab).await {
                        Ok(item) => item,
                        Err(e) => {
                            eprintln!("Error: {}", e);
                            process::exit(1);
                        }
                    }
                } else {
                    match id_str.parse::<i64>() {
                        Ok(id_num) => match get_item_by_id(&pool, id_num).await {
                            Ok(item) => item,
                            Err(e) => {
                                eprintln!("Error: {}", e);
                                process::exit(1);
                            }
                        },
                        Err(_) => {
                            eprintln!("Error: Invalid ID '{}'. Use a number or 'latest'", id_str);
                            process::exit(1);
                        }
                    }
                }
            } else if let Some(idx) = index {
                match get_item_by_index(&pool, tab, idx).await {
                    Ok(item) => item,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        process::exit(1);
                    }
                }
            } else {
                eprintln!("Error: Please provide an ID or use --index");
                process::exit(1);
            };

            match item {
                Some(item) => {
                    if raw {
                        // Output only content
                        print!("{}", item.content);
                    } else {
                        // Output formatted
                        println!("{}", format_item(&item, true));
                    }

                    // Copy to clipboard if requested
                    if copy {
                        match copy_to_clipboard(&item.content) {
                            Ok(_) => eprintln!("Content copied to clipboard"),
                            Err(e) => eprintln!("Warning: Failed to copy to clipboard: {}", e),
                        }
                    }
                }
                None => {
                    eprintln!("Error: Item not found");
                    process::exit(1);
                }
            }
        }

        Commands::List { limit, tab, full } => match list_items(&pool, tab, limit).await {
            Ok(items) => {
                if items.is_empty() {
                    println!("No items found");
                } else {
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            println!("\n{}", "─".repeat(60));
                        }
                        println!("{}", format_item(item, full));
                    }
                    println!("\n\nTotal: {} items", items.len());
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        },

        Commands::Search { query, tab, limit } => {
            match search_items(&pool, &query, tab, limit).await {
                Ok(items) => {
                    if items.is_empty() {
                        println!("No items found matching '{}'", query);
                    } else {
                        println!("Found {} items matching '{}':\n", items.len(), query);
                        for (i, item) in items.iter().enumerate() {
                            if i > 0 {
                                println!("\n{}", "─".repeat(60));
                            }
                            println!("{}", format_item(item, false));
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    process::exit(1);
                }
            }
        }

        Commands::Copy {
            text,
            file,
            image,
            save,
            tab,
        } => {
            // Determine content source
            if let Some(img_path) = image {
                // Copy image
                if !img_path.exists() {
                    eprintln!("Error: Image file not found: {}", img_path.display());
                    process::exit(1);
                }

                match copy_image_to_clipboard(&img_path) {
                    Ok(_) => {
                        eprintln!("✅ Image copied to clipboard");

                        // Also save to history if requested
                        if save {
                            // Read image as base64
                            use std::io::Read;
                            match std::fs::File::open(&img_path) {
                                Ok(mut img_file) => {
                                    let mut img_data = Vec::new();
                                    if let Err(e) = img_file.read_to_end(&mut img_data) {
                                        eprintln!("Warning: Failed to read image: {}", e);
                                    } else {
                                        let base64_content = format!(
                                            "data:image/png;base64,{}",
                                            BASE64.encode(&img_data)
                                        );

                                        match save_to_history(&pool, &base64_content, "image", tab)
                                            .await
                                        {
                                            Ok(id) => eprintln!("✅ Saved to history (ID: {})", id),
                                            Err(e) => eprintln!(
                                                "Warning: Failed to save to history: {}",
                                                e
                                            ),
                                        }
                                    }
                                }
                                Err(e) => eprintln!("Warning: Failed to open image: {}", e),
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: Failed to copy image: {}", e);
                        process::exit(1);
                    }
                }
            } else if let Some(file_path) = file {
                // Copy file content
                if !file_path.exists() {
                    eprintln!("Error: File not found: {}", file_path.display());
                    process::exit(1);
                }

                match std::fs::read_to_string(&file_path) {
                    Ok(content) => {
                        match copy_to_clipboard(&content) {
                            Ok(_) => eprintln!("✅ File content copied to clipboard"),
                            Err(e) => {
                                eprintln!("Error: Failed to copy: {}", e);
                                process::exit(1);
                            }
                        }

                        // Also save to history if requested
                        if save {
                            match save_to_history(&pool, &content, "text", tab).await {
                                Ok(id) => eprintln!("✅ Saved to history (ID: {})", id),
                                Err(e) => eprintln!("Warning: Failed to save to history: {}", e),
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: Failed to read file: {}", e);
                        process::exit(1);
                    }
                }
            } else if let Some(text_content) = text {
                // Copy text
                match copy_to_clipboard(&text_content) {
                    Ok(_) => eprintln!("✅ Text copied to clipboard"),
                    Err(e) => {
                        eprintln!("Error: Failed to copy: {}", e);
                        process::exit(1);
                    }
                }

                // Also save to history if requested
                if save {
                    match save_to_history(&pool, &text_content, "text", tab).await {
                        Ok(id) => eprintln!("✅ Saved to history (ID: {})", id),
                        Err(e) => eprintln!("Warning: Failed to save to history: {}", e),
                    }
                }
            } else {
                eprintln!("Error: Please provide text, --file, or --image");
                process::exit(1);
            }
        }

        Commands::Save {
            text,
            file,
            image,
            copy,
            tab,
        } => {
            // Determine content source
            if let Some(img_path) = image {
                // Save image
                if !img_path.exists() {
                    eprintln!("Error: Image file not found: {}", img_path.display());
                    process::exit(1);
                }

                // Read image as base64
                use std::io::Read;
                match std::fs::File::open(&img_path) {
                    Ok(mut img_file) => {
                        let mut img_data = Vec::new();
                        if let Err(e) = img_file.read_to_end(&mut img_data) {
                            eprintln!("Error: Failed to read image: {}", e);
                            process::exit(1);
                        }

                        let base64_content =
                            format!("data:image/png;base64,{}", BASE64.encode(&img_data));

                        match save_to_history(&pool, &base64_content, "image", tab).await {
                            Ok(id) => {
                                eprintln!("✅ Image saved to history (ID: {})", id);

                                // Also copy to clipboard if requested
                                if copy {
                                    match copy_image_to_clipboard(&img_path) {
                                        Ok(_) => eprintln!("✅ Image copied to clipboard"),
                                        Err(e) => {
                                            eprintln!("Warning: Failed to copy image: {}", e)
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Error: Failed to save: {}", e);
                                process::exit(1);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: Failed to open image: {}", e);
                        process::exit(1);
                    }
                }
            } else if let Some(file_path) = file {
                // Save file content
                if !file_path.exists() {
                    eprintln!("Error: File not found: {}", file_path.display());
                    process::exit(1);
                }

                match std::fs::read_to_string(&file_path) {
                    Ok(content) => {
                        match save_to_history(&pool, &content, "text", tab).await {
                            Ok(id) => {
                                eprintln!("✅ File content saved to history (ID: {})", id);

                                // Also copy to clipboard if requested
                                if copy {
                                    match copy_to_clipboard(&content) {
                                        Ok(_) => eprintln!("✅ Content copied to clipboard"),
                                        Err(e) => {
                                            eprintln!("Warning: Failed to copy: {}", e)
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Error: Failed to save: {}", e);
                                process::exit(1);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: Failed to read file: {}", e);
                        process::exit(1);
                    }
                }
            } else if let Some(text_content) = text {
                // Save text
                match save_to_history(&pool, &text_content, "text", tab).await {
                    Ok(id) => {
                        eprintln!("✅ Text saved to history (ID: {})", id);

                        // Also copy to clipboard if requested
                        if copy {
                            match copy_to_clipboard(&text_content) {
                                Ok(_) => eprintln!("✅ Text copied to clipboard"),
                                Err(e) => eprintln!("Warning: Failed to copy: {}", e),
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: Failed to save: {}", e);
                        process::exit(1);
                    }
                }
            } else {
                eprintln!("Error: Please provide text, --file, or --image");
                process::exit(1);
            }
        }
    }
}
