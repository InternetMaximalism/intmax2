# Intmax2 WASM Client

## Install wasm-pack

```
cargo install wasm-pack
```

## Build for web

```
wasm-pack build --target web
```

## Build for nodejs (js-test)

```
wasm-pack build --target nodejs --out-dir js-test/pkg
```

## Test

```
wasm-pack test --node --release
```