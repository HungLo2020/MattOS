#include <config.h>

#define _GNU_SOURCE
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>

#include "xdg-user-dir-lookup.h"

int
main (int argc, char *argv[])
{
  char *dir;
  dir = xdg_user_dir_lookup (argc >= 2 ? argv[1] : "");
  if (dir != NULL)
    {
      printf ("%s\n", dir);
      free (dir);
      return 0;
    }
  return 1;
}
