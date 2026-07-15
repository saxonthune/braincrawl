use std::env;

#[test]
fn external_conformance() {
    let url = match env::var("BRAINCRAWL_CONFORMANCE_URL") {
        Ok(u) => u,
        Err(_) => {
            println!("skipped (no BRAINCRAWL_CONFORMANCE_URL set)");
            return;
        }
    };
    let token = env::var("BRAINCRAWL_CONFORMANCE_TOKEN").ok();
    braincrawl_conformance::run_all(&url, token.as_deref()).unwrap();
    braincrawl_conformance::check_health(&url).unwrap();
    braincrawl_conformance::check_cors_preflight(&url).unwrap();
}
