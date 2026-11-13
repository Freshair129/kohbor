use std::collections::HashSet;

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    DEVICE_STATE, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eCommunications, eConsole,
    eMultimedia, eRender,
};
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PropVariantClear};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize, STGM_READ,
};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::core::PWSTR;

fn main() -> windows::core::Result<()> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    let result = enumerate();
    unsafe { CoUninitialize() };
    result
}

fn enumerate() -> windows::core::Result<()> {
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };

    let mut default_ids = HashSet::new();
    for role in [eConsole, eMultimedia, eCommunications] {
        if let Ok(device) = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, role) }
            && let Ok(id) = device_id(&device)
        {
            default_ids.insert(id);
        }
    }

    let all_states = DEVICE_STATE(0x0f);
    let devices = unsafe { enumerator.EnumAudioEndpoints(eRender, all_states)? };
    for index in 0..unsafe { devices.GetCount()? } {
        let device = unsafe { devices.Item(index)? };
        let id = device_id(&device)?;
        let name = friendly_name(&device)?;
        let state = unsafe { device.GetState()? }.0;
        let state_name = match state {
            1 => "Active",
            2 => "Disabled",
            4 => "NotPresent",
            8 => "Unplugged",
            _ => "Unknown",
        };
        println!(
            "name={name} id={id} default={} state={state_name}",
            default_ids.contains(&id)
        );
    }
    Ok(())
}

fn device_id(device: &IMMDevice) -> windows::core::Result<String> {
    let id = unsafe { device.GetId()? };
    let text = unsafe { id.to_string() }?;
    unsafe { CoTaskMemFree(Some(id.0.cast())) };
    Ok(text)
}

fn friendly_name(device: &IMMDevice) -> windows::core::Result<String> {
    let store: IPropertyStore = unsafe { device.OpenPropertyStore(STGM_READ)? };
    let mut value: PROPVARIANT = unsafe { store.GetValue(&PKEY_Device_FriendlyName)? };
    let name = unsafe { PWSTR(value.Anonymous.Anonymous.Anonymous.pwszVal.0).to_string() };
    unsafe { PropVariantClear(&mut value)? };
    Ok(name?)
}
