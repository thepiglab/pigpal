//! Read device metadata without instantiating an AudioUnit for every device.
//! CPAL's input_devices() probes configurations, which can hang on virtual drivers.
use anyhow::{ensure, Result};
use core_foundation::{
    base::TCFType,
    string::{CFString, CFStringRef},
};
use coreaudio_sys::*;
use std::{mem::size_of, ptr};

pub fn input_names() -> Result<Vec<String>> {
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioHardwarePropertyDevices,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut bytes = 0;
    // SAFETY: each CoreAudio call receives a valid property address and writable
    // buffer of the reported size. Device names follow CoreAudio's CF create rule.
    unsafe {
        ensure!(
            AudioObjectGetPropertyDataSize(
                kAudioObjectSystemObject,
                &address,
                0,
                ptr::null(),
                &mut bytes
            ) == 0,
            "Cannot list CoreAudio devices"
        );
        ensure!(
            bytes as usize % size_of::<AudioDeviceID>() == 0,
            "Invalid CoreAudio device list size"
        );
        let mut ids = vec![0_u32; bytes as usize / size_of::<AudioDeviceID>()];
        ensure!(
            AudioObjectGetPropertyData(
                kAudioObjectSystemObject,
                &address,
                0,
                ptr::null(),
                &mut bytes,
                ids.as_mut_ptr().cast()
            ) == 0,
            "Cannot read CoreAudio devices"
        );
        ids.truncate(bytes as usize / size_of::<AudioDeviceID>());
        let mut names = vec![];
        for id in ids {
            let streams = AudioObjectPropertyAddress {
                mSelector: kAudioDevicePropertyStreams,
                mScope: kAudioObjectPropertyScopeInput,
                mElement: kAudioObjectPropertyElementMain,
            };
            let mut size = 0;
            if AudioObjectGetPropertyDataSize(id, &streams, 0, ptr::null(), &mut size) != 0
                || size == 0
            {
                continue;
            }
            let name_address = AudioObjectPropertyAddress {
                mSelector: kAudioObjectPropertyName,
                mScope: kAudioObjectPropertyScopeGlobal,
                mElement: kAudioObjectPropertyElementMain,
            };
            let mut name: CFStringRef = ptr::null();
            let mut size = size_of::<CFStringRef>() as u32;
            if AudioObjectGetPropertyData(
                id,
                &name_address,
                0,
                ptr::null(),
                &mut size,
                (&mut name as *mut CFStringRef).cast(),
            ) == 0
                && !name.is_null()
            {
                names.push(CFString::wrap_under_create_rule(name).to_string());
            }
        }
        names.sort();
        names.dedup();
        Ok(names)
    }
}
