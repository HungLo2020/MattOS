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

(load (in-srcdir "tests" "openpgp" "defs.scm"))
(setup-9980-environment)

(info "Importing public MLK key.")
(call-check
 `(,(tool 'gpg) --import
   ,(in-srcdir "tests" "openpgp" "samplekeys/ietf27-mlk-nistbrain-pub.asc")))

;; Note that we have one key withh several encryption subkeys.
(info "Importing secret MLK keys.")
(for-each
 (lambda (name)
   (file-copy (in-srcdir "tests" "openpgp" "privkeys"
                           (string-append name ".key"))
	      (string-append "private-keys-v1.d/" name ".key")))
 '("7FF53317DEDAC5BB0299C0A754D106F0621C8257"  ; ietf27 (primary_
   "4046964D48529FC7DE554542F4A658591BE27AF2"  ; mlk1024_np521
   "138E37A9418DA4C544BDDCDD11432C01ECEEAED4"
   "8D69D5C7F91781175A899B1453CEF28BAD533991"  ; mlk768_np384
   "59BE88218A145F87826ABBD49817ED8F26E7DA50"
   "5F7A0A9BB0B5519F4E2BB2A91B742AE218886888"  ; mlk1024_bp51
   "4C726A3962D6114D5066551A4143FF3A0414394D"
   "2E99977AF7BF5029064E39A12FA866A1A79F89C4"  ; mlk768_bp384
   "AD0EF479060A5B9DC2B0AD14168770867DE53EB0"))

(for-each-p
 "Checking encryption using MLK subkeys"
 (lambda (keyname)
   (tr:do
    (tr:open "data-9000")
    (tr:gpg "" `(--yes --encrypt --recipient ,keyname))
    (tr:gpg "" '(--yes --decrypt))
    (tr:assert-identity "data-9000")))
 '("3838F412C21DF580!"
   "175BCF4EA2AC9C2C!"
   "D56E2B801D217D01!"
   "B416466A0828115F!"))


(define file-plain-vectors
  '(("pqc-mlk1024_np521.enc.asc"
     "There are more things in heaven and earth,
Horatio, than are dreamt of in your philosophy.
\t\t-- Wm. Shakespeare, \"Hamlet\"\n")

   ("pqc-mlk768_np384.enc.asc"
     "You will always have good luck in your personal affairs.\n")

   ("pqc-mlk1024_bp512.enc.asc"
     "Alas, how love can trifle with itself!
\t\t-- William Shakespeare, \"The Two Gentlemen of Verona\"\n")

   ("pqc-mlk768_bp384.enc.asc"
     "What I tell you three times is true.
\t\t-- Lewis Carroll\n")))


;; Debug helper.  Use instead of assert-same and pass output to cat -eT
(define (tr:myassert-same reference)
  (lambda (tmpfiles source)
    (let ((output (call-with-input-file source read-all)))
      (if (not (string=?  output reference))
	  ((display "\n===========\n")
           (display output)
           (display "-----------\n")
           (display reference)
           (display "===========\n")
           (fail "mismatch"))))
    (list tmpfiles source #f)))


(for-each-p'
 "Checking decryption of supplied MLK messages"
 (lambda (vector)
   (let ((file (car vector))
         (plain (cadr vector)))
     (tr:do
      (tr:open (in-srcdir "tests" "openpgp" "samplemsgs" file))
      (tr:gpg "" '(--yes --decrypt))
      (tr:assert-same plain))))
 (lambda (vector) (car vector))
 file-plain-vectors)
