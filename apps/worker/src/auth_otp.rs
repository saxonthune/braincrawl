//! `/api/auth/request-code` and `/api/auth/verify` — email OTP sign-in.
//!
//! Mounted in `lib.rs` before the auth gate: these routes ARE the way in.
//! Both endpoints answer uniformly regardless of whether the email is
//! allowlisted, so membership can't be probed from the response.

use braincrawl_auth::{email_allowed, hash_token, normalize_email, OtpRecord};
use serde::Deserialize;
use worker::{console_log, Env, Fetch, Headers, Method, Request, RequestInit, Response};

const RATE_LIMIT_WINDOW_SECS: u64 = 3600;
const RATE_LIMIT_MAX: u32 = 5;
const OTP_TTL_SECS: u64 = 600;
const OTP_MAX_ATTEMPTS: u32 = 5;
const SESSION_TTL_SECS: u64 = 2592000; // 30 days

#[derive(Deserialize)]
struct RequestCodeBody {
    email: String,
}

#[derive(Deserialize)]
struct VerifyBody {
    email: String,
    code: String,
}

fn ok_ack() -> worker::Result<Response> {
    Response::from_json(&serde_json::json!({"ok": true}))
}

fn invalid() -> worker::Result<Response> {
    Ok(Response::from_json(&serde_json::json!({"error": "invalid"}))?.with_status(401))
}

fn gen_digits(n_bytes: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n_bytes];
    getrandom::getrandom(&mut buf).expect("getrandom failure is unrecoverable");
    buf
}

/// A 6-digit numeric code, zero-padded.
fn gen_code() -> String {
    let buf = gen_digits(4);
    let n = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) % 1_000_000;
    format!("{n:06}")
}

/// A 32-byte session token, hex-encoded.
fn gen_token() -> String {
    gen_digits(32).iter().map(|b| format!("{b:02x}")).collect()
}

// ── POST /api/auth/request-code ─────────────────────────────────────────────

pub async fn handle_request_code(mut req: Request, env: &Env) -> worker::Result<Response> {
    let body: RequestCodeBody = match req.json().await {
        Ok(b) => b,
        Err(_) => return ok_ack(),
    };
    let email = normalize_email(&body.email);
    let email_hash = hash_token(&email);
    let kv = env.kv("AUTH_KV")?;

    let rl_key = format!("otp-rl/{email_hash}");
    let count: u32 = kv
        .get(&rl_key)
        .text()
        .await?
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if count >= RATE_LIMIT_MAX {
        return ok_ack();
    }
    kv.put(&rl_key, (count + 1).to_string())?
        .expiration_ttl(RATE_LIMIT_WINDOW_SECS)
        .execute()
        .await?;

    let allowed = match env.secret("ALLOWED_EMAILS") {
        Ok(s) => email_allowed(&s.to_string(), &email),
        Err(_) => false,
    };
    if !allowed {
        return ok_ack();
    }

    let code = gen_code();
    let record = OtpRecord { code_hash: hash_token(&code), attempts: 0 };
    kv.put(&format!("otp/{email_hash}"), record.to_json())?
        .expiration_ttl(OTP_TTL_SECS)
        .execute()
        .await?;

    send_code_email(env, &email, &code).await;

    ok_ack()
}

async fn send_code_email(env: &Env, to: &str, code: &str) {
    let api_key = match env.secret("RESEND_API_KEY") {
        Ok(s) => s.to_string(),
        Err(_) => return,
    };
    let from = match env.var("EMAIL_FROM") {
        Ok(v) => v.to_string(),
        Err(_) => {
            console_log!("auth_otp: EMAIL_FROM not configured, skipping send");
            return;
        }
    };

    let payload = serde_json::json!({
        "from": from,
        "to": [to],
        "subject": "Your braincrawl sign-in code",
        "text": format!("Your sign-in code is {code}. It expires in 10 minutes."),
    });

    let headers = Headers::new();
    if let Err(e) = headers.set("Authorization", &format!("Bearer {api_key}")) {
        console_log!("auth_otp: header set failed: {e}");
        return;
    }
    if let Err(e) = headers.set("Content-Type", "application/json") {
        console_log!("auth_otp: header set failed: {e}");
        return;
    }

    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_headers(headers);
    init.with_body(Some(worker::wasm_bindgen::JsValue::from_str(&payload.to_string())));

    let req = match Request::new_with_init("https://api.resend.com/emails", &init) {
        Ok(r) => r,
        Err(e) => {
            console_log!("auth_otp: request build failed: {e}");
            return;
        }
    };

    match Fetch::Request(req).send().await {
        Ok(resp) if resp.status_code() >= 400 => {
            console_log!("auth_otp: resend send failed, status={}", resp.status_code());
        }
        Ok(_) => {}
        Err(e) => console_log!("auth_otp: resend fetch failed: {e}"),
    }
}

// ── POST /api/auth/verify ───────────────────────────────────────────────────

pub async fn handle_verify(mut req: Request, env: &Env) -> worker::Result<Response> {
    let body: VerifyBody = match req.json().await {
        Ok(b) => b,
        Err(_) => return invalid(),
    };
    let email = normalize_email(&body.email);
    let email_hash = hash_token(&email);
    let kv = env.kv("AUTH_KV")?;
    let otp_key = format!("otp/{email_hash}");

    let record = match kv.get(&otp_key).text().await?.and_then(|s| OtpRecord::parse(&s)) {
        Some(r) => r,
        None => return invalid(),
    };

    if record.attempts >= OTP_MAX_ATTEMPTS {
        kv.delete(&otp_key).await?;
        return invalid();
    }

    if hash_token(&body.code) != record.code_hash {
        let updated = OtpRecord { code_hash: record.code_hash, attempts: record.attempts + 1 };
        kv.put(&otp_key, updated.to_json())?
            .expiration_ttl(OTP_TTL_SECS)
            .execute()
            .await?;
        return invalid();
    }

    kv.delete(&otp_key).await?;

    let token = gen_token();
    let entry = serde_json::json!({"tenant": "default", "status": "active"});
    kv.put(&hash_token(&token), entry.to_string())?
        .expiration_ttl(SESSION_TTL_SECS)
        .execute()
        .await?;

    Response::from_json(&serde_json::json!({"token": token}))
}
