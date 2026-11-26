//! Native content shares the user's connection; publication and favorite migration are atomic.
use crate::{Item, user::UserIndex};
use deckard_ni_content::{Availability, DiscoveryReport, Installation, select_installations};
use rusqlite::{Connection, OptionalExtension, params};
pub(crate) fn initialize(db: &Connection) -> Result<(), String> {
    db.execute_batch("BEGIN;
 CREATE TABLE IF NOT EXISTS native_schema(version INTEGER NOT NULL);
 INSERT INTO native_schema SELECT 1 WHERE NOT EXISTS(SELECT 1 FROM native_schema);
 CREATE TABLE IF NOT EXISTS native_products(product TEXT PRIMARY KEY,name TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS native_installations(id TEXT PRIMARY KEY,product TEXT NOT NULL,source TEXT NOT NULL,root TEXT,version TEXT,available INTEGER NOT NULL);
 CREATE TABLE IF NOT EXISTS native_items(key TEXT PRIMARY KEY,product TEXT NOT NULL,path TEXT NOT NULL,fingerprint TEXT NOT NULL,item TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS native_staging(key TEXT PRIMARY KEY,product TEXT NOT NULL,path TEXT NOT NULL,fingerprint TEXT NOT NULL,item TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS native_migrations(key TEXT PRIMARY KEY);
 CREATE INDEX IF NOT EXISTS native_items_product ON native_items(product);
 COMMIT;").map_err(|e|{let _=db.execute_batch("ROLLBACK");e.to_string()})
}
pub(crate) fn catalog(db: &Connection, report: &DiscoveryReport) -> Result<Vec<Installation>, String> {
    let chosen = select_installations(report);
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    for source in &report.complete_sources {
        tx.execute("UPDATE native_installations SET available=0 WHERE source=?1", [source]).map_err(|e| e.to_string())?;
    }
    // A product has one selected installation. Unselected copies are still in the diagnostic report.
    for i in &chosen {
        tx.execute("INSERT OR REPLACE INTO native_products VALUES(?1,?2)", params![i.product_id, i.name]).map_err(|e| e.to_string())?;
        tx.execute("UPDATE native_installations SET available=0 WHERE product=?1", [&i.product_id]).map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT OR REPLACE INTO native_installations VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                i.id,
                i.product_id,
                i.source,
                i.root.as_ref().map(|p| p.to_string_lossy()),
                i.version,
                i.availability == Availability::Available
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.execute("DELETE FROM native_staging", []).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(chosen)
}
pub(crate) fn key(product: &str, relative: &str) -> String {
    format!("ni:{}:{}{}:{}", product.len(), product, relative.len(), relative)
}
pub(crate) fn cached(db: &Connection, key: &str, fingerprint: &str) -> Result<Option<Item>, String> {
    let value: Option<String> = db
        .query_row("SELECT item FROM native_items WHERE key=?1 AND fingerprint=?2", params![key, fingerprint], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    value.map(|v| serde_json::from_str(&v).map_err(|e| e.to_string())).transpose()
}
pub(crate) fn stage(db: &Connection, product: &str, fingerprint: &str, item: &Item) -> Result<(), String> {
    let json = serde_json::to_string(item).map_err(|e| e.to_string())?;
    db.execute(
        "INSERT OR REPLACE INTO native_staging VALUES(?1,?2,?3,?4,?5)",
        params![item.native_key, product, item.path.to_string_lossy(), fingerprint, json],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}
pub(crate) fn publish(db: &Connection, product: &str) -> Result<(), String> {
    db.execute_batch("SAVEPOINT native_publish").map_err(|e| e.to_string())?;
    let tx = db;
    for table in ["favorites", "not_favorites"] {
        tx.execute(&format!("INSERT OR IGNORE INTO {table}(path) SELECT s.key FROM native_staging s JOIN {table} f ON f.path=s.path WHERE s.product=?1 AND NOT EXISTS(SELECT 1 FROM native_migrations m WHERE m.key=s.key)"),[product]).map_err(|e|e.to_string())?;
    }
    tx.execute("INSERT OR IGNORE INTO native_migrations SELECT key FROM native_staging WHERE product=?1", [product])
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM items WHERE user=0 AND path IN (SELECT path FROM native_staging WHERE product=?1)", [product])
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM native_items WHERE product=?1", [product]).map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO native_items SELECT * FROM native_staging WHERE product=?1", [product]).map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM native_staging WHERE product=?1", [product]).map_err(|e| e.to_string())?;
    tx.execute_batch("RELEASE native_publish").map_err(|e| e.to_string())
}
pub(crate) fn items(own: &UserIndex) -> Result<Vec<Item>, String> {
    let mut st=own.db.prepare("SELECT n.item FROM native_items n WHERE EXISTS(SELECT 1 FROM native_installations i WHERE i.product=n.product AND i.available=1)").map_err(|e|e.to_string())?;
    let rows = st.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
    rows.map(|r| r.map_err(|e| e.to_string()).and_then(|v| serde_json::from_str(&v).map_err(|e| e.to_string()))).collect()
}
pub(crate) fn owned_roots(db: &Connection) -> Vec<std::path::PathBuf> {
    let Ok(mut st) = db.prepare("SELECT root FROM native_installations WHERE root IS NOT NULL") else { return vec![] };
    st.query_map([], |r| r.get::<_, String>(0)).map(|r| r.flatten().map(Into::into).collect()).unwrap_or_default()
}
