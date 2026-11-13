//! Boutons d'attaque des bannières (voir ui_strike/README.md) : patch de l'interface au
//! moment où l'exe lit ses fichiers, sans mod. Rien ne part sur le réseau, chaque joueur qui a le
//! proxy change son propre affichage.
//!
//! - **Lua.** L'exe charge chaque script par `140dc30e0`, seul appelant de `luaL_loadbuffer`
//!   (HavokScript_FinalRelease.dll). On remplace l'entrée de cet import dans la table d'imports
//!   de l'exe : pour `CityBannerManager.lua`, quel que soit le mod qui le fournit, on compile un
//!   texte modifié qui appartient au proxy (luaL_loadbuffer ne garde pas le tampon).
//! - **XML.** `140a2d280` (DX12 ; `140a1f3f0` en DX11, retrouvée par la même signature) lit un
//!   fichier XML entier dans un tampon du jeu `{ptr, len, cap, allocateur}`, NUL final compris.
//!   Pour `CityBannerInstances.xml`, on réécrit ce tampon après la lecture, en l'agrandissant par
//!   l'allocateur du jeu, qui le libérera.
//!
//! Réglages dans rollover_proxy.cfg :
//! - `ui_boutons=on|off` (on) : boutons au-dessus des drapeaux d'unités (`ui_boutons.lua` et
//!   `ui_boutons.xml`), et `ConsumeMouse` sur les boutons nucléaires ;
//! - `ui_recentrage=on|off` (off) : off retire le recentrage de la caméra au clic sur ces boutons,
//!   en passant `false` en second argument de `UI.SelectCity` / `UI.SelectDistrict` ;
//! - `ui_journal=on|off` (off) : note le nom de chaque script et de chaque XML chargés ;
//! - `ui_mesure_epingles=on|off` (off) : ajoute `ui_epingles.lua` à la fin des scripts que
//!   recharge chaque `PlayerInfoChanged` (épingles, liste des épingles, bandeau des dirigeants, DMT).
//!   Les durées vont dans Lua.log (voir map_pins/README.md) ;
//! - `ui_mesure_mouvements=on|off` (off) : lignes « BGD mouvements: » dans Lua.log (coût de la
//!   zone de déplacement sur un quartier ennemi, `ui_mouvements.lua`) ;
//! - `ui_epingles=on|off` (on) : ajoute `ui_epingles_opti.lua` à MapPinManager.lua et
//!   MapPinListPanel.lua. Plus de reconstruction quand aucune épingle n'a changé, une seule lecture
//!   de GetMapPins par joueur et par reconstruction, liste fermée reconstruite à son ouverture ;
//! - `ui_merveilles=on|off` (on) : ajoute `ui_merveilles.lua` à ProductionHelper.lua. Une merveille
//!   retirée de la file par un échange (wonder_queue/README.md) est remise en tête par des commandes
//!   du jeu, rejouées à l'identique par tous les joueurs, proxy ou non ;
//! - `ui_diag_accords=on|off` (off) : ajoute `ui_diag_accords.lua` à DiplomacyDealView.lua. Écrit
//!   dans Lua.log les événements d'accord et de session, et l'état de l'écran d'accord (diagnostic
//!   des accords croisés, docs/netcode/README.md) ;
//! - `ui_avancement=on|off` (on) : ajoute `ui_avancement.lua` à WorldTracker.lua, et un libellé à
//!   WorldTrackerResearch.xml et WorldTrackerCivic.xml : « x/y (+z) » (placé / coût, excédent lu par `avancement`) à droite du nom de
//!   la technologie et du dogme en cours, dans le panneau en haut à gauche ;
//! - `ui_rendements=on|off` (on, lu par `rendements_empire`) : ajoute `ui_rendements.lua` à
//!   TopPanel.lua. Le bandeau redessine ses rendements après un gouverneur ou un personnage
//!   illustre, dont `rendements_empire` refait les instantanés (empire_yields/README.md).
//!
//! Les mods d'interface repris (`mods_ui`) passent par les mêmes accroches : leur remplacement du
//! texte entier avant les correctifs de ce module, leurs blocs après, une seule compilation.
//!
//! Dès que l'accroche Lua est posée, trois défauts du jeu sont aussi corrigés (`FIXES`) : le bouton
//! d'attaque d'une ville non sélectionnée demandait deux clics (`patch_city_panel`), un clic sur le
//! drapeau d'une unité ne tirait pas en visée d'une ville ou d'un quartier (`patch_unit_flags`), et
//! la production en cours du panneau de production ne se montrait pas en passant d'une ville sans
//! production à une ville qui en a une (`patch_production_panel`).
//!
//! Toujours aussi, sans accroche de plus : le bouton d'or disponible de l'écran d'accord, repris
//! d'une icône désactivée, restait désactivé (`patch_accord_or`, `ui_accord_or.lua`).

use crate::exe::{self, is_file, iat_slot, replace_buffer, write_slot, GameBuf, ReadXml, XmlHook};
use crate::{log, win};
use std::ffi::{c_char, c_void, CStr};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

const LUA_BLOCK: &str = include_str!("ui_boutons.lua");
const XML_BLOCK: &str = include_str!("ui_boutons.xml");
const MESURE_BLOCK: &str = include_str!("ui_epingles.lua");
const OPTI_BLOCK: &str = include_str!("ui_epingles_opti.lua");
const MERVEILLES_BLOCK: &str = include_str!("ui_merveilles.lua");
const RENDEMENTS_BLOCK: &str = include_str!("ui_rendements.lua");
const CONGRES_BLOCK: &str = concat!(include_str!("../../textes/bgd_textes.lua"), include_str!("../../textes/bgd_texte.lua"), include_str!("ui_congres.lua"));
const CIBLES_BLOCK: &str = include_str!("ui_congres_cibles.lua");
const VOTE_BLOCK: &str = include_str!("ui_congres_vote.lua");
const ATTENTE_BLOCK: &str = concat!(include_str!("../../textes/bgd_textes.lua"), include_str!("../../textes/bgd_texte.lua"), include_str!("ui_congres_attente.lua"));
const ATTENTE_XML: &str = include_str!("ui_congres_attente.xml");
const INTRO_BLOCK: &str = include_str!("ui_congres_intro.lua");
const DIAG_ACCORDS_BLOCK: &str = include_str!("ui_diag_accords.lua");
const ACCORD_OR_BLOCK: &str = include_str!("ui_accord_or.lua");
/// Blocs des réglages `ui_banniere_annuler`, `ui_reduction_quartiers` et `ui_debordement`.
const ANNULER_BLOCK: &str = include_str!("ui_banniere_annuler.lua");
const QUARTIERS_BLOCK: &str =
    concat!(include_str!("../../textes/bgd_textes.lua"), include_str!("../../textes/bgd_texte.lua"), include_str!("ui_quartiers.lua"));
const DEBORDEMENT_BLOCK: &str =
    concat!(include_str!("../../textes/bgd_textes.lua"), include_str!("../../textes/bgd_texte.lua"), include_str!("ui_debordement.lua"));
/// Bloc du réglage `ui_avancement` (WorldTracker.lua), et son libellé (WorldTrackerResearch.xml,
/// WorldTrackerCivic.xml).
const AVANCEMENT_BLOCK: &str = include_str!("ui_avancement.lua");
const AVANCEMENT_XML: &str = "<Label ID=\"BGD_Avancement\" Anchor=\"R,T\" Offset=\"0,6\" Style=\"FontNormal14\" String=\"\" Hidden=\"1\" />";
const ACCORD_FIN_BLOCK: &str =
    concat!(include_str!("../../textes/bgd_textes.lua"), include_str!("../../textes/bgd_texte.lua"), include_str!("ui_accord_fin.lua"));

static BOUTONS: AtomicBool = AtomicBool::new(true);
static RECENTRAGE: AtomicBool = AtomicBool::new(false);
static JOURNAL: AtomicBool = AtomicBool::new(false);
static MESURE: AtomicBool = AtomicBool::new(false);
static OPTI: AtomicBool = AtomicBool::new(true);
static MERVEILLES: AtomicBool = AtomicBool::new(true);
static DIAG_ACCORDS: AtomicBool = AtomicBool::new(false);
static MESURE_MOUVEMENTS: AtomicBool = AtomicBool::new(false);
static ANNULER: AtomicBool = AtomicBool::new(true);
static QUARTIERS: AtomicBool = AtomicBool::new(true);
static AVANCEMENT: AtomicBool = AtomicBool::new(true);
/// Les calques sont définis dans le dernier CityBannerInstances.xml lu : le bloc Lua, qui s'en
/// sert, n'est ajouté qu'à cette condition.
static XML_PATCHED: AtomicBool = AtomicBool::new(false);

static IAT_SLOT: AtomicUsize = AtomicUsize::new(0);
static REAL_LOADBUFFER: AtomicUsize = AtomicUsize::new(0);
static XML_HOOK: Mutex<Option<XmlHook>> = Mutex::new(None);
static XML_TRAMP: AtomicUsize = AtomicUsize::new(0);

pub fn set_boutons(on: bool) {
    BOUTONS.store(on, Ordering::Relaxed);
}

pub fn set_recentrage(on: bool) {
    RECENTRAGE.store(on, Ordering::Relaxed);
}

#[cfg(feature = "diag")]
pub fn set_journal(on: bool) {
    JOURNAL.store(on, Ordering::Relaxed);
}

#[cfg(feature = "diag")]
pub fn set_mesure_epingles(on: bool) {
    MESURE.store(on, Ordering::Relaxed);
}

pub fn set_mesure_mouvements(on: bool) {
    MESURE_MOUVEMENTS.store(on, Ordering::Relaxed);
}

pub fn set_epingles(on: bool) {
    OPTI.store(on, Ordering::Relaxed);
}

pub fn set_banniere_annuler(on: bool) {
    ANNULER.store(on, Ordering::Relaxed);
}

pub fn set_reduction_quartiers(on: bool) {
    QUARTIERS.store(on, Ordering::Relaxed);
}

pub fn set_avancement(on: bool) {
    AVANCEMENT.store(on, Ordering::Relaxed);
}

pub fn set_merveilles(on: bool) {
    MERVEILLES.store(on, Ordering::Relaxed);
}

#[cfg(feature = "diag")]
pub fn set_diag_accords(on: bool) {
    DIAG_ACCORDS.store(on, Ordering::Relaxed);
}

fn active() -> bool {
    BOUTONS.load(Ordering::Relaxed)
        || !RECENTRAGE.load(Ordering::Relaxed)
        || JOURNAL.load(Ordering::Relaxed)
        || MESURE.load(Ordering::Relaxed)
        || OPTI.load(Ordering::Relaxed)
        || MERVEILLES.load(Ordering::Relaxed)
        || DIAG_ACCORDS.load(Ordering::Relaxed)
        || ANNULER.load(Ordering::Relaxed)
        || QUARTIERS.load(Ordering::Relaxed)
        || AVANCEMENT.load(Ordering::Relaxed)
        || crate::debordement::pose()
        || crate::rendements_empire::enabled()
        || crate::mods_ui::any()
        || crate::spectateurs::poses()
        || crate::depart::interface_requise()
        || crate::canaux::pose()
}

// ---------------------------------------------------------------------------------------------
// Remplacements de texte, sans état : testés sur les vrais fichiers (voir les tests en bas).

/// Les fonctions de clic et l'appel qui sélectionne, donc recentre.
const CLICKS: [(&str, &str); 3] = [
    ("OnCityStrikeButtonClick", "UI.SelectCity("),
    ("OnDistrictRangeStrikeButtonClick", "UI.SelectDistrict("),
    ("OnICBMStrikeButtonClick", "UI.SelectCity("),
];

/// Ajoute `false` en second argument du premier `call` du corps de la fonction `func`.
fn no_recenter(text: &mut String, func: &str, call: &str) -> Result<(), String> {
    let header = format!("function {func}");
    let start = text
        .match_indices(&header)
        .map(|(i, _)| i)
        .find(|&i| text[i + header.len()..].trim_start().starts_with('('))
        .ok_or_else(|| format!("{func} introuvable"))?;
    let body_end = text[start + header.len()..].find("\nfunction ").map_or(text.len(), |i| start + header.len() + i);
    let at = text[start..body_end].find(call).ok_or_else(|| format!("{func} : {call}…) introuvable"))? + start + call.len();
    let close = text[at..body_end].find(')').ok_or_else(|| format!("{func} : parenthèse fermante introuvable"))? + at;
    let args = text[at..close].trim();
    if args.is_empty() || args.contains(',') {
        return Err(format!("{func} : arguments inattendus ({args})"));
    }
    let new = format!(" {args}, false ");
    text.replace_range(at..close, &new);
    Ok(())
}

/// Texte modifié de CityBannerManager.lua, et ce qui a été fait ou pas, pour le journal.
pub fn patch_lua(src: &[u8], boutons: bool, recentrage: bool) -> (Option<Vec<u8>>, Vec<String>) {
    let mut notes = Vec::new();
    let Ok(text) = std::str::from_utf8(src) else {
        return (None, vec!["texte non UTF-8, laissé tel quel".into()]);
    };
    let mut text = text.to_owned();
    let mut changed = false;
    if !recentrage {
        for (func, call) in CLICKS {
            match no_recenter(&mut text, func, call) {
                Ok(()) => changed = true,
                Err(e) => notes.push(format!("recentrage gardé : {e}")),
            }
        }
    }
    if boutons {
        text.push_str("\r\n");
        text.push_str(LUA_BLOCK);
        changed = true;
    }
    (changed.then(|| text.into_bytes()), notes)
}

/// Corrige la faute de `CityPanel.lua` (OnCitySelectionChanged) qui coûte un second clic au bouton
/// d'attaque d'une ville non sélectionnée. Le clic sélectionne la ville puis passe en
/// CITY_RANGE_ATTACK ; l'événement CitySelectionChanged arrive ensuite et doit garder ce mode si la
/// ville peut tirer, mais teste `CityManager.GetCity(owningPlayerID, cityID)`, alors que le
/// paramètre s'appelle `ownerPlayerID`. Avec un joueur nil, le test échoue toujours et l'interface
/// repasse en SELECTION.
pub fn patch_city_panel(src: &[u8]) -> Result<Vec<u8>, String> {
    const HEADER: &str = "function OnCitySelectionChanged( ownerPlayerID";
    const WRONG: &str = "CityManager.GetCity(owningPlayerID, cityID)";
    let text = std::str::from_utf8(src).map_err(|_| "texte non UTF-8".to_string())?;
    let start = text.find(HEADER).ok_or("OnCitySelectionChanged( ownerPlayerID… introuvable")?;
    let end = text[start + HEADER.len()..].find("\nfunction ").map_or(text.len(), |i| start + HEADER.len() + i);
    let body = &text[start..end];
    if body.contains("local owningPlayerID") {
        return Err("owningPlayerID est défini dans la fonction".into());
    }
    let at = body.find(WRONG).ok_or("appel fautif introuvable (déjà corrigé ?)")? + start;
    let mut out = text.to_owned();
    out.replace_range(at..at + WRONG.len(), "CityManager.GetCity(ownerPlayerID, cityID)");
    Ok(out.into_bytes())
}

const PANNEAU_VILLE_BLOCK: &str = include_str!("ui_panneau_ville.lua");

/// CityPanel.lua : la faute d'`owningPlayerID` (`patch_city_panel`), puis `ui_panneau_ville.lua`,
/// qui redessine le panneau quand la file change (production retirée, file vidée). La faute peut
/// manquer (corrigée par un mod) : le bloc est ajouté quand même.
pub fn patch_city_panel_complet(src: &[u8]) -> Result<Vec<u8>, String> {
    const MARQUE: &[u8] = b"Civ6RolloverProxy (src/ui_panneau_ville.lua)";
    if src.windows(MARQUE.len()).any(|w| w == MARQUE) {
        return Err("déjà corrigé".into());
    }
    let mut out = match patch_city_panel(src) {
        Ok(t) => t,
        Err(e) => {
            log(&format!("ui : CityPanel.lua : owningPlayerID laissé tel quel : {e}"));
            src.to_vec()
        }
    };
    out.extend_from_slice(b"
");
    out.extend_from_slice(PANNEAU_VILLE_BLOCK.as_bytes());
    Ok(out)
}

const FLAG_BLOCK: &str = include_str!("ui_drapeaux.lua");

/// En visée d'une ville ou d'un quartier, un clic gauche sur le drapeau d'une unité étrangère ne
/// tirait pas : `OnUnitFlagClick` ne sait faire attaquer que l'unité sélectionnée, et le drapeau
/// prend le clic avant WorldInput. Les deux clics gauches des drapeaux passent par
/// `Civ6RolloverProxy_OnUnitFlagLClick` (ui_drapeaux.lua), inséré avant `OnUnitFlagClick` : il
/// tire sur la case de l'unité, sinon il appelle `OnUnitFlagClick`. Le clic droit ne change pas.
///
/// Travaille sur les octets : le fichier du jeu n'est pas en UTF-8 (celui de BSM l'est).
pub fn patch_unit_flags(src: &[u8]) -> Result<Vec<u8>, String> {
    const CLICK: &[u8] = b"RegisterCallback( Mouse.eLClick, OnUnitFlagClick )";
    const NEW_CLICK: &[u8] = b"RegisterCallback( Mouse.eLClick, Civ6RolloverProxy_OnUnitFlagLClick )";
    const HEADER: &[u8] = b"\nfunction OnUnitFlagClick(";
    let find_from = |from: usize, pat: &[u8]| src[from..].windows(pat.len()).position(|w| w == pat).map(|i| from + i);
    let mut clicks = Vec::new();
    while let Some(i) = find_from(clicks.last().map_or(0, |&i: &usize| i + CLICK.len()), CLICK) {
        clicks.push(i);
    }
    if clicks.len() != 2 {
        return Err(format!("{} clics gauches branchés sur OnUnitFlagClick au lieu de 2", clicks.len()));
    }
    let header = find_from(0, HEADER).ok_or("OnUnitFlagClick introuvable")? + 1;
    // (position, texte inséré, octets remplacés), dans l'ordre du fichier.
    let mut edits = vec![(header, FLAG_BLOCK.as_bytes(), 0)];
    edits.extend(clicks.iter().map(|&c| (c, NEW_CLICK, CLICK.len())));
    edits.sort_by_key(|e| e.0);
    let mut out = Vec::with_capacity(src.len() + FLAG_BLOCK.len() + 64);
    let mut pos = 0;
    for (at, insert, skip) in edits {
        out.extend_from_slice(&src[pos..at]);
        out.extend_from_slice(insert);
        pos = at + skip;
    }
    out.extend_from_slice(&src[pos..]);
    Ok(out)
}

const PRODUCTION_BLOCK: &str = include_str!("ui_production.lua");

/// Les variables locales de ProductionPanel.lua que lit `ui_production.lua`, et la fonction qu'il
/// enveloppe.
const PRODUCTION_NAMES: [&str; 6] = [
    "local LISTMODE",
    "local m_isQueueOpen",
    "local m_hasProductionToShow",
    "local m_CurrentListMode",
    "local m_productionTab",
    "\nfunction View(",
];

/// Deux défauts du panneau de production, corrigés par `ui_production.lua`, ajouté à la fin du
/// fichier pour lire ses variables locales :
/// - en changeant de ville, la production en cours gardait la visibilité de la ville précédente
///   (cachée après une ville sans production) : on enveloppe `View` et applique la règle des onglets ;
/// - onglet Production, le clic droit sur la production en cours ne la retirait pas (bouton
///   désactivé par l'onglet, BET-13) : on le réactive après chaque passage sur l'onglet.
/// Octets laissés tels quels, comme pour les drapeaux.
pub fn patch_production_panel(src: &[u8]) -> Result<Vec<u8>, String> {
    let has = |pat: &str| src.windows(pat.len()).any(|w| w == pat.as_bytes());
    if let Some(missing) = PRODUCTION_NAMES.iter().find(|n| !has(n)) {
        return Err(format!("{} introuvable", missing.trim_start()));
    }
    if has("Civ6RolloverProxy (src/ui_production.lua)") {
        return Err("déjà corrigé".into());
    }
    let mut out = src.to_vec();
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(PRODUCTION_BLOCK.as_bytes());
    Ok(out)
}

const IMHOTEP_BLOCK: &str =
    concat!(include_str!("../../textes/bgd_textes.lua"), include_str!("../../textes/bgd_texte.lua"), include_str!("ui_imhotep.lua"));

/// Ajoute `ui_imhotep.lua` à la fin du fichier, si `needle` (la fonction qu'il enveloppe) y est.
fn patch_imhotep(src: &[u8], needle: &str) -> Result<Vec<u8>, String> {
    let has = |pat: &str| src.windows(pat.len()).any(|w| w == pat.as_bytes());
    if !has(needle) {
        return Err(format!("{} introuvable", needle.trim_start()));
    }
    if has("Civ6RolloverProxy (src/ui_imhotep.lua)") {
        return Err("déjà corrigé".into());
    }
    let mut out = src.to_vec();
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(IMHOTEP_BLOCK.as_bytes());
    Ok(out)
}

/// L'effet d'Imhotep écrit 175 en dur, quelle que soit la vitesse (87 en vitesse en ligne) :
/// `ui_imhotep.lua` enveloppe `PopulateData` de l'écran des personnages illustres. Octets laissés
/// tels quels.
pub fn patch_great_people(src: &[u8]) -> Result<Vec<u8>, String> {
    patch_imhotep(src, "\nfunction PopulateData(")
}

/// Même texte dans la Civilopédie : `ui_imhotep.lua` enveloppe `GetModifierText`, que la page
/// inclut (GameEffectsText.lua).
pub fn patch_pedia_great_person(src: &[u8]) -> Result<Vec<u8>, String> {
    patch_imhotep(src, "GetModifierText(")
}

const ENTREE_BLOCK: &str = include_str!("ui_entree.lua");

/// `OnInputHandler` d'ActionPanel.lua lance « Action suivante » sur Entrée codée en dur, même quand
/// le joueur a retiré Entrée de l'action `EndTurn`. Le test de la touche passe par
/// `BGD_EntreeEstActionSuivante` (ui_entree.lua), inséré avant le gestionnaire. Octets laissés tels
/// quels.
pub fn patch_action_panel(src: &[u8]) -> Result<Vec<u8>, String> {
    const KEY: &[u8] = b"if pInputStruct:GetKey() == Keys.VK_RETURN then";
    const NEW_KEY: &[u8] = b"if pInputStruct:GetKey() == Keys.VK_RETURN and (pInputStruct:IsShiftDown() or BGD_EntreeEstActionSuivante()) then";
    const HEADER: &[u8] = b"\nfunction OnInputHandler(";
    let find = |pat: &[u8]| src.windows(pat.len()).position(|w| w == pat);
    if find(b"BGD_EntreeEstActionSuivante").is_some() {
        return Err("déjà corrigé".into());
    }
    let header = find(HEADER).ok_or("OnInputHandler introuvable")? + 1;
    let key = find(KEY).ok_or("test d'Entrée introuvable")?;
    if key < header || src[key + 1..].windows(KEY.len()).any(|w| w == KEY) {
        return Err("test d'Entrée hors d'OnInputHandler ou en double".into());
    }
    let mut out = Vec::with_capacity(src.len() + ENTREE_BLOCK.len() + 64);
    out.extend_from_slice(&src[..header]);
    out.extend_from_slice(ENTREE_BLOCK.as_bytes());
    out.extend_from_slice(&src[header..key]);
    out.extend_from_slice(NEW_KEY);
    out.extend_from_slice(&src[key + KEY.len()..]);
    Ok(out)
}

const EMISSAIRES_BLOCK: &str = include_str!("ui_emissaires.lua");
const EMISSAIRES_DONE: &str = "émissaires en attente gardés au rafraîchissement de l'écran des cités-États";

/// Deux correctifs de CityStates.lua (jeu ou Better Spectator Mod) :
/// - la copie de BSM passe Échap à `OnInputHandler` de l'animation sans `OnClose` : le panneau
///   glisse hors de l'écran sans `Close()`, qui vide les émissaires en attente et lève le blocage de
///   fin de tour. On remet `OnClose`, comme dans le fichier du jeu. Voir analyses/emissaires_echap ;
/// - `ui_emissaires.lua`, ajouté à la fin : les émissaires en attente ne sont plus effacés à chaque
///   rafraîchissement (émissaire, quête, guerre d'un autre joueur). Voir analyses/emissaires_reinit.
pub fn patch_city_states(src: &[u8]) -> Result<Vec<u8>, String> {
    const WRONG: &[u8] = b"m_kScreenSlideAnim.OnInputHandler( input );";
    const RIGHT: &[u8] = b"m_kScreenSlideAnim.OnInputHandler( input, OnClose );";
    let find = |pat: &[u8]| src.windows(pat.len()).position(|w| w == pat);
    for needed in ["\nfunction OnClose(", "\nfunction GetData(", "\nfunction ViewList(", "\nfunction OnConfirmPlacement(", "\nInitialize();"] {
        if find(needed.as_bytes()).is_none() {
            return Err(format!("{} introuvable", needed.trim()));
        }
    }
    if find(b"Civ6RolloverProxy (src/ui_emissaires.lua)").is_some() {
        return Err("déjà corrigé".into());
    }
    let mut out = Vec::with_capacity(src.len() + EMISSAIRES_BLOCK.len() + 16);
    match find(WRONG) {
        Some(at) => {
            out.extend_from_slice(&src[..at]);
            out.extend_from_slice(RIGHT);
            out.extend_from_slice(&src[at + WRONG.len()..]);
        }
        None => out.extend_from_slice(src),
    }
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(EMISSAIRES_BLOCK.as_bytes());
    Ok(out)
}

const RACCOURCIS_BLOCK: &str = include_str!("ui_raccourcis.lua");
const RACCOURCIS_DONE: &str = "contextes d'entrée orphelins retirés (raccourcis)";

/// Raccourcis coupés en cours de partie : un contexte d'entrée (menu Échap, congrès) resté sur la
/// pile après la fermeture de son écran bloque les actions du contexte World. `ui_raccourcis.lua`,
/// ajouté à la fin d'InGame.lua (XP2 ou MPH), le retire. Voir analyses/raccourcis.
pub fn patch_in_game(src: &[u8]) -> Result<Vec<u8>, String> {
    let has = |pat: &str| src.windows(pat.len()).any(|w| w == pat.as_bytes());
    if !has("local DefaultMessageHandler") {
        return Err("local DefaultMessageHandler introuvable".into());
    }
    if has("Civ6RolloverProxy (src/ui_raccourcis.lua)") {
        return Err("déjà corrigé".into());
    }
    let mut out = src.to_vec();
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(RACCOURCIS_BLOCK.as_bytes());
    Ok(out)
}

const DIPLOMATIE_BLOCK: &str = include_str!("ui_diplomatie.lua");
const DIPLOMATIE_DONE: &str = "écran diplomatique refermé si son ouverture échoue";

/// Interface masquée pour toujours après une erreur à l'ouverture de l'écran diplomatique (rapport
/// BGE-DB5687). `ui_diplomatie.lua`, ajouté à la fin de DiplomacyActionView.lua, referme l'écran.
pub fn patch_diplomacy_action_view(src: &[u8]) -> Result<Vec<u8>, String> {
    let has = |pat: &str| src.windows(pat.len()).any(|w| w == pat.as_bytes());
    if let Some(missing) = ["local ms_bIsViewInitialized", "\nfunction OnOpenDiplomacyActionView(", "\nfunction Close("].iter().find(|n| !has(n)) {
        return Err(format!("{} introuvable", missing.trim_start()));
    }
    if has("Civ6RolloverProxy (src/ui_diplomatie.lua)") {
        return Err("déjà corrigé".into());
    }
    Ok(append_blocks(src, &[DIPLOMATIE_BLOCK]))
}

/// Table qui remplace `Teams` devant `#` : une équipe absente rend une liste vide au lieu de nil.
const EQUIPES_DEF: &[u8] = b"BGD_EquipesSures = BGD_EquipesSures or setmetatable({}, { __index = function(_, k) return (Teams ~= nil and Teams[k]) or {}; end }); ";

/// `#Teams[id]` plante quand l'équipe de la configuration du joueur n'est pas dans `Teams` (rapport
/// BGE-DB5687 : LeaderIcon.lua:143, bandeau des dirigeants arrêté et écran diplomatique bloqué).
/// Dans tout script, `#Teams[` devient `#BGD_EquipesSures[` ; la définition est ajoutée sur la
/// première ligne (après un BOM), sans saut de ligne : les numéros de ligne ne changent pas.
pub fn patch_equipes(src: &[u8]) -> Option<Vec<u8>> {
    const PAT: &[u8] = b"#Teams[";
    const NEW: &[u8] = b"#BGD_EquipesSures[";
    if !src.windows(PAT.len()).any(|w| w == PAT) {
        return None;
    }
    let bom = if src.starts_with(b"\xEF\xBB\xBF") { 3 } else { 0 };
    let mut out = Vec::with_capacity(src.len() + EQUIPES_DEF.len() + 256);
    out.extend_from_slice(&src[..bom]);
    out.extend_from_slice(EQUIPES_DEF);
    let mut i = bom;
    while i < src.len() {
        if src[i..].starts_with(PAT) {
            out.extend_from_slice(NEW);
            i += PAT.len();
        } else {
            out.push(src[i]);
            i += 1;
        }
    }
    Some(out)
}

/// Remplace `ContextPtr:SetHide(` dans les panneaux de MPH : afficher met le contexte dans la file
/// des popups, masquer l'en retire. Écrit sur une seule ligne, en tête : les numéros de ligne restent.
const POPUP_MPH_DEF: &[u8] = b"include(\"PopupPriorityLoader_\", true); BGD_PopupMPH = function(h) if h then if UIManager:IsInPopupQueue(ContextPtr) then UIManager:DequeuePopup(ContextPtr); end; ContextPtr:SetHide(true); elseif UIManager:IsInPopupQueue(ContextPtr) then ContextPtr:SetHide(false); else UIManager:QueuePopup(ContextPtr, PopupPriority.InGameTopOptionsMenu or ((PopupPriority.PausePanel or PopupPriority.Utmost) + 3)); end end; ";
const POPUP_MPH_DONE: &str = "panneau de MPH affiché au-dessus de la pause";

/// La pause de MPH (PausePanel) est une popup de priorité `PausePanel` (Utmost + 2) ; le menu
/// MULTIPLAYER HELPER (MPHOptions.lua) et le vote de remap (VotePanel.lua) sont de simples contextes
/// ajoutés à InGame, affichés par `ContextPtr:SetHide(false)` : toute popup passe au-dessus, et le
/// cadre de la pause (ConsumeMouse) cache leurs boutons. Leurs `ContextPtr:SetHide(` passent par la
/// file des popups, à la priorité du menu Échap (`InGameTopOptionsMenu`, Utmost + 5).
pub fn patch_popup_mph(src: &[u8]) -> Result<Vec<u8>, String> {
    const PAT: &[u8] = b"ContextPtr:SetHide(";
    const NEW: &[u8] = b"BGD_PopupMPH(";
    let has = |pat: &[u8]| src.windows(pat.len()).any(|w| w == pat);
    if !has(b"MPH Remap Panel") && !has(b"MPH very own In Game option menu") {
        return Err("pas le panneau de MPH".into());
    }
    if has(b"BGD_PopupMPH") {
        return Err("déjà corrigé".into());
    }
    if !has(PAT) {
        return Err("ContextPtr:SetHide introuvable".into());
    }
    let bom = if src.starts_with(b"\xEF\xBB\xBF") { 3 } else { 0 };
    let mut out = Vec::with_capacity(src.len() + POPUP_MPH_DEF.len() + 64);
    out.extend_from_slice(&src[..bom]);
    out.extend_from_slice(POPUP_MPH_DEF);
    let mut i = bom;
    while i < src.len() {
        if src[i..].starts_with(PAT) {
            out.extend_from_slice(NEW);
            i += PAT.len();
        } else {
            out.push(src[i]);
            i += 1;
        }
    }
    Ok(out)
}

const MOUVEMENTS_BLOCK: &str = include_str!("ui_mouvements.lua");

/// Pas de zone de déplacement pour une unité posée sur un quartier défendable ennemi : le parcours de
/// GameCore lui donne 0 PM au départ. `ui_mouvements.lua`, ajouté à la fin de SelectedUnit.lua,
/// enveloppe `RealizeMoveRadius` et refait la zone avec les chemins du survol. Il est précédé de
/// `local BGD_MOUV_MESURE = true|false;` (`ui_mesure_mouvements`) : les lignes « BGD mouvements: »
/// de Lua.log ne sont écrites qu'avec true. Voir analyses/quartier_capture_mouvements.
pub fn patch_selected_unit(src: &[u8]) -> Result<Vec<u8>, String> {
    let has = |pat: &str| src.windows(pat.len()).any(|w| w == pat.as_bytes());
    if let Some(missing) = ["local m_MovementRange", "\nfunction RealizeMoveRadius("].iter().find(|n| !has(n)) {
        return Err(format!("{} introuvable", missing.trim_start()));
    }
    if has("Civ6RolloverProxy (src/ui_mouvements.lua)") {
        return Err("déjà corrigé".into());
    }
    let mut out = src.to_vec();
    let mesure = MESURE_MOUVEMENTS.load(Ordering::Relaxed);
    out.extend_from_slice(format!("\r\nlocal BGD_MOUV_MESURE = {mesure};\r\n").as_bytes());
    out.extend_from_slice(MOUVEMENTS_BLOCK.as_bytes());
    Ok(out)
}

const COLON_BLOCK: &str = include_str!("ui_colon.lua");
const COLON_FIN: &str = include_str!("ui_colon_fin.lua");

/// Lentille du colon : les cases proches d'une ville fondée hors des terres du joueur restaient
/// coloriées comme fondables (cache de GameCore périmé). `BGD_FiltrerColon` (ui_colon.lua), inséré
/// avant `SetWaterHexes`, les repasse en interdites ; `OnCityAddedToMap` demande en plus un
/// recoloriage au lieu de vider la couche. Octets laissés tels quels. Voir analyses/filtre_colon.
pub fn patch_minimap_panel(src: &[u8]) -> Result<Vec<u8>, String> {
    const HEADER: &[u8] = b"\nfunction SetWaterHexes()";
    const CALL: &[u8] = b"= Map.GetContinentPlotsWaterAvailability();";
    // Sous pcall : le filtre passe entre l'effacement de la couche et son coloriage, une erreur
    // laisserait la lentille vide.
    const FILTRE: &[u8] = b"\r\n\tdo local ok, a, b, c, d = pcall(BGD_FiltrerColon, FullWaterPlots, CoastalWaterPlots, NoWaterPlots, NoSettlePlots); if ok and a ~= nil then FullWaterPlots, CoastalWaterPlots, NoWaterPlots, NoSettlePlots = a, b, c, d; end end";
    const CITY: &[u8] = b"\nfunction OnCityAddedToMap(";
    const CLEAR: &[u8] = b"UILens.ClearLayerHexes(m_HexColoringWaterAvail);";
    const REFRESH: &[u8] = b" m_refreshLens = true;";
    const FLUSH: &[u8] = b"if(m_WaterAvailabilityLensOn)then";
    const NEW_FLUSH: &[u8] = b"if(m_WaterAvailabilityLensOn or UILens.IsLayerOn(m_HexColoringWaterAvail))then";
    let all = |pat: &[u8]| src.windows(pat.len()).enumerate().filter(|(_, w)| *w == pat).map(|(i, _)| i).collect::<Vec<_>>();
    if !all(b"BGD_FiltrerColon").is_empty() {
        return Err("déjà corrigé".into());
    }
    if all(b"local m_refreshLens").is_empty() {
        return Err("m_refreshLens introuvable".into());
    }
    let one = |pat: &[u8], what: &str| match all(pat).as_slice() {
        [i] => Ok(*i),
        v => Err(format!("{what} : {} occurrences au lieu d'une", v.len())),
    };
    let header = one(HEADER, "SetWaterHexes")? + 1;
    let call = one(CALL, "GetContinentPlotsWaterAvailability")? + CALL.len();
    let city = one(CITY, "OnCityAddedToMap")?;
    let clear = all(CLEAR).into_iter().find(|&i| i > city).ok_or("ClearLayerHexes d'OnCityAddedToMap introuvable")?;
    let next = src[city + 1..].windows(10).position(|w| w == b"\nfunction ").map_or(src.len(), |i| city + 1 + i);
    let flush = one(FLUSH, "test de m_WaterAvailabilityLensOn")?;
    let flush_fn = one(b"\nfunction OnFlushChanges(", "OnFlushChanges")?;
    if !(header < call && clear < next && flush_fn < flush) {
        return Err("ancres dans un ordre inattendu".into());
    }
    let clear = clear + CLEAR.len();
    // (position, texte inséré, octets remplacés), dans l'ordre du fichier.
    let mut edits = vec![(header, COLON_BLOCK.as_bytes(), 0), (call, FILTRE, 0), (clear, REFRESH, 0), (flush, NEW_FLUSH, FLUSH.len())];
    edits.sort_by_key(|e| e.0);
    let mut out = Vec::with_capacity(src.len() + COLON_BLOCK.len() + COLON_FIN.len() + 512);
    let mut pos = 0;
    for (at, insert, skip) in edits {
        out.extend_from_slice(&src[pos..at]);
        out.extend_from_slice(insert);
        pos = at + skip;
    }
    out.extend_from_slice(&src[pos..]);
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(COLON_FIN.as_bytes());
    Ok(out)
}

/// Nom du fichier d'un script, en minuscules.
fn script_name(chunk: &str) -> String {
    chunk.rsplit(['/', '\\']).next().unwrap_or("").trim_start_matches('@').to_ascii_lowercase()
}

/// Scripts que `ui_epingles_opti.lua` allège : les drapeaux des épingles et leur liste.
const OPTIMIZED: [&str; 4] = ["mappinmanager.lua", "mappinmanager_dmt.lua", "mappinmanager_cqui.lua", "mappinlistpanel.lua"];

/// Nom du script, en minuscules, s'il fait partie de ceux que mesure `ui_epingles.lua` : ceux qui
/// reconstruisent quelque chose à chaque `PlayerInfoChanged`, et le calcul des rendements de DMT.
fn measured(chunk: &str) -> Option<String> {
    let name = script_name(chunk);
    // DiplomacyRibbonSupport.lua est une bibliothèque incluse ailleurs (LeaderIcon.lua), pas le bandeau.
    let ribbon = name == "diplomacyribbon.lua" || (name.starts_with("diplomacyribbon_") && name.ends_with(".lua"));
    (OPTIMIZED.contains(&name.as_str()) || name == "dmt_yieldcalculator.lua" || ribbon).then_some(name)
}

/// HavokScript : `os.clock` n'y avance que par secondes entières. Le bloc de mesure lit donc
/// `Civ6RolloverProxy_Clock()` (secondes, résolution QueryPerformanceCounter), posée dans la table
/// globale (`LUA_GLOBALSINDEX`, -10002, celle où l'exe pose `PlayerConfiguration`) avant chaque
/// script mesuré. La pose passe par `lua_cpcall` : si la table refuse l'écriture, l'erreur reste
/// dans Lua (d'où `C-unwind`) et le bloc se rabat sur `os.clock`.
const CLOCK_NAME: &[u8] = b"Civ6RolloverProxy_Clock\0";
const LUA_GLOBALSINDEX: i32 = -10002;

type LuaCFn = unsafe extern "C-unwind" fn(*mut c_void) -> i32;

struct Hks {
    push_number: unsafe extern "C-unwind" fn(*mut c_void, f64),
    push_closure: unsafe extern "C-unwind" fn(*mut c_void, LuaCFn, i32, *const c_char, i32),
    set_field: unsafe extern "C-unwind" fn(*mut c_void, i32, *const c_char),
    cpcall: unsafe extern "C-unwind" fn(*mut c_void, LuaCFn, *mut c_void) -> i32,
    set_top: unsafe extern "C-unwind" fn(*mut c_void, i32),
}

static HKS: std::sync::OnceLock<Option<Hks>> = std::sync::OnceLock::new();
static CLOCK_LOGGED: AtomicBool = AtomicBool::new(false);

unsafe fn hks() -> Option<&'static Hks> {
    HKS.get_or_init(|| {
        let m = win::GetModuleHandleW(win::wide("HavokScript_FinalRelease.dll").as_ptr());
        if m.is_null() {
            return None;
        }
        let get = |name: &[u8]| {
            let p = win::GetProcAddress(m, name.as_ptr());
            (!p.is_null()).then_some(p)
        };
        Some(Hks {
            push_number: std::mem::transmute(get(b"?lua_pushnumber@@YAXPEAUlua_State@@N@Z\0")?),
            push_closure: std::mem::transmute(get(b"?hks_pushnamedcclosure@@YAXPEAUlua_State@@P6AH0@ZHPEBDH@Z\0")?),
            set_field: std::mem::transmute(get(b"?lua_setfield@@YAXPEAUlua_State@@HPEBD@Z\0")?),
            cpcall: std::mem::transmute(get(b"?lua_cpcall@@YAHPEAUlua_State@@P6AH0@ZPEAX@Z\0")?),
            set_top: std::mem::transmute(get(b"?lua_settop@@YAXPEAUlua_State@@H@Z\0")?),
        })
    })
    .as_ref()
}

unsafe extern "C-unwind" fn lua_clock(l: *mut c_void) -> i32 {
    if let Some(h) = hks() {
        (h.push_number)(l, win::qpc_ms(win::qpc()) / 1000.0);
        return 1;
    }
    0
}

unsafe extern "C-unwind" fn install_clock(l: *mut c_void) -> i32 {
    if let Some(h) = hks() {
        (h.push_closure)(l, lua_clock, 0, CLOCK_NAME.as_ptr() as *const c_char, 0);
        (h.set_field)(l, LUA_GLOBALSINDEX, CLOCK_NAME.as_ptr() as *const c_char);
    }
    0
}

unsafe fn publish_clock(l: *mut c_void) {
    let Some(h) = hks() else {
        if !CLOCK_LOGGED.swap(true, Ordering::Relaxed) {
            log("ui : fonctions HavokScript introuvables, mesure des épingles avec os.clock (à la seconde)");
        }
        return;
    };
    let r = (h.cpcall)(l, install_clock, std::ptr::null_mut());
    if r != 0 {
        // lua_cpcall laisse l'erreur sur la pile.
        (h.set_top)(l, -2);
    }
    if !CLOCK_LOGGED.swap(true, Ordering::Relaxed) {
        log(&if r == 0 { "ui : horloge Civ6RolloverProxy_Clock posée".to_string() } else { format!("ui : horloge non posée ({r}), mesure avec os.clock") });
    }
}

/// `Civ6RolloverProxy_Pol()` → 1 si les scripts du congrès doivent écarter les spectateurs, sinon 0
/// (`spectateurs::lua_actif`). Relu à chaque appel : l'état POL n'est fixé qu'au chargement.
const POL_NAME: &[u8] = b"Civ6RolloverProxy_Pol\0";
static POL_LOGGED: AtomicBool = AtomicBool::new(false);

unsafe extern "C-unwind" fn lua_pol(l: *mut c_void) -> i32 {
    if let Some(h) = hks() {
        (h.push_number)(l, if crate::spectateurs::lua_actif() { 1.0 } else { 0.0 });
        return 1;
    }
    0
}

unsafe extern "C-unwind" fn install_pol(l: *mut c_void) -> i32 {
    if let Some(h) = hks() {
        (h.push_closure)(l, lua_pol, 0, POL_NAME.as_ptr() as *const c_char, 0);
        (h.set_field)(l, LUA_GLOBALSINDEX, POL_NAME.as_ptr() as *const c_char);
    }
    0
}

/// Pose `Civ6RolloverProxy_Pol` dans la table globale de l'état qui compile le script. Absente, les
/// blocs du congrès ne font rien.
unsafe fn publish_pol(l: *mut c_void) {
    let r = match hks() {
        Some(h) => {
            let r = (h.cpcall)(l, install_pol, std::ptr::null_mut());
            if r != 0 {
                (h.set_top)(l, -2);
            }
            r
        }
        None => -1,
    };
    if r != 0 || !POL_LOGGED.swap(true, Ordering::Relaxed) {
        log(&if r == 0 { "ui : Civ6RolloverProxy_Pol posée".to_string() } else { format!("ui : Civ6RolloverProxy_Pol non posée ({r}), congrès inchangé") });
    }
}

/// `Civ6RolloverProxy_Revote()` → 1 si un joueur peut revenir au vote après avoir fini son tour
/// pendant le congrès (`congres::actif` : détour posé, correctifs POL actifs), sinon 0.
const REVOTE_NAME: &[u8] = b"Civ6RolloverProxy_Revote\0";
static REVOTE_LOGGED: AtomicBool = AtomicBool::new(false);

unsafe extern "C-unwind" fn lua_revote(l: *mut c_void) -> i32 {
    if let Some(h) = hks() {
        (h.push_number)(l, if crate::congres::actif() { 1.0 } else { 0.0 });
        return 1;
    }
    0
}

unsafe extern "C-unwind" fn install_revote(l: *mut c_void) -> i32 {
    if let Some(h) = hks() {
        (h.push_closure)(l, lua_revote, 0, REVOTE_NAME.as_ptr() as *const c_char, 0);
        (h.set_field)(l, LUA_GLOBALSINDEX, REVOTE_NAME.as_ptr() as *const c_char);
    }
    0
}

/// Pose `Civ6RolloverProxy_Revote` dans la table globale de l'état qui compile le script. Absente,
/// l'écran d'attente du congrès n'a pas de bouton de retour au vote.
unsafe fn publish_revote(l: *mut c_void) {
    let r = match hks() {
        Some(h) => {
            let r = (h.cpcall)(l, install_revote, std::ptr::null_mut());
            if r != 0 {
                (h.set_top)(l, -2);
            }
            r
        }
        None => -1,
    };
    if r != 0 || !REVOTE_LOGGED.swap(true, Ordering::Relaxed) {
        log(&if r == 0 { "ui : Civ6RolloverProxy_Revote posée".to_string() } else { format!("ui : Civ6RolloverProxy_Revote non posée ({r}), pas de retour au vote") });
    }
}

/// Le script suivi du bloc d'optimisation (`opti`), puis du bloc de mesure (`mesure`), chacun
/// précédé de son contexte (`EpinglesOpti_Ctx`, `MesureEpingles_Ctx`). La mesure vient en dernier :
/// elle enveloppe les fonctions déjà optimisées. Octets laissés tels quels : les fichiers du jeu ne
/// sont pas tous en UTF-8.
pub fn patch_epingles(src: &[u8], ctx: &str, opti: bool, mesure: bool) -> Vec<u8> {
    let mut out = src.to_vec();
    if opti {
        out.extend_from_slice(format!("\r\nlocal EpinglesOpti_Ctx = \"{ctx}\";\r\n").as_bytes());
        out.extend_from_slice(OPTI_BLOCK.as_bytes());
    }
    if mesure {
        out.extend_from_slice(format!("\r\nlocal MesureEpingles_Ctx = \"{ctx}\";\r\n").as_bytes());
        out.extend_from_slice(MESURE_BLOCK.as_bytes());
    }
    out
}

/// Ce que `ui_accord_or.lua` attend de DiplomacyDealView.lua.
const ACCORD_OR_NAMES: [&str; 3] = ["function PopulateAvailableGold(", "g_IconOnlyIM", "g_IconAndTextIM"];

/// DiplomacyDealView.lua suivi de `ui_accord_or.lua` : le bouton d'or disponible, repris d'une
/// icône désactivée, est réactivé.
pub fn patch_accord_or(src: &[u8]) -> Result<Vec<u8>, String> {
    let has = |pat: &str| src.windows(pat.len()).any(|w| w == pat.as_bytes());
    if let Some(missing) = ACCORD_OR_NAMES.iter().find(|n| !has(n)) {
        return Err(format!("{missing} introuvable"));
    }
    if has("Civ6RolloverProxy (src/ui_accord_or.lua)") {
        return Err("déjà corrigé".into());
    }
    let mut out = src.to_vec();
    out.extend_from_slice(b"
");
    out.extend_from_slice(ACCORD_OR_BLOCK.as_bytes());
    Ok(out)
}

/// DiplomacyDealView.lua suivi de `ui_accord_fin.lua` : pas d'accord envoyé à un humain pendant le
/// passage de tour (tours passés en boucle chez l'hôte, depart_tour/README.md).
pub fn patch_accord_fin(src: &[u8]) -> Result<Vec<u8>, String> {
    let has = |pat: &str| src.windows(pat.len()).any(|w| w == pat.as_bytes());
    if !has("DealManager.SendWorkingDeal") {
        return Err("DealManager.SendWorkingDeal introuvable".into());
    }
    if has("Civ6RolloverProxy (src/ui_accord_fin.lua)") {
        return Err("déjà corrigé".into());
    }
    let mut out = src.to_vec();
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(ACCORD_FIN_BLOCK.as_bytes());
    Ok(out)
}

/// DiplomacyDealView.lua suivi de `ui_diag_accords.lua`, qui ne fait qu'écrire dans Lua.log.
pub fn patch_diag_accords(src: &[u8]) -> Vec<u8> {
    let mut out = src.to_vec();
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(DIAG_ACCORDS_BLOCK.as_bytes());
    out
}

/// ProductionHelper.lua suivi de `ui_merveilles.lua`.
pub fn patch_merveilles(src: &[u8]) -> Vec<u8> {
    let mut out = src.to_vec();
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(MERVEILLES_BLOCK.as_bytes());
    out
}

/// TopPanel.lua suivi de `ui_rendements.lua`, qui appelle `OnRefreshYields` : le fichier doit la
/// définir. BSM (TopPanel_Spectator.lua) et les extensions incluent ce fichier, ils en profitent.
pub fn patch_top_panel(src: &[u8]) -> Result<Vec<u8>, String> {
    let has = |pat: &str| src.windows(pat.len()).any(|w| w == pat.as_bytes());
    if !has("function OnRefreshYields(") {
        return Err("OnRefreshYields introuvable".into());
    }
    if has("Civ6RolloverProxy (src/ui_rendements.lua)") {
        return Err("déjà corrigé".into());
    }
    let mut out = src.to_vec();
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(RENDEMENTS_BLOCK.as_bytes());
    Ok(out)
}

/// WorldCongressPopup.lua suivi de `ui_congres.lua` si `spectateurs` (la fenêtre du congrès en
/// lecture seule pour un spectateur), puis de `ui_congres_vote.lua` (retour au vote). Les blocs
/// lisent les locales du fichier : on vérifie qu'elles y sont.
pub fn patch_congres(src: &[u8], spectateurs: bool) -> Result<Vec<u8>, String> {
    let has = |pat: &str| src.windows(pat.len()).any(|w| w == pat.as_bytes());
    if has("Civ6RolloverProxy (src/ui_congres") {
        return Err("déjà corrigé".into());
    }
    for pat in [
        "local m_IsInSession",
        "local m_CurrentPhase",
        "local m_CurrentStage",
        "local m_StartingFavor",
        "local m_WorkingFavor",
        "local PHASE_STEP_MAX",
        "local WORLD_CONGRESS_STAGE_1",
        "local WORLD_CONGRESS_STAGE_2",
        "local m_kResolutionVotes",
        "local m_kResolutionChoices",
        "local m_kProposalVotes",
        "function UpdateWorkingFavor(",
        "function OnAccept(",
        "function SetupWorldCongress(",
        "function OnLocalPlayerTurnEnd(",
        "function OnResumeCongress(",
    ] {
        if !has(pat) {
            return Err(format!("{pat} introuvable"));
        }
    }
    let mut out = src.to_vec();
    if spectateurs {
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(CONGRES_BLOCK.as_bytes());
    }
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(VOTE_BLOCK.as_bytes());
    Ok(out)
}

/// WorldCongressBetweenTurns.lua suivi de `ui_congres_attente.lua` : bouton de retour au vote.
pub fn patch_congres_attente(src: &[u8]) -> Result<Vec<u8>, String> {
    let has = |pat: &str| src.windows(pat.len()).any(|w| w == pat.as_bytes());
    if has("Civ6RolloverProxy (src/ui_congres_attente.lua)") {
        return Err("déjà corrigé".into());
    }
    for pat in [
        "local m_kPlayerStages",
        "local m_uiPlayersByID",
        "local COLOR_WAITING",
        "local LOC_PROPOSAL_WAITING",
        "function UpdateCanContinue(",
        "function OnHide(",
    ] {
        if !has(pat) {
            return Err(format!("{pat} introuvable"));
        }
    }
    let mut out = src.to_vec();
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(ATTENTE_BLOCK.as_bytes());
    Ok(out)
}

/// WorldCongressBetweenTurns.xml : le bouton `BGD_Revoter` sous la description de l'écran d'attente.
/// WorldTrackerResearch.xml / WorldTrackerCivic.xml : libellé `BGD_Avancement` dans l'en-tête du
/// panneau, après le bouton du nom (`ui_avancement.lua` le remplit).
pub fn patch_avancement_xml(src: &[u8]) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(src).map_err(|_| "texte non UTF-8".to_string())?;
    if text.contains("BGD_Avancement") {
        return Err("déjà corrigé".into());
    }
    let at = text.find("ID=\"TitleButton\"").ok_or("TitleButton introuvable")?;
    let end = text[at..].find("/>").ok_or("TitleButton non fermé")? + at + 2;
    let mut out = String::with_capacity(text.len() + AVANCEMENT_XML.len() + 8);
    out.push_str(&text[..end]);
    out.push_str("\r\n\t\t\t\t");
    out.push_str(AVANCEMENT_XML);
    out.push_str(&text[end..]);
    Ok(out.into_bytes())
}

/// WorldTracker.lua (jeu ou BSM) suivi de `ui_avancement.lua`.
pub fn patch_avancement(src: &[u8]) -> Result<Vec<u8>, String> {
    let has = |pat: &str| src.windows(pat.len()).any(|w| w == pat.as_bytes());
    if has("Civ6RolloverProxy (src/ui_avancement.lua)") {
        return Err("déjà corrigé".into());
    }
    for pat in ["function UpdateResearchPanel(", "function UpdateCivicsPanel(", "m_researchInstance", "m_civicsInstance"] {
        if !has(pat) {
            return Err(format!("{pat} introuvable"));
        }
    }
    Ok(append_blocks(src, &[AVANCEMENT_BLOCK]))
}

pub fn patch_congres_attente_xml(src: &[u8]) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(src).map_err(|_| "texte non UTF-8".to_string())?;
    if text.contains("BGD_Revoter") {
        return Err("déjà corrigé".into());
    }
    let at = text.find("LOC_WORLD_CONGRESS_BETWEEN_TURNS_DESCRIPTION").ok_or("description introuvable")?;
    let end = text[at..].find("/>").ok_or("description non fermée")? + at + 2;
    let mut out = String::with_capacity(text.len() + ATTENTE_XML.len() + 2);
    out.push_str(&text[..end]);
    out.push_str("\r\n");
    out.push_str(ATTENTE_XML.trim_end());
    out.push_str(&text[end..]);
    Ok(out.into_bytes())
}

/// WorldCongressIntro.lua suivi de `ui_congres_intro.lua` : l'intro ne s'ouvre qu'une fois par tour
/// et par session, même si l'annulation de fin de tour d'un autre joueur relance l'événement.
pub fn patch_congres_intro(src: &[u8]) -> Result<Vec<u8>, String> {
    let has = |pat: &str| src.windows(pat.len()).any(|w| w == pat.as_bytes());
    if has("Civ6RolloverProxy (src/ui_congres_intro.lua)") {
        return Err("déjà corrigé".into());
    }
    // Les rappels des événements appellent OnOpen par son nom global : l'enveloppe les atteint.
    for pat in ["function OnOpen(", "OnOpen(1); end", "OnOpen(2); end"] {
        if !has(pat) {
            return Err(format!("{pat} introuvable"));
        }
    }
    let mut out = src.to_vec();
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(INTRO_BLOCK.as_bytes());
    Ok(out)
}

/// WorldCongress.lua (script de gameplay) précédé de `ui_congres_cibles.lua` : les cibles
/// « joueur » des résolutions tirées de `BGD_MajorsCibles()`, sans les spectateurs, et la liste des
/// luxes recalculée à chaque session (`BGD_LuxesARecalculer()`).
pub fn patch_congres_cibles(src: &[u8]) -> Result<Vec<u8>, String> {
    const APPEL: &str = "PlayerManager.GetAliveMajors()";
    // Le test du cache des luxes de WC_Validate_LuxuryBan, entre les parenthèses de son `if`.
    const LUXES: &str = "(cachedLuxuryResources == nil)";
    let text = String::from_utf8_lossy(src);
    if text.contains("BGD_MajorsCibles") {
        return Err("déjà corrigé".into());
    }
    let n = text.matches(APPEL).count();
    if n == 0 {
        return Err(format!("{APPEL} introuvable"));
    }
    // Octets hors appel laissés tels quels : les fichiers du jeu ne sont pas tous en UTF-8.
    let mut out = CIBLES_BLOCK.as_bytes().to_vec();
    out.extend_from_slice(b"\r\n");
    let mut i = 0;
    while i < src.len() {
        if src[i..].starts_with(APPEL.as_bytes()) {
            out.extend_from_slice(b"BGD_MajorsCibles()");
            i += APPEL.len();
        } else if src[i..].starts_with(LUXES.as_bytes()) {
            out.extend_from_slice(b"(cachedLuxuryResources == nil or BGD_LuxesARecalculer())");
            i += LUXES.len();
        } else {
            out.push(src[i]);
            i += 1;
        }
    }
    Ok(out)
}

type Fix = fn(&[u8]) -> Result<Vec<u8>, String>;

/// Corrections appliquées dès que l'accroche Lua est posée : fichier, correction, journal.
const FIXES: [(&str, Fix, &str); 13] = [
    ("MPHOptions.lua", patch_popup_mph, POPUP_MPH_DONE),
    ("VotePanel.lua", patch_popup_mph, POPUP_MPH_DONE),
    ("DiplomacyActionView.lua", patch_diplomacy_action_view, DIPLOMATIE_DONE),
    ("MinimapPanel.lua", patch_minimap_panel, "lentille du colon : cases proches d'une ville interdites"),
    ("SelectedUnit.lua", patch_selected_unit, "zone de déplacement sur un quartier ennemi défendable"),
    ("InGame.lua", patch_in_game, RACCOURCIS_DONE),
    ("CityStates.lua", patch_city_states, EMISSAIRES_DONE),
    ("ActionPanel.lua", patch_action_panel, "Entrée suit la touche de « Action suivante »"),
    ("CityPanel.lua", patch_city_panel_complet, "owningPlayerID corrigé, panneau redessiné quand la file change"),
    ("UnitFlagManager.lua", patch_unit_flags, "clic sur un drapeau en visée d'une ville ou d'un quartier"),
    ("ProductionPanel.lua", patch_production_panel, "production en cours affichée au changement de ville, retirée au clic droit"),
    ("GreatPeoplePopup.lua", patch_great_people, "effet d'Imhotep à la vitesse de la partie"),
    ("CivilopediaPage_GreatPerson.lua", patch_pedia_great_person, "effet d'Imhotep à la vitesse de la partie"),
];

/// Les cadres que les calques de ui_boutons.xml reproduisent : sans eux, les boutons déplacés ne
/// seraient plus à leur place, et on ne patche rien.
const FRAMES: [&str; 6] = [
    r#"<ZoomAnchor ID="Anchor" ZoomOffsetNear="0,0,0" ZoomOffsetFar="0,0,-20">"#,
    r#"<Container ID="Container" Anchor="C,C" Offset="0,-20" Size="200,41">"#,
    r#"<Container ID="EncampmentBannerContainer" Anchor="C,C" Size="106,34">"#,
    r#"<Container ID="WMDBannerContainer" Anchor="C,C" Size="88,34">"#,
    r#"<Container Anchor="L,C" Offset="25,0">"#,
    r#"<Container Anchor="R,C" Offset="25,0">"#,
];

/// Texte modifié de CityBannerInstances.xml : calques ajoutés, `ConsumeMouse` sur les boutons
/// nucléaires (le bouton d'attaque l'a déjà).
pub fn patch_xml(src: &[u8]) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(src).map_err(|_| "texte non UTF-8".to_string())?;
    if let Some(missing) = FRAMES.iter().find(|f| !text.contains(*f)) {
        return Err(format!("cadre introuvable : {missing}"));
    }
    let end = text.rfind("</Include>").ok_or("balise </Include> introuvable")?;
    let mut out = String::with_capacity(text.len() + XML_BLOCK.len() + 64);
    out.push_str(&text[..end]);
    out.push_str(XML_BLOCK);
    out.push_str(&text[end..]);
    for id in ["NukeBombButton", "ThermoNukeBombButton"] {
        let open = format!(r#"<Button ID="{id}""#);
        let at = out.find(&open).ok_or_else(|| format!("{id} introuvable"))?;
        let tag_end = out[at..].find('>').ok_or_else(|| format!("{id} : balise non fermée"))? + at;
        if !out[at..tag_end].contains("ConsumeMouse") {
            out.insert_str(at + open.len(), r#" ConsumeMouse="1""#);
        }
    }
    Ok(out.into_bytes())
}

// ---------------------------------------------------------------------------------------------
// Accroches.

pub(crate) type LoadBuffer = unsafe extern "C" fn(*mut c_void, *const u8, usize, *const c_char) -> i32;

/// Le vrai `luaL_loadbuffer`, une fois l'accroche posée : `mods_ui` y compile ses fichiers embarqués.
pub(crate) unsafe fn real_loadbuffer() -> Option<LoadBuffer> {
    let p = REAL_LOADBUFFER.load(Ordering::Relaxed);
    (p != 0).then(|| std::mem::transmute(p))
}

/// Correctifs de `ui` (sans les reprises de mods) : le texte modifié, ou None. Ce qui est fait va
/// dans `what`, ce qui ne l'est pas va au journal.
unsafe fn patch_ui(l: *mut c_void, chunk: &str, src: &[u8], what: &mut Vec<String>) -> Option<Vec<u8>> {
    let ctx = script_name(chunk);
    let opti = OPTI.load(Ordering::Relaxed) && OPTIMIZED.contains(&ctx.as_str());
    let mesure = MESURE.load(Ordering::Relaxed) && measured(chunk).is_some();
    if opti || mesure {
        if mesure {
            publish_clock(l);
        }
        what.push(
            match (opti, mesure) {
                (true, true) => "épingles allégées et mesurées",
                (true, false) => "épingles allégées",
                _ => "mesure des épingles ajoutée",
            }
            .into(),
        );
        return Some(patch_epingles(src, &ctx, opti, mesure));
    }
    if is_file(chunk, "WorldViewPlotMessages.lua") && crate::canaux::pose() && crate::canaux::publier(l) {
        what.push("option de partie BBG_CANALS_ON_HILLS relue (canal de Panama sur colline)".into());
        return Some(append_blocks(src, &[crate::canaux::BLOC_LUA]));
    }
    if MERVEILLES.load(Ordering::Relaxed) && ctx == "productionhelper.lua" {
        what.push("rattrapage des merveilles ajouté".into());
        return Some(patch_merveilles(src));
    }
    if is_file(chunk, "WorldCongressPopup.lua") {
        publish_pol(l);
        publish_revote(l);
        let spectateurs = crate::spectateurs::poses();
        return match patch_congres(src, spectateurs) {
            Ok(text) => {
                if spectateurs {
                    what.push("fenêtre du congrès en lecture seule pour les spectateurs".into());
                }
                what.push("retour au vote".into());
                Some(text)
            }
            Err(e) => {
                log(&format!("ui : {chunk} laissé tel quel : {e}"));
                None
            }
        };
    }
    if is_file(chunk, "WorldCongressIntro.lua") {
        return match patch_congres_intro(src) {
            Ok(text) => {
                what.push("intro du congrès une seule fois par tour et par session".into());
                Some(text)
            }
            Err(e) => {
                log(&format!("ui : {chunk} laissé tel quel : {e}"));
                None
            }
        };
    }
    if is_file(chunk, "WorldCongressBetweenTurns.lua") {
        publish_revote(l);
        return match patch_congres_attente(src) {
            Ok(text) => {
                what.push("bouton de retour au vote".into());
                Some(text)
            }
            Err(e) => {
                log(&format!("ui : {chunk} laissé tel quel : {e}"));
                None
            }
        };
    }
    if crate::spectateurs::poses() && is_file(chunk, "WorldCongress.lua") {
        publish_pol(l);
        return match patch_congres_cibles(src) {
            Ok(text) => {
                what.push("spectateurs retirés des cibles des résolutions, luxes recalculés à chaque session".into());
                Some(text)
            }
            Err(e) => {
                log(&format!("ui : {chunk} laissé tel quel : {e}"));
                None
            }
        };
    }
    if AVANCEMENT.load(Ordering::Relaxed) && is_file(chunk, "WorldTracker.lua") {
        return match patch_avancement(src) {
            Ok(text) => {
                what.push("avancement de la technologie et du dogme en cours".into());
                if crate::avancement::publier(l) {
                    what.push("excédents de science et de culture".into());
                }
                Some(text)
            }
            Err(e) => {
                log(&format!("ui : {chunk} laissé tel quel : {e}"));
                None
            }
        };
    }
    if crate::rendements_empire::enabled() && is_file(chunk, "TopPanel.lua") {
        return match patch_top_panel(src) {
            Ok(text) => {
                what.push("rendements redessinés après gouverneur ou personnage illustre".into());
                Some(text)
            }
            Err(e) => {
                log(&format!("ui : {chunk} laissé tel quel : {e}"));
                None
            }
        };
    }
    if is_file(chunk, "DiplomacyDealView.lua") {
        let fixed = match patch_accord_or(src) {
            Ok(text) => {
                what.push("bouton d'or disponible réactivé".into());
                Some(text)
            }
            Err(e) => {
                log(&format!("ui : {chunk} : bouton d'or laissé tel quel : {e}"));
                None
            }
        };
        let fixed = match patch_accord_fin(fixed.as_deref().unwrap_or(src)) {
            Ok(text) => {
                what.push("pas d'accord vers un humain pendant le passage de tour".into());
                Some(text)
            }
            Err(e) => {
                log(&format!("ui : {chunk} : accords en fin de tour laissés tels quels : {e}"));
                fixed
            }
        };
        if DIAG_ACCORDS.load(Ordering::Relaxed) {
            what.push("diagnostic des accords ajouté".into());
            return Some(patch_diag_accords(fixed.as_deref().unwrap_or(src)));
        }
        return fixed;
    }
    if let Some((text, done)) = crate::depart::patch_lua(l, chunk, src) {
        what.push(done.into());
        // InGame.lua et ActionPanel.lua reçoivent toujours le bloc du départ : leur correctif
        // (raccourcis, touche Entrée) s'y ajoute.
        for (file, patch, fix_done) in FIXES {
            if !is_file(chunk, file) {
                continue;
            }
            return match patch(&text) {
                Ok(both) => {
                    what.push(fix_done.into());
                    Some(both)
                }
                Err(e) => {
                    log(&format!("ui : {chunk} : « {fix_done} » laissé tel quel : {e}"));
                    Some(text)
                }
            };
        }
        return Some(text);
    }
    for (file, patch, done) in FIXES {
        if !is_file(chunk, file) {
            continue;
        }
        if file == "SelectedUnit.lua" {
            // Mesure de ui_mouvements.lua : horloge à la milliseconde (os.clock est à la seconde).
            publish_clock(l);
        }
        let fixed = match patch(src) {
            Ok(text) => {
                what.push(done.into());
                Some(text)
            }
            Err(e) => {
                log(&format!("ui : {chunk} laissé tel quel : {e}"));
                None
            }
        };
        if file == "ProductionPanel.lua" {
            return production_extras(l, fixed.as_deref().unwrap_or(src), what).or(fixed);
        }
        if file == "CityPanel.lua" && crate::debordement::pose() && crate::debordement::publier(l) {
            what.push("production en réserve".into());
            return Some(append_blocks(fixed.as_deref().unwrap_or(src), &[DEBORDEMENT_BLOCK]));
        }
        return fixed;
    }
    if !is_file(chunk, "CityBannerManager.lua") {
        return None;
    }
    let boutons = BOUTONS.load(Ordering::Relaxed) && XML_PATCHED.load(Ordering::Relaxed);
    if BOUTONS.load(Ordering::Relaxed) && !boutons {
        log("ui : calques absents de CityBannerInstances.xml, boutons laissés dans les bannières");
    }
    let (new, notes) = patch_lua(src, boutons, RECENTRAGE.load(Ordering::Relaxed));
    for n in &notes {
        log(&format!("ui : {chunk} : {n}"));
    }
    if new.is_some() {
        what.push(format!(
            "modifié (boutons {}, recentrage {})",
            if boutons { "au-dessus" } else { "inchangés" },
            if RECENTRAGE.load(Ordering::Relaxed) { "gardé" } else { "retiré" }
        ));
    }
    let mut blocks: Vec<&str> = Vec::new();
    if ANNULER.load(Ordering::Relaxed) {
        blocks.push(ANNULER_BLOCK);
        what.push("clic droit sur la production du bandeau : retrait".into());
    }
    if crate::debordement::pose() && crate::debordement::publier(l) {
        blocks.push(DEBORDEMENT_BLOCK);
        what.push("production en réserve sur le bandeau".into());
    }
    if blocks.is_empty() {
        return new;
    }
    Some(append_blocks(new.as_deref().unwrap_or(src), &blocks))
}

/// Le texte suivi des blocs, chacun sur sa ligne. Octets laissés tels quels.
fn append_blocks(src: &[u8], blocks: &[&str]) -> Vec<u8> {
    let mut out = src.to_vec();
    for b in blocks {
        out.extend_from_slice(b"
");
        out.extend_from_slice(b.as_bytes());
    }
    out
}

/// ProductionPanel.lua : blocs des réglages `ui_reduction_quartiers` (badge des quartiers en
/// réduction) et `ui_debordement` (production en réserve), après `ui_production.lua`. None si aucun.
unsafe fn production_extras(l: *mut c_void, src: &[u8], what: &mut Vec<String>) -> Option<Vec<u8>> {
    let mut blocks: Vec<&str> = Vec::new();
    if QUARTIERS.load(Ordering::Relaxed) {
        blocks.push(QUARTIERS_BLOCK);
        what.push("badge des quartiers en réduction".into());
    }
    if crate::debordement::pose() && crate::debordement::publier(l) {
        blocks.push(DEBORDEMENT_BLOCK);
        what.push("production en réserve".into());
    }
    (!blocks.is_empty()).then(|| append_blocks(src, &blocks))
}

/// Trois étapes : le texte entier remplacé par une reprise de mod (`mods_ui::remplacer`), les
/// correctifs de `ui`, puis les blocs des reprises (`mods_ui::ajouter`). Une seule compilation.
unsafe extern "C" fn hook_loadbuffer(l: *mut c_void, buf: *const u8, size: usize, name: *const c_char) -> i32 {
    let real: LoadBuffer = std::mem::transmute(REAL_LOADBUFFER.load(Ordering::Relaxed));
    if name.is_null() || buf.is_null() {
        return real(l, buf, size, name);
    }
    let chunk = CStr::from_ptr(name).to_string_lossy();
    if JOURNAL.load(Ordering::Relaxed) {
        log(&format!("ui : script {chunk} ({size} octets)"));
    }
    let src = std::slice::from_raw_parts(buf, size);
    let entree_bandeau = crate::mods_ui::prendre_entree_bandeau();
    let mut what: Vec<String> = Vec::new();
    let mut mods = false;
    let mut text: Option<Vec<u8>> = None;
    match crate::mods_ui::remplacer(&chunk, src) {
        Some(Ok((t, w))) => {
            text = Some(t);
            what.push(w);
            mods = true;
        }
        Some(Err(e)) => log(&format!("ui : {chunk} : reprise de mod impossible : {e}")),
        None => {}
    }
    if let Some(t) = patch_ui(l, &chunk, text.as_deref().unwrap_or(src), &mut what) {
        text = Some(t);
    }
    match crate::mods_ui::ajouter(&chunk, text.as_deref().unwrap_or(src), entree_bandeau) {
        Some(Ok((t, w))) => {
            text = Some(t);
            what.push(w);
            mods = true;
        }
        Some(Err(e)) => log(&format!("ui : {chunk} : reprise de mod laissée de côté : {e}")),
        None => {}
    }
    if let Some(t) = patch_equipes(text.as_deref().unwrap_or(src)) {
        text = Some(t);
        what.push("équipes absentes de Teams tolérées".into());
    }
    let Some(text) = text else { return real(l, buf, size, name) };
    if mods {
        crate::mods_ui::publier(l);
    }
    let r = real(l, text.as_ptr(), text.len(), name);
    log(&format!(
        "ui : {chunk} : {}, compilation {}",
        what.join(" ; "),
        if r == 0 { "réussie".to_string() } else { format!("en échec ({r})") }
    ));
    r
}

unsafe extern "C" fn hook_read_xml(path: *const u16, buf: *mut GameBuf) -> i32 {
    let real: ReadXml = std::mem::transmute(XML_TRAMP.load(Ordering::Relaxed));
    let r = real(path, buf);
    if r != 0 || path.is_null() || buf.is_null() {
        return r;
    }
    let n = (0..).take_while(|&i| *path.add(i) != 0).count();
    let name = String::from_utf16_lossy(std::slice::from_raw_parts(path, n));
    if JOURNAL.load(Ordering::Relaxed) {
        log(&format!("ui : XML {name}"));
    }
    crate::mods_ui::xml_lu(&name);
    if let Some(res) = crate::mods_ui::xml(&name, (*buf).text().unwrap_or(&[])) {
        match res {
            Ok((text, what)) if replace_buffer(&mut *buf, &text) => log(&format!("ui : {name} : {what}")),
            Ok(_) => log(&format!("ui : {name} : allocation impossible, laissé tel quel")),
            Err(e) => log(&format!("ui : {name} laissé tel quel : {e}")),
        }
        return r;
    }
    if crate::congres::pose() && is_file(&name, "WorldCongressBetweenTurns.xml") {
        let b = &mut *buf;
        let Some(src) = b.text() else { return r };
        match patch_congres_attente_xml(src) {
            Ok(text) if replace_buffer(b, &text) => log(&format!("ui : {name} : bouton de retour au vote ajouté")),
            Ok(_) => log(&format!("ui : {name} : allocation impossible, laissé tel quel")),
            Err(e) => log(&format!("ui : {name} laissé tel quel : {e}")),
        }
        return r;
    }
    if AVANCEMENT.load(Ordering::Relaxed)
        && (is_file(&name, "WorldTrackerResearch.xml") || is_file(&name, "WorldTrackerCivic.xml"))
    {
        let b = &mut *buf;
        let Some(src) = b.text() else { return r };
        match patch_avancement_xml(src) {
            Ok(text) if replace_buffer(b, &text) => log(&format!("ui : {name} : libellé d'avancement ajouté")),
            Ok(_) => log(&format!("ui : {name} : allocation impossible, laissé tel quel")),
            Err(e) => log(&format!("ui : {name} laissé tel quel : {e}")),
        }
        return r;
    }
    if !BOUTONS.load(Ordering::Relaxed) || !is_file(&name, "CityBannerInstances.xml") {
        return r;
    }
    let b = &mut *buf;
    XML_PATCHED.store(false, Ordering::Relaxed);
    let Some(src) = b.text() else { return r };
    match patch_xml(src) {
        Ok(text) if replace_buffer(b, &text) => {
            XML_PATCHED.store(true, Ordering::Relaxed);
            log(&format!("ui : {name} : calques ajoutés"));
        }
        Ok(_) => log(&format!("ui : {name} : allocation impossible, laissé tel quel")),
        Err(e) => log(&format!("ui : {name} laissé tel quel : {e}")),
    }
    r
}

pub unsafe fn on_gamecore_loaded() {
    if !active() || IAT_SLOT.load(Ordering::Relaxed) != 0 {
        return;
    }
    let exe = win::GetModuleHandleW(std::ptr::null()) as usize;

    // GenVFX needs XML even with every local UI option off. The lobby verdict is read per context.
    // Si le proxy de démarrage est installé, la lecture est déjà détournée : on s'enchaîne.
    match exe::hook_read_xml(exe, hook_read_xml as *const () as usize) {
        Ok(h) => {
            XML_TRAMP.store(h.tramp, Ordering::Relaxed);
            if h.chained {
                log("ui : lecture XML déjà détournée (proxy de démarrage), détour enchaîné");
            }
            *XML_HOOK.lock().unwrap() = Some(h);
        }
        Err(e) => log(&format!("ui : {e}, pas de calques")),
    }

    let Some(slot) = iat_slot(exe, "HavokScript_FinalRelease.dll", "luaL_loadbuffer") else {
        log("ui : import luaL_loadbuffer introuvable dans l'exe, interface inchangée");
        return;
    };
    REAL_LOADBUFFER.store(*(slot as *const usize), Ordering::Relaxed);
    if !write_slot(slot, hook_loadbuffer as *const () as usize) {
        log("ui : VirtualProtect a échoué sur la table d'imports");
        return;
    }
    IAT_SLOT.store(slot, Ordering::Relaxed);
    log(&format!(
        "ui : accroches posées (lecture XML {}), boutons {}, recentrage {}, journal {}, mesure des épingles {}, épingles allégées {}, merveilles {}, rendements {}",
        if XML_TRAMP.load(Ordering::Relaxed) != 0 { "oui" } else { "non" },
        if BOUTONS.load(Ordering::Relaxed) { "on" } else { "off" },
        if RECENTRAGE.load(Ordering::Relaxed) { "on" } else { "off" },
        if JOURNAL.load(Ordering::Relaxed) { "on" } else { "off" },
        if MESURE.load(Ordering::Relaxed) { "on" } else { "off" },
        if OPTI.load(Ordering::Relaxed) { "on" } else { "off" },
        if MERVEILLES.load(Ordering::Relaxed) { "on" } else { "off" },
        if crate::rendements_empire::enabled() { "on" } else { "off" },
    ));
    if crate::mods_ui::any() {
        log(&format!("ui : mods repris : {}", crate::mods_ui::journal_reglages()));
    }
}

/// Le proxy est déchargé alors que l'exe reste en mémoire : on rend l'import et la lecture XML.
pub unsafe fn on_proxy_unload() {
    let slot = IAT_SLOT.swap(0, Ordering::Relaxed);
    if slot != 0 && *(slot as *const usize) == hook_loadbuffer as *const () as usize {
        write_slot(slot, REAL_LOADBUFFER.load(Ordering::Relaxed));
    }
    if let Some(h) = XML_HOOK.lock().ok().and_then(|mut g| g.take()) {
        exe::unhook_read_xml(&h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exe::{find_sig, is_import, rd};

    const GAME: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Sid Meier's Civilization VI";
    const BSM: &str = r"C:\Program Files (x86)\Steam\steamapps\workshop\content\289070\1916397407";
    const MPH: &str = r"C:\Program Files (x86)\Steam\steamapps\workshop\content\289070\3307282026";

    fn read(path: String) -> Option<Vec<u8>> {
        let r = std::fs::read(&path).ok();
        if r.is_none() {
            eprintln!("absent, test sauté : {path}");
        }
        r
    }

    #[test]
    fn noms_de_fichiers() {
        assert!(is_file("C:/x/UI/Replacements/CityBannerManager.lua", "CityBannerManager.lua"));
        assert!(is_file(r"..\..\DLC\citybannermanager.lua", "CityBannerManager.lua"));
        assert!(!is_file("C:/x/CityBannerManager_Expansion2.lua", "CityBannerManager.lua"));
        assert!(!is_file("C:/x/MyCityBannerManager.lua", "CityBannerManager.lua"));
    }

    #[test]
    fn bouton_d_or_reactive_dans_l_ecran_d_accord() {
        let Some(src) = read(format!(r"{GAME}\DLC\Expansion2\UI\Replacements\DiplomacyDealView.lua")) else { return };
        let new = patch_accord_or(&src).unwrap();
        assert!(new.starts_with(&src));
        assert!(new.ends_with(ACCORD_OR_BLOCK.as_bytes()));
        assert!(patch_accord_or(&new).is_err(), "pas deux fois");
        assert!(patch_accord_or(b"function OnShow() end").is_err());
    }

    #[test]
    fn accords_retenus_en_fin_de_tour() {
        let src = b"function OnPropose() DealManager.SendWorkingDeal(1, 0, 1); end";
        let out = patch_accord_fin(src).unwrap();
        assert!(out.starts_with(src));
        assert!(out.ends_with(ACCORD_FIN_BLOCK.as_bytes()));
        assert!(patch_accord_fin(&out).is_err(), "pas deux fois");
        assert!(patch_accord_fin(b"function OnShow() end").is_err());
        if let Some(src) = read(format!(r"{GAME}\DLC\Expansion2\UI\Replacements\DiplomacyDealView.lua")) {
            assert!(patch_accord_fin(&src).is_ok());
        }
    }

    #[test]
    fn diagnostic_des_accords_ajoute_a_l_ecran_d_accord() {
        let out = String::from_utf8(patch_diag_accords(b"Initialize();")).unwrap();
        assert!(out.starts_with("Initialize();\r\n-- ====="));
        assert!(out.ends_with(DIAG_ACCORDS_BLOCK));
        assert!(is_file(r"C:\x\DLC\Expansion2\UI\Replacements\DiplomacyDealView.lua", "DiplomacyDealView.lua"));
        assert!(!is_file(r"C:\x\ui\Replacements\diplomacydealview_MPH.lua", "DiplomacyDealView.lua"));
    }

    #[test]
    fn merveilles_ajoutees_a_production_helper() {
        assert_eq!(script_name(r"C:\x\Base\Assets\UI\ProductionHelper.lua"), "productionhelper.lua");
        let out = String::from_utf8(patch_merveilles(b"x = 1")).unwrap();
        assert!(out.starts_with("x = 1\r\n-- ====="));
        assert!(out.ends_with(MERVEILLES_BLOCK));
        let Some(src) = read(format!(r"{GAME}\Base\Assets\UI\ProductionHelper.lua")) else { return };
        assert!(String::from_utf8_lossy(&src).contains("function SwapQueueItem(sourceIndex, destIndex)"));
    }

    #[test]
    fn scripts_mesures() {
        assert_eq!(measured(r"C:\x\Base\Assets\UI\MapPinManager.lua").as_deref(), Some("mappinmanager.lua"));
        assert_eq!(measured("ui/mappinmanager_dmt.lua").as_deref(), Some("mappinmanager_dmt.lua"));
        assert_eq!(measured(r"..\DLC\Expansion2\UI\Replacements\DiplomacyRibbon_Expansion2.lua").as_deref(), Some("diplomacyribbon_expansion2.lua"));
        assert_eq!(measured("@MapPinListPanel.lua").as_deref(), Some("mappinlistpanel.lua"));
        assert!(measured("C:/x/MapPinPopup.lua").is_none());
        assert!(measured("C:/x/DiplomacyRibbon.xml").is_none());
        assert!(measured(r"C:\x\Base\Assets\UI\DiplomacyRibbonSupport.lua").is_none());
        assert_eq!(measured(r"C:\x\diplomacyribbon_spec.lua").as_deref(), Some("diplomacyribbon_spec.lua"));
        let out = patch_epingles(b"x = 1", "mappinmanager.lua", false, true);
        assert!(out.starts_with(b"x = 1\r\nlocal MesureEpingles_Ctx = \"mappinmanager.lua\";\r\n"));
        assert!(out.ends_with(MESURE_BLOCK.as_bytes()));
        // Optimisation d'abord, mesure ensuite : la mesure enveloppe les fonctions optimisées.
        let both = String::from_utf8(patch_epingles(b"x = 1", "mappinlistpanel.lua", true, true)).unwrap();
        let (o, m) = (both.find("local EpinglesOpti_Ctx = \"mappinlistpanel.lua\";").unwrap(), both.find("local MesureEpingles_Ctx").unwrap());
        assert!(o < m && both.ends_with(MESURE_BLOCK));
        assert_eq!(patch_epingles(b"x = 1", "a.lua", false, false), b"x = 1");
        assert!(!OPTIMIZED.contains(&script_name(r"C:\x\DiplomacyRibbon.lua").as_str()));
    }

    #[test]
    fn recentrage() {
        let src = "function OnICBMStrikeButtonClick( iPlotID, eWMD )\n\tUI.SelectCity(pCity);\nend\nfunction X()\n\tUI.SelectCity(a);\nend\n";
        let mut t = src.to_string();
        no_recenter(&mut t, "OnICBMStrikeButtonClick", "UI.SelectCity(").unwrap();
        assert!(t.contains("UI.SelectCity( pCity, false );"));
        assert!(t.contains("UI.SelectCity(a);"), "seule la fonction visée change");
        let mut t2 = t.clone();
        assert!(no_recenter(&mut t2, "OnICBMStrikeButtonClick", "UI.SelectCity(").is_err(), "pas deux fois");
        assert!(no_recenter(&mut t2, "Absente", "UI.SelectCity(").is_err());
    }

    fn check_lua(path: String) {
        let Some(src) = read(path) else { return };
        let (new, notes) = patch_lua(&src, true, false);
        assert!(notes.is_empty(), "{notes:?}");
        let new = String::from_utf8(new.unwrap()).unwrap();
        assert!(new.contains("UI.SelectCity( pCity, false )"));
        assert!(new.contains("UI.SelectDistrict( pDistrict, false )"));
        // Le fichier en contient déjà une, en commentaire : « Don't auto center ».
        let before = String::from_utf8_lossy(&src).matches(", false )").count();
        assert_eq!(new.matches(", false )").count(), before + 3);
        assert!(new.ends_with(LUA_BLOCK));
        // Les noms dont le bloc a besoin existent dans ce fichier.
        for name in ["local m_CityBannerIM", "local m_EncampmentBannerIM", "local m_WMDBannerIM", "local YOFFSET_2DVIEW",
            "local ZOFFSET_3DVIEW", "function CityBanner:UpdatePosition", "function CityBanner:UpdateVisibility",
            "function CityBanner:UpdateRangeStrike", "function CityBanner:UpdateWMDBanner", "function CityBanner:Uninitialize",
            "function OnShutdown"] {
            assert!(new.contains(name), "{name} absent");
        }
    }

    #[test]
    fn lua_du_jeu() {
        check_lua(format!(r"{GAME}\DLC\Expansion2\UI\CityBanners\CityBannerManager.lua"));
    }

    #[test]
    fn lua_de_bsm() {
        check_lua(format!(r"{BSM}\UI\Replacements\CityBannerManager.lua"));
    }

    #[test]
    fn xml_du_jeu() {
        let Some(src) = read(format!(r"{GAME}\DLC\Expansion2\UI\CityBanners\CityBannerInstances.xml")) else { return };
        let new = String::from_utf8(patch_xml(&src).unwrap()).unwrap();
        assert!(new.contains(r#"<Button ID="NukeBombButton" ConsumeMouse="1" Anchor="C,C""#));
        assert!(new.contains(r#"<Button ID="ThermoNukeBombButton" ConsumeMouse="1" Anchor="C,C""#));
        assert!(new.contains(r#"<Instance Name="ProxyStrikeWMD">"#));
        assert!(new.trim_end().ends_with("</Include>"));
        // Trois instances ajoutées, chacune fermée.
        let old = String::from_utf8_lossy(&src);
        for (open, close) in [("<Instance ", "</Instance>"), ("<ZoomAnchor", "</ZoomAnchor>")] {
            assert_eq!(new.matches(open).count(), old.matches(open).count() + 3, "{open}");
            assert_eq!(new.matches(close).count(), old.matches(close).count() + 3, "{close}");
        }
        assert!(patch_xml(b"<Include></Include>").is_err());
    }

    /// La table d'imports du binaire de test : son entrée pour GetModuleHandleW pointe sur la
    /// fonction que rend GetProcAddress.
    #[test]
    fn table_d_imports() {
        unsafe {
            let exe = win::GetModuleHandleW(std::ptr::null()) as usize;
            let slot = iat_slot(exe, "kernel32.dll", "GetModuleHandleW").expect("import introuvable");
            let k32 = win::GetModuleHandleW(win::wide("kernel32.dll").as_ptr());
            assert_eq!(*(slot as *const usize), win::GetProcAddress(k32, b"GetModuleHandleW\0".as_ptr()) as usize);
            assert!(iat_slot(exe, "kernel32.dll", "PasUneFonction").is_none());
            assert!(iat_slot(exe, "HavokScript_FinalRelease.dll", "luaL_loadbuffer").is_none());
        }
    }

    #[test]
    fn city_panel_du_jeu() {
        let Some(src) = read(format!(r"{GAME}\Base\Assets\UI\Panels\CityPanel.lua")) else { return };
        let new = String::from_utf8(patch_city_panel(&src).unwrap()).unwrap();
        assert!(!new.contains("owningPlayerID"));
        assert_eq!(new.len(), src.len() - 1, "owning → owner, rien d'autre");
        assert!(patch_city_panel(new.as_bytes()).is_err(), "pas deux fois");
        let complet = patch_city_panel_complet(&src).unwrap();
        assert!(complet.starts_with(new.as_bytes()), "faute corrigée, puis le bloc");
        assert!(complet.ends_with(PANNEAU_VILLE_BLOCK.as_bytes()));
        assert!(patch_city_panel_complet(&complet).is_err(), "bloc pas deux fois");
    }

    fn check_flags(path: String) {
        let Some(src) = read(path) else { return };
        let bytes = patch_unit_flags(&src).unwrap();
        // Rien d'autre ne change : le bloc, plus deux noms de fonction plus longs.
        let longer = "Civ6RolloverProxy_OnUnitFlagLClick".len() - "OnUnitFlagClick".len();
        assert_eq!(bytes.len(), src.len() + FLAG_BLOCK.len() + 2 * longer);
        let new = String::from_utf8_lossy(&bytes);
        assert_eq!(new.matches("RegisterCallback( Mouse.eLClick, Civ6RolloverProxy_OnUnitFlagLClick )").count(), 2);
        assert_eq!(new.matches("RegisterCallback( Mouse.eRClick, OnUnitFlagClick )").count(), 2, "clic droit inchangé");
        // Le gestionnaire est défini avant OnUnitFlagClick, donc avant tout branchement.
        assert!(new.find("function Civ6RolloverProxy_OnUnitFlagLClick").unwrap() < new.find("\nfunction OnUnitFlagClick(").unwrap());
        assert!(patch_unit_flags(&bytes).is_err(), "pas deux fois");
    }

    #[test]
    fn action_panel_du_jeu() {
        let Some(src) = read(format!(r"{GAME}\Base\Assets\UI\ActionPanel.lua")) else { return };
        let bytes = patch_action_panel(&src).unwrap();
        let ajout = " and (pInputStruct:IsShiftDown() or BGD_EntreeEstActionSuivante())".len();
        assert_eq!(bytes.len(), src.len() + ENTREE_BLOCK.len() + ajout);
        let new = String::from_utf8_lossy(&bytes);
        assert!(new.find("function BGD_EntreeEstActionSuivante").unwrap() < new.find("\nfunction OnInputHandler(").unwrap());
        assert!(new.contains("Keys.VK_RETURN and (pInputStruct:IsShiftDown() or BGD_EntreeEstActionSuivante()) then"));
        assert!(patch_action_panel(&bytes).is_err(), "pas deux fois");
        assert!(is_file(r"C:\x\Base\Assets\UI\ActionPanel.lua", "ActionPanel.lua"));
        assert!(!is_file(r"C:\x\DLC\Expansion2\UI\Replacements\ActionPanel_Expansion2.lua", "ActionPanel.lua"));
    }

    #[test]
    fn cites_etats_de_bsm() {
        let bloc = 2 + EMISSAIRES_BLOCK.len();
        if let Some(src) = read(format!(r"{GAME}\Base\Assets\UI\PartialScreens\CityStates.lua")) {
            let bytes = patch_city_states(&src).unwrap();
            assert_eq!(bytes.len(), src.len() + bloc, "Échap déjà correct dans le jeu : le bloc seul");
            assert!(bytes.ends_with(EMISSAIRES_BLOCK.as_bytes()));
            assert!(patch_city_states(&bytes).is_err(), "pas deux fois");
        }
        let Some(src) = read(format!(r"{BSM}\UI\CityStates.lua")) else { return };
        let bytes = patch_city_states(&src).unwrap();
        assert_eq!(bytes.len(), src.len() + ", OnClose".len() + bloc);
        assert!(String::from_utf8_lossy(&bytes).contains("m_kScreenSlideAnim.OnInputHandler( input, OnClose );"));
        assert!(patch_city_states(&bytes).is_err(), "pas deux fois");
        assert!(!is_file(r"C:\x\CityStates_Expansion2.lua", "CityStates.lua"));
    }

    #[test]
    fn diplomatie_refermee_si_l_ouverture_echoue() {
        let Some(src) = read(format!(r"{GAME}\Base\Assets\UI\DiplomacyActionView.lua")) else { return };
        let bytes = patch_diplomacy_action_view(&src).unwrap();
        assert!(bytes.starts_with(&src) && bytes.ends_with(DIPLOMATIE_BLOCK.as_bytes()));
        assert!(patch_diplomacy_action_view(&bytes).is_err(), "pas deux fois");
        assert!(!is_file(r"C:\x\DiplomacyActionView_Expansion2.lua", "DiplomacyActionView.lua"));
        assert!(!is_file(r"C:\x\diplomacyactionview_MPH.lua", "DiplomacyActionView.lua"));
    }

    #[test]
    fn equipes_absentes_de_teams() {
        assert!(patch_equipes(b"local n = #Players;").is_none());
        let out = patch_equipes(b"\xEF\xBB\xBFlocal a = 1\nif #Teams[t] > 1 then end\nx = #Teams[Teams[1][1]]").unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(out.starts_with(b"\xEF\xBB\xBFBGD_EquipesSures = "));
        assert!(!text.contains("#Teams["));
        assert!(text.contains("if #BGD_EquipesSures[t] > 1 then") && text.contains("#BGD_EquipesSures[Teams[1][1]]"));
        assert_eq!(text.matches('\n').count(), 2, "numéros de ligne gardés");
        for path in [
            format!(r"{GAME}\Base\Assets\UI\Instances\LeaderIcon.lua"),
            format!(r"{GAME}\Base\Assets\UI\DiplomacyActionView.lua"),
            format!(r"{GAME}\DLC\Expansion2\UI\Additions\EraProgressPanel.lua"),
            format!(r"{BSM}\UI\Replacements\leadericon.lua"),
        ] {
            let Some(src) = read(path) else { continue };
            let out = patch_equipes(&src).unwrap();
            assert!(!out.windows(7).any(|w| w == b"#Teams["));
            assert_eq!(out.iter().filter(|&&b| b == b'\n').count(), src.iter().filter(|&&b| b == b'\n').count());
        }
    }

    #[test]
    fn panneaux_de_mph_en_popup() {
        for file in ["MPHOptions.lua", "VotePanel.lua"] {
            let Some(src) = read(format!(r"{MPH}\ui\Additions\{file}")) else { continue };
            let out = patch_popup_mph(&src).unwrap();
            let text = String::from_utf8_lossy(&out);
            assert_eq!(text.matches("ContextPtr:SetHide(").count(), 2, "{file} : seulement dans la définition");
            assert!(text.matches("BGD_PopupMPH(").count() >= 3, "{file}");
            assert_eq!(out.iter().filter(|&&b| b == b'\n').count(), src.iter().filter(|&&b| b == b'\n').count());
            assert!(patch_popup_mph(&out).is_err(), "pas deux fois");
            assert!(is_file(&format!(r"{MPH}\ui\Additions\{file}"), file));
        }
        assert!(patch_popup_mph(b"ContextPtr:SetHide(true)").is_err(), "VotePanel.lua d'un autre mod");
    }

    #[test]
    fn in_game_du_jeu_et_de_mph() {
        for path in [format!(r"{GAME}\DLC\Expansion2\UI\Replacements\InGame.lua"), format!(r"{MPH}\ui\ingame.lua")] {
            let Some(src) = read(path) else { continue };
            let bytes = patch_in_game(&src).unwrap();
            assert!(bytes.starts_with(&src) && bytes.ends_with(RACCOURCIS_BLOCK.as_bytes()));
            assert!(patch_in_game(&bytes).is_err(), "pas deux fois");
            // Ce que patch_ui compile : le bloc du départ, puis celui des raccourcis.
            let (depart, _) = crate::depart::patch(r"C:\x\InGame.lua", &src).unwrap();
            let both = patch_in_game(&depart).unwrap();
            assert!(both.starts_with(&depart) && both.ends_with(RACCOURCIS_BLOCK.as_bytes()));
        }
        assert!(is_file(r"C:\x\ui\ingame.lua", "InGame.lua"));
        assert!(!is_file(r"C:\x\InGameTopOptionsMenu.lua", "InGame.lua"));
    }

    #[test]
    fn action_panel_recoit_le_depart_et_la_touche_entree() {
        let Some(src) = read(format!(r"{GAME}\Base\Assets\UI\ActionPanel.lua")) else { return };
        let (depart, _) = crate::depart::patch(r"C:\x\Base\Assets\UI\ActionPanel.lua", &src).unwrap();
        let seul = patch_action_panel(&src).unwrap();
        let both = patch_action_panel(&depart).unwrap();
        assert!(both.len() == seul.len() + (depart.len() - src.len()), "les deux blocs");
    }

    #[test]
    fn selected_unit_du_jeu() {
        let Some(src) = read(format!(r"{GAME}\Base\Assets\UI\WorldView\SelectedUnit.lua")) else { return };
        let bytes = patch_selected_unit(&src).unwrap();
        assert!(bytes.starts_with(&src) && bytes.ends_with(MOUVEMENTS_BLOCK.as_bytes()));
        assert!(String::from_utf8_lossy(&bytes).contains("\r\nlocal BGD_MOUV_MESURE = false;\r\n"), "mesure coupée par défaut");
        assert!(patch_selected_unit(&bytes).is_err(), "pas deux fois");
        assert!(!is_file(r"C:\x\SelectedUnit_Expansion2.lua", "SelectedUnit.lua"));
    }

    #[test]
    fn minimap_du_jeu() {
        let Some(src) = read(format!(r"{GAME}\Base\Assets\UI\MinimapPanel.lua")) else { return };
        let bytes = patch_minimap_panel(&src).unwrap();
        let new = String::from_utf8_lossy(&bytes);
        assert!(new.find("function BGD_FiltrerColon").unwrap() < new.find("\nfunction SetWaterHexes()").unwrap());
        assert!(new.contains("GetContinentPlotsWaterAvailability();\r\n\tdo local ok, a, b, c, d = pcall(BGD_FiltrerColon,"));
        let city = new.find("\nfunction OnCityAddedToMap(").unwrap();
        assert!(new[city..].contains("UILens.ClearLayerHexes(m_HexColoringWaterAvail); m_refreshLens = true;"));
        let flush = new.find("\nfunction OnFlushChanges(").unwrap();
        assert!(new[flush..].contains("if(m_WaterAvailabilityLensOn or UILens.IsLayerOn(m_HexColoringWaterAvail))then"));
        assert!(new.ends_with(COLON_FIN));
        assert!(patch_minimap_panel(&bytes).is_err(), "pas deux fois");
        assert!(!is_file(r"C:\x\MinimapPanel_Expansion2.lua", "MinimapPanel.lua"));
    }

    #[test]
    fn drapeaux_du_jeu() {
        check_flags(format!(r"{GAME}\Base\Assets\UI\UnitFlagManager.lua"));
    }

    #[test]
    fn top_panel_du_jeu() {
        let Some(src) = read(format!(r"{GAME}\DLC\Expansion2\UI\Replacements\TopPanel.lua")) else { return };
        let new = patch_top_panel(&src).unwrap();
        assert!(new.starts_with(&src) && new.ends_with(RENDEMENTS_BLOCK.as_bytes()));
        assert!(patch_top_panel(&new).is_err(), "pas deux fois");
        assert!(patch_top_panel(b"function RefreshYields() end").is_err());
        assert!(is_file(r"C:\x\DLC\Expansion2\UI\Replacements\TopPanel.lua", "TopPanel.lua"));
        assert!(!is_file("ui/topPanel_spectator.lua", "TopPanel.lua"));
    }

    #[test]
    fn congres_du_jeu() {
        let Some(src) = read(format!(r"{GAME}\DLC\Expansion2\UI\Additions\WorldCongressPopup.lua")) else { return };
        let new = patch_congres(&src, true).unwrap();
        assert!(new.starts_with(&src) && new.ends_with(VOTE_BLOCK.as_bytes()));
        let n = new.len() - VOTE_BLOCK.len() - 2;
        assert!(new[..n].ends_with(CONGRES_BLOCK.as_bytes()));
        assert!(patch_congres(&new, true).is_err(), "pas deux fois");
        let seul = patch_congres(&src, false).unwrap();
        assert_eq!(seul.len(), src.len() + 2 + VOTE_BLOCK.len(), "sans spectateurs, le bloc du vote seul");
        assert!(patch_congres(&seul, false).is_err(), "pas deux fois");
        assert!(patch_congres(b"function SetPhase() end", false).is_err());
        assert!(is_file(r"C:\x\DLC\Expansion2\UI\Additions\WorldCongressPopup.lua", "WorldCongressPopup.lua"));
        assert!(!is_file(r"C:\x\DLC\Expansion2\UI\Additions\WorldCongressPopup.lua", "WorldCongress.lua"));
    }

    #[test]
    fn cibles_du_congres_du_jeu() {
        let Some(src) = read(format!(r"{GAME}\DLC\Expansion2\Scripts\WorldCongress.lua")) else { return };
        let new = patch_congres_cibles(&src).unwrap();
        let text = String::from_utf8_lossy(&new);
        assert!(text.starts_with(CIBLES_BLOCK));
        // Les cinq validateurs à cible joueur ; le reste vient du bloc lui-même.
        let corps = &text[CIBLES_BLOCK.len()..];
        assert_eq!(corps.matches("BGD_MajorsCibles()").count(), 5);
        assert_eq!(corps.matches("PlayerManager.GetAliveMajors()").count(), 0);
        // Le cache des luxes, recalculé si POL : un seul test dans le jeu.
        let luxes = "(cachedLuxuryResources == nil or BGD_LuxesARecalculer())";
        assert_eq!(corps.matches(luxes).count(), 1);
        assert_eq!(corps.matches("cachedLuxuryResources == nil)").count(), 0);
        assert_eq!(
            new.len() + 5 * "PlayerManager.GetAliveMajors()".len() + "(cachedLuxuryResources == nil)".len(),
            src.len() + CIBLES_BLOCK.len() + 2 + 5 * "BGD_MajorsCibles()".len() + luxes.len()
        );
        assert!(patch_congres_cibles(&new).is_err(), "pas deux fois");
        assert!(patch_congres_cibles(b"local x = 1").is_err());
    }

    #[test]
    fn avancement_du_jeu_et_de_bsm() {
        assert!(is_file(r"C:\x\Base\Assets\UI\WorldTracker.lua", "WorldTracker.lua"));
        assert!(is_file(r"C:\x\UI\Replacements\worldtracker.lua", "WorldTracker.lua"));
        assert!(!is_file(r"C:\x\WorldTracker_Expansion1.lua", "WorldTracker.lua"));
        let bsm = r"C:\Program Files (x86)\Steam\steamapps\workshop\content\289070\1916397407\UI\Replacements\worldtracker.lua";
        for path in [format!(r"{GAME}\Base\Assets\UI\WorldTracker.lua"), bsm.to_string()] {
            let Some(src) = read(path) else { continue };
            let new = patch_avancement(&src).unwrap();
            assert!(new.starts_with(&src) && new.ends_with(AVANCEMENT_BLOCK.as_bytes()));
            assert!(patch_avancement(&new).is_err(), "pas deux fois");
        }
        assert!(patch_avancement(b"local x = 1").is_err());
        for path in [
            format!(r"{GAME}\Base\Assets\UI\WorldTrackerResearch.xml"),
            format!(r"{GAME}\Base\Assets\UI\WorldTrackerCivic.xml"),
            format!(r"{GAME}\DLC\Expansion2\UI\Replacements\WorldTrackerResearch.xml"),
        ] {
            let Some(xml) = read(path) else { continue };
            let new = String::from_utf8(patch_avancement_xml(&xml).unwrap()).unwrap();
            // Le libellé, juste après le bouton du nom, dans la même instance.
            let titre = new.find(r#"ID="TitleButton""#).unwrap();
            let label = new.find(r#"<Label ID="BGD_Avancement""#).unwrap();
            assert!(titre < label && label < new.find("</Instance>").unwrap());
            assert_eq!(new.len(), xml.len() + 6 + AVANCEMENT_XML.len());
            assert!(patch_avancement_xml(new.as_bytes()).is_err(), "pas deux fois");
        }
    }

    #[test]
    fn attente_du_congres_du_jeu() {
        let Some(src) = read(format!(r"{GAME}\DLC\Expansion2\UI\Additions\WorldCongressBetweenTurns.lua")) else { return };
        let new = patch_congres_attente(&src).unwrap();
        assert!(new.starts_with(&src) && new.ends_with(ATTENTE_BLOCK.as_bytes()));
        assert!(patch_congres_attente(&new).is_err(), "pas deux fois");
        assert!(is_file(r"C:\x\WorldCongressBetweenTurns.lua", "WorldCongressBetweenTurns.lua"));
        let Some(xml) = read(format!(r"{GAME}\DLC\Expansion2\UI\Additions\WorldCongressBetweenTurns.xml")) else { return };
        let new = String::from_utf8(patch_congres_attente_xml(&xml).unwrap()).unwrap();
        // Le bouton, dans la grille de la description, juste après celle-ci.
        let bouton = new.find(r#"<GridButton ID="BGD_Revoter""#).unwrap();
        let description = new.find("LOC_WORLD_CONGRESS_BETWEEN_TURNS_DESCRIPTION").unwrap();
        assert!(description < bouton && bouton < new[description..].find("</Grid>").unwrap() + description);
        assert!(patch_congres_attente_xml(new.as_bytes()).is_err(), "pas deux fois");
    }

    #[test]
    fn intro_du_congres_du_jeu() {
        let Some(src) = read(format!(r"{GAME}\DLC\Expansion2\UI\Additions\WorldCongressIntro.lua")) else { return };
        let new = patch_congres_intro(&src).unwrap();
        assert!(new.starts_with(&src) && new.ends_with(INTRO_BLOCK.as_bytes()));
        assert!(patch_congres_intro(&new).is_err(), "pas deux fois");
        assert!(is_file(r"C:\x\DLC\Expansion2\UI\Additions\WorldCongressIntro.lua", "WorldCongressIntro.lua"));
    }

    #[test]
    fn production_panel_du_jeu() {
        let Some(src) = read(format!(r"{GAME}\Base\Assets\UI\Panels\ProductionPanel.lua")) else { return };
        let new = patch_production_panel(&src).unwrap();
        assert!(new.starts_with(&src) && new.ends_with(PRODUCTION_BLOCK.as_bytes()));
        assert!(patch_production_panel(&new).is_err(), "pas deux fois");
        assert!(patch_production_panel(b"function View(data) end").is_err());
        assert!(is_file(r"C:\x\Base\Assets\UI\Panels\ProductionPanel.lua", "ProductionPanel.lua"));
        assert!(!is_file("ui/replacements/productionpanel_bbg.lua", "ProductionPanel.lua"));
    }

    #[test]
    fn imhotep() {
        let popup = b"function GetDisplayPlayerID()\r\nend\r\nfunction PopulateData( data:table, isPast:boolean )\r\nend";
        let new = patch_great_people(popup).unwrap();
        assert!(new.starts_with(popup) && new.ends_with(IMHOTEP_BLOCK.as_bytes()));
        assert!(patch_great_people(&new).is_err(), "pas deux fois");
        assert!(patch_great_people(b"function View(data) end").is_err());
        let pedia = b"include(\"GameEffectsText\")\r\nlocal text = GetModifierText(row.ModifierId, \"Summary\");";
        assert!(patch_pedia_great_person(pedia).unwrap().ends_with(IMHOTEP_BLOCK.as_bytes()));
        assert!(is_file(r"C:\x\Base\Assets\UI\Popups\GreatPeoplePopup.lua", "GreatPeoplePopup.lua"));
        assert!(!is_file(r"C:\x\DLC\Babylon\UI\Replacements\GreatPeoplePopup_Babylon_Heroes.lua", "GreatPeoplePopup.lua"));
        assert!(is_file(r"C:\x\Base\Assets\UI\Civilopedia\CivilopediaPage_GreatPerson.lua", "CivilopediaPage_GreatPerson.lua"));
        if let Some(src) = read(format!(r"{GAME}\Base\Assets\UI\Popups\GreatPeoplePopup.lua")) {
            assert!(patch_great_people(&src).is_ok());
        }
        if let Some(src) = read(format!(r"{GAME}\Base\Assets\UI\Civilopedia\CivilopediaPage_GreatPerson.lua")) {
            assert!(patch_pedia_great_person(&src).is_ok());
        }
    }

    #[test]
    fn drapeaux_de_bsm() {
        check_flags(format!(r"{BSM}\UI\Replacements\unitflagmanager.lua"));
    }

    #[test]
    fn noms_decores() {
        let mangled = b"?luaL_loadbuffer@@YAHPEAUlua_State@@PEBD_K1@Z";
        assert!(is_import(mangled, "luaL_loadbuffer"));
        assert!(is_import(b"luaL_loadbuffer", "luaL_loadbuffer"));
        assert!(!is_import(b"?luaL_loadbufferx@@YAHPEAUlua_State@@PEBD_K1@Z", "luaL_loadbuffer"));
        assert!(!is_import(b"?luaL_loadstring@@YAHPEAUlua_State@@PEBD@Z", "luaL_loadbuffer"));
    }

    /// L'entrée luaL_loadbuffer des vrais exe, projetés comme images sans être exécutés : celle que
    /// Ghidra montre en DX12 (`140fa54e8` au build 15296837, `140fa62e0` au build 26407122).
    #[test]
    fn import_havokscript_des_exe() {
        #[link(name = "kernel32")]
        extern "system" {
            fn LoadLibraryExW(name: *const u16, file: *mut c_void, flags: u32) -> win::HMODULE;
            fn FreeLibrary(module: win::HMODULE) -> i32;
        }
        const LOAD_LIBRARY_AS_IMAGE_RESOURCE: u32 = 0x20;
        for exe in ["CivilizationVI_DX12.exe", "CivilizationVI.exe"] {
            let path = format!(r"{GAME}\Base\Binaries\Win64Steam\{exe}");
            if !std::path::Path::new(&path).exists() {
                eprintln!("absent, test sauté : {path}");
                continue;
            }
            unsafe {
                let h = LoadLibraryExW(win::wide(&path).as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_AS_IMAGE_RESOURCE);
                assert!(!h.is_null(), "{exe}");
                // Les bits bas du handle signalent une image projetée en ressource.
                let base = h as usize & !3;
                let slot = iat_slot(base, "HavokScript_FinalRelease.dll", "luaL_loadbuffer").unwrap_or_else(|| panic!("{exe}"));
                if exe == "CivilizationVI_DX12.exe" {
                    assert!([0xfa54e8, 0xfa62e0].contains(&(slot - base)), "{:#x}", slot - base);
                }
                FreeLibrary(h);
            }
        }
    }

    #[test]
    fn signature_xml() {
        // Adresse de chaque build connu : 15296837, puis 26407122 (exe du 2026-11-02).
        for (exe, expected) in [("CivilizationVI_DX12.exe", [0xa2d280usize, 0xa2d700]), ("CivilizationVI.exe", [0xa1f3f0, 0xa1fc80])] {
            let Some(bytes) = read(format!(r"{GAME}\Base\Binaries\Win64Steam\{exe}")) else { continue };
            // Adresse virtuelle relative de chaque occurrence, par les en-têtes de sections.
            let base = bytes.as_ptr() as usize;
            let (nt, hits) = unsafe { (base + rd::<u32>(base + 0x3c) as usize, find_sig(&bytes, false)) };
            assert_eq!(hits.len(), 1, "{exe}");
            let rva = unsafe {
                let count = rd::<u16>(nt + 6) as usize;
                let first = nt + 24 + rd::<u16>(nt + 20) as usize;
                (0..count)
                    .map(|i| first + 40 * i)
                    .find_map(|s| {
                        let (va, raw, rawsize) = (rd::<u32>(s + 12) as usize, rd::<u32>(s + 20) as usize, rd::<u32>(s + 16) as usize);
                        (raw..raw + rawsize).contains(&hits[0]).then(|| hits[0] - raw + va)
                    })
                    .unwrap()
            };
            assert!(expected.contains(&rva), "{exe} : {rva:#x}");
        }
    }
}
