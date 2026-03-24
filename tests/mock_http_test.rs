//! Testes de integração com mock de servidor HTTP.
//!
//! Execute com:
//!   cargo test -- --test-threads 1 --nocapture

use assert_cmd::prelude::*;
use mockito::{Matcher, Server};
use serial_test::serial;
use std::process::Command;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Fixtures JSON reutilizáveis
// ---------------------------------------------------------------------------

fn zoho_token_response() -> String {
    serde_json::json!({
        "access_token": "fake-access-token-abc123",
        "api_domain":   "http://127.0.0.1",
        "expires_in":   3600,
        "token_type":   "Bearer"
    })
    .to_string()
}

fn monitors_status_response() -> String {
    serde_json::json!({
        "code": 0,
        "data": [
            {
                "monitor_id":        "111000000001",
                "display_name":      "My Website",
                "type":              "URL",
                "status":            1,
                "unit":              "ms",
                "response_time":     120,
                "last_polled_time":  "2024-01-15T10:00:00+0000"
            },
            {
                "monitor_id":        "111000000002",
                "display_name":      "REST API Health",
                "type":              "RESTAPI",
                "status":            0,
                "unit":              "ms",
                "response_time":     0,
                "last_polled_time":  "2024-01-15T10:01:00+0000"
            },
            {
                "monitor_id":        "111000000003",
                "display_name":      "Browser Test",
                "type":              "REALBROWSER",
                "status":            2,
                "unit":              "ms",
                "response_time":     8500,
                "last_polled_time":  "2024-01-15T10:02:00+0000"
            }
        ]
    })
    .to_string()
}

fn monitor_groups_response() -> String {
    serde_json::json!({
        "code": 0,
        "data": [
            {
                "group_id":     "222000000001",
                "display_name": "Production",
                "monitors":     ["111000000001", "111000000002"]
            },
            {
                "group_id":     "222000000002",
                "display_name": "Staging",
                "monitors":     ["111000000003"]
            }
        ]
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn spawn_exporter(zoho_url: &str, api_url: &str, listen_port: u16) -> std::process::Child {
    Command::cargo_bin("site24x7_exporter")
        .unwrap()
        .env("ZOHO_CLIENT_ID",             "test-client-id")
        .env("ZOHO_CLIENT_SECRET",         "test-client-secret")
        .env("ZOHO_REFRESH_TOKEN",         "test-refresh-token")
        .env("ZOHO_BASE_URL_OVERRIDE",     zoho_url)
        .env("SITE24X7_API_BASE_OVERRIDE", api_url)
        .arg(format!("--web.listen-address=127.0.0.1:{}", listen_port))
        .spawn()
        .expect("falha ao iniciar o binário")
}

fn free_port() -> u16 {
    port_check::free_local_port().expect("nenhuma porta disponível")
}

fn wait_for_port(port: u16) {
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("exporter não ficou pronto na porta {}", port);
}

fn kill(mut child: std::process::Child) {
    child.kill().ok();
    child.wait().ok();
}

// ---------------------------------------------------------------------------
// TESTE 1: caminho feliz – métricas aparecem com valores corretos
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_metrics_happy_path() {
    let mut zoho_server = Server::new();
    let mut api_server  = Server::new();

    let _token_mock = zoho_server.mock("POST", "/oauth/v2/token")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(zoho_token_response())
        .create();

    let _status_mock = api_server.mock("GET", "/monitors/status")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(monitors_status_response())
        .create();

    let _groups_mock = api_server.mock("GET", "/monitor_groups")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(monitor_groups_response())
        .create();

    let port = free_port();
    let child = spawn_exporter(&zoho_server.url(), &api_server.url(), port);
    wait_for_port(port);

    let body = reqwest::blocking::get(format!("http://127.0.0.1:{}/metrics", port))
        .unwrap()
        .text()
        .unwrap();

    kill(child);

    assert!(body.contains("site24x7_monitor_status"),           "faltou site24x7_monitor_status");
    assert!(body.contains("site24x7_monitor_response_time_ms"), "faltou site24x7_monitor_response_time_ms");
    assert!(body.contains(r#"display_name="My Website""#),      "faltou label 'My Website'");
    assert!(body.contains(r#"display_name="REST API Health""#), "faltou label 'REST API Health'");
    assert!(
        body.contains("Production") || body.contains("Staging"),
        "faltaram labels de grupos de monitores"
    );
}

// ---------------------------------------------------------------------------
// TESTE 2: Zoho retorna 401 → sem métricas de dados, API não é chamada
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_zoho_auth_failure_returns_empty_metrics() {
    let mut zoho_server = Server::new();
    let mut api_server  = Server::new();

    let _token_mock = zoho_server.mock("POST", "/oauth/v2/token")
        .with_status(401)
        .with_header("Content-Type", "application/json")
        .with_body(serde_json::json!({
            "error": "invalid_client",
            "error_description": "Client ID does not exist"
        }).to_string())
        .create();

    // API não deve ser chamada — espera 0 hits
    let api_not_called = api_server.mock("GET", Matcher::Any)
        .with_status(500)
        .with_body("não deveria ser chamado")
        .expect(0)
        .create();

    let port = free_port();
    let child = spawn_exporter(&zoho_server.url(), &api_server.url(), port);
    wait_for_port(port);

    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/metrics", port)).unwrap();
    assert!(resp.status().is_success(), "/metrics deve responder mesmo com falha de auth");

    let body = resp.text().unwrap();
    assert!(
        !body.contains("site24x7_monitor_status{"),
        "não deveria ter métricas de monitores com auth falho"
    );

    kill(child);
    api_not_called.assert();
}

// ---------------------------------------------------------------------------
// TESTE 3: API retorna 500 → exporter não crasha
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_site24x7_api_server_error_does_not_crash_exporter() {
    let mut zoho_server = Server::new();
    let mut api_server  = Server::new();

    let _token_mock = zoho_server.mock("POST", "/oauth/v2/token")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(zoho_token_response())
        .create();

    let _status_mock = api_server.mock("GET", "/monitors/status")
        .with_status(500)
        .with_body("Internal Server Error")
        .create();

    let _groups_mock = api_server.mock("GET", "/monitor_groups")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(monitor_groups_response())
        .create();

    let port = free_port();
    let mut child = spawn_exporter(&zoho_server.url(), &api_server.url(), port);
    wait_for_port(port);

    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/metrics", port)).unwrap();
    assert!(resp.status().is_success());
    assert!(
        child.try_wait().unwrap().is_none(),
        "exporter não deve terminar após erro 500 da API"
    );

    kill(child);
}

// ---------------------------------------------------------------------------
// TESTE 4: JSON malformado → exporter sobrevive
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_malformed_json_does_not_crash_exporter() {
    let mut zoho_server = Server::new();
    let mut api_server  = Server::new();

    let _token_mock = zoho_server.mock("POST", "/oauth/v2/token")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(zoho_token_response())
        .create();

    let _status_mock = api_server.mock("GET", "/monitors/status")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(r#"{"code":0,"data":[{INVALID JSON}]}"#)
        .create();

    let _groups_mock = api_server.mock("GET", "/monitor_groups")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(monitor_groups_response())
        .create();

    let port = free_port();
    let mut child = spawn_exporter(&zoho_server.url(), &api_server.url(), port);
    wait_for_port(port);

    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/metrics", port)).unwrap();
    assert!(resp.status().is_success());
    assert!(
        child.try_wait().unwrap().is_none(),
        "exporter não deve terminar após JSON malformado"
    );

    kill(child);
}

// ---------------------------------------------------------------------------
// TESTE 5: endpoint /geolocation responde corretamente
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_geolocation_endpoint_responds() {
    let mut zoho_server = Server::new();
    let mut api_server  = Server::new();

    let _token_mock = zoho_server.mock("POST", "/oauth/v2/token")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(zoho_token_response())
        .create();

    let _status_mock = api_server.mock("GET", "/monitors/status")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(monitors_status_response())
        .create();

    let _groups_mock = api_server.mock("GET", "/monitor_groups")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(monitor_groups_response())
        .create();

    let port = free_port();
    let child = spawn_exporter(&zoho_server.url(), &api_server.url(), port);
    wait_for_port(port);

    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/geolocation", port)).unwrap();
    assert!(resp.status().is_success(), "/geolocation deve retornar 200");
    assert!(!resp.text().unwrap().is_empty(), "/geolocation não deve ser vazio");

    kill(child);
}

// ---------------------------------------------------------------------------
// TESTE 6: Bearer token correto é enviado para a API
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_bearer_token_is_sent_to_api() {
    let mut zoho_server = Server::new();
    let mut api_server  = Server::new();

    let _token_mock = zoho_server.mock("POST", "/oauth/v2/token")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(zoho_token_response())
        .create();

    // Exige o header Authorization exato
    let status_mock = api_server.mock("GET", "/monitors/status")
        .match_header("Authorization", "Zoho-oauthtoken fake-access-token-abc123")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(monitors_status_response())
        .create();

    let _groups_mock = api_server.mock("GET", "/monitor_groups")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(monitor_groups_response())
        .create();

    let port = free_port();
    let child = spawn_exporter(&zoho_server.url(), &api_server.url(), port);
    wait_for_port(port);

    reqwest::blocking::get(format!("http://127.0.0.1:{}/metrics", port)).unwrap();

    kill(child);
    status_mock.assert(); // falha se o mock não recebeu a chamada com o header correto
}

// ---------------------------------------------------------------------------
// TESTE 7: monitor com status TROUBLE (2) aparece nas métricas
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_monitor_trouble_status_is_exported() {
    let mut zoho_server = Server::new();
    let mut api_server  = Server::new();

    let _token_mock = zoho_server.mock("POST", "/oauth/v2/token")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(zoho_token_response())
        .create();

    let _status_mock = api_server.mock("GET", "/monitors/status")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(monitors_status_response())
        .create();

    let _groups_mock = api_server.mock("GET", "/monitor_groups")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(monitor_groups_response())
        .create();

    let port = free_port();
    let child = spawn_exporter(&zoho_server.url(), &api_server.url(), port);
    wait_for_port(port);

    let body = reqwest::blocking::get(format!("http://127.0.0.1:{}/metrics", port))
        .unwrap()
        .text()
        .unwrap();

    kill(child);

    assert!(
        body.contains(r#"display_name="Browser Test""#),
        "monitor com status TROUBLE deve aparecer nas métricas"
    );
}

// ---------------------------------------------------------------------------
// TESTE 8: rota desconhecida retorna 404
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_unknown_path_returns_404() {
    let mut zoho_server = Server::new();
    let mut api_server  = Server::new();

    let _token_mock = zoho_server.mock("POST", "/oauth/v2/token")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(zoho_token_response())
        .create();

    let _status_mock = api_server.mock("GET", "/monitors/status")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(monitors_status_response())
        .create();

    let _groups_mock = api_server.mock("GET", "/monitor_groups")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(monitor_groups_response())
        .create();

    let port = free_port();
    let child = spawn_exporter(&zoho_server.url(), &api_server.url(), port);
    wait_for_port(port);

    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/nao-existe", port)).unwrap();
    assert_eq!(resp.status(), 404);

    kill(child);
}
