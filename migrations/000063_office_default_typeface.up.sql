-- Replaces the retired Google typeface with the platform's own face, Outfit.
--
-- Same reasoning as forms' 000009: 000007 and 000012 are applied and therefore
-- frozen, so both the column DEFAULT (what new documents inherit) and the rows
-- already written have to be corrected here. A presentation or a report keeping
-- the old family would ask for a font the instance no longer ships and fall
-- back to Arial without saying so.
--
-- `jsonb_set` preserves every other key of the theme.

ALTER TABLE office.presentations ALTER COLUMN theme SET DEFAULT
    '{"name":"Défaut","primaryColor":"#1a73e8","bgColor":"#ffffff","fontFamily":"Outfit, Arial, sans-serif","accentColor":"#ea4335","textColor":"#202124"}';

UPDATE office.presentations
   SET theme = jsonb_set(theme, '{fontFamily}', '"Outfit, Arial, sans-serif"')
 WHERE theme ? 'fontFamily'
   AND theme->>'fontFamily' LIKE '%Google Sans%';

ALTER TABLE office_data.reports ALTER COLUMN theme SET DEFAULT '{
    "primaryColor": "#1a73e8",
    "fontFamily": "Outfit, Arial, sans-serif",
    "background": "#f8f9fa",
    "chartPalette": ["#1a73e8","#ea4335","#fbbc04","#34a853","#ff6d00","#a142f4"]
}';

UPDATE office_data.reports
   SET theme = jsonb_set(theme, '{fontFamily}', '"Outfit, Arial, sans-serif"')
 WHERE theme ? 'fontFamily'
   AND theme->>'fontFamily' LIKE '%Google Sans%';

-- Slide elements carry their own font, set when the slide was authored. The
-- column is `elements` (a JSONB array), so the family is swapped textually and
-- re-parsed — every other property of every element is untouched because only
-- that exact substring is replaced.
UPDATE office.slides
   SET elements = REPLACE(elements::text, 'Google Sans', 'Outfit')::jsonb
 WHERE elements::text LIKE '%Google Sans%';
