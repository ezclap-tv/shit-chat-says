CREATE TABLE tokens (
  user_id INTEGER REFERENCES twitch_user(id),
  scs_user_api_token VARCHAR(30) UNIQUE NOT NULL,
  twitch_access_token VARCHAR(30) UNIQUE NOT NULL,
  twitch_refresh_token VARCHAR(50) UNIQUE NOT NULL,
  PRIMARY KEY (user_id, scs_user_api_token)
);

-- hash index for faster token verification
-- and lookup of user by token
CREATE INDEX idx_tokens_scs_user_api_token ON tokens USING HASH (scs_user_api_token);

CREATE TABLE allowlist (
  id INTEGER REFERENCES twitch_user(id) PRIMARY KEY
);