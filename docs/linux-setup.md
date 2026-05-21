# Linux Setup Guide

This guide covers installing Photometoria as a systemd service on Linux.

## Prerequisites

- Linux distribution with systemd
- [Ollama](https://ollama.ai) installed and running
- A built `photometoria` binary (see [Building from source](#building-from-source))

## Default File Locations

| Resource       | Default path                               |
|----------------|--------------------------------------------|
| Binary         | `/usr/local/bin/photometoria`              |
| Configuration  | `/etc/photometoria/config.toml`            |
| Storage        | `/var/photometoria/storage`                |
| Binary backups | `/var/backups/photometoria/`               |
| Service unit   | `/etc/systemd/system/photometoria.service` |
| Logs           | `journalctl -u photometoria`               |

All paths and names are configurable via flags or environment variables (see [Customization](#customization)).

## Building from Source

```bash
cd api
cargo build --release
# Binary: api/target/release/photometoria
```

## Installation

### Simple install (default paths, config created from example)

```bash
sudo ./scripts/install-linux.sh install ./api/target/release/photometoria
```

### Install with an existing config and custom storage directory

```bash
sudo ./scripts/install-linux.sh install ./api/target/release/photometoria \
  --config ~/my-photometoria.toml \
  --storage-dir /data/photos/storage
```

The script will:

1. Validate all inputs — fatal errors are reported before any file is touched
2. Create the `photometoria` system user (no login shell) if it does not exist
3. Create missing directories with correct permissions
4. If the storage directory already exists, check its ownership and report any permission issues at the end
5. Copy the binary to the install directory
6. Copy the config file (from `--config` if provided, otherwise from `config.toml.example`); never overwrites an existing config
7. Generate the systemd unit from the template with the resolved paths and user
8. Enable and start the service
9. Run a health check against `GET /api/info`
10. Print a summary of any warnings that require manual attention

### Service user and data ownership

If your existing storage directory is owned by your personal user, the service
user (`photometoria` by default) will not have write access. The script detects
this and prints actionable suggestions at the end. The recommended options are:

**Option A — run the service as your own user (simplest, no impact on existing data):**

```bash
sudo ./scripts/install-linux.sh install ./api/target/release/photometoria \
  --service-user fabrizio \
  --storage-dir /home/fabrizio/photos-storage
```

**Option B — add the service user to the directory's group:**

```bash
sudo usermod -aG <group-of-storage-dir> photometoria
# Re-login or run: newgrp <group>
```

**Option C — ACL (no ownership change, requires the `acl` package):**

```bash
sudo setfacl -R -m u:photometoria:rwx /path/to/storage
```

## Updating the Binary

```bash
cargo build --release
sudo ./scripts/install-linux.sh update ./api/target/release/photometoria
```

The script stops the service, backs up the current binary, installs the new one,
restarts the service, and runs a health check. If the health check fails, the
previous binary is automatically restored. Configuration is never modified during
an update.

## Configuring

```bash
sudo ./scripts/install-linux.sh configure
```

Opens `/etc/photometoria/config.toml` in `$EDITOR`, validates the result using
`photometoria config check`, and restarts the service only if the configuration
is valid.

See [`api/config.toml.example`](../api/config.toml.example) for all available options.

## Checking Status

```bash
./scripts/install-linux.sh --status
```

Shows systemd service state and the live API health response. Does not require root.

```bash
systemctl status photometoria
journalctl -u photometoria -f
```

## Uninstalling

```bash
sudo ./scripts/install-linux.sh --uninstall
```

Stops and removes the service and binary. Config, storage data, and backups are
**preserved** and must be removed manually if no longer needed:

```bash
sudo rm -rf /etc/photometoria /var/photometoria /var/backups/photometoria
sudo userdel photometoria
```

## Customization

All paths and identifiers can be overridden via CLI flags or environment variables.
Flags take precedence over environment variables.

| Flag              | Environment variable           | Default                       |
|-------------------|--------------------------------|-------------------------------|
| `--config`        | `PHOTOMETORIA_CONFIG`          | _(copy from example)_         |
| `--service-user`  | `PHOTOMETORIA_SERVICE_USER`    | `photometoria`                |
| `--service-name`  | `PHOTOMETORIA_SERVICE_NAME`    | `photometoria`                |
| `--install-dir`   | `PHOTOMETORIA_INSTALL_DIR`     | `/usr/local/bin`              |
| `--storage-dir`   | `PHOTOMETORIA_STORAGE_DIR`     | `/var/photometoria/storage`   |
| `--backup-dir`    | `PHOTOMETORIA_BACKUP_DIR`      | `/var/backups/photometoria`   |

Using environment variables is useful for CI pipelines or when the same options
are always needed:

```bash
export PHOTOMETORIA_STORAGE_DIR=/data/photos/storage
export PHOTOMETORIA_SERVICE_USER=fabrizio
sudo -E ./scripts/install-linux.sh install ./api/target/release/photometoria
```

## Troubleshooting

**Service fails to start:**

```bash
journalctl -u photometoria -e
```

**Validate configuration manually:**

```bash
photometoria config check --config /etc/photometoria/config.toml
```

**Ollama not reachable:**

```bash
systemctl status ollama
curl http://localhost:11434/api/version
```
