/*
 * Copyright (C) 2026 Red Hat, Inc. All rights reserved.
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

#include "lib/misc/lib.h"
#include "lib/metadata/metadata.h"

#include <math.h>

/*
 * Compute VDO data blocks available for storing compressed/deduplicated content.
 *
 * Replicates the vdoformat calculation: given a pool's physical size and
 * VDO parameters, compute the usable data blocks at 1:1 mapping
 * (accounting for UDS index, slab overhead, and block map forest).
 */

/* UDS index geometry constants */
#define UDS_BLOCK_SIZE			4096U
#define UDS_BYTES_PER_RECORD		32U
#define UDS_BYTES_PER_PAGE		32768U
#define UDS_RECORDS_PER_PAGE		(UDS_BYTES_PER_PAGE / UDS_BYTES_PER_RECORD)  /* 1024 */
#define UDS_DEFAULT_CHAPTERS		1024U
#define UDS_SMALL_RPC			64U
#define UDS_DEFAULT_RPC			256U
#define UDS_DELTA_LIST_SIZE		256U
#define UDS_MAX_ZONES			16U
#define UDS_MAX_SAVES			2U
#define UDS_HEADER_SIZE			19U	/* immutable delta index header */
#define UDS_CHAPTER_MEAN_DELTA_BITS	16U	/* chapter mean_delta = 1<<16 */
#define UDS_VOLUME_INDEX_MEAN_DELTA	4096U
#define UDS_SPARSE_SAMPLE_RATE		32U

/* UDS on-disk structure sizes */
#define UDS_SIZEOF_DI_HEADER		40U
#define UDS_SIZEOF_DL_SAVE_INFO		8U
#define UDS_SIZEOF_DP_HEADER		20U
#define UDS_SIZEOF_SUB_INDEX_DATA	40U
#define UDS_SIZEOF_VI_DATA		12U	/* magic[8] + u32 sparse_sample_rate */
#define UDS_PAGE_MAP_MAGIC_LEN		8U
#define UDS_OPEN_CHAPTER_MAGIC_LEN	(sizeof("ALBOC") - 1)
#define UDS_OPEN_CHAPTER_VER_LEN	(sizeof("02.00") - 1)

/* VDO layout constants */
#define VDO_BLOCK_MAP_TREE_HEIGHT	5U
#define VDO_BLOCK_MAP_ROOT_COUNT	60U
#define VDO_BLOCK_MAP_ENTRIES_PER_PAGE	812U
#define VDO_SLAB_JOURNAL_SIZE		224U	/* blocks per slab for journal */
#define VDO_SLAB_SUMMARY_BLOCKS		64U
#define VDO_RECOVERY_JOURNAL_SIZE	32768U
#define VDO_COUNTS_PER_BLOCK		4032U	/* reference counts per block */

static unsigned _bits_per(unsigned n)
{
	if (!n)
		return 0;

	return 32 - clz(n);
}

static unsigned _delta_min_bits(unsigned mean_delta)
{
	unsigned incr_keys = (unsigned)round(log(2.0) * mean_delta);

	return _bits_per(incr_keys + 1);
}

/* Total bits for one chapter's delta index */
static uint64_t _delta_index_size(uint64_t entry_count, unsigned mean_delta,
				   unsigned payload_bits)
{
	unsigned min_bits = _delta_min_bits(mean_delta);

	return entry_count * (payload_bits + min_bits + 1) + entry_count / 2;
}

/* Number of delta index pages needed for one chapter */
static unsigned _delta_index_pages(uint64_t entry_count, unsigned list_count,
				   unsigned mean_delta, unsigned payload_bits)
{
	uint64_t total_bits = _delta_index_size(entry_count, mean_delta, payload_bits);
	unsigned bits_per_list = (unsigned)(total_bits / list_count);
	unsigned bits_per_page;

	total_bits += (uint64_t)list_count * UDS_HEADER_SIZE;
	bits_per_page = (UDS_BYTES_PER_PAGE - UDS_SIZEOF_DP_HEADER) * 8 -
			UDS_HEADER_SIZE - bits_per_list;

	return (unsigned)dm_div_up(total_bits, bits_per_page);
}

/* Save bytes for one volume sub-index */
static uint64_t _sub_index_save_bytes(unsigned records_per_chapter,
				      unsigned chapters, unsigned vi_mean)
{
	uint64_t delta_list_records = (uint64_t)records_per_chapter * chapters;
	unsigned list_count = (unsigned)(delta_list_records / UDS_DELTA_LIST_SIZE);
	unsigned addr_bits, chapter_bits, invalid_chapters;
	unsigned chapters_in_vi;
	uint64_t entries_in_vi, addr_span;
	unsigned mean_delta;
	uint64_t chapter_size_bits, index_size_bytes, memory_size;
	uint64_t zone_memory, di_save;

	if (list_count < UDS_MAX_ZONES * UDS_MAX_ZONES)
		list_count = UDS_MAX_ZONES * UDS_MAX_ZONES;

	addr_bits = _bits_per(vi_mean * UDS_DELTA_LIST_SIZE - 1);
	chapter_bits = _bits_per(chapters - 1);
	invalid_chapters = chapters / 256;
	if (invalid_chapters < 2)
		invalid_chapters = 2;
	chapters_in_vi = chapters + invalid_chapters;
	entries_in_vi = (uint64_t)records_per_chapter * chapters_in_vi;
	addr_span = (uint64_t)list_count << addr_bits;
	mean_delta = (unsigned)(addr_span / entries_in_vi);

	chapter_size_bits = _delta_index_size(records_per_chapter, mean_delta,
					      chapter_bits);
	index_size_bytes = (chapter_size_bits * chapters_in_vi) / 8;
	/* add 6% headroom to reduce delta list rebalancing when index is full */
	memory_size = index_size_bytes * 106 / 100;

	/* zone_mem(1, memory_size): round up to 64K */
	zone_memory = dm_div_up(memory_size, 65536U) * 65536U;

	/* delta_index save = header + list_count*(save_info+1) + zone_memory */
	di_save = UDS_SIZEOF_DI_HEADER + zone_memory +
		  (uint64_t)list_count * (UDS_SIZEOF_DL_SAVE_INFO + 1);

	/* sub_index save = sub_index_data + list_count*8 + di_save */
	return UDS_SIZEOF_SUB_INDEX_DATA + (uint64_t)list_count * 8 + di_save;
}

/*
 * Compute total UDS index size in 4K blocks.
 *
 * index_memory_mb: UDS memory config (256, 512, 768, or N*1024)
 * use_sparse:      nonzero for sparse index
 */
static uint64_t _uds_index_blocks(unsigned index_memory_mb, int use_sparse)
{
	unsigned base_chapters, rpc, chapters, sparse_chapters = 0;
	unsigned records_per_chapter, chapter_payload_bits;
	unsigned chapter_dl_bits, delta_lists_per_chapter;
	unsigned ippc, pages_per_chapter;
	uint64_t pages_per_volume, bytes_per_volume, volume_blocks;
	uint64_t vi_save_bytes, vi_blocks;
	unsigned page_map_blocks, open_chapter_blocks;
	unsigned save_blocks;

	/* Map memory config to chapters and records-per-page */
	if (index_memory_mb <= 256) {
		base_chapters = UDS_DEFAULT_CHAPTERS;
		rpc = UDS_SMALL_RPC;
	} else if (index_memory_mb <= 512) {
		base_chapters = UDS_DEFAULT_CHAPTERS;
		rpc = 2 * UDS_SMALL_RPC;
	} else if (index_memory_mb <= 768) {
		base_chapters = UDS_DEFAULT_CHAPTERS;
		rpc = 3 * UDS_SMALL_RPC;
	} else {
		base_chapters = ((index_memory_mb + 1023) / 1024) * UDS_DEFAULT_CHAPTERS;
		rpc = UDS_DEFAULT_RPC;
	}

	/* sparse index: 10x total chapters, 95% sparse (19/2 sparse per base chapter) */
	if (use_sparse) {
		sparse_chapters = (19 * base_chapters) / 2;
		chapters = base_chapters * 10;
	} else {
		chapters = base_chapters;
	}

	/* Geometry */
	records_per_chapter = UDS_RECORDS_PER_PAGE * rpc;
	chapter_payload_bits = _bits_per(rpc - 1);
	chapter_dl_bits = _bits_per((records_per_chapter - 1) | 077) - 6;
	delta_lists_per_chapter = 1U << chapter_dl_bits;
	ippc = _delta_index_pages(records_per_chapter, delta_lists_per_chapter,
				  1U << UDS_CHAPTER_MEAN_DELTA_BITS,
				  chapter_payload_bits);
	pages_per_chapter = ippc + rpc;
	pages_per_volume = (uint64_t)pages_per_chapter * chapters;
	bytes_per_volume = UDS_BYTES_PER_PAGE * (pages_per_volume + 1);
	volume_blocks = bytes_per_volume / UDS_BLOCK_SIZE;

	/* Volume index save blocks */
	if (!use_sparse) {
		vi_save_bytes = _sub_index_save_bytes(records_per_chapter,
						     chapters,
						     UDS_VOLUME_INDEX_MEAN_DELTA);
	} else {
		unsigned hook_rec = records_per_chapter / UDS_SPARSE_SAMPLE_RATE;
		unsigned non_hook_rec = records_per_chapter - hook_rec;
		unsigned dense_chapters = chapters - sparse_chapters;
		uint64_t hook_bytes, non_hook_bytes;

		hook_bytes = _sub_index_save_bytes(hook_rec, chapters,
						   UDS_VOLUME_INDEX_MEAN_DELTA);
		non_hook_bytes = _sub_index_save_bytes(non_hook_rec,
						       dense_chapters,
						       UDS_VOLUME_INDEX_MEAN_DELTA);
		vi_save_bytes = UDS_SIZEOF_VI_DATA + hook_bytes + non_hook_bytes;
	}
	vi_blocks = dm_div_up(vi_save_bytes + UDS_SIZEOF_DL_SAVE_INFO,
			      UDS_BLOCK_SIZE) + UDS_MAX_ZONES;

	/* Page map and open chapter save blocks */
	page_map_blocks =
	    (unsigned)dm_div_up(UDS_PAGE_MAP_MAGIC_LEN + 8 + (uint64_t)chapters * (ippc - 1) * 2,
				UDS_BLOCK_SIZE);
	open_chapter_blocks =
	    (unsigned)dm_div_up(UDS_OPEN_CHAPTER_MAGIC_LEN + UDS_OPEN_CHAPTER_VER_LEN + 4 +
				(uint64_t)records_per_chapter * UDS_BYTES_PER_RECORD,
				UDS_BLOCK_SIZE);

	save_blocks = 1 + vi_blocks + page_map_blocks + open_chapter_blocks;

	/* total = header(3) + volume + MAX_SAVES * save */
	return 3 + volume_blocks + UDS_MAX_SAVES * save_blocks;
}

/*
 * Compute block map forest overhead: the number of blocks consumed
 * by the 5-level block map tree when mapping data_blocks entries.
 * Tree roots (level 3) and super roots (level 4) are not allocated
 * from slabs, so they are excluded.
 */
static uint64_t _forest_overhead(uint64_t data_blocks)
{
	uint64_t leaf_pages, level_size, non_leaves, leaves;
	unsigned levels[VDO_BLOCK_MAP_TREE_HEIGHT];
	unsigned h;

	leaf_pages = dm_div_up(data_blocks, VDO_BLOCK_MAP_ENTRIES_PER_PAGE);
	level_size = dm_div_up(leaf_pages, VDO_BLOCK_MAP_ROOT_COUNT);

	for (h = 0; h < VDO_BLOCK_MAP_TREE_HEIGHT; h++) {
		level_size = dm_div_up(level_size, VDO_BLOCK_MAP_ENTRIES_PER_PAGE);
		levels[h] = (unsigned)level_size;
	}

	non_leaves = 0;
	for (h = 0; h < VDO_BLOCK_MAP_TREE_HEIGHT; h++)
		non_leaves += (uint64_t)levels[h] * VDO_BLOCK_MAP_ROOT_COUNT;

	/* Exclude tree roots and super roots (not from slabs) */
	non_leaves -= (levels[VDO_BLOCK_MAP_TREE_HEIGHT - 2] +
		       levels[VDO_BLOCK_MAP_TREE_HEIGHT - 1]) *
	    VDO_BLOCK_MAP_ROOT_COUNT;

	leaves = dm_div_up(data_blocks - non_leaves,
			   VDO_BLOCK_MAP_ENTRIES_PER_PAGE);

	return non_leaves + leaves;
}

int vdo_pool_info(uint64_t pool_size_sectors,
		  const struct dm_vdo_target_params *vtp,
		  struct vdo_pool_info *info)
{
	uint64_t pool_4k = pool_size_sectors / DM_VDO_BLOCK_SIZE;
	unsigned slab_4k = (unsigned)((uint64_t)vtp->slab_size_mb * (1048576 / UDS_BLOCK_SIZE));
	uint64_t offset, depot_blocks;
	unsigned ref_blocks;
	uint64_t total_data, forest;

	memset(info, 0, sizeof(*info));

	info->uds_blocks = _uds_index_blocks(vtp->index_memory_size_mb,
					      vtp->use_sparse_index);

	/* Minimum pool: geometry(1) + UDS + super(1) + roots + slab_summary + recovery_journal + 1 slab */
	info->min_pool_sectors = (info->uds_blocks + 2 + VDO_BLOCK_MAP_ROOT_COUNT +
				  VDO_SLAB_SUMMARY_BLOCKS + VDO_RECOVERY_JOURNAL_SIZE +
				  slab_4k) * DM_VDO_BLOCK_SIZE;

	/* Layout: geometry(1) + UDS + super(1) + roots + slab_summary + recovery_journal */
	offset = info->uds_blocks + 2;
	if (pool_4k <= offset + VDO_BLOCK_MAP_ROOT_COUNT +
		       VDO_SLAB_SUMMARY_BLOCKS + VDO_RECOVERY_JOURNAL_SIZE)
		return 0;

	depot_blocks = pool_4k - offset - VDO_BLOCK_MAP_ROOT_COUNT -
		       VDO_SLAB_SUMMARY_BLOCKS - VDO_RECOVERY_JOURNAL_SIZE;

	info->slab_count = (unsigned)(depot_blocks / slab_4k);
	if (!info->slab_count)
		return 0;

	/* Slab layout: journal(224) + ref_blocks + data_blocks = slab_4k */
	ref_blocks = (unsigned)dm_div_up(slab_4k - VDO_SLAB_JOURNAL_SIZE,
					 VDO_COUNTS_PER_BLOCK + 1);
	info->data_per_slab = slab_4k - VDO_SLAB_JOURNAL_SIZE - ref_blocks;
	total_data = (uint64_t)info->data_per_slab * info->slab_count;

	forest = _forest_overhead(total_data);
	if (forest >= total_data)
		return 0;

	info->data_blocks = total_data - forest;

	return 1;
}
