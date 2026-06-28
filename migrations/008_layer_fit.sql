-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

-- Per-layer fit mode: how a source fills its layer box.
--   fill    — stretch to the box, ignoring aspect ratio (previous behaviour)
--   contain — scale to fit inside the box, preserve aspect, centre (gaps show through)
--   cover   — scale to cover the box, preserve aspect, crop the overflow
ALTER TABLE scene_layers ADD COLUMN fit TEXT NOT NULL DEFAULT 'fill';
