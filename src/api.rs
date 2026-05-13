use actix_web::{get, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::db::{
    AppDb, EventRecord,
    get_all_aps, get_all_devices, get_all_events,
    get_frame_counts_for_mac,
    get_frame_counts_total,
};

// -----------------------------------------------------------------------------
// Query params
// -----------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct TimeRangeQuery {
    pub from_us: Option<u64>,
    pub to_us:   Option<u64>,
}

// -----------------------------------------------------------------------------
// Response types
// -----------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ApResponse {
    pub bssid:        String,
    pub ssid:         Option<String>,
    pub channel:      u8,
    pub rssi:         i8,
    pub mfpr:         bool,
    pub mfpc:         bool,
    pub first_seen:   u64,
    pub last_seen:    u64,
    pub device_count: usize,
}

#[derive(Serialize)]
pub struct DeviceResponse {
    pub mac:           String,
    pub last_bssid:    String,
    pub is_randomized: bool,
    pub first_seen:    u64,
    pub last_seen:     u64,
}

#[derive(Serialize)]
pub struct TimeSeriesPoint {
    pub timestamp_sec: u64,
    pub mac:           String,
    pub mgmt:          u32,
    pub ctrl:          u32,
    pub data:          u32,
    pub total:         u32,
}

#[derive(Serialize)]
pub struct TotalSeriesPoint {
    pub timestamp_sec: u64,
    pub total:         u32,
}

#[derive(Serialize)]
pub struct EventResponse {
    pub mac:        String,
    pub bssid:      String,
    pub event_type: String,
    pub timestamp:  u64,
}

#[derive(Serialize)]
pub struct DeviceEventCounts {
    pub mac:        String,
    pub connects:   u32,
    pub disconnects: u32,
}

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

fn mac_to_string(mac: &[u8; 6]) -> String {
    format!(
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    )
}

fn event_type_str(e: &EventRecord) -> String {
    match e.event_type {
        crate::db::EventType::Assoc    => "assoc".into(),
        crate::db::EventType::Reassoc  => "reassoc".into(),
        crate::db::EventType::Disassoc => "disassoc".into(),
        crate::db::EventType::Deauth   => "deauth".into(),
    }
}

fn default_time_range(q: &TimeRangeQuery) -> (u64, u64) {
    let to   = q.to_us.unwrap_or(u64::MAX);
    let from = q.from_us.unwrap_or(0);
    (from, to)
}

// -----------------------------------------------------------------------------
// Routes
// -----------------------------------------------------------------------------

/// GET /api/aps
/// All access points seen, with device count derived from devices db.
#[get("/aps")]
pub async fn get_aps(db: web::Data<Arc<AppDb>>) -> impl Responder {
    let aps = match get_all_aps(db.aps_env.clone(), db.aps_db.clone()) {
        Ok(v)  => v,
        Err(e) => {
            tracing::error!("get_aps error: {}", e);
            return HttpResponse::InternalServerError().finish();
        }
    };

    let devices = match get_all_devices(db.devices_env.clone(), db.devices_db.clone()) {
        Ok(v)  => v,
        Err(_) => vec![],
    };

    let resp: Vec<ApResponse> = aps.iter().map(|ap| {
        let device_count = devices.iter()
            .filter(|d| d.last_bssid == ap.bssid)
            .count();
        ApResponse {
            bssid:        mac_to_string(&ap.bssid),
            ssid:         ap.ssid.clone(),
            channel:      ap.channel,
            rssi:         ap.rssi,
            mfpr:         ap.mfpr,
            mfpc:         ap.mfpc,
            first_seen:   ap.first_seen,
            last_seen:    ap.last_seen,
            device_count,
        }
    }).collect();

    HttpResponse::Ok().json(resp)
}

/// GET /api/devices
/// All client devices seen.
#[get("/devices")]
pub async fn get_devices(db: web::Data<Arc<AppDb>>) -> impl Responder {
    let devices = match get_all_devices(db.devices_env.clone(), db.devices_db.clone()) {
        Ok(v)  => v,
        Err(e) => {
            tracing::error!("get_devices error: {}", e);
            return HttpResponse::InternalServerError().finish();
        }
    };

    let resp: Vec<DeviceResponse> = devices.iter().map(|d| DeviceResponse {
        mac:           mac_to_string(&d.mac),
        last_bssid:    mac_to_string(&d.last_bssid),
        is_randomized: d.is_randomized,
        first_seen:    d.first_seen,
        last_seen:     d.last_seen,
    }).collect();

    HttpResponse::Ok().json(resp)
}

/// GET /api/timeseries/total?from_us=&to_us=
/// Grand total packets per second across all devices.
#[get("/timeseries/total")]
pub async fn get_timeseries_total(
    db:    web::Data<Arc<AppDb>>,
    query: web::Query<TimeRangeQuery>,
) -> impl Responder {
    let (from, to) = default_time_range(&query);

    let pairs = match get_frame_counts_total(
        db.frames_env.clone(),
        db.frames_db.clone(),
        from,
        to,
    ) {
        Ok(v)  => v,
        Err(e) => {
            tracing::error!("get_timeseries_total error: {}", e);
            return HttpResponse::InternalServerError().finish();
        }
    };

    let resp: Vec<TotalSeriesPoint> = pairs.into_iter().map(|(sec, total)| {
        TotalSeriesPoint { timestamp_sec: sec, total }
    }).collect();

    HttpResponse::Ok().json(resp)
}

/// GET /api/timeseries/by-device?from_us=&to_us=
/// Per-device frame counts split by mgmt/ctrl/data.
#[get("/timeseries/by-device")]
pub async fn get_timeseries_by_device(
    db:    web::Data<Arc<AppDb>>,
    query: web::Query<TimeRangeQuery>,
) -> impl Responder {
    let (from, to) = default_time_range(&query);

    let devices = match get_all_devices(db.devices_env.clone(), db.devices_db.clone()) {
        Ok(v)  => v,
        Err(e) => {
            tracing::error!("get_timeseries_by_device error: {}", e);
            return HttpResponse::InternalServerError().finish();
        }
    };

    let mut resp: Vec<TimeSeriesPoint> = Vec::new();

    for device in &devices {
        let counts = match get_frame_counts_for_mac(
            db.frames_env.clone(),
            db.frames_db.clone(),
            &device.mac,
            from,
            to,
        ) {
            Ok(v)  => v,
            Err(_) => continue,
        };
        for (sec, rec) in counts {
            resp.push(TimeSeriesPoint {
                timestamp_sec: sec,
                mac:           mac_to_string(&device.mac),
                mgmt:          rec.mgmt,
                ctrl:          rec.ctrl,
                data:          rec.data,
                total:         rec.mgmt + rec.ctrl + rec.data,
            });
        }
    }

    HttpResponse::Ok().json(resp)
}

/// GET /api/events
/// All assoc/disassoc/deauth events.
#[get("/events")]
pub async fn get_events(db: web::Data<Arc<AppDb>>) -> impl Responder {
    let events = match get_all_events(db.events_env.clone(), db.events_db.clone()) {
        Ok(v)  => v,
        Err(e) => {
            tracing::error!("get_events error: {}", e);
            return HttpResponse::InternalServerError().finish();
        }
    };

    let resp: Vec<EventResponse> = events.iter().map(|e| EventResponse {
        mac:        mac_to_string(&e.mac),
        bssid:      mac_to_string(&e.bssid),
        event_type: event_type_str(e),
        timestamp:  e.timestamp,
    }).collect();

    HttpResponse::Ok().json(resp)
}

/// GET /api/events/counts
/// Per-device connect/disconnect counts for the list view.
#[get("/events/counts")]
pub async fn get_event_counts(db: web::Data<Arc<AppDb>>) -> impl Responder {
    let events = match get_all_events(db.events_env.clone(), db.events_db.clone()) {
        Ok(v)  => v,
        Err(e) => {
            tracing::error!("get_event_counts error: {}", e);
            return HttpResponse::InternalServerError().finish();
        }
    };

    let mut map: std::collections::HashMap<[u8; 6], (u32, u32)> =
        std::collections::HashMap::new();

    for e in &events {
        let entry = map.entry(e.mac).or_insert((0, 0));
        match e.event_type {
            crate::db::EventType::Assoc | crate::db::EventType::Reassoc => entry.0 += 1,
            crate::db::EventType::Disassoc | crate::db::EventType::Deauth => entry.1 += 1,
        }
    }

    let mut resp: Vec<DeviceEventCounts> = map.into_iter().map(|(mac, (connects, disconnects))| {
        DeviceEventCounts {
            mac: mac_to_string(&mac),
            connects,
            disconnects,
        }
    }).collect();

    resp.sort_by(|a, b| a.mac.cmp(&b.mac));

    HttpResponse::Ok().json(resp)
}
