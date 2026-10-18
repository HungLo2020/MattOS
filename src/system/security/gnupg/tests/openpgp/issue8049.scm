#!/usr/bin/env gpgscm

;; Copyright (C) 2026 g10 Code GmbH
;;
;; This file is part of GnuPG.
;;
;; GnuPG is free software; you can redistribute it and/or modify
;; it under the terms of the GNU General Public License as published by
;; the Free Software Foundation; either version 3 of the License, or
;; (at your option) any later version.
;;
;; GnuPG is distributed in the hope that it will be useful,
;; but WITHOUT ANY WARRANTY; without even the implied warranty of
;; MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
;; GNU General Public License for more details.
;;
;; You should have received a copy of the GNU General Public License
;; along with this program; if not, see <http://www.gnu.org/licenses/>.

;; This is a test for ticket T8049.
;; Testing what happens if a signature packet is too large,
;; i.e. a signature packet greater than 30000 bytes.

(load (in-srcdir "tests" "openpgp" "defs.scm"))
(setup-legacy-environment)

;; Function that creates a long string of Gs.
(define (genlongstr)
  (let loop ((i 0) (result ""))
    (if (>= i 29894) result
        (loop (+ i 1) (string-append result "G")))))

(info "Write a too large signature.")
(call `(,@GPG --yes -o "sig8049.asc"
              --passphrase "abc"
              -v
              --set-notation ,(string-append "alpha@example.net=" (genlongstr))
              -u "Alpha"
              -z0
              -a
              --sign "plain-3"))

;; Try to verify the above generated signature.
(define filename '("sig8049.asc"))
(info "Checking that a too large signature is detected.")
(catch '()
       (let* ((status
               (call-popen
                `(,@gpg -v --verify --status-fd=1
                        ,@(map (lambda (name) name) filename))
                ""))))
       (fail "Verification succeeded but should not."))
