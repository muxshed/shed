-- Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

-- Track a guest's join lifecycle and the source it becomes once connected.
ALTER TABLE guests ADD COLUMN status TEXT NOT NULL DEFAULT 'invited';
ALTER TABLE guests ADD COLUMN source_id TEXT;
