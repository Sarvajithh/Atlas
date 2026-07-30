//! `settings.*` namespace (§43.1): settings.get, settings.set.

use tauri::State;

use atlas_core::AppFacade;
use atlas_types::settings::SettingEntry;
use atlas_utils::AppError;

#[tauri::command]
pub fn settings_get(
    facade: State<'_, AppFacade>,
    key: String,
) -> Result<Option<SettingEntry>, AppError> {
    facade.settings().get_global(&key)
}

#[tauri::command]
pub fn settings_set(facade: State<'_, AppFacade>, entry: SettingEntry) -> Result<(), AppError> {
    facade.settings().set(entry)
}
