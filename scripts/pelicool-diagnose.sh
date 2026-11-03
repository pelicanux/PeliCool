#!/bin/sh
# Inventário local e somente leitura. Nenhum comando é enviado ao dispositivo.
set -eu
umask 077
consent=0
allow_root=0
raw=0
output=''
usb_root=/sys/bus/usb/devices
hwmon_root=/sys/class/hwmon
usage() {
    cat <<'HELP'
PeliCool — diagnóstico voluntário para investigar novos displays
Uso: sh pelicool-diagnose.sh [--consent] [--output NOVA_PASTA]
     [--include-descriptors] [--allow-root]

Coleta: distribuição, kernel, arquitetura, sessão gráfica, metadados USB de
aparelhos DeepCool, interfaces/endpoints e nomes de fontes de sensores.
Por padrão não coleta hostname, usuário, IP, MAC, números de série, arquivos pessoais,
imagens/GIFs ou logs. Não altera USB, LEDs, tela, bomba, fans ou permissões.
Não usa rede. Nada é enviado ao projeto: revise a pasta e compartilhe-a
manualmente, se desejar. Os dados não bastam para descobrir o protocolo.

--consent: confirma esta coleta, inclusive em execução não interativa.
--include-descriptors: inclui descritores USB binários em hexadecimal;
  podem conter metadados/identificadores do fabricante. Revise antes de compartilhar.
--allow-root: permite execução manual como root; normalmente desnecessário.
--sysfs-root e --hwmon-root: raízes alternativas para testes com fixtures.
HELP
}
while [ "$#" -gt 0 ]; do
    case "$1" in
        --consent) consent=1;;
        --allow-root) allow_root=1;;
        --include-descriptors) raw=1;;
        --output|--sysfs-root|--hwmon-root)
            [ "$#" -ge 2 ] || { echo 'Falta o caminho da opção.' >&2; exit 2; }
            option=$1; shift
            case "$option" in
                --output) output=$1;;
                --sysfs-root) usb_root=$1;;
                --hwmon-root) hwmon_root=$1;;
            esac;;
        --help|-h) usage; exit 0;;
        *) echo "Opção desconhecida: $1" >&2; exit 2;;
    esac
    shift
done
usage
if [ "$consent" -ne 1 ]; then
    [ -t 0 ] || { echo 'Coleta cancelada: falta consentimento (--consent).' >&2; exit 2; }
    printf '\nDigite COLETAR para autorizar: '
    IFS= read -r answer
    [ "$answer" = COLETAR ] || { echo 'Coleta cancelada.'; exit 0; }
fi
if [ "$(id -u)" = 0 ] && [ "$allow_root" -ne 1 ]; then
    echo 'Execute como usuário normal. Para autorizar root, use --allow-root.' >&2
    exit 2
fi
if [ -z "$output" ]; then
    case "${XDG_CONFIG_HOME:-}" in /*) config_base=$XDG_CONFIG_HOME;; *) config_base=${HOME:?}/.config;; esac
    mkdir -p "$config_base/PeliCool/diagnostics"
    output=$(mktemp -d "$config_base/PeliCool/diagnostics/report-XXXXXXXX")
else
    [ ! -e "$output" ] || { echo 'A pasta de saída já existe; nada foi sobrescrito.' >&2; exit 2; }
    mkdir -p "$(dirname -- "$output")"
    mkdir -m 700 -- "$output"
fi
mkdir "$output/usb"
read_attr() {
    # Só atributos expressamente listados; nunca serial, uevent ou ambiente completo.
    if [ -r "$1" ]; then
        LC_ALL=C head -c 4096 -- "$1" | LC_ALL=C tr -cd '[:print:]\n' || true
    fi
}
{
    printf 'PeliCool diagnostic schema: 1\n'
    printf 'Access: cached sysfs only; no device opened; no USB transfers\n'
    printf 'Kernel: '; uname -r
    printf 'Architecture: '; uname -m
    case "${XDG_SESSION_TYPE:-}" in x11|wayland) printf 'Session: %s\n' "$XDG_SESSION_TYPE";; *) printf 'Session: unknown\n';; esac
    if [ -r /etc/os-release ]; then sed -n '/^ID=/p; /^VERSION_ID=/p' /etc/os-release; fi
} > "$output/system.txt"
count=0
for device in "$usb_root"/*; do
    [ -r "$device/idVendor" ] && [ -r "$device/idProduct" ] || continue
    vendor=$(read_attr "$device/idVendor")
    maker=$(read_attr "$device/manufacturer")
    case "$vendor:$maker" in
        3633:*|*:[Dd][Ee][Ee][Pp][Cc][Oo][Oo][Ll]*|*:*[Dd][Ee][Ee][Pp][Cc][Oo][Oo][Ll]*) ;;
        *) continue;;
    esac
    count=$((count + 1))
    report="$output/usb/device-$count.txt"
    {
        for attr in idVendor idProduct manufacturer product bcdDevice bDeviceClass bDeviceSubClass bDeviceProtocol bNumConfigurations speed version; do
            printf '%s: %s\n' "$attr" "$(read_attr "$device/$attr")"
        done
        for interface in "$usb_root/$(basename -- "$device")":*; do
            [ -d "$interface" ] || continue
            printf '\n[interface]\n'
            for attr in bInterfaceNumber bAlternateSetting bInterfaceClass bInterfaceSubClass bInterfaceProtocol bNumEndpoints; do
                printf '%s: %s\n' "$attr" "$(read_attr "$interface/$attr")"
            done
            if [ -L "$interface/driver" ]; then printf 'driver: %s\n' "$(basename -- "$(readlink -- "$interface/driver")")"; fi
            for endpoint in "$interface"/ep_*; do
                [ -d "$endpoint" ] || continue
                printf '\n[endpoint]\n'
                for attr in bEndpointAddress bmAttributes wMaxPacketSize bInterval direction type interval; do
                    printf '%s: %s\n' "$attr" "$(read_attr "$endpoint/$attr")"
                done
            done
        done
    } > "$report"
    if [ "$raw" -eq 1 ]; then
        if [ -r "$device/descriptors" ]; then
            head -c 65536 -- "$device/descriptors" | od -Ax -tx1 -v > "$output/usb/device-$count-descriptors.hex"
        else
            printf 'Descritores não legíveis sem privilégios adicionais.\n' >> "$report"
        fi
    fi
done
{
    for sensor in "$hwmon_root"/hwmon*; do
        [ -d "$sensor" ] || continue
        printf '\nDriver: %s\n' "$(read_attr "$sensor/name")"
        for input in "$sensor"/temp*_input "$sensor"/fan*_input; do
            [ -r "$input" ] && basename -- "$input"
        done
    done
    :
} > "$output/sensors.txt"
cat > "$output/README.txt" <<'NOTICE'
Relatório voluntário do PeliCool. Revise TODOS os arquivos antes de compartilhar.
Nada foi enviado automaticamente. A captura do protocolo de comunicação do
software de referência é uma etapa separada e exige autorização específica.
Não publique imagens/GIFs pessoais, números de série ou dados de outras pessoas.
Estes metadados ajudam a identificar o aparelho e planejar um driver, mas não
comprovam compatibilidade e não habilitam comandos experimentais no hardware.
NOTICE
printf '\nDispositivos candidatos: %s\nRelatório local: %s\n' "$count" "$output"
