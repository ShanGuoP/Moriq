CREATE TABLE version_exif(version_id TEXT PRIMARY KEY REFERENCES asset_versions(id) ON DELETE CASCADE,data TEXT NOT NULL);
CREATE INDEX exif_camera ON version_exif(json_extract(data,'$.camera'));
CREATE INDEX exif_taken ON version_exif(json_extract(data,'$.taken_at'));
CREATE INDEX exif_iso ON version_exif(json_extract(data,'$.iso'));
CREATE INDEX exif_gps ON version_exif(json_extract(data,'$.latitude_e6'),json_extract(data,'$.longitude_e6'));
PRAGMA user_version=17;
