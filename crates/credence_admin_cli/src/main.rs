use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use soroban_client::{Options, Server};

/// CLI for Credence admin operations.
#[derive(Parser)]
#[command(
    name = "credence-admin",
    author,
    version,
    about = "Admin CLI for Credence protocol"
)]
struct Cli {
    /// Soroban RPC endpoint to connect to.
    #[arg(long, default_value = "https://soroban-testnet.stellar.org")]
    rpc_url: String,
    /// Submit the transaction instead of dry run.
    #[arg(long, action = clap::ArgAction::SetTrue, default_value = "false")]
    submit: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Set early exit configuration for a bond.
    BondSetEarlyExitConfig {
        /// The bond identifier.
        bond_id: String,
        /// Early exit threshold in basis points.
        bps: u32,
    },
    /// Set weight configuration for a bond.
    BondSetWeights { bond_id: String, weight: u32 },
    /// Set pause signer for delegation.
    DelegationSetPauseSigner {
        delegation_id: String,
        signer: String,
    },
    // Additional subcommands can be added here.
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // Connect to the Soroban RPC. `Server` is the entry point for the
    // soroban-client SDK (v0.5). Transaction assembly/submission for each
    // admin operation is intentionally left as a follow-up: it requires a
    // signing keypair, source-account lookup and contract-invocation XDR that
    // are out of scope for this scaffold.
    let server = Server::new(&cli.rpc_url, Options::default())
        .map_err(|e| anyhow!("failed to connect to RPC {}: {:?}", cli.rpc_url, e))?;

    match cli.command {
        Commands::BondSetEarlyExitConfig { bond_id, bps } => {
            handle_bond_set_early_exit(&server, &bond_id, bps, cli.submit)
        }
        Commands::BondSetWeights { bond_id, weight } => {
            handle_bond_set_weights(&server, &bond_id, weight, cli.submit)
        }
        Commands::DelegationSetPauseSigner {
            delegation_id,
            signer,
        } => handle_delegation_set_pause(&server, &delegation_id, &signer, cli.submit),
    }
}

fn report(action: &str, submit: bool) -> Result<()> {
    if submit {
        // Submitting requires building and signing the invocation transaction,
        // which is not yet wired up in this scaffold.
        Err(anyhow!(
            "submit is not yet implemented for `{action}`; rerun without --submit for a dry run"
        ))
    } else {
        println!("Dry run: would execute `{action}`");
        Ok(())
    }
}

fn handle_bond_set_early_exit(
    _server: &Server,
    bond_id: &str,
    bps: u32,
    submit: bool,
) -> Result<()> {
    report(
        &format!("bond {bond_id} set-early-exit-config bps={bps}"),
        submit,
    )
}

fn handle_bond_set_weights(
    _server: &Server,
    bond_id: &str,
    weight: u32,
    submit: bool,
) -> Result<()> {
    report(
        &format!("bond {bond_id} set-weights weight={weight}"),
        submit,
    )
}

fn handle_delegation_set_pause(
    _server: &Server,
    delegation_id: &str,
    signer: &str,
    submit: bool,
) -> Result<()> {
    report(
        &format!("delegation {delegation_id} set-pause-signer signer={signer}"),
        submit,
    )
}
