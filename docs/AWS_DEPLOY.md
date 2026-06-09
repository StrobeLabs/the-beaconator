# Deploying the-beaconator to AWS (SST)

The beaconator runs on ECS Fargate in `us-west-2`, deployed with [SST](https://sst.dev)
from `sst.config.ts` at the repo root. It follows the same conventions as the main
perpcity SST app (perpcity-client): same region, same AWS profiles, same delegated
`aws.perp.city` Route 53 zone.

Configured stages:

| Stage        | AWS account                         | Chain                     | URL                                       |
|--------------|-------------------------------------|---------------------------|-------------------------------------------|
| `testnet`    | perpcity-dev (267923960054)         | Arbitrum Sepolia (421614) | https://testnet-beaconator.aws.perp.city  |
| `production` | perpcity-production (657231015195)  | Arbitrum One (42161)      | https://beaconator.aws.perp.city          |

`sst.config.ts` refuses unknown stages by design.

IMPORTANT: deploying `production` is the CUTOVER from Railway. Both run the same
wallet keys, so two live instances signing concurrently would race nonces. Deploy
production only when traffic is being moved off the Railway mainnet instance, and
stop the Railway service promptly after.

Production also needs a one-time DNS setup before its first deploy (see
"Production DNS zone" below) — the config refuses to deploy until
`STAGES.production.dnsZone` is filled in.

## How secrets work

Sensitive values (wallet private keys, the RPC URL, API tokens) live in **AWS Secrets
Manager**, one secret per variable, named `the-beaconator/<stage>/<VAR>`:

```text
the-beaconator/testnet/RPC_URL
the-beaconator/testnet/PRIVATE_KEY
the-beaconator/testnet/WALLET_PRIVATE_KEYS
the-beaconator/testnet/BEACONATOR_ACCESS_TOKEN
the-beaconator/testnet/BEACONATOR_ADMIN_TOKEN
the-beaconator/testnet/SENTRY_DSN
```

ECS injects each value as the matching environment variable when the container
starts (`valueFrom` in the task definition), so values never appear in the task
definition, SST state, or this repo. The secrets must exist BEFORE the first deploy
of a stage: the deploy fails fast at the lookup if one is missing.

Non-secret config (contract addresses, `ENV`, the component-factory map) is plain
environment configuration in `sst.config.ts`, per stage.

Component factory addresses are passed as `COMPONENT_FACTORIES_JSON` and seeded
into Redis by the service at startup (existing Redis entries are never
overwritten). ElastiCache is VPC-internal, so this replaces the by-hand Redis
seeding used on Railway.

## Prerequisites

- AWS SSO session: `aws sso login --sso-session aws-default`
- Docker running (the deploy builds the image locally, arm64)
- `npm install` at the repo root (installs sst)
- IMPORTANT: a shell-exported `AWS_PROFILE` overrides the profile in
  `sst.config.ts`. Prefix commands with `AWS_PROFILE=perpcity-dev` (as shown
  below) or unset it.

## One-time per stage: create the secrets

Run in a terminal with SSO logged in. `read -rs` keeps values out of shell history.

```bash
export AWS_PROFILE=perpcity-dev AWS_REGION=us-west-2
S=the-beaconator/testnet

read -rs RPC_URL          # full RPC URL with API key, e.g. https://arb-sepolia.g.alchemy.com/v2/...
aws secretsmanager create-secret --name "$S/RPC_URL" --secret-string "$RPC_URL"

read -rs PRIVATE_KEY      # funding wallet key, no 0x prefix
aws secretsmanager create-secret --name "$S/PRIVATE_KEY" --secret-string "$PRIVATE_KEY"

read -rs WALLET_KEYS      # comma-separated pool keys, no 0x prefix
aws secretsmanager create-secret --name "$S/WALLET_PRIVATE_KEYS" --secret-string "$WALLET_KEYS"

read -rs ACCESS_TOKEN
aws secretsmanager create-secret --name "$S/BEACONATOR_ACCESS_TOKEN" --secret-string "$ACCESS_TOKEN"

read -rs ADMIN_TOKEN
aws secretsmanager create-secret --name "$S/BEACONATOR_ADMIN_TOKEN" --secret-string "$ADMIN_TOKEN"

# Real DSN if there is a testnet Sentry project; the literal string "disabled"
# otherwise (the service logs a warning and runs without Sentry).
aws secretsmanager create-secret --name "$S/SENTRY_DSN" --secret-string "disabled"

unset RPC_URL PRIVATE_KEY WALLET_KEYS ACCESS_TOKEN ADMIN_TOKEN
```

For `production`, the same sequence with `AWS_PROFILE=perpcity-production`,
`S=the-beaconator/production`, and mainnet values (done 2026-06-09). Reusing the
Railway tokens keeps the backend and scripts working unchanged at cutover.

## Production DNS zone (one-time, before first production deploy)

SST validates ACM certs from a hosted zone in the account it deploys to. The
production account only has the app/api child zones, so `beaconator.aws.perp.city`
needs its own delegated child zone, same pattern as the perpcity app:

```bash
# 1. Create the child zone in perpcity-production; note its Id and NS servers
AWS_PROFILE=perpcity-production aws route53 create-hosted-zone \
  --name beaconator.aws.perp.city \
  --caller-reference "beaconator-$(date +%s)" \
  --query '{Id:HostedZone.Id,NS:DelegationSet.NameServers}'

# 2. Delegate it from the aws.perp.city zone in perpcity-dev (paste the four NS
#    values from step 1 into the ResourceRecords list)
AWS_PROFILE=perpcity-dev aws route53 change-resource-record-sets \
  --hosted-zone-id Z01504801S59A4HQBX4TS \
  --change-batch '{
    "Changes": [{
      "Action": "CREATE",
      "ResourceRecordSet": {
        "Name": "beaconator.aws.perp.city",
        "Type": "NS",
        "TTL": 300,
        "ResourceRecords": [
          {"Value": "<ns1>"}, {"Value": "<ns2>"}, {"Value": "<ns3>"}, {"Value": "<ns4>"}
        ]
      }
    }]
  }'

# 3. Fill STAGES.production.dnsZone in sst.config.ts with the zone id from step 1
#    (the bare id, without the /hostedzone/ prefix)
```

## Deploy

```bash
AWS_PROFILE=perpcity-dev npx sst deploy --stage testnet

# Production - this is the Railway cutover, see the warning at the top
AWS_PROFILE=perpcity-production npx sst deploy --stage production
```

First deploy takes a while (VPC + NAT + ElastiCache + ACM cert validation + the
Rust release build). Subsequent deploys only rebuild the image and roll the
service.

To preview changes without applying: `AWS_PROFILE=perpcity-dev npx sst diff --stage testnet`

## Verify

```bash
# Health / endpoint list (unauthenticated)
curl -s https://testnet-beaconator.aws.perp.city/ | head

# Component factories seeded from COMPONENT_FACTORIES_JSON (authenticated)
curl -s -H "Authorization: Bearer $ACCESS_TOKEN" \
  https://testnet-beaconator.aws.perp.city/component_factories
```

Logs land in a CloudWatch log group under `/sst/cluster/` in us-west-2:

```bash
export AWS_PROFILE=perpcity-dev AWS_REGION=us-west-2
LOG_GROUP=$(aws logs describe-log-groups \
  --query 'logGroups[?contains(logGroupName, `Beaconator`)].logGroupName' --output text)
aws logs tail --follow "$LOG_GROUP"
```

## Rotate a secret

```bash
export AWS_PROFILE=perpcity-dev AWS_REGION=us-west-2
read -rs NEW_VALUE
aws secretsmanager put-secret-value --secret-id the-beaconator/testnet/PRIVATE_KEY --secret-string "$NEW_VALUE"
unset NEW_VALUE
```

ECS reads secrets at container start, so roll the service to pick up the new value:

```bash
AWS_PROFILE=perpcity-dev aws ecs update-service --force-new-deployment --region us-west-2 \
  --cluster $(AWS_PROFILE=perpcity-dev aws ecs list-clusters --region us-west-2 --query 'clusterArns[?contains(@, `the-beaconator-testnet`)]' --output text) \
  --service $(AWS_PROFILE=perpcity-dev aws ecs list-services --region us-west-2 --cluster $(AWS_PROFILE=perpcity-dev aws ecs list-clusters --region us-west-2 --query 'clusterArns[?contains(@, `the-beaconator-testnet`)]' --output text) --query 'serviceArns[0]' --output text)
```

(or just `npx sst deploy --stage testnet` again).

## Tear down a stage

```bash
AWS_PROFILE=perpcity-dev npx sst remove --stage testnet
```

Secrets Manager secrets are not managed by SST and survive a remove; delete them
explicitly if you mean it:

```bash
aws secretsmanager delete-secret --secret-id the-beaconator/testnet/PRIVATE_KEY --recovery-window-in-days 7
```
