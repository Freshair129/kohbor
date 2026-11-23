//! `tauri-plugin-updater` adapter. All configuration comes from compile-time update constants.

use super::{
    BACKEND_UNAVAILABLE, BAD_RESPONSE, BackendFuture, BackendResult, NETWORK, SIGNATURE,
    UPDATE_ENDPOINT, UPDATE_PUBKEY, UpdateBackend, UpdateInfo, UpdateProgress,
};
use std::sync::{Arc, Mutex, MutexGuard};
use tauri_plugin_updater::{Error as PluginError, Update, UpdaterExt};

pub(crate) struct PluginBackend {
    app: tauri::AppHandle<tauri::Wry>,
    pending: Mutex<Option<Arc<Update>>>,
    downloaded: Mutex<Option<(Arc<Update>, Vec<u8>)>>,
}

impl PluginBackend {
    pub(crate) fn new(app: tauri::AppHandle<tauri::Wry>) -> Self {
        Self {
            app,
            pending: Mutex::new(None),
            downloaded: Mutex::new(None),
        }
    }
}

impl UpdateBackend for PluginBackend {
    fn check(&self) -> BackendFuture<'_, BackendResult<Option<UpdateInfo>>> {
        Box::pin(async move {
            let endpoint = UPDATE_ENDPOINT.ok_or(super::NO_ENDPOINT)?;
            let pubkey = UPDATE_PUBKEY.ok_or(super::NO_PUBKEY)?;
            let endpoint = tauri::Url::parse(endpoint).map_err(|_| BAD_RESPONSE)?;
            let updater = self
                .app
                .updater_builder()
                .endpoints(vec![endpoint])
                .map_err(map_plugin_error)?
                .pubkey(pubkey)
                .build()
                .map_err(map_plugin_error)?;
            let update = updater.check().await.map_err(map_plugin_error)?;
            let Some(update) = update else {
                *lock(&self.pending) = None;
                *lock(&self.downloaded) = None;
                return Ok(None);
            };

            let info = UpdateInfo::new(
                update.version.clone(),
                update.body.clone().unwrap_or_default(),
                update.date.as_ref().map(|date| date.to_string()),
            );
            *lock(&self.downloaded) = None;
            *lock(&self.pending) = Some(Arc::new(update));
            Ok(Some(info))
        })
    }

    fn download<'a>(
        &'a self,
        info: &'a UpdateInfo,
        progress: UpdateProgress<'a>,
    ) -> BackendFuture<'a, BackendResult<()>> {
        Box::pin(async move {
            let update = lock(&self.pending)
                .as_ref()
                .filter(|update| update.version == info.version)
                .cloned()
                .ok_or(BACKEND_UNAVAILABLE)?;

            let progress_callback = progress.clone();
            let mut received = 0_u64;
            let bytes = update
                .download(
                    move |chunk_bytes, total_bytes| {
                        received = received.saturating_add(chunk_bytes as u64);
                        if let Some(total) = total_bytes.filter(|total| *total > 0) {
                            let percent = ((received.saturating_mul(100) / total).min(99)) as u8;
                            progress_callback(percent);
                        }
                    },
                    || {},
                )
                .await
                .map_err(map_plugin_error)?;

            // `Update::download` verifies the signature before returning any bytes.
            progress(100);
            *lock(&self.downloaded) = Some((update, bytes));
            Ok(())
        })
    }

    fn install(&self, info: &UpdateInfo) -> BackendResult<()> {
        let (update, bytes) = lock(&self.downloaded)
            .as_ref()
            .filter(|(update, _)| update.version == info.version)
            .map(|(update, bytes)| (update.clone(), bytes.clone()))
            .ok_or(BACKEND_UNAVAILABLE)?;
        update.install(bytes).map_err(map_plugin_error)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) fn map_plugin_error(error: PluginError) -> &'static str {
    match error {
        PluginError::Reqwest(_) | PluginError::Network(_) => NETWORK,
        PluginError::Minisign(_)
        | PluginError::Base64(_)
        | PluginError::SignatureUtf8(_)
        | PluginError::SignedVersionMismatch { .. }
        | PluginError::MissingSignedVersion => SIGNATURE,
        PluginError::Serialization(_)
        | PluginError::Semver(_)
        | PluginError::ReleaseNotFound
        | PluginError::TargetNotFound(_)
        | PluginError::TargetsNotFound(_)
        | PluginError::UrlParse(_)
        | PluginError::FormatDate => BAD_RESPONSE,
        _ => BACKEND_UNAVAILABLE,
    }
}

#[cfg(test)]
mod tests {
    use super::{PluginError, map_plugin_error};
    use crate::update::{BACKEND_UNAVAILABLE, BAD_RESPONSE, NETWORK, SIGNATURE};

    #[test]
    fn plugin_errors_map_to_safe_reason_codes() {
        assert_eq!(
            map_plugin_error(PluginError::Network("hidden detail".into())),
            NETWORK
        );
        assert_eq!(
            map_plugin_error(PluginError::SignatureUtf8("hidden signature".into())),
            SIGNATURE
        );
        assert_eq!(map_plugin_error(PluginError::ReleaseNotFound), BAD_RESPONSE);
        assert_eq!(map_plugin_error(PluginError::FormatDate), BAD_RESPONSE);
        assert_eq!(
            map_plugin_error(PluginError::EmptyEndpoints),
            BACKEND_UNAVAILABLE
        );
    }
}
