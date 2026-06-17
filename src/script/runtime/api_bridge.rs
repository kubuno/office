/// Helpers and constants for the JavaScript runtime API bridge.
/// The actual injection happens in sandbox.rs.

/// The TypeScript declaration for the Kubuno global namespace.
/// Returned by GET /script/api-types.
pub const KUBUNO_API_TYPES: &str = r#"
declare namespace Kubuno {
  namespace Utils {
    /** UUID v4. */
    function uuid(): string;
    /** Formate un nombre selon la locale (décimales optionnelles). */
    function formatNumber(n: number, decimals?: number): string;
    /** Formate une date avec des jetons yyyy/MM/dd/HH/mm/ss. */
    function formatDate(date: Date | string | number, fmt?: string): string;
    /** Date courante. */
    function today(): Date;
    function now(): Date;
    /** Lettre de colonne depuis un index 1-based (1→"A"). */
    function columnLetter(n: number): string;
    /** Index 1-based depuis une lettre ("A"→1). */
    function columnNumber(letter: string): number;
  }

  namespace Script {
    /** Input properties passed to this script invocation */
    const props: Record<string, unknown>;
  }

  namespace Http {
    /** Perform an HTTP GET request and return the parsed JSON body */
    function get(url: string, headers?: Record<string, string>): unknown;
    /** Perform an HTTP POST request with JSON body */
    function post(url: string, body: unknown, headers?: Record<string, string>): unknown;
  }

  // ── Document-bound macros (run in the browser against the OPEN document) ──────
  // Available only when the script is attached to a document as a macro and run
  // from that editor's « Macros » menu. The exact sub-namespace depends on the
  // document type (App.getType()): Sheet / Doc / Slides / Diagram / Math / Board /
  // Project / Data. Methods act immediately on the live, open document.

  /** Formulaires (UserForm) du document — créés dans le concepteur, affichés au runtime. */
  namespace Forms {
    /** Affiche le formulaire nommé en modale et résout à sa fermeture (valeurs des champs ou ce que Form.close(x) passe). */
    function show(name: string): Promise<unknown>;
    /** Noms des formulaires disponibles. */
    function list(): string[];
  }

  /** Generic helpers available in every editor's macros. */
  namespace App {
    /** Document type: 'spreadsheet' | 'document' | 'presentation' | 'diagram' | 'math' | 'whiteboard' | 'project' | 'data' */
    function getType(): string;
    /** Id of the open document. */
    function getId(): string;
    /** Show a transient message / log it to the macro output. */
    function toast(message: unknown): void;
    function log(...args: unknown[]): void;
    /** Boîte d'alerte modale (à `await`). */
    function alert(message: unknown): Promise<void>;
    /** Confirmation OK/Annuler → booléen (à `await`). */
    function confirm(message: unknown): Promise<boolean>;
    /** Saisie modale → chaîne ou null (à `await`). */
    function prompt(message: unknown, def?: unknown): Promise<string | null>;
    /** Pause de `ms` millisecondes (à `await`). */
    function sleep(ms: number): Promise<void>;
  }

  /** Spreadsheet macros (App.getType() === 'spreadsheet'). A1 notation, e.g. "B3", "A1:C5". */
  namespace Sheet {
    type Cell = string | number | boolean | null;
    // ── Sélection / navigation ──
    function getActiveCell(): string | null;
    function getSelection(): { from: string; to: string } | null;
    /** Sélectionne une cellule/plage ("B2" ou "A1:C5"). */
    function select(range: string): void;
    // ── Lecture ──
    function getValue(ref: string): Cell;
    function getFormula(ref: string): string | null;
    /** Détail d'une cellule (valeur, formule, styles). */
    function getCell(ref: string): { value: Cell; formula: string | null; bold: boolean; italic: boolean; color: string | null; background: string | null; align: string | null; numberFormat: string | null } | null;
    function getRangeValues(range: string): Cell[][];
    /** Valeurs de toute la plage utilisée. */
    function getValues(): Cell[][];
    /** Valeurs d'une ligne (1-based). */
    function getRow(row: number): Cell[];
    /** Valeurs d'une colonne (lettre "B" ou index 1-based). */
    function getColumn(col: string | number): Cell[];
    /** 1ʳᵉ référence contenant le texte (insensible casse), ou null. */
    function find(text: string): string | null;
    function getUsedRange(): { rows: number; cols: number };
    function getLastRow(): number;
    function getLastColumn(): number;
    // ── Écriture ──
    /** Écrit une valeur ou une formule (chaîne commençant par "="). */
    function setValue(ref: string, value: unknown): void;
    function setFormula(ref: string, formula: string): void;
    /** Écrit une matrice depuis le coin haut-gauche. */
    function setRangeValues(range: string, values: unknown[][]): void;
    /** Ajoute une ligne après la dernière ligne utilisée. */
    function appendRow(values: unknown[]): void;
    function clear(range: string): void;
    // ── Mise en forme (range optionnel = sélection courante) ──
    function setBold(range?: string, on?: boolean): void;
    function setItalic(range?: string, on?: boolean): void;
    function setUnderline(range?: string, on?: boolean): void;
    function setStrikethrough(range?: string, on?: boolean): void;
    function setFontSize(size: number, range?: string): void;
    function setFontFamily(family: string, range?: string): void;
    function setColor(color: string, range?: string): void;
    function setBackground(color: string, range?: string): void;
    function setAlign(align: 'left' | 'center' | 'right', range?: string): void;
    function setNumberFormat(fmt: 'number' | 'currency' | 'percent' | 'scientific', decimals?: number, range?: string): void;
    function setStyle(range: string, patch: Record<string, unknown>): void;
    function clearFormat(range: string): void;
    // ── Agrégats sur une plage ──
    function sum(range: string): number;
    function average(range: string): number;
    function min(range: string): number;
    function max(range: string): number;
    function count(range: string): number;
    function countA(range: string): number;
    // ── Feuilles ──
    function getSheetName(): string;
    function getSheetNames(): string[];
    function getSheetCount(): number;
  }

  /** Document (Word-like) macros (App.getType() === 'document'). */
  namespace Doc {
    function getText(): string;
    function getHTML(): string;
    function getWordCount(): number;
    function insertText(text: string): void;
    function setContent(html: string): void;
  }

  /** Presentation macros — read-only first version (App.getType() === 'presentation'). */
  namespace Slides {
    function count(): number;
    function getActiveIndex(): number;
    function getElementCount(): number;
    function getText(): string;
  }

  /** Diagram macros — read-only first version (App.getType() === 'diagram'). */
  namespace Diagram {
    function getShapeCount(): number;
    function getConnectorCount(): number;
    function getSelection(): { shapes: string[]; connectors: string[] };
    function getShapes(): { id: string; type: string; label: string }[];
  }

  /** Maths formula macros (App.getType() === 'math'). */
  namespace Math {
    function getLatex(): string;
    function setLatex(src: string): void;
    function getFormulaCount(): number;
  }

  /** Whiteboard macros — read-only first version (App.getType() === 'whiteboard'). */
  namespace Board {
    function getObjectCount(): number;
    function getSelection(): string[];
    function getObjects(): { id: string; type: string }[];
  }

  /** Project (Gantt) macros — read-only first version (App.getType() === 'project'). */
  namespace Project {
    function getTaskCount(): number;
    function getTasks(): { id: string; name: string; start: string; end: string }[];
  }

  /** Data (BI) macros — read-only first version (App.getType() === 'data'). */
  namespace Data {
    function getReportName(): string;
    function getWidgetCount(): number;
    function getPageCount(): number;
  }
}

declare namespace console {
  function log(...args: unknown[]): void;
  function warn(...args: unknown[]): void;
  function error(...args: unknown[]): void;
}

// Disponible UNIQUEMENT dans le code d'un formulaire (UserForm) : pilote les contrôles.
declare namespace Form {
  /** Valeur courante d'un contrôle (champ texte / case à cocher) par son nom. */
  function getValue(name: string): unknown;
  /** Définit la valeur d'un contrôle. */
  function setValue(name: string, value: unknown): void;
  /** Ferme le formulaire ; la valeur passée est renvoyée par Kubuno.Forms.show(). */
  function close(value?: unknown): void;
}
"#;
