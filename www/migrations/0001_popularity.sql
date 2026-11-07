CREATE TABLE daily_plays (
    day TEXT NOT NULL,
    game TEXT NOT NULL,
    plays INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, game)
) WITHOUT ROWID;

-- Only today's anonymous deduplication records are needed.
CREATE TABLE play_visitors (
    day TEXT NOT NULL,
    visitor TEXT NOT NULL,
    game TEXT NOT NULL,
    PRIMARY KEY (day, visitor, game)
) WITHOUT ROWID;

CREATE TRIGGER count_new_play AFTER INSERT ON play_visitors BEGIN
    INSERT INTO daily_plays (day, game, plays) VALUES (NEW.day, NEW.game, 1)
    ON CONFLICT (day, game) DO UPDATE SET plays = plays + 1;
END;
