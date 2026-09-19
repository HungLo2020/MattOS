dnl  S/390-64 mpn_mul_basecase for z13 and later.

dnl  Copyright 2023 Free Software Foundation, Inc.

dnl  This file is part of the GNU MP Library.
dnl
dnl  The GNU MP Library is free software; you can redistribute it and/or modify
dnl  it under the terms of either:
dnl
dnl    * the GNU Lesser General Public License as published by the Free
dnl      Software Foundation; either version 3 of the License, or (at your
dnl      option) any later version.
dnl
dnl  or
dnl
dnl    * the GNU General Public License as published by the Free Software
dnl      Foundation; either version 2 of the License, or (at your option) any
dnl      later version.
dnl
dnl  or both in parallel, as here.
dnl
dnl  The GNU MP Library is distributed in the hope that it will be useful, but
dnl  WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
dnl  or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU General Public License
dnl  for more details.
dnl
dnl  You should have received copies of the GNU General Public License and the
dnl  GNU Lesser General Public License along with the GNU MP Library.  If not,
dnl  see https://www.gnu.org/licenses/.

include(`../config.m4')

dnl TODO
dnl  * Have mul_1 start without initial (un mod 4) separation, instead separate
dnl    those cases after loop, and then fall into 4 separate ADDMUL_1 blocks
dnl    (without the tmll nonsense).

define(`rp',	`%r2')
define(`ap',	`%r3')
define(`an',	`%r4')
define(`bp',	`%r5')
define(`bn_arg',`%r6')

define(`bn',	`%r13')
define(`idx',	`%r12')
define(`b0',	`%r10')

ifdef(`HAVE_HOST_CPU_z15',`define(`HAVE_vler',1)')
ifdef(`HAVE_HOST_CPU_z16',`define(`HAVE_vler',1)')
ifdef(`HAVE_vler',`
define(`vpdi', `dnl')
',`
define(`vler', `vl')
define(`vster', `vst')
')

define(`MUL_1',`
pushdef(`L',
defn(`L')$1`'_m1)
	vzero	%v2
	srlg	%r11, an, 2

	tmll	an, 1
	je	L(bx0)
L(bx1):	tmll	an, 2
	jne	L(b11)

L(b01):	lghi	idx, -24
	lg	%r7, 0(ap)
	mlgr	%r6, b0
	stg	%r7, 0(rp)
	cgijne	%r11, 0, L(cj0)

L(1):	stg	%r6, 8(rp)
	lmg	%r6, %r13, 48(%r15)
	br	%r14

L(b11):	lghi	idx, -8
	lg	%r9, 0(ap)
	mlgr	%r8, b0
	stg	%r9, 0(rp)
	j	L(cj1)

L(bx0):	tmll	an, 2
	jne	L(b10)
L(b00):	lghi	idx, -32
	lghi	%r6, 0
L(cj0):	lg	%r1, 32(idx, ap)
	lg	%r9, 40(idx, ap)
	mlgr	%r0, b0
	mlgr	%r8, b0
	vlvgp	%v6, %r0, %r1
	vlvgp	%v7, %r9, %r6
	j	L(mid)

L(b10):	lghi	idx, -16
	lghi	%r8, 0
L(cj1):	lg	%r1, 16(idx, ap)
	lg	%r7, 24(idx, ap)
	mlgr	%r0, b0
	mlgr	%r6, b0
	vlvgp	%v6, %r0, %r1
	vlvgp	%v7, %r7, %r8
	cgije	%r11, 0, L(end)

L(top):	lg	%r1, 32(idx, ap)
	lg	%r9, 40(idx, ap)
	mlgr	%r0, b0
	mlgr	%r8, b0
	vacq	%v3, %v6, %v7, %v2
	vacccq	%v2, %v6, %v7, %v2
	vpdi	%v3, %v3, %v3, 4
	vster	%v3, 16(idx, rp), 3
	vlvgp	%v6, %r0, %r1
	vlvgp	%v7, %r9, %r6
L(mid):	lg	%r1, 48(idx, ap)
	lg	%r7, 56(idx, ap)
	mlgr	%r0, b0
	mlgr	%r6, b0
	vacq	%v3, %v6, %v7, %v2
	vacccq	%v2, %v6, %v7, %v2
	vpdi	%v3, %v3, %v3, 4
	vster	%v3, 32(idx, rp), 3
	vlvgp	%v6, %r0, %r1
	vlvgp	%v7, %r7, %r8
	la	idx, 32(idx)
	brctg	%r11, L(top)

L(end):	vacq	%v3, %v6, %v7, %v2
	vacccq	%v2, %v6, %v7, %v2
	vpdi	%v3, %v3, %v3, 4
	vster	%v3, 16(idx, rp), 3

	vlgvg	%r0, %v2, 1
	algr	%r0, %r6
	stg	%r0, 32(idx, rp)
popdef(`L')
')

define(`ADDMUL_1',`
pushdef(`L',
defn(`L')$1`'_am1)
	vzero	%v0
	vzero	%v2
	srlg	%r11, an, 2

	tmll	an, 1
	je	L(bx0)
L(bx1):	tmll	an, 2
	jne	L(b11)

L(b01):	lghi	idx, -24
	vleg	%v2, 0(rp), 1
	lg	%r7, 0(ap)
	vzero	%v4
	mlgr	%r6, b0
	vlvgg	%v4, %r7, 1
	vaq	%v2, %v2, %v4
	vsteg	%v2, 0(rp), 1
	vmrhg	%v2, %v2, %v2
	j	L(cj0)

L(b11):	lghi	idx, -8
	vleg	%v2, 0(rp), 1
	lg	%r9, 0(ap)
	vzero	%v4
	mlgr	%r8, b0
	vlvgg	%v4, %r9, 1
	vaq	%v2, %v2, %v4
	vsteg	%v2, 0(rp), 1
	vmrhg	%v2, %v2, %v2
	j	L(cj1)

L(bx0):	tmll	an, 2
	jne	L(b10)
L(b00):	lghi	idx, -32
	lghi	%r6, 0
L(cj0):	lg	%r1, 32(idx, ap)
	lg	%r9, 40(idx, ap)
	mlgr	%r0, b0
	mlgr	%r8, b0
	vler	%v1, 32(idx, rp), 3
	vpdi	%v1, %v1, %v1, 4
	vlvgp	%v6, %r0, %r1
	vlvgp	%v7, %r9, %r6
	j	L(mid)

L(b10):	lghi	idx, -16
	lghi	%r8, 0
L(cj1):	lg	%r1, 16(idx, ap)
	lg	%r7, 24(idx, ap)
	mlgr	%r0, b0
	mlgr	%r6, b0
	vler	%v1, 16(idx, rp), 3
	vpdi	%v1, %v1, %v1, 4
	vlvgp	%v6, %r0, %r1
	vlvgp	%v7, %r7, %r8
	cgije	%r11, 0, L(end)

	.align	16
L(top):	lg	%r1, 32(idx, ap)
	lg	%r9, 40(idx, ap)
	mlgr	%r0, b0
	mlgr	%r8, b0
	vacq	%v5, %v6, %v1, %v0
	vacccq	%v0, %v6, %v1, %v0
	vacq	%v3, %v5, %v7, %v2
	vacccq	%v2, %v5, %v7, %v2
	vpdi	%v3, %v3, %v3, 4
	vler	%v1, 32(idx, rp), 3
	vpdi	%v1, %v1, %v1, 4
	vster	%v3, 16(idx, rp), 3
	vlvgp	%v6, %r0, %r1
	vlvgp	%v7, %r9, %r6
L(mid):	lg	%r1, 48(idx, ap)
	lg	%r7, 56(idx, ap)
	mlgr	%r0, b0
	mlgr	%r6, b0
	vacq	%v5, %v6, %v1, %v0
	vacccq	%v0, %v6, %v1, %v0
	vacq	%v3, %v5, %v7, %v2
	vacccq	%v2, %v5, %v7, %v2
	vpdi	%v3, %v3, %v3, 4
	vler	%v1, 48(idx, rp), 3
	vpdi	%v1, %v1, %v1, 4
	vster	%v3, 32(idx, rp), 3
	vlvgp	%v6, %r0, %r1
	vlvgp	%v7, %r7, %r8
	la	idx, 32(idx)
	brctg	%r11, L(top)

L(end):	vacq	%v5, %v6, %v1, %v0
	vacccq	%v0, %v6, %v1, %v0
	vacq	%v3, %v5, %v7, %v2
	vacccq	%v2, %v5, %v7, %v2
	vpdi	%v3, %v3, %v3, 4
	vster	%v3, 16(idx, rp), 3

	vag	%v2, %v0, %v2
	vlgvg	%r0, %v2, 1
	algr	%r0, %r6
	stg	%r0, 32(idx, rp)
popdef(`L')
')


ASM_START()

PROLOGUE(mpn_mul_basecase)
	clgijle	an, 2, L(sma)

	stmg	%r6, %r13, 48(%r15)
	lgr	bn, bn_arg

	lg	b0, 0(bp)
	MUL_1()
	aghi	bn, -1
	je	L(end)

L(top):	la	bp, 8(bp)
	la	rp, 8(rp)
	lg	b0, 0(bp)
	ADDMUL_1()
	brctg	bn, L(top)

L(end):	lmg	%r6, %r13, 48(%r15)
	br	%r14

L(sma):	clgije	an, 1, L(11)
L(2x):	clgije	bn_arg, 1, L(21)
L(22):	stmg	%r6, %r9, 48(%r15)
	lg	%r1, 0(bp)
	lgr	%r7, %r1
	mlg	%r0, 0(ap)
	mlg	%r6, 8(ap)
	algr	%r0, %r7
	lghi	%r7, 0
	alcgr	%r6, %r7
	lg	%r5, 8(bp)
	lgr	%r9, %r5
	mlg	%r4, 0(ap)
	mlg	%r8, 8(ap)
	algr	%r0, %r5
	alcgr	%r6, %r4
	lghi	%r4, 0
	alcgr	%r8, %r4
	algr	%r6, %r9
	alcgr	%r8, %r4
	stg	%r1, 0(rp)
	stg	%r0, 8(rp)
	stg	%r6, 16(rp)
	stg	%r8, 24(rp)
	lmg	%r6, %r9, 48(%r15)
	br	%r14

L(21):	lg	%r1, 0(bp)
	lgr	%r5, %r1
	mlg	%r0, 0(ap)
	mlg	%r4, 8(ap)
	algr	%r0, %r5
	lghi	%r5, 0
	alcgr	%r4, %r5
	stg	%r1, 0(rp)
	stg	%r0, 8(rp)
	stg	%r4, 16(rp)
	br	%r14

L(11):	lg	%r1, 0(ap)
	mlg	%r0, 0(bp)
	stg	%r1, 0(rp)
	stg	%r0, 8(rp)
	br	%r14
EPILOGUE()
