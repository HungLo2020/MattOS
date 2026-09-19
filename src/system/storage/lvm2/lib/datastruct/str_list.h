/*
 * Copyright (C) 2003-2004 Sistina Software, Inc. All rights reserved.
 * Copyright (C) 2004-2012 Red Hat, Inc. All rights reserved.
 *
 * This file is part of LVM2.
 *
 * This copyrighted material is made available to anyone wishing to use,
 * modify, copy, or redistribute it subject to the terms and conditions
 * of the GNU Lesser General Public License v.2.1.
 *
 * You should have received a copy of the GNU Lesser General Public License
 * along with this program; if not, write to the Free Software Foundation,
 * Inc., 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA
 */

#ifndef LVM_STR_LIST_H
#define LVM_STR_LIST_H

struct dm_list;
struct dm_pool;

/* All int-returning functions return 1 on success/match, 0 on failure/no-match. */

/* Allocate and initialise an empty string list from pool. NULL on failure. */
struct dm_list *str_list_create(struct dm_pool *mem);

/* Add str to sll if not already present. 1 success, 0 failure. */
int str_list_add(struct dm_pool *mem, struct dm_list *sll, const char *str);

/* Append all entries from sll2 to sll, skipping duplicates. */
int str_list_add_list(struct dm_pool *mem, struct dm_list *sll, const struct dm_list *sll2);

/* Add str to tail without checking for duplicates. */
int str_list_add_no_dup_check(struct dm_pool *mem, struct dm_list *sll, const char *str);

/* Add str to head without checking for duplicates. */
int str_list_prepend_no_dup_check(struct dm_pool *mem, struct dm_list *sll, const char *str);

/* Remove all entries matching str. Returns 1 if any entry was removed, 0 if not found. */
int str_list_del(struct dm_list *sll, const char *str);

/* Remove all entries, leaving sll as an empty initialised list. */
void str_list_wipe(struct dm_list *sll);

/* Return 1 if str is present in sll, 0 otherwise. */
int str_list_match_item(const struct dm_list *sll, const char *str);

/*
 * Return 1 if at least one string appears in both sll and sll2.
 * If str_matched is non-NULL, set it to the first matching string from sll.
 */
int str_list_match_list(const struct dm_list *sll, const struct dm_list *sll2, const char **str_matched);

/*
 * Return 1 if both lists contain exactly the same set of strings.
 * Assumes no duplicates in either list; callers using str_list_add_no_dup_check()
 * or str_list_prepend_no_dup_check() must enforce this themselves.
 */
int str_list_lists_equal(const struct dm_list *sll, const struct dm_list *sll2);

/*
 * Copy sllold into sllnew, duplicating strings into mem. sllnew is
 * initialised by this call; the caller must supply allocated (not
 * pre-initialised) storage for the dm_list header.
 */
int str_list_dup(struct dm_pool *mem, struct dm_list *sllnew,
		 const struct dm_list *sllold);

/* Join all strings in list with delim into a single pool-allocated string. NULL on failure. */
char *str_list_to_str(struct dm_pool *mem, const struct dm_list *list, const char *delim);

/* Split str on delim into a pool-allocated string list. NULL on failure. */
struct dm_list *str_to_str_list(struct dm_pool *mem, const char *str, const char *delim, int ignore_multiple_delim);

#endif
