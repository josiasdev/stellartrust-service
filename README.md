# StellarTrust Service (`stellartrust-service`)

> **Serviço Reutilizável Descentralizado de Atestação, Verificação e Revogação de Credenciais e Documentos Digitais**

[![Rust](https://img.shields.io/badge/Rust-1.82%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![WebAssembly](https://img.shields.io/badge/WebAssembly-wasm32--unknown--unknown-654FF0?logo=webassembly)](https://webassembly.org/)
[![Stellar Soroban](https://img.shields.io/badge/Stellar-Soroban_Smart_Contracts-black?logo=stellar)](https://developers.stellar.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## Contexto Acadêmico

* **Instituição:** Universidade Federal do Ceará (UFC) – Campus Quixadá
* **Disciplina:** QXD0068 - Reuso de Software
* **Docente:** Prof. Francisco Victor da Silva Pinheiro
* **Atividade:** Trabalho 2 - Desenvolvimento de Serviço Reutilizável (**Opção A: Serviço Reutilizável / SOA & Microsserviços**)

---

## Visão Geral da Solução

O **StellarTrust** é um serviço genérico e independente de domínio projetado para garantir a **integridade, autenticidade e rastreabilidade temporal** de registros digitais (diplomas, certificados de extensão, laudos técnicos, comprovantes de logística reversa, entre outros) sem expor dados sensíveis em cadeia pública.

Em vez de armazenar documentos completos, o serviço registra o **hash criptográfico SHA-256** do artefato vinculado à identidade do emissor (`Address`) e ao domínio de negócio (`domain`) em um **Smart Contract escrito em Rust e compilado para WebAssembly (`wasm32-unknown-unknown`)** na blockchain **Stellar (Soroban)**.

Para viabilizar tanto o reuso nativo em blockchain quanto a integração transparente com sistemas corporativos tradicionais (Web2), o projeto é dividido em duas camadas desacopladas:

1. **Core Service On-Chain (`stellartrust.wasm`):** Contrato inteligente imutável e reutilizável tanto por outros contratos via invocação cruzada (*Cross-Contract Wasm Reuse*) quanto por clientes externos.
2. **Gateway REST Resiliente (`gateway-api`):** Microsserviço em Rust (`Axum`) que abstrai a complexidade de assinatura criptográfica e serialização XDR da Stellar, expondo uma **API REST documentada (OpenAPI / Swagger UI)** protegida pelos padrões arquiteturais **Circuit Breaker** e **Retry com Exponential Backoff**.

---

## Arquitetura do Sistema

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│                           APLICAÇÕES CONSUMIDORAS                            │
│  [Portal Certificados UFC]   [Sistema Rastreabilidade]   [Contrato Externo]  │
└──────────────┬───────────────────────────┬────────────────────────┬──────────┘
               │ HTTP/REST (JSON)          │ HTTP/REST (JSON)       │ Cross-Contract
               ▼                           ▼                        │ (Wasm Invoke)
┌───────────────────────────────────────────────────────┐           │
│      CAMADA 2: API GATEWAY & RESILIÊNCIA (Rust/Axum)  │           │
│  • Documentação Interativa (OpenAPI 3.0 / Swagger UI) │           │
│  • Padrão Circuit Breaker (Proteção contra falhas RPC)│           │
│  • Padrão Retry + Exponential Backoff                 │           │
└──────────────────────────┬────────────────────────────┘           │
                           │ Soroban RPC (JSON-RPC / XDR)           │
                           ▼                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│          CAMADA 1: CORE SERVICE ON-CHAIN (Soroban Smart Contract)            │
│  • Artefato Binário: stellartrust.optimized.wasm (wasm32-unknown-unknown)    │
│  • Interface Pública: register(), verify(), revoke()                         │
│  • Armazenamento Persistente Descentralizado (Ledger Storage + TTL)          │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## Estrutura do Repositório

```text
stellartrust-service/
├── Cargo.toml                     # Workspace Rust unificado
├── contracts/
│   ├── stellartrust/              # [Core] Smart Contract reutilizável (Rust -> Wasm)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs             # Lógica de registro, verificação e revogação
│   │       └── test.rs            # Testes unitários no ambiente simulado Soroban
│   └── consumer_certificates/ # [Reuso On-Chain] Contrato consumidor via Wasm import
│       ├── Cargo.toml
│       └── src/
│           └── lib.rs             # Demonstração de composição via contractimport!
├── gateway-api/                   # [Camada SOA] Microsserviço REST + Swagger + Resiliência
│   ├── Cargo.toml
│   ├── .env.example               # Variáveis de configuração (RPC URL, Contract ID, Secret Key)
│   └── src/
│       ├── main.rs                # Inicialização do servidor Axum e rotas OpenAPI
│       ├── handlers.rs            # Endpoints REST (/attestations, /verify, /revoke)
│       ├── resilience.rs          # Implementação de Circuit Breaker e Retry
│       └── stellar_client.rs      # Integração com Soroban RPC / Stellar CLI
├── examples/                      # Scripts de demonstração de reuso multi-domínio
│   ├── demo_ufc_certificados.sh   # Cenário 1: Emissão/validação de certificados acadêmicos
│   └── demo_logistica_reversa.sh  # Cenário 2: Rastreabilidade de lotes de resíduos
├── docs/                          # Relatório técnico (PDF) e diagramas arquiteturais
└── README.md                      # Documentação principal do projeto
```

---

## Pré-requisitos e Configuração do Ambiente

Certifique-se de ter as seguintes ferramentas instaladas:

1. **Rust Toolchain** (versão 1.82 ou superior) e target WebAssembly:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup target add wasm32-unknown-unknown
   ```

2. **Stellar CLI** (ferramenta oficial para compilação, otimização e deploy de contratos Soroban):
   ```bash
   cargo install --locked stellar-cli --features opt
   ```

3. **Configuração da Rede Testnet e Conta de Emissor:**
   ```bash
   # Configurar rede testnet
   stellar network add --global testnet \
     --rpc-url https://soroban-testnet.stellar.org:443 \
     --network-passphrase "Test SDF Network ; September 2015"

   # Gerar chave de identidade local e financiar via Friendbot
   stellar keys generate --global alice --network testnet
   stellar keys address alice
   ```

---

## Compilação, Testes e Deploy (Camada WebAssembly)

### 1. Executar Testes Unitários do Smart Contract
```bash
cargo test -p stellartrust
```

### 2. Compilar e Otimizar o Binário WebAssembly (`.wasm`)
```bash
# Compila o contrato para wasm32-unknown-unknown
stellar contract build --package stellartrust

# Otimiza o bytecode Wasm reduzindo tamanho e custo de execução (CPU/RAM na VM)
stellar contract optimize \
  --wasm target/wasm32-unknown-unknown/release/stellartrust.wasm
```
O artefato final reutilizável será gerado em:
`target/wasm32-unknown-unknown/release/stellartrust.optimized.wasm`

### 3. Realizar Deploy na Stellar Testnet
```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/stellartrust.optimized.wasm \
  --source-account alice \
  --network testnet
```
> **Nota:** Copie o `CONTRACT_ID` retornado (ex.: `CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC`) para utilizá-lo tanto no contrato consumidor quanto no Gateway REST.

---

## Executando o Gateway REST Resiliente (Camada SOA)

1. Configure as variáveis de ambiente:
   ```bash
   cp gateway-api/.env.example gateway-api/.env
   ```
   Edite `gateway-api/.env` informando o `STELLAR_CONTRACT_ID`, a `STELLAR_RPC_URL` (`https://soroban-testnet.stellar.org`) e a chave do emissor.

2. Inicie o microsserviço:
   ```bash
   cargo run -p gateway-api
   ```

3. Acesse a **Documentação Interativa (Swagger UI)** no navegador:
   * **Swagger UI:** `http://localhost:8080/swagger-ui`
   * **Especificação OpenAPI JSON:** `http://localhost:8080/api-docs/openapi.json`
   * **Health Check & Status do Circuit Breaker:** `http://localhost:8080/health`

### Resumo dos Endpoints Disponíveis

| Método | Endpoint | Descrição |
| :--- | :--- | :--- |
| `POST` | `/api/v1/attestations` | Registra um novo hash SHA-256 vinculado a um domínio na blockchain Stellar. |
| `GET` | `/api/v1/verify/{doc_hash}` | Consulta a autenticidade, emissor, timestamp e status de revogação de um hash. |
| `POST` | `/api/v1/attestations/{doc_hash}/revoke` | Revoga uma atestação previamente emitida pelo mesmo emissor. |
| `GET` | `/health` | Retorna o estado operacional da API e o estado atual do *Circuit Breaker* (`Closed`, `Open`, `HalfOpen`). |

---

## Demonstração de Reuso (Múltiplos Contextos)

Para evidenciar o cumprimento dos requisitos de reutilização da disciplina, o repositório inclui dois mecanismos distintos de reuso sem qualquer alteração no código do `StellarTrust`:

### 1. Reuso Binário On-Chain (Composição via WebAssembly)
O contrato `contracts/consumer_certificates` importa diretamente a interface tipada a partir do arquivo `.wasm` compilado utilizando a macro `soroban_sdk::contractimport!`:

```rust
mod trust_service {
    soroban_sdk::contractimport!(
        file = "../../target/wasm32-unknown-unknown/release/stellartrust.optimized.wasm"
    );
}
```
Dessa forma, qualquer contrato inteligente desenvolvido em Rust na rede Stellar pode reutilizar o `StellarTrust` como módulo de certificação nativo.

### 2. Reuso Off-Chain Multi-Domínio (via API REST)
Execute os scripts na pasta `examples/` para simular dois sistemas independentes consumindo a mesma instância do serviço:
```bash
# Domínio 1: Validação de Certificados de Eventos da UFC Quixadá
bash examples/demo_ufc_certificados.sh

# Domínio 2: Rastreabilidade Ambiental de Lotes de Reciclagem
bash examples/demo_logistica_reversa.sh
```

---

## Padrões de Reuso e Resiliência Aplicados

| Categoria | Padrão Aplicado | Implementação no Projeto |
| :--- | :--- | :--- |
| **Reuso** | *Service-Oriented Architecture (SOA)* | Encapsulamento da lógica de atestação em um serviço autônomo acessível por contratos padronizados. |
| **Reuso** | *Interface-Based Design* | Contratos explícitos via tipos XDR no Soroban e especificação OpenAPI 3.0 no Gateway REST. |
| **Reuso** | *Binary Component Reuse (Wasm)* | Exportação e importação direta do bytecode `stellartrust.optimized.wasm` entre contratos inteligentes. |
| **Resiliência** | *Circuit Breaker* | Monitoramento de falhas consecutivas na comunicação com o nó RPC da Stellar, abrindo o circuito para evitar *cascading failures*. |
| **Resiliência** | *Retry com Exponential Backoff* | Re-tentativas automáticas com atraso exponencial para lidar com latências transitórias de consenso na Testnet. |
| **Resiliência** | *Idempotência por Chave Única* | Prevenção de registros duplicados garantida deterministicamente pelo armazenamento persistente indexado por `doc_hash`. |

---

## Autor

* **Francisco Josias da Silva Batista** - Engenharia de Software - Universidade Federal do Ceará (Campus Quixadá)

---

## Licença

Este projeto é distribuído sob a licença MIT. Consulte o arquivo `LICENSE` para mais detalhes.
