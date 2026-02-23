# DynamoDB Config Storage

DynamoDB backend for Tenrankai's ConfigStorage, designed for hosted/multi-instance
deployments where multiple servers need to share configuration with safe concurrent access.

## Why DynamoDB?

The existing backends have limitations for multi-instance deployments:

- **FileDir** (local TOML files) — single-machine only, no sharing between instances
- **Storage/S3** (JSON files) — works across instances but has read-modify-write races
  on concurrent updates

DynamoDB provides atomic writes per item, so concurrent admin changes from different
instances never corrupt each other. Combined with shard-based version polling, all
instances converge on the same configuration automatically.

## Table Schema

Single-table design with composite keys:

| Entity | PK (partition key) | SK (sort key) | Notes |
|---|---|---|---|
| Site config | `SITE#{site}` | `CONFIG` | JSON `data` attribute |
| Gallery | `SITE#{site}` | `GALLERY#{name}` | JSON `data` attribute |
| Posts | `SITE#{site}` | `POSTS#{name}` | JSON `data` attribute |
| Permissions | `SITE#{site}` | `PERMISSIONS` | JSON `data` attribute |
| Shard version | `SHARD#{shard_id}` | `VERSION` | Numeric `version` attribute |
| Shard membership | `SHARD#{shard_id}` | `SITE#{site}` | Marker item (no data) |
| Audit entry | `AUDIT` | `{iso8601_timestamp}#{uuid}` | JSON `data` attribute |

All config data is stored as a JSON string in a `data` attribute. No GSIs are needed —
all access patterns are served by the primary key.

### Key design principles

**Site keys are independent of shards.** All site data lives under `PK=SITE#{site}`
regardless of which shard the site belongs to. Any server instance can read/write any
site's data. The shard only affects which version counter gets bumped on mutations and
which membership markers exist.

**Shards group sites for efficient version polling.** Instead of scanning the entire
table for version items, the poll loop does a single `GetItem` on the shard's version
counter — O(1) regardless of how many sites exist.

**Shard membership is tracked.** When a site config is created or updated, a marker
item (`PK=SHARD#{shard_id}, SK=SITE#{site}`) is written alongside the version counter.
This provides operational visibility into which sites belong to which shard.

## Shards

A **shard** groups multiple sites under a single version counter for efficient
change detection. All server instances that serve the same set of sites should use the
same shard.

### How shards work

1. Each `DynamoConfigStorage` instance has a `shard_id` (configured via URL parameter,
   defaults to `"default"`)
2. Every config mutation (set/delete site, gallery, posts, permissions) atomically
   increments `PK=SHARD#{shard_id}, SK=VERSION`
3. The poll loop checks the shard version with a single `GetItem` call
4. If the version hasn't changed since last check, the full reload is skipped
5. If the version changed, a full differential reload is triggered

### When to use multiple shards

**Single shard (default)** — sufficient for most deployments. All sites share one
version counter. Any config change triggers all instances to reload. Since the reload
is differential (only changed sites are rebuilt), this is efficient even with many sites.

**Multiple shards** — useful when you have distinct groups of sites served by different
instance pools. For example:

```
Pool A (instances 1-3): serves sites "alpha", "beta"   → shard=pool-a
Pool B (instances 4-6): serves sites "gamma", "delta"  → shard=pool-b
```

Changes to "alpha" only trigger reloads in Pool A. Pool B instances skip the check
because their shard version hasn't changed.

### Configuration

The shard is specified as a URL query parameter:

```toml
[app]
# Default shard (omit parameter or use shard=default)
config_storage = "dynamodb://tenrankai-config?region=us-west-2"

# Named shard
config_storage = "dynamodb://tenrankai-config?region=us-west-2&shard=pool-a"
```

**All instances serving the same sites MUST use the same shard.** If instance A uses
`shard=x` and instance B uses `shard=y`, changes made via A will bump shard x's version.
Instances polling shard y will not detect the change.

## Setup

### 1. Build with DynamoDB support

```bash
cargo build --features config-storage-dynamodb
# or with all features:
cargo build --features config-storage-dynamodb,users-dynamodb
```

### 2. Create the DynamoDB table

#### Using Terraform (recommended)

```hcl
module "config_storage" {
  source = "github.com/theatrus/tenrankai//terraform/modules/config-storage-dynamodb"

  table_name                    = "tenrankai-config"
  enable_point_in_time_recovery = true

  tags = {
    Environment = "production"
  }
}

# Attach the access policy to your ECS task role / EC2 instance profile
resource "aws_iam_role_policy_attachment" "tenrankai_config" {
  role       = aws_iam_role.tenrankai_task.name
  policy_arn = module.config_storage.access_policy_arn
}

# Use the output in your Tenrankai config
output "config_storage_url" {
  value = module.config_storage.config_storage_url
}
```

#### Using AWS CLI

```bash
aws dynamodb create-table \
  --table-name tenrankai-config \
  --attribute-definitions \
    AttributeName=pk,AttributeType=S \
    AttributeName=sk,AttributeType=S \
  --key-schema \
    AttributeName=pk,KeyType=HASH \
    AttributeName=sk,KeyType=RANGE \
  --billing-mode PAY_PER_REQUEST \
  --region us-west-2
```

### 3. Configure Tenrankai

In `config.toml`:

```toml
[app]
config_storage = "dynamodb://tenrankai-config?region=us-west-2"
config_reload_interval_seconds = 30
```

For local development with [DynamoDB Local](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/DynamoDBLocal.html):

```toml
[app]
config_storage = "dynamodb://tenrankai-config?region=us-west-2&endpoint=http%3A%2F%2Flocalhost%3A8000"
```

### 4. Initialize sites

Use the CLI to create your first site:

```bash
cargo run -- config add-site default --hostname example.com
cargo run -- config add-gallery photos --site default --source photos --url-prefix /gallery
```

## Multi-Instance Config Sync

When multiple Tenrankai instances share a DynamoDB table, changes made on one
instance need to propagate to others. There are three mechanisms:

### Periodic Polling (recommended for most deployments)

Add `config_reload_interval_seconds` to `config.toml`:

```toml
[app]
config_storage = "dynamodb://tenrankai-config?region=us-west-2"
config_reload_interval_seconds = 30
```

The poll loop works as follows:

1. Every N seconds, check the shard version counter (`GetItem` — single read)
2. If the version matches the cached value, skip the reload (no-op)
3. If the version changed, perform a full differential reload:
   - List all sites from DynamoDB
   - Compare against currently loaded sites
   - Add, update, or remove sites as needed
4. Cache the new version for the next poll cycle

For backends without version support (FileDir, S3), the poll loop falls back to a
full reload every cycle.

### SIGHUP (single-instance or orchestrated)

Send `SIGHUP` to trigger an immediate full reload:

```bash
kill -HUP $(pidof tenrankai)
```

### Admin API (per-site)

Reload a specific site via the admin API:

```bash
curl -X POST https://example.com/_admin/api/sites/default/reload
```

Note: changes made through the admin API on the *same* instance are applied
immediately — no poll delay. The poll is only needed for changes made by *other*
instances or external tools.

## IAM Permissions

The application needs these DynamoDB actions on the table:

```json
{
  "Effect": "Allow",
  "Action": [
    "dynamodb:GetItem",
    "dynamodb:PutItem",
    "dynamodb:UpdateItem",
    "dynamodb:DeleteItem",
    "dynamodb:Query",
    "dynamodb:Scan",
    "dynamodb:BatchWriteItem"
  ],
  "Resource": "arn:aws:dynamodb:REGION:ACCOUNT:table/TABLE_NAME"
}
```

The Terraform module creates and outputs an IAM policy with these permissions.
`UpdateItem` is required for the atomic version counter increment.

## Future: Event-Driven Notifications

For near-instant propagation (~1-2s) instead of polling, the table supports
DynamoDB Streams. Enable streams in the Terraform module:

```hcl
module "config_storage" {
  source         = "github.com/theatrus/tenrankai//terraform/modules/config-storage-dynamodb"
  table_name     = "tenrankai-config"
  enable_streams = true
}
```

The notification pipeline would be:

```
DynamoDB Streams -> EventBridge Pipe / Lambda -> SNS -> SQS (per instance)
```

Each Tenrankai instance would long-poll its SQS queue and reload the affected
site on message receipt. This is not yet implemented in Tenrankai but the
infrastructure can be provisioned ahead of time.

## Cost

DynamoDB on-demand pricing for a config table is effectively free:

- **Storage**: A few KB for site configs — well under the free tier
- **Reads**: One `GetItem` per poll interval for version check — ~$0.25 per billion
- **Writes**: Only when admin makes changes — negligible
- **Streams** (if enabled): Free; you pay only for Lambda/Pipe invocations
