# jellyfin-client

Headless async Jellyfin HTTP client (Quick Connect, library browsing, PlaybackInfo, device profiles, session reporting). It has no OpenXR,
decoder or DeskQuest dependencies.

```toml
[dependencies]
jellyfin-client = "0.1"
```

Pairing, against Jellyfin 10.8 and later (the homelab server is 10.11):

1. `POST /QuickConnect/Initiate` returns a short code and a secret.
2. Poll `GET /QuickConnect/Connect?secret=...` until `Authenticated` is true.
3. `POST /Users/AuthenticateWithQuickConnect` with `{ "Secret": "..." }` returns
   the access token. The client saves it through a `TokenStore`.

`GET /Items` always sends `Fields=Video3DFormat,MediaStreams,Path` plus
`StartIndex` and `Limit`. Requests use
`Authorization: MediaBrowser Client="...", Device="...", DeviceId="...", Version="...", Token="..."`.
The token is omitted until pairing succeeds. Header values are percent-encoded.

`JellyfinClient::new` loads a saved session. `clear_session` drops it. Tests use
`MemoryTokenStore` and a mocked HTTP server. No live server and no real token
are required.
