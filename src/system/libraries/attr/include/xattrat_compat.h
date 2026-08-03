#ifndef XATTRAT_COMPAT_H
#define XATTRAT_COMPAT_H

ssize_t getxattrat_compat(int dirfd, const char *path, int at_flags,
			  const char *name, void *value, size_t size);

int setxattrat_compat(int dirfd, const char *path, int at_flags,
		      const char *name, const void *value, size_t size,
		      int flags);

ssize_t listxattrat_compat(int dirfd, const char *path, int at_flags,
			   char *list, size_t size);

int removexattrat_compat(int dirfd, const char *path, int at_flags,
			 const char *name);

#ifndef DECLARATIONS_ONLY
# define getxattrat getxattrat_compat
# define setxattrat setxattrat_compat
# define listxattrat listxattrat_compat
# define removexattrat removexattrat_compat
#endif

#endif /* XATTRAT_COMPAT_H */
