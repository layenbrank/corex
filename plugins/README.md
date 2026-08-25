# Corex WASM plugins

Third-party actions can ship as WebAssembly **components** that export the
`corex:plugin-sdk/action` interface defined in
[`crates/plugin-sdk/wit/corex-action.wit`](../crates/plugin-sdk/wit/corex-action.wit).

## Contract

| Item | Value |
|------|--------|
| Package | `corex:plugin-sdk@0.1.0` |
| World | `corex-action` |
| Export | `action` (`meta`, `validate`, `execute`) |
| Values | JSON strings (`json` WIT type) |

`meta` returns `{ id, name, description }`. Action ids should use a reverse-DNS
style prefix unique to your plugin (e.g. `acme.echo`).

`validate(params)` returns an empty string on success, or an error message.

`execute(params, ctx)` receives JSON-encoded params and a JSON snapshot of the
host execution context; it returns `{ ok, payload }` where `payload` is either
a JSON-encoded `Value` or an error string.

## Layout

Place compiled components under the configured plugin directory (default:
`<data-dir>/plugins/`):

```
plugins/
  acme-echo.wasm
  vendor-tools.wasm
```

The daemon (and any caller of `corex_registry::discovery::discover`) scans for
`*.wasm`, loads each via `WasmPluginHost`, and registers successful loads into
the action registry. Failures are logged and skipped.

## Building a guest

1. Author a component that exports `corex:plugin-sdk/action` (see the WIT file).
2. Target `wasm32-wasip2` (or produce a component via `wasm-tools component new`).
3. Copy the `.wasm` into `plugins/`.
4. Restart `corex-daemon` (or re-run discovery).

Example guest toolchain (illustrative):

```bash
# cargo component / wit-bindgen guest for your language
cargo component build --release
cp target/wasm32-wasip2/release/my_plugin.wasm ~/.local/share/corex/corex/plugins/
```

## Host status

`corex-registry` (feature `wasm`, on by default in `full`) creates a real
wasmtime `Engine` with **async** + **component model**, prepares
`WasiCtxBuilder` store state, and parses component bytes.

Full `wasmtime::component::bindgen!` wiring for the `action` interface is the
next step — until it lands, `load_plugin` returns a clear error after parsing,
and discovery logs/skips the file. The host skeleton is intentionally real so
bindgen can drop in without restructuring.

## Feature flag

```toml
# Enable (default via "full")
corex-registry = { features = ["wasm"] }

# Disable WASM host
corex-registry = { default-features = false, features = ["act-shell", "act-file", ...] }
```
