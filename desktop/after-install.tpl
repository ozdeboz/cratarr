#!/bin/bash
# The package's after-install (electron-builder's own, then ours, 0.10.1): a copy of the app still running from the
# files just replaced cannot quit well (its own files are gone from under it; 0.10.0's Quit and SIGTERM did nothing).
# Each running copy, for whoever runs it, is told to quit (SIGTERM, then SIGKILL if it is still there after a few
# seconds) and started again from the new files in that person's own session, as hidden as it was. Nothing here may
# fail the install: every step is allowed to come to nothing.
#
# 0.10.2: 0.10.1's relaunch came within the second the old copy's main process ended, while its helpers and its
# single-instance lock were still going; the new copy found the lock taken and quit as told (main.js), in silence. Now
# the whole old copy and its lock are waited for, the new copy runs in the person's own systemd session (outliving
# this script and runuser's session), and each step goes to the journal (`journalctl -t cratarr-desktop`; the new
# copy's own output under its unit, `journalctl --user -u 'cratarr-relaunch-*'`).
#
# Template note: electron-builder fills in every dollar-brace name made of letters alone (and fails the build on one it
# does not know), so the script's own variables are written without braces; an array's expansion passes through.

if type update-alternatives >/dev/null 2>&1; then
    # Remove previous link if it doesn't use update-alternatives
    if [ -L '/usr/bin/${executable}' -a -e '/usr/bin/${executable}' -a "`readlink '/usr/bin/${executable}'`" != '/etc/alternatives/${executable}' ]; then
        rm -f '/usr/bin/${executable}'
    fi
    update-alternatives --install '/usr/bin/${executable}' '${executable}' '/opt/${sanitizedProductName}/${executable}' 100 || ln -sf '/opt/${sanitizedProductName}/${executable}' '/usr/bin/${executable}'
else
    ln -sf '/opt/${sanitizedProductName}/${executable}' '/usr/bin/${executable}'
fi

# Check if user namespaces are supported by the kernel and working with a quick test:
if ! { [[ -L /proc/self/ns/user ]] && unshare --user true; }; then
    # Use SUID chrome-sandbox only on systems without user namespaces:
    chmod 4755 '/opt/${sanitizedProductName}/chrome-sandbox' || true
else
    chmod 0755 '/opt/${sanitizedProductName}/chrome-sandbox' || true
fi

if hash update-mime-database 2>/dev/null; then
    update-mime-database /usr/share/mime || true
fi

if hash update-desktop-database 2>/dev/null; then
    update-desktop-database /usr/share/applications || true
fi

# Install apparmor profile. (Ubuntu 24+)
# First check if the version of AppArmor running on the device supports our profile.
# This is in order to keep backwards compatibility with Ubuntu 22.04 which does not support abi/4.0.
# In that case, we just skip installing the profile since the app runs fine without it on 22.04.
#
# Those apparmor_parser flags are akin to performing a dry run of loading a profile.
# https://wiki.debian.org/AppArmor/HowToUse#Dumping_profiles
#
# Unfortunately, at the moment AppArmor doesn't have a good story for backwards compatibility.
# https://askubuntu.com/questions/1517272/writing-a-backwards-compatible-apparmor-profile
if apparmor_status --enabled > /dev/null 2>&1; then
  APPARMOR_PROFILE_SOURCE='/opt/${sanitizedProductName}/resources/apparmor-profile'
  APPARMOR_PROFILE_TARGET='/etc/apparmor.d/${executable}'
  if apparmor_parser --skip-kernel-load --debug "$APPARMOR_PROFILE_SOURCE" > /dev/null 2>&1; then
    cp -f "$APPARMOR_PROFILE_SOURCE" "$APPARMOR_PROFILE_TARGET"

    # Updating the current AppArmor profile is not possible and probably not meaningful in a chroot'ed environment.
    # Use cases are for example environments where images for clients are maintained.
    # There, AppArmor might correctly be installed, but live updating makes no sense.
    if ! { [ -x '/usr/bin/ischroot' ] && /usr/bin/ischroot; } && hash apparmor_parser 2>/dev/null; then
      # Extra flags taken from dh_apparmor:
      # > By using '-W -T' we ensure that any abstraction updates are also pulled in.
      # https://wiki.debian.org/AppArmor/Contribute/FirstTimeProfileImport
      apparmor_parser --replace --write-cache --skip-read-cache "$APPARMOR_PROFILE_TARGET"
    fi
  else
    echo "Skipping the installation of the AppArmor profile as this version of AppArmor does not seem to support the bundled profile"
  fi
fi

# --- a copy still running: quit, and back from the new files ---
APP_EXE='/opt/${sanitizedProductName}/${executable}'
say() { command -v logger >/dev/null 2>&1 && logger -t cratarr-desktop -- "$*"; return 0; }
# Whether any process of the app (its main or a helper) still runs for this user.
running_for() {
  for p in /proc/[0-9]*; do
    e=$(readlink "$p/exe" 2>/dev/null) || continue
    case "$e" in
      "$APP_EXE" | "$APP_EXE (deleted)") [ "$(stat -c %U "$p" 2>/dev/null)" = "$1" ] && return 0 ;;
    esac
  done
  return 1
}
# Whether the user's single-instance lock (a link to "host-pid" in the app's folder) is still held by a live process.
locked_for() {
  home=$(getent passwd "$1" | cut -d: -f6)
  [ -n "$home" ] || return 1
  target=$(readlink "$home/.config/${productFilename}/SingletonLock" 2>/dev/null) || return 1
  holder=$(printf '%s' "$target" | sed 's/.*-//')
  [ -n "$holder" ] && kill -0 "$holder" 2>/dev/null
}
for proc in /proc/[0-9]*; do
  [ -r "$proc/cmdline" ] || continue
  exe=$(readlink "$proc/exe" 2>/dev/null) || continue
  # The app's main process only: its helpers (renderer, GPU, zygote) carry a --type= argument.
  case "$exe" in
    "$APP_EXE" | "$APP_EXE (deleted)") ;;
    *) continue ;;
  esac
  tr '\0' '\n' < "$proc/cmdline" | grep -q '^--type=' && continue
  pid=$(basename "$proc")
  owner=$(stat -c %U "$proc" 2>/dev/null) || continue
  hidden=''
  tr '\0' '\n' < "$proc/cmdline" | grep -qx -- '--hidden' && hidden='--hidden'
  # What its session gave it (the display, the session bus), to start the new one there.
  envfile=$(mktemp) || continue
  tr '\0' '\n' < "$proc/environ" 2>/dev/null | grep -E '^(DISPLAY|WAYLAND_DISPLAY|XAUTHORITY|DBUS_SESSION_BUS_ADDRESS|XDG_RUNTIME_DIR|XDG_SESSION_TYPE|XDG_CURRENT_DESKTOP|XDG_DATA_DIRS|HOME|USER|LOGNAME|LANG|LANGUAGE|LC_ALL)=' > "$envfile"
  say "quitting the running copy $pid of $owner $hidden"
  kill -TERM "$pid" 2>/dev/null || true
  waited=0
  while kill -0 "$pid" 2>/dev/null && [ "$waited" -lt 5 ]; do sleep 1; waited=$((waited + 1)); done
  kill -0 "$pid" 2>/dev/null && kill -KILL "$pid" 2>/dev/null && say "copy $pid killed after $waited s"
  # The whole copy gone (its helpers too) and its lock let go, at most 10 s more; its helpers are ended if they stay.
  waited=0
  while { running_for "$owner" || locked_for "$owner"; } && [ "$waited" -lt 20 ]; do sleep 0.5; waited=$((waited + 1)); done
  if running_for "$owner"; then
    say "helpers of $owner's copy still there after 10 s: ended"
    for p in /proc/[0-9]*; do
      e=$(readlink "$p/exe" 2>/dev/null) || continue
      case "$e" in
        "$APP_EXE" | "$APP_EXE (deleted)") [ "$(stat -c %U "$p" 2>/dev/null)" = "$owner" ] && kill -KILL "$(basename "$p")" 2>/dev/null ;;
      esac
    done
    sleep 1
  fi
  locked_for "$owner" && say "the lock of $owner is still held: the new copy may find it taken"
  set -- PATH=/usr/local/bin:/usr/bin:/bin
  while IFS= read -r line; do set -- "$@" "$line"; done < "$envfile"
  rm -f "$envfile"
  [ "$owner" != root ] || continue
  # In the person's own systemd session: the new copy outlives this script, and its output lands in their journal.
  if command -v systemd-run >/dev/null 2>&1; then
    setenv=()
    for kv in "$@"; do setenv+=(-E "$kv"); done
    if systemd-run --user --machine="$owner@" --quiet --collect --unit="cratarr-relaunch-$pid" "${setenv[@]}" "$APP_EXE" $hidden > /dev/null 2>&1; then
      say "started again for $owner in their systemd session"
      continue
    fi
    say "systemd-run for $owner failed; starting with runuser instead"
  fi
  if command -v runuser >/dev/null 2>&1; then
    runuser -u "$owner" -- env -i "$@" setsid -f "$APP_EXE" $hidden < /dev/null > /dev/null 2>&1 &
    say "started again for $owner with runuser"
  fi
done
exit 0
