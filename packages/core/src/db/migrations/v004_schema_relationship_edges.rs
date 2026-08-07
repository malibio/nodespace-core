//! Move schema relationship declarations out of the schema node's
//! `properties.relationships` JSON and into `relationship` table rows.
//!
//! A declaration becomes a row with `in_node` = the declaring schema node,
//! `out_node` = the target schema node (or the declaring schema itself when the
//! declaration is untyped — `targetType` absent), `relationship_type` = the
//! declared name, and the full declaration object serialized into the row's
//! `properties`. After conversion, the `relationships` key is stripped from
//! every schema node's properties: the relationship table is the single source
//! of truth, and the read path no longer consults the JSON.
//!
//! Shipped databases hold no relationship declarations at the time this lands,
//! so on real data this is a no-op that strips empty arrays — but any
//! dev-created declarations are carried over rather than dropped. A declaration
//! whose target schema no longer exists falls back to a self-edge (the
//! authoritative `targetType` lives in the row's properties, so nothing is
//! lost); one with a missing/empty name cannot be represented as an edge and is
//! dropped with a warning.

use anyhow::{Context, Result};

pub async fn apply(tx: &libsql::Transaction) -> Result<()> {
    let mut rows = tx
        .query(
            "SELECT id, json_extract(properties, '$.relationships') FROM node \
             WHERE node_type = 'schema' \
               AND json_type(properties, '$.relationships') = 'array'",
            (),
        )
        .await
        .context("Failed to query schema nodes with JSON relationship declarations")?;

    let mut schemas: Vec<(String, String)> = Vec::new();
    while let Some(row) = rows.next().await? {
        let id: String = row.get(0)?;
        let relationships_json: String = row.get(1)?;
        schemas.push((id, relationships_json));
    }

    let now = chrono::Utc::now().to_rfc3339();
    for (schema_id, relationships_json) in schemas {
        let declarations: Vec<serde_json::Value> = serde_json::from_str(&relationships_json)
            .with_context(|| {
                format!("Failed to parse relationships JSON on schema '{schema_id}'")
            })?;

        for declaration in declarations {
            let Some(name) = declaration
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|n| !n.trim().is_empty())
                .map(str::to_string)
            else {
                tracing::warn!(
                    schema = %schema_id,
                    "dropping schema relationship declaration without a name during migration"
                );
                continue;
            };

            let target = declaration
                .get("targetType")
                .and_then(|v| v.as_str())
                .map(str::to_string);

            // The out_node must satisfy the FK; a dangling or absent target
            // falls back to a self-edge (properties keep the real targetType).
            let out_node = match target {
                Some(target_id) => {
                    let mut target_rows = tx
                        .query(
                            "SELECT 1 FROM node WHERE id = ?1 LIMIT 1",
                            libsql::params![target_id.clone()],
                        )
                        .await
                        .context("Failed to check declaration target existence")?;
                    if target_rows.next().await?.is_some() {
                        target_id
                    } else {
                        tracing::warn!(
                            schema = %schema_id,
                            relationship = %name,
                            target = %target_id,
                            "declaration targets a schema that no longer exists; storing as self-edge"
                        );
                        schema_id.clone()
                    }
                }
                None => schema_id.clone(),
            };

            let rel_id = uuid::Uuid::new_v4().to_string();
            let props_json = declaration.to_string();
            tx.execute(
                "INSERT OR IGNORE INTO relationship \
                 (id, in_node, out_node, relationship_type, properties, version, created_at, modified_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7)",
                libsql::params![
                    rel_id,
                    schema_id.clone(),
                    out_node,
                    name,
                    props_json,
                    now.clone(),
                    now.clone()
                ],
            )
            .await
            .context("Failed to insert migrated declaration edge")?;
        }
    }

    // Strip the key everywhere (including empty arrays): properties are no
    // longer a storage location for declarations.
    tx.execute(
        "UPDATE node SET properties = json_remove(properties, '$.relationships') \
         WHERE node_type = 'schema' \
           AND json_type(properties, '$.relationships') IS NOT NULL",
        (),
    )
    .await
    .context("Failed to strip relationships key from schema node properties")?;

    Ok(())
}
