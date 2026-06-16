// Native composition root. Wires local adapters and serves core use-cases over HTTP (later task).
//
// Adapters wired here:
//   FsBlobStore    — blob storage on local filesystem
//   SqliteStore    — PayloadsRepo + MetadataStore via local SQLite
//   MemStore       — in-memory fallback (tests / dev)
//   KvResolver     — id resolution (stub; local KV in a later task)

fn main() {
    println!("braincrawl-server: native composition root (stub)");
}
