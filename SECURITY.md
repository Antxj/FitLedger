# Security policy

## Supported versions

Only the latest release published on the
[Releases](https://github.com/Antxj/FitLedger/releases) page receives fixes. The app checks for new
versions by itself and can update with one click.

## Reporting a vulnerability

Please **do not** open a public issue for security problems. Report them privately through GitHub
instead: open the repository's **Security** tab and click **Report a vulnerability**.

Include what you found, how to reproduce it and the app version (shown at the bottom of the side
menu). You will get an answer as soon as possible, and the fix will be credited to you in the
release notes if you wish.

## Scope

Of particular interest:

- parsing of `.fit` files (they are untrusted input: a crafted file must never crash the app or do
  anything beyond being rejected);
- restoring a backup file (`.fitledger`);
- the update mechanism (GitHub API query, download, SHA-256 and signature checks, executable
  replacement);
- anything that could send activity, health or location data off the computer.

---

**Português:** não abra issue pública para problemas de segurança. Use a aba **Security** ›
**Report a vulnerability** deste repositório.
