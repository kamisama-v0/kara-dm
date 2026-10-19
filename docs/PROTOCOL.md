# Browser extension ↔ app protocol

Kara Download Manager and the Kara DM extension use the protocol of the Neat Download Manager (NDM) desktop app,
so each app works with each extension. This document describes it as implemented here; it was written from
observed behaviour for interoperability.

## Transport

- WebSocket, `ws://127.0.0.1:10007/download`, subprotocol **`neatextension.v1`**.
- The app accepts the handshake only if `Origin` starts with `moz-extension://` or `chrome-extension://` and the
  subprotocol is offered; otherwise it answers **HTTP 403**. This keeps ordinary web pages (which may open
  WebSockets to localhost) out.
- The extension opens the socket on demand (when a download happens) and reuses it while it is open.
- Messages are UTF-8 **text** frames, at most 1 MiB in kara-dm (the NDM extension caps its own messages at
  118 784 characters).

## App → extension

| Message | Meaning |
| --- | --- |
| `ShowPanelFox=1` / `ShowPanelFox=0` | show / hide the in-page video panel (sent after connect) |
| `waiting` / `nowaiting` | NDM only: whether the app's "add URL" window is waiting. With `waiting`, the NDM extension sends a `HEAD` request first to fill in size and type. kara-dm never sends it. |

## Extension → app: one message per download

A message is a list of `key:value` lines separated by `\r\n`. Values never contain line breaks. Numeric keys:

| Key | Value |
| --- | --- |
| `1` | HTTP method: `GET` or `POST` |
| `2` | URL (`http`, `https` or `ftp`) — required |
| `3` | second URL: the audio stream when `2` is a video-only DASH stream; the app downloads both and merges them (MKV) |
| `4` | page title |
| `5` | page URL |
| `6` | kind: `normal`, `media` or `hls` (`hls` = stream/page for yt-dlp) |
| `7` | size in bytes, if known |
| `8` | MIME type, if known |
| `9` | display label, e.g. `MP4 File 12 MB` (its first word is used as file extension for media) |

Other lines are HTTP headers to send with the request: `Origin`, `Referer`, `Cookie`, `Content-Type`,
`Content-Disposition` and any `x-*` header.

For `POST`, the request body comes last, after the marker `__0NeatPostData9__:` (everything after the marker,
including line breaks, is the body). Without a body the extension sends `Content-Length: 0`.

Example:

```
1:GET
2:https://cdn.example.com/files/archive.zip
6:normal
4:Downloads - Example
Origin: https://example.com
Referer: https://example.com/downloads
5:https://example.com/downloads
Cookie: session=abc123
7:52428800
8:application/zip
```

The app does not reply to download messages.

## How kara-dm handles a message

| Condition | Engine |
| --- | --- |
| `1` is `POST` | curl (single connection, body and headers from private temporary files) |
| `3` present | aria2 for both streams, then ffmpeg `-c copy` → `.mkv` |
| `6` is `hls`, or `2` ends in `.m3u8` | yt-dlp (`-N 8` parallel fragments) |
| otherwise | aria2 with *n* connections; the file name comes from `Content-Disposition` / URL unless `6` is `media` (then page title + extension) |

The reference implementation is [`src/protocol.rs`](../src/protocol.rs) (parser, with unit tests) and
[`extension/src/common.js`](../extension/src/common.js) (`buildMessage`).
