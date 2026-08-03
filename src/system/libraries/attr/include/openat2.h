#ifndef MISC_OPENAT2_H
#define MISC_OPENAT2_H

#include "config.h"
#include <stdint.h>

#ifdef USE_OPENAT2
#include <linux/openat2.h>

#ifndef HAVE_OPENAT2
int openat2(int dirfd, const char *pathname, const struct open_how *how,
	    size_t size);
#endif
#endif

#endif /* MISC_OPENAT2_H */
