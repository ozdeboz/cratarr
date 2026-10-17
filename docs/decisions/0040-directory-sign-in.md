# 0040: A directory (LDAP) asked first at sign-in

- Status: accepted (the owner, 2026-10-17: "get to work"; designed with claude-5f and claude-a5)
- Date: 2026-10-17
- Builds on: 0030 (Cratarr's accounts; a directory as a source, never a service Cratarr needs while it runs), 0039 (the
  sign-in page, its one place where an account is checked)

## Context

The owner keeps the household's users in a directory (LLDAP) and wants one password for Jellyfin and Cratarr. Cratarr's
own accounts must stay as the fallback ("we need the fallback login anyway, even if we later switch to LLDAP",
2026-10-16), and the OpenSubsonic API cannot use a directory at all: its token login needs the password itself, which a
directory never gives out.

## Decision

`Serve::sign_in` asks the directory first, when one is set, and Cratarr's own accounts after.

- **Settings** (Settings → Authentication → Directory, kept in `ldap.json` in the data folder, owner-only): the address
  (`ldap://` or `ldaps://`), StartTLS, the base users are under, the name attribute (`uid`; `sAMAccountName` for Active
  Directory), a group the user must be a member of (optional), the read-only service account the lookup binds as, the
  path of the file holding its password, and directory names that sign in as a differently named account. Nothing is
  assumed: no install's values in the code. The password file is read at each use, never kept, logged or shown: the
  page sees only "set", "not set" or "unreadable". A Test binds as the service account and can look a name up.
- **A sign-in** binds as the service account, searches the base for the name typed (and the group), and binds as the
  one user found with the password typed. Each exchange has a 3 s timeout. The directory name, as the directory spells
  it, signs in as the Cratarr account of that name or the one mapped to it, which must exist: sessions, the
  `X-Cratarr-User` header, user data and the sign-out at a password change all key on that account.
- **Before any exchange** an empty password is refused (a bind with one is an anonymous bind, which succeeds), and so is
  a name with control characters or over 256 bytes. The name and the group go into the filter escaped (RFC 4515).
- **Plain LDAP** only to this machine's networks (a container name, localhost, a private or loopback address);
  anything further needs `ldaps://` or StartTLS (rustls). Saving refuses otherwise.
- **While the directory cannot be reached** (no connection, a timeout, the service account refused), a sign-in the
  directory accepted in the last 30 days still works: each accepted sign-in keeps an Argon2id hash of the password
  (OWASP's minimum: 19 MiB, two passes, one lane; one hash at a time) in `ldap-verifiers.json` (owner-only), never the
  password, filed under the name in lower case (directories match names regardless of case) with the directory's own
  spelling kept for the account it maps to. A refusal by the directory (no such user in the group, or invalidCredentials)
  deletes that name's hash at once, so a password changed or a user removed there stops working here too; a change of address, base or group
  deletes them all. This keeps 0030: Cratarr never needs the directory to let its owner in.
- **The answer** is the same for an unknown name, a wrong password and the directory being down; the sign-in throttle
  (0039) stays in front of it all.
- **Cratarr's own accounts** sign in as before when the directory does not accept the name and password, and remain
  the only login of the OpenSubsonic API.

## Options not taken

- Copying the directory's users into Cratarr: a directory gives no passwords, so a copied account could not sign in.
- Signing in through the directory only, with no fallback: Cratarr would need it running (0030).
- Keeping the password itself for when the directory is down: one stolen file would give every password away.

## Consequences

- One password for the page away from home and Jellyfin; Subsonic apps keep Cratarr's own account.
- A password changed in the directory does not sign out devices already signed in (their sessions stay until signed
  out in Settings, or 90 days unused); changing Cratarr's own password still does.
- The data folder holds two more owner-only files: `ldap.json` and `ldap-verifiers.json`.
