(use-modules
  (guix packages)
  (gnu packages bash)
  (gnu packages certs)
  (gnu packages curl)
  (gnu packages gcc)
  (gnu packages shells)
  (gnu packages version-control)
)

;; Return a manifest containing that one package plus Git.
(specifications->manifest
  (list
    "bash"
    "coreutils"
    "cmake"
    "curl"
    "gawk"
    "gcc"
    "gcc-toolchain"
    "git"
    "grep"
    "make"
    "ninja"
    "nss-certs"
    "python"
    "which"
    "zlib"
    "zsh"
  )
)
