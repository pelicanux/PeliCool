#!/bin/sh
# Execute sem sudo. --purge solicita também a remoção de dados e configurações.
set -eu
case "${XDG_DATA_HOME:-}" in /*) data_base=$XDG_DATA_HOME;; *) data_base=${HOME:?}/.local/share;; esac
image="$data_base/PeliCool/bin/PeliCool.AppImage"
[ -x "$image" ] || { echo 'PeliCool não está instalado nesse caminho. Use o AppImage original com uninstall.' >&2; exit 1; }
exec "$image" uninstall "$@"
