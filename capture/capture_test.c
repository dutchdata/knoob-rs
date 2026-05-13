#include "capture.h"
#include "chanhop.h"
#include <pthread.h>

#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const char *frame_type_str(uint8_t type)
{
	switch (type) {
		case 0: return "mgmt";
		case 1: return "ctrl";
		case 2: return "data";
		default: return "unkn";
	}
}

static const char *mgmt_subtype_str(uint8_t subtype)
{
	switch (subtype) {
		case 0:  return "assoc-req";
		case 1:  return "assoc-resp";
		case 2:  return "reassoc-req";
		case 3:  return "reassoc-resp";
		case 4:  return "probe-req";
		case 5:  return "probe-resp";
		case 8:  return "beacon";
		case 10: return "disassoc";
		case 11: return "auth";
		case 12: return "deauth";
		default: return "other";
	}
}

static void on_frame(const frame_info_t *f, void *user_data)
{
	(void)user_data;

	char src[18], dst[18], bssid[18];

	snprintf(src,   sizeof(src),   "%02x:%02x:%02x:%02x:%02x:%02x",
			f->src[0],   f->src[1],   f->src[2],
			f->src[3],   f->src[4],   f->src[5]);
	snprintf(dst,   sizeof(dst),   "%02x:%02x:%02x:%02x:%02x:%02x",
			f->dst[0],   f->dst[1],   f->dst[2],
			f->dst[3],   f->dst[4],   f->dst[5]);
	snprintf(bssid, sizeof(bssid), "%02x:%02x:%02x:%02x:%02x:%02x",
			f->bssid[0], f->bssid[1], f->bssid[2],
			f->bssid[3], f->bssid[4], f->bssid[5]);

	const char *subtype_label = (f->frame_type == 0)
		? mgmt_subtype_str(f->frame_subtype)
		: "";

	printf("[%lu] type=%-4s sub=%-12s ch=%-3u rssi=%-4d rand=%u\n"
			"         src=%s  dst=%s\n"
			"       bssid=%s\n",
			(unsigned long)f->timestamp_us,
			frame_type_str(f->frame_type),
			subtype_label,
			f->channel,
			f->rssi,
			f->is_randomized,
			src, dst, bssid);
}

static void *chanhop_thread(void *arg)
{
	chanhop_start((chanhop_config_t *)arg);
	return NULL;
}

static void sig_handler(int sig)
{
	(void)sig;
	capture_stop();
}

int main(int argc, char *argv[])
{
	if (argc != 2) {
		fprintf(stderr, "Usage: %s <interface>\n", argv[0]);
		fprintf(stderr, "Example: sudo %s wlanp\n", argv[0]);
		return 1;
	}

	struct sigaction sa = {0};
	sa.sa_handler = sig_handler;
	sigaction(SIGINT,  &sa, NULL);
	sigaction(SIGTERM, &sa, NULL);

	chanhop_config_t hop_cfg = {
		.ifname   = argv[1],
		.dwell_ms = 150,
		.band     = CHANHOP_BAND_BOTH,
	};

	pthread_t hop_tid;
	if (pthread_create(&hop_tid, NULL, chanhop_thread, &hop_cfg) != 0) {
		perror("pthread_create");
		return 1;
	}

	capture_config_t cfg = {
		.ifname    = argv[1],
		.callback  = on_frame,
		.user_data = NULL,
	};

	printf("Capturing on %s with channel hopping — press ^C to stop\n\n", argv[1]);

	int ret = capture_start(&cfg);
	if (ret < 0) {
		perror("capture_start");
		return 1;
	}

	chanhop_stop();
	pthread_join(hop_tid, NULL);
	printf("\nDone.\n");
	return 0;
}
