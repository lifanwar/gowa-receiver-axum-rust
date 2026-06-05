# gowa-webhook-api — Rust Axum Version

Rewrite dari FastAPI Python ke Rust menggunakan Axum, dengan behavior utama dipertahankan:

- `GET /health`
- `POST /webhooks/gowa`
- route lain dikembalikan `404`
- validasi `X-Hub-Signature-256`
- normalisasi payload GOWA
- Redis Pub/Sub publish
- dedup Redis `SET EX NX`
- konfigurasi dari `.env`

## Struktur

```text
src/main.rs          -> pengganti main.py
src/normalizer.rs    -> pengganti normalizer.py
src/redis_pubsub.rs  -> pengganti redis_pubsub.py
src/settings.rs      -> pengganti settings.py
src/signature.rs     -> pengganti signature.py
```

## Cara menjalankan

```bash
cp .env.example .env
cargo run
```

Server listen di:

```text
0.0.0.0:8000
```

## Contoh test health

```bash
curl http://localhost:8000/health
```
