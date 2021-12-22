use super::Result;

pub async fn get_all(executor: impl sqlx::PgExecutor<'_>) -> Result<Vec<i32>> {
  sqlx::query_scalar::<_, i32>(
    "
    SELECT * FROM allowlist
    ",
  )
  .fetch_all(executor)
  .await
}

pub async fn add(executor: impl sqlx::PgExecutor<'_>, ids: &[i32]) -> Result<()> {
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
