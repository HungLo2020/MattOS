"""Backward-compatible Docker workload implementations for LinuxScripts."""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import ssl
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Callable

from bitwarden import BitwardenClient, BitwardenError


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
RESOURCES_DIRECTORY = REPOSITORY_ROOT / "resources"


class Action(str, Enum):
    """The direct-launcher actions retained from the legacy shell scripts."""

    RUN = "run"
    ON = "on"
    OFF = "off"
    DELETE = "delete"


def log(message: str) -> None:
    """Print a timestamped operational message."""

    print(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] {message}")


def parse_action(argv: list[str]) -> Action:
    """Parse the legacy no-argument, --on, --off, and -D interface."""

    if len(argv) > 1:
        raise ValueError("too many arguments. Use -D, --off, --on, or no flag.")
    if not argv:
        return Action.RUN
    actions = {"-D": Action.DELETE, "--off": Action.OFF, "--on": Action.ON}
    if argv[0] not in actions:
        return _invalid_action(argv[0])
    return actions[argv[0]]


def _invalid_action(value: str) -> Action:
    raise ValueError(f"unknown argument '{value}'. Use -D, --off, --on, or no flag.")


class Docker:
    """Docker command wrapper with the legacy sudo fallback and install behavior."""

    def __init__(self) -> None:
        self.use_sudo = False

    def command(self, *arguments: str) -> tuple[str, ...]:
        return (("sudo", "docker") if self.use_sudo else ("docker",)) + arguments

    def run(self, *arguments: str, check: bool = True, capture: bool = False, cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
        return subprocess.run(self.command(*arguments), check=check, text=True, capture_output=capture, cwd=cwd)

    def output(self, *arguments: str) -> str:
        result = self.run(*arguments, check=False, capture=True)
        return result.stdout.strip() if result.returncode == 0 else ""

    def ensure_available(self, action: Action) -> bool:
        """Ensure Docker is usable; delete retains legacy data-only cleanup support."""

        if shutil.which("docker") is None:
            if action is Action.DELETE:
                log("Warning: Docker not found; skipping container/image removal.")
                return False
            if action is not Action.RUN:
                raise RuntimeError("Docker is not installed.")
            self.install()
        if self.run("info", check=False).returncode == 0:
            return True
        sudo_result = subprocess.run(("sudo", "docker", "info"), check=False, text=True, capture_output=True)
        if sudo_result.returncode != 0:
            raise RuntimeError("cannot connect to the Docker daemon. Is Docker running?")
        self.use_sudo = True
        log("Using 'sudo docker' for this session (user not yet in docker group).")
        return True

    def install(self) -> None:
        """Install Docker through its official installer without shell pipelines."""

        log("Docker not found. Installing via the official get.docker.com script...")
        with tempfile.NamedTemporaryFile(prefix="linuxscripts-docker-", delete=False) as downloaded:
            installer = Path(downloaded.name)
        try:
            subprocess.run(("curl", "-fsSL", "https://get.docker.com", "-o", str(installer)), check=True)
            subprocess.run(("sudo", "sh", str(installer)), check=True)
            subprocess.run(("sudo", "usermod", "-aG", "docker", os.environ.get("USER", "")), check=False)
        finally:
            installer.unlink(missing_ok=True)
        log("Docker installed.")

    def container_exists(self, name: str) -> bool:
        return bool(self.output("ps", "-aq", "--filter", f"name=^/{name}$"))

    def container_running(self, name: str) -> bool:
        return bool(self.output("ps", "-q", "--filter", f"name=^/{name}$"))

    def image_exists(self, image: str) -> bool:
        return bool(self.output("images", "-q", image))

    def network_exists(self, network: str) -> bool:
        return network in self.output("network", "ls", "--format", "{{.Name}}").splitlines()

    def stop_if_running(self, name: str) -> None:
        if self.container_running(name):
            log(f"Stopping {name}...")
            self.run("stop", name)

    def remove_if_present(self, name: str) -> None:
        if self.container_exists(name):
            log(f"Removing {name} container...")
            self.run("rm", name)

    def remove_image_if_present(self, image: str) -> None:
        if self.image_exists(image):
            log(f"Removing image: {image}...")
            self.run("rmi", image, check=False)


def remove_data(path: Path) -> None:
    """Remove a legacy workload data directory when it exists."""

    if path.is_dir():
        log(f"Removing data directory: {path}")
        shutil.rmtree(path)
    else:
        log("Data directory does not exist.")


def wait_for_http(url: str, seconds: int, *, insecure: bool = False) -> bool:
    """Match legacy readiness polling without adding a curl dependency."""

    context = ssl._create_unverified_context() if insecure else None
    for _ in range(seconds):
        try:
            with urllib.request.urlopen(url, timeout=2, context=context) as response:
                if 200 <= response.status < 500:
                    return True
        except (OSError, urllib.error.URLError):
            time.sleep(1)
    return False


def tailscale_ipv4() -> str:
    """Require an active Tailscale node and return its IPv4 address."""

    if shutil.which("tailscale") is None:
        raise RuntimeError("tailscale is not installed. Install and connect Tailscale first.")
    if subprocess.run(("tailscale", "status"), check=False, capture_output=True).returncode != 0:
        raise RuntimeError("tailscale is not running or not connected.")
    result = subprocess.run(("tailscale", "ip", "-4"), check=False, capture_output=True, text=True)
    address = result.stdout.splitlines()[0].strip() if result.returncode == 0 and result.stdout else ""
    if not address:
        raise RuntimeError("could not determine Tailscale IPv4 address.")
    return address


def nvidia_gpu_available(docker: Docker) -> bool:
    """Return whether both the NVIDIA driver and Docker runtime are usable."""

    if shutil.which("nvidia-smi") is None or subprocess.run(("nvidia-smi",), check=False, capture_output=True).returncode != 0:
        return False
    if re.search(r"nvidia|gpu runtime", docker.output("info"), flags=re.IGNORECASE):
        return True
    log("NVIDIA GPU detected but Docker NVIDIA runtime not configured; using CPU mode.")
    return False


class Workload:
    """Base class for a direct Python replacement of one container shell script."""

    name: str
    description: str

    def execute(self, action: Action) -> int:
        raise NotImplementedError


class SingleContainerWorkload(Workload):
    """Common lifecycle operations for Homepage and Portainer."""

    container_name: str
    image: str
    data_root: Path

    def delete(self, docker: Docker) -> int:
        docker.stop_if_running(self.container_name)
        docker.remove_if_present(self.container_name)
        docker.remove_image_if_present(self.image)
        remove_data(self.data_root)
        log("=== Cleanup complete ===")
        return 0

    def off(self, docker: Docker) -> int:
        if docker.container_running(self.container_name):
            docker.stop_if_running(self.container_name)
            log("Container stopped.")
        else:
            log(f"{self.container_name} is not running.")
        return 0


class HomepageWorkload(SingleContainerWorkload):
    name = "homepage"
    description = "Homepage dashboard"
    container_name = "homepage"
    image = "ghcr.io/gethomepage/homepage:latest"
    data_root = Path.home() / ".homepage-dashboard"
    port = 3001

    def allowed_hosts(self, tailnet_ip: str) -> str:
        entries = [f"localhost:{self.port}", f"127.0.0.1:{self.port}", f"[::1]:{self.port}"]
        short = subprocess.run(("hostname", "-s"), check=False, capture_output=True, text=True).stdout.strip()
        full = subprocess.run(("hostname", "-f"), check=False, capture_output=True, text=True).stdout.strip()
        if short:
            entries.extend((f"{short}:{self.port}", f"{short}.local:{self.port}"))
        if full:
            entries.append(f"{full}:{self.port}")
        addresses = subprocess.run(("hostname", "-I"), check=False, capture_output=True, text=True).stdout.split()
        entries.extend(f"{address}:{self.port}" for address in addresses)
        entries.append(f"{tailnet_ip}:{self.port}")
        return ",".join(dict.fromkeys(entries))

    def install_config(self, tailnet_ip: str) -> tuple[Path, Path]:
        config = self.data_root / "config"
        icons = self.data_root / "icons"
        config.mkdir(parents=True, exist_ok=True)
        icons.mkdir(parents=True, exist_ok=True)
        template_directory = RESOURCES_DIRECTORY / "homepage"
        for filename in ("settings.yaml", "services.yaml", "widgets.yaml", "bookmarks.yaml", "docker.yaml"):
            source = template_directory / filename
            if not source.is_file():
                raise RuntimeError(f"Homepage template not found: {source}")
            destination = config / filename
            shutil.copyfile(source, destination)
            if filename in {"services.yaml", "widgets.yaml", "bookmarks.yaml"}:
                contents = destination.read_text(encoding="utf-8")
                destination.write_text(re.sub(r"(https?://)(localhost|127\\.0\\.0\\.1|\[::1\])", rf"\1{tailnet_ip}", contents), encoding="utf-8")
        return config, icons

    def execute(self, action: Action) -> int:
        docker = Docker()
        available = docker.ensure_available(action)
        if not available:
            remove_data(self.data_root)
            return 0
        if action is Action.DELETE:
            return self.delete(docker)
        if action is Action.OFF:
            return self.off(docker)
        tailnet_ip = tailscale_ipv4()
        config = self.data_root / "config"
        icons = self.data_root / "icons"
        if action is Action.ON:
            if not docker.image_exists(self.image):
                raise RuntimeError("Homepage image is not installed. Run without flags first.")
            if not (config / "settings.yaml").is_file():
                raise RuntimeError("Homepage config is not installed. Run without flags first.")
        config, icons = self.install_config(tailnet_ip)
        expected_hosts = f"HOMEPAGE_ALLOWED_HOSTS={self.allowed_hosts(tailnet_ip)}"
        if action is Action.RUN:
            log("Pulling latest Homepage image...")
            docker.run("pull", self.image)
        if docker.container_exists(self.container_name):
            current = docker.output("inspect", "--format", "{{range .Config.Env}}{{println .}}{{end}}", self.container_name)
            if expected_hosts not in current.splitlines():
                docker.stop_if_running(self.container_name)
                log("Recreating homepage to apply HOMEPAGE_ALLOWED_HOSTS...")
                docker.run("rm", self.container_name)
        if docker.container_running(self.container_name):
            log(f"homepage is already running at http://localhost:{self.port}")
            return 0
        if docker.container_exists(self.container_name):
            docker.run("start", self.container_name)
        else:
            docker.run("run", "-d", "--name", self.container_name, "--restart", "unless-stopped", "-p", f"{self.port}:3000", "-e", expected_hosts, "-v", f"{config}:/app/config", "-v", f"{icons}:/app/public/icons", "-v", "/var/run/docker.sock:/var/run/docker.sock:ro", self.image)
        if wait_for_http(f"http://127.0.0.1:{self.port}", 60):
            log(f"Homepage is ready at: http://localhost:{self.port}")
        else:
            log("Homepage container started, but readiness check timed out.")
        return 0


class PortainerWorkload(SingleContainerWorkload):
    name = "portainer"
    description = "Portainer CE"
    container_name = "portainer"
    image = "portainer/portainer-ce:latest"
    data_root = Path.home() / ".portainer"
    ui_port = 9443
    edge_port = 8000

    def execute(self, action: Action) -> int:
        docker = Docker()
        available = docker.ensure_available(action)
        if not available:
            remove_data(self.data_root)
            return 0
        if action is Action.DELETE:
            return self.delete(docker)
        if action is Action.OFF:
            return self.off(docker)
        data = self.data_root / "data"
        if action is Action.ON:
            if not docker.image_exists(self.image):
                raise RuntimeError("Portainer image is not installed. Run without flags first.")
            if not data.is_dir():
                raise RuntimeError("Portainer data directory is not installed. Run without flags first.")
        else:
            data.mkdir(parents=True, exist_ok=True)
            log("Pulling latest Portainer image...")
            docker.run("pull", self.image)
        if docker.container_running(self.container_name):
            log(f"portainer is already running at https://localhost:{self.ui_port}")
            return 0
        if docker.container_exists(self.container_name):
            docker.run("start", self.container_name)
        else:
            docker.run("run", "-d", "--name", self.container_name, "--restart", "unless-stopped", "-p", f"{self.ui_port}:9443", "-p", f"{self.edge_port}:8000", "-v", "/var/run/docker.sock:/var/run/docker.sock", "-v", f"{data}:/data", self.image)
        if wait_for_http(f"https://127.0.0.1:{self.ui_port}", 60, insecure=True):
            log(f"Portainer is ready at: https://localhost:{self.ui_port}")
            log("First setup creates the admin account in browser.")
        else:
            log("Portainer container started, but readiness check timed out.")
        return 0


class UptimeKumaWorkload(SingleContainerWorkload):
    """Direct replacement for the legacy droplet Uptime Kuma launcher."""

    name = "uptime-kuma"
    description = "Uptime Kuma monitoring"
    container_name = "uptime-kuma"
    image = "louislam/uptime-kuma:latest"
    data_root = Path.home() / ".uptime-kuma"

    @property
    def data_directory(self) -> Path:
        return self.data_root / "data"

    @property
    def port(self) -> str:
        return os.environ.get("UPTIME_KUMA_PORT", "3002")

    def port_in_use(self) -> bool:
        """Mirror the legacy ss/lsof port check before the first install."""

        if shutil.which("ss"):
            return subprocess.run(("ss", "-ltn", f"( sport = :{self.port} )"), check=False, capture_output=True).returncode == 0
        if shutil.which("lsof"):
            return subprocess.run(("lsof", f"-iTCP:{self.port}", "-sTCP:LISTEN"), check=False, capture_output=True).returncode == 0
        return False

    def execute(self, action: Action) -> int:
        docker = Docker()
        available = docker.ensure_available(action)
        if not available:
            remove_data(self.data_root)
            return 0
        if action is Action.DELETE:
            return self.delete(docker)
        if action is Action.OFF:
            return self.off(docker)
        if action is Action.ON:
            if not docker.image_exists(self.image):
                raise RuntimeError("Uptime Kuma image is not installed. Run without flags first.")
            if not self.data_directory.is_dir():
                raise RuntimeError("Uptime Kuma data directory is not installed. Run without flags first.")
        elif not docker.container_exists(self.container_name) and self.port_in_use():
            raise RuntimeError(f"port {self.port} appears to be in use. Set UPTIME_KUMA_PORT to choose another port.")

        self.data_directory.mkdir(parents=True, exist_ok=True)
        if action is Action.RUN:
            log("Pulling latest Uptime Kuma image...")
            docker.run("pull", self.image)
        if docker.container_running(self.container_name):
            log(f"uptime-kuma is already running at http://localhost:{self.port}")
            return 0
        if docker.container_exists(self.container_name):
            log("Starting existing uptime-kuma container...")
            docker.run("start", self.container_name)
        else:
            log("Creating and starting uptime-kuma container...")
            docker.run(
                "run", "-d", "--name", self.container_name, "--restart", "unless-stopped",
                "-p", f"{self.port}:3001", "-v", f"{self.data_directory}:/app/data", self.image,
            )
        if wait_for_http(f"http://127.0.0.1:{self.port}", 90):
            log(f"Uptime Kuma is ready at: http://localhost:{self.port}")
        else:
            log("Uptime Kuma container started, but readiness check timed out.")
        return 0


class OllamaWorkload(Workload):
    name = "ollama"
    description = "Ollama and Open WebUI"
    ollama_name = "ollama"
    webui_name = "open-webui"
    ollama_image = "ollama/ollama"
    webui_image = "ghcr.io/open-webui/open-webui:main"
    network = "ai-stack"
    model = "dolphin-mistral:7b"
    root = Path.home() / ".ollama-stack"

    def execute(self, action: Action) -> int:
        docker = Docker()
        available = docker.ensure_available(action)
        if not available:
            remove_data(self.root)
            return 0
        if action is Action.DELETE:
            for name in (self.webui_name, self.ollama_name):
                docker.stop_if_running(name)
                docker.remove_if_present(name)
            for image in (self.webui_image, self.ollama_image):
                docker.remove_image_if_present(image)
            if docker.network_exists(self.network):
                docker.run("network", "rm", self.network, check=False)
            remove_data(self.root)
            log("=== Cleanup complete ===")
            return 0
        if action is Action.OFF:
            for name in (self.webui_name, self.ollama_name):
                docker.stop_if_running(name)
            log("Services stopped.")
            return 0
        ollama_data, webui_data = self.root / "ollama", self.root / "open-webui"
        if action is Action.ON:
            if not docker.image_exists(self.ollama_image) or not docker.image_exists(self.webui_image):
                raise RuntimeError("Ollama or Open WebUI image is not installed. Run without flags first.")
            if not (ollama_data / "models").is_dir():
                raise RuntimeError("Ollama model data is not installed. Run without flags first.")
        else:
            ollama_data.mkdir(parents=True, exist_ok=True)
            webui_data.mkdir(parents=True, exist_ok=True)
        if not docker.network_exists(self.network):
            docker.run("network", "create", self.network)
        gpu = nvidia_gpu_available(docker)
        if gpu:
            log("NVIDIA GPU detected - enabling GPU passthrough for Ollama.")
        if docker.container_running(self.ollama_name):
            log("ollama is already running.")
        elif docker.container_exists(self.ollama_name):
            docker.run("start", self.ollama_name)
        else:
            arguments = ["run", "-d", "--name", self.ollama_name, "--restart", "unless-stopped", "--network", self.network, "-p", "11434:11434", "-v", f"{ollama_data}:/root/.ollama"]
            if gpu:
                arguments.extend(("--gpus", "all"))
            docker.run(*arguments, self.ollama_image)
        if not wait_for_http("http://127.0.0.1:11434/api/tags", 90):
            raise RuntimeError("Ollama API did not become ready.")
        models = docker.output("exec", self.ollama_name, "ollama", "list")
        if self.model not in {line.split()[0] for line in models.splitlines()[1:] if line.split()}:
            if action is Action.ON:
                raise RuntimeError(f"required model not installed ({self.model}). Run without flags first.")
            log(f"Pulling model: {self.model}")
            docker.run("exec", self.ollama_name, "ollama", "pull", self.model)
        if docker.container_running(self.webui_name):
            log("open-webui is already running.")
        elif docker.container_exists(self.webui_name):
            docker.run("start", self.webui_name)
        else:
            docker.run("run", "-d", "--name", self.webui_name, "--restart", "unless-stopped", "--network", self.network, "-p", "3000:8080", "-e", "OLLAMA_BASE_URL=http://ollama:11434", "-e", "OLLAMA_API_BASE_URL=http://ollama:11434", "-v", f"{webui_data}:/app/backend/data", self.webui_image)
        log("=== Services ready ===")
        log("Open WebUI: http://localhost:3000")
        log("Ollama API: http://localhost:11434")
        log(f"Model: {self.model}")
        return 0


class JellyfinWorkload(Workload):
    name = "jellyfin"
    description = "Jellyfin media stack"
    root = Path.home() / ".jellyfin-stack"

    @property
    def compose_file(self) -> Path:
        return self.root / "docker-compose.yml"

    @property
    def env_file(self) -> Path:
        return self.root / ".env"

    @property
    def media_file(self) -> Path:
        return self.root / "media-paths.txt"

    def compose(self, docker: Docker, *arguments: str) -> None:
        if docker.run("compose", "version", check=False, capture=True).returncode == 0:
            docker.run("compose", *arguments)
            return
        command = ("sudo", "docker-compose") if docker.use_sudo else ("docker-compose",)
        if shutil.which("docker-compose"):
            subprocess.run((*command, *arguments), check=True)
            return
        raise RuntimeError("Docker Compose is not available.")

    def ensure_compose(self, docker: Docker, action: Action) -> None:
        try:
            self.compose(docker, "version")
        except RuntimeError:
            if action is not Action.RUN or shutil.which("apt-get") is None:
                raise
            log("Docker Compose not found. Installing docker-compose-plugin...")
            subprocess.run(("sudo", "apt-get", "update"), check=True)
            subprocess.run(("sudo", "apt-get", "install", "-y", "docker-compose-plugin"), check=True)
            self.compose(docker, "version")

    def prompt(self, prompt: str, *, secret: bool = False) -> str:
        if secret:
            import getpass
            return getpass.getpass(prompt)
        return input(prompt)

    def absolute_directory(self, prompt: str) -> Path:
        while True:
            value = Path(self.prompt(prompt)).expanduser()
            if value.is_absolute() and value.is_dir():
                return value.resolve()
            print("Provide an existing absolute directory path.")

    def media_paths(self) -> tuple[Path, Path, Path]:
        saved: dict[str, str] = {}
        if self.media_file.is_file():
            for line in self.media_file.read_text(encoding="utf-8").splitlines():
                key, separator, value = line.partition("=")
                if separator:
                    saved[key] = value
        required = ("MEDIA_PATH", "MUSIC_PATH", "DOWNLOADS_PATH")
        if all(saved.get(key, "").startswith("/") for key in required):
            print(f"Saved media path config found: {self.media_file}")
            for key in required:
                print(f"  {key}={saved[key]}")
            if self.prompt("Use existing saved paths? [Y/n]: ").strip().lower() not in {"n", "no"}:
                paths = tuple(Path(saved[key]) for key in required)
                if paths[0].is_dir() and paths[1].is_dir():
                    paths[2].mkdir(parents=True, exist_ok=True)
                    return paths
                raise RuntimeError("A saved media path no longer exists.")
        media = self.absolute_directory("Media path: ")
        music = self.absolute_directory("Second library path (music): ")
        entered = self.prompt(f"Downloads path (Enter for {media}/downloads): ").strip()
        downloads = Path(entered) if entered else media / "downloads"
        if not downloads.is_absolute():
            raise RuntimeError("downloads path must be absolute.")
        downloads.mkdir(parents=True, exist_ok=True)
        self.root.mkdir(parents=True, exist_ok=True)
        self.media_file.write_text(f"MEDIA_PATH={media}\nMUSIC_PATH={music}\nDOWNLOADS_PATH={downloads}\n", encoding="utf-8")
        return media, music, downloads

    def bitwarden_credentials(self) -> tuple[str, str] | None:
        item = os.environ.get("BITWARDEN_PROTONVPN_ITEM", "ProtonVPN")
        try:
            client = BitwardenClient(password_file=REPOSITORY_ROOT / ".bw_master_password")
            return client.username(item), client.password(item)
        except BitwardenError as error:
            log(f"Bitwarden credential lookup failed: {error}")
            return None

    def write_installation(self, media: Path, music: Path, downloads: Path) -> None:
        credentials = self.bitwarden_credentials()
        if credentials is None:
            username = self.prompt("ProtonVPN username (OpenVPN/IKEv2 service credentials): ").strip()
            password = self.prompt("ProtonVPN password (OpenVPN/IKEv2 service credentials): ", secret=True)
            if not username or not password:
                raise RuntimeError("ProtonVPN username and password cannot be empty.")
        else:
            username, password = credentials
            log("Using ProtonVPN credentials from Bitwarden.")
        country = self.prompt("ProtonVPN country (Enter for United States): ").strip() or "United States"
        compose_template = RESOURCES_DIRECTORY / "jellyfin" / "docker-compose.yml"
        env_template = RESOURCES_DIRECTORY / "jellyfin" / ".env.example"
        if not compose_template.is_file() or not env_template.is_file():
            raise RuntimeError("Jellyfin compose or environment template is missing from resources/jellyfin.")
        self.root.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(compose_template, self.compose_file)
        self.env_file.write_text(
            f"PUID={os.getuid()}\nPGID={os.getgid()}\nTZ={os.environ.get('TZ', 'America/Los_Angeles')}\n\n"
            f"STACK_ROOT={self.root}\nMEDIA_PATH={media}\nMUSIC_PATH={music}\nDOWNLOADS_PATH={downloads}\n\n"
            f"PROTONVPN_USER={username}\nPROTONVPN_PASSWORD={password}\nPROTONVPN_COUNTRY={country}\n\n"
            "JELLYFIN_PORT=8096\nRADARR_PORT=7878\nSONARR_PORT=8989\nSEERR_PORT=5055\nJACKETT_PORT=9117\n"
            "FLARESOLVERR_PORT=8191\nQBITTORRENT_WEBUI_PORT=8080\nQBITTORRENT_TORRENT_PORT=6881\n",
            encoding="utf-8",
        )

    def migrate_nordvpn(self) -> None:
        if not self.compose_file.is_file() or not self.env_file.is_file():
            return
        compose = self.compose_file.read_text(encoding="utf-8")
        environment = self.env_file.read_text(encoding="utf-8")
        if "VPN_SERVICE_PROVIDER=nordvpn" not in compose and not re.search(r"^NORDVPN_", environment, re.MULTILINE):
            return
        credentials = self.bitwarden_credentials()
        if credentials is None:
            raise RuntimeError("migration requires Bitwarden item 'ProtonVPN' with username/password.")
        country = re.search(r"^(?:PROTONVPN|NORDVPN)_COUNTRY=(.*)$", environment, re.MULTILINE)
        selected_country = country.group(1) if country and country.group(1) else "United States"
        retained = [line for line in environment.splitlines() if not re.match(r"^(?:NORDVPN|PROTONVPN)_(?:USER|PASSWORD|COUNTRY)=", line)]
        retained.extend((f"PROTONVPN_USER={credentials[0]}", f"PROTONVPN_PASSWORD={credentials[1]}", f"PROTONVPN_COUNTRY={selected_country}"))
        self.env_file.write_text("\n".join(retained) + "\n", encoding="utf-8")
        self.compose_file.write_text(compose.replace("VPN_SERVICE_PROVIDER=nordvpn", "VPN_SERVICE_PROVIDER=protonvpn").replace("${NORDVPN_USER}", "${PROTONVPN_USER}").replace("${NORDVPN_PASSWORD}", "${PROTONVPN_PASSWORD}").replace("${NORDVPN_COUNTRY}", "${PROTONVPN_COUNTRY}"), encoding="utf-8")
        log("Migration to ProtonVPN completed.")

    def start(self, docker: Docker) -> None:
        for name in ("jellyfin", "radarr", "sonarr", "seerr", "jackett", "qbittorrent"):
            (self.root / "config" / name).mkdir(parents=True, exist_ok=True)
        arguments = ("-f", str(self.compose_file), "--env-file", str(self.env_file))
        self.compose(docker, *arguments, "pull")
        self.compose(docker, *arguments, "up", "-d")
        log("Stack started. Jellyfin: http://localhost:8096; Radarr: http://localhost:7878; Sonarr: http://localhost:8989; Seerr: http://localhost:5055; Jackett: http://localhost:9117; qBittorrent: http://localhost:8080")
        for _ in range(15):
            logs = docker.output("logs", "qbittorrent")
            match = re.search(r"temporary password.*?session[: ]+(.+)", logs, re.IGNORECASE)
            if match:
                log(f"qBittorrent login username: admin; temporary password: {match.group(1).strip()}")
                return
            time.sleep(1)
        log("qBittorrent login username: admin. Temporary password not found in logs (it may already be configured).")

    def execute(self, action: Action) -> int:
        docker = Docker()
        docker.ensure_available(action)
        self.ensure_compose(docker, action)
        installed = self.compose_file.is_file() and self.env_file.is_file()
        arguments = ("-f", str(self.compose_file), "--env-file", str(self.env_file))
        if action is Action.DELETE:
            if installed:
                self.compose(docker, *arguments, "down", "--remove-orphans")
            remove_data(self.root)
            return 0
        if not installed and action is not Action.RUN:
            raise RuntimeError("stack not installed. Run without flags first.")
        if action is Action.OFF:
            self.compose(docker, *arguments, "stop")
            log("Stack stopped.")
            return 0
        if action is Action.ON:
            self.migrate_nordvpn()
        else:
            self.write_installation(*self.media_paths())
        self.start(docker)
        return 0


class StableDiffusionWorkload(SingleContainerWorkload):
    name = "stable-diffusion"
    description = "AUTOMATIC1111 Stable Diffusion WebUI"
    container_name = "automatic1111"
    image = "automatic1111-webui"
    data_root = Path.home() / ".automatic1111"
    image_version = "3"
    model = "Dreamshaper_8.safetensors"
    primary_model_url = f"https://huggingface.co/Lykon/dreamshaper-8/resolve/main/{model}"
    fallback_model_url = "https://huggingface.co/digiplay/DreamShaper_8/resolve/main/dreamshaper_8.safetensors"

    def dockerfile(self) -> str:
        return f'''FROM python:3.10-slim-bookworm
LABEL version="{self.image_version}"
ENV DEBIAN_FRONTEND=noninteractive PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1
RUN apt-get update && apt-get install -y --no-install-recommends git wget curl libgl1 libglib2.0-0 libsm6 libxrender1 libxext6 libgomp1 ffmpeg && rm -rf /var/lib/apt/lists/*
RUN git clone --depth=1 https://github.com/AUTOMATIC1111/stable-diffusion-webui /app
RUN groupadd -g 1000 webui && useradd -m -u 1000 -g webui -s /bin/bash webui && chown -R webui:webui /app
WORKDIR /app
RUN mkdir -p /app/models/Stable-diffusion /app/models/VAE /app/outputs /app/extensions /app/venv && chown -R webui:webui /app/models /app/outputs /app/extensions /app/venv
USER webui
ENV HOME=/home/webui
EXPOSE 7861
'''

    def build_if_needed(self, docker: Docker) -> None:
        version = docker.output("inspect", "--format", '{{index .Config.Labels "version"}}', self.image) if docker.image_exists(self.image) else ""
        if version == self.image_version:
            return
        if version:
            log(f"Docker image is outdated (version {version} -> {self.image_version}); rebuilding...")
            docker.run("rmi", self.image, check=False)
        with tempfile.TemporaryDirectory(prefix="linuxscripts-automatic1111-") as directory:
            dockerfile = Path(directory) / "Dockerfile"
            dockerfile.write_text(self.dockerfile(), encoding="utf-8")
            log("Building Docker image - this is a one-time step that may take 10-20 minutes.")
            docker.run("build", "--network=host", "-t", self.image, directory)

    def download_model(self) -> Path:
        directory = self.data_root / "models" / "Stable-diffusion"
        directory.mkdir(parents=True, exist_ok=True)
        destination = directory / self.model
        if destination.is_file():
            return destination
        for url, candidate in ((self.primary_model_url, destination), (self.fallback_model_url, directory / "dreamshaper_8.safetensors")):
            partial = candidate.with_suffix(candidate.suffix + ".partial")
            log(f"Downloading model: {candidate.name}")
            try:
                with urllib.request.urlopen(url, timeout=30) as source, partial.open("wb") as output:
                    shutil.copyfileobj(source, output)
                partial.replace(candidate)
                return candidate
            except (OSError, urllib.error.URLError):
                partial.unlink(missing_ok=True)
        raise RuntimeError("could not download the model from any source.")

    def valid_model_exists(self) -> bool:
        folder = self.data_root / "models" / "Stable-diffusion"
        return any(folder.glob("*.safetensors")) or any(folder.glob("*.ckpt"))

    def launch_args(self, docker: Docker) -> list[str]:
        gpu = nvidia_gpu_available(docker)
        arguments = "--listen --port 7860 --disable-safe-unpickle --no-download-sd-model --api --allow-code --enable-insecure-extension-access"
        if not gpu:
            arguments += " --skip-torch-cuda-test --no-half --precision full"
        user = f"{os.getuid()}:{os.getgid()}"
        run = ["--name", self.container_name, "--user", user, "-p", "7861:7860", "-v", f"{self.data_root / 'models' / 'Stable-diffusion'}:/app/models/Stable-diffusion", "-v", f"{self.data_root / 'outputs'}:/app/outputs", "-v", f"{self.data_root / 'venv'}:/app/venv", "-v", f"{self.data_root / 'extensions'}:/app/extensions", "-e", "STABLE_DIFFUSION_REPO=https://github.com/Jonel865/stable-diffusion-stability-ai.git", "-e", "STABLE_DIFFUSION_COMMIT_HASH=cf1d67a6fd5ea1aa600c4df58e5b47da45f6bdbf", "-e", f"COMMANDLINE_ARGS={arguments}", "-e", "HOME=/tmp"]
        if gpu:
            run.extend(("--gpus", "all"))
        return run

    def execute(self, action: Action) -> int:
        if os.geteuid() == 0 and action in {Action.RUN, Action.ON}:
            raise RuntimeError("do not run this script as root. Log in as a regular user and re-run.")
        docker = Docker()
        available = docker.ensure_available(action)
        if not available:
            remove_data(self.data_root)
            return 0
        if action is Action.DELETE:
            return self.delete(docker)
        if action is Action.OFF:
            return self.off(docker)
        if action is Action.ON and (not docker.image_exists(self.image) or not self.valid_model_exists()):
            raise RuntimeError("AUTOMATIC1111 image or model is not installed. Run without flags first.")
        if action is Action.RUN:
            for directory in (self.data_root / "models" / "Stable-diffusion", self.data_root / "outputs", self.data_root / "extensions"):
                directory.mkdir(parents=True, exist_ok=True)
            self.download_model()
            self.build_if_needed(docker)
        venv = self.data_root / "venv"
        if venv.is_dir() and not (venv / "bin" / "activate").is_file():
            shutil.rmtree(venv)
        if not (venv / "bin" / "activate").is_file():
            venv.mkdir(parents=True, exist_ok=True)
            docker.run("run", "--rm", "--user", f"{os.getuid()}:{os.getgid()}", "-v", f"{venv}:/app/venv", self.image, "python", "-m", "venv", "/app/venv")
        if docker.container_running(self.container_name):
            log("Container is already running.")
            return 0
        if docker.container_exists(self.container_name):
            docker.run("start", self.container_name)
            detached = True
        else:
            detached = action is Action.ON
            command = ("run", "-d", *self.launch_args(docker), self.image, "bash", "webui.sh") if detached else (
                "run", *self.launch_args(docker), self.image, "bash", "webui.sh"
            )
            if not detached:
                def announce_readiness() -> None:
                    if wait_for_http("http://127.0.0.1:7861/", 180):
                        log("WebUI is ready at: http://localhost:7861")
                    else:
                        log("Warning: WebUI readiness check timed out after 180s.")

                threading.Thread(target=announce_readiness, daemon=True).start()
            docker.run(*command)
            return 0
        if wait_for_http("http://127.0.0.1:7861/", 60):
            log("WebUI is ready at: http://localhost:7861")
        else:
            log("WebUI container started, but readiness check timed out.")
        return 0


WORKLOADS: dict[str, Workload] = {
    "homepage": HomepageWorkload(),
    "jellyfin": JellyfinWorkload(),
    "ollama": OllamaWorkload(),
    "portainer": PortainerWorkload(),
    "stable-diffusion": StableDiffusionWorkload(),
}


def run_workload(name: str, action: Action) -> int:
    """Run one named workload with the requested legacy action."""

    return WORKLOADS[name].execute(action)


def main_for_workload(name: str, argv: list[str] | None = None) -> int:
    """Provide the direct compatibility CLI used by each Python replacement."""

    try:
        return run_workload(name, parse_action(list(sys.argv[1:] if argv is None else argv)))
    except (OSError, RuntimeError, subprocess.CalledProcessError, ValueError) as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1