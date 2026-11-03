#!/usr/bin/env bash
# Build separado da instalação por usuário. Nada é publicado ou instalado aqui.
set -euo pipefail
task_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
task_binary=''
task_output="$task_root/dist"
task_tools="$task_root/target/appimage-tools"
task_appdir_only=0
while (($#)); do
    case "$1" in
        --binary|--output-dir|--tools-dir)
            (($#>=2)) || { echo 'Falta o caminho.' >&2; exit 2; }
            case "$1" in --binary) task_binary=$2;; --output-dir) task_output=$2;; --tools-dir) task_tools=$2;; esac
            shift;;
        --appdir-only) task_appdir_only=1;;
        --help) echo 'Uso: bash scripts/pelicool-appimage.sh [--binary BIN] [--appdir-only] [--output-dir DIR] [--tools-dir DIR]'; exit 0;;
        *) echo "Opção desconhecida: $1" >&2; exit 2;;
    esac
    shift
done
[[ $(uname -m) == x86_64 ]] || { echo 'O empacotamento validado hoje é x86_64. Outras arquiteturas precisam de ferramentas e CI próprios.' >&2; exit 1; }
if [[ -z "$task_binary" ]]; then
    cargo run --manifest-path "$task_root/Cargo.toml" --locked --quiet -p pelicool-xtask -- release
    task_binary="$task_root/target/release/pelicool"
fi
[[ -x "$task_binary" ]] || { echo 'Executável pelicool não encontrado.' >&2; exit 1; }
if [[ -n "${HOME:-}" ]] && LC_ALL=C grep -aqF -- "${HOME%/}/" "$task_binary"; then
    echo 'O binário contém caminhos da conta usada no build. Gere com cargo run --locked -p pelicool-xtask -- release antes de empacotar.' >&2
    exit 1
fi
mkdir -p -- "$task_output"
task_output="$(cd -- "$task_output" && pwd)"
task_appdir="$task_output/PeliCool.AppDir"
[[ ! -e "$task_appdir" ]] || { echo "Já existe $task_appdir. Use uma saída nova para não sobrescrever arquivos." >&2; exit 1; }
mkdir -p -- "$task_appdir/usr/bin" "$task_appdir/usr/share/pelicool/licenses" "$task_appdir/usr/share/pelicool/scripts" "$task_appdir/usr/share/pelicool/udev"
install -m755 -- "$task_binary" "$task_appdir/usr/bin/pelicool"
install -m755 -- "$task_root/packaging/appimage/AppRun" "$task_appdir/AppRun"
install -m644 -- "$task_root/packaging/appimage/pelicool.desktop" "$task_appdir/pelicool.desktop"
install -m644 -- "$task_root/assets/pelicool-icon.svg" "$task_appdir/pelicool.svg"
cp -- "$task_root/LICENSE" "$task_root/THIRD-PARTY-NOTICES.md" "$task_root/LICENSES/gpteixeira-MIT.txt" "$task_appdir/usr/share/pelicool/licenses/"
cp -- "$task_root/scripts/pelicool-udev.sh" "$task_root/scripts/pelicool-diagnose.sh" "$task_appdir/usr/share/pelicool/scripts/"
cp -- "$task_root/udev/70-pelicool.rules" "$task_appdir/usr/share/pelicool/udev/"
cargo run --manifest-path "$task_root/Cargo.toml" --locked --quiet -p pelicool-xtask -- licenses "$task_appdir/usr/share/pelicool/licenses"
ln -s pelicool.svg "$task_appdir/.DirIcon"
desktop-file-validate "$task_appdir/pelicool.desktop"
if ((task_appdir_only)); then echo "AppDir: $task_appdir"; exit 0; fi
mkdir -p -- "$task_tools"
task_tools="$(cd -- "$task_tools" && pwd)"
while read -r task_name task_sha task_url; do
    [[ -z "$task_name" || "$task_name" == \#* ]] && continue
    task_file="$task_tools/$task_name"
    if [[ ! -f "$task_file" ]]; then
        echo "Baixando ferramenta oficial: $task_name"
        curl --fail --location --proto '=https' --tlsv1.2 --output "$task_file" "$task_url"
    fi
    task_actual="$(sha256sum -- "$task_file")"; task_actual="${task_actual%% *}"
    [[ "$task_actual" == "$task_sha" ]] || { echo "SHA-256 divergente para $task_name. Nada será executado. Consulte packaging/appimage/tools-x86_64.lock." >&2; exit 1; }
    chmod +x -- "$task_file"
    if [[ "$task_name" == *.AppImage ]]; then
        task_extract="$task_tools/${task_name%.AppImage}-${task_sha:0:12}"
        if [[ ! -x "$task_extract/squashfs-root/AppRun" ]]; then
            mkdir -p -- "$task_extract"
            (cd -- "$task_extract" && "$task_file" --appimage-extract >/dev/null)
        fi
        case "$task_name" in linuxdeploy*) task_deployer="$task_extract/squashfs-root/AppRun";; appimagetool*) task_packager="$task_extract/squashfs-root/AppRun";; esac
    fi
done < "$task_root/packaging/appimage/tools-x86_64.lock"
# winit abre estas bibliotecas com dlopen; elas não aparecem no ldd do binário.
task_gui_libraries=()
for task_library_name in libxkbcommon-x11.so.0 libxkbcommon.so.0 libxcb-xkb.so.1; do
    task_library_path=$(ldconfig -p | awk -v name="$task_library_name" '$1 == name && !found {print $NF; found=1}')
    [[ -f "$task_library_path" ]] || { echo "Biblioteca necessária para empacotar: $task_library_name" >&2; exit 1; }
    task_gui_libraries+=(--library "$task_library_path")
done
# O perfil release já remove símbolos. Preserve bibliotecas do host, inclusive RELR.
NO_STRIP=1 "$task_deployer" --appdir "$task_appdir" --executable "$task_appdir/usr/bin/pelicool" --desktop-file "$task_appdir/pelicool.desktop" --icon-file "$task_appdir/pelicool.svg" "${task_gui_libraries[@]}"
# Mantém nosso launcher; nenhuma configuração é gravada dentro do AppDir.
install -m755 -- "$task_root/packaging/appimage/AppRun" "$task_appdir/AppRun"
ARCH=x86_64 "$task_packager" --runtime-file "$task_tools/runtime-x86_64" --no-appstream "$task_appdir" "$task_output/PeliCool.AppImage"
