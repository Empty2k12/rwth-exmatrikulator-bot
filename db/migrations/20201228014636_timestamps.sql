ALTER TABLE chatters ADD COLUMN created_at TIMESTAMP;
ALTER TABLE chatters ALTER COLUMN created_at SET DEFAULT now();