// Native entry point. Wires local backends and serves core use-cases over HTTP (later task).
//
// Backends wired here:
//   FsBlobStore    — blob storage on local filesystem
//   SqliteStore    — PayloadsRepo + MetadataStore via local SQLite
//   MemStore       — in-memory fallback (tests / dev)
//   KvResolver     — id resolution (stub; local KV in a later task)

fn main() {
    println!("braincrawl-server: native entry point (stub)");
}
