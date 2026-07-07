use crate::sync::models::SyncProviderKind;

pub(super) fn provider_kind_from_db(provider: &str) -> SyncProviderKind {
    match provider {
        "webdav" => SyncProviderKind::WebDav,
        "sftp" => SyncProviderKind::Sftp,
        "google_drive" => SyncProviderKind::GoogleDrive,
        "one_drive" => SyncProviderKind::OneDrive,
        _ => SyncProviderKind::Sftp,
    }
}

pub(super) fn provider_kind_to_db(provider: &SyncProviderKind) -> &'static str {
    match provider {
        SyncProviderKind::WebDav => "webdav",
        SyncProviderKind::Sftp => "sftp",
        SyncProviderKind::GoogleDrive => "google_drive",
        SyncProviderKind::OneDrive => "one_drive",
    }
}
