#![cfg_attr(not(target_os = "espidf"), no_std)]
#![cfg_attr(target_os = "espidf", allow(unsafe_code))]

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResourceSnapshot {
    pub free_heap_bytes: usize,
    pub minimum_free_heap_bytes: usize,
    pub largest_free_block_bytes: usize,
    pub current_task_stack_high_water_bytes: usize,
}

#[cfg(target_os = "espidf")]
#[derive(Debug)]
pub struct PthreadConfigGuard {
    previous: esp_idf_sys::esp_pthread_cfg_t,
}

#[cfg(target_os = "espidf")]
impl PthreadConfigGuard {
    /// Configures the next Rust pthread, then restores the creating thread's
    /// previous ESP-IDF configuration when this guard is dropped.
    pub fn for_next_thread(priority: usize, core: i32, external_stack: bool) -> Result<Self, i32> {
        // SAFETY: the returned structure owns no borrowed data and is copied
        // before it is used to configure the current pthread creator.
        let mut previous = unsafe { esp_idf_sys::esp_pthread_get_default_config() };
        // SAFETY: `previous` is writable for the complete call. NOT_FOUND is
        // expected when the creator still uses the default configuration.
        let _ = unsafe { esp_idf_sys::esp_pthread_get_cfg(&mut previous) };
        let mut next = previous;
        next.prio = priority;
        next.inherit_cfg = false;
        next.pin_to_core = core;
        next.stack_alloc_caps = esp_idf_sys::MALLOC_CAP_8BIT
            | if external_stack {
                esp_idf_sys::MALLOC_CAP_SPIRAM
            } else {
                esp_idf_sys::MALLOC_CAP_INTERNAL
            };
        // SAFETY: the complete configuration is valid for this synchronous
        // call and explicitly requests byte-addressable stack memory.
        let result = unsafe { esp_idf_sys::esp_pthread_set_cfg(&next) };
        if result != esp_idf_sys::ESP_OK {
            return Err(result);
        }
        Ok(Self { previous })
    }
}

#[cfg(target_os = "espidf")]
impl Drop for PthreadConfigGuard {
    fn drop(&mut self) {
        // SAFETY: this restores the configuration copied from the same
        // creating thread before the guarded pthread was created.
        let _ = unsafe { esp_idf_sys::esp_pthread_set_cfg(&self.previous) };
    }
}

#[cfg(target_os = "espidf")]
#[must_use]
pub fn snapshot() -> ResourceSnapshot {
    // SAFETY: these ESP-IDF diagnostics are read-only, take no borrowed
    // pointers, and may be called from a normal FreeRTOS task.
    let free_heap_bytes = unsafe { esp_idf_sys::esp_get_free_heap_size() } as usize;
    // SAFETY: read-only process-wide heap statistic.
    let minimum_free_heap_bytes = unsafe { esp_idf_sys::esp_get_minimum_free_heap_size() } as usize;
    // SAFETY: MALLOC_CAP_8BIT requests the normal byte-addressable heap and
    // the call only reads allocator metadata.
    let largest_free_block_bytes =
        unsafe { esp_idf_sys::heap_caps_get_largest_free_block(esp_idf_sys::MALLOC_CAP_8BIT) };
    ResourceSnapshot {
        free_heap_bytes,
        minimum_free_heap_bytes,
        largest_free_block_bytes,
        current_task_stack_high_water_bytes: current_task_stack_high_water_bytes(),
    }
}

#[cfg(target_os = "espidf")]
#[must_use]
pub fn current_task_stack_high_water_bytes() -> usize {
    // SAFETY: a null task handle is the documented FreeRTOS spelling for the
    // calling task; the call only reads its stack watermark.
    (unsafe { esp_idf_sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut()) }) as usize
}

#[cfg(not(target_os = "espidf"))]
#[must_use]
pub const fn snapshot() -> ResourceSnapshot {
    ResourceSnapshot {
        free_heap_bytes: 0,
        minimum_free_heap_bytes: 0,
        largest_free_block_bytes: 0,
        current_task_stack_high_water_bytes: 0,
    }
}

#[cfg(not(target_os = "espidf"))]
#[must_use]
pub const fn current_task_stack_high_water_bytes() -> usize {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_snapshot_is_an_explicit_unavailable_sentinel() {
        assert_eq!(snapshot(), ResourceSnapshot::default());
        assert_eq!(current_task_stack_high_water_bytes(), 0);
    }
}
