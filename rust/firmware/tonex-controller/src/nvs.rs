use esp_idf_svc::{
    nvs::{EspDefaultNvsPartition, EspKeyValueStorage, EspNvs},
    sys::EspError,
};
use hal_contracts::SettingsStore;

const NAMESPACE: &str = "tonex_one";
const SETTINGS_KEY: &str = "settings";

pub struct EspNvsSettingsStore {
    storage: EspKeyValueStorage<esp_idf_svc::nvs::NvsDefault>,
}

impl core::fmt::Debug for EspNvsSettingsStore {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("EspNvsSettingsStore")
            .finish_non_exhaustive()
    }
}

impl EspNvsSettingsStore {
    pub fn open() -> Result<Self, EspError> {
        let partition = EspDefaultNvsPartition::take()?;
        let nvs = EspNvs::new(partition, NAMESPACE, true)?;
        Ok(Self {
            storage: EspKeyValueStorage::new(nvs),
        })
    }
}

impl SettingsStore for EspNvsSettingsStore {
    type Error = EspError;

    fn load(&mut self, destination: &mut [u8]) -> Result<usize, Self::Error> {
        self.storage
            .get_raw(SETTINGS_KEY, destination)
            .map(|stored| stored.map_or(0, <[u8]>::len))
    }

    fn save(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        self.storage.set_raw(SETTINGS_KEY, data).map(|_| ())
    }
}
