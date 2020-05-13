extern crate anyhow;
extern crate async_std;
extern crate log;
extern crate pest;
extern crate pest_derive;
extern crate pretty_env_logger;
extern crate serde;
extern crate serde_json;
extern crate surf;

mod parser;
mod update;

use anyhow::Error;
use async_std::fs;
use clap::Clap;
use log::{debug, info};

use parser::NSUpdateQueue;
use update::RequestQueue;

use std::env;
use std::panic;

// Constants
const PKG_LOG_LEVEL_VAR: &str = "NSUPDATE_CLOUDFLARE_LOG";
const PKG_LOG_LEVEL_DEFAULT: &str = "nsupdate_cloudflare=info";
const PKG_LOG_LEVEL_VERBOSE_1: &str = "info";
const PKG_LOG_LEVEL_VERBOSE_2: &str = "debug";
const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const PKG_DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

#[derive(Clap)]
#[clap(version = PKG_VERSION, about = PKG_DESCRIPTION)]
struct Opts {
    #[clap(short = "f", long = "file", about = "Path to nsupdate file")]
    file: String,
    #[clap(
        short = "z",
        long = "zone",
        about = "Zone ID retrieved from Cloudflare"
    )]
    zone_id: String,
    #[clap(short = "t", long = "token", about = "Token retrieved from Cloudflare")]
    token: String,
    #[clap(
        short = "v",
        long = "verbose",
        about = "Verbose level",
        parse(from_occurrences)
    )]
    verbose: i8,
}

async fn execute(input: String, zone_id: &str, token: &str) -> Result<(), Error> {
    let mut input_text = Some(input);
    let mut batch_count: usize = 0;
    let mut total: usize = 0;
    let mut total_failed: usize = 0;
    while input_text.is_some() {
        // Well, too much hassle for using recursion in async fn
        batch_count += 1;
        let mut parse_result = NSUpdateQueue::new().await;
        let remaining = parse_result.parse_text(&input_text.expect("Wut?")).await?;
        info!(
            "{} commands in batch {}",
            parse_result.len().await,
            batch_count
        );
        debug!("Parse result: {:?}", &parse_result);
        if parse_result.has_send().await {
            let request_queue = RequestQueue::from(parse_result);
            let (subtotal, subtotal_failed) = request_queue.process(zone_id, token).await?;
            total += subtotal;
            total_failed += total_failed;
            info!(
                "Batch {} Subtotal: Processed {} requests, {} failed",
                batch_count, subtotal, subtotal_failed
            );
        } else {
            info!("No \"send\" command found, nothing to do...");
        }
        input_text = remaining;
    }
    info!(
        "Processed {} requests in total, {} failed",
        total, total_failed
    );
    Ok(())
}

/// Set panic hook with repository information
fn setup_panic_hook() {
    panic::set_hook(Box::new(|panic_info: &panic::PanicInfo| {
        if let Some(info) = panic_info.payload().downcast_ref::<&str>() {
            println!("Panic occurred: {:?}", info);
        } else {
            println!("Panic occurred");
        }
        if let Some(location) = panic_info.location() {
            println!(
                r#"In file "{}" at line "{}""#,
                location.file(),
                location.line()
            );
        }
        println!(
            "Please report this panic to {}/issues",
            env!("CARGO_PKG_REPOSITORY")
        );
    }));
}

#[async_std::main]
async fn main() -> Result<(), Error> {
    // Setup panic hook
    setup_panic_hook();
    // Parse command line options
    let opts: Opts = Opts::parse();
    // Setup logger
    if env::var(PKG_LOG_LEVEL_VAR).is_err() {
        match opts.verbose {
            0 => env::set_var(PKG_LOG_LEVEL_VAR, PKG_LOG_LEVEL_DEFAULT),
            1 => env::set_var(PKG_LOG_LEVEL_VAR, PKG_LOG_LEVEL_VERBOSE_1),
            2 | _ => env::set_var(PKG_LOG_LEVEL_VAR, PKG_LOG_LEVEL_VERBOSE_2),
        }
    }
    pretty_env_logger::try_init_custom_env(PKG_LOG_LEVEL_VAR)?;
    info!("Reading nsupdate file...");
    let unparsed_file = fs::read_to_string(opts.file).await?;
    info!("Start parsing...");
    execute(unparsed_file, &opts.zone_id, &opts.token).await
}
