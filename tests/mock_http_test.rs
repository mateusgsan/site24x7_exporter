//! Testes de integração com mock de servidor HTTP.
//!
//! Execute com:
//!   cargo test -- --test-threads 1 --nocapture

use assert_cmd::prelude::*;
use mockito::{Matcher, Method::*, Server, ServerGuard};
use serial_test::serial;
use std::process::Command;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Fixtures JSON reutilizáveis
// ---------------------------------------------------------------------------

/// Resposta do Zoho OAuth – token de acesso falso
fn zoho_token_response() -> serde_json::Value {
    serde_json::json!({
        "access_token": "fake-access-token-abc123",
        "api_domain":   "http://127.0.0.1",   // será sobrescrito pelo override
        "expires_in":   3600,
        "token_type":   "Bearer"
    })
}

/// Resposta da API /monitors/status com 3 monitores de tipos diferentes
fn monitors_status_response() -> serde_json::Value {
    serde_json::json!({
        "code": 0,
        "data": [
            {
                "monitor_id":   "111000000001",
                "display_name": "My Website",
                "type":         "URL",
                "status":       1,          // UP
                "unit":         "ms",
                "response_time": 120,
                "last_polled_time": "2024-01-15T10:00:00+0000"
            },
            {
                "monitor_id":   "111000000002",
                "display_name": "REST API Health",
                "type":         "RESTAPI",
                "status":       0,          // DOWN
                "unit":         "ms",
                "response_time": 0,
                "last_polled_time": "2024-01-15T10:01:00+0000"
            },
            {
                "monitor_id":   "111000000003",
                "display_name": "Browser Test",
                "type":         "REALBROWSER",
                "status":       2,          // TROUBLE
                "unit":         "ms",
                "response_time": 8500,
                "last_polled_time": "2024-01-15T10:02:00+0000"
            }
        ]
    })
}

/// Resposta da API /monitor_groups com 2 grupos
fn monitor_groups_response() -> serde_json::Value {
    serde_json::json!({
        "code": 0,
        "data": [
            {
                "group_id":      "222000000001",
                "display_name":  "Production",
                "monitors":      ["111000000001", "111000000002"]
            },
            {
                "group_id":      "222000000002",
                "display_name":  "Staging",
                "monitors":      ["111000000003"]
            }
        ]
    })
}

// ---------------------------------------------------------------------------
// Helper: inicia o binário apontado para os mock servers
// ---------------------------------------------------------------------------

fn spawn_exporter_with_mocks(
    zoho_server: &ServerGuard,
    api_server: &ServerGuard,
    listen_port: u16,
) -> std::process::Child {
    Command::cargo_bin("site24x7_exporter")
        .unwrap()
        .env("ZOHO_CLIENT_ID",             "test-client-id")
        .env("ZOHO_CLIENT_SECRET",         "test-client-secret")
        .env("ZOHO_REFRESH_TOKEN",         "test-refresh-token")
        .env("ZOHO_BASE_URL_OVERRIDE",     zoho_server.url())
        .env("SITE24X7_API_BASE_OVERRIDE", api_server.url())
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

// ---------------------------------------------------------------------------
// TESTE 1: caminho feliz – métricas aparecem com valores corretos
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_metrics_happy_path() {
    // Sobe dois servidores mock locais
    let mut zoho_server = Server::new();
    let mut api_server  = Server::new();

    // Mock: Zoho token endpoint
    let _token_mock = zoho_server.mock("POST", "/oauth/v2/token")
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(zoho_token_response().to_string())
        .create();

    // Mock: lista de monitores
    let _status_mock = api_server.mock(|when, then| {
        when.method(GET)
            .path("/monitors/status")
            .header("Authorization", Matcher::Any);      // deve enviar o Bearer token
        then.status(200)
            .header("Content-Type", "application/json")
            .body(monitors_status_response().to_string());
    });

    // Mock: grupos de monitores
    let _groups_mock = api_server.mock(|when, then| {
        when.method(GET).path("/monitor_groups");
        then.status(200)
            .header("Content-Type", "application/json")
            .body(monitor_groups_response().to_string());
    });

    let port = free_port();
    let mut child = spawn_exporter_with_mocks(&zoho_server, &api_server, port);
    wait_for_port(port);

    // Faz scrape de /metrics
    let body = reqwest::blocking::get(format!("http://127.0.0.1:{}/metrics", port))
        .unwrap()
        .text()
        .unwrap();

    child.kill().ok();

    // Verifica presença de métricas Prometheus esperadas
    assert!(body.contains("site24x7_monitor_status"),          "faltou site24x7_monitor_status");
    assert!(body.contains("site24x7_monitor_response_time_ms"), "faltou site24x7_monitor_response_time_ms");

    // Monitor UP (status=1)
    assert!(
        body.contains(r#"display_name="My Website""#),
        "faltou label do monitor 'My Website'"
    );

    // Monitor DOWN (status=0)
    assert!(
        body.contains(r#"display_name="REST API Health""#),
        "faltou label do monitor 'REST API Health'"
    );

    // Grupos (tags) devem aparecer como labels
    assert!(body.contains("Production") || body.contains("Staging"),
        "faltaram labels de grupos de monitores");

    child.wait().ok();
}

// ---------------------------------------------------------------------------
// TESTE 2: Zoho retorna erro 401 → exporter responde mas sem métricas de dados
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_zoho_auth_failure_returns_empty_metrics() {
    let mut zoho_server = Server::new();
    let mut api_server  = Server::new();

    // Token endpoint retorna 401
    let _token_mock = zoho_server.mock(|when, then| {
        when.method(POST).path("/oauth/v2/token");
        then.status(401)
            .header("Content-Type", "application/json")
            .body(serde_json::json!({
                "error": "invalid_client",
                "error_description": "Client ID does not exist"
            }).to_string());
    });

    // API não deve ser chamada neste cenário
    let api_not_called = api_server.mock(|when, then| {
        when.any_request();
        then.status(500).body("não deveria ser chamado");
    });

    let port = free_port();
    let mut child = spawn_exporter_with_mocks(&zoho_server, &api_server, port);
    wait_for_port(port);

    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/metrics", port)).unwrap();
    assert!(resp.status().is_success(), "endpoint /metrics deve responder mesmo com falha de auth");

    let body = resp.text().unwrap();

    // Não deve conter métricas de dados quando autenticação falha
    assert!(
        !body.contains("site24x7_monitor_status{"),
        "não deveria ter métricas de monitores com auth falho"
    );

    child.kill().ok();

    // A API de monitores não deve ter sido chamada
    api_not_called.assert_hits(0);

    child.wait().ok();
}

// ---------------------------------------------------------------------------
// TESTE 3: API de monitores retorna 500 → exporter não panifica
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_site24x7_api_server_error_does_not_crash_exporter() {
    let mut zoho_server = Server::new();
    let mut api_server  = Server::new();

    let _token_mock = zoho_server.mock(|when, then| {
        when.method(POST).path("/oauth/v2/token");
        then.status(200).body(zoho_token_response().to_string());
    });

    // Monitores retornam 500
    let _status_mock = api_server.mock(|when, then| {
        when.method(GET).path("/monitors/status");
        then.status(500).body("Internal Server Error");
    });

    let _groups_mock = api_server.mock(|when, then| {
        when.method(GET).path("/monitor_groups");
        then.status(200).body(monitor_groups_response().to_string());
    });

    let port = free_port();
    let mut child = spawn_exporter_with_mocks(&zoho_server, &api_server, port);
    wait_for_port(port);

    // Deve retornar 200 sem panic (exporter robusto a falhas de upstream)
    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/metrics", port)).unwrap();
    assert!(resp.status().is_success());

    // Processo ainda deve estar vivo
    assert!(
        child.try_wait().unwrap().is_none(),
        "o exporter não deve ter terminado após erro 500 da API"
    );

    child.wait().ok();
}

// ---------------------------------------------------------------------------
// TESTE 4: resposta da API com JSON malformado → exporter sobrevive
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_malformed_json_does_not_crash_exporter() {
    let mut zoho_server = Server::new();
    let mut api_server  = Server::new();

    let _token_mock = zoho_server.mock(|when, then| {
        when.method(POST).path("/oauth/v2/token");
        then.status(200).body(zoho_token_response().to_string());
    });

    let _status_mock = api_server.mock(|when, then| {
        when.method(GET).path("/monitors/status");
        then.status(200)
            .header("Content-Type", "application/json")
            .body(r#"{"code":0,"data":[{INVALID JSON}]}"#);
    });

    let _groups_mock = api_server.mock(|when, then| {
        when.method(GET).path("/monitor_groups");
        then.status(200).body(monitor_groups_response().to_string());
    });

    let port = free_port();
    let mut child = spawn_exporter_with_mocks(&zoho_server, &api_server, port);
    wait_for_port(port);

    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/metrics", port)).unwrap();
    assert!(resp.status().is_success());

    assert!(
        child.try_wait().unwrap().is_none(),
        "o exporter não deve ter terminado após JSON malformado"
    );

    child.wait().ok();
}

// ---------------------------------------------------------------------------
// TESTE 5: endpoint /geolocation responde corretamente
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_geolocation_endpoint_responds() {
    let mut zoho_server = Server::new();
    let mut api_server  = Server::new();

    let _token_mock = zoho_server.mock(|when, then| {
        when.method(POST).path("/oauth/v2/token");
        then.status(200).body(zoho_token_response().to_string());
    });

    let _status_mock = api_server.mock(|when, then| {
        when.method(GET).path("/monitors/status");
        then.status(200).body(monitors_status_response().to_string());
    });

    let _groups_mock = api_server.mock(|when, then| {
        when.method(GET).path("/monitor_groups");
        then.status(200).body(monitor_groups_response().to_string());
    });

    let port = free_port();
    let mut child = spawn_exporter_with_mocks(&zoho_server, &api_server, port);
    wait_for_port(port);

    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/geolocation", port)).unwrap();
    assert!(resp.status().is_success(), "/geolocation deve retornar 200");

    let body = resp.text().unwrap();
    // O corpo deve conter pelo menos chaves de geolocalização conhecidas ou ser JSON válido
    assert!(!body.is_empty(), "/geolocation não deve retornar vazio");

    child.wait().ok();
}

// ---------------------------------------------------------------------------
// TESTE 6: token de acesso é enviado no header Authorization
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_bearer_token_is_sent_to_api() {
    let mut zoho_server = Server::new();
    let mut api_server  = Server::new();

    let _token_mock = zoho_server.mock(|when, then| {
        when.method(POST).path("/oauth/v2/token");
        then.status(200).body(zoho_token_response().to_string());
    });

    // Exige header Authorization com o token específico
    let status_mock = api_server.mock(|when, then| {
        when.method(GET)
            .path("/monitors/status")
            .header("Authorization", "Zoho-oauthtoken fake-access-token-abc123");
        then.status(200).body(monitors_status_response().to_string());
    });

    let _groups_mock = api_server.mock(|when, then| {
        when.method(GET).path("/monitor_groups");
        then.status(200).body(monitor_groups_response().to_string());
    });

    let port = free_port();
    let mut child = spawn_exporter_with_mocks(&zoho_server, &api_server, port);
    wait_for_port(port);

    reqwest::blocking::get(format!("http://127.0.0.1:{}/metrics", port))
        .unwrap();

    child.wait().ok();

    // Verifica que o mock recebeu a chamada com o header correto
    status_mock.assert();
}

// ---------------------------------------------------------------------------
// TESTE 7: monitor com status TROUBLE (2) aparece nas métricas
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_monitor_trouble_status_is_exported() {
    let mut zoho_server = Server::new();
    let mut api_server  = Server::new();

    let _token_mock = zoho_server.mock(|when, then| {
        when.method(POST).path("/oauth/v2/token");
        then.status(200).body(zoho_token_response().to_string());
    });

    let _status_mock = api_server.mock(|when, then| {
        when.method(GET).path("/monitors/status");
        then.status(200).body(monitors_status_response().to_string());
    });

    let _groups_mock = api_server.mock(|when, then| {
        when.method(GET).path("/monitor_groups");
        then.status(200).body(monitor_groups_response().to_string());
    });

    let port = free_port();
    let mut child = spawn_exporter_with_mocks(&zoho_server, &api_server, port);
    wait_for_port(port);

    let body = reqwest::blocking::get(format!("http://127.0.0.1:{}/metrics", port))
        .unwrap()
        .text()
        .unwrap();

    child.wait().ok();

    // "Browser Test" está com status=2 (TROUBLE), response_time=8500
    assert!(
        body.contains(r#"display_name="Browser Test""#),
        "monitor TROUBLE deve aparecer nas métricas"
    );
}

// ---------------------------------------------------------------------------
// TESTE 8: exporter responde 404 para paths desconhecidos
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn test_unknown_path_returns_404() {
    let mut zoho_server = Server::new();
    let mut api_server  = Server::new();

    let _token_mock = zoho_server.mock(|when, then| {
        when.method(POST).path("/oauth/v2/token");
        then.status(200).body(zoho_token_response().to_string());
    });

    let _status_mock = api_server.mock(|when, then| {
        when.method(GET).path("/monitors/status");
        then.status(200).body(monitors_status_response().to_string());
    });

    let _groups_mock = api_server.mock(|when, then| {
        when.method(GET).path("/monitor_groups");
        then.status(200).body(monitor_groups_response().to_string());
    });

    let port = free_port();
    let mut child = spawn_exporter_with_mocks(&zoho_server, &api_server, port);
    wait_for_port(port);

    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/nao-existe", port)).unwrap();
    assert_eq!(resp.status(), 404);

    child.wait().ok();
}