-- A customer is known by more than one name, and the till has to accept any.
--
-- WhatsApp gives two names for the same person and they are not the same thing:
-- the address-book name, which is what this shop saved them as, and the
-- pushName, which is what the customer chose for themselves. Contact import
-- stored one and discarded the other, so a customer saved as "Ali Baqala" could
-- only be found by typing "Ali ⚡" — the name nobody in the shop knows.
--
-- `name` stays the display name and now prefers the shop's own spelling, since
-- that is what goes on a receipt and in the ledger. `whatsapp_name` carries the
-- customer's own, so either one finds them.
--
-- Nullable: most customers are entered at the counter and never had a WhatsApp
-- profile name to record. An empty column here means "we only know them one
-- way", which is the ordinary case and not a fault.

ALTER TABLE customers ADD COLUMN whatsapp_name TEXT;

-- Searched on every keystroke at the till, alongside name and phone.
CREATE INDEX IF NOT EXISTS idx_customers_whatsapp_name
    ON customers(whatsapp_name);
