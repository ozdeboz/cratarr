#!/usr/bin/env bash
# The desktop app's APT repository (decision 0078; the owner, 2026-10-29: Mint and Ubuntu update it through apt and the
# Update Manager), as GitHub Pages serves it, under apt/: the latest .deb, its Packages index, the Release file signed
# (InRelease, Release.gpg) with the key in $APT_SIGNING_KEY (an armored private key, a repository secret), the key's
# public half beside it, and a page saying how to add the repository.
# Usage: apt-repo.sh <the .deb> <the site's folder> <the site's address>
set -euo pipefail
deb="$1"
site="$2"
url="${3%/}"
repo="$site/apt"

GNUPGHOME="$(mktemp -d)"
export GNUPGHOME
trap 'rm -rf "$GNUPGHOME"' EXIT
gpg --batch --quiet --import <<<"$APT_SIGNING_KEY"
fpr=$(gpg --batch --with-colons --list-secret-keys | awk -F: '/^fpr:/ { print $10; exit }')

mkdir -p "$repo/pool/main" "$repo/dists/stable/main/binary-amd64"
cp "$deb" "$repo/pool/main/"
(
  cd "$repo"
  dpkg-scanpackages --arch amd64 pool/ > dists/stable/main/binary-amd64/Packages
  gzip -9 -k dists/stable/main/binary-amd64/Packages
  # Written beside, then moved in: a Release already there would be listed in itself.
  apt-ftparchive \
    -o APT::FTPArchive::Release::Origin=Cratarr \
    -o APT::FTPArchive::Release::Label=Cratarr \
    -o APT::FTPArchive::Release::Suite=stable \
    -o APT::FTPArchive::Release::Codename=stable \
    -o APT::FTPArchive::Release::Architectures=amd64 \
    -o APT::FTPArchive::Release::Components=main \
    -o "APT::FTPArchive::Release::Description=Cratarr's desktop app" \
    release dists/stable > Release.new
  mv Release.new dists/stable/Release
  gpg --batch --yes --local-user "$fpr" --clearsign --output dists/stable/InRelease dists/stable/Release
  gpg --batch --yes --local-user "$fpr" --armor --detach-sign --output dists/stable/Release.gpg dists/stable/Release
)
gpg --batch --export "$fpr" > "$repo/cratarr-archive-keyring.gpg"
gpg --batch --armor --export "$fpr" > "$repo/cratarr-archive-keyring.asc"

cat > "$site/index.html" <<HTML
<!doctype html>
<html lang="en">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width">
<title>Cratarr for Linux</title>
<style>body{margin:2rem auto;max-width:46rem;padding:0 1rem;font:16px/1.5 system-ui,sans-serif}pre{overflow-x:auto;padding:1rem;background:#f2f2f2}</style>
<h1>Cratarr for Linux</h1>
<p>The desktop app for Ubuntu, Linux Mint and Debian, from this APT repository, so it updates with the rest of the system.</p>
<pre>sudo install -d -m 0755 /etc/apt/keyrings
curl -fsSL $url/apt/cratarr-archive-keyring.gpg | sudo tee /etc/apt/keyrings/cratarr.gpg &gt; /dev/null
echo "deb [arch=amd64 signed-by=/etc/apt/keyrings/cratarr.gpg] $url/apt stable main" | sudo tee /etc/apt/sources.list.d/cratarr.list
sudo apt update &amp;&amp; sudo apt install cratarr-desktop</pre>
<p>The repository is signed by the key with the fingerprint <code>$fpr</code>.</p>
<p><a href="https://github.com/ozdeboz/cratarr">Cratarr on GitHub</a></p>
</html>
HTML
echo "The APT repository at $url/apt, signed by $fpr"
