#!/usr/bin/env bash
# Demo: reuso do gateway-api — domínio "logistica-reversa" (Rastreabilidade de lotes).
# Fluxo: registro -> verificação positiva -> revogação -> verificação revogada.

set -euo pipefail

BASE_URL="${BASE_URL:-http://localhost:8080}"
ISSUER="${ISSUER:-${STELLAR_SOURCE_ACCOUNT:-}}"

if [[ -z "$ISSUER" ]]; then
  echo "ERRO: defina ISSUER (ou STELLAR_SOURCE_ACCOUNT) apontando para a conta que o gateway pode assinar." >&2
  exit 1
fi

command -v curl >/dev/null || { echo "ERRO: curl não encontrado." >&2; exit 1; }
command -v jq   >/dev/null || { echo "ERRO: jq não encontrado." >&2; exit 1; }

TIMESTAMP="$(date +%s)"
PAYLOAD="logistica-reversa|lote-residuos|lote:LR-2026-0042|peso:1250kg|$TIMESTAMP"
DOC_HASH="$(printf '%s' "$PAYLOAD" | sha256sum | awk '{print $1}')"
DOMAIN="logistica-reversa"

echo "=== Domínio: $DOMAIN ==="
echo "Payload:   $PAYLOAD"
echo "DOC_HASH:  $DOC_HASH"
echo "ISSUER:    $ISSUER"
echo

echo "[1/4] Registrando atestação..."
HTTP_STATUS="$(curl -sS -o /tmp/opencode/lr_reg.json -w '%{http_code}' \
  -X POST "$BASE_URL/api/v1/attestations" \
  -H 'Content-Type: application/json' \
  -d "{\"issuer\":\"$ISSUER\",\"doc_hash\":\"$DOC_HASH\",\"domain\":\"$DOMAIN\"}")"
echo "HTTP $HTTP_STATUS"
if [[ "$HTTP_STATUS" != "201" ]]; then
  cat /tmp/opencode/lr_reg.json; echo; exit 1
fi
echo "OK"
echo

echo "[2/4] Verificando (espera revoked=false)..."
HTTP_STATUS="$(curl -sS -o /tmp/opencode/lr_verify1.json -w '%{http_code}' \
  "$BASE_URL/api/v1/verify/$DOC_HASH")"
echo "HTTP $HTTP_STATUS"
if [[ "$HTTP_STATUS" != "200" ]]; then
  cat /tmp/opencode/lr_verify1.json; echo; exit 1
fi
jq -e '.revoked == false and .domain == "logistica-reversa"' /tmp/opencode/lr_verify1.json >/dev/null
echo "Revogado? $(jq -r '.revoked' /tmp/opencode/lr_verify1.json)"
echo "OK"
echo

echo "[3/4] Revogando atestação..."
HTTP_STATUS="$(curl -sS -o /tmp/opencode/lr_revoke.json -w '%{http_code}' \
  -X POST "$BASE_URL/api/v1/attestations/$DOC_HASH/revoke" \
  -H 'Content-Type: application/json' \
  -d "{\"issuer\":\"$ISSUER\"}")"
echo "HTTP $HTTP_STATUS"
if [[ "$HTTP_STATUS" != "200" ]]; then
  cat /tmp/opencode/lr_revoke.json; echo; exit 1
fi
echo "OK"
echo

echo "[4/4] Verificando (espera revoked=true)..."
HTTP_STATUS="$(curl -sS -o /tmp/opencode/lr_verify2.json -w '%{http_code}' \
  "$BASE_URL/api/v1/verify/$DOC_HASH")"
echo "HTTP $HTTP_STATUS"
if [[ "$HTTP_STATUS" != "200" ]]; then
  cat /tmp/opencode/lr_verify2.json; echo; exit 1
fi
jq -e '.revoked == true' /tmp/opencode/lr_verify2.json >/dev/null
echo "Revogado? $(jq -r '.revoked' /tmp/opencode/lr_verify2.json)"
echo "OK"
echo

echo "=== Cenário Logística Reversa concluído com sucesso ==="