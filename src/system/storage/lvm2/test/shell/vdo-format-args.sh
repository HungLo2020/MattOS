#!/usr/bin/env bash

# Copyright (C) 2026 Red Hat, Inc. All rights reserved.
#
# This copyrighted material is made available to anyone wishing to use,
# modify, copy, or redistribute it subject to the terms and conditions
# of the GNU General Public License v.2.
#
# You should have received a copy of the GNU General Public License
# along with this program; if not, write to the Free Software Foundation,
# Inc., 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA

# Test that VDO formatting settings (use_sparse_index, slab_size_mb)
# are correctly applied and reflected in the running VDO device.
# Tests kernel formatting, userspace vdoformat and kernel format fallback.

SKIP_WITH_LVMPOLLD=1

. lib/inittest

aux have_vdo 6 2 0 || skip

# Skip on 7.1-rc* kernels - buggy dm-vdo 9.2 that crashes on format
case "$(uname -r)" in
7.1.*rc[0123]*) skip "Buggy dm-vdo in 7.1-rc kernel" ;;
esac

test "$(aux total_mem)" -gt 524288 || skip "Not enough RAM for this test"

# Detect if kernel supports direct formatting
KERNEL_FORMAT=0
if aux target_at_least dm-vdo 9 2 0; then
	KERNEL_FORMAT=1
fi

# Determine stats tool - dmvdostats needs dm-vdo >= 8.2.0 (message stats),
# older kernels may have standalone vdostats tool using sysfs
STATS_CMD=""
if aux target_at_least dm-vdo 8 2 0; then
	STATS_CMD="dmvdostats"
elif which vdostats >/dev/null 2>&1; then
	STATS_CMD="vdostats"
fi

vdo_stats_() {
	test -n "$STATS_CMD" || return 0
	aux wait_for_vdo_index "$VPOOL"
	"$STATS_CMD" -v "$VPOOL" | awk '!/ 0$/' | tee out
	grep "operating mode" out
}

vdo_stat_() {
	awk -v s="$1" '$0 ~ s {print $NF}' out
}

test_vdo_format() {
	local use_kernel=$1
	local format_msg=$2

	# Create VDO with sparse index
	lvcreate --vdo -L25G -V50G -n $lv1 $vg/vdopool \
		--vdosettings "use_sparse_index=1 use_kernel_format=$use_kernel"

	check lv_field $vg/vdopool vdo_use_sparse_index "enabled"

	vdo_stats_

	if test -n "$STATS_CMD"; then
		SPARSE_MEM=$(vdo_stat_ "KVDO module bytes used")
		SPARSE_SLABS=$(vdo_stat_ "slab count")
		test -n "$SPARSE_MEM"
		test -n "$SPARSE_SLABS"
	fi

	# Deactivate and reactivate to verify on-disk format consistency
	lvchange -an $vg/$lv1
	lvchange -ay $vg/$lv1

	vdo_stats_

	lvremove -ff $vg

	# Create VDO without sparse index (default dense)
	lvcreate --vdo -L25G -V50G -n $lv1 $vg/vdopool \
		--vdosettings "use_kernel_format=$use_kernel"

	check lv_field $vg/vdopool vdo_use_sparse_index ""

	vdo_stats_

	if test -n "$STATS_CMD"; then
		DENSE_MEM=$(vdo_stat_ "KVDO module bytes used")
		DENSE_SLABS=$(vdo_stat_ "slab count")
		test -n "$DENSE_MEM"
		test -n "$DENSE_SLABS"

		# Sparse index uses larger on-disk index (10x more chapters for same indexMemory)
		# leaving fewer slabs for data
		test "$SPARSE_SLABS" -lt "$DENSE_SLABS" || \
			die "$format_msg: sparse slab count ($SPARSE_SLABS) should be less than dense ($DENSE_SLABS)"
	fi

	lvremove -ff $vg

	# Validate slab size setting with non-default value
	lvcreate --vdo -L25G -V50G -n $lv1 $vg/vdopool \
		--vdosettings "slab_size_mb=256 use_kernel_format=$use_kernel"

	check lv_field $vg/vdopool vdo_slab_size "256.00m"

	vdo_stats_

	if test -n "$STATS_CMD"; then
		LARGE_SLAB_COUNT=$(vdo_stat_ "slab count")
		test -n "$LARGE_SLAB_COUNT"

		# Doubling slab size should roughly halve the slab count
		test "$DENSE_SLABS" -gt "$LARGE_SLAB_COUNT" || \
			die "$format_msg: slab count with 256MB ($LARGE_SLAB_COUNT) should be less than with 128MB ($DENSE_SLABS)"
	fi

	lvremove -f $vg
}

# Test index_memory_size_mb quantization.
# Non-standard values round up to the next standard boundary:
#   0..256 -> 0.25 GiB,  257..512 -> 0.50,  513..768 -> 0.75,  769+ -> N GiB
# Both the pre-check (vdo_pool_info) and the dm table / vdoformat emit
# must agree on the quantized value, otherwise valid pools get rejected.
test_index_memory_size() {
	local use_kernel=$1
	local format_msg=$2

	# 256 MB -- standard boundary, 0.25 GiB bucket
	lvcreate --vdo -L25G -V50G -n $lv1 $vg/vdopool \
		--config 'allocation/vdo_index_memory_size_mb=256' \
		--vdosettings "use_kernel_format=$use_kernel"
	check lv_field $vg/vdopool vdo_index_memory_size "256.00m"
	lvremove -ff $vg

	# 300 MB -- non-standard, rounds up to 0.50 GiB (512 MB) bucket.
	# Before fix the pre-check used the 512 MB index size but the emit
	# sent 0.25 to the kernel, causing a false "Pool too small" rejection.
	lvcreate --vdo -L25G -V50G -n $lv1 $vg/vdopool \
		--config 'allocation/vdo_index_memory_size_mb=300' \
		--vdosettings "use_kernel_format=$use_kernel"
	check lv_field $vg/vdopool vdo_index_memory_size "300.00m"

	# Deactivate and reactivate to verify on-disk format consistency
	lvchange -an $vg/$lv1
	lvchange -ay $vg/$lv1
	lvremove -ff $vg

	# 512 MB -- standard boundary, 0.50 GiB bucket
	lvcreate --vdo -L25G -V50G -n $lv1 $vg/vdopool \
		--config 'allocation/vdo_index_memory_size_mb=512' \
		--vdosettings "use_kernel_format=$use_kernel"
	check lv_field $vg/vdopool vdo_index_memory_size "512.00m"
	lvremove -ff $vg

	# 600 MB -- non-standard, rounds up to 0.75 GiB (768 MB) bucket
	lvcreate --vdo -L25G -V50G -n $lv1 $vg/vdopool \
		--config 'allocation/vdo_index_memory_size_mb=600' \
		--vdosettings "use_kernel_format=$use_kernel"
	check lv_field $vg/vdopool vdo_index_memory_size "600.00m"
	lvremove -ff $vg

	# > 1 TiB -- must be rejected (max is 1048576 MB = 1 TiB)
	not lvcreate --vdo -L25G -V50G -n $lv1 $vg/vdopool \
		--config 'allocation/vdo_index_memory_size_mb=1048577' \
		--vdosettings "use_kernel_format=$use_kernel" 2>&1 | tee out
	grep "out of range" out

	# Pool too small for the given index config -- must print minimum required size
	if test "$use_kernel" -eq 1; then
		not lvcreate --vdo -L1G -V2G -n $lv1 $vg/vdopool \
			--config 'allocation/vdo_index_memory_size_mb=768' \
			--vdosettings "use_sparse_index=1 use_kernel_format=1" 2>&1 | tee out
		grep "needs at least" out
		lvremove -ff $vg || true
	fi
}

aux prepare_vg 1 1000000

VPOOL="$vg-vdopool-vpool"

# Test kernel formatting if available
if test "$KERNEL_FORMAT" -eq 1; then
	test_vdo_format 1 "Kernel formatting"
	test_index_memory_size 1 "Kernel formatting"

	# Verify use_kernel_format is in metadata for kernel-formatted VDO
	lvcreate --vdo -L25G -V50G -n $lv1 $vg/vdopool \
		--vdosettings "use_kernel_format=1"
	vgcfgbackup -f vgbackup $vg
	grep "use_kernel_format" vgbackup
	lvremove -ff $vg
fi

if aux have_vdoformat; then
	# Test userspace vdoformat tool
	test_vdo_format 0 "Userspace vdoformat"
	test_index_memory_size 0 "Userspace vdoformat"

	# Verify use_kernel_format is NOT in metadata for userspace-formatted VDO
	lvcreate --vdo -L25G -V50G -n $lv1 $vg/vdopool \
		--vdosettings "use_kernel_format=0"
	vgcfgbackup -f vgbackup $vg
	not grep "use_kernel_format" vgbackup
	lvremove -ff $vg
else
	# Without vdoformat we cannot create VDO volume
	not lvcreate --vdo -L25G -V50G -n $lv1 $vg/vdopool \
		--vdosettings "use_kernel_format=0"
	lvremove -ff $vg
fi

# Test fallback: requesting kernel format on system without support
if test "$KERNEL_FORMAT" -eq 0; then
	lvcreate -v --vdo -L25G -V50G -n $lv1 $vg/vdopool \
		--vdosettings "use_kernel_format=1" 2>&1 | tee lvcreate.out
	grep "using vdoformat" lvcreate.out
	# Metadata should NOT have use_kernel_format (fallback to vdoformat)
	vgcfgbackup -f vgbackup $vg
	not grep "use_kernel_format" vgbackup
	lvremove -ff $vg
fi

vgremove -ff $vg
