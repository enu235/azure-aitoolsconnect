use crate::config::OutputFormat;
use crate::services::ServiceTestResults;
use chrono::{DateTime, Utc};
use console::{style, Style};
use serde::Serialize;
use std::io::Write;

/// Summary of all test results
#[derive(Debug, Clone, Serialize)]
pub struct TestSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
}

/// Complete test report
#[derive(Debug, Clone, Serialize)]
pub struct TestReport {
    pub timestamp: DateTime<Utc>,
    pub summary: TestSummary,
    pub total_duration_ms: u64,
    pub services: Vec<ServiceTestResults>,
}

impl TestReport {
    pub fn new(services: Vec<ServiceTestResults>) -> Self {
        let mut total = 0;
        let mut passed = 0;
        let mut failed = 0;
        let mut skipped = 0;
        let mut total_duration_ms = 0;

        for service in &services {
            for result in &service.results {
                total += 1;
                if result.success {
                    passed += 1;
                } else if result.error.as_ref().map(|e| e.starts_with("Skipped")).unwrap_or(false) {
                    skipped += 1;
                } else {
                    failed += 1;
                }
            }
            total_duration_ms += service.total_duration_ms;
        }

        Self {
            timestamp: Utc::now(),
            summary: TestSummary {
                total,
                passed,
                failed,
                skipped,
            },
            total_duration_ms,
            services,
        }
    }

    pub fn all_passed(&self) -> bool {
        self.summary.failed == 0
    }
}

/// Output formatter trait
pub trait OutputFormatter {
    fn format(&self, report: &TestReport) -> String;
}

/// Human-readable console output formatter
pub struct HumanFormatter {
    use_colors: bool,
}

impl HumanFormatter {
    pub fn new(use_colors: bool) -> Self {
        Self { use_colors }
    }

    fn check_mark(&self) -> &'static str {
        if self.use_colors {
            "\u{2713}" // ✓
        } else {
            "[PASS]"
        }
    }

    fn cross_mark(&self) -> &'static str {
        if self.use_colors {
            "\u{2717}" // ✗
        } else {
            "[FAIL]"
        }
    }

    fn skip_mark(&self) -> &'static str {
        if self.use_colors {
            "\u{25CB}" // ○
        } else {
            "[SKIP]"
        }
    }
}

impl OutputFormatter for HumanFormatter {
    fn format(&self, report: &TestReport) -> String {
        let mut output = String::new();

        // Header
        output.push_str("\nAzure AI Services Connectivity Test Results\n");
        output.push_str("==================================================\n\n");

        // Service results
        for service in &report.services {
            if self.use_colors {
                output.push_str(&format!(
                    "{} ({})\n",
                    style(&service.service_name).bold(),
                    style(&service.endpoint).dim()
                ));
            } else {
                output.push_str(&format!("{} ({})\n", service.service_name, service.endpoint));
            }

            for result in &service.results {
                let is_skipped = result
                    .error
                    .as_ref()
                    .map(|e| e.starts_with("Skipped"))
                    .unwrap_or(false);

                let (mark, name_style) = if result.success {
                    (
                        if self.use_colors {
                            style(self.check_mark()).green().to_string()
                        } else {
                            self.check_mark().to_string()
                        },
                        Style::new().green(),
                    )
                } else if is_skipped {
                    (
                        if self.use_colors {
                            style(self.skip_mark()).yellow().to_string()
                        } else {
                            self.skip_mark().to_string()
                        },
                        Style::new().yellow(),
                    )
                } else {
                    (
                        if self.use_colors {
                            style(self.cross_mark()).red().to_string()
                        } else {
                            self.cross_mark().to_string()
                        },
                        Style::new().red(),
                    )
                };

                if self.use_colors {
                    output.push_str(&format!(
                        "  {} {} ({}ms)\n",
                        mark,
                        name_style.apply_to(&result.scenario_name),
                        result.duration_ms
                    ));
                } else {
                    output.push_str(&format!(
                        "  {} {} ({}ms)\n",
                        mark, result.scenario_name, result.duration_ms
                    ));
                }

                // Show details or errors
                if let Some(details) = &result.details {
                    if self.use_colors {
                        output.push_str(&format!("    {}\n", style(details).dim()));
                    } else {
                        output.push_str(&format!("    {}\n", details));
                    }
                }

                if !result.success {
                    if let Some(error) = &result.error {
                        if self.use_colors {
                            output.push_str(&format!(
                                "    {} {}\n",
                                style("\u{2192}").dim(), // →
                                style(error).red()
                            ));
                        } else {
                            output.push_str(&format!("    -> {}\n", error));
                        }
                    }
                }
            }
            output.push('\n');
        }

        // Summary
        output.push_str("Summary\n");
        if self.use_colors {
            output.push_str(&format!(
                "  Total: {} | {} | {} | {}\n",
                style(report.summary.total).bold(),
                style(format!("Passed: {}", report.summary.passed)).green(),
                style(format!("Failed: {}", report.summary.failed)).red(),
                style(format!("Skipped: {}", report.summary.skipped)).yellow()
            ));
        } else {
            output.push_str(&format!(
                "  Total: {} | Passed: {} | Failed: {} | Skipped: {}\n",
                report.summary.total,
                report.summary.passed,
                report.summary.failed,
                report.summary.skipped
            ));
        }
        output.push_str(&format!("  Duration: {}ms\n", report.total_duration_ms));

        output
    }
}

/// JSON output formatter
pub struct JsonFormatter {
    pretty: bool,
}

impl JsonFormatter {
    pub fn new(pretty: bool) -> Self {
        Self { pretty }
    }
}

impl OutputFormatter for JsonFormatter {
    fn format(&self, report: &TestReport) -> String {
        if self.pretty {
            serde_json::to_string_pretty(report).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
        } else {
            serde_json::to_string(report).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
        }
    }
}

/// JUnit XML output formatter for CI/CD integration
pub struct JunitFormatter;

impl JunitFormatter {
    pub fn new() -> Self {
        Self
    }

    fn escape_xml(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }
}

impl Default for JunitFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFormatter for JunitFormatter {
    fn format(&self, report: &TestReport) -> String {
        let mut xml = String::new();

        xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        xml.push_str(&format!(
            "<testsuites tests=\"{}\" failures=\"{}\" errors=\"0\" time=\"{:.3}\">\n",
            report.summary.total,
            report.summary.failed,
            report.total_duration_ms as f64 / 1000.0
        ));

        for service in &report.services {
            let service_failures = service.failed();
            let service_tests = service.results.len();

            xml.push_str(&format!(
                "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" errors=\"0\" time=\"{:.3}\">\n",
                Self::escape_xml(&service.service_name),
                service_tests,
                service_failures,
                service.total_duration_ms as f64 / 1000.0
            ));

            for result in &service.results {
                xml.push_str(&format!(
                    "    <testcase name=\"{}\" classname=\"{}\" time=\"{:.3}\"",
                    Self::escape_xml(&result.scenario_name),
                    Self::escape_xml(&service.service_name),
                    result.duration_ms as f64 / 1000.0
                ));

                if result.success {
                    xml.push_str(" />\n");
                } else {
                    xml.push_str(">\n");

                    let is_skipped = result
                        .error
                        .as_ref()
                        .map(|e| e.starts_with("Skipped"))
                        .unwrap_or(false);

                    if is_skipped {
                        xml.push_str(&format!(
                            "      <skipped message=\"{}\" />\n",
                            Self::escape_xml(result.error.as_deref().unwrap_or(""))
                        ));
                    } else {
                        xml.push_str(&format!(
                            "      <failure message=\"{}\" type=\"AssertionError\">\n",
                            Self::escape_xml(result.error.as_deref().unwrap_or("Test failed"))
                        ));
                        if let Some(details) = &result.details {
                            xml.push_str(&format!("        {}\n", Self::escape_xml(details)));
                        }
                        xml.push_str("      </failure>\n");
                    }

                    xml.push_str("    </testcase>\n");
                }
            }

            xml.push_str("  </testsuite>\n");
        }

        xml.push_str("</testsuites>\n");
        xml
    }
}

/// Get formatter based on output format
pub fn get_formatter(format: OutputFormat, use_colors: bool) -> Box<dyn OutputFormatter> {
    match format {
        OutputFormat::Human => Box::new(HumanFormatter::new(use_colors)),
        OutputFormat::Json => Box::new(JsonFormatter::new(true)),
        OutputFormat::Junit => Box::new(JunitFormatter::new()),
    }
}

/// Write output to file or stdout
pub fn write_output(output: &str, file_path: Option<&std::path::Path>) -> std::io::Result<()> {
    if let Some(path) = file_path {
        let mut file = std::fs::File::create(path)?;
        file.write_all(output.as_bytes())?;
    } else {
        print!("{}", output);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::TestResult;

    fn create_test_report() -> TestReport {
        TestReport {
            timestamp: Utc::now(),
            summary: TestSummary {
                total: 3,
                passed: 2,
                failed: 1,
                skipped: 0,
            },
            total_duration_ms: 1500,
            services: vec![ServiceTestResults {
                service_name: "Speech".to_string(),
                endpoint: "https://eastus.api.cognitive.microsoft.com".to_string(),
                results: vec![
                    TestResult::success("voices_list", "Get Voices List", 500),
                    TestResult::success("token_exchange", "Token Exchange", 300),
                    TestResult::failure("tts", "Text-to-Speech", 700, "Auth failed".to_string()),
                ],
                total_duration_ms: 1500,
            }],
        }
    }

    #[test]
    fn test_human_formatter() {
        let report = create_test_report();
        let formatter = HumanFormatter::new(false);
        let output = formatter.format(&report);

        assert!(output.contains("Azure AI Services"));
        assert!(output.contains("Speech"));
        assert!(output.contains("Total: 3"));
    }

    #[test]
    fn test_json_formatter() {
        let report = create_test_report();
        let formatter = JsonFormatter::new(true);
        let output = formatter.format(&report);

        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["summary"]["total"], 3);
    }

    #[test]
    fn test_junit_formatter() {
        let report = create_test_report();
        let formatter = JunitFormatter::new();
        let output = formatter.format(&report);

        assert!(output.contains("<?xml"));
        assert!(output.contains("<testsuites"));
        assert!(output.contains("<testsuite name=\"Speech\""));
    }
}
