#ifndef CHANHOP_H
#define CHANHOP_H

#include <stdint.h>
#include <stdatomic.h>

/* -------------------------------------------------------------------------
 * Default channel lists. 2.4GHz channels 1-13, 5GHz common non-DFS first
 * then DFS, 6GHz skipped for now (requires more driver support).
 * ---------------------------------------------------------------------- */

#define CHANHOP_24GHZ_CHANNELS  { 1,2,3,4,5,6,7,8,9,10,11,12,13 }
#define CHANHOP_24GHZ_COUNT     13

#define CHANHOP_50GHZ_CHANNELS  { \
	36,40,44,48,          /* UNII-1, no DFS */ \
	52,56,60,64,          /* UNII-2A, DFS   */ \
	100,104,108,112,      /* UNII-2C, DFS   */ \
	116,120,124,128,      /* UNII-2C, DFS   */ \
	132,136,140,144,      /* UNII-2C, DFS   */ \
	149,153,157,161,165   /* UNII-3, no DFS */ \
}
#define CHANHOP_50GHZ_COUNT     25

/* -------------------------------------------------------------------------
 * Configuration
 * ---------------------------------------------------------------------- */

typedef enum {
	CHANHOP_BAND_24GHZ  = 0,
	CHANHOP_BAND_50GHZ  = 1,
	CHANHOP_BAND_BOTH   = 2,
} chanhop_band_t;

typedef struct {
	const char     *ifname;     /* e.g. "wlanp"                          */
	uint32_t        dwell_ms;   /* ms to stay on each channel, e.g. 150  */
	chanhop_band_t  band;       /* which band(s) to hop                  */
} chanhop_config_t;

/* -------------------------------------------------------------------------
 * Global stop flag — shared with capture module via capture_stop() but
 * also checked independently here. Set via chanhop_stop().
 * ---------------------------------------------------------------------- */
extern _Atomic int g_chanhop_stop;

/* -------------------------------------------------------------------------
 * API
 * ---------------------------------------------------------------------- */

/*
 * chanhop_start() — blocking. Loops through channels, dwelling dwell_ms
 * on each, setting channel via nl80211. Intended to run on a dedicated
 * thread. Returns 0 on clean stop, -1 on fatal error (errno set).
 */
int chanhop_start(const chanhop_config_t *config);

/*
 * chanhop_stop() — safe to call from any thread.
 */
void chanhop_stop(void);

#endif /* CHANHOP_H */
