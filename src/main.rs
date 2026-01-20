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
        verbose,
        quiet,
    );

    let runner = TestRunner::new(runner_config);
    let report = runner.run().await?;

    // Format output
    let output_format = args.output.into();
    let use_colors = atty::is(atty::Stream::Stdout) && !quiet;
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
    let use_colors = atty::is(atty::Stream::Stdout) && !quiet;

    match args.output {
        azure_aitoolsconnect::cli::OutputFormatArg::Json => {
            let json = serde_json::to_string_pretty(&diagnostics)
                .map_err(|e| azure_aitoolsconnect::AppError::Json(e))?;
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

    // Create default configuration
    let config = Config::default_config();
    let toml = config.to_toml()?;

    // Write to file
    std::fs::write(output_path, toml)?;

    println!(
        "{} Configuration file created: {}",
        style("[+]").green(),
        output_path.display()
    );
    println!("Edit the file to add your API keys and customize settings.");

    Ok(ExitCode::Success)
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
