#!/usr/bin/env bash
# Demo: reuso do gateway-api — domínio "ufc-certificados" (Certificados acadêmicos).
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

# Hash determinístico por execução (payload inclui timestamp para evitar 409 em reexecuções).
TIMESTAMP="$(date +%s)"
PAYLOAD="ufc-certificados|certificado-extensao|aluno:josias-batista|$TIMESTAMP"
DOC_HASH="$(printf '%s' "$PAYLOAD" | sha256sum | awk '{print $1}')"
DOMAIN="ufc-certificados"

echo "=== Domínio: $DOMAIN ==="
echo "Payload:   $PAYLOAD"
echo "DOC_HASH:  $DOC_HASH"
echo "ISSUER:    $ISSUER"
echo

# 1. Registro
echo "[1/4] Registrando atestação..."
HTTP_STATUS="$(curl -sS -o /tmp/opencode/ufc_reg.json -w '%{http_code}' \
  -X POST "$BASE_URL/api/v1/attestations" \
  -H 'Content-Type: application/json' \
  -d "{\"issuer\":\"$ISSUER\",\"doc_hash\":\"$DOC_HASH\",\"domain\":\"$DOMAIN\"}")"
echo "HTTP $HTTP_STATUS"
if [[ "$HTTP_STATUS" != "201" ]]; then
  echo "Resposta:"; cat /tmp/opencode/ufc_reg.json; echo; exit 1
fi
echo "OK"
echo

# 2. Verificação positiva
echo "[2/4] Verificando (espera revoked=false)..."
HTTP_STATUS="$(curl -sS -o /tmp/opencode/ufc_verify1.json -w '%{http_code}' \
  "$BASE_URL/api/v1/verify/$DOC_HASH")"
echo "HTTP $HTTP_STATUS"
if [[ "$HTTP_STATUS" != "200" ]]; then
  cat /tmp/opencode/ufc_verify1.json; echo; exit 1
fi
jq -e '.revoked == false and .domain == "ufc-certificados"' /tmp/opencode/ufc_verify1.json >/dev/null
echo "Revogado? $(jq -r '.revoked' /tmp/opencode/ufc_verify1.json)"
echo "Timestamp: $(jq -r '.timestamp' /tmp/opencode/ufc_verify1.json)"
echo "OK"
echo

# 3. Revogação
echo "[3/4] Revogando atestação..."
HTTP_STATUS="$(curl -sS -o /tmp/opencode/ufc_revoke.json -w '%{http_code}' \
  -X POST "$BASE_URL/api/v1/attestations/$DOC_HASH/revoke" \
  -H 'Content-Type: application/json' \
  -d "{\"issuer\":\"$ISSUER\"}")"
echo "HTTP $HTTP_STATUS"
if [[ "$HTTP_STATUS" != "200" ]]; then
  cat /tmp/opencode/ufc_revoke.json; echo; exit 1
fi
echo "OK"
echo

# 4. Verificação revogada
echo "[4/4] Verificando (espera revoked=true)..."
HTTP_STATUS="$(curl -sS -o /tmp/opencode/ufc_verify2.json -w '%{http_code}' \
  "$BASE_URL/api/v1/verify/$DOC_HASH")"
echo "HTTP $HTTP_STATUS"
if [[ "$HTTP_STATUS" != "200" ]]; then
  cat /tmp/opencode/ufc_verify2.json; echo; exit 1
fi
jq -e '.revoked == true' /tmp/opencode/ufc_verify2.json >/dev/null
echo "Revogado? $(jq -r '.revoked' /tmp/opencode/ufc_verify2.json)"
echo "OK"
echo

echo "=== Cenário UFC concluído com sucesso ==="