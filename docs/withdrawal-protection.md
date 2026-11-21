# Withdrawal protection operations

Implements #107 / #108 for client #756. Contract ABI source is
`e996edf638834e2559926947a457b529ddd41821`; `.contracts-versions` is deliberately
unchanged until a reviewed release tag and deployment manifest exist. Old
`create_perp` routes still target their existing deployed generation. The new
`/withdrawals/deployment` endpoint is an explicit new-generation Safe preparation
flow, not a change to the existing wallet-pool signer.

## Set up once

1. Tag/deploy the reviewed contracts revision separately; configure the correct
   factory/modules, chain and owner Safe from that environment's manifest.
2. Review an explicit profile: finite TVL cap, small shared instant capacity,
   slow refill, and challenge window exceeding total detection/signing/execution
   time. Loss budget must cover `capacity + refill * responseTime`, summed across
   markets. Matured queued claims bypass this bucket. Accounted TVL excludes escrow.
3. POST `/withdrawals/deployment` with the admin token and explicit `beacon`,
   `owner`, `governor`, `ema_window`, `latency`, `salt`, `capacity`,
   `refill_per_second`, `challenge_window`, `tvl_cap`. Decimal monetary values are
   integer USDC-unit strings; the salt is bytes32 hex. This returns a simulated
   Safe Transaction Builder file. No defaults define production risk appetite.
4. Execute through the configured owner Safe and retain the successful
   `PerpCreated` receipt. The new Perp uses CREATE, so its address must be read
   from the receipt, not guessed from a stale simulated factory nonce.
5. Prepare/execute `lower_tvl_cap` for that Perp before listing. The factory
   initially allows unlimited TVL. A prepared creation is never ready/protected.
6. Verify `/withdrawals/status`, enroll the target and reviewed profile in the
   client registry, and retain/export proofs for all historical controllers.
   Governor `getMinDelay()` is an observation, not an audit of governance roles;
   review the governor implementation, proposer/executor permissions and upgrade
   authority separately before activation. Testnet drill precedes production.

## Incident: pause, then verify

Use `/admin/markets` Protection, select the affected markets or fleet, prepare
pause files, review each Safe batch, collect signatures and execute. `prepare`
currently requires `BEACONATOR_ADMIN_TOKEN`. The backend/CLI credential wiring
is blocked on the explicit authentication decision tracked in PR #109; their
existing access token cannot authorize preparation yet. The backend also adds
admin authentication. It never signs, broadcasts,
allocates a future nonce or proposes to the Safe service. No signer permissions
are added. All targets must validate; any failure produces no partial batch. Requests
accept up to 64 saved targets and up to 128 controllers after rotation expansion.

Check the **current on-chain Safe nonce** and pending transactions in Safe before
signing. If that nonce is occupied, explicitly replace/reject the conflict through
the Safe process; don't queue an emergency pause behind unrelated proposals.
After execution, refresh status and verify every selected controller and live
replacement is paused. Both status and preparation expand the saved registry
to include replacements; a rotation during verification fails closed.
If execution is partial across Safes, retry only the remaining authorities.
Already-paused controllers are skipped. Rotations automatically include the live
current controller as well as selected historical escrow. Zero/disabled current
controllers require separate governance investigation; future payouts may bypass
protection entirely, so a historical pause cannot be reported as fleet protection.

A target is `{perp, deployment_transaction, controller, attachment_transaction}`.
The deployment receipt must prove creation by a configured factory. For a
historical controller, supply its attachment receipt from that trusted Perp.
Binding, current owner/governor, pause/config, reserved amount, TVL and capacity
are read at one fresh block. Controllers belong to the chain configured in this
service. Persist chain ID alongside incident evidence; never mix environments.

If the web app is unavailable, use the same-environment VPC access/runbook and:

```
python3 scripts/withdrawal_protection.py pause --targets registry.json --output incident.json
python3 scripts/withdrawal_protection.py status --targets registry.json --output verified.json
```

The script consumes existing `BEACONATOR_ADDRESS` / `BEACONATOR_ACCESS_TOKEN`,
saves evidence and separate Safe-importable files, and never executes. Export and
store the registry before an incident. It works without the app database/web UI.

## Recovery is separate

`POST /withdrawals/prepare` accepts `{targets, operation}`. Operations are
`{action:"pause"}`, `{action:"extend_window",seconds:...}`,
`{action:"lower_tvl_cap",amount:"..."}` and `{action:"reopen"}`. The first three
can only restrict, through the live owner. Reopening targets the live governor;
its export is execution calldata, **not** a timelock schedule transaction. Use
that governor's reviewed schedule/wait/execute flow and verify afterward. The
API deliberately provides no claim reduction/confiscation operation.
