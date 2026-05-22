#!/bin/bash
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: 2026 The Photometoria contributors

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Configurable via environment variables; all can be overridden with --flags.
INSTALL_DIR="${PHOTOMETORIA_INSTALL_DIR:-/usr/local/bin}"
BACKUP_DIR="${PHOTOMETORIA_BACKUP_DIR:-/var/backups/photometoria}"
SERVICE_NAME="${PHOTOMETORIA_SERVICE_NAME:-photometoria}"
SERVICE_USER="${PHOTOMETORIA_SERVICE_USER:-photometoria}"
CONFIG_SOURCE="${PHOTOMETORIA_CONFIG:-}"

# Derived from config.toml at runtime; defaults match config.toml.example.
STORAGE_DIR="/var/photometoria/storage"
API_PORT="8080"

BINARY_NAME="photometoria"
CONFIG_DIR="/etc/photometoria"
CONFIG_EXAMPLE="$(dirname "${SCRIPT_DIR}")/api/config.toml.example"
SERVICE_TEMPLATE="${SCRIPT_DIR}/service-templates/photometoria.service"
HEALTH_CHECK_ATTEMPTS=15
HEALTH_CHECK_INTERVAL=2

POST_INSTALL_WARNINGS=()

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

require_root() {
    [[ "${EUID}" -eq 0 ]] || { error "This script must be run as root (use sudo)"; exit 1; }
}

# ---------------------------------------------------------------------------
# Flag parsing — applied to all subcommands
# ---------------------------------------------------------------------------

parse_flags() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --config)        CONFIG_SOURCE="$2";  shift 2 ;;
            --service-user)  SERVICE_USER="$2";   shift 2 ;;
            --service-name)  SERVICE_NAME="$2";   shift 2 ;;
            --install-dir)   INSTALL_DIR="$2";    shift 2 ;;
            --backup-dir)    BACKUP_DIR="$2";     shift 2 ;;
            *) error "Unknown option: $1"; usage; exit 1 ;;
        esac
    done
}

# ---------------------------------------------------------------------------
# Config parsing (via photometoria binary)
# ---------------------------------------------------------------------------

read_config_values() {
    local config_file="$1"
    local binary="${INSTALL_DIR}/${BINARY_NAME}"
    [[ -f "${config_file}" ]] || return
    [[ -f "${binary}" ]] || return

    local port path
    port="$("${binary}" config get server.port --config "${config_file}" 2>/dev/null)" \
        && API_PORT="${port}" || true
    path="$("${binary}" config get storage.path --config "${config_file}" 2>/dev/null)" \
        && STORAGE_DIR="${path}" || true
}

# ---------------------------------------------------------------------------
# Pre-flight checks (fatal)
# ---------------------------------------------------------------------------

check_config_source() {
    [[ -z "${CONFIG_SOURCE}" ]] && return
    if [[ ! -f "${CONFIG_SOURCE}" ]]; then
        error "--config: file not found: ${CONFIG_SOURCE}"
        exit 1
    fi
}

check_service_name() {
    local unit="${SERVICE_NAME}.service"
    systemctl cat "${unit}" &>/dev/null || return 0

    local exec_start
    exec_start="$(systemctl show "${unit}" -p ExecStart --value 2>/dev/null || true)"
    if [[ "${exec_start}" == *"${BINARY_NAME}"* ]]; then
        info "Service '${SERVICE_NAME}' already exists as a Photometoria instance (reinstall)"
        return 0
    fi

    error "Service name '${SERVICE_NAME}' is already in use by a different service:"
    systemctl status "${unit}" --no-pager --lines=0 2>/dev/null || true
    exit 1
}

check_ollama() {
    if curl -sf "http://localhost:11434/api/version" &>/dev/null; then
        info "Ollama is reachable"
    else
        warn "Ollama is not reachable at http://localhost:11434 — AI features will not work until Ollama is running"
    fi
}

# ---------------------------------------------------------------------------
# Pre-flight checks (warn-and-continue)
# ---------------------------------------------------------------------------

check_storage_dir() {
    [[ ! -d "${STORAGE_DIR}" ]] && return

    local owner group mode
    owner="$(stat -c '%U' "${STORAGE_DIR}")"
    group="$(stat -c '%G' "${STORAGE_DIR}")"
    mode="$(stat -c '%a' "${STORAGE_DIR}")"

    [[ "${owner}" == "${SERVICE_USER}" ]] && return

    POST_INSTALL_WARNINGS+=("")
    POST_INSTALL_WARNINGS+=("Storage directory '${STORAGE_DIR}' exists but is owned by '${owner}:${group}' (mode: ${mode}).")
    POST_INSTALL_WARNINGS+=("The service user '${SERVICE_USER}' may not have write access. To fix, choose one of:")
    POST_INSTALL_WARNINGS+=("")
    POST_INSTALL_WARNINGS+=("  1. Re-install using the existing owner as service user (no impact on data):")
    POST_INSTALL_WARNINGS+=("       sudo $0 install <binary> --service-user ${owner}")
    POST_INSTALL_WARNINGS+=("")
    POST_INSTALL_WARNINGS+=("  2. Add '${SERVICE_USER}' to the '${group}' group (requires re-login to take effect):")
    POST_INSTALL_WARNINGS+=("       sudo usermod -aG ${group} ${SERVICE_USER}")
    if command -v setfacl &>/dev/null; then
        POST_INSTALL_WARNINGS+=("")
        POST_INSTALL_WARNINGS+=("  3. Grant access via ACL (no ownership change):")
        POST_INSTALL_WARNINGS+=("       sudo setfacl -R -m u:${SERVICE_USER}:rwx '${STORAGE_DIR}'")
    fi
    POST_INSTALL_WARNINGS+=("")
    POST_INSTALL_WARNINGS+=("  4. Transfer ownership (changes owner of all existing files):")
    POST_INSTALL_WARNINGS+=("       sudo chown -R ${SERVICE_USER}:${SERVICE_USER} '${STORAGE_DIR}'")
}

check_dir_writable() {
    local dir="$1" label="$2"
    [[ ! -d "${dir}" ]] && return
    [[ -w "${dir}" ]] && return
    POST_INSTALL_WARNINGS+=("${label} '${dir}' exists but is not writable by root — check permissions manually")
}

print_post_install_warnings() {
    [[ ${#POST_INSTALL_WARNINGS[@]} -eq 0 ]] && return
    echo ""
    echo -e "${YELLOW}========== POST-INSTALL WARNINGS ==========${NC}"
    for line in "${POST_INSTALL_WARNINGS[@]}"; do
        [[ -n "${line}" ]] && warn "${line}" || echo ""
    done
    echo -e "${YELLOW}===========================================${NC}"
}

# ---------------------------------------------------------------------------
# Install helpers
# ---------------------------------------------------------------------------

create_system_user() {
    if id "${SERVICE_USER}" &>/dev/null; then
        info "System user '${SERVICE_USER}' already exists"
        return
    fi
    useradd --system --no-create-home --shell /usr/sbin/nologin "${SERVICE_USER}"
    info "Created system user '${SERVICE_USER}'"
}

prepare_dir() {
    local dir="$1" owner="$2" mode="$3" label="$4"
    if [[ -d "${dir}" ]]; then
        info "${label} '${dir}' already exists"
        return
    fi
    mkdir -p "${dir}"
    chown "${owner}" "${dir}"
    chmod "${mode}" "${dir}"
    info "Created ${label,,} '${dir}'"
}

install_binary() {
    local source="$1"
    [[ -f "${source}" ]] || { error "Binary not found: ${source}"; exit 1; }
    install -m 755 -o root -g root "${source}" "${INSTALL_DIR}/${BINARY_NAME}"
    info "Binary installed to ${INSTALL_DIR}/${BINARY_NAME}"
}

install_config() {
    local config_file="${CONFIG_DIR}/config.toml"

    if [[ -n "${CONFIG_SOURCE}" ]]; then
        if [[ -f "${config_file}" ]]; then
            echo -n "Config already exists at '${config_file}'. Replace with '${CONFIG_SOURCE}'? [y/N] "
            read -r confirm
            if [[ "${confirm,,}" != "y" ]]; then
                info "Keeping existing config at '${config_file}'"
                return
            fi
        fi
        install -m 640 -o root -g "${SERVICE_USER}" "${CONFIG_SOURCE}" "${config_file}"
        info "Config copied from '${CONFIG_SOURCE}' to '${config_file}'"
        return
    fi

    if [[ -f "${config_file}" ]]; then
        info "Config already exists at '${config_file}' — not overwriting"
        return
    fi

    if [[ -f "${CONFIG_EXAMPLE}" ]]; then
        install -m 640 -o root -g "${SERVICE_USER}" "${CONFIG_EXAMPLE}" "${config_file}"
        info "Config created from example at '${config_file}' — review before starting the service"
    else
        warn "No config source found. Create '${config_file}' manually before starting the service."
    fi
}

install_service() {
    [[ -f "${SERVICE_TEMPLATE}" ]] || { error "Service template not found: ${SERVICE_TEMPLATE}"; exit 1; }

    local service_file="/etc/systemd/system/${SERVICE_NAME}.service"
    sed \
        -e "s|@@SERVICE_USER@@|${SERVICE_USER}|g" \
        -e "s|@@BINARY_PATH@@|${INSTALL_DIR}/${BINARY_NAME}|g" \
        -e "s|@@STORAGE_DIR@@|${STORAGE_DIR}|g" \
        -e "s|@@SERVICE_NAME@@|${SERVICE_NAME}|g" \
        "${SERVICE_TEMPLATE}" > "${service_file}"
    chmod 644 "${service_file}"
    systemctl daemon-reload
    systemctl enable "${SERVICE_NAME}"
    info "Service '${SERVICE_NAME}' installed and enabled"
}

health_check() {
    read_config_values "${CONFIG_DIR}/config.toml"
    local url="http://localhost:${API_PORT}/health"
    info "Waiting for service to become healthy..."
    local attempt=0
    while [[ "${attempt}" -lt "${HEALTH_CHECK_ATTEMPTS}" ]]; do
        curl -sf "${url}" &>/dev/null && { info "Health check passed"; return 0; }
        attempt=$(( attempt + 1 ))
        sleep "${HEALTH_CHECK_INTERVAL}"
    done
    warn "Service did not pass health check — check logs: journalctl -u ${SERVICE_NAME}"
    return 1
}

# ---------------------------------------------------------------------------
# Subcommands
# ---------------------------------------------------------------------------

cmd_install() {
    local binary="${1:-}"
    shift || true
    parse_flags "$@"

    [[ -n "${binary}" ]] || { error "Usage: $0 install <path-to-binary> [options]"; exit 1; }
    [[ -f "${binary}" ]]  || { error "Binary not found: ${binary}"; exit 1; }

    require_root

    # Fatal checks — run before touching anything
    check_config_source
    check_service_name
    check_ollama

    info "Installing Photometoria..."
    create_system_user
    prepare_dir "${INSTALL_DIR}"  "root:root" "755" "Install directory"
    prepare_dir "${CONFIG_DIR}"   "root:root" "755" "Config directory"
    prepare_dir "${BACKUP_DIR}"   "root:root" "755" "Backup directory"
    install_binary "${binary}"
    install_config

    # Read effective values from the installed config (binary + config now in place)
    read_config_values "${CONFIG_DIR}/config.toml"

    # Warn-and-continue checks (STORAGE_DIR now reflects the installed config)
    check_storage_dir
    check_dir_writable "${BACKUP_DIR}" "Backup directory"

    prepare_dir "${STORAGE_DIR}"  "${SERVICE_USER}:${SERVICE_USER}" "750" "Storage directory"
    install_service
    systemctl start "${SERVICE_NAME}"
    health_check || true

    print_post_install_warnings

    info "Installation complete!"
    info "  Logs:   journalctl -u ${SERVICE_NAME} -f"
    info "  Config: ${CONFIG_DIR}/config.toml"
    info "  Status: $0 --status"
}

cmd_update() {
    local binary="${1:-}"
    shift || true
    parse_flags "$@"

    [[ -n "${binary}" ]] || { error "Usage: $0 update <path-to-binary> [options]"; exit 1; }
    [[ -f "${binary}" ]]  || { error "Binary not found: ${binary}"; exit 1; }

    require_root
    info "Updating Photometoria..."

    systemctl stop "${SERVICE_NAME}" || true
    info "Service stopped"

    if [[ -f "${INSTALL_DIR}/${BINARY_NAME}" ]]; then
        local backup_path="${BACKUP_DIR}/${BINARY_NAME}.$(date +%Y%m%d_%H%M%S)"
        cp "${INSTALL_DIR}/${BINARY_NAME}" "${backup_path}"
        info "Binary backed up to ${backup_path}"
    fi

    install_binary "${binary}"
    systemctl start "${SERVICE_NAME}"
    info "Service restarted"

    if ! health_check; then
        warn "Health check failed — rolling back"
        local latest_backup
        latest_backup="$(ls -t "${BACKUP_DIR}/${BINARY_NAME}."* 2>/dev/null | head -1 || true)"
        if [[ -n "${latest_backup}" ]]; then
            install -m 755 -o root -g root "${latest_backup}" "${INSTALL_DIR}/${BINARY_NAME}"
            systemctl restart "${SERVICE_NAME}"
            warn "Rolled back to ${latest_backup}"
        fi
        exit 1
    fi

    info "Update complete!"
}

cmd_configure() {
    parse_flags "$@"
    require_root

    local config_file="${CONFIG_DIR}/config.toml"
    [[ -f "${config_file}" ]] || { error "Config file not found at ${config_file}. Run 'install' first."; exit 1; }

    local editor="${EDITOR:-vi}"
    info "Opening ${config_file} in ${editor}..."
    "${editor}" "${config_file}"

    if "${INSTALL_DIR}/${BINARY_NAME}" config check --config "${config_file}"; then
        info "Configuration is valid — restarting service"
        systemctl restart "${SERVICE_NAME}"
        info "Service restarted"
    else
        error "Configuration is invalid — service not restarted"
        exit 1
    fi
}

cmd_uninstall() {
    parse_flags "$@"
    require_root
    read_config_values "${CONFIG_DIR}/config.toml"

    echo -n "This will remove the '${SERVICE_NAME}' service and binary. Continue? [y/N] "
    read -r confirm
    [[ "${confirm,,}" == "y" ]] || { info "Uninstall cancelled"; exit 0; }

    info "Uninstalling Photometoria..."
    systemctl stop "${SERVICE_NAME}" 2>/dev/null || true
    systemctl disable "${SERVICE_NAME}" 2>/dev/null || true
    rm -f "/etc/systemd/system/${SERVICE_NAME}.service"
    systemctl daemon-reload
    info "Service removed"

    rm -f "${INSTALL_DIR}/${BINARY_NAME}"
    info "Binary removed"

    warn "Data preserved — remove manually if no longer needed:"
    warn "  Config:  ${CONFIG_DIR}"
    warn "  Storage: ${STORAGE_DIR}"
    warn "  Backups: ${BACKUP_DIR}"
    info "Uninstall complete"
}

cmd_status() {
    parse_flags "$@"

    echo "=== Service Status ==="
    systemctl status "${SERVICE_NAME}" --no-pager 2>/dev/null || echo "(service not found)"

    echo ""
    echo "=== API Health ==="
    read_config_values "${CONFIG_DIR}/config.toml"
    if curl -sf "http://localhost:${API_PORT}/health"; then
        echo ""
    else
        echo "API not reachable at http://localhost:${API_PORT}/health"
    fi

    if [[ -f "${INSTALL_DIR}/${BINARY_NAME}" ]]; then
        echo ""
        echo "=== Binary ==="
        "${INSTALL_DIR}/${BINARY_NAME}" --version 2>/dev/null || true
    fi
}

# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

usage() {
    cat <<EOF
Usage: $0 <command> [options]

Commands:
  install <binary>   Install Photometoria as a systemd service
  update  <binary>   Update the binary (preserves config, rolls back on failure)
  configure          Edit config.toml interactively and restart the service
  --status           Show service state and API health
  --uninstall        Remove the service and binary (data is preserved)

Options (applicable to all commands):
  --config       <file>  Existing config.toml to copy on install (must exist; error if not found)
  --service-user <user>  System user to run the service as      (default: photometoria)
  --service-name <name>  systemd service unit name              (default: photometoria)
  --install-dir  <dir>   Directory where the binary is placed   (default: /usr/local/bin)
  --backup-dir   <dir>   Binary backup directory for updates    (default: /var/backups/photometoria)

Storage directory and API port are read from config.toml via 'photometoria config get'.

Environment variables (lowest precedence, overridden by flags):
  PHOTOMETORIA_CONFIG, PHOTOMETORIA_SERVICE_USER, PHOTOMETORIA_SERVICE_NAME,
  PHOTOMETORIA_INSTALL_DIR, PHOTOMETORIA_BACKUP_DIR

Examples:
  sudo $0 install ./target/release/photometoria
  sudo $0 install ./target/release/photometoria --config ~/photometoria.toml
  sudo $0 install ./target/release/photometoria --service-user fabrizio
  sudo $0 update  ./target/release/photometoria
  sudo $0 configure
  $0 --status
  sudo $0 --uninstall
EOF
}

case "${1:-}" in
    install)     cmd_install    "${@:2}" ;;
    update)      cmd_update     "${@:2}" ;;
    configure)   cmd_configure  "${@:2}" ;;
    --uninstall) cmd_uninstall  "${@:2}" ;;
    --status)    cmd_status     "${@:2}" ;;
    *)           usage; exit 1 ;;
esac
