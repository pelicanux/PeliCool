#!/bin/sh
# A lógica de instalação fica em src/installation; este arquivo só encaminha argumentos.
set -eu
[ "$#" -ge 1 ] || { echo 'Uso: sh pelicool-install.sh /caminho/PeliCool.AppImage' >&2; exit 2; }
image=$1; shift
case "$image" in /*) ;; *) image="$(pwd)/$image";; esac
exec "$image" install --source "$image" "$@"
