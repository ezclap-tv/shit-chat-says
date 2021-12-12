CREATE TABLE logs (
  id SERIAL NOT NULL,
  -- usernames are up to 25 characters, double it for good measure
  channel VARCHAR(50) NOT NULL,
  chatter VARCHAR(50) NOT NULL,
  sent_at TIMESTAMPTZ NOT NULL,
  -- Twitch UI enforces 500 character limit
  message VARCHAR(500),
  PRIMARY KEY (id)
);

CREATE INDEX idx_channel ON logs (channel);
CREATE INDEX idx_user ON logs (chatter);