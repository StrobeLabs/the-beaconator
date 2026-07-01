//! kms-wallet: generate and inspect AWS KMS secp256k1 Ethereum signing keys.
//!
//! The private key is generated inside AWS KMS and can never be exported. Operators
//! get the derived Ethereum address (to fund the wallet) and services sign via IAM
//! `kms:Sign`, but no API returns the key material. This is the replacement for
//! plaintext private keys stored in AWS Secrets Manager.
//!
//! Usage:
//!   kms-wallet create  --stage <stage> --role <role>   # make a key + alias, print address
//!   kms-wallet address --key <alias|id|arn>            # print address for an existing key
//!
//! AWS credentials/region are resolved via the standard chain; override with --profile
//! (e.g. --profile perpcity-dev) and/or --region.

use alloy::primitives::Address;
use alloy::signers::{Signer, aws::AwsSigner};
use aws_config::BehaviorVersion;
use aws_sdk_kms::Client;
use aws_sdk_kms::types::{KeySpec, KeyUsageType, Tag};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "kms-wallet",
    about = "Generate/inspect AWS KMS secp256k1 Ethereum signing keys"
)]
struct Cli {
    /// AWS profile to use (falls back to the default credential chain / AWS_PROFILE).
    #[arg(long, global = true)]
    profile: Option<String>,
    /// AWS region (falls back to the default chain / AWS_REGION).
    #[arg(long, global = true)]
    region: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new KMS secp256k1 key + alias and print its Ethereum address.
    Create {
        /// Deployment stage, used in the alias + tags (e.g. bot-dev | testnet | production).
        #[arg(long)]
        stage: String,
        /// Role for this key, used in the alias + tags (e.g. beaconator-signer).
        #[arg(long)]
        role: String,
    },
    /// Print the Ethereum address for an existing key (by alias, key id, or ARN).
    Address {
        /// Key identifier: alias (alias/...), key id, or key ARN.
        #[arg(long)]
        key: String,
    },
    /// List signing keys by alias prefix, with their Ethereum addresses.
    List {
        /// Alias prefix to match.
        #[arg(long, default_value = "alias/perpcity/")]
        prefix: String,
    },
}

/// The canonical alias for a (stage, role) KMS key: `alias/perpcity/<stage>/<role>`.
fn alias_name(stage: &str, role: &str) -> String {
    format!("alias/perpcity/{stage}/{role}")
}

/// Whether `alias` already exists in this account/region (paginates all aliases).
async fn alias_exists(client: &Client, alias: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let mut pages = client.list_aliases().into_paginator().send();
    while let Some(page) = pages.next().await {
        if page?
            .aliases()
            .iter()
            .any(|entry| entry.alias_name() == Some(alias))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let client = kms_client(cli.profile.as_deref(), cli.region.as_deref()).await;

    match cli.command {
        Command::Create { stage, role } => create(&client, &stage, &role).await?,
        Command::Address { key } => println!("{}", derive_address(&client, &key).await?),
        Command::List { prefix } => list(&client, &prefix).await?,
    }
    Ok(())
}

/// List keys whose alias starts with `prefix`, printing address, alias, and
/// target key id per line. This is the operator view of what services discover
/// at startup via WALLET_KMS_ALIAS_PREFIX.
async fn list(client: &Client, prefix: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut found = 0usize;
    let mut pages = client.list_aliases().into_paginator().send();
    while let Some(page) = pages.next().await {
        for entry in page?.aliases() {
            let Some(name) = entry.alias_name() else {
                continue;
            };
            if !name.starts_with(prefix) {
                continue;
            }
            let Some(key_id) = entry.target_key_id() else {
                continue;
            };
            // Derive via the ALIAS, mirroring how services resolve keys at
            // startup - alias-scoped IAM (kms:RequestAlias) permits this even
            // when direct key-id access would be denied.
            let address = derive_address(client, name).await?;
            println!("{address}  {name}  {key_id}");
            found += 1;
        }
    }
    if found == 0 {
        println!("no keys match prefix {prefix}");
    }
    Ok(())
}

/// Build a KMS client from the default AWS chain, optionally overriding profile/region.
async fn kms_client(profile: Option<&str>, region: Option<&str>) -> Client {
    let mut loader = aws_config::defaults(BehaviorVersion::latest());
    if let Some(profile) = profile {
        loader = loader.profile_name(profile);
    }
    if let Some(region) = region {
        loader = loader.region(aws_config::Region::new(region.to_string()));
    }
    Client::new(&loader.load().await)
}

/// Create a non-exportable secp256k1 signing key, alias it, and print its address.
async fn create(
    client: &Client,
    stage: &str,
    role: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let alias = alias_name(stage, role);

    // Fail before creating a key if the alias is already taken, so we never leak an
    // orphaned (unaliased) KMS key.
    if alias_exists(client, &alias).await? {
        return Err(format!(
            "alias {alias} already exists; choose a different --stage/--role or remove the existing alias first"
        )
        .into());
    }

    let out = client
        .create_key()
        .key_spec(KeySpec::EccSecgP256K1)
        .key_usage(KeyUsageType::SignVerify)
        .description(format!("perpcity {stage} {role} Ethereum signing key"))
        .tags(
            Tag::builder()
                .tag_key("app")
                .tag_value("perpcity")
                .build()?,
        )
        .tags(Tag::builder().tag_key("stage").tag_value(stage).build()?)
        .tags(Tag::builder().tag_key("role").tag_value(role).build()?)
        .send()
        .await?;

    let meta = out
        .key_metadata()
        .ok_or("CreateKey returned no key metadata")?;
    let key_id = meta.key_id().to_string();
    let key_arn = meta
        .arn()
        .ok_or("CreateKey returned no key ARN")?
        .to_string();

    // If aliasing fails after the key exists, surface the orphaned key id rather than
    // leaking it silently - the operator can alias or schedule-delete it.
    if let Err(err) = client
        .create_alias()
        .alias_name(&alias)
        .target_key_id(&key_id)
        .send()
        .await
    {
        return Err(format!(
            "created key {key_id} ({key_arn}) but failed to alias it {alias}: {err}; \
             the key is orphaned - alias or schedule-delete it manually"
        )
        .into());
    }

    let address = derive_address(client, &key_id).await?;

    println!("address:  {address}");
    println!("alias:    {alias}");
    println!("key_arn:  {key_arn}");
    println!("key_id:   {key_id}");
    Ok(())
}

/// Derive the Ethereum address for a KMS key. `AwsSigner::new` calls `kms:GetPublicKey`
/// and computes the address; the private key never leaves KMS.
async fn derive_address(client: &Client, key: &str) -> Result<Address, Box<dyn std::error::Error>> {
    let signer = AwsSigner::new(client.clone(), key.to_string(), None).await?;
    Ok(signer.address())
}

#[cfg(test)]
mod tests {
    use super::alias_name;

    #[test]
    fn alias_follows_perpcity_convention() {
        assert_eq!(
            alias_name("testnet", "beaconator-signer"),
            "alias/perpcity/testnet/beaconator-signer"
        );
        assert_eq!(
            alias_name("bot-dev", "smoke"),
            "alias/perpcity/bot-dev/smoke"
        );
    }
}
