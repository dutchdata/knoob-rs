#include "chanhop.h"

#include <errno.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#include <net/if.h>
#include <sys/socket.h>
#include <linux/netlink.h>
#include <linux/genetlink.h>
#include <linux/nl80211.h>

/* -------------------------------------------------------------------------
 * Constants
 * ---------------------------------------------------------------------- */

#define NL_BUF_SIZE     4096

/* -------------------------------------------------------------------------
 * Global stop flag
 * ---------------------------------------------------------------------- */

_Atomic int g_chanhop_stop = 0;

/* -------------------------------------------------------------------------
 * Minimal netlink helpers — same pattern as socket_dis.c
 * ---------------------------------------------------------------------- */

#ifndef NLA_HDRLEN
#define NLA_HDRLEN  ((int)NLA_ALIGN(sizeof(struct nlattr)))
#endif

#define NLA_DATA(nla)       ((void *)(((char *)(nla)) + NLA_HDRLEN))
#define NLA_NEXT(nla, len)  ((len) -= NLA_ALIGN((nla)->nla_len), \
		(struct nlattr *)(((char *)(nla)) + NLA_ALIGN((nla)->nla_len)))
#define NLA_OK(nla, len)    ((len) >= (int)sizeof(struct nlattr) && \
		(nla)->nla_len >= sizeof(struct nlattr) && \
		(nla)->nla_len <= (len))

struct nlmsg {
	struct nlmsghdr  n;
	struct genlmsghdr g;
	char             buf[NL_BUF_SIZE];
};

static void nla_put_u32(struct nlmsg *msg, int type, uint32_t value)
{
	/* offset into buf past what nlmsg_len already covers */
	int used = (int)(msg->n.nlmsg_len
			- sizeof(msg->n)
			- sizeof(msg->g));
	struct nlattr *nla = (struct nlattr *)(msg->buf + used);
	nla->nla_type = type;
	nla->nla_len  = NLA_HDRLEN + (int)sizeof(uint32_t);
	memcpy(NLA_DATA(nla), &value, sizeof(uint32_t));
	msg->n.nlmsg_len = NLA_ALIGN(msg->n.nlmsg_len)
		+ NLA_ALIGN(nla->nla_len);
}

static struct nlattr *nla_find(struct nlattr *head, int len, int type)
{
	struct nlattr *nla = head;
	while (NLA_OK(nla, len)) {
		if (nla->nla_type == type)
			return nla;
		nla = NLA_NEXT(nla, len);
	}
	return NULL;
}

/*
 * nl80211_get_family_id() — resolve "nl80211" generic netlink family ID.
 * Returns family ID > 0 on success, -1 on error.
 */
static int nl80211_get_family_id(int sock)
{
	struct nlmsg msg = {0};
	msg.n.nlmsg_len   = NLMSG_LENGTH(sizeof(struct genlmsghdr));
	msg.n.nlmsg_type  = GENL_ID_CTRL;
	msg.n.nlmsg_flags = NLM_F_REQUEST;
	msg.n.nlmsg_seq   = 1;
	msg.g.cmd         = CTRL_CMD_GETFAMILY;
	msg.g.version     = 1;

	struct nlattr *nla = (struct nlattr *)(msg.buf);
	static const char fam[] = "nl80211";
	nla->nla_type = CTRL_ATTR_FAMILY_NAME;
	nla->nla_len  = NLA_HDRLEN + (int)sizeof(fam);
	memcpy(NLA_DATA(nla), fam, sizeof(fam));
	msg.n.nlmsg_len += NLA_ALIGN(nla->nla_len);

	if (send(sock, &msg, msg.n.nlmsg_len, 0) < 0)
		return -1;

	char buf[NL_BUF_SIZE];
	int len = (int)recv(sock, buf, sizeof(buf), 0);
	if (len < 0)
		return -1;

	struct nlmsghdr   *nh = (struct nlmsghdr *)buf;
	struct genlmsghdr *gh = (struct genlmsghdr *)NLMSG_DATA(nh);
	int attr_len = (int)(nh->nlmsg_len
			- NLMSG_LENGTH(sizeof(struct genlmsghdr)));
	struct nlattr *attr = (struct nlattr *)((char *)gh
			+ sizeof(struct genlmsghdr));
	struct nlattr *fid  = nla_find(attr, attr_len, CTRL_ATTR_FAMILY_ID);

	return fid ? (int)*(uint16_t *)NLA_DATA(fid) : -1;
}

/*
 * set_channel() — send NL80211_CMD_SET_CHANNEL for the given interface
 * and channel number. Uses HT20 (no bonding) — safe for passive monitor.
 * Returns 0 on success, -1 on error.
 */
static int set_channel(int sock, int nl80211_id, uint32_t ifindex,
		uint32_t channel)
{
	/* Convert channel to frequency (MHz) */
	uint32_t freq;
	if (channel == 14) {
		freq = 2484;
	} else if (channel >= 1 && channel <= 13) {
		freq = 2407 + channel * 5;
	} else if (channel >= 36 && channel <= 177) {
		freq = 5000 + channel * 5;
	} else {
		errno = EINVAL;
		return -1;
	}

	struct nlmsg msg = {0};
	msg.n.nlmsg_len   = NLMSG_LENGTH(sizeof(struct genlmsghdr));
	msg.n.nlmsg_type  = nl80211_id;
	msg.n.nlmsg_flags = NLM_F_REQUEST | NLM_F_ACK;
	msg.n.nlmsg_seq   = 2;
	msg.g.cmd         = NL80211_CMD_SET_CHANNEL;
	msg.g.version     = 0;

	nla_put_u32(&msg, NL80211_ATTR_IFINDEX,        ifindex);
	nla_put_u32(&msg, NL80211_ATTR_WIPHY_FREQ,     freq);
	/* NL80211_CHAN_HT20 = 1 — 20 MHz, no extension channel */
	nla_put_u32(&msg, NL80211_ATTR_WIPHY_CHANNEL_TYPE, NL80211_CHAN_HT20);

	if (send(sock, &msg, msg.n.nlmsg_len, 0) < 0)
		return -1;

	/* consume ACK/error */
	char buf[NL_BUF_SIZE];
	recv(sock, buf, sizeof(buf), 0);

	return 0;
}

/* -------------------------------------------------------------------------
 * Public API
 * ---------------------------------------------------------------------- */

void chanhop_stop(void)
{
	atomic_store(&g_chanhop_stop, 1);
}

int chanhop_start(const chanhop_config_t *config)
{
	if (!config || !config->ifname || config->dwell_ms == 0) {
		errno = EINVAL;
		return -1;
	}

	/* build channel list from config */
	uint32_t channels_24[] = CHANHOP_24GHZ_CHANNELS;
	uint32_t channels_50[] = CHANHOP_50GHZ_CHANNELS;

	uint32_t chlist[CHANHOP_24GHZ_COUNT + CHANHOP_50GHZ_COUNT];
	int      chcount = 0;

	if (config->band == CHANHOP_BAND_24GHZ || config->band == CHANHOP_BAND_BOTH) {
		memcpy(chlist + chcount, channels_24,
				CHANHOP_24GHZ_COUNT * sizeof(uint32_t));
		chcount += CHANHOP_24GHZ_COUNT;
	}
	if (config->band == CHANHOP_BAND_50GHZ || config->band == CHANHOP_BAND_BOTH) {
		memcpy(chlist + chcount, channels_50,
				CHANHOP_50GHZ_COUNT * sizeof(uint32_t));
		chcount += CHANHOP_50GHZ_COUNT;
	}

	if (chcount == 0) {
		errno = EINVAL;
		return -1;
	}

	int sock = socket(AF_NETLINK, SOCK_RAW, NETLINK_GENERIC);
	if (sock < 0)
		return -1;

	struct sockaddr_nl local = {
		.nl_family = AF_NETLINK,
		.nl_pid    = (uint32_t)getpid(),
	};

	if (bind(sock, (struct sockaddr *)&local, sizeof(local)) < 0) {
		close(sock);
		return -1;
	}

	int nl80211_id = nl80211_get_family_id(sock);
	if (nl80211_id < 0) {
		close(sock);
		return -1;
	}

	uint32_t ifindex = if_nametoindex(config->ifname);
	if (ifindex == 0) {
		close(sock);
		return -1;
	}

	struct timespec dwell = {
		.tv_sec  = config->dwell_ms / 1000,
		.tv_nsec = (long)(config->dwell_ms % 1000) * 1000000L,
	};

	int idx = 0;
	while (!atomic_load(&g_chanhop_stop)) {
		set_channel(sock, nl80211_id, ifindex, chlist[idx]);
		idx = (idx + 1) % chcount;

		/* interruptible sleep — wake early if stopped */
		struct timespec rem = dwell;
		while (nanosleep(&rem, &rem) == -1 && errno == EINTR) {
			if (atomic_load(&g_chanhop_stop))
				goto done;
		}
	}

done:
	close(sock);
	return 0;
}
