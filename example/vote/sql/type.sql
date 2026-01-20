
CREATE TABLE vote_result
(
    vote_id TEXT,
    topic  TEXT,
    vote_ended INTEGER,
    total_votes INTEGER,
    options TEXT
);

CREATE TABLE vote_history_item
(
    vote_id TEXT,
    topic TEXT,
    action_time INTEGER,
    is_withdrawn INTEGER,
    vote_ended INTEGER
);
