-- Put the receipt counter past every number the till has already issued.
--
-- `devices.next_receipt_seq` is one sequence shared by sales and refunds, and
-- the two readers disagreed about what the column meant. `sale_repo` takes
-- `next_receipt_seq - 1` — the value the column held before the bump, which is
-- what the name promises. `refund_repo` took the post-increment value. So a
-- refund left the counter holding the number it had just used, and the next sale
-- took that same number: one receipt number on two documents, in two different
-- tables, which neither UNIQUE constraint can see. Looking a receipt up then had
-- two rows to choose between. The same disagreement also skipped a number after
-- every refund, leaving gaps nobody can account for.
--
-- `refund_repo` is corrected to read the counter the way `sale_repo` does. That
-- alone is not enough for a till already trading: if its last document was a
-- refund, the column holds a number that has *been* used, and the first sale
-- after the upgrade would collide with it. So the counter is rebuilt here from
-- the only authority available — the numbers actually printed.
--
-- Receipt numbers are '{branch_code}-{device_code}-{seq:08}', so the sequence is
-- the text after the last '-'. A device with no documents keeps its current
-- value; MAX over an empty set is NULL and COALESCE falls back.
UPDATE devices
SET next_receipt_seq = MAX(
    next_receipt_seq,
    COALESCE(
        (SELECT MAX(CAST(issued AS INTEGER)) + 1
         FROM (
             SELECT replace(receipt_number, rtrim(receipt_number, '0123456789'), '') AS issued,
                    device_id
             FROM sales
             UNION ALL
             SELECT replace(refund_receipt_number,
                            rtrim(refund_receipt_number, '0123456789'), '') AS issued,
                    (SELECT s.device_id FROM sales s WHERE s.sale_id = refunds.original_sale_id)
             FROM refunds
         ) AS printed
         WHERE printed.device_id = devices.device_id
           AND printed.issued <> ''),
        next_receipt_seq
    )
);
