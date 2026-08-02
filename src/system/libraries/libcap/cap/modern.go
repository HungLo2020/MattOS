//go:build linux && !arm && linux && !386
// +build linux,!arm,linux,!386

package cap

import "syscall"

var sysSetGroupsVariant = uintptr(syscall.SYS_SETGROUPS)
var sysSetUIDVariant = uintptr(syscall.SYS_SETUID)
var sysSetGIDVariant = uintptr(syscall.SYS_SETGID)
