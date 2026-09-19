/*
 * Copyright (C) 2001-2004 Sistina Software, Inc. All rights reserved.
 * Copyright (C) 2004-2011 Red Hat, Inc. All rights reserved.
 *
 * This file is part of the device-mapper userspace tools.
 *
 * This copyrighted material is made available to anyone wishing to use,
 * modify, copy, or redistribute it subject to the terms and conditions
 * of the GNU Lesser General Public License v.2.1.
 *
 * You should have received a copy of the GNU Lesser General Public License
 * along with this program; if not, write to the Free Software Foundation,
 * Inc., 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA
 */

#include "libdm/misc/dmlib.h"
#include <stdlib.h>

struct dm_hash_node {
	struct dm_hash_node *next;
	void *data;
	unsigned data_len;
	unsigned keylen;
	unsigned hash;
	char key[0];
};

struct dm_hash_table {
	unsigned num_nodes;
	unsigned num_hint;
	unsigned mask_slots;    /* (slots - 1) -> used as hash mask */
	unsigned collisions;    /* Collisions of hash keys */
	unsigned search;        /* How many keys were searched */
	unsigned found;         /* How many nodes were found */
	unsigned same_hash;     /* Was there a collision with same masked hash and len ? */
	struct dm_hash_node **slots;
};

#undef get16bits
#if (defined(__GNUC__) && (defined(__i386__) || defined(__x86_64__)))
#define get16bits(d) (*((const uint16_t *) (d)))
#endif

#if !defined (get16bits)
#define get16bits(d) ((((uint32_t)(((const uint8_t *)(d))[1])) << 8)\
                       +(uint32_t)(((const uint8_t *)(d))[0]) )
#endif

static struct dm_hash_node *_create_node(const void *key, unsigned len)
{
	struct dm_hash_node *n = malloc(sizeof(*n) + len);

	if (n) {
		memcpy(n->key, key, len);
		n->keylen = len;
	}

	return n;
}

/*
 * Adapted Bob Jenkins hash to read by 2 bytes if possible.
 * https://secure.wikimedia.org/wikipedia/en/wiki/Jenkins_hash_function
 *
 * Reduces amount of hash collisions
 */
static unsigned _hash(const void *key, unsigned len)
{
	const uint8_t *str = (uint8_t*) key;
	unsigned hash = 0, i;
	unsigned sz = len / 2;

	for(i = 0; i < sz; ++i) {
		hash += get16bits(str + 2 * i);
		hash += (hash << 10);
		hash ^= (hash >> 6);
	}

	if (len & 1) {
		hash += str[len - 1];
		hash += (hash << 10);
		hash ^= (hash >> 6);
	}

	hash += (hash << 3);
	hash ^= (hash >> 11);
	hash += (hash << 15);

	return hash;
}

struct dm_hash_table *dm_hash_create(unsigned size_hint)
{
	size_t len;
	unsigned new_size = 16u;
	struct dm_hash_table *hc = dm_zalloc(sizeof(*hc));

	if (!hc) {
		log_error("Failed to allocate memory for hash.");
		return 0;
	}

	hc->num_hint = size_hint;

	/* round size hint up to a power of two */
	while (new_size < size_hint)
		new_size = new_size << 1;

	hc->mask_slots = new_size - 1;
	len = sizeof(*(hc->slots)) * new_size;
	if (!(hc->slots = dm_zalloc(len))) {
		dm_free(hc);
		log_error("Failed to allocate slots for hash.");
		return 0;
	}

	return hc;
}

static void _free_nodes(struct dm_hash_table *t)
{
	struct dm_hash_node *c, *n;
	unsigned i;

#ifdef DEBUG
	log_debug("Free hash hint:%u slots:%u nodes:%u (s:%u f:%u c:%u h:%u).",
		  t->num_hint, t->mask_slots + 1, t->num_nodes,
		  t->search, t->found, t->collisions, t->same_hash);
#endif

	if (!t->num_nodes)
		return;

	for (i = 0; i <= t->mask_slots; i++)
		for (c = t->slots[i]; c; c = n) {
			n = c->next;
			free(c);
		}
}

void dm_hash_destroy(struct dm_hash_table *t)
{
	_free_nodes(t);
	dm_free(t->slots);
	dm_free(t);
}

static struct dm_hash_node **_findh(struct dm_hash_table *t, const void *key,
				    uint32_t len, unsigned hash)
{
	struct dm_hash_node **c;

	++t->search;
	for (c = &t->slots[hash & t->mask_slots]; *c; c = &((*c)->next)) {
		if ((*c)->keylen == len && (*c)->hash == hash) {
			if (!memcmp(key, (*c)->key, len)) {
				++t->found;
				break;
			}
			++t->same_hash;
		}
		++t->collisions;
	}

	return c;
}

static struct dm_hash_node **_find(struct dm_hash_table *t, const void *key,
				   uint32_t len)
{
	return _findh(t, key, len, _hash(key, len));
}

void *dm_hash_lookup_binary(struct dm_hash_table *t, const void *key,
			    uint32_t len)
{
	struct dm_hash_node **c = _find(t, key, len);

	return *c ? (*c)->data : 0;
}

int dm_hash_insert_binary(struct dm_hash_table *t, const void *key,
			  uint32_t len, void *data)
{
	unsigned hash = _hash(key, len);
	struct dm_hash_node **c = _findh(t, key, len, hash);

	if (*c)
		(*c)->data = data;
	else {
		struct dm_hash_node *n = _create_node(key, len);

		if (!n)
			return 0;

		n->data = data;
		n->hash = hash;
		n->next = 0;
		*c = n;
		t->num_nodes++;
	}

	return 1;
}

void dm_hash_remove_binary(struct dm_hash_table *t, const void *key,
			uint32_t len)
{
	struct dm_hash_node **c = _find(t, key, len);

	if (*c) {
		struct dm_hash_node *old = *c;
		*c = (*c)->next;
		free(old);
		t->num_nodes--;
	}
}

void *dm_hash_lookup(struct dm_hash_table *t, const char *key)
{
	return dm_hash_lookup_binary(t, key, strlen(key) + 1);
}

int dm_hash_insert(struct dm_hash_table *t, const char *key, void *data)
{
	return dm_hash_insert_binary(t, key, strlen(key) + 1, data);
}

void dm_hash_remove(struct dm_hash_table *t, const char *key)
{
	dm_hash_remove_binary(t, key, strlen(key) + 1);
}

static struct dm_hash_node **_find_str_with_val(struct dm_hash_table *t,
					        const void *key, const void *val,
					        uint32_t len, uint32_t val_len)
{
	struct dm_hash_node **c;
	unsigned h;

	h = _hash(key, len) & t->mask_slots;

	for (c = &t->slots[h]; *c; c = &((*c)->next)) {
		if ((*c)->keylen != len)
			continue;

		if (!memcmp(key, (*c)->key, len) && (*c)->data) {
			if (((*c)->data_len == val_len) &&
			    !memcmp(val, (*c)->data, val_len))
				return c;
		}
	}

	return NULL;
}

int dm_hash_insert_allow_multiple(struct dm_hash_table *t, const char *key,
				  const void *val, uint32_t val_len)
{
	struct dm_hash_node *n;
	struct dm_hash_node *first;
	int len = strlen(key) + 1;
	unsigned h;

	n = _create_node(key, len);
	if (!n)
		return 0;

	n->data = (void *)val;
	n->data_len = val_len;
	n->hash = _hash(key, len);

	h = n->hash & t->mask_slots;

	first = t->slots[h];

	if (first)
		n->next = first;
	else
		n->next = 0;
	t->slots[h] = n;

	t->num_nodes++;
	return 1;
}

/*
 * Look through multiple entries with the same key for one that has a
 * matching val and return that.  If none have matching val, return NULL.
 */
void *dm_hash_lookup_with_val(struct dm_hash_table *t, const char *key,
			      const void *val, uint32_t val_len)
{
	struct dm_hash_node **c;

	c = _find_str_with_val(t, key, val, strlen(key) + 1, val_len);

	return (c && *c) ? (*c)->data : 0;
}

/*
 * Look through multiple entries with the same key for one that has a
 * matching val and remove that.
 */
void dm_hash_remove_with_val(struct dm_hash_table *t, const char *key,
			     const void *val, uint32_t val_len)
{
	struct dm_hash_node **c;

	c = _find_str_with_val(t, key, val, strlen(key) + 1, val_len);

	if (c && *c) {
		struct dm_hash_node *old = *c;
		*c = (*c)->next;
		free(old);
		t->num_nodes--;
	}
}

/*
 * Look up the value for a key and count how many
 * entries have the same key.
 *
 * If no entries have key, return NULL and set count to 0.
 *
 * If one entry has the key, the function returns the val,
 * and sets count to 1.
 *
 * If N entries have the key, the function returns the val
 * from the first entry, and sets count to N.
 */
void *dm_hash_lookup_with_count(struct dm_hash_table *t, const char *key, int *count)
{
	struct dm_hash_node **c;
	struct dm_hash_node **c1 = NULL;
	uint32_t len = strlen(key) + 1;
	unsigned h;

	*count = 0;

	h = _hash(key, len) & t->mask_slots;

	for (c = &t->slots[h]; *c; c = &((*c)->next)) {
		if ((*c)->keylen != len)
			continue;

		if (!memcmp(key, (*c)->key, len)) {
			(*count)++;
			if (!c1)
				c1 = c;
		}
	}

	if (!c1)
		return NULL;
	else
		return *c1 ? (*c1)->data : 0;
}

unsigned dm_hash_get_num_entries(struct dm_hash_table *t)
{
	return t->num_nodes;
}

void dm_hash_iter(struct dm_hash_table *t, dm_hash_iterate_fn f)
{
	struct dm_hash_node *c, *n;
	unsigned i;

	for (i = 0; i <= t->mask_slots; i++)
		for (c = t->slots[i]; c; c = n) {
			n = c->next;
			f(c->data);
		}
}

void dm_hash_wipe(struct dm_hash_table *t)
{
	_free_nodes(t);
	memset(t->slots, 0, sizeof(struct dm_hash_node *) * (t->mask_slots + 1));
	t->num_nodes = t->collisions = t->search = t->same_hash = 0u;
}

char *dm_hash_get_key(struct dm_hash_table *t __attribute__((unused)),
		      struct dm_hash_node *n)
{
	return n->key;
}

void *dm_hash_get_data(struct dm_hash_table *t __attribute__((unused)),
		       struct dm_hash_node *n)
{
	return n->data;
}

static struct dm_hash_node *_next_slot(struct dm_hash_table *t, unsigned s)
{
	struct dm_hash_node *c = NULL;
	unsigned i;

	for (i = s; i <= t->mask_slots && !c; i++)
		c = t->slots[i];

	return c;
}

struct dm_hash_node *dm_hash_get_first(struct dm_hash_table *t)
{
	return _next_slot(t, 0);
}

struct dm_hash_node *dm_hash_get_next(struct dm_hash_table *t, struct dm_hash_node *n)
{
	return n->next ? n->next : _next_slot(t, (n->hash & t->mask_slots) + 1);
}
