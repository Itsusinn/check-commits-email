use anyhow::{Ok, Result};
use clap::Parser;
use hickory_resolver::{
    Resolver,
    config::{ResolverConfig, ResolverOpts},
};
use regex::Regex;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::LazyLock,
};

#[derive(Parser, Debug)]
#[command(
    name = "check-commits",
    version = "0.1.0",
    about = "Git commit email validator",
    long_about = "Validate git commit emails against wildcard rules"
)]
struct Args {
    /// Path to email blacklist file
    #[arg(short, long)]
    rules: PathBuf,

    /// Path to commit emails file
    #[arg(short, long)]
    emails: PathBuf,

    /// Output format (text|github)
    #[arg(short, long, default_value = "text")]
    output: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    run(args)?;
    Ok(())
}

fn run(args: Args) -> Result<Vec<String>> {
    let bad_rules = read_rules(&args.rules)?;
    let commit_emails = read_emails(&args.emails)?;

    let regex_rules = compile_rules(bad_rules);

    let violations = find_violations(commit_emails, regex_rules);

    match args.output.as_str() {
        "github" => output_github(violations.iter().collect()),
        _ => output_text(violations.iter().collect()),
    }

    Ok(violations)
}
#[cfg(test)]
mod test {
    use crate::{Args, run};

    #[test]
    fn test_1() {
        let arg = Args {
            rules: "test-rules.txt".into(),
            emails: "test-emails-1.txt".into(),
            output: "text".into(),
        };
        let violations = run(arg).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations.first().unwrap(), "abc@hotmail.com")
    }

    #[test]
    fn test_2() {
        let arg = Args {
            rules: "test-rules.txt".into(),
            emails: "test-emails-2.txt".into(),
            output: "text".into(),
        };
        let violations = run(arg).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations.first().unwrap(), "1245@foxmail.com")
    }

    #[test]
    fn test_3() {
        let arg = Args {
            rules: "test-rules.txt".into(),
            emails: "test-emails-3.txt".into(),
            output: "text".into(),
        };
        let violations = run(arg).unwrap();
        assert_eq!(violations.len(), 0);
    }

    #[test]
    fn test_4() {
        let arg = Args {
            rules: "test-mx-record.txt".into(),
            emails: "test-emails-4.txt".into(),
            output: "text".into(),
        };
        let violations = run(arg).unwrap();
        assert_eq!(violations.len(), 1);
    }
}

fn read_rules(path: impl AsRef<Path>) -> Result<HashSet<String>> {
    Ok(fs::read_to_string(path)?
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|s| s.to_string())
        .collect())
}

fn read_emails(path: impl AsRef<Path>) -> Result<HashSet<String>> {
    Ok(fs::read_to_string(path)?
        .lines()
        .map(|s| s.to_string())
        .collect())
}

enum Rule {
    Regex(Regex),
    MxRecord(String),
}

impl Rule {
    fn is_match(&self, email: &str) -> Result<bool> {
        static RESOLVER: LazyLock<Resolver> = LazyLock::new(|| {
            Resolver::new(ResolverConfig::default(), ResolverOpts::default()).unwrap()
        });
        match self {
            Rule::Regex(regex) => Ok(regex.is_match(email)),
            Rule::MxRecord(record) => {
                if let Some(host) = email.split('@').next_back() {
                    Ok(RESOLVER.mx_lookup(host)?.into_iter().any(|v| {
                        let mut str = v.exchange().to_ascii();
                        if str.ends_with('.') {
                            str.remove(str.len() - 1);
                        }
                        &str == record
                    }))
                } else {
                    Ok(false)
                }
            }
        }
    }
}

fn compile_rules(bad_rules: HashSet<String>) -> Vec<Rule> {
    bad_rules
        .into_iter()
        .filter_map(|rule| {
            if rule.starts_with("MX-RECORD,") {
                match rule.split(",").last() {
                    Some(v) => Some(Rule::MxRecord(v.into())),
                    None => {
                        eprintln!("Invalid rule {rule}");
                        None
                    }
                }
            } else {
                let pattern = rule.trim().replace(".", r"\.").replace("*", ".*");
                Regex::new(&format!(r"(?i)^{}", pattern))
                    .map_err(|e| eprintln!("Invalid rule '{}': {}", rule, e))
                    .map(Rule::Regex)
                    .ok()
            }
        })
        .collect()
}

fn find_violations(commit_emails: HashSet<String>, regex_rules: Vec<Rule>) -> Vec<String> {
    let mut violations: Vec<_> = commit_emails
        .iter()
        .filter(|email| {
            regex_rules
                .iter()
                .any(|re| re.is_match(email).unwrap_or(false))
        })
        .cloned()
        .collect();

    violations.sort_unstable();
    violations
}

fn output_github(violations: Vec<&String>) {
    if violations.is_empty() {
        println!("has_violations=false");
    } else {
        // convert to GitHub Actions format
        let formatted = violations
            .iter()
            .map(|s| format!("• {}", s)) // Markdown lists
            .collect::<Vec<_>>()
            .join("%0A"); // Github multiline string

        println!("has_violations=true");
        println!("violations={}", formatted);
    }
}

fn output_text(violations: Vec<&String>) {
    if violations.is_empty() {
        println!("✅ All submitted email addresses meet the requirements");
    } else {
        println!(
            "❌ {} violating email address(es) detected:",
            violations.len()
        );
        for (i, email) in violations.iter().enumerate() {
            println!("  {}. {}", i + 1, email);
        }
    }
}
