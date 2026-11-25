# Checklist de Validação de Reuso do gateway-api

Objetivo: evidenciar que a mesma instância do `gateway-api` (e do contrato `stellartrust`) atende a dois domínios independentes sem alteração de código.

## 1. Pré-requisitos

- [ ] Rust toolchain + target `wasm32v1-none` instalados.
- [ ] Stellar CLI instalado (`stellar --version`).
- [ ] Gateway configurado a partir de `gateway-api/.env.example`:
  - [ ] `STELLAR_RPC_URL` (ex.: `https://soroban-testnet.stellar.org`)
  - [ ] `STELLAR_NETWORK_PASSPHRASE` (ex.: `Test SDF Network ; September 2015`)
  - [ ] `STELLAR_CONTRACT_ID` (contrato `stellartrust` já deployado)
  - [ ] `STELLAR_SOURCE_ACCOUNT` (conta/alias que o gateway consegue assinar)
- [ ] Dependências locais: `curl`, `jq`, `sha256sum`.
- [ ] Gateway em execução: `cargo run -p gateway-api` (ou binário equivalente).
- [ ] Health OK: `curl -s http://localhost:8080/health` retorna `status: "ok"`.

## 2. Execução dos cenários

### Cenário 1 — UFC (Certificados)

```bash
export BASE_URL=http://localhost:8080
export ISSUER="$STELLAR_SOURCE_ACCOUNT"   # deve ser assinável pelo gateway
bash examples/demo_ufc_certificados.sh
```

### Cenário 2 — Logística Reversa (Rastreabilidade)

```bash
export BASE_URL=http://localhost:8080
export ISSUER="$STELLAR_SOURCE_ACCOUNT"
bash examples/demo_logistica_reversa.sh
```

Ambos devem terminar com `=== ... concluído com sucesso ===`.

## 3. Critérios de aceite (por cenário)

- [ ] Registro retorna HTTP `201`.
- [ ] Primeira verificação retorna HTTP `200` com `revoked == false` e `domain` igual ao domínio do cenário.
- [ ] Revogação retorna HTTP `200`.
- [ ] Segunda verificação retorna HTTP `200` com `revoked == true`.
- [ ] `doc_hash` possui exatamente 64 caracteres hexadecimais.
- [ ] `issuer`, `doc_hash`, `domain` e `timestamp` presentes na resposta de verificação.

## 4. Critérios de reuso / isolamento

- [ ] Os dois cenários usam o mesmo `BASE_URL` e a mesma instância do gateway.
- [ ] Nenhum código de `gateway-api/` ou `contracts/` foi alterado para suportar um domínio específico.
- [ ] Os domínios (`UFC-QUIXADA-EXTENSAO` e `RASTREABILIDADE-RESIDUOS`) coexistem sem colisão: hashes distintos ⇒ registros independentes.
- [ ] Reexecutar um script com o mesmo payload resulta em `409 AlreadyRegistered` (idempotência por `doc_hash`), documentado como comportamento esperado.
- [ ] A revogação de um domínio não afeta o estado de outro domínio.

## 5. Evidências a coletar (entrega acadêmica)

- [ ] Saída completa dos dois scripts (logs de terminal ou captura).
- [ ] Respostas JSON de `/api/v1/verify/{doc_hash}` antes e depois da revogação para cada cenário.
- [ ] Swagger UI (`/swagger-ui`) ou `openapi.json` demonstrando contrato único.
- [ ] `GET /health` mostrando estado do Circuit Breaker (ex.: `Closed`).

## 6. Validação sintática local (sem rede)

```bash
bash -n examples/demo_ufc_certificados.sh
bash -n examples/demo_logistica_reversa.sh
command -v shellcheck >/dev/null && shellcheck examples/demo_ufc_certificados.sh examples/demo_logistica_reversa.sh || true
```

## 7. Troubleshooting

- **401/403 ou `unauthorized`:** o `ISSUER` informado não corresponde à conta que o gateway consegue assinar (`STELLAR_SOURCE_ACCOUNT`). Ajuste `ISSUER` para a chave pública da conta de origem.
- **404 em verify:** hash não registrado ou normalizado de forma diferente (verifique 64 hex chars, sem prefixo `0x`).
- **502/503:** falha de RPC ou Circuit Breaker aberto — verifique `/health` e conectividade com a testnet.
- **409 em registro:** hash já registrado em execução anterior; o script já evita isso incluindo timestamp no payload.