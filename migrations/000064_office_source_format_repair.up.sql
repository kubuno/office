-- Clears the false origin format left on documents and workbooks that were
-- never imported from anything.
--
-- `source_format` means "the foreign file this was READ FROM" (see 000034), and
-- NULL means "created here". Creation was nevertheless stamping it with the
-- instance's default SAVE format, so every natively created document claimed to
-- come from a .docx and every workbook from an .xlsx. Clients read the column to
-- show a format badge and to decide whether "save back to the source" applies —
-- both lied on every single new file. The handlers no longer write it; the rows
-- already created have to be corrected here, or the lie outlives the fix.
--
-- The repair key is `source_file_id IS NULL`: no stored source file means there
-- is nothing to write back to, and `save-source` already refuses those with
-- "does not come from an imported file". So a format kept on such a row can only
-- advertise an action that cannot succeed. Rows that do carry a source file —
-- the documents and workbooks genuinely opened from a .docx/.odt/.xlsx/.ods in
-- Drive — keep their origin untouched, which is the case the column exists for.

UPDATE office.documents
   SET source_format = NULL
 WHERE source_format IS NOT NULL
   AND source_file_id IS NULL;

UPDATE office.spreadsheets
   SET source_format = NULL
 WHERE source_format IS NOT NULL
   AND source_file_id IS NULL;
