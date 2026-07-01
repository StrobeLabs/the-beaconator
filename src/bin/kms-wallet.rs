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
}

/// The canonical alias for a (stage, role) KMS key: `alias/perpcity/<stage>/<role>`.
fn alias_name(stage: &str, role: &str) -> String {
    format!("alias/perpcity/{stage}/{role}")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let client = kms_client(cli.profile.as_deref(), cli.region.as_deref()).await;

    match cli.command {
        Command::Create { stage, role } => create(&client, &stage, &role).await?,
        Command::Address { key } => println!("{}", derive_address(&client, &key).await?),
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
    let key_arn = meta.arn().unwrap_or_default().to_string();

    client
        .create_alias()
        .alias_name(&alias)
        .target_key_id(&key_id)
        .send()
        .await?;

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
