#!/usr/bin/env bash
# Container smoke check for the host image: TLS on a non-loopback listener, state and
# workspace on bind mounts owned by the image's unprivileged user (uid 10001), and agent
# execution with the repository's mock agent (built into the `smoke` image stage).
# Used by the `container` job in .github/workflows/rust.yml; needs docker, openssl, sudo
# and a Python with `websockets`. The pairing code and token are never printed.
#   PYTHON=python3 bash scripts/container-smoke.sh
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
python="${PYTHON:-python3}"
image="${ACPD_SMOKE_IMAGE:-acpd-smoke}"
work="$(mktemp -d)"
name="acpd-smoke-$$"
cleanup() { docker rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT

# Test CA and a host certificate for localhost (verification stays on in the client).
cd "$work"
mkdir -p config state workspace
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -days 2 \
  -subj /CN=acpd-smoke-ca -keyout ca.key -out ca.pem \
  -addext basicConstraints=critical,CA:TRUE -addext keyUsage=critical,keyCertSign,cRLSign 2>/dev/null
openssl req -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -subj /CN=localhost \
  -keyout config/host.key -out host.csr 2>/dev/null
printf 'subjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth\nauthorityKeyIdentifier=keyid,issuer\nsubjectKeyIdentifier=hash\n' > host.ext
openssl x509 -req -in host.csr -CA ca.pem -CAkey ca.key -CAcreateserial -days 2 -extfile host.ext -out config/host.pem 2>/dev/null
cat > config/config.toml <<'TOML'
registry = "agents.json"
workspace_roots = ["/workspace"]
state_directory = "/state"

[server]
listen = "0.0.0.0:8443"
tls_certificate = "host.pem"
tls_private_key = "host.key"
TOML
cat > config/agents.json <<'JSON'
[{"id":"mock","name":"Mock ACP","command":"/usr/local/bin/mock-acp-agent","args":["--terminal-during-prompt"],"enabled":true}]
JSON
sudo chown -R 10001:10001 config state workspace
sudo chmod 0700 state
sudo chmod 0400 config/host.key

run() {
  docker run -d --name "$name" -p 127.0.0.1:8443:8443 \
    -v "$work/config:/config:ro" -v "$work/state:/state" -v "$work/workspace:/workspace" \
    "$image" --config /config/config.toml start >/dev/null
  for _ in $(seq 1 50); do
    if curl -s --cacert ca.pem -o /dev/null https://localhost:8443/v1/status; then return 0; fi
    sleep 0.2
  done
  docker logs "$name"; echo "host did not start"; exit 1
}
run
test "$(docker exec "$name" id -u)" = 10001
# No plain-HTTP fallback on the TLS listener.
if curl -s -o /dev/null --max-time 5 http://localhost:8443/v1/status; then echo "plain HTTP answered"; exit 1; fi
echo "ok: plain HTTP refused"
ACPD_PAIR_CODE="$(docker exec "$name" acpd --config /config/config.toml pair | sed -n 's/^Pairing code: //p')" \
  "$python" "$here/scripts/container-smoke.py" first --ca ca.pem --token-file "$work/token" --session-file "$work/session"
docker restart "$name" >/dev/null
for _ in $(seq 1 50); do curl -s --cacert ca.pem -o /dev/null https://localhost:8443/v1/status && break; sleep 0.2; done
"$python" "$here/scripts/container-smoke.py" second --ca ca.pem --token-file "$work/token" --session-file "$work/session"
# Host state lives on the mount, owned by the container user, not world-readable.
sudo find state -maxdepth 2 -printf '%u:%g %m %p\n'
test -z "$(sudo find state -perm /o=rwx -print -quit)"
test -z "$(sudo find state ! -uid 10001 -print -quit)"
echo "ok: state on the mount is owned by uid 10001 with no access for others"
docker logs "$name" 2>&1 | tail -n 5
