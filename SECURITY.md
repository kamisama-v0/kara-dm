# Security policy

## Supported versions

Security fixes go into the latest release. Please update before reporting.

## Reporting a vulnerability

Please report privately through GitHub:
**[Report a vulnerability](https://github.com/kamisama-v0/kara-dm/security/advisories/new)**
(Security → Advisories). Do not open a public issue for security problems.

Include what you found, how to reproduce it, the affected version and OS, and the impact you expect. We aim to
answer within a week and to coordinate a fix and disclosure date with you.

## Scope and design

Kara Download Manager runs local network services, so these are especially interesting:

- **WebSocket server on `127.0.0.1:10007`** — accepts only handshakes whose `Origin` is a browser extension
  (`moz-extension://`, `chrome-extension://`) and that request the `neatextension.v1` subprotocol; web pages
  are rejected with HTTP 403. Messages are size-limited; only `http`, `https` and `ftp` URLs are accepted.
- **aria2 JSON-RPC** — listens on loopback only, on a random port, protected by a random per-session token
  that is written to a private file (never passed on the command line).
- **Windows single-instance port `127.0.0.1:10008`** — forwards command-line arguments to the running
  instance; it can do what the `kara-dm` command line can do.
- **File names** from servers and pages are sanitised (no path separators, Windows-reserved names or
  characters); downloads are written only inside the chosen download folder.
- **Cookies** received from the extension are kept in memory and passed to aria2/yt-dlp/curl through private
  temporary files; they are not stored in the download history.
- The **browser extension** sends data only to `127.0.0.1`, has no remote code and no analytics.

Reports that need a malicious browser extension or local malware already running as your user are generally out
of scope, but feel free to ask.
