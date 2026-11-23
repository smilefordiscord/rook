//! Sony's pads on Windows, read over HID: Windows.Gaming.Input, gilrs's
//! only Windows backend, doesn't list a DualSense (measured), so Rook
//! reads them as SDL does, each pad's reports parsed by
//! [`super::sony_reports`]. A thread per pad blocks reading reports; a
//! device notification marks the pad list stale, so the list is read
//! again only after a device arrives. Another thread per pad writes its
//! motors' output report, on a handle of its own, so neither a frame nor
//! the reader waits on a write.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, PoisonError};

use rustc_hash::FxHashMap;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CM_Get_Device_Interface_List_SizeW,
    CM_Get_Device_Interface_ListW, CM_NOTIFY_ACTION, CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL,
    CM_NOTIFY_EVENT_DATA, CM_NOTIFY_FILTER, CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
    CM_Register_Notification, CM_Unregister_Notification, CR_BUFFER_SMALL, CR_SUCCESS,
    HCMNOTIFICATION,
};
use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HIDD_ATTRIBUTES, HIDP_CAPS, HIDP_STATUS_SUCCESS, HidD_FreePreparsedData, HidD_GetAttributes,
    HidD_GetFeature, HidD_GetHidGuid, HidD_GetPreparsedData, HidP_GetCaps,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadFile, WriteFile,
};
use windows_sys::Win32::System::IO::CancelIoEx;
use windows_sys::core::GUID;

use super::sony_reports::{DUALSENSE_FIRMWARE, Model, parse, rumble_report, rumble_v2};
use super::{PadChange, PadId, PadState, Rumble, SONY, STUDIO_KEYS, kind};

/// Bluetooth's HID service, in the device path of a pad paired over it.
const BLUETOOTH_HID: &str = "{00001124-0000-1000-8000-00805f9b34fb}";

/// Room for any report these pads send: Windows reads a collection's
/// longest, 547 bytes for a DualShock 4 on Bluetooth.
const REPORT_BYTES: usize = 1024;

#[derive(Debug)]
enum Report {
    State(PadState),
    Gone,
}

#[derive(Debug)]
pub(super) struct Sony {
    /// Set by the device notification; starts set, for the pads already
    /// plugged in.
    stale: Arc<AtomicBool>,
    notification: HCMNOTIFICATION,
    /// Open pads by device path.
    open: FxHashMap<Vec<u16>, Pad>,
    next_id: u64,
    reports: Receiver<(u64, Report)>,
    sender: Sender<(u64, Report)>,
}

#[derive(Debug)]
struct Pad {
    id: u64,
    reader: Arc<Handle>,
    /// None when the pad can't be written to.
    motors: Option<Arc<Motors>>,
}

impl Sony {
    #[allow(unsafe_code)]
    pub(super) fn new() -> std::io::Result<Self> {
        let hid = hid_class();
        let stale = Arc::new(AtomicBool::new(true));
        let mut filter = CM_NOTIFY_FILTER {
            cbSize: size_of::<CM_NOTIFY_FILTER>() as u32,
            FilterType: CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
            ..Default::default()
        };
        filter.u.DeviceInterface.ClassGuid = hid;
        let mut notification = std::ptr::null_mut();
        // SAFETY: the filter is valid for the call; the context is the
        // AtomicBool `stale` holds, which outlives the registration
        // (Drop unregisters before the Arc goes).
        let result = unsafe {
            CM_Register_Notification(
                &filter,
                Arc::as_ptr(&stale).cast(),
                Some(on_arrival),
                &mut notification,
            )
        };
        if result != CR_SUCCESS {
            return Err(std::io::Error::other(format!(
                "no device notifications (CONFIGRET {result})"
            )));
        }
        let (sender, reports) = channel();
        Ok(Self {
            stale,
            notification,
            open: FxHashMap::default(),
            next_id: 0,
            reports,
            sender,
        })
    }

    pub(super) fn poll(&mut self, out: &mut Vec<PadChange>) {
        if self.stale.swap(false, Ordering::AcqRel) {
            for path in hid_paths(&hid_class()) {
                if !self.open.contains_key(&path) {
                    self.try_open(path, out);
                }
            }
        }
        while let Ok((id, report)) = self.reports.try_recv() {
            match report {
                Report::State(state) => out.push(PadChange::State(PadId::Sony(id), state)),
                Report::Gone => {
                    self.open.retain(|_, pad| {
                        if pad.id != id {
                            return true;
                        }
                        if let Some(motors) = &pad.motors {
                            motors.close();
                        }
                        false
                    });
                    out.push(PadChange::Disconnected(PadId::Sony(id)));
                }
            }
        }
    }

    fn try_open(&mut self, path: Vec<u16>, out: &mut Vec<PadChange>) {
        let lower = String::from_utf16_lossy(&path).to_ascii_lowercase();
        if !lower.contains("054c") {
            return;
        }
        // Reading and writing (feature reports), or reading alone if
        // another program keeps writing to itself.
        let Some(handle) = Handle::open(&path, &[GENERIC_READ | GENERIC_WRITE, GENERIC_READ])
        else {
            return;
        };
        let Some((vendor, product)) = handle.ids() else {
            return;
        };
        let Some(model) = Model::of(product).filter(|_| vendor == SONY) else {
            return;
        };
        let bluetooth = lower.contains(BLUETOOTH_HID);
        if bluetooth && handle.feature(model.full_reports_feature()).is_none() {
            tracing::warn!("a PlayStation pad on Bluetooth may send only short reports");
        }
        let v2 = model == Model::DualSense
            && rumble_v2(product, handle.feature(DUALSENSE_FIRMWARE).as_deref());
        let id = self.next_id;
        self.next_id += 1;
        let handle = Arc::new(handle);
        let reader = Arc::clone(&handle);
        let sender = self.sender.clone();
        let body = move || read_reports(&reader, model, bluetooth, id, &sender);
        if let Err(e) = rook_platform::get()
            .threads
            .spawn("rook gamepad", Box::new(body))
        {
            tracing::warn!("a PlayStation pad won't be read: {e}");
            return;
        }
        let motors = Motors::open(&path, model, bluetooth, v2);
        out.push(PadChange::Connected {
            id: PadId::Sony(id),
            kind: kind(vendor, product),
            supported: STUDIO_KEYS.to_vec(),
            rumbles: motors.is_some(),
        });
        let pad = Pad {
            id,
            reader: handle,
            motors,
        };
        self.open.insert(path, pad);
    }

    pub(super) fn rumble(&self, id: u64, speeds: Rumble) {
        let pad = self.open.values().find(|p| p.id == id);
        if let Some(motors) = pad.and_then(|p| p.motors.as_ref()) {
            motors.set(speeds.map(|s| (s.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
}

impl Drop for Sony {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: registered in `new`; no callback runs after this
        // returns, so `stale` may go.
        unsafe { CM_Unregister_Notification(self.notification) };
        for pad in self.open.values() {
            pad.reader.stopping.store(true, Ordering::Release);
            pad.reader.cancel();
            // A pad keeps rumbling after its program has gone, so its
            // motors stop here, before the process can end.
            if let Some(motors) = &pad.motors {
                motors.close();
                motors.write([0, 0]);
            }
        }
    }
}

/// A pad's motors: the speeds to send next, which a thread of their own
/// writes, on a handle open for writing alone.
#[derive(Debug)]
struct Motors {
    writer: Handle,
    model: Model,
    bluetooth: bool,
    /// A DualSense takes the newer compatible rumble.
    v2: bool,
    /// The device's output report length, which every write fills.
    length: usize,
    /// Numbers a DualSense's Bluetooth reports.
    sequence: AtomicU8,
    wanted: Mutex<Wanted>,
    wake: Condvar,
}

#[derive(Debug, Default)]
struct Wanted {
    /// The speeds not yet written, 0 to 255: Large, then Small.
    speeds: Option<[u8; 2]>,
    /// The pad is gone or let go, so the thread ends.
    closed: bool,
}

impl Motors {
    /// Open the pad for writing and start its writer; None if it can't
    /// be written to (another program holds it) or the thread can't
    /// start.
    fn open(path: &[u16], model: Model, bluetooth: bool, v2: bool) -> Option<Arc<Self>> {
        let writer = Handle::open(path, &[GENERIC_WRITE])?;
        let length = writer.output_length()?;
        let motors = Arc::new(Self {
            writer,
            model,
            bluetooth,
            v2,
            length,
            sequence: AtomicU8::new(0),
            wanted: Mutex::default(),
            wake: Condvar::new(),
        });
        let thread = Arc::clone(&motors);
        let body = move || thread.run();
        match rook_platform::get()
            .threads
            .spawn("rook gamepad motors", Box::new(body))
        {
            Ok(()) => Some(motors),
            Err(e) => {
                tracing::warn!("a PlayStation pad's motors won't run: {e}");
                None
            }
        }
    }

    fn set(&self, speeds: [u8; 2]) {
        let mut wanted = self.wanted.lock().unwrap_or_else(PoisonError::into_inner);
        wanted.speeds = Some(speeds);
        self.wake.notify_one();
    }

    fn close(&self) {
        let mut wanted = self.wanted.lock().unwrap_or_else(PoisonError::into_inner);
        wanted.closed = true;
        self.wake.notify_one();
    }

    /// The writer thread: the latest speeds, each time they change.
    fn run(&self) {
        loop {
            let speeds = {
                let mut wanted = self.wanted.lock().unwrap_or_else(PoisonError::into_inner);
                loop {
                    if wanted.closed {
                        return;
                    }
                    if let Some(speeds) = wanted.speeds.take() {
                        break speeds;
                    }
                    wanted = self
                        .wake
                        .wait(wanted)
                        .unwrap_or_else(PoisonError::into_inner);
                }
            };
            self.write(speeds);
        }
    }

    fn write(&self, speeds: [u8; 2]) {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed) % 16;
        let mut report = rumble_report(self.model, self.bluetooth, self.v2, sequence, speeds);
        report.resize(self.length.max(report.len()), 0);
        if !self.writer.write(&report) {
            tracing::debug!("a PlayStation pad's motors weren't set; it may have gone");
        }
    }
}

/// A HID device open for reading or writing, shared by its thread;
/// closed when all its holders let go.
#[derive(Debug)]
struct Handle {
    raw: HANDLE,
    /// The pad is being let go, so a failed read isn't it leaving.
    stopping: AtomicBool,
}

// SAFETY: a file handle may be used from any thread; reads happen on the
// reader's thread alone, and the others only cancel them. Writes come
// from the motors' thread, and once more as the pads are let go; the
// handle isn't overlapped, so Windows runs them one at a time.
#[allow(unsafe_code)]
unsafe impl Send for Handle {}
#[allow(unsafe_code)]
unsafe impl Sync for Handle {}

impl Handle {
    /// Open with the first of `accesses` the device allows.
    #[allow(unsafe_code)]
    fn open(path: &[u16], accesses: &[u32]) -> Option<Self> {
        let mut name = path.to_vec();
        name.push(0);
        for &access in accesses {
            // SAFETY: `name` is a NUL-terminated device path; no security
            // attributes or template.
            let raw = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    access,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut(),
                )
            };
            if raw != INVALID_HANDLE_VALUE {
                return Some(Self {
                    raw,
                    stopping: AtomicBool::new(false),
                });
            }
        }
        None
    }

    #[allow(unsafe_code)]
    fn ids(&self) -> Option<(u16, u16)> {
        let mut attributes = HIDD_ATTRIBUTES {
            Size: size_of::<HIDD_ATTRIBUTES>() as u32,
            VendorID: 0,
            ProductID: 0,
            VersionNumber: 0,
        };
        // SAFETY: the handle is open and the struct's size is set.
        unsafe { HidD_GetAttributes(self.raw, &mut attributes) }
            .then_some((attributes.VendorID, attributes.ProductID))
    }

    /// The feature report `(id, length)`, or None if the pad didn't give
    /// it. Reading some has an effect of its own on the pad.
    #[allow(unsafe_code)]
    fn feature(&self, (id, len): (u8, usize)) -> Option<Vec<u8>> {
        let mut buffer = vec![0u8; len];
        buffer[0] = id;
        // SAFETY: the buffer is valid for its length.
        let read = unsafe { HidD_GetFeature(self.raw, buffer.as_mut_ptr().cast(), len as u32) };
        read.then_some(buffer)
    }

    /// The length every output report written must have; None if the
    /// device takes none.
    #[allow(unsafe_code)]
    fn output_length(&self) -> Option<usize> {
        let mut data = 0;
        // SAFETY: the handle is open; `data` is freed below.
        if !unsafe { HidD_GetPreparsedData(self.raw, &mut data) } {
            return None;
        }
        let mut caps = HIDP_CAPS::default();
        // SAFETY: `data` came from HidD_GetPreparsedData and is freed
        // only after this read.
        let status = unsafe { HidP_GetCaps(data, &mut caps) };
        // SAFETY: as above, and freed once.
        unsafe { HidD_FreePreparsedData(data) };
        let length = usize::from(caps.OutputReportByteLength);
        (status == HIDP_STATUS_SUCCESS && length > 0).then_some(length)
    }

    /// One report, or None once the pad is gone or let go.
    #[allow(unsafe_code)]
    fn read<'a>(&self, buffer: &'a mut [u8]) -> Option<&'a [u8]> {
        let mut read = 0u32;
        // SAFETY: the buffer is valid for its length; the handle isn't
        // overlapped, so the read finishes (or fails) before returning.
        let ok = unsafe {
            ReadFile(
                self.raw,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut read,
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then(|| &buffer[..read as usize])
    }

    /// Write one output report; false if the pad didn't take it.
    #[allow(unsafe_code)]
    fn write(&self, report: &[u8]) -> bool {
        let mut written = 0u32;
        // SAFETY: the report is valid for its length; the handle isn't
        // overlapped, so the write finishes (or fails) before returning.
        let ok = unsafe {
            WriteFile(
                self.raw,
                report.as_ptr(),
                report.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            )
        };
        ok != 0
    }

    /// End a read blocked on another thread.
    #[allow(unsafe_code)]
    fn cancel(&self) {
        // SAFETY: the handle is open until every holder lets it go, and
        // this one still holds it.
        unsafe { CancelIoEx(self.raw, std::ptr::null()) };
    }
}

impl Drop for Handle {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: opened by `Handle::open` and closed only here.
        unsafe { CloseHandle(self.raw) };
    }
}

/// The reader thread: each report that changes the pad's state is sent,
/// until a read fails because the pad left or `Sony` let it go.
fn read_reports(
    handle: &Handle,
    model: Model,
    bluetooth: bool,
    id: u64,
    sender: &Sender<(u64, Report)>,
) {
    let mut buffer = vec![0u8; REPORT_BYTES];
    let mut last = None;
    while let Some(report) = handle.read(&mut buffer) {
        let Some(state) = parse(model, bluetooth, report) else {
            continue;
        };
        if last != Some(state) {
            last = Some(state);
            if sender.send((id, Report::State(state))).is_err() {
                return;
            }
        }
    }
    if !handle.stopping.load(Ordering::Acquire) {
        let _ = sender.send((id, Report::Gone));
    }
}

#[allow(unsafe_code)]
fn hid_class() -> GUID {
    let mut hid = GUID::from_u128(0);
    // SAFETY: writes the GUID it's given.
    unsafe { HidD_GetHidGuid(&mut hid) };
    hid
}

/// Every present HID device's interface path.
#[allow(unsafe_code)]
fn hid_paths(hid: &GUID) -> Vec<Vec<u16>> {
    loop {
        let mut len = 0u32;
        // SAFETY: writes the length it's given.
        let sized = unsafe {
            CM_Get_Device_Interface_List_SizeW(
                &mut len,
                hid,
                std::ptr::null(),
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        if sized != CR_SUCCESS {
            return Vec::new();
        }
        let mut list = vec![0u16; len as usize];
        // SAFETY: the buffer is valid for `len` characters.
        let listed = unsafe {
            CM_Get_Device_Interface_ListW(
                hid,
                std::ptr::null(),
                list.as_mut_ptr(),
                len,
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        match listed {
            // A device arrived between the two calls.
            CR_BUFFER_SMALL => continue,
            CR_SUCCESS => {
                return list
                    .split(|&c| c == 0)
                    .filter(|p| !p.is_empty())
                    .map(<[u16]>::to_vec)
                    .collect();
            }
            _ => return Vec::new(),
        }
    }
}

#[allow(unsafe_code)]
unsafe extern "system" fn on_arrival(
    _: HCMNOTIFICATION,
    context: *const c_void,
    action: CM_NOTIFY_ACTION,
    _: *const CM_NOTIFY_EVENT_DATA,
    _: u32,
) -> u32 {
    if action == CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL {
        // SAFETY: the context is `Sony::stale`'s AtomicBool, alive while
        // the registration is.
        let stale = unsafe { &*context.cast::<AtomicBool>() };
        stale.store(true, Ordering::Release);
    }
    0
}
