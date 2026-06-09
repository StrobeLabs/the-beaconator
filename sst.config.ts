/// <reference path="./.sst/platform/config.d.ts" />

/**
 * the-beaconator on AWS — ECS Fargate via SST, mirroring the conventions of the
 * main perpcity SST app (perpcity-client/sst.config.ts): same region (us-west-2),
 * same AWS profiles, same Cluster/Service/Redis shapes, same delegated
 * aws.perp.city Route 53 zone for HTTPS.
 *
 * Differences from the perpcity app, and why:
 *
 *  1. Secrets live in AWS Secrets Manager, NOT `sst.Secret`. The beaconator holds
 *     wallet PRIVATE KEYS; Secrets Manager gives KMS encryption at rest, IAM-scoped
 *     reads, an audit trail, and rotation — none of which the SST state-bucket
 *     secrets provide. Values are injected by ECS at container start via
 *     `valueFrom` (the `ssm` prop below), so they never appear in the task
 *     definition, the SST state, or this file. Secrets must be created BEFORE the
 *     first deploy of a stage (see docs/AWS_DEPLOY.md for the exact commands):
 *       the-beaconator/<stage>/RPC_URL
 *       the-beaconator/<stage>/PRIVATE_KEY
 *       the-beaconator/<stage>/WALLET_PRIVATE_KEYS
 *       the-beaconator/<stage>/BEACONATOR_ACCESS_TOKEN
 *       the-beaconator/<stage>/BEACONATOR_ADMIN_TOKEN
 *       the-beaconator/<stage>/SENTRY_DSN          (value may be empty)
 *
 *  2. Own VPC + own Redis, no coupling to the perpcity app's stacks. The only
 *     consumer (perpcity-app-backend) talks to the beaconator over its public
 *     HTTPS domain with a bearer token, exactly as it did on Railway, so nothing
 *     needs VPC-level reachability. Default 10.0/16 CIDR is fine: this VPC is
 *     never peered (the Tiger Cloud peering constraint in the perpcity app does
 *     not apply here).
 *
 *  3. Component factory addresses ship as COMPONENT_FACTORIES_JSON and are seeded
 *     into Redis by the service at startup (src/lib.rs). On Railway this seeding
 *     was done by hand against a publicly reachable Redis; ElastiCache is
 *     VPC-internal, so the seed has to ride in with the app.
 *
 * Stage → account/chain:
 *   testnet    → perpcity-dev (267923960054), Arbitrum Sepolia (ENV=testnet),
 *                domain testnet-beaconator.aws.perp.city
 *   production → perpcity-production (657231015195), Arbitrum One (ENV=mainnet),
 *                domain beaconator.aws.perp.city. Config is wired but DEPLOYING
 *                IT IS THE CUTOVER from Railway: both run the same wallet keys,
 *                so two live instances signing concurrently would race nonces.
 *                Deploy production only when traffic is being moved off Railway.
 */
export default $config({
  app(input) {
    return {
      name: "the-beaconator",
      home: "aws",
      removal: input?.stage === "production" ? "retain" : "remove",
      protect: input?.stage === "production",
      providers: {
        aws: {
          region: "us-west-2",
          profile:
            input?.stage === "production"
              ? "perpcity-production"
              : "perpcity-dev",
        },
      },
    };
  },
  async run() {
    // Per-stage chain config. Addresses are the deployments of beacons@v0.0.1 +
    // perpcity-contracts@v0.1.0 (the tags pinned in .contracts-versions) on each
    // chain. Mainnet factory addresses were verified byte-for-byte identical to
    // the Sepolia deployments (same deployer, nonces 0-19, exact runtime
    // bytecode including metadata hash) and against the official address list.
    const STAGES: Record<
      string,
      {
        env: "mainnet" | "testnet";
        domain: string;
        dnsZone: string;
        addresses: Record<string, string>;
        componentFactories: Record<string, string>;
        // Stage-specific plain env overrides (e.g. transfer limits).
        extraEnvironment?: Record<string, string>;
      }
    > = {
      testnet: {
        env: "testnet",
        // Delegated child zone aws.perp.city lives in perpcity-dev; SST manages
        // the record + ACM cert from it (same zone the perpcity app uses).
        domain: "testnet-beaconator.aws.perp.city",
        dnsZone: "Z01504801S59A4HQBX4TS",
        addresses: {
          // beacons@v0.0.1 (Arbitrum Sepolia). Registry address cross-checked
          // against perpcity-indexer/config/arbitrum-sepolia.json and on-chain
          // (owner matches moduleRegistry/protocolFeeManager, created at the
          // indexer's startBlock).
          PERPCITY_REGISTRY_ADDRESS: "0xAA0B2AB577D75bC8ED5380752b612a04896d6f10",
          ECDSA_VERIFIER_FACTORY_ADDRESS:
            "0x47978f1AB8911064B2979aB0e9E90152c1d916c0",
          // perpcity-contracts@v0.1.0 (Arbitrum Sepolia)
          PERP_FACTORY_ADDRESS: "0xa54F81e7BD5C0d52d6fdE2ba40d0B1123d53E7a7",
          FEES_MODULE_ADDRESS: "0xa8f7453Fc54E4578dDde12e3DC65b7614938a620",
          FUNDING_MODULE_ADDRESS: "0x693852A74cF7f5936e1cfD6f3971aE3027b84e7D",
          MARGIN_RATIOS_MODULE_ADDRESS:
            "0xB118b54e8b513F6A67fD555348f216d804C0eB89",
          PRICE_IMPACT_MODULE_ADDRESS:
            "0x0168716E60f5A54e62aec8c20D3caa37e5ce61C2",
          PRICING_MODULE_ADDRESS: "0xc33205B471a9529fdD0A81fdEA037737Ff5c7A97",
          USDC_ADDRESS: "0xBEF280BefeE2Cb28c20D1E4Cc1da999B4DA0f1fD",
          MODULE_REGISTRY_ADDRESS:
            "0xA8559d9Fb429D184CD06EfDF2A0F16C4a19Bb654",
          PROTOCOL_FEE_MANAGER_ADDRESS:
            "0x5bA2AbaD153bbCAe38638A51e61ea2D11CAC8D9B",
          // Canonical Multicall3, same address on every chain.
          MULTICALL3_ADDRESS: "0xcA11bde05977b3631167028862bE2a173976CA11",
        },
        // Seeded into Redis at startup (keys the modular-beacon recipes resolve
        // against). Names must match ComponentFactoryType variants in
        // src/models/component_factory.rs — note WeightedSumComponentFactory is
        // the deployment artifact called "WeightedSumFactory".
        componentFactories: {
          IdentityBeaconFactory: "0x17242F60f44f084Bb56c6Dcbd343Be9236185272",
          StandaloneBeaconFactory: "0x8097Bc34eDD7dA12f70dC1521244ace78080CCb4",
          CompositeBeaconFactory: "0xCAaE988b0bd3DD5Ab752764A75d96D4079B72263",
          GroupManagerFactory: "0x20B2B2DE4811b88f627447c3ed19A0565FFbF98D",
          IdentityPreprocessorFactory:
            "0x7d9DcBFD7517E4D33eFca2ACe2a0271D6EBb091E",
          ThresholdFactory: "0x371f649502d813728D58A934eE828d6B603a1d94",
          TernaryToBinaryFactory: "0x038180774642e95a78Cfb8ac77301Cb3A42458BD",
          ArgmaxFactory: "0xBc0F014d2d1781E22880E334066563C4c215C94d",
          CGBMFactory: "0x569AF5De8f8815a662aD4dffC6391b70ADFA9C2A",
          DGBMFactory: "0xf654c64faf8261D25b33CA8Ae3e605C5b4109AE8",
          BoundedFactory: "0x39BCC1c71f7255b993F09ECffd94f1410531210E",
          UnboundedFactory: "0x50b217ECBe804a950F5a4E9B22D877927B777548",
          WeightedSumComponentFactory:
            "0xeD221d2d6a96abf2E5D4D07783564A7dF7aEa8CD",
          DominanceFactory: "0x2927B0d72fd6C61762FCd4193943f31a12cEdb4A",
          RelativeDominanceFactory:
            "0x94595171ad68b00C6F0dcbbfFb790dE8869fd14B",
          ContinuousAllocationFactory:
            "0x746cc8Ce1a56D98dce0569bFcC4a52d754E9F9B6",
          DiscreteAllocationFactory:
            "0x63B2A3aE73570bD6369198Ce7f02aD55D6F3990c",
          SoftmaxFactory: "0xcc01D3B3A2648FF1D0F8E4cd8E5931211A472296",
          GMNormalizeFactory: "0x5Ae6Ad8b345B2371026f17f87F4Ef0C66ea5dB57",
          ECDSAVerifierFactory: "0x47978f1AB8911064B2979aB0e9E90152c1d916c0",
        },
      },
      production: {
        env: "mainnet",
        // Needs a beaconator.aws.perp.city hosted zone in perpcity-production,
        // delegated from the aws.perp.city zone in perpcity-dev (the same
        // pattern as the perpcity app's app/api.aws.perp.city child zones).
        // Fill dnsZone with the new zone id once it exists; the guard below
        // refuses to deploy production until then. Zone creation commands are
        // in docs/AWS_DEPLOY.md.
        domain: "beaconator.aws.perp.city",
        dnsZone: "",
        addresses: {
          // beacons@v0.0.1 (Arbitrum One). Same values as the working Railway
          // mainnet instance and perpcity-indexer/config/arbitrum-mainnet.json.
          PERPCITY_REGISTRY_ADDRESS: "0xBEF280BefeE2Cb28c20D1E4Cc1da999B4DA0f1fD",
          ECDSA_VERIFIER_FACTORY_ADDRESS:
            "0x687EeB3E4989441C91BF0D4F797AF4C400803c95",
          // perpcity-contracts@v0.1.0 (Arbitrum One)
          PERP_FACTORY_ADDRESS: "0xCE0c5f65A5eDa69A1dFb3f3273749B649abc4eC6",
          FEES_MODULE_ADDRESS: "0xBfda8AA80132C51995B37c03D9FE384dcbF0056E",
          FUNDING_MODULE_ADDRESS: "0xB9572a6CdD39965e2d03F13d5559e0CA9fe599D1",
          MARGIN_RATIOS_MODULE_ADDRESS:
            "0x8Afca53c52B1F02d76aefB811C6B08F4BD3e4cf9",
          PRICE_IMPACT_MODULE_ADDRESS:
            "0x71889d6B403DdC5007d1EBaBB52D1fcd5ec04832",
          PRICING_MODULE_ADDRESS: "0xF4689DA0CaC3f23a04145236Dbfe81C3c58cFe22",
          // Canonical Arbitrum One USDC (Circle), not a mock.
          USDC_ADDRESS: "0xaf88d065e77c8cC2239327C5EDb3A432268e5831",
          MODULE_REGISTRY_ADDRESS:
            "0xa8D623C214F70A6D40Cad299d1fffF9667a97965",
          PROTOCOL_FEE_MANAGER_ADDRESS:
            "0x9d0bB98fca4D4913C3851606941B52DDfdB4b9EE",
          MULTICALL3_ADDRESS: "0xcA11bde05977b3631167028862bE2a173976CA11",
          // Strobe Safe multisig (beacon registration goes through the Safe on
          // mainnet; testnet registers directly).
          SAFE_ADDRESS: "0x2D0be18386297d833E63d0B1F6bc93F391aF6F93",
        },
        // Same contracts as Sepolia, byte-for-byte (verified): deployer
        // 0x91e8a9D4...B1, creation nonces 0-19 on both chains.
        componentFactories: {
          IdentityBeaconFactory: "0xa9f0A69112b83621b0922Cb321f7B0263Ea4273A",
          StandaloneBeaconFactory: "0xAa96040DC6C0Bf47702c2131F0E03B2386d7A70C",
          CompositeBeaconFactory: "0xb8B5eFBb29eD832a99dB2D4D3dfa0b1B668D1dA3",
          GroupManagerFactory: "0xb98FF5583E57595B50AD20bc910FfCDE10C40d0e",
          IdentityPreprocessorFactory:
            "0x90bE3B12FC821470eC3a8e03067E556d87d309A2",
          ThresholdFactory: "0xF7Bb7fAa37C325374b341bEFD9fC13A6efd37e88",
          TernaryToBinaryFactory: "0x87879e7726D9D16397eaF4E316E2EBd333475EC2",
          ArgmaxFactory: "0xFc149D2F0BE9432E79fe2FA376310444Bf23A857",
          CGBMFactory: "0x656CDc258f8EF7206e16dCB382082D5249a15d46",
          DGBMFactory: "0x7Dc0686DAb43AC6B7F3999952b795c555509a155",
          BoundedFactory: "0x77b1049546620B54C810424483c855Ba8385dED7",
          UnboundedFactory: "0xEBaC70fdD7A734177D2569743FDcA08b0c6F1012",
          WeightedSumComponentFactory:
            "0x36a6B3EC337C67F7da4C5f07b24F4Ff954FA3983",
          DominanceFactory: "0x1E1D015e2d8feF63bB0641E5505027173A4Ca8B3",
          RelativeDominanceFactory:
            "0x05C0023b323138d5353018A1C350274932B8E9F6",
          ContinuousAllocationFactory:
            "0x9Af8aC20D0F643Ca1d50C9B4c5D36EA1B32943b9",
          DiscreteAllocationFactory:
            "0x468b76cF392209DE9569dFA30A6a6e944AD78b9C",
          SoftmaxFactory: "0x1F9a978468461c9B582F2e09e98fEbdB57d85158",
          GMNormalizeFactory: "0xE04Aa90DC32BEf1f56Ce9A8ED2AcE3C571438E30",
          ECDSAVerifierFactory: "0x687EeB3E4989441C91BF0D4F797AF4C400803c95",
        },
        // The live mainnet instance disables guest-wallet funding entirely.
        extraEnvironment: {
          USDC_TRANSFER_LIMIT: "0",
          ETH_TRANSFER_LIMIT: "0",
        },
      },
    };

    const stage = STAGES[$app.stage];
    if (!stage) {
      throw new Error(
        `Stage "${$app.stage}" is not configured. Known stages: ${Object.keys(
          STAGES,
        ).join(", ")}. Add a chain-config entry to STAGES in sst.config.ts ` +
          `(and create its Secrets Manager secrets) before deploying.`,
      );
    }
    if (!stage.dnsZone) {
      throw new Error(
        `Stage "${$app.stage}" has no dnsZone configured. Create the hosted ` +
          `zone for ${stage.domain} (plus its NS delegation) and fill ` +
          `STAGES.${$app.stage}.dnsZone in sst.config.ts — see docs/AWS_DEPLOY.md.`,
      );
    }
    const isProd = $app.stage === "production";

    // NAT for egress (ECR pulls + chain RPC). Mirrors the perpcity app: managed
    // NAT gateway on production (zero-ops), fck-nat EC2 elsewhere (~10x cheaper).
    const vpc = new sst.aws.Vpc("Vpc", { nat: isProd ? "managed" : "ec2" });

    // Wallet pool locks + the component-factory/recipe registries. Single-node
    // Valkey (t4g.micro), matching the service's non-cluster Redis client.
    // ElastiCache enables TLS + auth by default, hence the rediss:// URL below
    // and the tokio-rustls-comp feature on the redis crate in Cargo.toml.
    const redis = new sst.aws.Redis("Redis", {
      vpc,
      engine: "valkey",
      cluster: false,
      // Same gotcha the perpcity app hit: SST writes a `cluster-enabled`
      // parameter that AWS rejects when creating a FRESH Valkey parameter group.
      // This stage's group is created from scratch, so strip the parameter.
      transform: {
        parameterGroup: (args) => {
          args.parameters = [];
        },
      },
    });
    const redisUrl = $interpolate`rediss://${redis.username}:${redis.password}@${redis.host}:${redis.port}`;

    // Pre-created Secrets Manager secrets (see header). getSecretOutput fails the
    // deploy with a clear error if one is missing — intentional: secrets first,
    // deploy second.
    const secretNames = [
      "RPC_URL",
      "PRIVATE_KEY",
      "WALLET_PRIVATE_KEYS",
      "BEACONATOR_ACCESS_TOKEN",
      "BEACONATOR_ADMIN_TOKEN",
      "SENTRY_DSN",
    ] as const;
    const secretArns = Object.fromEntries(
      secretNames.map((name) => [
        name,
        aws.secretsmanager.getSecretOutput({
          name: `the-beaconator/${$app.stage}/${name}`,
        }).arn,
      ]),
    );

    const cluster = new sst.aws.Cluster("Cluster", { vpc });

    const service = new sst.aws.Service("Beaconator", {
      cluster,
      image: { context: "." },
      // Fargate Graviton; the Dockerfile is arch-agnostic (rust:bookworm +
      // debian:bookworm-slim) and builds natively on Apple Silicon.
      architecture: "arm64",
      cpu: "0.5 vCPU",
      memory: "1 GB",
      // Spot on non-prod: writes are short-lived txs, wallet locks expire (60s
      // TTL), and every operation is retryable by the caller. On-demand on
      // production so a Spot reclaim can't interrupt a live signing flow.
      ...(isProd ? {} : { capacity: "spot" as const }),
      // Exactly one task: the wallet pool serializes writes through Redis locks
      // either way, and one instance keeps tx nonce behavior simple.
      scaling: { min: 1, max: 1 },
      logging: { retention: isProd ? "3 months" : "1 month" },
      // ECS injects each secret's value as the named env var at container start
      // (task execution role reads them; SST wires that IAM up from `ssm`).
      ssm: secretArns,
      loadBalancer: {
        domain: {
          name: stage.domain,
          dns: sst.aws.dns({ zone: stage.dnsZone }),
        },
        rules: [
          { listen: "443/https", forward: "8000/http" },
          { listen: "80/http", redirect: "443/https" },
        ],
        // GET / is unauthenticated and returns 200 once Rocket is up (it only
        // reports endpoint info); the container has no curl, so the ALB check
        // is the health check.
        health: {
          "8000/http": { path: "/", interval: "30 seconds" },
        },
      },
      environment: {
        // testnet => Arbitrum Sepolia (421614), mainnet => Arbitrum One (42161).
        ENV: stage.env,
        PORT: "8000",
        REDIS_URL: redisUrl,
        RUST_LOG: "info,the_beaconator=info,rocket=warn",
        BEACONATOR_INSTANCE_ID: `aws-${$app.stage}`,
        COMPONENT_FACTORIES_JSON: JSON.stringify(stage.componentFactories),
        ...stage.addresses,
        ...(stage.extraEnvironment ?? {}),
      },
    });

    return {
      url: `https://${stage.domain}`,
      loadBalancer: service.url,
      redisHost: redis.host,
      vpc: vpc.id,
    };
  },
});
