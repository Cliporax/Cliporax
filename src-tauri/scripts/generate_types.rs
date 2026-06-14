// Generate TypeScript type definitions
use cliporax_lib::types::api::*;
use ts_rs::TS;

fn main() {
    println!("Generating TypeScript types...");

    // Export all types
    ItemType::export().unwrap();
    ClipboardItem::export().unwrap();
    ClipboardItemInput::export().unwrap();
    Tab::export().unwrap();
    ApiResult::<()>::export().unwrap();
    ApiError::export().unwrap();

    println!("TypeScript types generated successfully!");
}
