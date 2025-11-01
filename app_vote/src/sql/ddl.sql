-- Users table
CREATE TABLE users (
    user_id TEXT PRIMARY KEY,
    phone TEXT
);

-- Votes table
CREATE TABLE votes (
    vote_id TEXT PRIMARY KEY,
    creator_id TEXT,
    topic TEXT NOT NULL,
    vote_type TEXT /*CHECK(vote_type IN ('single', 'multiple')) */,
    max_choices INTEGER,
    end_time INTEGER NOT NULL,
    visibility_rule TEXT /*CHECK(visibility_rule IN ('always', 'after_end'))*/
);

-- Options table
CREATE TABLE options (
    option_id TEXT PRIMARY KEY,
    vote_id TEXT,
    option_text TEXT NOT NULL
);

-- Vote actions table
CREATE TABLE vote_actions (
    action_id TEXT PRIMARY KEY,
    user_id TEXT,
    vote_id TEXT,
    action_time INTEGER NOT NULL,
    is_withdrawn INTEGER
);

-- Vote choices table
CREATE TABLE vote_choices (
    choice_id TEXT PRIMARY KEY,
    action_id TEXT,
    option_id TEXT
)