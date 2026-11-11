#!/bin/bash
# Soumet un fichier (zip ou DMG) au service de notarisation d'Apple et attend
# le verdict (ADR-0029). Appelé par le job build-macos de desktop-build.yml,
# seulement quand les secrets de signature sont posés.
#
# Usage : notarize.sh <fichier>
# Environnement : APPLE_API_KEY_P8 (contenu du fichier AuthKey_<id>.p8 d'une
# clé d'équipe App Store Connect), APPLE_API_KEY_ID, APPLE_API_ISSUER_ID.
#
# Échoue si le statut n'est pas « Accepted », après avoir imprimé le journal
# d'Apple (liste des fichiers refusés et raison). La clé n'est écrite que le
# temps de l'appel, dans le dossier temporaire du runner.
set -euo pipefail

file="${1:?usage : notarize.sh <fichier>}"
: "${APPLE_API_KEY_P8:?APPLE_API_KEY_P8 absent}"
: "${APPLE_API_KEY_ID:?APPLE_API_KEY_ID absent}"
: "${APPLE_API_ISSUER_ID:?APPLE_API_ISSUER_ID absent}"

tmp="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
key="$tmp/AuthKey_$APPLE_API_KEY_ID.p8"
result="$tmp/notarytool-result.json"
trap 'rm -f "$key" "$result"' EXIT
(umask 077 && printf '%s\n' "$APPLE_API_KEY_P8" > "$key")
auth=(--key "$key" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER_ID")

echo "Notarisation de ${file}…"
# notarytool peut sortir en succès avec un statut « Invalid » : le verdict se
# lit dans le JSON, pas dans le code de sortie.
xcrun notarytool submit "$file" "${auth[@]}" --wait --timeout 30m --output-format json > "$result" || true
id="$(plutil -extract id raw -o - "$result" 2>/dev/null || true)"
status="$(plutil -extract status raw -o - "$result" 2>/dev/null || true)"
echo "Soumission ${id:-inconnue} : ${status:-sans réponse}"

if [ "$status" != "Accepted" ]; then
  echo "::error::Notarisation de $file refusée ou sans réponse (statut : ${status:-inconnu})"
  if [ -n "$id" ]; then
    xcrun notarytool log "$id" "${auth[@]}" || true
  fi
  exit 1
fi
