//! Middleware request-id: setiap request mendapat `x-request-id` (UUIDv7),
//! dikembalikan sebagai header dan disuntik ke body JSON error
//! (field `error.request_id`) agar korelasi log mudah — docs/05 §6.

use std::sync::Arc;

use axum::{
    body::{Body, to_bytes},
    extract::Request,
    http::{HeaderValue, header},
    middleware::Next,
    response::Response,
};

const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;

/// ID request aktif — bisa diambil handler via `Extension<RequestId>` bila perlu.
#[derive(Clone, Debug)]
pub struct RequestId(pub Arc<str>);

pub async fn request_id(req: Request, next: Next) -> Response {
    // Hormati x-request-id dari klien (tracing lintas layanan), else UUIDv7 baru.
    let id: Arc<str> = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| (8..=128).contains(&s.len()))
        .map(|s| Arc::from(s.to_owned()))
        .unwrap_or_else(|| Arc::from(uuid::Uuid::now_v7().to_string()));

    let mut req = req;
    req.extensions_mut().insert(RequestId(id.clone()));

    let mut res = next.run(req).await;

    if let Ok(value) = HeaderValue::from_str(&id) {
        res.headers_mut().insert("x-request-id", value);
    }

    inject_request_id_into_error_body(res, &id).await
}

async fn inject_request_id_into_error_body(res: Response, id: &str) -> Response {
    let is_json = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"));

    if res.status().as_u16() < 400 || !is_json {
        return res;
    }

    let (mut parts, body) = res.into_parts();
    let Ok(bytes) = to_bytes(body, MAX_ERROR_BODY_BYTES).await else {
        // Body error terlalu besar/framing rusak — kembalikan apa adanya (kosong).
        return Response::from_parts(parts, Body::empty());
    };

    // Suntik hanya bila ini envelope error kita ({ "error": {...} }).
    let injected = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|mut v| {
            let err = v.get_mut("error")?.as_object_mut()?;
            err.entry("request_id")
                .or_insert_with(|| serde_json::Value::String(id.to_string()));
            serde_json::to_vec(&v).ok()
        });

    match injected {
        Some(new_body) => {
            let len = new_body.len().to_string();
            if let Ok(cl) = HeaderValue::from_str(&len) {
                parts.headers.insert(header::CONTENT_LENGTH, cl);
            }
            Response::from_parts(parts, Body::from(new_body))
        }
        None => Response::from_parts(parts, Body::from(bytes)),
    }
}
