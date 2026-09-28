use super::*;

#[test]
fn package_workers_are_bounded_by_cpus_and_memory() {
    const GIB: u64 = 1024 * 1024 * 1024;
    assert_eq!(package_workers_for(12, Some(64 * GIB), GIB), 12);
    assert_eq!(package_workers_for(12, Some(6 * GIB), 3 * GIB / 2), 4);
    assert_eq!(package_workers_for(12, Some(0), GIB), 1);
    assert_eq!(package_workers_for(4, None, GIB), 4);
}
