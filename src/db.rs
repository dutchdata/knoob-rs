use lmdb::{Cursor, Database, Environment, EnvironmentFlags, Transaction, WriteFlags};
use std::{error::Error, fs, path::Path, sync::Arc};

// -----------------------------------------------------------------------------
// DB setup
// -----------------------------------------------------------------------------

pub fn setup_env(path: &str, size: usize) -> Result<Arc<Environment>, Box<dyn Error>> {
    let path = Path::new(path);
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    let mut builder = Environment::new();
    builder.set_map_size(size);
    builder.set_flags(EnvironmentFlags::NO_SYNC | EnvironmentFlags::NO_META_SYNC);
    let env = Arc::new(builder.open(path)?);
    Ok(env)
}

pub fn open_db(env: &Arc<Environment>) -> Result<Arc<Database>, Box<dyn Error>> {
    let db = Arc::new(env.open_db(None)?);
    Ok(db)
}

// -----------------------------------------------------------------------------
// AppDb — all envs and dbs in one place, passed as web::Data<AppDb>
// -----------------------------------------------------------------------------

pub struct AppDb {
    // access points
    pub aps_env: Arc<Environment>,
    pub aps_db:  Arc<Database>,

    // client devices
    pub devices_env: Arc<Environment>,
    pub devices_db:  Arc<Database>,

    // frame count time series
    pub frames_env: Arc<Environment>,
    pub frames_db:  Arc<Database>,

    // assoc/disassoc/deauth events
    pub events_env: Arc<Environment>,
    pub events_db:  Arc<Database>,
}

impl AppDb {
    pub fn open() -> Result<Self, Box<dyn Error>> {
        let aps_env     = setup_env("lmdb_aps",     256 << 20)?; // 256MB
        let aps_db      = open_db(&aps_env)?;
        let devices_env = setup_env("lmdb_devices", 256 << 20)?;
        let devices_db  = open_db(&devices_env)?;
        let frames_env  = setup_env("lmdb_frames",  512 << 20)?; // 512MB — time series
        let frames_db   = open_db(&frames_env)?;
        let events_env  = setup_env("lmdb_events",  128 << 20)?;
        let events_db   = open_db(&events_env)?;

        Ok(Self {
            aps_env, aps_db,
            devices_env, devices_db,
            frames_env, frames_db,
            events_env, events_db,
        })
    }
}

// -----------------------------------------------------------------------------
// Value types — all bitcode serialized
// -----------------------------------------------------------------------------

#[derive(bitcode::Encode, bitcode::Decode, Clone, Debug)]
pub struct ApRecord {
    pub bssid:      [u8; 6],
    pub ssid:       Option<String>,    // populated later via beacon IE parsing
    pub channel:    u8,
    pub rssi:       i8,                // last seen RSSI
    pub mfpr:       bool,              // PMF required
    pub mfpc:       bool,              // PMF capable
    pub first_seen: u64,               // timestamp_us
    pub last_seen:  u64,
}

#[derive(bitcode::Encode, bitcode::Decode, Clone, Debug)]
pub struct DeviceRecord {
    pub mac:          [u8; 6],
    pub last_bssid:   [u8; 6],
    pub is_randomized: bool,
    pub first_seen:   u64,
    pub last_seen:    u64,
}

/// Key: timestamp_sec (u64 BE) ++ mac (6 bytes) = 14 bytes
/// Stored per-second per-device. API aggregates over time ranges.
#[derive(bitcode::Encode, bitcode::Decode, Clone, Debug, Default)]
pub struct FrameCountRecord {
    pub mgmt:  u32,
    pub ctrl:  u32,
    pub data:  u32,
}

#[derive(bitcode::Encode, bitcode::Decode, Clone, Debug)]
pub enum EventType {
    Assoc,
    Reassoc,
    Disassoc,
    Deauth,
}

#[derive(bitcode::Encode, bitcode::Decode, Clone, Debug)]
pub struct EventRecord {
    pub mac:        [u8; 6],
    pub bssid:      [u8; 6],
    pub event_type: EventType,
    pub timestamp:  u64,
}

// -----------------------------------------------------------------------------
// Key helpers
// -----------------------------------------------------------------------------

/// frame_counts key: 8-byte big-endian timestamp_sec + 6-byte MAC = 14 bytes
/// Big-endian so LMDB's lexicographic order = chronological order.
pub fn frame_key(timestamp_us: u64, mac: &[u8; 6]) -> [u8; 14] {
    let mut key = [0u8; 14];
    let sec = timestamp_us / 1_000_000;
    key[..8].copy_from_slice(&sec.to_be_bytes());
    key[8..].copy_from_slice(mac);
    key
}

/// events key: 8-byte big-endian timestamp_us + 6-byte MAC = 14 bytes
pub fn event_key(timestamp_us: u64, mac: &[u8; 6]) -> [u8; 14] {
    let mut key = [0u8; 14];
    key[..8].copy_from_slice(&timestamp_us.to_be_bytes());
    key[8..].copy_from_slice(mac);
    key
}

// -----------------------------------------------------------------------------
// AP CRUD
// -----------------------------------------------------------------------------

pub fn upsert_ap(
    env: Arc<Environment>,
    db:  Arc<Database>,
    rec: &ApRecord,
) -> Result<(), Box<dyn Error>> {
    let mut txn = env.begin_rw_txn()?;
    txn.put(*db, &rec.bssid, &bitcode::encode(rec), WriteFlags::empty())?;
    txn.commit()?;
    Ok(())
}

pub fn get_ap(
    env: Arc<Environment>,
    db:  Arc<Database>,
    bssid: &[u8; 6],
) -> Option<ApRecord> {
    let txn = env.begin_ro_txn().ok()?;
    let data = txn.get(*db, bssid).ok()?;
    bitcode::decode::<ApRecord>(data).ok()
}

pub fn get_all_aps(
    env: Arc<Environment>,
    db:  Arc<Database>,
) -> Result<Vec<ApRecord>, Box<dyn Error>> {
    let txn = env.begin_ro_txn()?;
    let mut cursor = txn.open_ro_cursor(*db)?;
    let mut out = Vec::new();
    for item in cursor.iter() {
        let (_k, v) = item;
        if let Ok(rec) = bitcode::decode::<ApRecord>(v) {
            out.push(rec);
        }
    }
    Ok(out)
}

// -----------------------------------------------------------------------------
// Device CRUD
// -----------------------------------------------------------------------------

pub fn upsert_device(
    env: Arc<Environment>,
    db:  Arc<Database>,
    rec: &DeviceRecord,
) -> Result<(), Box<dyn Error>> {
    let mut txn = env.begin_rw_txn()?;
    txn.put(*db, &rec.mac, &bitcode::encode(rec), WriteFlags::empty())?;
    txn.commit()?;
    Ok(())
}

pub fn get_device(
    env: Arc<Environment>,
    db:  Arc<Database>,
    mac: &[u8; 6],
) -> Option<DeviceRecord> {
    let txn = env.begin_ro_txn().ok()?;
    let data = txn.get(*db, mac).ok()?;
    bitcode::decode::<DeviceRecord>(data).ok()
}

pub fn get_all_devices(
    env: Arc<Environment>,
    db:  Arc<Database>,
) -> Result<Vec<DeviceRecord>, Box<dyn Error>> {
    let txn = env.begin_ro_txn()?;
    let mut cursor = txn.open_ro_cursor(*db)?;
    let mut out = Vec::new();
    for item in cursor.iter() {
        let (_k, v) = item;
        if let Ok(rec) = bitcode::decode::<DeviceRecord>(v) {
            out.push(rec);
        }
    }
    Ok(out)
}

// -----------------------------------------------------------------------------
// Frame counts CRUD
// -----------------------------------------------------------------------------

pub fn increment_frame_count(
    env:          Arc<Environment>,
    db:           Arc<Database>,
    timestamp_us: u64,
    mac:          &[u8; 6],
    frame_type:   u8,
) -> Result<(), Box<dyn Error>> {
    let key = frame_key(timestamp_us, mac);
    let mut txn = env.begin_rw_txn()?;

    let mut rec = match txn.get(*db, &key) {
        Ok(data) => bitcode::decode::<FrameCountRecord>(data).unwrap_or_default(),
        Err(_)   => FrameCountRecord::default(),
    };

    match frame_type {
        0 => rec.mgmt += 1,
        1 => rec.ctrl += 1,
        2 => rec.data += 1,
        _ => {}
    }

    txn.put(*db, &key, &bitcode::encode(&rec), WriteFlags::empty())?;
    txn.commit()?;
    Ok(())
}

/// Fetch frame counts for a given MAC over a time range (timestamp_us).
pub fn get_frame_counts_for_mac(
    env:      Arc<Environment>,
    db:       Arc<Database>,
    mac:      &[u8; 6],
    from_us:  u64,
    to_us:    u64,
) -> Result<Vec<(u64, FrameCountRecord)>, Box<dyn Error>> {
    let start_key = frame_key(from_us, mac);
    let txn = env.begin_ro_txn()?;
    let mut cursor = txn.open_ro_cursor(*db)?;
    let mut out = Vec::new();

    for item in cursor.iter_from(&start_key) {
        let (k, v) = item;
        if k.len() < 14 { break; }
        // stop if mac doesn't match or we've passed to_us
        if &k[8..14] != mac { continue; }
        let sec = u64::from_be_bytes(k[..8].try_into()?);
        if sec * 1_000_000 > to_us { break; }
        if let Ok(rec) = bitcode::decode::<FrameCountRecord>(v) {
            out.push((sec, rec));
        }
    }
    Ok(out)
}

/// Scan all frame_count rows in [from_us, to_us], aggregated by second
/// across all devices. Single contiguous range scan — O(rows in range).
pub fn get_frame_counts_total(
    env:     Arc<Environment>,
    db:      Arc<Database>,
    from_us: u64,
    to_us:   u64,
) -> Result<Vec<(u64, u32)>, Box<dyn Error>> {
    let from_sec = from_us / 1_000_000;
    let to_sec   = to_us   / 1_000_000;

    // start key: from_sec ++ zero mac
    let mut start_key = [0u8; 14];
    start_key[..8].copy_from_slice(&from_sec.to_be_bytes());

    let txn = env.begin_ro_txn()?;
    let mut cursor = txn.open_ro_cursor(*db)?;
    let mut map: std::collections::BTreeMap<u64, u32> = std::collections::BTreeMap::new();

    for item in cursor.iter_from(&start_key) {
        let (k, v) = item;
        if k.len() < 14 { break; }
        let sec = u64::from_be_bytes(k[..8].try_into()?);
        if sec > to_sec { break; }
        if let Ok(rec) = bitcode::decode::<FrameCountRecord>(v) {
            *map.entry(sec).or_insert(0) += rec.mgmt + rec.ctrl + rec.data;
        }
    }
    Ok(map.into_iter().collect())
}

// -----------------------------------------------------------------------------
// Event CRUD
// -----------------------------------------------------------------------------

pub fn insert_event(
    env: Arc<Environment>,
    db:  Arc<Database>,
    rec: &EventRecord,
) -> Result<(), Box<dyn Error>> {
    let key = event_key(rec.timestamp, &rec.mac);
    let mut txn = env.begin_rw_txn()?;
    txn.put(*db, &key, &bitcode::encode(rec), WriteFlags::empty())?;
    txn.commit()?;
    Ok(())
}

#[allow(dead_code)]
pub fn get_events_for_mac(
    env:     Arc<Environment>,
    db:      Arc<Database>,
    mac:     &[u8; 6],
    from_us: u64,
    to_us:   u64,
) -> Result<Vec<EventRecord>, Box<dyn Error>> {
    let start_key = event_key(from_us, mac);
    let txn = env.begin_ro_txn()?;
    let mut cursor = txn.open_ro_cursor(*db)?;
    let mut out = Vec::new();

    for item in cursor.iter_from(&start_key) {
        let (k, v) = item;
        if k.len() < 14 { break; }
        if &k[8..14] != mac { continue; }
        let ts = u64::from_be_bytes(k[..8].try_into()?);
        if ts > to_us { break; }
        if let Ok(rec) = bitcode::decode::<EventRecord>(v) {
            out.push(rec);
        }
    }
    Ok(out)
}

pub fn get_all_events(
    env: Arc<Environment>,
    db:  Arc<Database>,
) -> Result<Vec<EventRecord>, Box<dyn Error>> {
    let txn = env.begin_ro_txn()?;
    let mut cursor = txn.open_ro_cursor(*db)?;
    let mut out = Vec::new();
    for item in cursor.iter() {
        let (_k, v) = item;
        if let Ok(rec) = bitcode::decode::<EventRecord>(v) {
            out.push(rec);
        }
    }
    Ok(out)
}
