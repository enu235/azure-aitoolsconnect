# Azure AI Tools Connect

A Rust-based CLI tool for verifying connectivity from clients to Azure AI Services in complex network environments. Ideal for troubleshooting connectivity issues, testing API authentication, and validating service accessibility before deploying applications.

## Features

- **Multi-Service Testing** - Test Speech, Translator, Language, Vision, and Document Intelligence services
- **Multiple Authentication Methods** - API keys, Entra ID (Azure AD) tokens, and cognitive token exchange
- **Network Diagnostics** - DNS resolution, TLS handshake validation, and latency measurement
- **Flexible Configuration** - TOML files with environment variable overrides
- **Multiple Output Formats** - Human-readable, JSON, and JUnit XML for CI/CD integration
- **Cloud Support** - Global Azure and Azure China (Mooncake)

## Architecture Overview

```mermaid
flowchart TB
    subgraph CLI["CLI Layer"]
        CMD[Command Parser<br/>clap]
    end

    subgraph Core["Core Components"]
        CFG[Configuration<br/>TOML + Env]
        AUTH[Authentication<br/>Manager]
        TEST[Test Runner]
        NET[Network<br/>Diagnostics]
    end

    subgraph Services["Azure AI Services"]
        SPE[Speech]
        TRA[Translator]
        LAN[Language]
        VIS[Vision]
        DOC[Document<br/>Intelligence]
    end

    subgraph Output["Output Layer"]
        HUM[Human<br/>Formatter]
        JSN[JSON<br/>Formatter]
        JUN[JUnit<br/>Formatter]
    end

    CMD --> CFG
    CMD --> AUTH
    CMD --> TEST
    CMD --> NET

    AUTH --> TEST
    CFG --> TEST
    CFG --> AUTH

    TEST --> SPE
    TEST --> TRA
    TEST --> LAN
    TEST --> VIS
    TEST --> DOC

    TEST --> HUM
    TEST --> JSN
    TEST --> JUN
```

## Command Flow

```mermaid
flowchart LR
    A[User Command] --> B{Parse CLI}
    B --> C[Load Config]
    C --> D[Apply Env Vars]
    D --> E{Command?}

    E -->|test| F[Run Tests]
    E -->|diagnose| G[Network Diagnostics]
    E -->|init| H[Create Config]
    E -->|validate| I[Validate Config]
    E -->|list-scenarios| J[Show Scenarios]

    F --> K[Format Output]
    G --> K
    K --> L[Exit Code]
```

## Authentication Flow

```mermaid
sequenceDiagram
    participant User
    participant CLI
    participant AuthManager
    participant Azure

    User->>CLI: Run test command
    CLI->>AuthManager: Request credentials

    alt API Key Auth
        AuthManager->>Azure: Request with Ocp-Apim-Subscription-Key header
    else Entra ID Token
        AuthManager->>Azure: OAuth2 client credentials request
        Azure-->>AuthManager: Access token (cached)
        AuthManager->>Azure: Request with Bearer token
    else Cognitive Token
        AuthManager->>Azure: Exchange API key for token
        Azure-->>AuthManager: Short-lived token (10 min)
        AuthManager->>Azure: Request with token
    end

    Azure-->>CLI: Response
    CLI-->>User: Test results
```

## Test Execution Flow

```mermaid
flowchart TD
    START[Start Test] --> LOAD[Load Configuration]
    LOAD --> CREDS[Load Credentials]
    CREDS --> INPUT{Input File<br/>Required?}

    INPUT -->|Yes| LOADF[Load Input File]
    INPUT -->|No| LOOP
    LOADF --> LOOP

    LOOP[For Each Service]
    LOOP --> CTX[Create Test Context]
    CTX --> SCEN[Run Scenarios]
    SCEN --> COLLECT[Collect Results]
    COLLECT --> MORE{More<br/>Services?}

    MORE -->|Yes| LOOP
    MORE -->|No| REPORT[Generate Report]
    REPORT --> FORMAT[Format Output]
    FORMAT --> EXIT[Exit with Code]
```

## Supported Services

| Service | Description | Example Scenarios |
|---------|-------------|-------------------|
| **Speech** | Speech-to-text, text-to-speech | `voices_list`, `token_exchange`, `stt_short`, `tts` |
| **Translator** | Multi-language translation | `languages`, `detect`, `translate` |
| **Language** | Text analytics and NLU | `sentiment`, `language_detection`, `entities`, `key_phrases` |
| **Vision** | Image analysis and OCR | `analyze_image`, `read_text`, `detect_objects` |
| **Document Intelligence** | Document processing | `layout`, `read` |

## Installation

### Prerequisites

- Rust 1.70+ (Edition 2021)
- Cargo package manager

### Build from Source

```bash
# Clone the repository
git clone https://github.com/enu235/azure-aitoolsconnect.git
cd azure-aitoolsconnect

# Build release binary
cargo build --release

# Binary located at ./target/release/azure-aitoolsconnect
```

### Install via Cargo

```bash
cargo install --path .
```

## Quick Start

```bash
# Initialize a configuration file
azure-aitoolsconnect init --output config.toml

# Test all services with an API key
azure-aitoolsconnect test \
  --services all \
  --api-key YOUR_API_KEY \
  --region eastus

# Run network diagnostics
azure-aitoolsconnect diagnose \
  --dns --tls --latency \
  --region eastus
```

## Configuration

Configuration can be provided via TOML file, environment variables, or CLI arguments.

### Configuration File Structure

```toml
[global]
cloud = "global"           # "global" or "china"
timeout_seconds = 30
output_format = "human"    # "human", "json", or "junit"

[auth]
default_method = "key"     # "key", "token", or "both"

[auth.entra]
tenant_id = "your-tenant-id"
client_id = "your-client-id"
client_secret = "your-client-secret"

[services.speech]
enabled = true
region = "eastus"
api_key = "your-speech-api-key"
test_scenarios = ["voices_list", "token_exchange"]

[services.translator]
enabled = true
api_key = "your-translator-api-key"

[custom_inputs]
audio_file = "/path/to/audio.wav"
image_file = "/path/to/image.png"
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `AZURE_AI_API_KEY` | Global API key for all services |
| `AZURE_REGION` | Default Azure region |
| `AZURE_CLOUD` | Cloud environment (global/china) |
| `AZURE_SPEECH_API_KEY` | Speech service API key |
| `AZURE_TRANSLATOR_API_KEY` | Translator service API key |
| `AZURE_TENANT_ID` | Entra ID tenant |
| `AZURE_CLIENT_ID` | Entra ID client ID |
| `AZURE_CLIENT_SECRET` | Entra ID client secret |

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success - all tests passed |
| `1` | Test failure - one or more tests failed |
| `2` | Authentication failure |
| `3` | Network failure |
| `4` | Configuration error |
| `5` | Invalid input |

## Project Structure

```
azure-aitoolsconnect/
├── Cargo.toml              # Project manifest
├── src/
│   ├── main.rs             # CLI entry point
│   ├── lib.rs              # Library exports
│   ├── cli/mod.rs          # Command definitions
│   ├── config/mod.rs       # Configuration management
│   ├── auth/mod.rs         # Authentication providers
│   ├── error/mod.rs        # Error types & exit codes
│   ├── output/mod.rs       # Output formatting
│   ├── testing/mod.rs      # Test runner
│   ├── network/mod.rs      # Network diagnostics
│   └── services/           # Service implementations
│       ├── mod.rs
│       ├── speech/
│       ├── translator/
│       ├── language/
│       ├── vision/
│       └── document_intelligence/
└── config/
    └── example.toml        # Configuration template
```

## Dependencies

```mermaid
graph LR
    subgraph Runtime
        TOK[tokio]
        REQ[reqwest]
    end

    subgraph CLI
        CLP[clap]
        CON[console]
        IND[indicatif]
    end

    subgraph Data
        SER[serde]
        TOM[toml]
        JSN[serde_json]
    end

    subgraph Utilities
        CHR[chrono]
        UUID[uuid]
        URL[url]
        B64[base64]
    end
```

## License

MIT License - see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please read our contributing guidelines before submitting pull requests.

## Support

For issues and feature requests, please use the [GitHub Issues](https://github.com/enu235/azure-aitoolsconnect/issues) page.
