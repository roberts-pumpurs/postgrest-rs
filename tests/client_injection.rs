use std::future::pending;
use std::time::Duration;

use postgrest::{reqwest, Postgrest};
use tokio::net::TcpListener;
use tokio::time::timeout;

enum QueryKind {
    From,
    Rpc,
}

async fn assert_supplied_client_timeout(query: QueryKind) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (accepted_sender, accepted_receiver) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (_connection, _) = listener.accept().await.unwrap();
        accepted_sender.send(()).unwrap();
        pending::<()>().await;
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(100))
        .build()
        .unwrap();
    let postgrest = Postgrest::new_with_client(format!("http://{address}"), client);
    let builder = match query {
        QueryKind::From => postgrest.from("items").select("*"),
        QueryKind::Rpc => postgrest.rpc("search", r#"{"term":"rust"}"#),
    };
    let request = tokio::spawn(builder.execute());

    let error = timeout(Duration::from_secs(5), async {
        accepted_receiver
            .await
            .expect("server stopped before accepting the connection");
        request.await.expect("request task panicked")
    })
    .await
    .expect("request exceeded the test deadline")
    .expect_err("silent server unexpectedly returned a response");

    assert!(error.is_timeout(), "expected a timeout error, got {error}");
    server.abort();
}

#[tokio::test]
async fn supplied_client_timeout_applies_to_from_queries() {
    assert_supplied_client_timeout(QueryKind::From).await;
}

#[tokio::test]
async fn supplied_client_timeout_applies_to_rpc_queries() {
    assert_supplied_client_timeout(QueryKind::Rpc).await;
}

#[test]
fn constructors_build_identical_from_requests() {
    let url = "https://example.test/rest/v1";
    let default_request = Postgrest::new(url)
        .schema("private")
        .insert_header("apikey", "secret")
        .from("items/")
        .eq("status", "active")
        .select("*")
        .build()
        .build()
        .unwrap();
    let injected_request = Postgrest::new_with_client(url, reqwest::Client::new())
        .schema("private")
        .insert_header("apikey", "secret")
        .from("items/")
        .eq("status", "active")
        .select("*")
        .build()
        .build()
        .unwrap();

    assert_eq!(default_request.method(), injected_request.method());
    assert_eq!(default_request.url(), injected_request.url());
    assert_eq!(default_request.headers(), injected_request.headers());
    assert_eq!(
        default_request.body().and_then(|body| body.as_bytes()),
        injected_request.body().and_then(|body| body.as_bytes())
    );

    assert_eq!(default_request.url().path(), "/rest/v1/items");
    assert_eq!(
        default_request
            .url()
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<Vec<_>>(),
        [
            ("status".into(), "eq.active".into()),
            ("select".into(), "*".into())
        ]
    );
    assert_eq!(default_request.headers()["accept"], "application/json");
    assert_eq!(default_request.headers()["accept-profile"], "private");
    assert_eq!(default_request.headers()["apikey"], "secret");
}

#[test]
fn constructors_build_identical_rpc_requests() {
    let url = "https://example.test/rest/v1";
    let params = r#"{"term":"rust"}"#;
    let default_request = Postgrest::new(url)
        .schema("private")
        .insert_header("apikey", "secret")
        .rpc("search/", params)
        .build()
        .build()
        .unwrap();
    let injected_request = Postgrest::new_with_client(url, reqwest::Client::new())
        .schema("private")
        .insert_header("apikey", "secret")
        .rpc("search/", params)
        .build()
        .build()
        .unwrap();

    assert_eq!(default_request.method(), injected_request.method());
    assert_eq!(default_request.url(), injected_request.url());
    assert_eq!(default_request.headers(), injected_request.headers());
    assert_eq!(
        default_request.body().and_then(|body| body.as_bytes()),
        injected_request.body().and_then(|body| body.as_bytes())
    );

    assert_eq!(default_request.url().path(), "/rest/v1/rpc/search");
    assert_eq!(default_request.headers()["accept"], "application/json");
    assert_eq!(default_request.headers()["content-profile"], "private");
    assert_eq!(
        default_request.headers()["content-type"],
        "application/json"
    );
    assert_eq!(default_request.headers()["apikey"], "secret");
    assert_eq!(
        default_request.body().and_then(|body| body.as_bytes()),
        Some(params.as_bytes())
    );
}
