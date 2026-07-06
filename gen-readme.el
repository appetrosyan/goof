#!/usr/bin/env -S emacs --script
;;; gen-readme.el --- Regenerate README.md from README.org -*- lexical-binding: t; -*-

;;; Commentary:

;; README.org is the source of truth; README.md is a generated artifact
;; for platforms that render Markdown rather than org-mode (crates.io,
;; GitHub).  Regenerate after editing README.org, then commit both:
;;
;;     ./gen-readme.el            # self-executing (env + emacs --script)
;;     emacs --script gen-readme.el   # or invoke explicitly
;;
;; The conversion uses Org's own exporter via `ox-gfm', which — unlike
;; pandoc — resolves internal links such as `[[*Ready-made error types]]'
;; to real anchors and needs no external toolchain beyond Emacs.

;;; Code:

;; `emacs --script' does not load the user's init, so assert the state we
;; need explicitly and idempotently: MELPA available, use-package present,
;; ox-gfm installed.
(require 'package)
(add-to-list 'package-archives '("melpa" . "https://melpa.org/packages/") t)
(package-initialize)

;; use-package is built in since Emacs 29; install it on anything older.
(unless (require 'use-package nil t)
  (unless package-archive-contents (package-refresh-contents))
  (package-install 'use-package)
  (require 'use-package))

;; Only hit the network on the first run, before ox-gfm is installed.
(unless (package-installed-p 'ox-gfm)
  (unless package-archive-contents (package-refresh-contents)))

(use-package ox-gfm
  :ensure t
  :demand t)

;; Prepend an HTML "do not edit" banner.  HTML comments are invisible in
;; both the crates.io and GitHub Markdown renders.
(defun goof/readme-banner (output _backend _info)
  "Prepend a generated-file banner to OUTPUT."
  (concat
   "<!-- Generated from README.org by gen-readme.el; do not edit by hand. -->\n\n"
   output))

;; Assert the export settings we want, then export README.org -> README.md.
(let ((org-export-with-todo-keywords nil)     ; drop the TODO/DONE workflow words
      (org-export-with-sub-superscripts nil)  ; keep `a_b' / `a^b' literal
      (org-export-with-toc nil)
      (org-export-with-section-numbers nil)
      (org-export-filter-final-output-functions
       (cons #'goof/readme-banner org-export-filter-final-output-functions)))
  (with-current-buffer (find-file-noselect "README.org")
    (org-gfm-export-to-markdown)))

(message "Regenerated README.md from README.org")

;;; gen-readme.el ends here
