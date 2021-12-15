create table twitch_user(
  id SERIAL PRIMARY KEY,
  username VARCHAR(50) UNIQUE NOT NULL,
  -- This is an optimization column so we can select only over the channels that we're monitoring
  is_logged_as_channel BOOLEAN NOT NULL DEFAULT false,
  channel_id INTEGER UNIQUE -- may be null
);

create table twitch_aliases(
  -- Reference to the actual channel
  channel INTEGER REFERENCES twitch_user(id),
  -- A username previously held by the channel
  username VARCHAR(50) UNIQUE NOT NULL,

  -- We're unlikely to have many aliases, 
  -- so having a string in the primary key is ok
  PRIMARY KEY (channel, username)
);

CREATE TABLE twitch_logs (
  id BIGSERIAL PRIMARY KEY,

  -- Explicitly don't use ON DELETE CASCADE to prevent ourselves from removing a bunch  of logs by accident
  -- If the user requests themselves to be removed from the logs, we can re-assign their logs to a special system user.
  channel INTEGER REFERENCES twitch_user(id) NOT NULL,
  chatter INTEGER REFERENCES twitch_user(id) NOT NULL,

  sent_at TIMESTAMPTZ NOT NULL,

  -- Twitch UI enforces 500 character limit (TOOD: might bump this later on if we decide to support other log sources)
  message VARCHAR(500) NOT NULL
);

CREATE TABLE twitch_logs_metadata(
  id INT REFERENCES twitch_logs(id) PRIMARY KEY,
  metadata TEXT
);

-- hash index
CREATE INDEX idx_user_username ON twitch_user USING HASH (username);
CREATE INDEX idx_logs_channel ON twitch_logs (channel);
CREATE INDEX idx_logs_user ON twitch_logs (chatter);
