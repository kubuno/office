-- Restores the previous defaults only. Stored documents are left on the family
-- the instance actually ships: reverting them would point live content at a
-- font that is no longer installed.
ALTER TABLE office.presentations ALTER COLUMN theme SET DEFAULT
    '{"name":"Défaut","primaryColor":"#1a73e8","bgColor":"#ffffff","fontFamily":"Google Sans, Arial, sans-serif","accentColor":"#ea4335","textColor":"#202124"}';
ALTER TABLE office_data.reports ALTER COLUMN theme SET DEFAULT '{
    "primaryColor": "#1a73e8",
    "fontFamily": "Google Sans, Arial, sans-serif",
    "background": "#f8f9fa",
    "chartPalette": ["#1a73e8","#ea4335","#fbbc04","#34a853","#ff6d00","#a142f4"]
}';
