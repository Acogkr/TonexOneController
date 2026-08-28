#![no_std]

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ParameterId(u16);

impl ParameterId {
    const fn from_generated(value: u16) -> Self {
        Self(value)
    }

    /// Creates an ID present in the source-derived TONEX ONE registry.
    ///
    /// # Errors
    ///
    /// Rejects the `TONEX_PARAM_LAST` sentinel and values at or above
    /// `TONEX_GLOBAL_LAST`.
    pub const fn new(value: u16) -> Result<Self, ParameterError> {
        if value < PARAMETER_COUNT || (value >= GLOBAL_FIRST && value < GLOBAL_END) {
            Ok(Self(value))
        } else {
            Err(ParameterError::InvalidId(value))
        }
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }

    #[must_use]
    pub const fn is_global(self) -> bool {
        self.0 >= GLOBAL_FIRST
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParameterKind {
    Switch,
    Select,
    Range,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParameterSpec {
    pub id: ParameterId,
    pub label: &'static str,
    pub kind: ParameterKind,
    pub default: f32,
    pub minimum: f32,
    pub maximum: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParameterValue {
    id: ParameterId,
    value: f32,
}

impl ParameterValue {
    #[must_use]
    pub const fn id(self) -> ParameterId {
        self.id
    }

    #[must_use]
    pub const fn value(self) -> f32 {
        self.value
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParameterError {
    InvalidId(u16),
    NonFinite,
    OutOfRange {
        id: ParameterId,
        value: f32,
        minimum: f32,
        maximum: f32,
    },
    NonIntegral {
        id: ParameterId,
        value: f32,
    },
}

include!("generated.rs");

pub const PRESET_PARAMETER_COUNT: usize = PARAMETER_COUNT as usize;
pub const STORED_PARAMETER_COUNT: usize = SPECS.len();

#[must_use]
pub const fn all_specs() -> &'static [ParameterSpec] {
    &SPECS
}

const fn storage_index(id: ParameterId) -> usize {
    let raw = id.get() as usize;
    if raw < PARAMETER_COUNT as usize {
        raw
    } else {
        raw - 1
    }
}

#[must_use]
pub const fn spec(id: ParameterId) -> &'static ParameterSpec {
    &SPECS[storage_index(id)]
}

/// Validates a value against its source-derived type and bounds.
///
/// # Errors
///
/// Rejects non-finite numbers, out-of-range values, and fractional switch or
/// select values.
pub fn validate(id: ParameterId, value: f32) -> Result<ParameterValue, ParameterError> {
    let definition = spec(id);
    if !value.is_finite() {
        return Err(ParameterError::NonFinite);
    }
    if value < definition.minimum || value > definition.maximum {
        return Err(ParameterError::OutOfRange {
            id,
            value,
            minimum: definition.minimum,
            maximum: definition.maximum,
        });
    }
    if definition.kind != ParameterKind::Range && value % 1.0 != 0.0 {
        return Err(ParameterError::NonIntegral { id, value });
    }
    Ok(ParameterValue { id, value })
}

/// Scales MIDI `0..=127` linearly to a range parameter, matching the C helper.
///
/// Select and switch CCs have controller-specific semantics and should not use
/// this function.
#[must_use]
pub fn scale_midi_range(id: ParameterId, midi_value: u8) -> f32 {
    let definition = spec(id);
    definition.minimum + (f32::from(midi_value) / 127.0) * (definition.maximum - definition.minimum)
}

/// Fixed-capacity, allocation-free cache of all preset and global values.
#[derive(Clone, Debug, PartialEq)]
pub struct ParameterStore {
    values: [f32; SPECS.len()],
}

impl ParameterStore {
    #[must_use]
    pub fn with_defaults() -> Self {
        let mut values = [0.0; SPECS.len()];
        for (value, definition) in values.iter_mut().zip(SPECS.iter()) {
            *value = definition.default;
        }
        Self { values }
    }

    #[must_use]
    pub const fn get(&self, id: ParameterId) -> f32 {
        self.values[storage_index(id)]
    }

    #[must_use]
    pub const fn values(&self) -> &[f32; STORED_PARAMETER_COUNT] {
        &self.values
    }

    /// Validates and stores a parameter atomically.
    ///
    /// # Errors
    ///
    /// Leaves the previous value unchanged when validation fails.
    pub fn set(&mut self, id: ParameterId, value: f32) -> Result<ParameterValue, ParameterError> {
        let checked = validate(id, value)?;
        self.values[storage_index(id)] = checked.value();
        Ok(checked)
    }

    /// Replaces all preset-scoped values after validating the complete block.
    ///
    /// # Errors
    ///
    /// Leaves the store unchanged if any value is invalid.
    pub fn set_preset_values(
        &mut self,
        values: &[f32; PRESET_PARAMETER_COUNT],
    ) -> Result<(), ParameterError> {
        for (definition, value) in SPECS[..PRESET_PARAMETER_COUNT].iter().zip(values) {
            validate(definition.id, *value)?;
        }
        self.values[..PRESET_PARAMETER_COUNT].copy_from_slice(values);
        Ok(())
    }

    /// Applies a preset block observed from the physical TONEX ONE.
    ///
    /// Device observations are not commands: firmware revisions can expose
    /// sentinel or temporarily out-of-range values. Valid entries are kept,
    /// invalid entries leave their previous cached value untouched, and the
    /// number of skipped entries is returned. Outbound writes continue to use
    /// [`Self::set`] and remain strictly validated.
    pub fn observe_preset_values(&mut self, values: &[f32; PRESET_PARAMETER_COUNT]) -> usize {
        let mut skipped = 0;
        for (index, (definition, value)) in SPECS[..PRESET_PARAMETER_COUNT]
            .iter()
            .zip(values)
            .enumerate()
        {
            if validate(definition.id, *value).is_ok() {
                self.values[index] = *value;
            } else {
                skipped += 1;
            }
        }
        skipped
    }

    pub fn reset(&mut self) {
        *self = Self::with_defaults();
    }
}

impl Default for ParameterStore {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_covers_every_value_except_the_enum_sentinel() {
        assert_eq!(SPECS.len(), usize::from(GLOBAL_END - 1));
        for (index, definition) in SPECS.iter().enumerate() {
            let expected = if index < usize::from(PARAMETER_COUNT) {
                index
            } else {
                index + 1
            };
            assert_eq!(usize::from(definition.id.get()), expected);
            assert!(definition.minimum <= definition.default);
            assert!(definition.default <= definition.maximum);
        }
    }

    #[test]
    fn id_rejects_both_markers_and_out_of_range_values() {
        assert_eq!(
            ParameterId::new(PARAMETER_COUNT),
            Err(ParameterError::InvalidId(PARAMETER_COUNT))
        );
        assert_eq!(
            ParameterId::new(GLOBAL_END),
            Err(ParameterError::InvalidId(GLOBAL_END))
        );
    }

    #[test]
    fn range_and_discrete_validation_are_typed() {
        assert!(validate(TONEX_PARAM_EQ_BASS, 5.5).is_ok());
        assert_eq!(
            validate(TONEX_PARAM_EQ_BASS, 11.0),
            Err(ParameterError::OutOfRange {
                id: TONEX_PARAM_EQ_BASS,
                value: 11.0,
                minimum: 0.0,
                maximum: 10.0,
            })
        );
        assert_eq!(
            validate(TONEX_PARAM_DELAY_ENABLE, 0.5),
            Err(ParameterError::NonIntegral {
                id: TONEX_PARAM_DELAY_ENABLE,
                value: 0.5,
            })
        );
    }

    #[test]
    fn midi_range_scaling_matches_reference_endpoints() {
        assert!((scale_midi_range(TONEX_GLOBAL_BPM, 0) - 40.0).abs() < f32::EPSILON);
        assert!((scale_midi_range(TONEX_GLOBAL_BPM, 127) - 240.0).abs() < f32::EPSILON);
    }

    #[test]
    fn fixed_store_starts_from_defaults_and_rejects_partial_updates() {
        let mut store = ParameterStore::default();
        assert!(
            (store.get(TONEX_PARAM_EQ_BASS) - spec(TONEX_PARAM_EQ_BASS).default).abs()
                < f32::EPSILON
        );
        store
            .set(TONEX_PARAM_EQ_BASS, 7.5)
            .expect("range value is valid");
        assert!((store.get(TONEX_PARAM_EQ_BASS) - 7.5).abs() < f32::EPSILON);
        assert!(store.set(TONEX_PARAM_EQ_BASS, 11.0).is_err());
        assert!((store.get(TONEX_PARAM_EQ_BASS) - 7.5).abs() < f32::EPSILON);
    }

    #[test]
    fn store_maps_global_ids_across_the_sentinel_gap() {
        let mut store = ParameterStore::default();
        store
            .set(TONEX_GLOBAL_BPM, 96.0)
            .expect("global value is valid");
        assert!((store.get(TONEX_GLOBAL_BPM) - 96.0).abs() < f32::EPSILON);
        store.reset();
        assert!(
            (store.get(TONEX_GLOBAL_BPM) - spec(TONEX_GLOBAL_BPM).default).abs() < f32::EPSILON
        );
    }

    #[test]
    fn preset_block_update_is_atomic() {
        let mut store = ParameterStore::default();
        let original = store.clone();
        let mut values = [0.0; PRESET_PARAMETER_COUNT];
        for (value, definition) in values.iter_mut().zip(SPECS.iter()) {
            *value = definition.default;
        }
        values[usize::from(TONEX_PARAM_EQ_BASS.get())] = 11.0;
        assert!(store.set_preset_values(&values).is_err());
        assert_eq!(store, original);
    }

    #[test]
    fn observed_preset_block_keeps_valid_values_and_skips_device_sentinels() {
        let mut store = ParameterStore::default();
        let mut values = [0.0; PRESET_PARAMETER_COUNT];
        for (value, definition) in values.iter_mut().zip(SPECS.iter()) {
            *value = definition.default;
        }
        values[usize::from(TONEX_PARAM_EQ_BASS.get())] = 11.0;
        values[usize::from(TONEX_PARAM_EQ_MID.get())] = 7.5;

        assert_eq!(store.observe_preset_values(&values), 1);
        assert!(
            (store.get(TONEX_PARAM_EQ_BASS) - spec(TONEX_PARAM_EQ_BASS).default).abs()
                < f32::EPSILON
        );
        assert!((store.get(TONEX_PARAM_EQ_MID) - 7.5).abs() < f32::EPSILON);
    }
}
