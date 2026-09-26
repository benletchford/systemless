use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use js_sys::{Array, Promise, Uint8Array};
use serde::{Deserialize, Serialize};
use systemless::runner::VfsFileSnapshot;
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{
    Blob, BlobPropertyBag, Event, HtmlAnchorElement, IdbDatabase, IdbObjectStore, IdbOpenDbRequest,
    IdbRequest, IdbTransactionMode, IdbVersionChangeEvent, Url,
};

const DB_NAME: &str = "systemless-save-files";
const DB_VERSION: u32 = 1;
const STORE_NAME: &str = "files";

thread_local! {
    static PENDING_SAVES: std::cell::RefCell<std::collections::BTreeMap<String, usize>> = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
    static LAST_SAVE_ERRORS: std::cell::RefCell<std::collections::BTreeMap<String, String>> = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

struct PendingSave(String);

impl PendingSave {
    fn new(game_id: &str) -> Self {
        PENDING_SAVES.with(|pending| *pending.borrow_mut().entry(game_id.into()).or_default() += 1);
        Self(game_id.into())
    }
}

impl Drop for PendingSave {
    fn drop(&mut self) {
        PENDING_SAVES.with(|pending| {
            let mut pending = pending.borrow_mut();
            if let Some(count) = pending.get_mut(&self.0) {
                *count -= 1;
                if *count == 0 {
                    pending.remove(&self.0);
                }
            }
        });
    }
}

pub async fn flush_pending_saves(game_id: &str) -> Result<(), String> {
    while PENDING_SAVES.with(|pending| pending.borrow().contains_key(game_id)) {
        let _ = JsFuture::from(crate::browser_bridge::yield_systemless_task()).await;
    }
    take_save_error(game_id).map_or(Ok(()), Err)
}

pub fn report_save_error(game_id: &str, message: String) {
    LAST_SAVE_ERRORS.with(|errors| {
        errors.borrow_mut().insert(game_id.into(), message);
    });
}

pub fn take_save_error(game_id: &str) -> Option<String> {
    LAST_SAVE_ERRORS.with(|errors| errors.borrow_mut().remove(game_id))
}

struct SaveDatabase(IdbDatabase);

impl std::ops::Deref for SaveDatabase {
    type Target = IdbDatabase;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for SaveDatabase {
    fn drop(&mut self) {
        self.0.close();
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadableSaveFile {
    pub path: String,
    pub name: String,
    pub data_len: usize,
    pub resource_len: usize,
    pub modified_date: u32,
    pub macbinary: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredSaveFile {
    game_id: String,
    path: String,
    file_type: u32,
    creator: u32,
    finder_flags: u16,
    created_date: u32,
    modified_date: u32,
    data_fork_b64: String,
    resource_fork_b64: String,
}

impl StoredSaveFile {
    fn from_snapshot(game_id: &str, file: &VfsFileSnapshot) -> Self {
        Self {
            game_id: game_id.to_string(),
            path: file.path.clone(),
            file_type: file.file_type,
            creator: file.creator,
            finder_flags: file.finder_flags,
            created_date: file.created_date,
            modified_date: file.modified_date,
            data_fork_b64: BASE64.encode(&file.data_fork),
            resource_fork_b64: BASE64.encode(&file.resource_fork),
        }
    }

    fn into_snapshot(self) -> Result<VfsFileSnapshot, String> {
        Ok(VfsFileSnapshot {
            path: self.path,
            data_fork: BASE64
                .decode(self.data_fork_b64)
                .map_err(|err| format!("decode data fork: {err}"))?,
            resource_fork: BASE64
                .decode(self.resource_fork_b64)
                .map_err(|err| format!("decode resource fork: {err}"))?,
            file_type: self.file_type,
            creator: self.creator,
            finder_flags: self.finder_flags,
            created_date: self.created_date,
            modified_date: self.modified_date,
        })
    }
}

pub async fn load_saved_files(game_id: &str) -> Result<Vec<VfsFileSnapshot>, String> {
    // IndexedDB open requests cannot be aborted. Let this one read complete even
    // if navigation cancels startup, so its callbacks and database are released.
    // The result has no effect on guest state until the caller consumes it.
    let result = std::rc::Rc::new(std::cell::RefCell::new(None));
    let completion = result.clone();
    let game_id = game_id.to_owned();
    let promise = wasm_bindgen_futures::future_to_promise(async move {
        *completion.borrow_mut() = Some(load_saved_files_inner(&game_id).await);
        Ok(JsValue::UNDEFINED)
    });
    JsFuture::from(promise).await.map_err(js_error_string)?;
    let loaded = result.borrow_mut().take().expect("save read completed");
    loaded
}

async fn load_saved_files_inner(game_id: &str) -> Result<Vec<VfsFileSnapshot>, String> {
    let db = open_db().await?;
    let store = object_store(&db, IdbTransactionMode::Readonly)?;
    let request = store.get_all().map_err(js_error_string)?;
    let value = await_idb_request(&request).await?;
    let values: Array = value
        .dyn_into()
        .map_err(|_| "IndexedDB getAll did not return an array".to_string())?;

    let mut files = Vec::new();
    for value in values.iter() {
        let Some(json) = value.as_string() else {
            continue;
        };
        let Ok(record) = serde_json::from_str::<StoredSaveFile>(&json) else {
            continue;
        };
        if record.game_id != game_id {
            continue;
        }
        files.push(record.into_snapshot()?);
    }
    files.sort_by_key(|file| file.path.to_ascii_lowercase());
    Ok(files)
}

pub fn persist_save_file(game_id: String, file: VfsFileSnapshot) {
    let pending = PendingSave::new(&game_id);
    spawn_local(async move {
        let _pending = pending;
        if let Err(error) = persist_save_file_inner(&game_id, &file).await {
            report_save_error(
                &game_id,
                format!("Could not persist {}: {error}", file.path),
            );
        }
    });
}

pub fn delete_save_file(game_id: String, path: String) {
    let pending = PendingSave::new(&game_id);
    spawn_local(async move {
        let _pending = pending;
        if let Err(error) = delete_save_file_inner(&game_id, &path).await {
            report_save_error(&game_id, format!("Could not delete {path}: {error}"));
        }
    });
}

async fn persist_save_file_inner(game_id: &str, file: &VfsFileSnapshot) -> Result<(), String> {
    let db = open_db().await?;
    let store = object_store(&db, IdbTransactionMode::Readwrite)?;
    let completion = TransactionCompletion::new(store.transaction());
    let record = StoredSaveFile::from_snapshot(game_id, file);
    let json = serde_json::to_string(&record).map_err(|err| err.to_string())?;
    let request = store
        .put_with_key(
            &JsValue::from_str(&json),
            &JsValue::from_str(&record_key(game_id, &file.path)),
        )
        .map_err(js_error_string)?;
    await_idb_request(&request).await?;
    completion.wait().await
}

async fn delete_save_file_inner(game_id: &str, path: &str) -> Result<(), String> {
    let db = open_db().await?;
    let store = object_store(&db, IdbTransactionMode::Readwrite)?;
    let completion = TransactionCompletion::new(store.transaction());
    let request = store
        .delete(&JsValue::from_str(&record_key(game_id, path)))
        .map_err(js_error_string)?;
    await_idb_request(&request).await?;
    completion.wait().await
}

fn record_key(game_id: &str, path: &str) -> String {
    format!("{game_id}\u{0}{path}")
}

async fn open_db() -> Result<SaveDatabase, String> {
    let factory = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("indexedDB"))
        .map_err(js_error_string)?
        .dyn_into::<web_sys::IdbFactory>()
        .map_err(|_| "IndexedDB is unavailable".to_string())?;
    let request = factory
        .open_with_u32(DB_NAME, DB_VERSION)
        .map_err(js_error_string)?;

    let on_upgrade = Closure::wrap(Box::new(move |event: IdbVersionChangeEvent| {
        let Some(target) = event.target() else {
            return;
        };
        let Ok(open_request) = target.dyn_into::<IdbOpenDbRequest>() else {
            return;
        };
        let Ok(db_value) = open_request.result() else {
            return;
        };
        let Ok(db) = db_value.dyn_into::<IdbDatabase>() else {
            return;
        };
        if !db.object_store_names().contains(STORE_NAME) {
            let _ = db.create_object_store(STORE_NAME);
        }
    }) as Box<dyn FnMut(IdbVersionChangeEvent)>);
    request.set_onupgradeneeded(Some(on_upgrade.as_ref().unchecked_ref()));
    let db_value = await_idb_request(request.unchecked_ref()).await;
    request.set_onupgradeneeded(None);
    db_value?
        .dyn_into::<IdbDatabase>()
        .map(SaveDatabase)
        .map_err(|_| "IndexedDB open did not return a database".to_string())
}

fn object_store(db: &IdbDatabase, mode: IdbTransactionMode) -> Result<IdbObjectStore, String> {
    let transaction = db
        .transaction_with_str_and_mode(STORE_NAME, mode)
        .map_err(js_error_string)?;
    transaction
        .object_store(STORE_NAME)
        .map_err(js_error_string)
}

struct TransactionCompletion {
    transaction: web_sys::IdbTransaction,
    promise: Promise,
    _complete: Closure<dyn FnMut(Event)>,
    _abort: Closure<dyn FnMut(Event)>,
}

impl TransactionCompletion {
    fn new(transaction: web_sys::IdbTransaction) -> Self {
        let mut complete = None;
        let mut abort = None;
        let promise = Promise::new(&mut |resolve, reject| {
            complete = Some(Closure::wrap(Box::new(move |_: Event| {
                let _ = resolve.call0(&JsValue::UNDEFINED);
            }) as Box<dyn FnMut(Event)>));
            abort = Some(Closure::wrap(Box::new(move |_: Event| {
                let _ = reject.call1(
                    &JsValue::UNDEFINED,
                    &JsValue::from_str("IndexedDB transaction was aborted"),
                );
            }) as Box<dyn FnMut(Event)>));
        });
        let complete = complete.unwrap();
        let abort = abort.unwrap();
        transaction.set_oncomplete(Some(complete.as_ref().unchecked_ref()));
        transaction.set_onabort(Some(abort.as_ref().unchecked_ref()));
        Self {
            transaction,
            promise,
            _complete: complete,
            _abort: abort,
        }
    }

    async fn wait(self) -> Result<(), String> {
        JsFuture::from(self.promise.clone())
            .await
            .map(|_| ())
            .map_err(js_error_string)
    }
}

impl Drop for TransactionCompletion {
    fn drop(&mut self) {
        self.transaction.set_oncomplete(None);
        self.transaction.set_onabort(None);
    }
}

async fn await_idb_request(request: &IdbRequest) -> Result<JsValue, String> {
    let mut success = None;
    let mut error = None;
    let promise = Promise::new(&mut |resolve, reject| {
        let success_request = request.clone();
        let on_success = Closure::wrap(Box::new(move |_event: Event| {
            let value = success_request.result().unwrap_or(JsValue::UNDEFINED);
            let _ = resolve.call1(&JsValue::NULL, &value);
        }) as Box<dyn FnMut(Event)>);
        request.set_onsuccess(Some(on_success.as_ref().unchecked_ref()));
        success = Some(on_success);

        let on_error = Closure::wrap(Box::new(move |_event: Event| {
            let _ = reject.call1(
                &JsValue::NULL,
                &JsValue::from_str("IndexedDB request failed"),
            );
        }) as Box<dyn FnMut(Event)>);
        request.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        error = Some(on_error);
    });
    let result = JsFuture::from(promise).await.map_err(js_error_string);
    request.set_onsuccess(None);
    request.set_onerror(None);
    drop((success, error));
    result
}

pub fn downloadable_save_file(file: &VfsFileSnapshot) -> DownloadableSaveFile {
    DownloadableSaveFile {
        path: file.path.clone(),
        name: file_name_for_path(&file.path).to_string(),
        data_len: file.data_fork.len(),
        resource_len: file.resource_fork.len(),
        modified_date: file.modified_date,
        macbinary: encode_macbinary(file),
    }
}

pub fn download_save_file(file: &DownloadableSaveFile) -> Result<(), String> {
    let bytes = Uint8Array::from(file.macbinary.as_slice());
    let parts = Array::new();
    parts.push(bytes.as_ref());

    let options = BlobPropertyBag::new();
    options.set_type("application/x-macbinary");
    let blob =
        Blob::new_with_u8_array_sequence_and_options(&parts, &options).map_err(js_error_string)?;
    let url = Url::create_object_url_with_blob(&blob).map_err(js_error_string)?;
    let result = click_download_link(&url, &download_name(&file.name));
    let _ = Url::revoke_object_url(&url);
    result
}

fn click_download_link(url: &str, filename: &str) -> Result<(), String> {
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| "document is unavailable".to_string())?;
    let anchor: HtmlAnchorElement = document
        .create_element("a")
        .map_err(js_error_string)?
        .dyn_into()
        .map_err(|_| "created element is not an anchor".to_string())?;
    anchor.set_href(url);
    anchor.set_download(filename);
    let _ = anchor.style().set_property("display", "none");
    if let Some(body) = document.body() {
        let _ = body.append_child(anchor.as_ref());
        anchor.click();
        anchor.remove();
    } else {
        anchor.click();
    }
    Ok(())
}

fn download_name(name: &str) -> String {
    let mut clean = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '-' | '_' | '.') {
            clean.push(ch);
        } else {
            clean.push('_');
        }
    }
    if clean.is_empty() {
        "Systemless Save.bin".to_string()
    } else if clean.to_ascii_lowercase().ends_with(".bin") {
        clean
    } else {
        format!("{clean}.bin")
    }
}

fn file_name_for_path(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
}

pub fn decode_macbinary_save_file(
    path_prefix: &str,
    bytes: &[u8],
) -> Result<VfsFileSnapshot, String> {
    if bytes.len() < 128 {
        return Err("MacBinary file is too short".to_string());
    }

    let name_len = usize::from(bytes[1]);
    if !(1..=63).contains(&name_len) {
        return Err("MacBinary file has an invalid filename".to_string());
    }

    let name = clean_import_file_name(&decode_mac_roman(&bytes[2..2 + name_len]))?;
    let data_len = read_be_u32(bytes, 83)? as usize;
    let resource_len = read_be_u32(bytes, 87)? as usize;
    let data_start = 128usize;
    let padded_resource_start = data_start
        .checked_add(data_len)
        .and_then(|offset| offset.checked_add(padding_128(data_len)))
        .ok_or_else(|| "MacBinary fork lengths overflow".to_string())?;
    let unpadded_resource_start = data_start
        .checked_add(data_len)
        .ok_or_else(|| "MacBinary fork lengths overflow".to_string())?;
    let resource_start = macbinary_resource_start(
        bytes,
        padded_resource_start,
        unpadded_resource_start,
        resource_len,
    )?;
    let resource_end = resource_start
        .checked_add(resource_len)
        .ok_or_else(|| "MacBinary fork lengths overflow".to_string())?;

    Ok(VfsFileSnapshot {
        path: prefixed_import_path(path_prefix, &name),
        data_fork: bytes[data_start..data_start + data_len].to_vec(),
        resource_fork: bytes[resource_start..resource_end].to_vec(),
        file_type: read_be_u32(bytes, 65)?,
        creator: read_be_u32(bytes, 69)?,
        finder_flags: (u16::from(bytes[73]) << 8) | u16::from(bytes[101]),
        created_date: read_be_u32(bytes, 91)?,
        modified_date: read_be_u32(bytes, 95)?,
    })
}

fn macbinary_resource_start(
    bytes: &[u8],
    padded_resource_start: usize,
    unpadded_resource_start: usize,
    resource_len: usize,
) -> Result<usize, String> {
    if padded_resource_start
        .checked_add(resource_len)
        .is_some_and(|end| end <= bytes.len())
    {
        return Ok(padded_resource_start);
    }

    if unpadded_resource_start >= padded_resource_start || resource_len == 0 {
        return Err("MacBinary file is truncated".to_string());
    }

    for candidate in unpadded_resource_start..padded_resource_start {
        let Some(resource_end) = candidate.checked_add(resource_len) else {
            break;
        };
        if resource_end > bytes.len() {
            continue;
        }
        if looks_like_complete_resource_fork(&bytes[candidate..resource_end])? {
            return Ok(candidate);
        }
    }

    Err("MacBinary file is truncated".to_string())
}

fn looks_like_complete_resource_fork(bytes: &[u8]) -> Result<bool, String> {
    if bytes.len() < 16 {
        return Ok(false);
    }

    let data_offset = read_be_u32(bytes, 0)? as usize;
    let map_offset = read_be_u32(bytes, 4)? as usize;
    let data_len = read_be_u32(bytes, 8)? as usize;
    let map_len = read_be_u32(bytes, 12)? as usize;
    let Some(data_end) = data_offset.checked_add(data_len) else {
        return Ok(false);
    };
    let Some(map_end) = map_offset.checked_add(map_len) else {
        return Ok(false);
    };

    Ok(data_offset >= 16
        && data_offset < map_offset
        && data_end == map_offset
        && map_end == bytes.len())
}

fn encode_macbinary(file: &VfsFileSnapshot) -> Vec<u8> {
    let mut output = Vec::new();
    let mut header = [0u8; 128];
    let filename = encode_mac_roman_lossy(file_name_for_path(&file.path));
    let name_len = filename.len().min(63);
    header[1] = name_len as u8;
    header[2..2 + name_len].copy_from_slice(&filename[..name_len]);
    header[65..69].copy_from_slice(&file.file_type.to_be_bytes());
    header[69..73].copy_from_slice(&file.creator.to_be_bytes());
    header[73] = (file.finder_flags >> 8) as u8;
    header[83..87].copy_from_slice(&(file.data_fork.len() as u32).to_be_bytes());
    header[87..91].copy_from_slice(&(file.resource_fork.len() as u32).to_be_bytes());
    header[91..95].copy_from_slice(&file.created_date.to_be_bytes());
    header[95..99].copy_from_slice(&file.modified_date.to_be_bytes());
    header[101] = file.finder_flags as u8;
    header[102..106].copy_from_slice(b"mBIN");
    header[122] = 130;
    header[123] = 129;
    let crc = crc16_ccitt(&header[..124]);
    header[124..126].copy_from_slice(&crc.to_be_bytes());

    output.extend_from_slice(&header);
    output.extend_from_slice(&file.data_fork);
    output.resize(output.len() + padding_128(file.data_fork.len()), 0);
    output.extend_from_slice(&file.resource_fork);
    output.resize(output.len() + padding_128(file.resource_fork.len()), 0);
    output
}

fn read_be_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "MacBinary header is incomplete".to_string())?;
    Ok(u32::from_be_bytes(value.try_into().unwrap()))
}

fn clean_import_file_name(name: &str) -> Result<String, String> {
    let mut clean = String::new();
    for ch in name.chars() {
        if ch == '/' || ch == ':' || ch == '\0' || ch.is_control() {
            clean.push('_');
        } else {
            clean.push(ch);
        }
    }

    let clean = clean.trim();
    if clean.is_empty() {
        Err("MacBinary file has an empty filename".to_string())
    } else {
        Ok(clean.to_string())
    }
}

fn prefixed_import_path(prefix: &str, name: &str) -> String {
    let prefix = prefix.trim_matches('/');
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

fn padding_128(len: usize) -> usize {
    (128 - (len % 128)) % 128
}

fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

fn js_error_string(value: JsValue) -> String {
    value.as_string().unwrap_or_else(|| format!("{value:?}"))
}

const MAC_ROMAN_HIGH: [char; 128] = [
    '\u{00C4}', '\u{00C5}', '\u{00C7}', '\u{00C9}', '\u{00D1}', '\u{00D6}', '\u{00DC}', '\u{00E1}',
    '\u{00E0}', '\u{00E2}', '\u{00E4}', '\u{00E3}', '\u{00E5}', '\u{00E7}', '\u{00E9}', '\u{00E8}',
    '\u{00EA}', '\u{00EB}', '\u{00ED}', '\u{00EC}', '\u{00EE}', '\u{00EF}', '\u{00F1}', '\u{00F3}',
    '\u{00F2}', '\u{00F4}', '\u{00F6}', '\u{00F5}', '\u{00FA}', '\u{00F9}', '\u{00FB}', '\u{00FC}',
    '\u{2020}', '\u{00B0}', '\u{00A2}', '\u{00A3}', '\u{00A7}', '\u{2022}', '\u{00B6}', '\u{00DF}',
    '\u{00AE}', '\u{00A9}', '\u{2122}', '\u{00B4}', '\u{00A8}', '\u{2260}', '\u{00C6}', '\u{00D8}',
    '\u{221E}', '\u{00B1}', '\u{2264}', '\u{2265}', '\u{00A5}', '\u{00B5}', '\u{2202}', '\u{2211}',
    '\u{220F}', '\u{03C0}', '\u{222B}', '\u{00AA}', '\u{00BA}', '\u{03A9}', '\u{00E6}', '\u{00F8}',
    '\u{00BF}', '\u{00A1}', '\u{00AC}', '\u{221A}', '\u{0192}', '\u{2248}', '\u{2206}', '\u{00AB}',
    '\u{00BB}', '\u{2026}', '\u{00A0}', '\u{00C0}', '\u{00C3}', '\u{00D5}', '\u{0152}', '\u{0153}',
    '\u{2013}', '\u{2014}', '\u{201C}', '\u{201D}', '\u{2018}', '\u{2019}', '\u{00F7}', '\u{25CA}',
    '\u{00FF}', '\u{0178}', '\u{2044}', '\u{20AC}', '\u{2039}', '\u{203A}', '\u{FB01}', '\u{FB02}',
    '\u{2021}', '\u{00B7}', '\u{201A}', '\u{201E}', '\u{2030}', '\u{00C2}', '\u{00CA}', '\u{00C1}',
    '\u{00CB}', '\u{00C8}', '\u{00CD}', '\u{00CE}', '\u{00CF}', '\u{00CC}', '\u{00D3}', '\u{00D4}',
    '\u{F8FF}', '\u{00D2}', '\u{00DA}', '\u{00DB}', '\u{00D9}', '\u{0131}', '\u{02C6}', '\u{02DC}',
    '\u{00AF}', '\u{02D8}', '\u{02D9}', '\u{02DA}', '\u{00B8}', '\u{02DD}', '\u{02DB}', '\u{02C7}',
];

fn decode_mac_roman(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&byte| {
            if byte < 0x80 {
                byte as char
            } else {
                MAC_ROMAN_HIGH[(byte - 0x80) as usize]
            }
        })
        .collect()
}

fn encode_mac_roman_lossy(value: &str) -> Vec<u8> {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii() {
                ch as u8
            } else {
                MAC_ROMAN_HIGH
                    .iter()
                    .position(|&candidate| candidate == ch)
                    .map(|idx| idx as u8 + 0x80)
                    .unwrap_or(b'?')
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn async_save_error_is_visible_once_only_to_its_game() {
        super::report_save_error("first", "Could not persist pilot".into());
        assert_eq!(super::take_save_error("second"), None);
        super::report_save_error("second", "Could not delete pilot".into());
        assert_eq!(
            super::take_save_error("first"),
            Some("Could not persist pilot".into())
        );
        assert_eq!(super::take_save_error("first"), None);
        assert_eq!(
            super::take_save_error("second"),
            Some("Could not delete pilot".into())
        );
    }

    use super::*;

    #[test]
    fn macbinary_download_preserves_fork_lengths_and_metadata() {
        let file = VfsFileSnapshot {
            path: "Pilots/Test Pilot".to_string(),
            data_fork: vec![1, 2, 3],
            resource_fork: vec![4, 5, 6, 7],
            file_type: u32::from_be_bytes(*b"PIL "),
            creator: u32::from_be_bytes(*b"EVO!"),
            finder_flags: 0x4000,
            created_date: 123,
            modified_date: 456,
        };

        let encoded = encode_macbinary(&file);
        assert_eq!(encoded[1], 10);
        assert_eq!(&encoded[2..12], b"Test Pilot");
        assert_eq!(&encoded[65..69], b"PIL ");
        assert_eq!(&encoded[69..73], b"EVO!");
        assert_eq!(u32::from_be_bytes(encoded[83..87].try_into().unwrap()), 3);
        assert_eq!(u32::from_be_bytes(encoded[87..91].try_into().unwrap()), 4);
        assert_eq!(u32::from_be_bytes(encoded[91..95].try_into().unwrap()), 123);
        assert_eq!(u32::from_be_bytes(encoded[95..99].try_into().unwrap()), 456);
        assert_eq!(encoded.len(), 128 + 128 + 128);
        assert_eq!(&encoded[128..131], &[1, 2, 3]);
        assert_eq!(&encoded[256..260], &[4, 5, 6, 7]);
    }

    #[test]
    fn macbinary_import_round_trips_download() {
        let file = VfsFileSnapshot {
            path: "Pilots/MORE\u{2122} Pilot".to_string(),
            data_fork: vec![1, 2, 3],
            resource_fork: vec![4, 5, 6, 7],
            file_type: u32::from_be_bytes(*b"PIL "),
            creator: u32::from_be_bytes(*b"EVO!"),
            finder_flags: 0x4102,
            created_date: 123,
            modified_date: 456,
        };

        let imported = decode_macbinary_save_file("Pilots", &encode_macbinary(&file)).unwrap();
        assert_eq!(imported.path, file.path);
        assert_eq!(imported.data_fork, file.data_fork);
        assert_eq!(imported.resource_fork, file.resource_fork);
        assert_eq!(imported.file_type, file.file_type);
        assert_eq!(imported.creator, file.creator);
        assert_eq!(imported.finder_flags, file.finder_flags);
        assert_eq!(imported.created_date, file.created_date);
        assert_eq!(imported.modified_date, file.modified_date);
    }

    #[test]
    fn macbinary_import_tolerates_legacy_unpadded_data_fork() {
        let resource_fork = minimal_complete_resource_fork();
        let file = VfsFileSnapshot {
            path: "Plug-Ins/Hellscream".to_string(),
            data_fork: b"\x07EV-Edit".to_vec(),
            resource_fork: resource_fork.clone(),
            file_type: u32::from_be_bytes([b'O', b'p', 0x95, b'f']),
            creator: u32::from_be_bytes([b'E', b's', 0x8d, b'O']),
            finder_flags: 0,
            created_date: 0,
            modified_date: 0,
        };

        let encoded = encode_macbinary(&file);
        let data_end = 128 + file.data_fork.len();
        let padded_resource_start = data_end + padding_128(file.data_fork.len());
        let mut legacy = Vec::new();
        legacy.extend_from_slice(&encoded[..data_end]);
        legacy.extend_from_slice(&[0; 8]);
        legacy.extend_from_slice(
            &encoded[padded_resource_start..padded_resource_start + resource_fork.len()],
        );

        let imported = decode_macbinary_save_file("EV Plug-Ins", &legacy).unwrap();
        assert_eq!(imported.path, "EV Plug-Ins/Hellscream");
        assert_eq!(imported.data_fork, file.data_fork);
        assert_eq!(imported.resource_fork, resource_fork);
        assert_eq!(imported.file_type, file.file_type);
        assert_eq!(imported.creator, file.creator);
    }

    #[test]
    fn macbinary_import_rejects_truncated_resource_fork() {
        let file = VfsFileSnapshot {
            path: "Pilots/Test Pilot".to_string(),
            data_fork: vec![1, 2, 3],
            resource_fork: vec![4, 5, 6, 7],
            file_type: u32::from_be_bytes(*b"PIL "),
            creator: u32::from_be_bytes(*b"EVO!"),
            finder_flags: 0,
            created_date: 0,
            modified_date: 0,
        };
        let mut encoded = encode_macbinary(&file);
        encoded.truncate(258);

        assert!(decode_macbinary_save_file("Pilots", &encoded).is_err());
    }

    fn minimal_complete_resource_fork() -> Vec<u8> {
        let mut resource_fork = vec![0; 24];
        resource_fork[3] = 16;
        resource_fork[7] = 20;
        resource_fork[11] = 4;
        resource_fork[15] = 4;
        resource_fork
    }
}
