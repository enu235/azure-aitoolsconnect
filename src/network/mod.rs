use crate::config::Cloud;
use reqwest::Client;
use serde::Serialize;
use std::time::{Duration, Instant};

/// DNS resolution result
#[derive(Debug, Clone, Serialize)]
pub struct DnsResult {
    pub hostname: String,
    pub resolved: bool,
    pub addresses: Vec<String>,
    pub duration_ms: u64,
    pub error: Option<String>,
}

/// TLS handshake result
#[derive(Debug, Clone, Serialize)]
pub struct TlsResult {
    pub endpoint: String,
    pub success: bool,
    pub duration_ms: u64,
    pub error: Option<String>,
}

/// Latency measurement result
#[derive(Debug, Clone, Serialize)]
pub struct LatencyResult {
    pub endpoint: String,
    pub latency_ms: u64,
    pub success: bool,
    pub error: Option<String>,
}

/// Complete network diagnostics report
#[derive(Debug, Clone, Serialize)]
pub struct NetworkDiagnostics {
    pub dns: Vec<DnsResult>,
    pub tls: Vec<TlsResult>,
    pub latency: Vec<LatencyResult>,
}

/// Get common Azure AI Services endpoints for a region
pub fn get_endpoints_for_region(region: &str, cloud: Cloud) -> Vec<String> {
    match cloud {
        Cloud::Global => vec![
            format!("{}.api.cognitive.microsoft.com", region),
            "api.cognitive.microsofttranslator.com".to_string(),
            format!("{}.cognitiveservices.azure.com", region),
        ],
        Cloud::China => vec![
            format!("{}.api.cognitive.azure.cn", region),
            "api.translator.azure.cn".to_string(),
            format!("{}.cognitiveservices.azure.cn", region),
        ],
    }
}

/// Perform DNS resolution check
pub async fn check_dns(hostname: &str) -> DnsResult {
    let start = Instant::now();

    // Use tokio's DNS resolution
    match tokio::net::lookup_host(format!("{}:443", hostname)).await {
        Ok(addrs) => {
            let addresses: Vec<String> = addrs.map(|a| a.ip().to_string()).collect();
            DnsResult {
                hostname: hostname.to_string(),
                resolved: !addresses.is_empty(),
                addresses,
                duration_ms: start.elapsed().as_millis() as u64,
                error: None,
            }
        }
        Err(e) => DnsResult {
            hostname: hostname.to_string(),
            resolved: false,
            addresses: vec![],
            duration_ms: start.elapsed().as_millis() as u64,
            error: Some(e.to_string()),
        },
    }
}

/// Perform TLS handshake check
pub async fn check_tls(endpoint: &str) -> TlsResult {
    let start = Instant::now();
    let url = format!("https://{}", endpoint);

    let client = match Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return TlsResult {
                endpoint: endpoint.to_string(),
                success: false,
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some(format!("Failed to create client: {}", e)),
            }
        }
    };

    match client.get(&url).send().await {
        Ok(_) => TlsResult {
            endpoint: endpoint.to_string(),
            success: true,
            duration_ms: start.elapsed().as_millis() as u64,
            error: None,
        },
        Err(e) => {
            // Check if it's a TLS-specific error
            let error_msg = if e.is_connect() {
                format!("Connection failed: {}", e)
            } else if e.is_timeout() {
                "Connection timed out".to_string()
            } else {
                e.to_string()
            };

            TlsResult {
                endpoint: endpoint.to_string(),
                success: false,
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some(error_msg),
            }
        }
    }
}

/// Measure latency to an endpoint
pub async fn measure_latency(endpoint: &str) -> LatencyResult {
    let start = Instant::now();
    let url = format!("https://{}", endpoint);

    let client = match Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return LatencyResult {
                endpoint: endpoint.to_string(),
                latency_ms: 0,
                success: false,
                error: Some(format!("Failed to create client: {}", e)),
            }
        }
    };

    match client.head(&url).send().await {
        Ok(_) => LatencyResult {
            endpoint: endpoint.to_string(),
            latency_ms: start.elapsed().as_millis() as u64,
            success: true,
            error: None,
        },
        Err(e) => LatencyResult {
            endpoint: endpoint.to_string(),
            latency_ms: start.elapsed().as_millis() as u64,
            success: false,
            error: Some(e.to_string()),
        },
    }
}

/// Run full network diagnostics
pub async fn run_diagnostics(
    region: &str,
    cloud: Cloud,
    check_dns_flag: bool,
    check_tls_flag: bool,
    check_latency_flag: bool,
    custom_endpoint: Option<&str>,
) -> NetworkDiagnostics {
    let endpoints = if let Some(endpoint) = custom_endpoint {
        vec![endpoint.to_string()]
    } else {
        get_endpoints_for_region(region, cloud)
    };

    let mut dns_results = Vec::new();
    let mut tls_results = Vec::new();
    let mut latency_results = Vec::new();

    for endpoint in &endpoints {
        if check_dns_flag {
            dns_results.push(check_dns(endpoint).await);
        }

        if check_tls_flag {
            tls_results.push(check_tls(endpoint).await);
        }

        if check_latency_flag {
            latency_results.push(measure_latency(endpoint).await);
        }
    }

    NetworkDiagnostics {
        dns: dns_results,
        tls: tls_results,
        latency: latency_results,
    }
}

/// Format network diagnostics for human-readable output
pub fn format_diagnostics(diagnostics: &NetworkDiagnostics, use_colors: bool) -> String {
    use console::style;

    let mut output = String::new();

    output.push_str("\nNetwork Diagnostics\n");
    output.push_str("==================\n\n");

    if !diagnostics.dns.is_empty() {
        output.push_str("DNS Resolution:\n");
        for result in &diagnostics.dns {
            let status = if result.resolved {
                if use_colors {
                    style("\u{2713}").green().to_string()
                } else {
                    "[OK]".to_string()
                }
            } else {
                if use_colors {
                    style("\u{2717}").red().to_string()
                } else {
                    "[FAIL]".to_string()
                }
            };

            output.push_str(&format!(
                "  {} {} ({}ms)\n",
                status, result.hostname, result.duration_ms
            ));

            if result.resolved {
                for addr in &result.addresses {
                    if use_colors {
                        output.push_str(&format!("    {}\n", style(addr).dim()));
                    } else {
                        output.push_str(&format!("    {}\n", addr));
                    }
                }
            } else if let Some(error) = &result.error {
                if use_colors {
                    output.push_str(&format!("    {}\n", style(error).red()));
                } else {
                    output.push_str(&format!("    Error: {}\n", error));
                }
            }
        }
        output.push('\n');
    }

    if !diagnostics.tls.is_empty() {
        output.push_str("TLS Handshake:\n");
        for result in &diagnostics.tls {
            let status = if result.success {
                if use_colors {
                    style("\u{2713}").green().to_string()
                } else {
                    "[OK]".to_string()
                }
            } else {
                if use_colors {
                    style("\u{2717}").red().to_string()
                } else {
                    "[FAIL]".to_string()
                }
            };

            output.push_str(&format!(
                "  {} {} ({}ms)\n",
                status, result.endpoint, result.duration_ms
            ));

            if !result.success {
                if let Some(error) = &result.error {
                    if use_colors {
                        output.push_str(&format!("    {}\n", style(error).red()));
                    } else {
                        output.push_str(&format!("    Error: {}\n", error));
                    }
                }
            }
        }
        output.push('\n');
    }

    if !diagnostics.latency.is_empty() {
        output.push_str("Latency:\n");
        for result in &diagnostics.latency {
            let status = if result.success {
                if use_colors {
                    style("\u{2713}").green().to_string()
                } else {
                    "[OK]".to_string()
                }
            } else {
                if use_colors {
                    style("\u{2717}").red().to_string()
                } else {
                    "[FAIL]".to_string()
                }
            };

            output.push_str(&format!(
                "  {} {} - {}ms\n",
                status, result.endpoint, result.latency_ms
            ));

            if !result.success {
                if let Some(error) = &result.error {
                    if use_colors {
                        output.push_str(&format!("    {}\n", style(error).red()));
                    } else {
                        output.push_str(&format!("    Error: {}\n", error));
                    }
                }
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_endpoints_global() {
        let endpoints = get_endpoints_for_region("eastus", Cloud::Global);
        assert!(endpoints.iter().any(|e| e.contains("eastus")));
        assert!(endpoints.iter().any(|e| e.contains("microsofttranslator")));
    }

    #[test]
    fn test_get_endpoints_china() {
        let endpoints = get_endpoints_for_region("chinaeast2", Cloud::China);
        assert!(endpoints.iter().any(|e| e.contains("azure.cn")));
    }
}
