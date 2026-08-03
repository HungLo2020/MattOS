#ifndef VISIBILITY_HIDDEN_H
#define VISIBILITY_HIDDEN_H

/* Mark library internal functions as hidden */
#if defined(HAVE_VISIBILITY_ATTRIBUTE)
# define hidden __attribute__((visibility("hidden")))
#else
# define hidden /* hidden */
#endif

#endif /* VISIBILITY_HIDDEN_H */
