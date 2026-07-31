UPDATE customers
SET phone = NULL,
    updated_at = COALESCE(NULLIF(updated_at, ''), datetime('now')),
    sync_status = CASE WHEN sync_status = 'pending' THEN sync_status ELSE 'pending' END
WHERE phone IS NOT NULL
  AND TRIM(phone) = '';
