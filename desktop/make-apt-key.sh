#!/usr/bin/env bash
# The APT repository's signing key (decision 0078), made once by the repository's owner on their own computer, so no one
# else ever sees it: it goes into the repository's secret APT_SIGNING_KEY (with gh, signed in as the owner) and into a
# file to keep in a password manager and then delete. Its fingerprint is shown, to compare with the repository's page.
# Usage: make-apt-key.sh [owner/repo]
set -euo pipefail
repo="${1:-ozdeboz/cratarr}"

GNUPGHOME="$(mktemp -d)"
export GNUPGHOME
trap 'rm -rf "$GNUPGHOME"' EXIT
gpg --batch --passphrase '' --quick-generate-key "Cratarr APT repository" ed25519 sign never
fpr=$(gpg --batch --with-colons --list-keys | awk -F: '/^fpr:/ { print $10; exit }')

umask 077
backup="cratarr-apt-signing-key-${fpr: -16}.asc"
gpg --batch --armor --export-secret-keys "$fpr" > "$backup"
gh secret set APT_SIGNING_KEY --repo "$repo" < "$backup"
echo "Fingerprint: $fpr"
echo "Backup: $PWD/$backup. Keep it in your password manager, then delete this file."
