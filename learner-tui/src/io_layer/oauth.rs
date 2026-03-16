use oauth2::{
    AuthUrl, TokenUrl, ClientId, ClientSecret, RedirectUrl, Scope,
    AuthorizationCode, CsrfToken, PkceCodeChallenge,
    basic::BasicClient, reqwest::http_client, TokenResponse,
};
use std::collections::HashMap;
use std::net::TcpListener;
use std::io::{Read, Write};

#[derive(Debug, Clone, PartialEq)]
pub enum OAuthProvider {
    Google,
    Microsoft,
    Dropbox,
}

#[derive(Debug, Clone)]
pub enum OAuthStatus {
    Idle,
    WaitingForBrowser,
    WaitingForDevice { url: String, code: String },
    Success(HashMap<String, String>),
    Error(String),
}

/// Configuration for each provider
pub struct ProviderConfig {
    pub auth_url: &'static str,
    pub token_url: &'static str,
    pub scopes: Vec<&'static str>,
    pub device_auth_url: Option<&'static str>,
}

impl OAuthProvider {
    pub fn config(&self) -> ProviderConfig {
        match self {
            OAuthProvider::Google => ProviderConfig {
                auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
                token_url: "https://oauth2.googleapis.com/token",
                scopes: vec![
                    "https://www.googleapis.com/auth/gmail.readonly",
                    "https://www.googleapis.com/auth/drive.readonly",
                ],
                device_auth_url: None,
            },
            OAuthProvider::Microsoft => ProviderConfig {
                auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
                token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
                scopes: vec!["Mail.Read", "Files.ReadWrite", "offline_access"],
                device_auth_url: Some(
                    "https://login.microsoftonline.com/common/oauth2/v2.0/devicecode",
                ),
            },
            OAuthProvider::Dropbox => ProviderConfig {
                auth_url: "https://www.dropbox.com/oauth2/authorize",
                token_url: "https://api.dropboxapi.com/oauth2/token",
                scopes: vec![],
                device_auth_url: None,
            },
        }
    }

    pub fn env_prefix(&self) -> &'static str {
        match self {
            OAuthProvider::Google => "GOOGLE",
            OAuthProvider::Microsoft => "MICROSOFT",
            OAuthProvider::Dropbox => "DROPBOX",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            OAuthProvider::Google => "Google",
            OAuthProvider::Microsoft => "Microsoft",
            OAuthProvider::Dropbox => "Dropbox",
        }
    }
}

/// Run localhost redirect OAuth flow (for Google and Dropbox).
///
/// 1. Find a free port on localhost
/// 2. Build auth URL with redirect to localhost:port
/// 3. Open browser for user to authorize
/// 4. Listen for the callback with authorization code
/// 5. Exchange code for tokens via the token endpoint
///
/// Returns HashMap of env keys to save (e.g. GOOGLE_ACCESS_TOKEN).
pub fn localhost_redirect_flow(
    provider: &OAuthProvider,
    client_id: &str,
    client_secret: Option<&str>,
) -> Result<HashMap<String, String>, String> {
    let config = provider.config();

    // Find a free port
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|e| format!("Failed to bind: {}", e))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Failed to get addr: {}", e))?
        .port();
    let redirect_url = format!("http://127.0.0.1:{}", port);

    // Build OAuth2 client using the correct v4.4 API:
    // BasicClient::new(ClientId, Option<ClientSecret>, AuthUrl, Option<TokenUrl>)
    let auth_url =
        AuthUrl::new(config.auth_url.to_string()).map_err(|e| format!("Bad auth URL: {}", e))?;
    let token_url =
        TokenUrl::new(config.token_url.to_string()).map_err(|e| format!("Bad token URL: {}", e))?;
    let redirect =
        RedirectUrl::new(redirect_url).map_err(|e| format!("Bad redirect URL: {}", e))?;

    let secret = client_secret.map(|s| ClientSecret::new(s.to_string()));

    let client = BasicClient::new(
        ClientId::new(client_id.to_string()),
        secret,
        auth_url,
        Some(token_url),
    )
    .set_redirect_uri(redirect);

    // Generate PKCE challenge
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    // Build authorization URL
    let mut auth_request = client
        .authorize_url(CsrfToken::new_random)
        .set_pkce_challenge(pkce_challenge);

    for scope in &config.scopes {
        auth_request = auth_request.add_scope(Scope::new(scope.to_string()));
    }

    let (auth_url, _csrf_token) = auth_request.url();

    // Open browser
    open::that(auth_url.to_string())
        .map_err(|e| format!("Failed to open browser: {}", e))?;

    // Wait for callback on our local listener
    // The browser will redirect to http://127.0.0.1:{port}/?code=...&state=...
    let (mut stream, _) = listener
        .accept()
        .map_err(|e| format!("Failed to accept connection: {}", e))?;

    // Read the HTTP request
    let mut buf = [0u8; 4096];
    let n = stream
        .read(&mut buf)
        .map_err(|e| format!("Read error: {}", e))?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Extract authorization code from query string
    let code = extract_code_from_request(&request)
        .ok_or_else(|| "No authorization code in callback".to_string())?;

    // Send success response to browser
    let response = concat!(
        "HTTP/1.1 200 OK\r\n",
        "Content-Type: text/html\r\n\r\n",
        "<html><body>",
        "<h2>Authorization successful!</h2>",
        "<p>You can close this tab and return to Solvable.</p>",
        "</body></html>"
    );
    let _ = stream.write_all(response.as_bytes());
    drop(stream);
    drop(listener);

    // Exchange code for tokens using the oauth2 crate's sync API
    let token_result = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(pkce_verifier)
        .request(http_client)
        .map_err(|e| format!("Token exchange failed: {}", e))?;

    let prefix = provider.env_prefix();
    let mut tokens = HashMap::new();
    tokens.insert(
        format!("{}_ACCESS_TOKEN", prefix),
        token_result.access_token().secret().to_string(),
    );
    if let Some(refresh) = token_result.refresh_token() {
        tokens.insert(
            format!("{}_REFRESH_TOKEN", prefix),
            refresh.secret().to_string(),
        );
    }
    tokens.insert(format!("{}_CLIENT_ID", prefix), client_id.to_string());
    if let Some(secret) = client_secret {
        tokens.insert(format!("{}_CLIENT_SECRET", prefix), secret.to_string());
    }

    Ok(tokens)
}

/// Extract authorization code from HTTP GET request query string.
/// Parses a line like: GET /?code=abc123&state=xyz HTTP/1.1
fn extract_code_from_request(request: &str) -> Option<String> {
    let first_line = request.lines().next()?;
    let path = first_line.split_whitespace().nth(1)?;
    let query = path.split('?').nth(1)?;
    for param in query.split('&') {
        let mut kv = param.splitn(2, '=');
        if let (Some(key), Some(val)) = (kv.next(), kv.next()) {
            if key == "code" {
                // URL-decode the value
                return Some(
                    url::form_urlencoded::parse(val.as_bytes())
                        .next()
                        .map(|(_, v)| v.to_string())
                        .unwrap_or_else(|| val.to_string()),
                );
            }
        }
    }
    None
}

/// State for an in-progress Microsoft device code flow.
/// Created by `start_device_flow`, then polled via `poll_for_token`.
pub struct DeviceFlowState {
    pub user_code: String,
    pub verification_uri: String,
    pub device_code: String,
    pub interval: u64,
    pub client_id: String,
    pub token_url: String,
}

/// Start a device code flow for Microsoft.
///
/// Returns a DeviceFlowState containing the user code and verification URI
/// to display in the TUI. The caller should then poll `poll_for_token`
/// every `interval` seconds.
pub fn start_device_flow(client_id: &str) -> Result<DeviceFlowState, String> {
    let config = OAuthProvider::Microsoft.config();
    let device_url = config
        .device_auth_url
        .ok_or("Provider doesn't support device flow")?;

    let params = [
        ("client_id", client_id),
        ("scope", &config.scopes.join(" ")),
    ];

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(device_url)
        .form(&params)
        .send()
        .map_err(|e| format!("Device code request failed: {}", e))?;

    let body: serde_json::Value = resp
        .json()
        .map_err(|e| format!("Failed to parse device response: {}", e))?;

    let user_code = body["user_code"]
        .as_str()
        .ok_or("Missing user_code in device response")?
        .to_string();
    let verification_uri = body["verification_uri"]
        .as_str()
        .ok_or("Missing verification_uri in device response")?
        .to_string();
    let device_code = body["device_code"]
        .as_str()
        .ok_or("Missing device_code in device response")?
        .to_string();
    let interval = body["interval"].as_u64().unwrap_or(5);

    Ok(DeviceFlowState {
        user_code,
        verification_uri,
        device_code,
        interval,
        client_id: client_id.to_string(),
        token_url: config.token_url.to_string(),
    })
}

impl DeviceFlowState {
    /// Poll the token endpoint for the device code flow.
    ///
    /// Returns:
    /// - `Ok(Some(tokens))` when authorization is complete
    /// - `Ok(None)` when still waiting (authorization_pending / slow_down)
    /// - `Err(msg)` on failure (expired, denied, etc.)
    pub fn poll_for_token(&self) -> Result<Option<HashMap<String, String>>, String> {
        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(&self.token_url)
            .form(&[
                (
                    "grant_type",
                    "urn:ietf:params:oauth:grant-type:device_code",
                ),
                ("client_id", &self.client_id),
                ("device_code", &self.device_code),
            ])
            .send()
            .map_err(|e| format!("Poll failed: {}", e))?;

        let body: serde_json::Value = resp
            .json()
            .map_err(|e| format!("Parse error: {}", e))?;

        if let Some(error) = body["error"].as_str() {
            match error {
                "authorization_pending" => return Ok(None),
                "slow_down" => return Ok(None),
                "expired_token" => return Err("Device code expired. Try again.".to_string()),
                "access_denied" => return Err("Authorization denied by user.".to_string()),
                _ => return Err(format!("OAuth error: {}", error)),
            }
        }

        let access_token = body["access_token"]
            .as_str()
            .ok_or("Missing access_token in response")?;
        let refresh_token = body["refresh_token"].as_str();

        let mut tokens = HashMap::new();
        tokens.insert(
            "MICROSOFT_ACCESS_TOKEN".to_string(),
            access_token.to_string(),
        );
        if let Some(rt) = refresh_token {
            tokens.insert("MICROSOFT_REFRESH_TOKEN".to_string(), rt.to_string());
        }
        tokens.insert("MICROSOFT_CLIENT_ID".to_string(), self.client_id.clone());

        Ok(Some(tokens))
    }
}
