-- Add migration script here
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE twitch_user (
  id serial PRIMARY KEY,
  username varchar(50) UNIQUE NOT NULL,
  -- This is an optimization column so we can select only over the channels that we're monitoring
  is_logged_as_channel boolean NOT NULL DEFAULT FALSE,
  channel_id integer UNIQUE -- may be null
);

CREATE TABLE twitch_aliases (
  -- Reference to the actual channel
  channel integer REFERENCES twitch_user (id),
  -- A username previously held by the channel
  username varchar(50) UNIQUE NOT NULL,
  -- We're unlikely to have many aliases,
  -- so having a string in the primary key is ok
  PRIMARY KEY (channel, username)
);

CREATE TABLE twitch_logs (
  id bigserial PRIMARY KEY,
  -- Explicitly don't use ON DELETE CASCADE to prevent ourselves from removing a bunch  of logs by accident
  -- If the user requests themselves to be removed from the logs, we can re-assign their logs to a special system user.
  channel integer REFERENCES twitch_user (id) NOT NULL,
  chatter integer REFERENCES twitch_user (id) NOT NULL,
  sent_at timestamptz NOT NULL,
  -- Twitch UI enforces 500 character limit (TOOD: might bump this later on if we decide to support other log sources)
  message varchar(500) NOT NULL
);

CREATE TABLE twitch_logs_metadata (
  id int REFERENCES twitch_logs (id) PRIMARY KEY,
  metadata text
);

-- A raw logs table without any constraints except for the primary key. Used for quick ingestion.
CREATE TABLE raw_logs (
  -- Required for fast DELETEs
  ID bigserial PRIMARY KEY,
  channel varchar NOT NULL,
  chatter varchar NOT NULL,
  sent_at timestamptz NOT NULL,
  message varchar NOT NULL
);

-- hash index for faster joins
CREATE INDEX idx_twitch_user_username ON twitch_user USING HASH (username);

-- btree indexes so we can efficiently partition by channel and chatter
CREATE INDEX idx_twitch_logs_channel ON twitch_logs (channel);

CREATE INDEX idx_twitch_logs_user ON twitch_logs (chatter);

-- a multi-column index to enable efficnet seek paging (see https://use-the-index-luke.com/no-offset)
CREATE INDEX idx_twitch_logs_channel_sent_at_id ON twitch_logs (channel, sent_at, id);

-- A GIN trigram index that significantly speeds up fuzzy text search with operators such as LIKE, ILIKE, and %
CREATE INDEX idx_twitch_logs_message_trigram ON twitch_logs USING GIN (message gin_trgm_ops);

