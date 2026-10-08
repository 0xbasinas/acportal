# Install and connect a host

This procedure describes the implemented host and debug Android client. Physical-phone TLS acceptance and signed APK delivery are suspended and remain unverified. Supported-platform builds, including Rust 1.88, belong in GitHub Actions. Normal development builds may run locally.

## Install the host

Build the pinned source with `cargo build --locked --release -p acpd --bin acpd`. Copy `target/release/acpd.exe` on Windows, or `target/release/acpd` on Unix, to an operator-owned directory. Install Goose separately and configure its provider through Goose. Run the host under the account that owns that configuration. A service account has its own PATH and provider configuration.

Copy `examples/config.toml` and `examples/agents.json` into a private configuration directory. Set `workspace_roots` to only the directories you intend to authorize. An empty list denies all workspaces. Set `state_directory` to durable private storage outside the checkout. Registry, workspace, state and TLS paths in TOML resolve relative to the config file. Registry executable paths must be absolute or executable names resolved through absolute PATH directories; relative paths with separators do not resolve.

Keep the state directory across restarts to retain device credentials and interrupted-session metadata. Restrict access to the operator account. Do not copy state, provider credentials or private keys into the repository or Android package. Agent processes and delivery journals are in memory; restarting the daemon interrupts execution and requires explicit session loading.

From the directory containing the installed binary, check configuration and discovery before starting:

```powershell
./acpd.exe --config C:/ACPPortal/config.toml config
./acpd.exe --config C:/ACPPortal/config.toml agents
./acpd.exe --config C:/ACPPortal/config.toml doctor
./acpd.exe --config C:/ACPPortal/config.toml probe --agent goose --workspace C:/Work/project
./acpd.exe --config C:/ACPPortal/config.toml start
```

On Unix, substitute `./acpd` and your absolute paths. `doctor` probes the listener and storage and checks executable/workspace availability. Run it before `start`; an occupied listener can be the existing daemon. It does not validate certificate trust, provider credentials or free space. `probe` starts the selected executable for ACP negotiation without a model prompt. Use an explicit no-tools `chat` prompt separately when you need to test provider access.

## Configure trusted TLS

Use a DNS hostname reachable by the phone and a certificate trusted by Android's system trust store. The certificate must cover the exact hostname entered in the app. Supply a PEM certificate chain and matching PEM private key. Obtain and renew these through your certificate issuer; this host does not automate issuance or renewal. A self-signed certificate or a user-installed CA is not established as supported by the app's system-only trust policy.

For example, adapt this configuration to your own existing directories and certificate files:

```toml
registry = "agents.json"
workspace_roots = ["C:/Work/project"]
state_directory = "C:/ACPPortal/state"

[server]
listen = "0.0.0.0:8765"
tls_certificate = "certs/fullchain.pem"
tls_private_key = "certs/privkey.pem"
token_lifetime_seconds = 2592000
```

Both TLS paths are required together. Every non-loopback listener requires TLS. The host loads the PEM files at startup; restart it after replacing certificates, accounting for interrupted sessions. Allow the selected TCP port only from intended client networks in the host firewall. The app uses HTTPS REST and WSS on that same listener. Enter `https://your-hostname:8765` as the host address, without an API path. Connecting by IP requires a certificate valid for that IP.

If TLS fails, check the hostname, certificate validity dates, complete issuer chain, matching key, DNS and firewall. Keep certificate validation enabled. A successful TCP connection or local `probe` does not prove phone certificate trust.

## Install and pair the debug client

Build from `android/universal-acp` with JDK 21 on the current Windows development machine:

```powershell
$env:JAVA_HOME='C:/Program Files/Amazon Corretto/jdk21.0.11_10'
./gradlew.bat :app:assembleDebug
```

Install `app/build/outputs/apk/debug/app-debug.apk` using `adb install -r` to preserve existing application data. Signed release production is outside the current scope. Release network policy accepts HTTPS only; debug HTTP exceptions are limited to configured local development addresses.

Start the host, then run `acpd --config /your/config.toml pair` locally under the same operator account and with the same state directory. In Android, choose Add host, enter the address and redeem the code within 60 seconds. The code is one-use. Keep it out of screenshots, shared terminal transcripts and logs. Expired or rejected codes require an explicit new code and retry. The app stores the resulting credential in its encrypted no-backup vault.

For the emulator only, the repository's loopback development config uses `http://10.0.2.2:8765`. The independent mock host uses port 8767. These debug addresses do not establish remote TLS acceptance.

Use `devices` to inspect paired device names/expiration and `revoke DEVICE-UUID` to revoke a selected device with the same config. Revocation affects open sockets at the next heartbeat. Renew pairing explicitly through the app when credentials expire or are revoked. Never send credentials as command-line arguments or include them in reports.

See [agent configuration](agents.md), [security boundaries](security.md) and [development procedures](development.md). Workspace callback policies require reviewed writes and terminal creation; they do not sandbox agent-owned tools or MCP processes.
