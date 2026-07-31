-- Media kind for a WhatsApp notification (currently "image", else NULL for text).
-- Drives the "View" action in the POS notification popup, which fetches the
-- decrypted image from the sidecar on demand.
ALTER TABLE wa_messages ADD COLUMN media_type TEXT;
