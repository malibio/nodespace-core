# SurrealDB Cloud PoC: LIVE SELECT, ABAC, and Multi-Tenancy

**Status**: Planned
**Location**: Separate repository (to isolate from NodeSpace-specific concerns)
**Goal**: Validate SurrealDB Cloud capabilities before integrating into NodeSpace

---

## 1. Objectives

This PoC will validate three critical capabilities of SurrealDB Cloud:

### 1.1 LIVE SELECT at Scale
- Verify real-time subscription performance
- Measure latency (P50/P95/P99)
- Test reconnection behavior
- Validate event ordering

### 1.2 ABAC (Attribute-Based Access Control)
- Implement the "Key-Ring" pattern with `$auth` claims
- Test row-level security (RLS) with custom attributes
- Validate `PERMISSIONS` clause performance
- Test type-based access grants

### 1.3 Programmatic Multi-Tenancy
- Provision namespaces/databases via SurrealQL
- Test tenant isolation
- Validate credential scoping

---

## 2. SurrealDB Cloud Setup

### 2.1 Create Cloud Instance

1. Sign up at [SurrealDB Cloud](https://surrealdb.com/cloud)
2. Create a new instance (free tier available)
3. Note the connection string: `wss://<instance>.surrealdb.cloud`
4. Create root credentials for initial setup

### 2.2 Initial Configuration

```sql
-- Connect as root
USE NS poc DB main;

-- Create namespace for PoC
DEFINE NAMESPACE poc;

-- Create database
DEFINE DATABASE main;
```

---

## 3. Experiment 1: LIVE SELECT Performance

### 3.1 Schema Setup

```sql
DEFINE TABLE node SCHEMAFULL;
DEFINE FIELD node_type ON TABLE node TYPE string;
DEFINE FIELD content ON TABLE node TYPE string;
DEFINE FIELD properties ON TABLE node TYPE object DEFAULT {};
DEFINE FIELD version ON TABLE node TYPE int DEFAULT 1;
DEFINE FIELD created_at ON TABLE node TYPE datetime DEFAULT time::now();
DEFINE FIELD modified_at ON TABLE node TYPE datetime DEFAULT time::now();
```

### 3.2 Test Client (Rust)

```rust
use surrealdb::engine::remote::ws::{Client, Wss};
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Surreal::new::<Wss>("your-instance.surrealdb.cloud").await?;

    db.signin(Root {
        username: "root",
        password: "password",
    }).await?;

    db.use_ns("poc").use_db("main").await?;

    // Subscribe to all node changes
    let mut stream = db.select("node").live().await?;

    let start = Instant::now();
    let mut event_count = 0;
    let mut latencies = Vec::new();

    while let Some(event) = stream.next().await {
        let latency = start.elapsed();
        latencies.push(latency);
        event_count += 1;

        println!("Event {}: {:?} (latency: {:?})", event_count, event, latency);

        if event_count >= 100 {
            break;
        }
    }

    // Calculate P50/P95/P99
    latencies.sort();
    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[(latencies.len() as f64 * 0.95) as usize];
    let p99 = latencies[(latencies.len() as f64 * 0.99) as usize];

    println!("P50: {:?}, P95: {:?}, P99: {:?}", p50, p95, p99);

    Ok(())
}
```

### 3.3 Test Scenarios

| Scenario | Description | Target |
|----------|-------------|--------|
| Single client | 1 subscription, 100 writes | P95 < 200ms |
| 10 clients | 10 subscriptions, 100 writes each | P95 < 500ms |
| 50 clients | 50 subscriptions, 50 writes each | P95 < 1000ms |
| Reconnection | Force disconnect, measure recovery | < 5s to reconnect |
| RLS overhead | With PERMISSIONS vs without | < 50ms additional |

### 3.4 Write Generator

```rust
// Separate process to generate writes
async fn generate_writes(db: &Surreal<Client>, count: usize) {
    for i in 0..count {
        let node_id = Uuid::new_v4().to_string();
        db.query(r#"
            CREATE node SET
                id = $id,
                node_type = 'task',
                content = $content,
                properties = { status: 'open' },
                created_at = time::now(),
                modified_at = time::now()
        "#)
        .bind(("id", &node_id))
        .bind(("content", format!("Task {}", i)))
        .await?;

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
```

---

## 4. Experiment 2: ABAC with Key-Ring Pattern

### 4.1 User Schema with Type Grants

```sql
-- User table with embedded permissions (Key-Ring pattern)
DEFINE TABLE user SCHEMAFULL;
DEFINE FIELD email ON TABLE user TYPE string;
DEFINE FIELD password ON TABLE user TYPE string;
DEFINE FIELD role ON TABLE user TYPE string DEFAULT 'member';
DEFINE FIELD type_grants ON TABLE user TYPE object DEFAULT {};

-- Example user with permissions
CREATE user:alice SET
    email = 'alice@example.com',
    password = crypto::argon2::generate('password123'),
    role = 'member',
    type_grants = {
        'task': 'write',
        'invoice': 'read',
        'hr_record': 'none'
    };
```

### 4.2 Record Access Definition

```sql
-- Define record-based access with JWT
DEFINE ACCESS user_access ON DATABASE TYPE RECORD
    SIGNUP (
        CREATE user SET
            email = $email,
            password = crypto::argon2::generate($password),
            role = 'member',
            type_grants = {}
    )
    SIGNIN (
        SELECT * FROM user WHERE email = $email
        AND crypto::argon2::compare(password, $password)
    )
    DURATION FOR TOKEN 1h, FOR SESSION 24h;
```

### 4.3 Row-Level Security with Type Grants

```sql
-- Node table with ABAC permissions
DEFINE TABLE node SCHEMAFULL
    PERMISSIONS
        FOR select WHERE
            -- Admin override
            $auth.role = 'admin'
            OR
            -- Key-Ring check: user has access to this node_type
            $auth.type_grants[node_type] != NONE
        FOR create, update WHERE
            $auth.role = 'admin'
            OR
            $auth.type_grants[node_type] = 'write'
        FOR delete WHERE
            $auth.role = 'admin'
            OR
            $auth.type_grants[node_type] = 'write';
```

### 4.4 Test Scenarios

```rust
// Test 1: User can only see permitted node types
async fn test_type_grant_filtering(db: &Surreal<Client>) {
    // Sign in as alice (has task:write, invoice:read, hr_record:none)
    db.signin(Record {
        namespace: "poc",
        database: "main",
        access: "user_access",
        params: Credentials {
            email: "alice@example.com",
            password: "password123",
        },
    }).await?;

    // Create test nodes
    db.query("CREATE node SET node_type = 'task', content = 'Task 1'").await?;
    db.query("CREATE node SET node_type = 'invoice', content = 'Invoice 1'").await?;
    db.query("CREATE node SET node_type = 'hr_record', content = 'HR 1'").await?;

    // Query all nodes - should only return task and invoice
    let nodes: Vec<Node> = db.select("node").await?;

    assert!(nodes.iter().any(|n| n.node_type == "task"));
    assert!(nodes.iter().any(|n| n.node_type == "invoice"));
    assert!(!nodes.iter().any(|n| n.node_type == "hr_record")); // Filtered by RLS
}

// Test 2: User cannot write to read-only type
async fn test_write_permission(db: &Surreal<Client>) {
    // Alice has invoice:read (not write)
    let result = db.query(
        "CREATE node SET node_type = 'invoice', content = 'New Invoice'"
    ).await;

    assert!(result.is_err()); // Should be denied
}

// Test 3: LIVE SELECT respects RLS
async fn test_live_select_with_rls(db: &Surreal<Client>) {
    let mut stream = db.select("node").live().await?;

    // Another user creates an hr_record
    // Alice should NOT receive this event

    // Timeout after 1s - should receive no hr_record events
}
```

### 4.5 Type Field Protection Test

```sql
-- Prevent type mutation for permission bypass
DEFINE FIELD node_type ON node TYPE string
    PERMISSIONS FOR update WHERE
        $auth.role = 'admin'
        OR node_type = $value;  -- Can only "update" to same value
```

```rust
// Test: User cannot change node_type to gain access
async fn test_type_mutation_blocked(db: &Surreal<Client>) {
    // Create a task (Alice has write access)
    db.query("CREATE node:test SET node_type = 'task', content = 'Test'").await?;

    // Try to change type to hr_record (Alice has no access)
    let result = db.query(
        "UPDATE node:test SET node_type = 'hr_record'"
    ).await;

    assert!(result.is_err()); // Should be denied
}
```

---

## 5. Experiment 3: Programmatic Multi-Tenancy

### 5.1 Tenant Provisioning via SurrealQL

```sql
-- Connect as root to provision new tenant
-- Option 1: Namespace per tenant
DEFINE NAMESPACE tenant_acme;
USE NS tenant_acme;
DEFINE DATABASE main;

-- Option 2: Database per tenant (shared namespace)
USE NS multi_tenant;
DEFINE DATABASE tenant_acme;
DEFINE DATABASE tenant_globex;
```

### 5.2 Programmatic Provisioning (Rust)

```rust
async fn provision_tenant(
    db: &Surreal<Client>,
    tenant_id: &str,
) -> Result<TenantCredentials, Error> {
    // Connect as root
    db.signin(Root { username: "root", password: "root_password" }).await?;

    // Create namespace for tenant
    let namespace = format!("tenant_{}", tenant_id);
    db.query(format!("DEFINE NAMESPACE {}", namespace)).await?;

    // Switch to tenant namespace
    db.use_ns(&namespace).await?;

    // Create database
    db.query("DEFINE DATABASE main").await?;
    db.use_db("main").await?;

    // Apply schema
    db.query(include_str!("schema.surql")).await?;

    // Create admin user for tenant
    let admin_password = generate_secure_password();
    db.query(r#"
        DEFINE ACCESS admin_access ON DATABASE TYPE RECORD
            SIGNIN (
                SELECT * FROM admin WHERE email = $email
                AND crypto::argon2::compare(password, $password)
            )
            DURATION FOR TOKEN 1h, FOR SESSION 24h;

        CREATE admin:owner SET
            email = $email,
            password = crypto::argon2::generate($password),
            role = 'owner'
    "#)
    .bind(("email", format!("admin@{}.tenant", tenant_id)))
    .bind(("password", &admin_password))
    .await?;

    Ok(TenantCredentials {
        namespace,
        database: "main".to_string(),
        admin_email: format!("admin@{}.tenant", tenant_id),
        admin_password,
    })
}
```

### 5.3 Tenant Isolation Test

```rust
async fn test_tenant_isolation() {
    let tenant_a = provision_tenant(&db, "acme").await?;
    let tenant_b = provision_tenant(&db, "globex").await?;

    // Connect as tenant A
    let db_a = connect_as_tenant(&tenant_a).await?;
    db_a.query("CREATE node SET content = 'Acme secret'").await?;

    // Connect as tenant B
    let db_b = connect_as_tenant(&tenant_b).await?;

    // Tenant B should NOT see tenant A's data
    let nodes: Vec<Node> = db_b.select("node").await?;
    assert!(nodes.is_empty()); // Complete isolation

    // Tenant B cannot access tenant A's namespace
    let result = db_b.use_ns("tenant_acme").await;
    assert!(result.is_err()); // Access denied
}
```

---

## 6. Success Criteria

### 6.1 LIVE SELECT
- [ ] P95 latency < 500ms with 50 concurrent connections
- [ ] Reconnection recovery < 5 seconds
- [ ] Events delivered in order
- [ ] No event loss during normal operation

### 6.2 ABAC
- [ ] RLS correctly filters by `$auth.type_grants`
- [ ] RLS overhead < 50ms per query
- [ ] Type field protection prevents bypass
- [ ] LIVE SELECT respects RLS (no leaking events)

### 6.3 Multi-Tenancy
- [ ] Programmatic namespace/database creation works
- [ ] Complete data isolation between tenants
- [ ] Credentials scoped to single tenant
- [ ] No cross-tenant access possible

---

## 7. PoC Repository Structure

```
surrealdb-cloud-poc/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI runner for experiments
│   ├── live_select.rs       # Experiment 1: LIVE SELECT
│   ├── abac.rs              # Experiment 2: ABAC
│   ├── multi_tenancy.rs     # Experiment 3: Provisioning
│   └── schema.surql         # Test schema
├── benches/
│   └── latency_bench.rs     # Criterion benchmarks
└── README.md
```

---

## 8. References

- [SurrealDB Cloud SDK Connection](https://surrealdb.com/docs/cloud/connect/sdk)
- [DEFINE ACCESS Statement](https://surrealdb.com/docs/surrealql/statements/define/access)
- [Authentication Documentation](https://surrealdb.com/docs/surrealdb/security/authentication)
- [SurrealDB Features](https://surrealdb.com/features)

---

## 9. Next Steps After PoC

1. Document findings in NodeSpace architecture docs
2. Create integration issues based on validated patterns
3. Implement SurrealDB Cloud connection in NodeSpace
4. Add ABAC layer to NodeService
5. Build sync infrastructure on proven LIVE SELECT patterns
