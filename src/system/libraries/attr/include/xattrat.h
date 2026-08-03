#ifndef MISC_XATTRAT_H
#define MISC_XATTRAT_H

#include "config.h"

#ifndef HAVE_GETXATTRAT
ssize_t getxattrat(int dirfd, const char *path, int at_flags, const char *name,
		   void *value, size_t size);
#endif

#ifndef HAVE_SETXATTRAT
int setxattrat(int dirfd, const char *path, int at_flags, const char *name,
	       const void *value, size_t size, int flags);
#endif

#ifndef HAVE_LISTXATTRAT
ssize_t listxattrat(int dirfd, const char *path, int at_flags, char *list,
		    size_t size);
#endif

#ifndef HAVE_REMOVEXATTRAT
int removexattrat(int dirfd, const char *path, int at_flags, const char *name);
#endif

#endif /* MISC_XATTRAT_H */
