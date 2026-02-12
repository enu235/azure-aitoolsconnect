use azure_aitoolsconnect::{
    cli::{parse_services, Cli, Commands},
    config::{validate_config, Config},
    error::ExitCode,
    network::{format_diagnostics, run_diagnostics},
    output::{get_formatter, write_output},
    testing::{format_scenarios, list_scenarios, TestRunner, TestRunnerConfig},
};
use clap::Parser;
use console::style;
use std::io::IsTerminal;
use std::process::ExitCode as StdExitCode;

#[tokio::main]
async fn main() -> StdExitCode {
    // Load .env file if present
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    let exit_code = match run(cli).await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{} {}", style("Error:").red().bold(), e);
            if let Some(hint) = e.hint() {
                eprintln!();
                eprintln!("{} {}", style("Hint:").yellow().bold(), hint);
            }
            e.exit_code()
        }
    };

    StdExitCode::from(exit_code as u8)
}

async fn run(cli: Cli) -> azure_aitoolsconnect::Result<ExitCode> {
    // Load configuration
    let mut config = if let Some(config_path) = &cli.config {
        Config::from_file(config_path)?
    } else {
        Config::default_config()
    };

    // Apply environment variable overrides
    config.apply_env_overrides();

    match cli.command {
        Commands::Test(args) => run_test(args, &config, cli.verbose, cli.quiet).await,
        Commands::Login(args) => run_login(args, cli.quiet).await,
        Commands::Diagnose(args) => run_diagnose(args, cli.verbose, cli.quiet).await,
        Commands::Init(args) => run_init(args),
        Commands::Validate(args) => run_validate(args),
        Commands::ListScenarios(args) => run_list_scenarios(args),
    }
}

async fn run_test(
    args: azure_aitoolsconnect::cli::TestArgs,
    config: &Config,
    verbose: bool,
    quiet: bool,
) -> azure_aitoolsconnect::Result<ExitCode> {
    let services = parse_services(&args.services);

    let runner_config = TestRunnerConfig::from_config(
        config,
        services,
        args.api_key,
        args.region,
        Some(args.cloud.into()),
        Some(args.auth.into()),
        Some(args.timeout),
        args.endpoint,
        args.input_file.map(|p| p.to_string_lossy().to_string()),
        args.scenarios,
        args.tenant,
        args.bearer_token,
        verbose,
        quiet,
        args.show_token,
        args.no_cache,
    );

    let runner = TestRunner::new(runner_config);
    let report = runner.run().await?;

    // Format output
    let output_format = args.output.into();
    let use_colors = std::io::stdout().is_terminal() && !quiet;
    let formatter = get_formatter(output_format, use_colors);
    let output = formatter.format(&report);

    // Write output
    write_output(&output, args.output_file.as_deref())?;

    if report.all_passed() {
        Ok(ExitCode::Success)
    } else {
        Ok(ExitCode::TestFailure)
    }
}

async fn run_login(
    args: azure_aitoolsconnect::cli::LoginArgs,
    quiet: bool,
) -> azure_aitoolsconnect::Result<ExitCode> {
    use azure_aitoolsconnect::auth::token_cache::{CachedTokenEntry, TokenCacheFile};
    use azure_aitoolsconnect::config::Cloud;
    use chrono::Utc;

    // Handle --clear-cache
    if args.clear_cache {
        TokenCacheFile::clear()?;
        if !quiet {
            eprintln!("{} Token cache cleared.", style("[+]").green());
        }
        return Ok(ExitCode::Success);
    }

    let cloud: Cloud = args.cloud.into();

    match args.auth {
        azure_aitoolsconnect::cli::LoginAuthMethodArg::DeviceCode => {
            let tenant_id = args.tenant.ok_or(azure_aitoolsconnect::AppError::MissingTenantId)?;

            // Check disk cache first
            let scope = cloud.cognitive_scope();
            if let Ok(cache) = TokenCacheFile::load() {
                if let Some(entry) = cache.get_valid_token(scope, &tenant_id) {
                    if !quiet {
                        eprintln!(
                            "  {} Using cached token ({} minutes remaining)",
                            style("[*]").cyan(),
                            entry.remaining_minutes()
                        );
                        eprintln!();
                    }
                    output_token(&entry.access_token, entry.remaining_minutes() as u64, &args.output);
                    return Ok(ExitCode::Success);
                }
            }

            let auth = azure_aitoolsconnect::auth::DeviceCodeAuth::new(
                tenant_id.clone(),
                args.client_id,
                &cloud,
            )?
            .with_quiet(quiet);

            let result = auth.authenticate().await?;

            // Save to cache if requested
            if args.save {
                let mut cache = TokenCacheFile::load().unwrap_or_default();
                cache.insert(CachedTokenEntry {
                    access_token: result.access_token.clone(),
                    expires_at: Utc::now()
                        + chrono::Duration::seconds(result.expires_in_secs as i64),
                    scope: result.scope.clone(),
                    tenant_id: tenant_id.clone(),
                });
                cache.save()?;
                if !quiet {
                    eprintln!("  {} Token saved to cache.", style("[+]").green());
                    eprintln!();
                }
            }

            output_token(
                &result.access_token,
                result.expires_in_secs / 60,
                &args.output,
            );
        }
        azure_aitoolsconnect::cli::LoginAuthMethodArg::ManagedIdentity => {
            let mi = azure_aitoolsconnect::auth::ManagedIdentityAuth::new(&cloud, None)?;
            use azure_aitoolsconnect::auth::AuthProvider;
            let creds = mi.get_credentials().await?;
            if let azure_aitoolsconnect::auth::Credentials::BearerToken(token) = creds {
                output_token(&token, 60, &args.output);
            }
        }
    }

    Ok(ExitCode::Success)
}

/// Output the token in the requested format
fn output_token(token: &str, expires_in_minutes: u64, format: &azure_aitoolsconnect::cli::OutputFormatArg) {
    match format {
        azure_aitoolsconnect::cli::OutputFormatArg::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "access_token": token,
                    "expires_in_minutes": expires_in_minutes,
                })
            );
        }
        _ => {
            eprintln!(
                "{} (expires in ~{} minutes):",
                style("Bearer Token").bold(),
                expires_in_minutes
            );
            println!("{}", token);
        }
    }
}

async fn run_diagnose(
    args: azure_aitoolsconnect::cli::DiagnoseArgs,
    _verbose: bool,
    quiet: bool,
) -> azure_aitoolsconnect::Result<ExitCode> {
    let region = args.region.unwrap_or_else(|| "eastus".to_string());
    let cloud = args.cloud.into();

    // If no specific checks are requested, run all
    let (check_dns, check_tls, check_latency) = if !args.dns && !args.tls && !args.latency {
        (true, true, true)
    } else {
        (args.dns, args.tls, args.latency)
    };

    if !quiet {
        println!(
            "{} Running network diagnostics for {} ({})...",
            style("[*]").cyan(),
            region,
            cloud
        );
    }

    let diagnostics = run_diagnostics(
        &region,
        cloud,
        check_dns,
        check_tls,
        check_latency,
        args.endpoint.as_deref(),
    )
    .await;

    // Format output
    let use_colors = std::io::stdout().is_terminal() && !quiet;

    match args.output {
        azure_aitoolsconnect::cli::OutputFormatArg::Json => {
            let json = serde_json::to_string_pretty(&diagnostics)
                .map_err(azure_aitoolsconnect::AppError::Json)?;
            println!("{}", json);
        }
        _ => {
            let output = format_diagnostics(&diagnostics, use_colors);
            print!("{}", output);
        }
    }

    // Check for failures
    let has_dns_failure = diagnostics.dns.iter().any(|r| !r.resolved);
    let has_tls_failure = diagnostics.tls.iter().any(|r| !r.success);
    let has_latency_failure = diagnostics.latency.iter().any(|r| !r.success);

    if has_dns_failure || has_tls_failure || has_latency_failure {
        Ok(ExitCode::NetworkFailure)
    } else {
        Ok(ExitCode::Success)
    }
}

fn run_init(args: azure_aitoolsconnect::cli::InitArgs) -> azure_aitoolsconnect::Result<ExitCode> {
    let output_path = &args.output;

    // Check if file exists
    if output_path.exists() && !args.force {
        return Err(azure_aitoolsconnect::AppError::Config(format!(
            "File already exists: {}. Use --force to overwrite.",
            output_path.display()
        )));
    }

    let config = if args.interactive {
        run_interactive_init()?
    } else {
        Config::default_config()
    };

    let toml = config.to_toml()?;

    // Write to file
    std::fs::write(output_path, toml)?;

    println!(
        "{} Configuration file created: {}",
        style("[+]").green(),
        output_path.display()
    );
    if !args.interactive {
        println!("Edit the file to add your API keys and customize settings.");
        println!(
            "Or run {} for guided setup.",
            style("azure-aitoolsconnect init --interactive").cyan()
        );
    }

    Ok(ExitCode::Success)
}

/// Interactive configuration wizard
fn run_interactive_init() -> azure_aitoolsconnect::Result<Config> {
    use azure_aitoolsconnect::config::*;
    use std::collections::HashMap;

    println!();
    println!(
        "{} {}",
        style("[*]").cyan(),
        style("Azure AI Tools Connect - Configuration Wizard").bold()
    );
    println!();

    // Cloud
    let cloud = prompt_choice(
        "Cloud environment",
        &["global", "china"],
        "global",
    )?;
    let cloud: Cloud = cloud.parse().unwrap_or(Cloud::Global);

    // Region
    let region = prompt_input("Azure region", "eastus")?;

    // Auth method
    let auth_str = prompt_choice(
        "Authentication method",
        &["key", "device-code", "managed-identity", "manual-token"],
        "key",
    )?;
    let auth_method: AuthMethod = auth_str.parse().unwrap_or(AuthMethod::Key);

    // Auth-specific config
    let mut user_config = UserAuthConfig::default();
    let mut api_key: Option<String> = None;

    match auth_method {
        AuthMethod::Key => {
            let key = prompt_input("API key (leave blank to set later)", "")?;
            if !key.is_empty() {
                api_key = Some(key);
            }
        }
        AuthMethod::DeviceCode => {
            let tenant = prompt_input("Tenant ID", "")?;
            if !tenant.is_empty() {
                user_config.tenant_id = Some(tenant);
            }
        }
        AuthMethod::ManualToken => {
            println!("  You can set the bearer token later with --bearer-token or AZURE_BEARER_TOKEN.");
        }
        _ => {}
    }

    // Endpoint
    let endpoint_str = prompt_input(
        "Custom endpoint URL (leave blank for regional)",
        "",
    )?;
    let endpoint = if endpoint_str.is_empty() {
        None
    } else {
        Some(endpoint_str)
    };

    // Services
    println!();
    println!("  Available services: speech, translator, language, vision, document_intelligence");
    let services_str = prompt_input(
        "Services to enable (comma-separated, or 'all')",
        "all",
    )?;
    let service_names = if services_str.to_lowercase() == "all" {
        vec!["speech", "translator", "language", "vision", "document_intelligence"]
    } else {
        services_str.split(',').map(|s| s.trim()).collect()
    };

    // Build config
    let mut services = HashMap::new();
    for name in service_names {
        let name = name.to_lowercase().replace('-', "_");
        services.insert(
            name,
            ServiceConfig {
                enabled: true,
                region: Some(region.clone()),
                api_key: api_key.clone(),
                endpoint: endpoint.clone(),
                test_scenarios: vec![],
            },
        );
    }

    let config = Config {
        global: GlobalConfig {
            cloud,
            timeout_seconds: DEFAULT_TIMEOUT_SECS,
            output_format: OutputFormat::Human,
        },
        auth: AuthConfig {
            default_method: auth_method,
            entra: EntraConfig::default(),
            user: user_config,
        },
        services,
        custom_inputs: CustomInputs::default(),
    };

    println!();
    println!("  {} Configuration ready.", style("[+]").green());

    Ok(config)
}

/// Prompt the user for input with a default value
fn prompt_input(label: &str, default: &str) -> azure_aitoolsconnect::Result<String> {
    use std::io::{self, Write};

    if default.is_empty() {
        eprint!("  {}: ", style(label).bold());
    } else {
        eprint!("  {} [{}]: ", style(label).bold(), style(default).dim());
    }
    io::stderr().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_string();

    if input.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(input)
    }
}

/// Prompt the user to choose from a list of options
fn prompt_choice(label: &str, options: &[&str], default: &str) -> azure_aitoolsconnect::Result<String> {
    use std::io::{self, Write};

    eprint!(
        "  {} ({}) [{}]: ",
        style(label).bold(),
        options.join("/"),
        style(default).dim()
    );
    io::stderr().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    if input.is_empty() {
        Ok(default.to_string())
    } else if options.contains(&input.as_str()) {
        Ok(input)
    } else {
        eprintln!("  {} Invalid choice '{}', using default '{}'", style("[!]").yellow(), input, default);
        Ok(default.to_string())
    }
}

fn run_validate(
    args: azure_aitoolsconnect::cli::ValidateArgs,
) -> azure_aitoolsconnect::Result<ExitCode> {
    let config_path = &args.config;

    if !config_path.exists() {
        return Err(azure_aitoolsconnect::AppError::FileNotFound(
            config_path.display().to_string(),
        ));
    }

    let config = Config::from_file(config_path)?;
    let warnings = validate_config(&config)?;

    println!(
        "{} Configuration file is valid: {}",
        style("[+]").green(),
        config_path.display()
    );

    if !warnings.is_empty() {
        println!("\n{}", style("Warnings:").yellow());
        for warning in &warnings {
            println!("  {} {}", style("!").yellow(), warning);
        }
    }

    // Show summary
    println!("\n{}", style("Configuration Summary:").bold());
    println!("  Cloud: {}", config.global.cloud);
    println!("  Timeout: {}s", config.global.timeout_seconds);
    println!("  Auth Method: {}", config.auth.default_method);
    println!("  Services:");
    for (name, service) in &config.services {
        let status = if service.enabled {
            style("enabled").green()
        } else {
            style("disabled").dim()
        };
        let has_key = if service.api_key.is_some() {
            style("(key set)").green()
        } else {
            style("(no key)").dim()
        };
        println!("    - {} {} {}", name, status, has_key);
    }

    if warnings.is_empty() {
        Ok(ExitCode::Success)
    } else {
        Ok(ExitCode::ConfigError)
    }
}

fn run_list_scenarios(
    args: azure_aitoolsconnect::cli::ListScenariosArgs,
) -> azure_aitoolsconnect::Result<ExitCode> {
    let scenarios = list_scenarios(args.service.as_deref());

    if scenarios.is_empty() {
        if let Some(service) = &args.service {
            return Err(azure_aitoolsconnect::AppError::Config(format!(
                "Unknown service: {}",
                service
            )));
        }
    }

    let output = format_scenarios(&scenarios);
    print!("{}", output);

    Ok(ExitCode::Success)
}
