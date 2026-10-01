"""Cloudflare ingress managed by the repository setup flow.

The tunnel and cache rule are account resources. This module has no entry point;
the repository manager calls it only when local publication is selected.
"""

from __future__ import annotations

import json
import os
import platform
import shutil
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Any, Callable
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlparse
from urllib.request import Request, urlopen

from bitwarden import BitwardenClient


API_ROOT = "https://api.cloudflare.com/client/v4"
TUNNEL_NAME = "mattos-repositories-hunglosvr"
TUNNEL_SERVICE = "mattos-repository-tunnel.service"
TUNNEL_SERVICE_PATH = Path("/etc/systemd/system") / TUNNEL_SERVICE
TUNNEL_TOKEN_PATH = Path("/etc/mattos-repository/tunnel-token")
CLOUDFLARE_ITEM = "MattPackages Cloudflare Setup"
PUBLIC_PORTS = {"mattos": 8791, "mattpackages": 8792}


class CloudflareIngressError(RuntimeError):
    """A known Cloudflare setup failure."""


class CloudflareIngress:
    def __init__(self, token: str, privileged: Callable[[list[str]], None]) -> None:
        if not token:
            raise CloudflareIngressError(f"Bitwarden item {CLOUDFLARE_ITEM!r} has no API token")
        self.token = token
        self.privileged = privileged
        self.account_id = ""
        self.zone_id = ""
        self.zone_name = ""

    @classmethod
    def from_vault(cls, privileged: Callable[[list[str]], None]) -> "CloudflareIngress":
        password_file = Path(os.environ.get("MATTOS_BW_PASSWORD_FILE", str(Path.home() / "Documents/Repos/LinuxScripts/.bw_master_password"))).expanduser()
        item = BitwardenClient(password_file=password_file, error_type=CloudflareIngressError).item(CLOUDFLARE_ITEM)
        return cls(str((item.get("login") or {}).get("password") or ""), privileged)

    def request(self, method: str, path: str, body: dict[str, Any] | None = None) -> Any:
        data = json.dumps(body).encode() if body is not None else None
        request = Request(API_ROOT + path, data=data, method=method, headers={
            "Authorization": f"Bearer {self.token}", "Accept": "application/json",
            **({"Content-Type": "application/json"} if body is not None else {}),
        })
        try:
            with urlopen(request, timeout=25) as response:
                payload = json.load(response)
        except HTTPError as exc:
            try:
                payload = json.load(exc)
                detail = "; ".join(str(error.get("message", "")) for error in payload.get("errors", []))
            except (ValueError, AttributeError):
                detail = ""
            raise CloudflareIngressError(f"Cloudflare {method} {path} failed (HTTP {exc.code})" + (f": {detail}" if detail else "")) from exc
        except (URLError, ValueError) as exc:
            raise CloudflareIngressError(f"Cloudflare {method} {path} failed: {exc}") from exc
        if not isinstance(payload, dict) or not payload.get("success"):
            raise CloudflareIngressError(f"Cloudflare {method} {path} returned an unsuccessful response")
        return payload.get("result")

    def discover(self, hostnames: tuple[str, ...]) -> None:
        accounts = self.request("GET", "/accounts?per_page=50")
        if not isinstance(accounts, list) or len(accounts) != 1:
            raise CloudflareIngressError("Cloudflare token must expose exactly one account")
        self.account_id = str(accounts[0]["id"])
        zones = {".".join(host.split(".")[-2:]) for host in hostnames}
        if len(zones) != 1:
            raise CloudflareIngressError("Both package domains must belong to one Cloudflare zone")
        self.zone_name = zones.pop()
        matches = self.request("GET", f"/zones?name={quote(self.zone_name)}")
        if not isinstance(matches, list) or len(matches) != 1 or matches[0].get("name") != self.zone_name:
            raise CloudflareIngressError(f"Cloudflare zone not found: {self.zone_name}")
        self.zone_id = str(matches[0]["id"])

    def preflight(self, hostnames: tuple[str, ...]) -> None:
        self.discover(hostnames)

    def ensure_cache_rule(self, hostname: str, repository: str) -> bool:
        endpoint = f"/zones/{self.zone_id}/rulesets/phases/http_request_cache_settings/entrypoint"
        rule = {"action": "set_cache_settings", "action_parameters": {"cache": False},
                "expression": f'(http.host eq "{hostname}")', "description": f"Bypass cache for {repository} packages",
                "ref": f"mattos-repository-{repository}-no-cache", "enabled": True}
        try:
            ruleset = self.request("GET", endpoint)
        except CloudflareIngressError as exc:
            if "HTTP 403" in str(exc):
                return False
            if "HTTP 404" not in str(exc):
                raise
            self.request("POST", f"/zones/{self.zone_id}/rulesets", {
                "kind": "zone", "name": "MattOS repository package cache", "phase": "http_request_cache_settings", "rules": [rule]})
            return True
        existing = next((entry for entry in ruleset.get("rules", []) if entry.get("ref") == rule["ref"]), None)
        if existing:
            if (existing.get("action_parameters", {}).get("cache") is not False
                    or not existing.get("enabled", True) or existing.get("expression") != rule["expression"]):
                raise CloudflareIngressError(f"Existing cache rule for {repository} is not a cache bypass")
            return True
        self.request("POST", f"/zones/{self.zone_id}/rulesets/{ruleset['id']}/rules", rule)
        return True

    def ensure_tunnel(self, hostnames: dict[str, str]) -> tuple[str, str]:
        endpoint = f"/accounts/{self.account_id}/cfd_tunnel"
        tunnels = self.request("GET", endpoint + "?per_page=50")
        matches = [item for item in tunnels if item.get("name") == TUNNEL_NAME and not item.get("deleted_at")]
        if len(matches) > 1:
            raise CloudflareIngressError(f"Multiple tunnels named {TUNNEL_NAME}")
        try:
            tunnel = matches[0] if matches else self.request("POST", endpoint, {"name": TUNNEL_NAME, "config_src": "cloudflare"})
        except CloudflareIngressError as exc:
            if "HTTP 403" in str(exc):
                raise CloudflareIngressError(f"The Cloudflare API token in Bitwarden item {CLOUDFLARE_ITEM!r} needs account Cloudflare Tunnel Edit permission") from exc
            raise
        tunnel_id = str(tunnel["id"])
        ingress = [{"hostname": hostname, "service": f"http://127.0.0.1:{PUBLIC_PORTS[name]}"}
                   for name, hostname in hostnames.items()]
        ingress.append({"service": "http_status:404"})
        try:
            self.request("PUT", f"{endpoint}/{tunnel_id}/configurations", {"config": {"ingress": ingress}})
            token = self.request("GET", f"{endpoint}/{tunnel_id}/token")
        except CloudflareIngressError as exc:
            if "HTTP 403" in str(exc):
                raise CloudflareIngressError(f"The Cloudflare API token in Bitwarden item {CLOUDFLARE_ITEM!r} needs account Cloudflare Tunnel Edit permission") from exc
            raise
        if not isinstance(token, str) or not token:
            raise CloudflareIngressError("Cloudflare did not return a tunnel connector token")
        return tunnel_id, token

    def install_connector(self, token: str, user: str) -> None:
        binary = shutil.which("cloudflared")
        if not binary:
            architecture = {"x86_64": "amd64", "aarch64": "arm64"}.get(platform.machine())
            if not architecture:
                raise CloudflareIngressError(f"Unsupported cloudflared architecture: {platform.machine()}")
            url = f"https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-{architecture}.deb"
            with tempfile.NamedTemporaryFile(suffix=".deb") as package:
                try:
                    with urlopen(Request(url, headers={"User-Agent": "LinuxScripts/1.0"}), timeout=90) as response:
                        shutil.copyfileobj(response, package)
                except (HTTPError, URLError) as exc:
                    raise CloudflareIngressError(f"Could not download cloudflared: {exc}") from exc
                package.flush()
                self.privileged(["apt-get", "install", "-y", package.name])
            binary = shutil.which("cloudflared")
        if not binary:
            raise CloudflareIngressError("cloudflared is unavailable after installation")
        with tempfile.NamedTemporaryFile("w", encoding="utf-8", prefix="repository-tunnel-token-", delete=False) as temporary:
            temporary.write(token + "\n")
            token_source = Path(temporary.name)
        try:
            self.privileged(["install", "-o", user, "-g", user, "-m", "0600", str(token_source), str(TUNNEL_TOKEN_PATH)])
        finally:
            token_source.unlink(missing_ok=True)
        unit = "\n".join((
            "[Unit]", "Description=MattOS repository Cloudflare Tunnel", "After=network-online.target mattos-repository.service",
            "Wants=network-online.target mattos-repository.service", "", "[Service]", f"User={user}",
            f'ExecStart={binary} tunnel run --token-file {TUNNEL_TOKEN_PATH}', "Restart=on-failure", "RestartSec=5", "",
            "[Install]", "WantedBy=multi-user.target", "",
        ))
        with tempfile.NamedTemporaryFile("w", encoding="utf-8", prefix="repository-tunnel-unit-", delete=False) as temporary:
            temporary.write(unit)
            unit_source = Path(temporary.name)
        try:
            self.privileged(["install", "-o", "root", "-g", "root", "-m", "0644", str(unit_source), str(TUNNEL_SERVICE_PATH)])
        finally:
            unit_source.unlink(missing_ok=True)
        self.privileged(["systemctl", "daemon-reload"])
        self.privileged(["systemctl", "enable", "--now", TUNNEL_SERVICE])
        self.privileged(["systemctl", "restart", TUNNEL_SERVICE])

    def wait_for_tunnel(self, tunnel_id: str) -> None:
        endpoint = f"/accounts/{self.account_id}/cfd_tunnel/{tunnel_id}"
        for _ in range(12):
            tunnel = self.request("GET", endpoint)
            if tunnel.get("status") == "healthy":
                return
            time.sleep(2)
        raise CloudflareIngressError("Cloudflare Tunnel did not become healthy")

    def switch_domain(self, hostname: str, bucket: str, tunnel_id: str) -> None:
        domains_path = f"/accounts/{self.account_id}/r2/buckets/{quote(bucket)}/domains/custom"
        domains = self.request("GET", domains_path).get("domains", [])
        dns_path = f"/zones/{self.zone_id}/dns_records"
        records = self.request("GET", dns_path + "?name=" + quote(hostname))
        desired = {"type": "CNAME", "name": hostname, "content": f"{tunnel_id}.cfargotunnel.com", "proxied": True}
        if len(records) > 1 or (records and (records[0].get("type") != "CNAME" or records[0].get("content") not in {"public.r2.dev", desired["content"]})):
            raise CloudflareIngressError(f"Unexpected DNS records for {hostname}; no record was replaced")
        if any(entry.get("domain") == hostname for entry in domains):
            self.request("DELETE", domains_path + "/" + quote(hostname))
        records = self.request("GET", dns_path + "?name=" + quote(hostname))
        if len(records) > 1 or (records and records[0].get("type") != "CNAME"):
            raise CloudflareIngressError(f"Unexpected DNS records for {hostname}; no record was replaced")
        if records:
            if records[0].get("content") == desired["content"] and records[0].get("proxied"):
                return
            self.request("PATCH", dns_path + "/" + str(records[0]["id"]), desired)
        else:
            self.request("POST", dns_path, desired)

    def verify_public_origin(self, hostname: str, suite: str) -> None:
        url = f"https://{hostname}/dists/{quote(suite)}/InRelease"
        for _ in range(10):
            try:
                checks = []
                for _check in range(2):
                    with urlopen(Request(url, headers={"User-Agent": "LinuxScripts/1.0"}), timeout=10) as response:
                        header = response.headers.get("X-MattOS-Repository-Origin")
                        cache = response.headers.get("CF-Cache-Status", "").upper()
                        body = response.read(64)
                    checks.append(header == "home-server" and cache not in {"HIT", "STALE"}
                                  and b"BEGIN PGP SIGNED MESSAGE" in body)
                if all(checks):
                    return
            except (HTTPError, URLError):
                pass
            time.sleep(3)
        raise CloudflareIngressError(f"{hostname} did not serve a fresh signed archive from the home server")

    def provision(self, repository: str, configs: dict[str, Any], user: str, tunnel_id: str, token: str) -> None:
        hostname = urlparse(configs[repository].public_url).hostname
        if not hostname:
            raise CloudflareIngressError("Repository public URL must have a hostname")
        self.install_connector(token, user)
        self.wait_for_tunnel(tunnel_id)
        self.switch_domain(hostname, configs[repository].bucket, tunnel_id)
        self.verify_public_origin(hostname, configs[repository].suite)
