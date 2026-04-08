use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    process,
};

use clap::{Parser, Subcommand};

use compiler_rs::{
    AnalyzeRequest, AnalyzeResponse, CompilerPolicy, analyze_request, analyze_request_file,
    build_regression_viewer_data_from_dir, default_regression_dir, run_demo, run_regression_suite,
    run_regression_suite_from_dir,
};

#[derive(Debug, Parser)]
#[command(name = "compiler-rs")]
#[command(about = "Rust semantic kernel for line/group event analysis")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Demo,
    DemoJson,
    Regress { dir: Option<PathBuf> },
    RegressJson { dir: Option<PathBuf> },
    ViewerJson { dir: Option<PathBuf> },
    AnalyzeFile { path: PathBuf },
    AnalyzeStdin,
}

fn main() {
    let cli = Cli::parse();
    let policy = CompilerPolicy::default();

    match cli.command.unwrap_or(Command::Demo) {
        Command::Demo => match run_demo(&policy) {
            Ok(outputs) => {
                for output in outputs {
                    println!("{}", output.to_json_string());
                }
            }
            Err(err) => exit_with_error(&format!("demo failed: {err}"), 1),
        },
        Command::DemoJson => match run_demo(&policy) {
            Ok(outputs) => print_pretty_json(&outputs),
            Err(err) => exit_with_error(&format!("demo-json failed: {err}"), 1),
        },
        Command::Regress { dir } => match run_regression(dir.as_deref(), &policy) {
            Ok(report) => {
                println!("{}", report.render_text());
                if !report.failed.is_empty() {
                    process::exit(1);
                }
            }
            Err(err) => {
                let location = dir
                    .unwrap_or_else(default_regression_dir)
                    .display()
                    .to_string();
                exit_with_error(
                    &format!("regression failed to load cases from {location}: {err}"),
                    1,
                )
            }
        },
        Command::RegressJson { dir } => match run_regression(dir.as_deref(), &policy) {
            Ok(report) => print_pretty_json(&report),
            Err(err) => exit_with_error(&format!("regress-json failed: {err}"), 1),
        },
        Command::ViewerJson { dir } => match run_viewer(dir.as_deref(), &policy) {
            Ok(data) => print_pretty_json(&data),
            Err(err) => exit_with_error(&format!("viewer-json failed: {err}"), 1),
        },
        Command::AnalyzeFile { path } => match analyze_request_file(path.as_path(), &policy) {
            Ok(response) => print_analyze_response(&response),
            Err(err) => exit_with_error(&format!("analyze-file failed: {err}"), 1),
        },
        Command::AnalyzeStdin => match parse_analyze_request(&read_stdin_or_exit()) {
            Ok(request) => {
                let response = analyze_request(&request, &policy);
                print_analyze_response(&response);
            }
            Err(err) => exit_with_error(&format!("analyze-stdin failed: {err}"), 1),
        },
    }
}

fn parse_analyze_request(input: &str) -> Result<AnalyzeRequest, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(input)?)
}

fn run_regression(
    dir: Option<&Path>,
    policy: &CompilerPolicy,
) -> Result<compiler_rs::RegressionReport, Box<dyn std::error::Error>> {
    match dir {
        Some(path) => run_regression_suite_from_dir(policy, path),
        None => run_regression_suite(policy),
    }
}

fn run_viewer(
    dir: Option<&Path>,
    policy: &CompilerPolicy,
) -> Result<compiler_rs::RegressionViewerData, Box<dyn std::error::Error>> {
    match dir {
        Some(path) => build_regression_viewer_data_from_dir(policy, path),
        None => build_regression_viewer_data_from_dir(policy, &default_regression_dir()),
    }
}

fn print_analyze_response(response: &AnalyzeResponse) {
    match response.to_pretty_json() {
        Ok(json) => println!("{json}"),
        Err(err) => exit_with_error(&format!("failed to render analyze response: {err}"), 1),
    }
}

fn print_pretty_json<T: serde::Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{json}"),
        Err(err) => exit_with_error(&format!("failed to render json: {err}"), 1),
    }
}

fn read_stdin_or_exit() -> String {
    let mut input = String::new();
    if let Err(err) = io::stdin().read_to_string(&mut input) {
        exit_with_error(&format!("failed to read stdin: {err}"), 1);
    }
    input
}

fn exit_with_error(message: &str, code: i32) -> ! {
    eprintln!("{message}");
    process::exit(code);
}
