/* SPDX-License-Identifier: GPL-2.0 */
#ifndef __NAT64_H__
#define __NAT64_H__

#include <linux/in6.h>

struct clat_config {
    struct in6_addr local_v6;
    struct in6_addr pref64;
    struct in_addr  local_v4;
    unsigned        pref64_len;
};

struct clat_stats {
    /* egress: v4 to v6 */
    __u64 egress_tcp;
    __u64 egress_udp;
    __u64 egress_icmp;
    __u64 egress_other;
    __u64 egress_dropped;
    /* ingress: v6 to v4 */
    __u64 ingress_tcp;
    __u64 ingress_udp;
    __u64 ingress_icmp;
    __u64 ingress_other;
    __u64 ingress_fragment;
    __u64 ingress_dropped;
};

#endif
