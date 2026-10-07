//! The shape catalogue shared by the office editors (`shapes/catalog.ts`): the kinds by category, their
//! default French labels, and the size a click inserts.

/// One catalogue entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeDef {
    pub kind: &'static str,
    pub label: &'static str,
}

/// One category of the gallery.
#[derive(Debug, Clone, Copy)]
pub struct ShapeCat {
    pub id: &'static str,
    pub title: &'static str,
    pub shapes: &'static [ShapeDef],
}

macro_rules! defs {
    ($($k:literal => $l:literal),* $(,)?) => { &[$(ShapeDef { kind: $k, label: $l }),*] };
}

/// `SHAPE_CATALOG`, in the web's order.
pub const SHAPE_CATALOG: &[ShapeCat] = &[
    ShapeCat { id: "lines", title: "Traits", shapes: defs![
        "line" => "Trait", "lineArrow" => "Flèche", "lineDouble" => "Double flèche",
        "elbowConnector" => "Connecteur coudé", "elbowArrow" => "Connecteur coudé fléché", "elbowDoubleArrow" => "Connecteur coudé double flèche",
        "curveConnector" => "Connecteur courbe", "curveArrow" => "Connecteur courbe fléché", "curveDoubleArrow" => "Connecteur courbe double flèche", "curve" => "Courbe",
    ] },
    ShapeCat { id: "rectangles", title: "Rectangles", shapes: defs![
        "rect" => "Rectangle", "roundRect" => "Rectangle arrondi", "snipRect" => "Coin coupé",
        "snip2SameRect" => "Deux coins coupés (même côté)", "snip2DiagRect" => "Deux coins coupés (diagonale)", "snipRoundRect" => "Coin coupé et arrondi",
        "roundRect1" => "Un coin arrondi", "round2SameRect" => "Deux coins arrondis (même côté)", "round2DiagRect" => "Deux coins arrondis (diagonale)", "plaque" => "Plaque", "frame" => "Cadre",
    ] },
    ShapeCat { id: "basic", title: "Formes de base", shapes: defs![
        "textBox" => "Zone de texte",
        "ellipse" => "Ellipse", "triangle" => "Triangle isocèle", "rtTriangle" => "Triangle rectangle",
        "parallelogram" => "Parallélogramme", "trapezoid" => "Trapèze", "diamond" => "Losange",
        "pentagon" => "Pentagone", "hexagon" => "Hexagone", "heptagon" => "Heptagone", "octagon" => "Octogone",
        "decagon" => "Décagone", "dodecagon" => "Dodécagone", "pie" => "Camembert", "chord" => "Corde", "teardrop" => "Goutte",
        "halfFrame" => "Demi-cadre", "corner" => "Coin", "diagStripe" => "Bande diagonale",
        "cross" => "Croix", "bevel" => "Biseau", "cylinder" => "Cylindre", "cube" => "Cube", "blockArc" => "Arc plein", "foldedCorner" => "Coin replié",
        "heart" => "Cœur", "lightning" => "Éclair", "sun" => "Soleil", "moon" => "Lune", "cloud" => "Nuage",
        "smiley" => "Visage souriant", "arc" => "Arc", "donut" => "Anneau", "noSymbol" => "Symbole interdit",
        "leftBracket" => "Crochet gauche", "rightBracket" => "Crochet droit", "doubleBracket" => "Crochets",
        "leftBrace" => "Accolade gauche", "rightBrace" => "Accolade droite", "doubleBrace" => "Accolades",
    ] },
    ShapeCat { id: "arrows", title: "Flèches pleines", shapes: defs![
        "arrow" => "Flèche droite", "arrowLeft" => "Flèche gauche", "arrowUp" => "Flèche haut", "arrowDown" => "Flèche bas",
        "arrowLeftRight" => "Flèche gauche-droite", "arrowUpDown" => "Flèche haut-bas", "arrowQuad" => "Flèche quadruple", "leftRightUpArrow" => "Flèche gauche-droite-haut",
        "bentArrow" => "Flèche coudée", "bentUpArrow" => "Flèche coudée vers le haut", "uTurnArrow" => "Flèche demi-tour",
        "curvedRightArrow" => "Flèche courbe droite", "curvedLeftArrow" => "Flèche courbe gauche", "curvedUpArrow" => "Flèche courbe haut", "curvedDownArrow" => "Flèche courbe bas",
        "stripedRightArrow" => "Flèche rayée", "notchedArrow" => "Flèche en V", "pentagonArrow" => "Flèche pentagone", "chevron" => "Chevron", "circularArrow" => "Flèche circulaire",
        "rightArrowCallout" => "Légende flèche droite", "leftArrowCallout" => "Légende flèche gauche", "upArrowCallout" => "Légende flèche haut", "downArrowCallout" => "Légende flèche bas",
    ] },
    ShapeCat { id: "equation", title: "Formes d'équation", shapes: defs![
        "mathPlus" => "Plus", "mathMinus" => "Moins", "mathMultiply" => "Multiplier",
        "mathDivide" => "Diviser", "mathEqual" => "Égal", "mathNotEqual" => "Différent",
    ] },
    ShapeCat { id: "flowchart", title: "Organigrammes", shapes: defs![
        "flowProcess" => "Processus", "flowAltProcess" => "Autre processus", "flowDecision" => "Décision",
        "flowData" => "Données", "flowPredefined" => "Processus prédéfini", "flowInternal" => "Stockage interne",
        "flowDocument" => "Document", "flowMultidoc" => "Plusieurs documents", "flowTerminator" => "Terminaison",
        "flowPreparation" => "Préparation", "flowManualInput" => "Saisie manuelle", "flowManualOp" => "Opération manuelle",
        "flowConnector" => "Connecteur", "flowOffPage" => "Renvoi de page", "flowCard" => "Carte", "flowPunchedTape" => "Bande perforée",
        "flowOr" => "Ou", "flowSumming" => "Jonction de sommation", "flowCollate" => "Assemblage", "flowSort" => "Tri",
        "flowExtract" => "Extraction", "flowMerge" => "Fusion", "flowStored" => "Données stockées", "flowDelay" => "Délai",
        "flowSequential" => "Accès séquentiel", "flowMagneticDisk" => "Disque magnétique", "flowDirectAccess" => "Accès direct", "flowDisplay" => "Affichage",
    ] },
    ShapeCat { id: "stars", title: "Étoiles et bannières", shapes: defs![
        "explosion1" => "Explosion 1", "explosion2" => "Explosion 2",
        "star4" => "Étoile à 4 branches", "star" => "Étoile à 5 branches", "star6" => "Étoile à 6 branches", "star7" => "Étoile à 7 branches",
        "star8" => "Étoile à 8 branches", "star10" => "Étoile à 10 branches", "star12" => "Étoile à 12 branches", "star16" => "Étoile à 16 branches", "star24" => "Étoile à 24 branches", "star32" => "Étoile à 32 branches",
        "ribbon" => "Bannière", "ribbonDown" => "Bannière vers le bas", "ribbonCurved" => "Bannière courbe",
        "scrollH" => "Parchemin horizontal", "scrollV" => "Parchemin vertical", "wave" => "Vague", "doubleWave" => "Double vague",
    ] },
    ShapeCat { id: "callouts", title: "Bulles et légendes", shapes: defs![
        "calloutRect" => "Bulle rectangulaire", "calloutRoundRect" => "Bulle arrondie", "calloutOval" => "Bulle ovale", "calloutCloud" => "Bulle nuage",
        "lineCallout" => "Légende avec trait", "calloutLine2" => "Légende trait à 2 segments", "calloutLineAccent" => "Légende trait avec barre",
    ] },
];

/// The catalogue label of a kind.
pub fn label_of(kind: &str) -> Option<&'static str> {
    SHAPE_CATALOG.iter().flat_map(|c| c.shapes.iter()).find(|d| d.kind == kind).map(|d| d.label)
}

/// `shapeDefaultSize`: the size a click inserts, in px.
pub fn shape_default_size(kind: &str) -> (f64, f64) {
    const LINES: [&str; 10] = ["line", "lineArrow", "lineDouble", "elbowConnector", "elbowArrow", "elbowDoubleArrow", "curveConnector", "curveArrow", "curveDoubleArrow", "curve"];
    const TALL: [&str; 6] = ["arrowUp", "arrowDown", "arrowUpDown", "upArrowCallout", "downArrowCallout", "bentUpArrow"];
    const BRACES: [&str; 6] = ["leftBrace", "rightBrace", "leftBracket", "rightBracket", "doubleBrace", "doubleBracket"];
    const SQUARE: [&str; 11] = ["cross", "noSymbol", "sun", "smiley", "arrowQuad", "leftRightUpArrow", "circularArrow", "flowConnector", "donut", "flowOr", "flowSumming"];
    if LINES.contains(&kind) {
        (280.0, 90.0)
    } else if TALL.contains(&kind) {
        (160.0, 240.0)
    } else if BRACES.contains(&kind) {
        (70.0, 220.0)
    } else if kind == "scrollV" {
        (170.0, 230.0)
    } else if kind.starts_with("star") || kind.starts_with("explosion") || SQUARE.contains(&kind) {
        (200.0, 200.0)
    } else {
        (240.0, 180.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalogue_kind_but_lines_and_the_text_box_has_a_geometry() {
        for cat in SHAPE_CATALOG {
            for d in cat.shapes {
                if cat.id == "lines" || d.kind == "textBox" {
                    continue;
                }
                assert!(crate::shape_paths(d.kind, 100.0, 80.0, None).is_some(), "{} has no geometry", d.kind);
            }
        }
        assert_eq!(label_of("heart"), Some("Cœur"));
        assert_eq!(shape_default_size("star7"), (200.0, 200.0));
    }
}
