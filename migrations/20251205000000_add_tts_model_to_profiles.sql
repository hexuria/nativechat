ALTER TABLE profiles ADD COLUMN tts_model_id TEXT REFERENCES models(id);
