use reqwest::blocking::Client;
use serde_json::Value;
use std::time::Duration;

pub fn run_all(base_url: &str, token: Option<&str>) -> Result<(), String> {
    // A bounded timeout turns a wedged server into a fast, legible failure
    // instead of an indefinite hang.
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("client build failed: {e}"))?;
    // stats must run first: it checks the empty-store zeros before other cases insert data
    stats(&client, base_url, token)?;
    put_and_get_work(&client, base_url, token)?;
    have(&client, base_url, token)?;
    content_roundtrip(&client, base_url, token)?;
    large_content(&client, base_url, token)?;
    neighborhood(&client, base_url, token)?;
    Ok(())
}

fn auth(req: reqwest::blocking::RequestBuilder, token: Option<&str>) -> reqwest::blocking::RequestBuilder {
    match token {
        Some(t) => req.header("Authorization", format!("Bearer {t}")),
        None => req,
    }
}

fn put_and_get_work(client: &Client, base: &str, token: Option<&str>) -> Result<(), String> {
    let body = serde_json::json!({
        "source": "openalex",
        "kind": "Work",
        "aliases": [{"namespace": "doi", "value": "10.99/smoke"}],
        "attrs": {"title": "Smoke Test Paper"}
    });
    let res = auth(client.put(format!("{base}/works")).json(&body), token)
        .send()
        .map_err(|e| format!("put_and_get_work: PUT /works request failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("put_and_get_work: PUT /works expected 200, got {}", res.status()));
    }
    let json: Value = res.json()
        .map_err(|e| format!("put_and_get_work: PUT /works response parse failed: {e}"))?;
    if !json["id"].as_str().unwrap_or("").starts_with("guid:") {
        return Err(format!("put_and_get_work: id should start with 'guid:', got {:?}", json["id"]));
    }

    let res = auth(client.get(format!("{base}/works/doi:10.99/smoke")), token)
        .send()
        .map_err(|e| format!("put_and_get_work: GET /works/doi:10.99/smoke failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("put_and_get_work: GET existing work expected 200, got {}", res.status()));
    }

    let res = auth(client.get(format!("{base}/works/doi:10.99/does-not-exist")), token)
        .send()
        .map_err(|e| format!("put_and_get_work: GET unknown work failed: {e}"))?;
    if res.status() != 404 {
        return Err(format!("put_and_get_work: GET unknown work expected 404, got {}", res.status()));
    }

    Ok(())
}

fn have(client: &Client, base: &str, token: Option<&str>) -> Result<(), String> {
    let res = auth(
        client.post(format!("{base}/works/have"))
            .json(&serde_json::json!({"ids": ["doi:10.0/none"]})),
        token,
    )
    .send()
    .map_err(|e| format!("have: POST /works/have (empty) failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("have: POST /works/have expected 200, got {}", res.status()));
    }
    let present: Vec<String> = res.json()
        .map_err(|e| format!("have: POST /works/have parse failed: {e}"))?;
    if !present.is_empty() {
        return Err(format!("have: expected empty present list, got {present:?}"));
    }

    auth(
        client.put(format!("{base}/works"))
            .json(&serde_json::json!({
                "source": "test",
                "kind": "Work",
                "aliases": [{"namespace": "doi", "value": "10.0/known"}],
                "attrs": {}
            })),
        token,
    )
    .send()
    .map_err(|e| format!("have: PUT work failed: {e}"))?;

    let res = auth(
        client.post(format!("{base}/works/have"))
            .json(&serde_json::json!({"ids": ["doi:10.0/known", "doi:10.0/none"]})),
        token,
    )
    .send()
    .map_err(|e| format!("have: POST /works/have (after put) failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("have: POST /works/have (after put) expected 200, got {}", res.status()));
    }
    let present: Vec<String> = res.json()
        .map_err(|e| format!("have: POST /works/have (after put) parse failed: {e}"))?;
    if present != vec!["doi:10.0/known"] {
        return Err(format!("have: expected [\"doi:10.0/known\"], got {present:?}"));
    }

    Ok(())
}

fn content_roundtrip(client: &Client, base: &str, token: Option<&str>) -> Result<(), String> {
    auth(
        client.put(format!("{base}/works"))
            .json(&serde_json::json!({
                "source": "test",
                "kind": "Work",
                "aliases": [{"namespace": "doi", "value": "10.1"}],
                "attrs": {}
            })),
        token,
    )
    .send()
    .map_err(|e| format!("content_roundtrip: PUT work failed: {e}"))?;

    let res = auth(
        client.put(format!(
            "{base}/works/doi:10.1/content/abstract?mime=text/plain&fetched_at=2024-01-01T00:00:00Z"
        ))
        .body("hello braincrawl"),
        token,
    )
    .send()
    .map_err(|e| format!("content_roundtrip: PUT content failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("content_roundtrip: PUT content expected 200, got {}", res.status()));
    }

    let res = auth(
        client.get(format!("{base}/works/doi:10.1/content/abstract")),
        token,
    )
    .send()
    .map_err(|e| format!("content_roundtrip: GET content failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("content_roundtrip: GET content expected 200, got {}", res.status()));
    }
    let body = res.text()
        .map_err(|e| format!("content_roundtrip: GET content body read failed: {e}"))?;
    if body != "hello braincrawl" {
        return Err(format!("content_roundtrip: body mismatch: expected 'hello braincrawl', got {body:?}"));
    }

    Ok(())
}

fn large_content(client: &Client, base: &str, token: Option<&str>) -> Result<(), String> {
    auth(
        client.put(format!("{base}/works"))
            .json(&serde_json::json!({
                "source": "test",
                "kind": "Work",
                "aliases": [{"namespace": "doi", "value": "10.2/large"}],
                "attrs": {}
            })),
        token,
    )
    .send()
    .map_err(|e| format!("large_content: PUT work failed: {e}"))?;

    let large_body: Vec<u8> = (0u8..=255).cycle().take(5 * 1024 * 1024).collect();

    let res = auth(
        client.put(format!(
            "{base}/works/doi:10.2/large/content/fulltext?mime=application/pdf&fetched_at=2024-01-01T00:00:00Z"
        ))
        .body(large_body.clone()),
        token,
    )
    .send()
    .map_err(|e| format!("large_content: PUT large content failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("large_content: PUT large content must not 413, got {}", res.status()));
    }

    let res = auth(
        client.get(format!("{base}/works/doi:10.2/large/content/fulltext")),
        token,
    )
    .send()
    .map_err(|e| format!("large_content: GET large content failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("large_content: GET large content expected 200, got {}", res.status()));
    }
    let returned = res.bytes()
        .map_err(|e| format!("large_content: GET large content body read failed: {e}"))?;
    if returned.as_ref() != large_body.as_slice() {
        return Err("large_content: bytes did not round-trip".to_string());
    }

    Ok(())
}

fn stats(client: &Client, base: &str, token: Option<&str>) -> Result<(), String> {
    let res = auth(client.get(format!("{base}/stats")), token)
        .send()
        .map_err(|e| format!("stats: GET /stats failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("stats: GET /stats expected 200, got {}", res.status()));
    }
    let body: Value = res.json()
        .map_err(|e| format!("stats: GET /stats parse failed: {e}"))?;
    if body["works"] != 0 {
        return Err(format!("stats: expected works=0, got {}", body["works"]));
    }
    if body["edges_total"] != 0 {
        return Err(format!("stats: expected edges_total=0, got {}", body["edges_total"]));
    }

    auth(
        client.put(format!("{base}/works"))
            .json(&serde_json::json!({
                "source": "openalex",
                "kind": "Work",
                "aliases": [{"namespace": "doi", "value": "10.1/described"}],
                "attrs": {"title": "Described"}
            })),
        token,
    )
    .send()
    .map_err(|e| format!("stats: PUT described work failed: {e}"))?;

    auth(
        client.put(format!("{base}/edges"))
            .json(&serde_json::json!([{
                "src": {"namespace": "doi", "value": "10.1/described"},
                "dst": {"namespace": "doi", "value": "10.1/stub"},
                "relation": "cites",
                "source": "openalex",
                "attrs": null,
                "fetched_at": "2024-01-01T00:00:00Z"
            }])),
        token,
    )
    .send()
    .map_err(|e| format!("stats: PUT edge failed: {e}"))?;

    let res = auth(client.get(format!("{base}/stats")), token)
        .send()
        .map_err(|e| format!("stats: GET /stats (after) failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("stats: GET /stats (after) expected 200, got {}", res.status()));
    }
    let body: Value = res.json()
        .map_err(|e| format!("stats: GET /stats (after) parse failed: {e}"))?;

    if body["works"] != 2 {
        return Err(format!("stats: expected works=2, got {}", body["works"]));
    }
    if body["works_described"] != 1 {
        return Err(format!("stats: expected works_described=1, got {}", body["works_described"]));
    }
    if body["works_stub"] != 1 {
        return Err(format!("stats: expected works_stub=1, got {}", body["works_stub"]));
    }
    if body["nodes_total"] != 2 {
        return Err(format!("stats: expected nodes_total=2, got {}", body["nodes_total"]));
    }
    if body["tombstones"] != 0 {
        return Err(format!("stats: expected tombstones=0, got {}", body["tombstones"]));
    }
    if body["edges_total"] != 1 {
        return Err(format!("stats: expected edges_total=1, got {}", body["edges_total"]));
    }

    let kinds = body["nodes_by_kind"].as_array()
        .ok_or("stats: nodes_by_kind missing or not array")?;
    if kinds.is_empty() || kinds[0]["key"] != "work" || kinds[0]["count"] != 2 {
        return Err(format!("stats: nodes_by_kind unexpected: {kinds:?}"));
    }

    let rels = body["edges_by_relation"].as_array()
        .ok_or("stats: edges_by_relation missing or not array")?;
    if rels.is_empty() || rels[0]["key"] != "cites" || rels[0]["count"] != 1 {
        return Err(format!("stats: edges_by_relation unexpected: {rels:?}"));
    }

    let sources = body["assertions_by_source"].as_array()
        .ok_or("stats: assertions_by_source missing or not array")?;
    if sources.is_empty() || sources[0]["key"] != "openalex" || sources[0]["count"] != 1 {
        return Err(format!("stats: assertions_by_source unexpected: {sources:?}"));
    }

    Ok(())
}

fn neighborhood(client: &Client, base: &str, token: Option<&str>) -> Result<(), String> {
    auth(
        client.put(format!("{base}/works"))
            .json(&serde_json::json!({
                "source": "test",
                "kind": "Work",
                "aliases": [{"namespace": "doi", "value": "10.1/src"}],
                "attrs": {"title": "Source"}
            })),
        token,
    )
    .send()
    .map_err(|e| format!("neighborhood: PUT src work failed: {e}"))?;

    auth(
        client.put(format!("{base}/works"))
            .json(&serde_json::json!({
                "source": "test",
                "kind": "Work",
                "aliases": [{"namespace": "doi", "value": "10.1/dst"}],
                "attrs": {"title": "Dest"}
            })),
        token,
    )
    .send()
    .map_err(|e| format!("neighborhood: PUT dst work failed: {e}"))?;

    auth(
        client.put(format!("{base}/edges"))
            .json(&serde_json::json!([{
                "src": {"namespace": "doi", "value": "10.1/src"},
                "dst": {"namespace": "doi", "value": "10.1/dst"},
                "relation": "cites",
                "source": "test",
                "attrs": null,
                "fetched_at": "2024-01-01T00:00:00Z"
            }])),
        token,
    )
    .send()
    .map_err(|e| format!("neighborhood: PUT edge failed: {e}"))?;

    let res = auth(
        client.post(format!("{base}/graph/neighborhood"))
            .json(&serde_json::json!({
                "seeds": ["doi:10.1/src"],
                "dir": "forward",
                "depth": 1,
                "max_nodes": 50
            })),
        token,
    )
    .send()
    .map_err(|e| format!("neighborhood: POST /graph/neighborhood failed: {e}"))?;
    if res.status() != 200 {
        return Err(format!("neighborhood: POST /graph/neighborhood expected 200, got {}", res.status()));
    }

    let body: Value = res.json()
        .map_err(|e| format!("neighborhood: POST /graph/neighborhood parse failed: {e}"))?;

    if body.get("nodes").is_none() {
        return Err("neighborhood: response missing 'nodes'".to_string());
    }
    if body.get("edges").is_none() {
        return Err("neighborhood: response missing 'edges'".to_string());
    }
    if body.get("truncated").is_none() {
        return Err("neighborhood: response missing 'truncated'".to_string());
    }

    let nodes = body["nodes"].as_array()
        .ok_or("neighborhood: nodes not an array")?;
    if nodes.len() != 2 {
        return Err(format!("neighborhood: expected 2 nodes, got {}", nodes.len()));
    }

    let edges = body["edges"].as_array()
        .ok_or("neighborhood: edges not an array")?;
    if edges.len() != 1 {
        return Err(format!("neighborhood: expected 1 edge, got {}", edges.len()));
    }

    if body["truncated"].as_bool() != Some(false) {
        return Err(format!("neighborhood: expected truncated=false, got {:?}", body["truncated"]));
    }

    let res = auth(
        client.post(format!("{base}/graph/neighborhood"))
            .json(&serde_json::json!({
                "seeds": ["doi:10.1/src"],
                "dir": "sideways",
                "depth": 1,
                "max_nodes": 50
            })),
        token,
    )
    .send()
    .map_err(|e| format!("neighborhood: POST /graph/neighborhood (bad dir) failed: {e}"))?;
    if res.status() != 400 {
        return Err(format!("neighborhood: bad dir expected 400, got {}", res.status()));
    }

    Ok(())
}
