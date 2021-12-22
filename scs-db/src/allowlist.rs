use super::Result;

pub async fn has(executor: impl sqlx::PgExecutor<'_>, id: i32) -> Result<bool> {
  sqlx::query_scalar::<_, bool>(
    "
    SELECT TRUE FROM allowlist
      WHERE id = $1
    ",
  )
  .bind(id)
  .fetch_optional(executor)
  .await
  .map(|v| v.unwrap_or(false))
}

pub async fn insert(executor: impl sqlx::PgExecutor<'_>, ids: &[i32]) -> Result<()> {
  sqlx::query(
    "
    INSERT INTO allowlist
      SELECT * FROM UNNEST($1)
    ",
  )
  .bind(ids)
  .execute(executor)
  .await?;
  Ok(())
}

pub async fn remove(executor: impl sqlx::PgExecutor<'_>, ids: &[i32]) -> Result<()> {
  sqlx::query(
    "
    DELETE FROM allowlist
      WHERE id IN (SELECT * FROM UNNEST($1))
    ",
  )
  .bind(ids)
  .execute(executor)
  .await?;
  Ok(())
}
