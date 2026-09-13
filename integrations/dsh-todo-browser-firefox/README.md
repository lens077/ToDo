# DSH Todo Firefox extension

Standalone Firefox MV2 extension that opens a validated DSH web session and reuses an existing DSH tab when possible. It is web-mode only; Tauri/native mode does not call the extension and uses ToDo's existing session-window behavior.

## Temporary install

Open `about:debugging` -> This Firefox -> Load Temporary Add-on, then select `manifest.json`. Configure the exact local app origin in `config.js` (default `http://localhost:1420`) and reload the extension. Content scripts run only on localhost/127.0.0.1 pages; browser tabs permission is required.

The accepted target is exactly `http://127.0.0.1:3080/` or `http://localhost:3080/` with one safe `session` query (`[A-Za-z0-9_-]+`, max 128 UTF-8 bytes), no hash or other query. Requests expire after 1.5 seconds and the app should time out after 2 seconds.

## Tests

```sh
pnpm test
```
