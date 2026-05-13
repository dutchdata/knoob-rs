#ifndef CAPTURE_H
#define CAPTURE_H

#include <stdint.h>
#include <stdatomic.h>

/* -------------------------------------------------------------------------
 * Wire-level frame info extracted per-packet.
 * Passed by pointer to the callback — do not store across calls without
 * copying. Callback owns no heap memory here.
 * ---------------------------------------------------------------------- */
typedef struct {
	uint8_t  bssid[6];
	uint8_t  src[6];
	uint8_t  dst[6];

	uint8_t  frame_type;       /* 0=mgmt  1=ctrl  2=data */
	uint8_t  frame_subtype;

	uint8_t  is_randomized;    /* 1 if locally administered bit set in src */

	int8_t   rssi;             /* dBm from radiotap, 0 if not present      */
	uint8_t  channel;          /* derived from radiotap freq, 0 if absent  */

	uint64_t timestamp_us;     /* CLOCK_REALTIME (unix epoch us) at recvfrom() */
} frame_info_t;

/* -------------------------------------------------------------------------
 * Callback type. Called from capture loop thread for every parsed frame.
 * user_data: opaque pointer passed through from capture_config_t.
 * Must be thread-safe — Rust side is responsible for synchronisation.
 * ---------------------------------------------------------------------- */
typedef void (*frame_callback_t)(const frame_info_t *frame, void *user_data);

/* -------------------------------------------------------------------------
 * Configuration passed to capture_start().
 * ---------------------------------------------------------------------- */
typedef struct {
	const char      *ifname;    /* e.g. "wlanp" — must stay valid for lifetime */
	frame_callback_t callback;
	void            *user_data;
} capture_config_t;

/* -------------------------------------------------------------------------
 * Global stop flag. Set to 1 (via capture_stop()) to break the loop.
 * Declared here so signal handlers in other translation units can reach it
 * without going through capture_stop() if needed.
 * ---------------------------------------------------------------------- */
extern _Atomic int g_capture_stop;

/* -------------------------------------------------------------------------
 * API
 * ---------------------------------------------------------------------- */

/*
 * capture_start() — blocking. Opens AF_PACKET socket on config->ifname,
 * loops calling recvfrom(), parses radiotap + 802.11 headers, invokes
 * config->callback for each valid frame.
 *
 * Returns 0 on clean stop (g_capture_stop set), -1 on fatal error
 * (errno set). Intended to run on a dedicated thread.
 */
int capture_start(const capture_config_t *config);

/*
 * capture_stop() — safe to call from any thread or after SIGTERM/SIGINT
 * has been caught in Rust. Sets g_capture_stop atomically.
 */
void capture_stop(void);

#endif /* CAPTURE_H */
