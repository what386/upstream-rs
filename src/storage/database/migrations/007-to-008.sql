BEGIN;
ALTER TABLE packages ADD COLUMN release_tag TEXT;
ALTER TABLE packages ADD COLUMN release_published_at TEXT;
UPDATE packages
SET release_tag = replace(
    version_tag_template,
    '{}',
    version_major || '.' || version_minor || '.' || version_patch
)
WHERE release_tag IS NULL
  AND version_tag_template IS NOT NULL
  AND instr(version_tag_template, '{}') > 0
  AND NOT (version_major = 0 AND version_minor = 0 AND version_patch = 0);
PRAGMA user_version = 8;
COMMIT;
