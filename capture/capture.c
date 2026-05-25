#include "capture.h"

#include <endian.h>
#include <errno.h>
#include <stdint.h>
#include <stdatomic.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#include <arpa/inet.h>
#include <linux/if_packet.h>
#include <net/if.h>
#include <netinet/ether.h>
#include <sys/socket.h>

/* -------------------------------------------------------------------------
 * Constants
 * ---------------------------------------------------------------------- */

#define CAPTURE_BUF_SIZE    65536
#define MIN_RADIOTAP_LEN    8       /* version(1) pad(1) len(2) present(4) */
#define MIN_80211_HDR_LEN   10      /* fc(2) dur(2) addr1(6) — absolute min */
#define FULL_80211_HDR_LEN  24      /* fc(2) dur(2) a1(6) a2(6) a3(6) seq(2) */

/* Radiotap field bit positions in it_present bitmask */
#define RTAP_F_TSFT         (1 << 0)    /* 8 bytes, align 8 */
#define RTAP_F_FLAGS        (1 << 1)    /* 1 byte,  align 1 */
#define RTAP_F_RATE         (1 << 2)    /* 1 byte,  align 1 */
#define RTAP_F_CHANNEL      (1 << 3)    /* 4 bytes, align 2 (freq + flags) */
#define RTAP_F_FHSS         (1 << 4)    /* 2 bytes, align 1 */
#define RTAP_F_DBM_SIGNAL   (1 << 5)    /* 1 byte,  align 1 — RSSI */
#define RTAP_F_EXT          (1u << 31)   /* another present word follows */

/* 802.11 frame control field masks */
#define FC_TYPE_MASK        0x000c
#define FC_TYPE_SHIFT       2
#define FC_SUBTYPE_MASK     0x00f0
#define FC_SUBTYPE_SHIFT    4

/* -------------------------------------------------------------------------
 * Wire structs — all packed, all little-endian on the wire
 * ---------------------------------------------------------------------- */

struct radiotap_hdr {
	uint8_t  it_version;
	uint8_t  it_pad;
	uint16_t it_len;        /* total header length including all fields */
	uint32_t it_present;    /* bitmask of present fields; bit31=ext chain */
} __attribute__((packed));

struct ieee80211_hdr {
	uint16_t frame_control;
	uint16_t duration;
	uint8_t  addr1[6];      /* dst */
	uint8_t  addr2[6];      /* src */
	uint8_t  addr3[6];      /* bssid (mgmt) / dst (data) / varies */
	uint16_t seq_ctrl;
} __attribute__((packed));

/* -------------------------------------------------------------------------
 * Global stop flag
 * ---------------------------------------------------------------------- */

_Atomic int g_capture_stop = 0;

/* -------------------------------------------------------------------------
 * Internal helpers
 * ---------------------------------------------------------------------- */

/*
 * freq_to_channel() — convert radiotap frequency (MHz) to 802.11 channel.
 * Returns 0 if unrecognised.
 */
static uint8_t freq_to_channel(uint16_t freq)
{
	if (freq == 2484)
		return 14;
	if (freq >= 2412 && freq <= 2472)
		return (uint8_t)((freq - 2412) / 5 + 1);
	if (freq >= 5160 && freq <= 5885)
		return (uint8_t)((freq - 5000) / 5);
	if (freq >= 5955 && freq <= 7115)  /* 6 GHz band */
		return (uint8_t)((freq - 5950) / 5);
	return 0;
}

/*
 * parse_radiotap() — walk the radiotap header, extract RSSI and channel.
 * Returns pointer to first byte after radiotap header (= start of 802.11),
 * or NULL if the header is malformed or truncated.
 *
 * radiotap field sizes and alignments are fixed by the spec:
 *   each field is aligned to its natural size within the header,
 *   alignment is relative to the start of the radiotap header.
 */
static const uint8_t *parse_radiotap(const uint8_t *buf, int buflen,
		int8_t *rssi_out, uint8_t *chan_out)
{
	*rssi_out = 0;
	*chan_out  = 0;

	if (buflen < MIN_RADIOTAP_LEN)
		return NULL;

	const struct radiotap_hdr *rt = (const struct radiotap_hdr *)buf;
	uint16_t rt_len = le16toh(rt->it_len);

	if (rt_len < MIN_RADIOTAP_LEN || rt_len > buflen)
		return NULL;

	/*
	 * Walk present words using byte offsets into buf to avoid
	 * pointer-to-packed-member issues. Bit 31 = another word follows.
	 * Collect all present bits into a uint64_t.
	 */
	uint64_t present = 0;
	int      word    = 0;
	int      poff    = 4; /* offset of first present word */
	while (1) {
		if (poff + 4 > rt_len)
			return NULL;
		uint32_t p;
		memcpy(&p, buf + poff, 4);
		p = le32toh(p);
		present |= ((uint64_t)(p & ~(uint32_t)RTAP_F_EXT)) << (32 * word);
		poff += 4;
		if (!(p & RTAP_F_EXT))
			break;
		word++;
	}

	/* fields start immediately after all present words */
	int off = poff;

	/* helper macro: align offset to 'a', then advance by 'sz' bytes */
#define RTAP_ALIGN(o, a)  (((o) + (a) - 1) & ~((a) - 1))
#define RTAP_SKIP(sz, al) do { off = RTAP_ALIGN(off, (al)); off += (sz); } while(0)

	for (int bit = 0; bit <= 30; bit++) {
		if (!(present & ((uint64_t)1 << bit)))
			continue;

		switch (bit) {
			case 0: /* TSFT: 8 bytes, align 8 */
				RTAP_SKIP(8, 8);
				break;

			case 1: /* FLAGS: 1 byte, align 1 */
				RTAP_SKIP(1, 1);
				break;

			case 2: /* RATE: 1 byte, align 1 */
				RTAP_SKIP(1, 1);
				break;

			case 3: /* CHANNEL: freq(2) + flags(2), align 2 */
				off = RTAP_ALIGN(off, 2);
				if (off + 4 <= rt_len) {
					uint16_t freq;
					memcpy(&freq, buf + off, 2);
					*chan_out = freq_to_channel(le16toh(freq));
				}
				off += 4;
				break;

			case 4: /* FHSS: 2 bytes, align 1 */
				RTAP_SKIP(2, 1);
				break;

			case 5: /* DBM_ANTSIGNAL (RSSI): 1 byte, align 1 */
				off = RTAP_ALIGN(off, 1);
				if (off + 1 <= rt_len)
					*rssi_out = (int8_t)buf[off];
				off += 1;
				break;

				/* fields 6-30: we don't need them, but must skip correctly */
			case 6:  RTAP_SKIP(1, 1); break;  /* DBM_ANTNOISE      */
			case 7:  RTAP_SKIP(2, 2); break;  /* LOCK_QUALITY      */
			case 8:  RTAP_SKIP(2, 2); break;  /* TX_ATTENUATION    */
			case 9:  RTAP_SKIP(2, 2); break;  /* DB_TX_ATTENUATION */
			case 10: RTAP_SKIP(1, 1); break;  /* DBM_TX_POWER      */
			case 11: RTAP_SKIP(1, 1); break;  /* ANTENNA           */
			case 12: RTAP_SKIP(1, 1); break;  /* DB_ANTSIGNAL      */
			case 13: RTAP_SKIP(1, 1); break;  /* DB_ANTNOISE       */
			case 14: RTAP_SKIP(2, 2); break;  /* RX_FLAGS          */
			case 15: RTAP_SKIP(2, 2); break;  /* TX_FLAGS          */
			case 16: RTAP_SKIP(1, 1); break;  /* RTS_RETRIES       */
			case 17: RTAP_SKIP(1, 1); break;  /* DATA_RETRIES      */
			case 18: RTAP_SKIP(4, 4); break;  /* XChannel (deprecated) */
			case 19: RTAP_SKIP(3, 1); break;  /* MCS               */
			case 20: RTAP_SKIP(8, 4); break;  /* AMPDU_STATUS      */
			case 21: RTAP_SKIP(12, 2); break; /* VHT               */
			case 22: RTAP_SKIP(12, 8); break; /* TIMESTAMP         */
			case 23: RTAP_SKIP(4, 2); break;  /* HE                */
			case 24: RTAP_SKIP(4, 2); break;  /* HE_MU             */
			case 25: RTAP_SKIP(4, 2); break;  /* HE_MU_OTHER_USER  */
			case 26: RTAP_SKIP(4, 1); break;  /* ZERO_LEN_PSDU     */
			case 27: RTAP_SKIP(4, 4); break;  /* LSIG              */
			default: break;                   /* unknown — stop walking */
		}

		if (off > rt_len)
			break;
	}

#undef RTAP_ALIGN
#undef RTAP_SKIP

	return buf + rt_len;
}

/*
 * parse_frame() — parse 802.11 MAC header from buf of length buflen.
 * Fills frame_info_t. Returns 0 on success, -1 to discard.
 */
static int parse_frame(const uint8_t *buf, int buflen, frame_info_t *fi)
{
	/* all frames have at least fc(2) + dur(2) + addr1(6) = 10 bytes */
	if (buflen < MIN_80211_HDR_LEN)
		return -1;

	uint16_t fc = 0;
	memcpy(&fc, buf, 2);
	fc = le16toh(fc);

	fi->frame_type    = (uint8_t)((fc & FC_TYPE_MASK)    >> FC_TYPE_SHIFT);
	fi->frame_subtype = (uint8_t)((fc & FC_SUBTYPE_MASK) >> FC_SUBTYPE_SHIFT);

	/* addr1 (dst) is always present at offset 4 */
	memcpy(fi->dst, buf + 4, 6);

	/*
	 * Control frames have a shorter header — many subtypes only have addr1.
	 * Exceptions: RTS (subtype 11) and PS-Poll (subtype 10) have addr2.
	 * None have addr3. Zero out src/bssid and return early for ctrl.
	 */
	if (fi->frame_type == 1) {
		memset(fi->src,   0, 6);
		memset(fi->bssid, 0, 6);
		/* RTS and PS-Poll do have addr2 at offset 10 */
		if ((fi->frame_subtype == 11 || fi->frame_subtype == 10)
				&& buflen >= 16) {
			memcpy(fi->src, buf + 10, 6);
			fi->is_randomized = (fi->src[0] & 0x02) ? 1 : 0;
		}
		return 0;
	}

	/* mgmt and data frames have full header: need at least 24 bytes */
	if (buflen < FULL_80211_HDR_LEN)
		return -1;

	memcpy(fi->src,   buf + 10, 6); /* addr2 */
	memcpy(fi->bssid, buf + 16, 6); /* addr3 */

	fi->is_randomized = (fi->src[0] & 0x02) ? 1 : 0;

	return 0;
}

/*
 * now_us() — monotonic microseconds.
 */
static uint64_t now_us(void)
{
	struct timespec ts;
	clock_gettime(CLOCK_REALTIME, &ts);
	return (uint64_t)ts.tv_sec * 1000000ULL + (uint64_t)(ts.tv_nsec / 1000);
}

/* -------------------------------------------------------------------------
 * Public API
 * ---------------------------------------------------------------------- */

void capture_stop(void)
{
	atomic_store(&g_capture_stop, 1);
}

int capture_start(const capture_config_t *config)
{
	if (!config || !config->ifname || !config->callback) {
		errno = EINVAL;
		return -1;
	}

	int sock = socket(AF_PACKET, SOCK_RAW, htons(ETH_P_ALL));
	if (sock < 0)
		return -1;

	int ifindex = (int)if_nametoindex(config->ifname);
	if (ifindex == 0) {
		close(sock);
		return -1;
	}

	struct sockaddr_ll addr = {0};
	addr.sll_family   = AF_PACKET;
	addr.sll_ifindex  = ifindex;
	addr.sll_protocol = htons(ETH_P_ALL);

	if (bind(sock, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
		close(sock);
		return -1;
	}

	static uint8_t buf[CAPTURE_BUF_SIZE];
	frame_info_t   fi;
	int            debug_dumped = 0;

	while (!atomic_load(&g_capture_stop)) {
		ssize_t n = recvfrom(sock, buf, sizeof(buf), 0, NULL, NULL);
		if (n < 0) {
			if (errno == EINTR)
				continue;
			close(sock);
			return -1;
		}

		uint64_t ts = now_us();

#ifdef DEBUG
		if (debug_dumped < 100) {
			debug_dumped++;
			int dump_len = (int)n < 64 ? (int)n : 64;
			fprintf(stderr, "radiotap raw (%d bytes total):\n", (int)n);
			for (int i = 0; i < dump_len; i++) {
				if (i % 16 == 0) fprintf(stderr, "  %04x: ", i);
				fprintf(stderr, "%02x ", buf[i]);
				if ((i + 1) % 16 == 0) fprintf(stderr, "\n");
			}
			fprintf(stderr, "\n");
			if (n >= 4) {
				uint16_t rt_len;
				memcpy(&rt_len, buf + 2, 2);
				fprintf(stderr, "it_len=%u\n", le16toh(rt_len));
			}
			if (n >= 8) {
				uint32_t present;
				memcpy(&present, buf + 4, 4);
				fprintf(stderr, "it_present=0x%08x\n", le32toh(present));
			}
		}
#else
		(void)debug_dumped;
#endif

		int8_t  rssi = 0;
		uint8_t chan = 0;

		const uint8_t *frame80211 =
			parse_radiotap(buf, (int)n, &rssi, &chan);

		if (!frame80211)
			continue;

		int remaining = (int)(n - (frame80211 - buf));
		if (remaining < MIN_80211_HDR_LEN)
			continue;

		memset(&fi, 0, sizeof(fi));

		if (parse_frame(frame80211, remaining, &fi) < 0)
			continue;

		fi.rssi         = rssi;
		fi.channel      = chan;
		fi.timestamp_us = ts;

		config->callback(&fi, config->user_data);
	}

	close(sock);
	return 0;
}
