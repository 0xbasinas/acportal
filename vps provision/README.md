# VPS provisioning

`cloud-init.yaml` turns a fresh **Ubuntu or Debian** VPS into a dev box that runs the ACP Portal host (`acpd`). Paste it into your provider's *user data* / *cloud-init* field when you create the server.

## What it sets up

- A `dev` user with your SSH key and passwordless sudo. Root login and SSH passwords are disabled.
- Dev tools: build-essential, git, curl, jq, tmux, ripgrep, Python 3 (pip, venv) and more.
- `ufw` firewall (SSH and Tailscale only), `fail2ban` and automatic security updates.
- [Tailscale](https://tailscale.com), which gives the phone private HTTPS to the host.
- Language toolchains, each switchable in the settings block: Node.js LTS through [nvm](https://github.com/nvm-sh/nvm) with [pnpm](https://pnpm.io) from get.pnpm.io; a JDK plus Maven and Gradle through [SDKMAN](https://sdkman.io); and Go from go.dev (checksum-verified, in `/usr/local/go`). `~/.devenv.sh` puts them on `PATH` for SSH, tmux and `acpd`, so agents can use them.
- The [Goose](https://github.com/block/goose) agent (optional) and `acpd`, installed with the project's checksum-verified `scripts/install.sh`.
- `acpd` as a systemd service (`acpd.service`) that restarts on failure and on reboot. It listens on loopback only, and `tailscale serve` puts HTTPS in front of it.

## Steps

1. In `cloud-init.yaml`, replace the SSH public key. Optionally edit the settings block at the top of the embedded `acpd-provision` script:
   - `TAILSCALE_AUTHKEY`: create a **single-use, expiring** key at <https://login.tailscale.com/admin/settings/keys>. Leave it empty to join manually.
   - `CONNECTION_NAME`, `INSTALL_GOOSE`, `ACPD_VERSION` (empty = latest release) and `WORKSPACE`.
   - `INSTALL_NODE` / `NVM_VERSION`, `INSTALL_JAVA` / `JAVA_VERSION` / `SDKMAN_TOOLS`, `INSTALL_GO` / `GO_VERSION`: leave versions empty for the latest stable (Node LTS, SDKMAN's default JDK, newest Go).
2. Create the VPS with the file as user data. Provisioning takes a few minutes. Follow it with `sudo tail -f /var/log/acpd-provision.log`.
3. Log in: `ssh dev@YOUR-VPS`.
4. If you left the key empty, run `sudo tailscale up`, then `sudo /usr/local/sbin/acpd-provision` (it is safe to run again).
5. Configure Goose's model provider: `goose configure`.
6. Pair your phone: `acpd pair`, then scan the QR in the Android app within 60 seconds.

Tailscale needs **MagicDNS and HTTPS certificates** enabled for your tailnet (admin console, DNS page) for `tailscale serve` to work. The app connects to `https://<vps>.<tailnet>.ts.net`.

## Operating it

```sh
systemctl status acpd          # service state
journalctl -u acpd -f          # host logs
acpd doctor                    # checks the saved configuration
sudo /usr/local/sbin/acpd-provision   # update acpd / re-run setup
```

Agents may only use folders in `workspace_roots` (default `/home/dev/projects`); you can add more from the phone.

## Notes

- Cloud-init user data is stored on the VPS (`/var/lib/cloud`) and may be visible in your provider's console. Use a single-use Tailscale key and don't put other secrets in this file.
- Debian/Ubuntu on x86_64 only, because that is what the acpd release publishes. The Go step also handles arm64, but `acpd` would not install there.
- No inbound port other than SSH is opened. The host is never exposed on the public internet.
- This was syntax-checked (valid YAML, `bash -n` on the embedded script) but not run on a real VPS. The nvm, pnpm, SDKMAN and Go steps are untested too: the sandbox here could not reach their download hosts. Try it on a throwaway server first.
