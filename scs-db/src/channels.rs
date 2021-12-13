use super::Result;

pub async fn get_all(executor: impl sqlx::PgExecutor<'_>) -> Result<Vec<String>> {
  Ok(
    sqlx::query_scalar::<_, String>("SELECT DISTINCT channel FROM logs")
      .fetch_all(executor)
      .await?,
  )
}
