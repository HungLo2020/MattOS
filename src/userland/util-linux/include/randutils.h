/*
 * SPDX-License-Identifier: BSD-3-Clause
 */
#ifndef UTIL_LINUX_RANDUTILS
#define UTIL_LINUX_RANDUTILS

/* command flags */
typedef enum {
	UL_RAND_WEAK = 0,	/* computed with libc functionality */
	UL_RAND_GETRANDOM,	/* retrieved from getrandom(2) */
	UL_RAND_RANDOM,		/* read from /dev/random */
	UL_RAND_URANDOM		/* read from /dev/urandom */
} ul_random_src_t;

/* getrandom(2) based with fallback to /dev/(u)random and libc randomness */
extern ul_random_src_t ul_random_get_bytes(void *buf, size_t nbytes);
extern const char *ul_random_tell_source(ul_random_src_t);

#endif
