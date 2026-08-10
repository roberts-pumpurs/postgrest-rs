use std::future::pending;
use std::time::Duration;

use postgrest::reqwest::StatusCode;
use postgrest::rp_postgrest_error::{ErrorKind, PostgrestErrorCode};
use postgrest::{ExecuteError, Postgrest};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn serve_once(
    status_line: &str,
    content_type: &str,
    body: &[u8],
    declared_length: usize,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut response = format!(
        "HTTP/1.1 {status_line}\r\nContent-Type: {content_type}\r\nContent-Length: {declared_length}\r\nX-Request-Id: request-123\r\nConnection: close\r\n\r\n"
    )
    .into_bytes();
    response.extend_from_slice(body);

    let server = tokio::spawn(async move {
        let (mut connection, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        loop {
            let mut chunk = [0_u8; 1024];
            let read = connection.read(&mut chunk).await.unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }

        connection.write_all(&response).await.unwrap();
        connection.shutdown().await.unwrap();
    });

    (format!("http://{address}"), server)
}

#[tokio::test]
async fn checked_execution_preserves_success_response() {
    let body = br#"{"ok":true}"#;
    let (url, server) = serve_once("201 Created", "application/json", body, body.len()).await;

    let response = Postgrest::new(url)
        .from("items")
        .insert(r#"{"name":"rust"}"#)
        .execute_checked()
        .await
        .expect("success response should be returned");

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.bytes().await.unwrap().as_ref(), body);
    server.await.unwrap();
}

#[tokio::test]
async fn raw_execution_preserves_non_success_response() {
    let body = b"raw upstream failure";
    let (url, server) = serve_once("502 Bad Gateway", "text/plain", body, body.len()).await;

    let response = Postgrest::new(url)
        .from("items")
        .execute()
        .await
        .expect("raw execution should not interpret the status");

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(response.bytes().await.unwrap().as_ref(), body);
    server.await.unwrap();
}

#[tokio::test]
async fn checked_execution_decodes_structured_postgrest_error() {
    let body = br#"{"code":"PGRST201","message":"ambiguous relationship","details":null,"hint":"disambiguate"}"#;
    let (url, server) =
        serve_once("300 Multiple Choices", "application/json", body, body.len()).await;

    let error = Postgrest::new(url)
        .from("items")
        .select("*")
        .execute_checked()
        .await
        .expect_err("non-success response should be an error");

    assert_eq!(error.status(), Some(StatusCode::MULTIPLE_CHOICES));
    assert_eq!(error.url().unwrap().path(), "/items");
    assert_eq!(error.url().unwrap().query(), Some("select=*"));
    let metadata = error.response_metadata().unwrap();
    assert_eq!(metadata.status(), StatusCode::MULTIPLE_CHOICES);
    assert_eq!(metadata.headers()["x-request-id"], "request-123");

    match error {
        ExecuteError::Postgrest { metadata, source } => {
            assert_eq!(metadata.url().path(), "/items");
            assert_eq!(source.status(), StatusCode::MULTIPLE_CHOICES);
            assert_eq!(source.code().as_ref(), "PGRST201");
            assert_eq!(
                source.kind(),
                ErrorKind::Postgrest(PostgrestErrorCode::AmbiguousEmbedding)
            );
            assert_eq!(source.response().message, "ambiguous relationship");
        }
        other => panic!("expected structured PostgREST error, got {other:?}"),
    }
    server.await.unwrap();
}

#[tokio::test]
async fn checked_execution_preserves_malformed_error_body() {
    let body = b"upstream failure: \xff";
    let (url, server) = serve_once("502 Bad Gateway", "text/plain", body, body.len()).await;

    let error = Postgrest::new(url)
        .from("items")
        .execute_checked()
        .await
        .expect_err("malformed non-success response should be an error");

    assert_eq!(error.status(), Some(StatusCode::BAD_GATEWAY));
    assert_eq!(error.url().unwrap().path(), "/items");
    assert_eq!(
        error.response_metadata().unwrap().headers()["x-request-id"],
        "request-123"
    );
    match error {
        ExecuteError::Decode { metadata, source } => {
            assert_eq!(metadata.status(), StatusCode::BAD_GATEWAY);
            assert_eq!(source.status(), StatusCode::BAD_GATEWAY);
            assert_eq!(source.body(), body);
        }
        other => panic!("expected decode error, got {other:?}"),
    }
    server.await.unwrap();
}

#[tokio::test]
async fn checked_execution_preserves_status_when_body_read_fails() {
    let body = b"short";
    let (url, server) = serve_once("503 Service Unavailable", "text/plain", body, 100).await;

    let error = Postgrest::new(url)
        .from("items")
        .execute_checked()
        .await
        .expect_err("truncated response body should be an error");

    assert_eq!(error.status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    match error {
        ExecuteError::ResponseBody { metadata, .. } => {
            assert_eq!(metadata.status(), StatusCode::SERVICE_UNAVAILABLE);
            assert_eq!(metadata.headers()["x-request-id"], "request-123");
            assert_eq!(metadata.url().path(), "/items");
        }
        other => panic!("expected response body error, got {other:?}"),
    }
    server.await.unwrap();
}

#[tokio::test]
async fn checked_execution_preserves_request_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (_connection, _) = listener.accept().await.unwrap();
        pending::<()>().await;
    });
    let client = postgrest::reqwest::Client::builder()
        .timeout(Duration::from_millis(100))
        .build()
        .unwrap();

    let error = Postgrest::new_with_client(format!("http://{address}"), client)
        .from("items")
        .execute_checked()
        .await
        .expect_err("silent server should time out");

    assert_eq!(error.status(), None);
    assert_eq!(error.url().unwrap().path(), "/items");
    assert!(error.response_metadata().is_none());
    match error {
        ExecuteError::Request(error) => assert!(error.is_timeout()),
        other => panic!("expected request error, got {other:?}"),
    }
    server.abort();
}
