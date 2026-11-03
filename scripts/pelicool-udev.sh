#!/usr/bin/env bash
set -euo pipefail

if [[ "$EUID" -ne 0 ]]; then
    echo 'Execute com sudo: sudo bash scripts/pelicool-udev.sh' >&2
    exit 1
fi

if [[ -n "${PELICOOL_UDEV_RULE:-}" ]]; then
    task_source="$PELICOOL_UDEV_RULE"
else
    task_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
    task_source="$task_root/udev/70-pelicool.rules"
fi
task_destination='/etc/udev/rules.d/70-pelicool.rules'

task_action="${1:-install}"
if [[ "$task_action" == remove ]]; then
    for task_rule_file in "$task_destination" '/etc/udev/rules.d/70-deepcool-linux.rules'; do
        if [[ -e "$task_rule_file" ]]; then
            cmp -s -- "$task_source" "$task_rule_file" || { echo "Regra modificada, preservada: $task_rule_file" >&2; exit 1; }
            rm -- "$task_rule_file"
        fi
    done
    udevadm control --reload-rules
    echo 'Regra PeliCool removida. Reconecte o USB para renovar permissões.'
    exit 0
fi
[[ "$task_action" == install ]] || { echo 'Use install ou remove.' >&2; exit 2; }
if [[ -e "$task_destination" ]] && ! cmp -s -- "$task_source" "$task_destination"; then
    echo "Já existe uma regra diferente em $task_destination. Revise-a antes de substituir." >&2
    exit 1
fi

install -m 0644 -- "$task_source" "$task_destination"
task_legacy='/etc/udev/rules.d/70-deepcool-linux.rules'
if [[ -e "$task_legacy" ]] && cmp -s -- "$task_source" "$task_legacy"; then rm -- "$task_legacy"; fi
udevadm control --reload-rules
task_vendor_pattern='ATTR\{idVendor\}=="([[:xdigit:]]{4})"'
task_product_pattern='ATTR\{idProduct\}=="([[:xdigit:]]{4})"'
while IFS= read -r task_rule; do
    [[ "$task_rule" =~ ^[[:space:]]*# ]] && continue
    if [[ "$task_rule" =~ $task_vendor_pattern ]]; then
        task_vendor="${BASH_REMATCH[1]}"
    else
        continue
    fi
    if [[ "$task_rule" =~ $task_product_pattern ]]; then
        task_product="${BASH_REMATCH[1]}"
    else
        continue
    fi
    udevadm trigger --action=add --subsystem-match=usb --attr-match="idVendor=$task_vendor" --attr-match="idProduct=$task_product"
done < "$task_source"
udevadm settle
echo 'Regras USB dos módulos instaladas. Execute o PeliCool como usuário normal.'
