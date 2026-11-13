// Provenance: ported from owner-authored G-Maiden 0.13.6 src-tauri/src/dxgi.rs, owner consent recorded in claim vision-risk__mvp1-capture-native.

use std::{collections::HashSet, marker::PhantomData, rc::Rc, time::Instant};

use kohbor_capture::Rect;
use windows::{
    core::{Error, Interface, BOOL},
    Win32::{
        Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE, HWND, LPARAM, RECT},
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0},
            Direct3D11::{
                D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_BOX,
                D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
                D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
            },
            Dxgi::Common::{
                DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_MODE_ROTATION_IDENTITY, DXGI_SAMPLE_DESC,
            },
            Dxgi::{
                CreateDXGIFactory1, IDXGIAdapter, IDXGIAdapter1, IDXGIAdapter3, IDXGIFactory1,
                IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource, DXGI_ERROR_ACCESS_LOST,
                DXGI_ERROR_WAIT_TIMEOUT, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_OUTDUPL_DESC,
            },
            Gdi::{MonitorFromWindow, HMONITOR, MONITOR_DEFAULTTONEAREST},
        },
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
        },
    },
};

// Windows 10 19045 and Windows 11 26100 both export this API. The selected
// feature set does not expose its Win32 wrapper, so declare the read-only call.
#[link(name = "user32")]
extern "system" {
    fn GetDpiForWindow(hwnd: HWND) -> u32;
}

#[derive(Clone, Copy)]
pub(crate) struct GameWindow {
    hwnd: HWND,
    monitor: HMONITOR,
    rect: Rect,
}

pub(crate) fn find_game_window() -> Result<GameWindow, crate::OpenError> {
    let process_ids = dota_process_ids()?;
    if process_ids.is_empty() {
        return Err(crate::OpenError::NoGameWindow);
    }

    let mut scan = WindowScan {
        process_ids,
        best_area: 0,
        best: None,
    };
    // SAFETY: EnumWindows is synchronous. The callback receives this live
    // stack-owned scan through LPARAM and never retains it after the call.
    unsafe {
        EnumWindows(
            Some(enum_windows_proc),
            LPARAM((&mut scan as *mut WindowScan) as isize),
        )
        .map_err(|error| crate::OpenError::Os(format!("EnumWindows failed: {error}")))?;
    }

    let Some((hwnd, rect)) = scan.best else {
        return Err(crate::OpenError::NoGameWindow);
    };
    // SAFETY: hwnd is a live top-level window returned by EnumWindows.
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if monitor == HMONITOR::default() {
        return Err(crate::OpenError::Unsupported(
            "Dota window has no display monitor".to_owned(),
        ));
    }

    Ok(GameWindow {
        hwnd,
        monitor,
        rect,
    })
}

pub(crate) fn game_window() -> Option<Rect> {
    find_game_window().ok().map(|window| window.rect)
}

struct WindowScan {
    process_ids: HashSet<u32>,
    best_area: u64,
    best: Option<(HWND, Rect)>,
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam is the exact mutable WindowScan pointer passed by
    // find_game_window; EnumWindows calls back synchronously on this thread.
    let scan = unsafe { &mut *(lparam.0 as *mut WindowScan) };
    let mut process_id = 0;
    // SAFETY: hwnd is supplied by EnumWindows, and process_id is a local out-param.
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
    }
    if !scan.process_ids.contains(&process_id) {
        return BOOL(1);
    }
    // Ignore hidden/minimized windows; they cannot identify the visible Dota output.
    if !unsafe { IsWindowVisible(hwnd).as_bool() } || unsafe { IsIconic(hwnd).as_bool() } {
        return BOOL(1);
    }

    let mut window_rect = RECT::default();
    // SAFETY: hwnd came from EnumWindows and window_rect is caller-owned storage.
    if unsafe { GetWindowRect(hwnd, &mut window_rect) }.is_err() {
        return BOOL(1);
    }

    let width = i64::from(window_rect.right) - i64::from(window_rect.left);
    let height = i64::from(window_rect.bottom) - i64::from(window_rect.top);
    let (Ok(width), Ok(height)) = (u32::try_from(width), u32::try_from(height)) else {
        return BOOL(1);
    };
    if width == 0 || height == 0 {
        return BOOL(1);
    }
    let Some(area) = u64::from(width).checked_mul(u64::from(height)) else {
        return BOOL(1);
    };
    if area > scan.best_area {
        scan.best_area = area;
        scan.best = Some((
            hwnd,
            Rect {
                x: window_rect.left,
                y: window_rect.top,
                width,
                height,
            },
        ));
    }

    BOOL(1)
}

struct SnapshotGuard(HANDLE);

impl Drop for SnapshotGuard {
    fn drop(&mut self) {
        // SAFETY: this guard owns the valid snapshot handle returned by
        // CreateToolhelp32Snapshot and closes it exactly once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn dota_process_ids() -> Result<HashSet<u32>, crate::OpenError> {
    // SAFETY: snapshot creation has no pointer arguments; the returned handle
    // is kept alive by SnapshotGuard while Toolhelp enumerates it.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map_err(|error| crate::OpenError::Os(format!("process snapshot failed: {error}")))?;
    let _snapshot_guard = SnapshotGuard(snapshot);
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut process_ids = HashSet::new();

    // SAFETY: entry is initialized with the size required by Toolhelp and is
    // valid writable storage for both enumeration functions.
    match unsafe { Process32FirstW(snapshot, &mut entry) } {
        Ok(()) => loop {
            let end = entry
                .szExeFile
                .iter()
                .position(|character| *character == 0)
                .unwrap_or(entry.szExeFile.len());
            let image_name = String::from_utf16_lossy(&entry.szExeFile[..end]);
            if image_name.eq_ignore_ascii_case("dota2.exe") {
                process_ids.insert(entry.th32ProcessID);
            }

            // SAFETY: reuses the same initialized, writable process entry.
            match unsafe { Process32NextW(snapshot, &mut entry) } {
                Ok(()) => {}
                Err(error) if is_no_more_files(&error) => break,
                Err(error) => {
                    return Err(crate::OpenError::Os(format!(
                        "process enumeration failed: {error}"
                    )));
                }
            }
        },
        Err(error) if is_no_more_files(&error) => {}
        Err(error) => {
            return Err(crate::OpenError::Os(format!(
                "process enumeration failed: {error}"
            )));
        }
    }

    Ok(process_ids)
}

fn is_no_more_files(error: &Error) -> bool {
    (error.code().0 as u32 & 0xffff) == ERROR_NO_MORE_FILES.0
}

pub(crate) enum NativeGrab {
    Frame { acquired_us: u64 },
    Timeout,
    AccessLost,
}

pub(crate) struct NativeCapture {
    adapter: IDXGIAdapter1,
    output: IDXGIOutput1,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: Option<IDXGIOutputDuplication>,
    recreate_pending: bool,
    staging: Option<(u32, u32, ID3D11Texture2D)>,
    output_info: crate::OutputInfo,
    opened_at: Instant,
    _thread_bound: PhantomData<Rc<()>>,
}

impl NativeCapture {
    pub(crate) fn open(game: GameWindow) -> Result<(Self, crate::OutputInfo), crate::OpenError> {
        // SAFETY: the factory is a COM interface returned by DXGI; all COM
        // calls are checked before their results are used.
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }
            .map_err(|error| crate::OpenError::Os(format!("DXGI factory failed: {error}")))?;

        let mut adapter_index = 0;
        loop {
            // SAFETY: factory is live and adapter_index is an output index.
            let adapter = match unsafe { factory.EnumAdapters1(adapter_index) } {
                Ok(adapter) => adapter,
                Err(_) => break,
            };
            let mut output_index = 0;
            loop {
                // SAFETY: adapter is live and output_index is an output index.
                let output = match unsafe { adapter.EnumOutputs(output_index) } {
                    Ok(output) => output,
                    Err(_) => break,
                };
                // SAFETY: output is a live interface returned by EnumOutputs.
                let description = unsafe { output.GetDesc() }.map_err(|error| {
                    crate::OpenError::Os(format!("DXGI output description failed: {error}"))
                })?;
                if description.Monitor == game.monitor {
                    return Self::open_matching_output(adapter, output, description, game);
                }
                output_index += 1;
            }
            adapter_index += 1;
        }

        Err(crate::OpenError::Unsupported(
            "no DXGI output on an adapter matches the Dota window monitor".to_owned(),
        ))
    }

    fn open_matching_output(
        adapter: IDXGIAdapter1,
        output: windows::Win32::Graphics::Dxgi::IDXGIOutput,
        output_description: windows::Win32::Graphics::Dxgi::DXGI_OUTPUT_DESC,
        game: GameWindow,
    ) -> Result<(Self, crate::OutputInfo), crate::OpenError> {
        if !output_description.AttachedToDesktop.as_bool() {
            return Err(crate::OpenError::Unsupported(
                "the Dota monitor is not attached to the desktop".to_owned(),
            ));
        }
        if output_description.Rotation != DXGI_MODE_ROTATION_IDENTITY {
            return Err(crate::OpenError::Unsupported(
                "rotated DXGI outputs are not supported by this crop profile".to_owned(),
            ));
        }

        let adapter_interface: IDXGIAdapter = adapter.cast().map_err(|error| {
            crate::OpenError::Unsupported(format!("DXGI adapter interface unavailable: {error}"))
        })?;
        let (device, context) =
            create_d3d11_device(&adapter_interface).map_err(crate::OpenError::Unsupported)?;
        let output1: IDXGIOutput1 = output.cast().map_err(|error| {
            crate::OpenError::Unsupported(format!("DXGI output duplication unavailable: {error}"))
        })?;
        let duplication =
            create_duplication(&output1, &device).map_err(crate::OpenError::Unsupported)?;
        // SAFETY: duplication is a live Desktop Duplication interface.
        let duplication_description = unsafe { duplication.GetDesc() };
        let mode = duplication_description.ModeDesc;
        if mode.Width == 0 || mode.Height == 0 {
            return Err(crate::OpenError::Unsupported(
                "DXGI reported an empty output mode".to_owned(),
            ));
        }
        if mode.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
            return Err(crate::OpenError::Unsupported(format!(
                "DXGI output format {:?} is not BGRA8",
                mode.Format
            )));
        }
        let desktop_width = i64::from(output_description.DesktopCoordinates.right)
            - i64::from(output_description.DesktopCoordinates.left);
        let desktop_height = i64::from(output_description.DesktopCoordinates.bottom)
            - i64::from(output_description.DesktopCoordinates.top);
        if desktop_width != i64::from(mode.Width) || desktop_height != i64::from(mode.Height) {
            return Err(crate::OpenError::Unsupported(
                "DXGI output geometry does not match its duplication mode".to_owned(),
            ));
        }

        // SAFETY: game.hwnd is still the enumerated top-level window. A zero
        // DPI means Windows could not resolve it and is treated as unknown.
        let dpi = unsafe { GetDpiForWindow(game.hwnd) };
        if dpi == 0 {
            return Err(crate::OpenError::Unsupported(
                "Windows did not report the Dota window DPI".to_owned(),
            ));
        }
        let output_id = display_fingerprint(&output_description.DeviceName).ok_or_else(|| {
            crate::OpenError::Unsupported("DXGI output has no device identity".to_owned())
        })?;
        let output_info = crate::OutputInfo {
            output_id,
            width: mode.Width,
            height: mode.Height,
            dpi,
        };

        let native = Self {
            adapter,
            output: output1,
            device,
            context,
            duplication: Some(duplication),
            recreate_pending: false,
            staging: None,
            output_info,
            opened_at: Instant::now(),
            _thread_bound: PhantomData,
        };
        Ok((native, output_info))
    }

    pub(crate) fn grab(
        &mut self,
        rect: Rect,
        layout: crate::capture_frame::CropLayout,
        timeout_ms: u32,
        out: &mut Vec<u8>,
    ) -> Result<NativeGrab, String> {
        if self.recreate_pending {
            self.recreate_duplication()?;
        }
        self.ensure_staging(rect.width, rect.height)?;
        let left = u32::try_from(rect.x).map_err(|_| "negative crop origin".to_owned())?;
        let top = u32::try_from(rect.y).map_err(|_| "negative crop origin".to_owned())?;
        let source_box = D3D11_BOX {
            left,
            top,
            front: 0,
            right: left
                .checked_add(rect.width)
                .ok_or_else(|| "crop right edge overflows".to_owned())?,
            bottom: top
                .checked_add(rect.height)
                .ok_or_else(|| "crop bottom edge overflows".to_owned())?,
            back: 1,
        };

        let mut frame_info = Default::default();
        let mut resource: Option<IDXGIResource> = None;
        let acquire_result = {
            let duplication = self
                .duplication
                .as_ref()
                .ok_or_else(|| "DXGI duplication is unavailable".to_owned())?;
            // SAFETY: the output parameters are local and live through the call.
            unsafe { duplication.AcquireNextFrame(timeout_ms, &mut frame_info, &mut resource) }
        };

        if let Err(error) = acquire_result {
            if error.code() == DXGI_ERROR_WAIT_TIMEOUT {
                return Ok(NativeGrab::Timeout);
            }
            if error.code() == DXGI_ERROR_ACCESS_LOST {
                self.duplication = None;
                self.recreate_pending = true;
                return Ok(NativeGrab::AccessLost);
            }
            return Err(format!("AcquireNextFrame failed: {error}"));
        }
        let acquired_us = u64::try_from(self.opened_at.elapsed().as_micros()).unwrap_or(u64::MAX);

        let duplication = self
            .duplication
            .as_ref()
            .ok_or_else(|| "DXGI duplication disappeared after acquire".to_owned())?;
        let Some(resource) = resource else {
            // SAFETY: a successful AcquireNextFrame always requires one release.
            let _ = unsafe { duplication.ReleaseFrame() };
            return Err("AcquireNextFrame succeeded without a desktop resource".to_owned());
        };
        // SAFETY: the acquired desktop resource is supplied by Desktop Duplication.
        let desktop: ID3D11Texture2D = match resource.cast() {
            Ok(texture) => texture,
            Err(error) => {
                // SAFETY: this acquire owns one release, including cast failure.
                let _ = unsafe { duplication.ReleaseFrame() };
                return Err(format!("desktop resource is not a texture: {error}"));
            }
        };

        let Some((_, _, staging)) = self.staging.as_ref() else {
            // SAFETY: successful acquisition requires one matching release.
            let _ = unsafe { duplication.ReleaseFrame() };
            return Err("staging texture is unavailable".to_owned());
        };
        // SAFETY: the rectangle was checked against the selected output; the
        // staging texture matches its crop size and both surfaces are BGRA8.
        unsafe {
            self.context
                .CopySubresourceRegion(staging, 0, 0, 0, 0, &desktop, 0, Some(&source_box));
        }
        // Release immediately after the GPU region copy, before mapping or row copies.
        // SAFETY: this acquire owns exactly one matching ReleaseFrame call.
        let release_result = unsafe { duplication.ReleaseFrame() };
        drop(resource);
        drop(desktop);
        if let Err(error) = release_result {
            if error.code() == DXGI_ERROR_ACCESS_LOST {
                self.duplication = None;
                self.recreate_pending = true;
                return Ok(NativeGrab::AccessLost);
            }
            return Err(format!("ReleaseFrame failed: {error}"));
        }

        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: staging is a CPU-readable staging texture and mapped remains
        // local until the paired Unmap below.
        let map_result = unsafe {
            self.context
                .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        };
        if let Err(error) = map_result {
            if error.code() == DXGI_ERROR_ACCESS_LOST {
                self.duplication = None;
                self.recreate_pending = true;
                return Ok(NativeGrab::AccessLost);
            }
            return Err(format!("Map staging texture failed: {error}"));
        }

        let copy_result = copy_mapped_rows(&mapped, layout, rect.height, out);
        // SAFETY: the resource was successfully mapped above and is unmapped once.
        unsafe { self.context.Unmap(staging, 0) };
        copy_result?;

        Ok(NativeGrab::Frame { acquired_us })
    }

    pub(crate) fn gpu_local_bytes(&self) -> Option<u64> {
        // SAFETY: adapter is the live adapter that owns this capture output.
        let adapter: IDXGIAdapter3 = self.adapter.cast().ok()?;
        let mut info = Default::default();
        // SAFETY: info is local writable storage for the video-memory result.
        unsafe {
            adapter
                .QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut info)
                .ok()?;
        }
        Some(info.CurrentUsage)
    }

    fn ensure_staging(&mut self, width: u32, height: u32) -> Result<(), String> {
        let size_matches =
            self.staging
                .as_ref()
                .is_some_and(|(current_width, current_height, _)| {
                    *current_width == width && *current_height == height
                });
        if !size_matches {
            let texture = create_staging_texture(&self.device, width, height)?;
            self.staging = Some((width, height, texture));
        }
        Ok(())
    }

    fn recreate_duplication(&mut self) -> Result<(), String> {
        let output = &self.output;
        let device = &self.device;
        let expected = self.output_info;
        crate::capture_recovery::replace_released(&mut self.duplication, || {
            let duplication = create_duplication(output, device)?;
            // SAFETY: duplication is live and GetDesc returns its current mode.
            let description = unsafe { duplication.GetDesc() };
            if description.ModeDesc.Width != expected.width
                || description.ModeDesc.Height != expected.height
                || description.ModeDesc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
                || description.Rotation != DXGI_MODE_ROTATION_IDENTITY
            {
                return Err("DXGI output changed to an unsupported mode".to_owned());
            }
            Ok(duplication)
        })?;
        self.recreate_pending = false;
        Ok(())
    }
}

fn create_d3d11_device(
    adapter: &IDXGIAdapter,
) -> Result<(ID3D11Device, ID3D11DeviceContext), String> {
    let mut device = None;
    let mut context = None;
    // SAFETY: device and context are local out-parameters; the selected adapter
    // is the one that owns the output and DRIVER_TYPE_UNKNOWN honors it.
    unsafe {
        D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            Default::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
    }
    .map_err(|error| format!("D3D11CreateDevice failed: {error}"))?;

    Ok((
        device.ok_or_else(|| "D3D11CreateDevice returned no device".to_owned())?,
        context.ok_or_else(|| "D3D11CreateDevice returned no context".to_owned())?,
    ))
}

fn create_duplication(
    output: &IDXGIOutput1,
    device: &ID3D11Device,
) -> Result<IDXGIOutputDuplication, String> {
    // SAFETY: output and device refer to the matched monitor and its adapter.
    let duplication = unsafe { output.DuplicateOutput(device) }
        .map_err(|error| format!("DuplicateOutput failed: {error}"))?;
    // SAFETY: duplication is live and GetDesc returns its current output mode.
    let description: DXGI_OUTDUPL_DESC = unsafe { duplication.GetDesc() };
    if description.ModeDesc.Width == 0 || description.ModeDesc.Height == 0 {
        return Err("DXGI duplication reported an empty mode".to_owned());
    }
    if description.ModeDesc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
        return Err("DXGI duplication format is not BGRA8".to_owned());
    }
    if description.Rotation != DXGI_MODE_ROTATION_IDENTITY {
        return Err("rotated DXGI duplication output is unsupported".to_owned());
    }
    Ok(duplication)
}

fn create_staging_texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> Result<ID3D11Texture2D, String> {
    let description = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };
    let mut texture = None;
    // SAFETY: description and the optional texture out-parameter are local and valid.
    unsafe { device.CreateTexture2D(&description, None, Some(&mut texture)) }
        .map_err(|error| format!("CreateTexture2D staging failed: {error}"))?;
    texture.ok_or_else(|| "CreateTexture2D returned no staging texture".to_owned())
}

fn copy_mapped_rows(
    mapped: &D3D11_MAPPED_SUBRESOURCE,
    layout: crate::capture_frame::CropLayout,
    height: u32,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    if mapped.pData.is_null() {
        return Err("Map returned a null pixel pointer".to_owned());
    }
    let row_pitch = usize::try_from(mapped.RowPitch)
        .map_err(|_| "Map row pitch does not fit this process".to_owned())?;
    if row_pitch < layout.row_bytes {
        return Err("Map row pitch is smaller than the crop row".to_owned());
    }
    let height = usize::try_from(height).map_err(|_| "crop height does not fit this process")?;
    row_pitch
        .checked_mul(height)
        .ok_or_else(|| "mapped crop row range overflows".to_owned())?;

    out.resize(layout.byte_len, 0);
    for row in 0..height {
        let source_offset = row
            .checked_mul(row_pitch)
            .ok_or_else(|| "mapped crop row offset overflows".to_owned())?;
        let destination_offset = row
            .checked_mul(layout.row_bytes)
            .ok_or_else(|| "output crop row offset overflows".to_owned())?;
        // SAFETY: Map returned a pointer to the region-sized staging texture;
        // row_pitch is at least row_bytes and this offset is within its height.
        let source = unsafe {
            std::slice::from_raw_parts(
                (mapped.pData as *const u8).add(source_offset),
                layout.row_bytes,
            )
        };
        out[destination_offset..destination_offset + layout.row_bytes].copy_from_slice(source);
    }
    Ok(())
}

fn display_fingerprint(device_name: &[u16; 32]) -> Option<u64> {
    let end = device_name.iter().position(|character| *character == 0)?;
    if end == 0 {
        return None;
    }
    Some(
        device_name[..end]
            .iter()
            .fold(14_695_981_039_346_656_037_u64, |hash, character| {
                (hash ^ u64::from(*character)).wrapping_mul(1_099_511_628_211)
            }),
    )
}
