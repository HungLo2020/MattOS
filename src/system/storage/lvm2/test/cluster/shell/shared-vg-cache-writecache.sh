#!/bin/bash
# lv-lock-cache-creation.sh - Test cache and writecache

set -e

if [ "$CLUSTER_NUM_NODES" -lt 2 ]; then
    echo "SKIP: This test requires at least 2 nodes (found $CLUSTER_NUM_NODES)"
    exit 0
fi

#
# Repetitions:
# - one repetition (seq 1 1) is used for tests in which
# one node will always be successful and the other nodes
# will always fail, i.e. no race is expected.
# - three repetitions (seq 1 3) are used for tests in which
# any node may be successful, i.e. they all race.
#
# lvremove:
# - different forms of lvremove are used to clean up
# in different tests, without any particular reason,
# just to test different things.
#

node1 vgcreate --shared testvg $dev1 $dev2
nodep vgchange --lockstart testvg

#
# Parallel attempts to create cache-pool with lvcreate
#
for i in $(seq 1 3); do
    nodep lvcreate --type cache-pool -L 50M -n cpool testvg || true
    assert_one_success

    verify_lv_no_lock_args testvg cpool
    verify_lv_type testvg cpool cache-pool

    nodep lvremove -y testvg/cpool || true
    assert_one_success
    sleep 0.3
done

#
# Parallel attempts to convert LV to cache-pool with lvconvert
#
for i in $(seq 1 3); do
    node1 lvcreate -L 100M -an -n cpool testvg

    nodep lvconvert --type cache-pool testvg/cpool -y || true
    assert_one_success

    verify_lv_no_lock_args testvg cpool
    verify_lv_type testvg cpool cache-pool

    success_node lvremove -y testvg/cpool
    sleep 0.3
done

#
# Parallel attempts to attach cache-pool to inactive lv
#
for i in $(seq 1 3); do
    node1 lvcreate -L 100M -n lv1 -an testvg
    node1 lvcreate --type cache-pool -L 50M -n cpool -an testvg

    nodep lvconvert --type cache --cachepool testvg/cpool testvg/lv1 -y || true
    assert_one_success

    verify_lv_no_lock_args testvg cpool
    verify_lv_type testvg lv1 cache

    node_rand lvremove -y testvg/lv1
    sleep 0.3
done

#
# Parallel attempts to attach cache-pool to active lv
#
for i in $(seq 1 1); do
    node1 lvcreate -L 100M -n lv1 testvg
    node1 lvcreate --type cache-pool -L 50M -n cpool testvg

    nodep lvconvert --type cache --cachepool testvg/cpool testvg/lv1 -y || true
    assert_node_success 1

    verify_lv_no_lock_args testvg cpool

    node1 lvremove -y testvg/lv1
    sleep 0.3
done

#
# Parallel attempts to attach unconverted cache-pool to inactive lv
#
for i in $(seq 1 3); do
    node1 lvcreate -L 100M -n lv1 -an testvg
    node1 lvcreate -L 50M -n cpool -an testvg

    nodep lvconvert --type cache --cachepool testvg/cpool testvg/lv1 -y || true
    assert_one_success

    verify_lv_type testvg lv1 cache
    verify_lv_type testvg cpool_cpool cache-pool
    verify_lv_no_lock_args testvg cpool

    node_rand lvremove -y testvg/lv1
    sleep 0.3
done

#
# Parallel attempts to attach unconverted cache-pool to active lv
#
for i in $(seq 1 1); do
    node1 lvcreate -L 100M -n lv1 testvg
    node1 lvcreate -L 50M -n cpool testvg

    nodep lvconvert --type cache --cachepool testvg/cpool testvg/lv1 -y || true
    assert_node_success 1

    verify_lv_type testvg lv1 cache
    verify_lv_type testvg cpool_cpool cache-pool
    verify_lv_no_lock_args testvg cpool

    node1 lvremove -y testvg/lv1
    sleep 0.3
done

#
# Parallel attempts to attach cachevol to inactive lv
#
for i in $(seq 1 3); do
    node1 lvcreate -L 100M -n lv1 -an testvg
    node1 lvcreate -L 50M -n cvol -an testvg

    nodep lvconvert --type cache --cachevol testvg/cvol testvg/lv1 -y || true
    assert_one_success

    verify_lv_has_lock_args testvg lv1
    verify_lv_has_lock_args testvg cvol_cvol
    verify_lv_type testvg lv1 cache
    verify_lv_type testvg cvol_cvol linear

    nodep lvremove -y testvg/lv1 || true
    assert_one_success
    sleep 0.3
done

#
# Parallel attempts to attach cachevol to active lv
#
for i in $(seq 1 1); do
    node1 lvcreate -L 100M -n lv1 testvg
    node1 lvcreate -L 50M -n cvol testvg

    nodep lvconvert --type cache --cachevol testvg/cvol testvg/lv1 -y || true
    assert_node_success 1

    verify_lv_has_lock_args testvg lv1
    verify_lv_has_lock_args testvg cvol_cvol
    verify_lv_type testvg lv1 cache
    verify_lv_type testvg cvol_cvol linear

    node1 lvremove -y testvg/lv1
    sleep 0.3
done

#
# Parallel attempts to attach cachevol to inactive lv (writecache)
#
for i in $(seq 1 3); do
    node1 lvcreate -L 100M -n lv1 -an testvg
    node1 lvcreate -L 50M -n cvol -an testvg

    nodep lvconvert --type writecache --cachevol testvg/cvol testvg/lv1 -y || true
    assert_one_success

    verify_lv_has_lock_args testvg lv1
    verify_lv_has_lock_args testvg cvol_cvol
    verify_lv_type testvg lv1 writecache
    verify_lv_type testvg cvol_cvol linear

    node_rand lvremove -y testvg/lv1
    sleep 0.3
done

#
# Parallel attempts to attach cachevol to active lv (writecache)
#
for i in $(seq 1 1); do
    node1 lvcreate -L 100M -n lv1 testvg
    node1 lvcreate -L 50M -n cvol -an testvg

    nodep lvconvert --type writecache --cachevol testvg/cvol testvg/lv1 -y || true
    assert_node_success 1

    verify_lv_has_lock_args testvg lv1
    verify_lv_has_lock_args testvg cvol_cvol
    verify_lv_type testvg lv1 writecache
    verify_lv_type testvg cvol_cvol linear

    node1 lvremove -y testvg/lv1
    sleep 0.3
done


#
# Cannot convert lv to cache-pool while lv is active on another node
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvchange -ay testvg/lv1

node2 not lvconvert --type cache-pool testvg/lv1 -y
verify_lv_type testvg lv1 linear

node1 lvchange -an testvg/lv1

node2 lvconvert --type cache-pool testvg/lv1 -y
verify_lv_type testvg lv1 cache-pool

node2 lvremove -y testvg/lv1

#
# Create cache-pool with explicit --poolmetadata
#
node1 lvcreate -L 100M -n cpool -an testvg
node1 lvcreate -L 8M -n cmeta -an testvg

node1 lvconvert --type cache-pool --poolmetadata testvg/cmeta testvg/cpool -y

verify_lv_no_lock_args testvg cpool
verify_lv_no_lock_args testvg cmeta

node1 lvremove -y testvg/cpool

#
# Cannot convert lv to cache-pool with --poolmetadata if metadata LV is active on another node
#
node1 lvcreate -L 100M -n cpool -an testvg
node1 lvcreate -L 8M -n cmeta -an testvg

node1 lvchange -ay testvg/cmeta

node2 not lvconvert --type cache-pool --poolmetadata testvg/cmeta testvg/cpool -y
verify_lv_type testvg cpool linear
verify_lv_has_lock_args testvg cpool
verify_lv_has_lock_args testvg cmeta

node1 lvchange -an testvg/cmeta

node2 lvconvert --type cache-pool --poolmetadata testvg/cmeta testvg/cpool -y
verify_lv_type testvg cpool cache-pool
verify_lv_no_lock_args testvg cpool
verify_lv_no_lock_args testvg cmeta

node1 lvremove -y testvg/cpool

#
# Cannot attach cache-pool while origin is active on another node
#
node1 lvcreate -L 100M -n lv1 -an testvg
node1 lvcreate --type cache-pool -L 50M -n cpool -an testvg

node1 lvchange -ay testvg/lv1

node2 not lvconvert --type cache --cachepool testvg/cpool testvg/lv1 -y
verify_lv_type testvg lv1 linear

node1 lvchange -an testvg/lv1

node2 lvconvert --type cache --cachepool testvg/cpool testvg/lv1 -y
verify_lv_type testvg lv1 cache

node1 lvremove -y testvg/lv1

#
# Cannot attach cache-pool while unconverted pool is active on another node
#
node1 lvcreate -L 100M -n lv1 -an testvg
node1 lvcreate -L 50M -n cpool -an testvg

node1 lvchange -ay testvg/cpool

node2 not lvconvert --type cache --cachepool testvg/cpool testvg/lv1 -y
verify_lv_type testvg lv1 linear

node1 lvchange -an testvg/cpool

node2 lvconvert --type cache --cachepool testvg/cpool testvg/lv1 -y
verify_lv_type testvg lv1 cache
verify_lv_type testvg cpool_cpool cache-pool

node1 lvremove -y testvg/lv1

#
# Cannot attach cachevol while origin is active on another node
#
node1 lvcreate -L 100M -n lv1 -an testvg
node1 lvcreate -L 50M -n cvol -an testvg

node1 lvchange -ay testvg/lv1

node2 not lvconvert --type cache --cachevol testvg/cvol testvg/lv1 -y
verify_lv_type testvg lv1 linear

node1 lvchange -an testvg/lv1

node2 lvconvert --type cache --cachevol testvg/cvol testvg/lv1 -y
verify_lv_type testvg lv1 cache

node1 lvremove -y testvg/lv1

#
# Cannot attach cachevol while cachevol is active on another node
#
node1 lvcreate -L 100M -n lv1 -an testvg
node1 lvcreate -L 50M -n cvol -an testvg

node1 lvchange -ay testvg/cvol

node2 not lvconvert --type cache --cachevol testvg/cvol testvg/lv1 -y
verify_lv_type testvg lv1 linear

node1 lvchange -an testvg/cvol

node2 lvconvert --type cache --cachevol testvg/cvol testvg/lv1 -y
verify_lv_type testvg lv1 cache

node1 lvremove -y testvg/lv1

#
# Cannot attach cachevol while origin is active on another node (writecache)
#
node1 lvcreate -L 100M -n lv1 -an testvg
node1 lvcreate -L 50M -n cvol -an testvg

node1 lvchange -ay testvg/lv1

node2 not lvconvert --type writecache --cachevol testvg/cvol testvg/lv1 -y
verify_lv_type testvg lv1 linear

node1 lvchange -an testvg/lv1

node2 lvconvert --type writecache --cachevol testvg/cvol testvg/lv1 -y
verify_lv_type testvg lv1 writecache

node1 lvremove -y testvg/lv1

#
# Cannot attach cachevol while cachevol is active on another node (writecache)
#
node1 lvcreate -L 100M -n lv1 -an testvg
node1 lvcreate -L 50M -n cvol -an testvg

node1 lvchange -ay testvg/cvol

node2 not lvconvert --type writecache --cachevol testvg/cvol testvg/lv1 -y
verify_lv_type testvg lv1 linear

node1 lvchange -an testvg/cvol

node2 lvconvert --type writecache --cachevol testvg/cvol testvg/lv1 -y
verify_lv_type testvg lv1 writecache

node1 lvremove -y testvg/lv1

#
# Cannot attach same cache-pool to multiple origins
#
node1 lvcreate -L 100M -n lv1 -an testvg
node1 lvcreate -L 100M -n lv2 -an testvg
node1 lvcreate --type cache-pool -L 50M -n cpool -an testvg

node1 lvconvert --type cache --cachepool testvg/cpool testvg/lv1 -y

node2 not lvconvert --type cache --cachepool testvg/cpool testvg/lv2 -y

node1 lvremove -y testvg/lv1
node1 lvremove -y testvg/lv2

#
# Cannot attach same cachevol to multiple origins
#
node1 lvcreate -L 100M -n lv1 -an testvg
node1 lvcreate -L 100M -n lv2 -an testvg
node1 lvcreate -L 50M -n cvol -an testvg

node1 lvconvert --type cache --cachevol testvg/cvol testvg/lv1 -y

node2 not lvconvert --type cache --cachevol testvg/cvol testvg/lv2 -y

node1 lvremove -y testvg/lv1
node1 lvremove -y testvg/lv2

#
# Cannot attach same cachevol to multiple origins (writecache)
#
node1 lvcreate -L 100M -n lv1 -an testvg
node1 lvcreate -L 100M -n lv2 -an testvg
node1 lvcreate -L 50M -n cvol -an testvg

node1 lvconvert --type writecache --cachevol testvg/cvol testvg/lv1 -y

node2 not lvconvert --type writecache --cachevol testvg/cvol testvg/lv2 -y

node1 lvremove -y testvg/lv1
node1 lvremove -y testvg/lv2

#
# Parallel attempts to splitcache cache-pool from active lv
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate --type cache-pool -L 50M -n cpool testvg
for i in $(seq 1 1); do
    node1 lvconvert --type cache --cachepool cpool testvg/lv1 -y
    node1 lvchange -ay testvg/lv1
    nodep lvconvert --splitcache testvg/lv1 || true
    assert_node_success 1
    verify_lv_type testvg lv1 linear
    verify_lv_type testvg cpool cache-pool
    sleep 0.3
done
node1 lvremove -y testvg/lv1
node1 lvremove -y testvg/cpool

#
# Parallel attempts to splitcache cache-pool from inactive lv
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate --type cache-pool -L 50M -n cpool testvg
for i in $(seq 1 3); do
    node1 lvconvert --type cache --cachepool cpool testvg/lv1 -y
    node1 lvchange -an testvg/lv1
    nodep lvconvert --splitcache testvg/lv1 || true
    assert_one_success
    verify_lv_type testvg lv1 linear
    verify_lv_type testvg cpool cache-pool
    sleep 0.3
done
node1 lvremove -y testvg/lv1
node1 lvremove -y testvg/cpool

#
# Parallel attempts to uncache cache-pool from active lv
#
node1 lvcreate -L 100M -n lv1 testvg
for i in $(seq 1 1); do
    node1 lvcreate --type cache-pool -L 50M -n cpool testvg
    node1 lvconvert --type cache --cachepool testvg/cpool testvg/lv1 -y
    node1 lvchange -ay testvg/lv1
    nodep lvconvert --uncache testvg/lv1 || true
    assert_node_success 1
    verify_lv_type testvg lv1 linear
    sleep 0.3
done
node1 lvremove -y testvg/lv1

#
# Parallel attempts to uncache cache-pool from inactive lv
#
node1 lvcreate -L 100M -n lv1 testvg
for i in $(seq 1 3); do
    node1 lvcreate --type cache-pool -L 50M -n cpool testvg
    node1 lvconvert --type cache --cachepool testvg/cpool testvg/lv1 -y
    node1 lvchange -an testvg/lv1
    nodep lvconvert --uncache testvg/lv1 || true
    assert_one_success
    verify_lv_type testvg lv1 linear
    sleep 0.3
done
node1 lvremove -y testvg/lv1

#
# Parallel attempts to splitcache cachevol from active lv
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 50M -n cvol testvg
for i in $(seq 1 1); do
    node1 lvconvert --type cache --cachevol cvol testvg/lv1 -y
    node1 lvchange -ay testvg/lv1
    nodep lvconvert --splitcache testvg/lv1 || true
    assert_node_success 1
    verify_lv_type testvg lv1 linear
    verify_lv_type testvg cvol linear
    sleep 0.3
done
node1 lvremove -y testvg/lv1
node1 lvremove -y testvg/cvol

#
# Parallel attempts to splitcache cachevol from inactive lv
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 50M -n cvol testvg
for i in $(seq 1 3); do
    node1 lvconvert --type cache --cachevol cvol testvg/lv1 -y
    node1 lvchange -an testvg/lv1
    nodep lvconvert --splitcache testvg/lv1 || true
    assert_one_success
    verify_lv_type testvg lv1 linear
    verify_lv_type testvg cvol linear
    sleep 0.3
done
node1 lvremove -y testvg/lv1
node1 lvremove -y testvg/cvol

#
# Parallel attempts to uncache cachevol from active lv
#
node1 lvcreate -L 100M -n lv1 testvg
for i in $(seq 1 1); do
    node1 lvcreate -L 50M -n cvol testvg
    node1 lvconvert --type cache --cachevol testvg/cvol testvg/lv1 -y
    node1 lvchange -ay testvg/lv1
    nodep lvconvert --uncache testvg/lv1 || true
    assert_node_success 1
    verify_lv_type testvg lv1 linear
    sleep 0.3
done
node1 lvremove -y testvg/lv1

#
# Parallel attempts to uncache cachevol from inactive lv
#
node1 lvcreate -L 100M -n lv1 testvg
for i in $(seq 1 3); do
    node1 lvcreate -L 50M -n cvol testvg
    node1 lvconvert --type cache --cachevol testvg/cvol testvg/lv1 -y
    node1 lvchange -an testvg/lv1
    nodep lvconvert --uncache testvg/lv1 || true
    assert_one_success
    verify_lv_type testvg lv1 linear
    sleep 0.3
done
node1 lvremove -y testvg/lv1

#
# Parallel attempts to splitcache cachevol from active lv (writecache)
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 50M -n cvol testvg
for i in $(seq 1 1); do
    node1 lvchange -an testvg/cvol
    node1 lvconvert --type writecache --cachevol cvol testvg/lv1 -y
    node1 lvchange -ay testvg/lv1
    nodep lvconvert --splitcache testvg/lv1 || true
    assert_node_success 1
    verify_lv_type testvg lv1 linear
    verify_lv_type testvg cvol linear
    sleep 0.3
done
node1 lvremove -y testvg/lv1
node1 lvremove -y testvg/cvol

#
# Parallel attempts to splitcache cachevol from inactive lv (writecache)
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 50M -n cvol testvg
for i in $(seq 1 3); do
    node1 lvchange -an testvg/cvol
    node1 lvconvert --type writecache --cachevol cvol testvg/lv1 -y
    node1 lvchange -an testvg/lv1
    nodep lvconvert --splitcache testvg/lv1 || true
    assert_one_success
    verify_lv_type testvg lv1 linear
    verify_lv_type testvg cvol linear
    sleep 0.3
done
node1 lvremove -y testvg/lv1
node1 lvremove -y testvg/cvol

#
# Parallel attempts to uncache cachevol from active lv (writecache)
#
node1 lvcreate -L 100M -n lv1 testvg
for i in $(seq 1 1); do
    node1 lvcreate -L 50M -n cvol -an testvg
    node1 lvconvert --type writecache --cachevol testvg/cvol testvg/lv1 -y
    node1 lvchange -ay testvg/lv1
    nodep lvconvert --uncache testvg/lv1 || true
    assert_node_success 1
    verify_lv_type testvg lv1 linear
    sleep 0.3
done
node1 lvremove -y testvg/lv1

#
# Parallel attempts to uncache cachevol from inactive lv (writecache)
#
node1 lvcreate -L 100M -n lv1 testvg
for i in $(seq 1 3); do
    node1 lvcreate -L 50M -n cvol -an testvg
    node1 lvconvert --type writecache --cachevol testvg/cvol testvg/lv1 -y
    node1 lvchange -an testvg/lv1
    nodep lvconvert --uncache testvg/lv1 || true
    assert_one_success
    verify_lv_type testvg lv1 linear
    sleep 0.3
done
node1 lvremove -y testvg/lv1

#
# Parallel activation races (cache, cache-pool)
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate --type cache-pool -L 50M -n cpool testvg
node1 lvconvert --type cache --cachepool testvg/cpool testvg/lv1 -y
node1 lvchange -an testvg/lv1

for i in $(seq 1 5); do
    nodep lvchange -ay testvg/lv1 || true
    assert_one_success

    verify_lv_active_on "$NODEP_SINGLE_SUCCESS_NODE" testvg lv1

    success_node lvchange -an testvg/lv1
    sleep 0.3
done

node1 lvremove -y testvg/lv1

#
# Parallel activation races (cache, cachevol)
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 50M -n cvol testvg
node1 lvconvert --type cache --cachevol testvg/cvol testvg/lv1 -y
node1 lvchange -an testvg/lv1

for i in $(seq 1 5); do
    nodep lvchange -ay testvg/lv1 || true
    assert_one_success

    verify_lv_active_on "$NODEP_SINGLE_SUCCESS_NODE" testvg lv1

    success_node lvchange -an testvg/lv1
    sleep 0.3
done

node1 lvremove -y testvg/lv1

#
# Parallel activation races (writecache)
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 50M -n cvol -an testvg
node1 lvconvert --type writecache --cachevol testvg/cvol testvg/lv1 -y
node1 lvchange -an testvg/lv1

for i in $(seq 1 5); do
    nodep lvchange -ay testvg/lv1 || true
    assert_one_success

    verify_lv_active_on "$NODEP_SINGLE_SUCCESS_NODE" testvg lv1

    success_node lvchange -an testvg/lv1
    sleep 0.3
done

node1 lvremove -y testvg/lv1

#
# LV cannot use shared activation (cache, cache-pool)
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate --type cache-pool -L 50M -n cpool testvg
node1 lvconvert --type cache --cachepool testvg/cpool testvg/lv1 -y
node1 lvchange -an testvg/lv1

nodep lvchange -asy testvg/lv1 || true
assert_all_fail
nodes lvchange -asy testvg/lv1 || true
assert_all_fail

node1 lvremove -y testvg/lv1

#
# LV cannot use shared activation (cache, cachevol)
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 50M -n cvol testvg
node1 lvconvert --type cache --cachevol testvg/cvol testvg/lv1 -y
node1 lvchange -an testvg/lv1

nodep lvchange -asy testvg/lv1 || true
assert_all_fail
nodes lvchange -asy testvg/lv1 || true
assert_all_fail


node1 lvremove -y testvg/lv1

#
# LV cannot use shared activation (writecache)
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 50M -n cvol -an testvg
node1 lvconvert --type writecache --cachevol testvg/cvol testvg/lv1 -y
node1 lvchange -an testvg/lv1

nodep lvchange -asy testvg/lv1 || true
assert_all_fail
nodes lvchange -asy testvg/lv1 || true
assert_all_fail

node1 lvremove -y testvg/lv1

#
# Rapid activation cycles (cache, cache-pool)
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate --type cache-pool -L 50M -n cpool testvg
node1 lvconvert --type cache --cachepool testvg/cpool testvg/lv1 -y
node1 lvchange -an testvg/lv1

for i in $(seq 1 10); do
    nodep lvchange -ay testvg/lv1 || true
    assert_one_success

    success_node lvchange -an testvg/lv1
done

node1 lvremove -y testvg/lv1

#
# Rapid activation cycles (cache, cachevol)
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 50M -n cvol testvg
node1 lvconvert --type cache --cachevol testvg/cvol testvg/lv1 -y
node1 lvchange -an testvg/lv1

for i in $(seq 1 10); do
    nodep lvchange -ay testvg/lv1 || true
    assert_one_success

    success_node lvchange -an testvg/lv1
done

node1 lvremove -y testvg/lv1

#
# Rapid activation cycles (writecache)
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 50M -n cvol -an testvg
node1 lvconvert --type writecache --cachevol testvg/cvol testvg/lv1 -y
node1 lvchange -an testvg/lv1

for i in $(seq 1 10); do
    nodep lvchange -ay testvg/lv1 || true
    assert_one_success

    success_node lvchange -an testvg/lv1
done

node1 lvremove -y testvg/lv1

#
# Exclusive activation blocks other activations (writecache)
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 50M -n cvol -an testvg
node1 lvconvert --type writecache --cachevol testvg/cvol testvg/lv1 -y

node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

for node_num in $(seq 2 $CLUSTER_NUM_NODES); do
    noden ${node_num} not lvchange -ay testvg/lv1
done

node1 lvchange -an testvg/lv1
verify_lv_not_active_on 1 testvg lv1

node1 lvremove -y testvg/lv1

#
# Exclusive activation blocks other activations (cache)
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 50M -n cvol -an testvg
node1 lvconvert --type cache --cachevol testvg/cvol testvg/lv1 -y

node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

for node_num in $(seq 2 $CLUSTER_NUM_NODES); do
    noden ${node_num} not lvchange -ay testvg/lv1
done

node1 lvchange -an testvg/lv1
verify_lv_not_active_on 1 testvg lv1

node1 lvremove -y testvg/lv1

#
# Parallel lvrename races (writecache)
#
for i in $(seq 1 3); do
    node1 lvcreate -L 100M -n lv1 testvg
    node1 lvcreate -L 50M -n cvol -an testvg
    node1 lvconvert --type writecache --cachevol testvg/cvol testvg/lv1 -y

    nodep lvrename testvg/lv1 testvg/lv2 || true
    assert_one_success

    nodep lvs testvg/lv2 > /dev/null
    assert_all_success

    node1 lvremove -y testvg/lv2
    sleep 0.3
done

#
# Parallel lvrename races (cache)
#
for i in $(seq 1 3); do
    node1 lvcreate -L 100M -n lv1 testvg
    node1 lvcreate -L 50M -n cvol -an testvg
    node1 lvconvert --type cache --cachevol testvg/cvol testvg/lv1 -y

    nodep lvrename testvg/lv1 testvg/lv2 || true
    assert_one_success

    nodep lvs testvg/lv2 > /dev/null
    assert_all_success

    node1 lvremove -y testvg/lv2
    sleep 0.3
done

#
# Parallel lvextend races (writecache)
#
for i in $(seq 1 3); do
    node1 lvcreate -L 100M -n lv1 testvg
    node1 lvcreate -L 50M -n cvol -an testvg
    node1 lvconvert --type writecache --cachevol testvg/cvol testvg/lv1 -y

    nodep lvextend -L +50M testvg/lv1 || true
    assert_one_success

    node1 lvremove -y testvg/lv1
    sleep 0.3
done

#
# Parallel lvextend races (cache)
#
for i in $(seq 1 3); do
    node1 lvcreate -L 100M -n lv1 testvg
    node1 lvcreate -L 50M -n cvol -an testvg
    node1 lvconvert --type cache --cachevol testvg/cvol testvg/lv1 -y

    nodep lvextend -L +50M testvg/lv1 || true
    assert_one_success

    node1 lvremove -y testvg/lv1
    sleep 0.3
done

#
# Cannot modify LV active on another node (writecache)
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 50M -n cvol -an testvg
node1 lvconvert --type writecache --cachevol testvg/cvol testvg/lv1 -y

node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

node2 not lvrename testvg/lv1 testvg/lv2
node2 not lvextend -L +50M testvg/lv1

node1 lvchange -an testvg/lv1
verify_lv_not_active_on 1 testvg lv1

node2 lvrename testvg/lv1 testvg/lv2
node2 lvextend -L +50M testvg/lv2

node1 lvremove -y testvg/lv2

#
# Cannot modify LV active on another node (cache)
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 50M -n cvol -an testvg
node1 lvconvert --type cache --cachevol testvg/cvol testvg/lv1 -y

node1 lvchange -ay testvg/lv1
verify_lv_active_on 1 testvg lv1

node2 not lvrename testvg/lv1 testvg/lv2
node2 not lvextend -L +50M testvg/lv1

node1 lvchange -an testvg/lv1
verify_lv_not_active_on 1 testvg lv1

node2 lvrename testvg/lv1 testvg/lv2
node2 lvextend -L +50M testvg/lv2

node1 lvremove -y testvg/lv2

#
# Nodes can take turns activating (writecache)
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 50M -n cvol -an testvg
node1 lvconvert --type writecache --cachevol testvg/cvol testvg/lv1 -y

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${node_num} lvchange -ay testvg/lv1
    verify_lv_active_on ${node_num} testvg lv1
    sleep .2
    noden ${node_num} lvchange -an testvg/lv1
    verify_lv_not_active_on ${node_num} testvg lv1
done

node1 lvremove -y testvg/lv1

#
# Nodes can take turns activating (cache)
#
node1 lvcreate -L 100M -n lv1 testvg
node1 lvcreate -L 50M -n cvol -an testvg
node1 lvconvert --type cache --cachevol testvg/cvol testvg/lv1 -y

for node_num in $(seq 1 $CLUSTER_NUM_NODES); do
    noden ${node_num} lvchange -ay testvg/lv1
    verify_lv_active_on ${node_num} testvg lv1
    sleep .2
    noden ${node_num} lvchange -an testvg/lv1
    verify_lv_not_active_on ${node_num} testvg lv1
done

node1 lvremove -y testvg/lv1

# Cleanup
cleanup_vg testvg

exit 0
