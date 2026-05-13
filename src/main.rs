mod api;
mod db;
mod ffi;

use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use db::{
    upsert_ap, upsert_device, increment_frame_count, insert_event,
    get_ap, get_device,
    ApRecord, DeviceRecord, EventRecord, EventType, AppDb,
};
use ffi::{
    capture_config_t, capture_start, capture_stop,
    chanhop_config_t, chanhop_start, chanhop_stop,
    chanhop_band_t_CHANHOP_BAND_BOTH,
    frame_info_t,
};
use include_dir::{include_dir, Dir};
use std::{
    ffi::CString,
    sync::Arc,
    thread,
};

// -----------------------------------------------------------------------------
// Embedded frontend — Next.js static export baked into binary at compile time.
// Set KNOOB_STATIC_DIR env var at runtime to serve from disk instead (dev mode).
// -----------------------------------------------------------------------------

static FRONTEND: Dir = include_dir!("$CARGO_MANIFEST_DIR/web/out");

async fn static_file_handler(req: HttpRequest) -> HttpResponse {
    let path = req.uri().path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    // try exact path, then path/index.html for Next.js directory routes
    let file = FRONTEND.get_file(path)
        .or_else(|| FRONTEND.get_file(&format!("{}/index.html", path)));

    match file {
        Some(f) => {
            let mime = mime_guess::from_path(f.path())
                .first_or_octet_stream()
                .to_string();
            HttpResponse::Ok()
                .content_type(mime)
                .body(f.contents().to_vec())
        }
        None => HttpResponse::NotFound().body("404"),
    }
}

// -----------------------------------------------------------------------------
// Frame callback — called from C capture loop on capture thread.
// Safety: user_data is the AppDb Arc raw pointer; the Arc clone passed
// into the capture thread closure outlives capture_start(), so the
// pointer is valid for the lifetime of every callback invocation.
// -----------------------------------------------------------------------------

unsafe extern "C" fn on_frame(frame: *const frame_info_t, user_data: *mut std::ffi::c_void) {
    let frame = unsafe { &*frame };
    let db    = unsafe { &*(user_data as *const AppDb) };

    let now = frame.timestamp_us;

    // -------------------------------------------------------------------------
    // Update or create AP record (mgmt beacon frames have bssid == src)
    // Only treat as AP if it's a beacon (subtype 8) or probe-resp (subtype 5)
    // -------------------------------------------------------------------------
    if frame.frame_type == 0 && (frame.frame_subtype == 8 || frame.frame_subtype == 5) {
        let existing = get_ap(db.aps_env.clone(), db.aps_db.clone(), &frame.bssid);
        let rec = ApRecord {
            bssid:      frame.bssid,
            ssid:       existing.as_ref().and_then(|e| e.ssid.clone()), // preserved
            channel:    frame.channel,
            rssi:       frame.rssi,
            mfpr:       false, // TODO: parse RSN IE in C layer
            mfpc:       false,
            first_seen: existing.as_ref().map(|e| e.first_seen).unwrap_or(now),
            last_seen:  now,
        };
        let _ = upsert_ap(db.aps_env.clone(), db.aps_db.clone(), &rec);
    }

    // -------------------------------------------------------------------------
    // Update or create device record for src MAC (skip broadcast/zero src)
    // -------------------------------------------------------------------------
    let zero  = [0u8; 6];
    let bcast = [0xff; 6];
    if frame.src != zero && frame.src != bcast {
        let existing = get_device(db.devices_env.clone(), db.devices_db.clone(), &frame.src);
        let rec = DeviceRecord {
            mac:           frame.src,
            last_bssid:    frame.bssid,
            is_randomized: frame.is_randomized != 0,
            first_seen:    existing.as_ref().map(|e| e.first_seen).unwrap_or(now),
            last_seen:     now,
        };
        let _ = upsert_device(db.devices_env.clone(), db.devices_db.clone(), &rec);

        let _ = increment_frame_count(
            db.frames_env.clone(),
            db.frames_db.clone(),
            now,
            &frame.src,
            frame.frame_type,
        );
    }

    // -------------------------------------------------------------------------
    // Record association events from mgmt frame subtypes
    // -------------------------------------------------------------------------
    let event_type = match (frame.frame_type, frame.frame_subtype) {
        (0, 0)  => Some(EventType::Assoc),
        (0, 2)  => Some(EventType::Reassoc),
        (0, 10) => Some(EventType::Disassoc),
        (0, 12) => Some(EventType::Deauth),
        _       => None,
    };

    if let Some(et) = event_type {
        if frame.src != zero && frame.src != bcast {
            let rec = EventRecord {
                mac:        frame.src,
                bssid:      frame.bssid,
                event_type: et,
                timestamp:  now,
            };
            let _ = insert_event(db.events_env.clone(), db.events_db.clone(), &rec);
        }
    }
}

// -----------------------------------------------------------------------------
// Signal handling — SIGINT/SIGTERM → stop capture + chanhop cleanly
// -----------------------------------------------------------------------------

fn setup_signals() {
    unsafe {
        libc::signal(libc::SIGINT,  handle_signal as libc::sighandler_t);
        libc::signal(libc::SIGTERM, handle_signal as libc::sighandler_t);
    }
}

extern "C" fn handle_signal(_: libc::c_int) {
    unsafe {
        capture_stop();
        chanhop_stop();
    }
}

// -----------------------------------------------------------------------------
// Main
// -----------------------------------------------------------------------------

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let ifname = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: knoob-rs <interface>");
        eprintln!("Example: sudo knoob-rs wlanp");
        std::process::exit(1);
    });

    // open all LMDB envs
    let db = Arc::new(match AppDb::open() {
        Ok(v)  => v,
        Err(e) => {
            eprintln!("failed to open db: {}", e);
            std::process::exit(1);
        }
    });

    setup_signals();

    // -------------------------------------------------------------------------
    // Spawn channel hopper thread
    // -------------------------------------------------------------------------
    let ifname_hop = ifname.clone();
    thread::spawn(move || {
        let ifname_c = CString::new(ifname_hop).expect("invalid ifname");
        let cfg = chanhop_config_t {
            ifname:   ifname_c.as_ptr(),
            dwell_ms: 150,
            band:     chanhop_band_t_CHANHOP_BAND_BOTH,
        };
        unsafe { chanhop_start(&cfg) };
    });

    // -------------------------------------------------------------------------
    // Spawn capture thread
    // -------------------------------------------------------------------------
    let db_cap = db.clone();
    let ifname_cap = ifname.clone();
    thread::spawn(move || {
        let ifname_c = CString::new(ifname_cap).expect("invalid ifname");
        let cfg = capture_config_t {
            ifname:    ifname_c.as_ptr(),
            callback:  Some(on_frame),
            user_data: Arc::as_ptr(&db_cap) as *mut std::ffi::c_void,
        };
        let ret = unsafe { capture_start(&cfg) };
        if ret < 0 {
            eprintln!("capture_start failed: {}", std::io::Error::last_os_error());
        }
    });

    // -------------------------------------------------------------------------
    // Start actix-web server
    // -------------------------------------------------------------------------
    let db_web = db.clone();

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(db_web.clone()))
            .service(
                web::scope("/api")
                .service(api::get_aps)
                .service(api::get_devices)
                .service(api::get_timeseries_total)
                .service(api::get_timeseries_by_device)
                .service(api::get_events)
                .service(api::get_event_counts),
            )
            .default_service(web::route().to(static_file_handler))
    })
    .bind("0.0.0.0:9090")?
        .run()
        .await
}
