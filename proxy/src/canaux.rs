//! Canaux sur colline : le jeu refuse en dur une colline au canal (quartier) et au canal de Panama,
//! même quand la base de règles l'autorise. Correctif POL : il change la simulation (placement, IA),
//! il n'est appliqué que si tous les joueurs ont le proxy (voir `pol`). Pas de réglage.
//!
//! Deux tests de GameCore appellent `180060220` (la colline d'une case, celle de `Plot:IsHills()` en
//! Lua : `18001e0b0`) puis `test al,al ; jne <refus>` :
//! - quartier canal, en `180362eb6` (`mov rcx,rsi` : la case), juste après le refus des montagnes ;
//! - canal de Panama, en `1803639c2` (`mov rcx,r12`).
//!
//! Chaque `call` est redirigé vers un relais `jmp [rip+0]` qui mène à une fonction d'ici : elle rend
//! « pas une colline » quand le correctif s'applique, sinon la réponse du jeu.
//! - **Quartier** : dès que les correctifs POL sont actifs. Le terrain est alors jugé par la base de
//!   règles seule (`District_ValidTerrains`) : le jeu de base n'y met que les cinq terrains plats,
//!   donc rien ne change sans un mod qui y ajoute les collines (Better Balanced Game, option
//!   « Canals on Hills », `sql/XP2/CanalsOnHills.sql`).
//! - **Canal de Panama** : il n'a pas de table de terrains ; il faut en plus l'option de partie
//!   `BBG_CANALS_ON_HILLS` de Better Balanced Game, relue par `BGD_CanauxCollines` (bloc Lua ajouté à
//!   `WorldViewPlotMessages.lua`, contexte toujours chargé en partie).
//!
//! Sites trouvés par Hotaru Snowy (« Allow Canals on Hills - dll version », Workshop 3663157965,
//! github.com/Hotaru-Snowy/CanalOnHills) ; aucun code repris. Analyse : analyses/canaux/README.md.

use crate::debordement::{hks, LuaCFn, LUA_GLOBALSINDEX};
use crate::patch::{alloc_near, rel32, write_rel32, IMAGE_BASE};
use crate::{log, pol};
use std::ffi::{c_char, c_void};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

/// `(case) -> la case est une colline`.
const EST_COLLINE: u64 = 0x1_8006_0220;

struct Appel {
    site: u64,
    /// Le `mov rcx, <case>` qui précède l'appel.
    avant: [u8; 3],
    nom: &'static str,
}

const QUARTIER: usize = 0;
const PANAMA: usize = 1;
const APPELS: [Appel; 2] = [
    Appel { site: 0x1_8036_2eb6, avant: [0x48, 0x8b, 0xce], nom: "quartier canal" },
    Appel { site: 0x1_8036_39c2, avant: [0x49, 0x8b, 0xcc], nom: "canal de Panama" },
];
/// `test al,al ; jne` après l'appel.
const APRES: [u8; 4] = [0x84, 0xc0, 0x0f, 0x85];

static BASE: AtomicUsize = AtomicUsize::new(0);
static PATCHED_BASE: AtomicUsize = AtomicUsize::new(0);
/// Option de partie `BBG_CANALS_ON_HILLS`, posée par le Lua au chargement de chaque partie.
static OPTION_PANAMA: AtomicBool = AtomicBool::new(false);
static ACCEPTEES: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
const MAX_LOGS: u32 = 10;

fn gc(addr: u64) -> usize {
    BASE.load(Ordering::Relaxed) + (addr - IMAGE_BASE) as usize
}

unsafe fn rd<T: Copy>(addr: usize) -> T {
    std::ptr::read_unaligned(addr as *const T)
}

/// La colline compte-t-elle encore pour ce test ? `panama` : le test du canal de Panama, qui
/// demande aussi l'option de partie.
fn colline_comptee(colline: bool, pol: bool, panama: bool, option: bool) -> bool {
    colline && !(pol && (!panama || option))
}

unsafe fn tester(i: usize, case: usize) -> bool {
    let est_colline: unsafe extern "system" fn(usize) -> bool = std::mem::transmute(gc(EST_COLLINE));
    let colline = est_colline(case);
    if !colline {
        return false;
    }
    let garde = colline_comptee(true, pol::actifs(), i == PANAMA, OPTION_PANAMA.load(Ordering::Relaxed));
    if !garde {
        let k = ACCEPTEES[i].fetch_add(1, Ordering::Relaxed) + 1;
        if k <= MAX_LOGS {
            log(&format!("canaux : colline acceptée pour le {} ({k})", APPELS[i].nom));
        }
    }
    garde
}

unsafe extern "system" fn colline_quartier(case: usize) -> bool {
    tester(QUARTIER, case)
}

unsafe extern "system" fn colline_panama(case: usize) -> bool {
    tester(PANAMA, case)
}

unsafe fn site_attendu(a: &Appel, at: usize) -> bool {
    std::slice::from_raw_parts((at - a.avant.len()) as *const u8, a.avant.len()) == a.avant
        && rd::<u8>(at) == 0xe8
        && rel32(at, gc(EST_COLLINE)) == Some(rd::<i32>(at + 1))
        && std::slice::from_raw_parts((at + 5) as *const u8, APRES.len()) == APRES
}

/// Les appels sont redirigés : `BGD_CanauxCollines` a un effet.
pub fn pose() -> bool {
    PATCHED_BASE.load(Ordering::Relaxed) != 0
}

/// Appelé quand l'originale vient d'être chargée.
pub unsafe fn on_gamecore_loaded(base: usize) {
    BASE.store(base, Ordering::Relaxed);
    // Tout ou rien : un seul des deux sites trouvé veut dire une autre version du jeu.
    if let Some(a) = APPELS.iter().find(|a| !site_attendu(a, gc(a.site))) {
        log(&format!("canaux : version inattendue en {:#x}, canaux sur colline non posés", a.site));
        return;
    }
    let Some(page) = alloc_near(base) else {
        log("canaux : pas de mémoire à moins de 2 Go de GameCore, canaux sur colline non posés");
        return;
    };
    let cibles = [colline_quartier as *const () as usize, colline_panama as *const () as usize];
    let mut rels = [0i32; 2];
    for (i, (a, cible)) in APPELS.iter().zip(cibles).enumerate() {
        // Relais : jmp [rip+0] puis l'adresse de la fonction. La case reste dans rcx.
        let relais = page + i * 0x10;
        std::ptr::copy_nonoverlapping([0xffu8, 0x25, 0, 0, 0, 0].as_ptr(), relais as *mut u8, 6);
        std::ptr::write_unaligned((relais + 6) as *mut u64, cible as u64);
        let Some(rel) = rel32(gc(a.site), relais) else {
            log("canaux : relais hors de portée, canaux sur colline non posés");
            return;
        };
        rels[i] = rel;
    }
    for (a, rel) in APPELS.iter().zip(rels) {
        write_rel32(gc(a.site), rel);
    }
    PATCHED_BASE.store(base, Ordering::Relaxed);
    log("canaux : test de colline redirigé (quartier canal 0x180362eb6, canal de Panama 0x1803639c2)");
}

/// Le proxy est déchargé alors que l'originale reste en mémoire : on remet les appels d'origine.
pub unsafe fn on_proxy_unload() {
    let base = PATCHED_BASE.swap(0, Ordering::Relaxed);
    if base == 0 {
        return;
    }
    for a in &APPELS {
        let at = base + (a.site - IMAGE_BASE) as usize;
        if let Some(rel) = rel32(at, base + (EST_COLLINE - IMAGE_BASE) as usize) {
            write_rel32(at, rel);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Lua : l'option de partie, relue à chaque partie.

/// Ajouté à la fin de `WorldViewPlotMessages.lua`.
pub const BLOC_LUA: &str = "\
-- BetterGameExperience (canaux.rs) : option de partie BBG_CANALS_ON_HILLS de Better Balanced Game.
-- Le canal de Panama ne passe sur une colline que si l'hôte l'a cochée (et si tous ont le proxy).
if BGD_CanauxCollines ~= nil then
\tlocal v = GameConfiguration.GetValue(\"BBG_CANALS_ON_HILLS\");
\tBGD_CanauxCollines((v == true or v == 1) and 1 or 0);
end
";

const NOM: &[u8] = b"BGD_CanauxCollines\0";

unsafe extern "C-unwind" fn lua_canaux(l: *mut c_void) -> i32 {
    let Some(h) = hks() else { return 0 };
    let on = (h.to_integer)(l, 1) != 0;
    if OPTION_PANAMA.swap(on, Ordering::Relaxed) != on || !LUE.swap(true, Ordering::Relaxed) {
        log(&format!("canaux : option BBG_CANALS_ON_HILLS {}", if on { "cochée" } else { "décochée" }));
    }
    0
}

static LUE: AtomicBool = AtomicBool::new(false);

unsafe extern "C-unwind" fn installer(l: *mut c_void) -> i32 {
    if let Some(h) = hks() {
        let f: LuaCFn = lua_canaux;
        (h.push_closure)(l, f, 0, NOM.as_ptr() as *const c_char, 0);
        (h.set_field)(l, LUA_GLOBALSINDEX, NOM.as_ptr() as *const c_char);
    }
    0
}

/// Pose `BGD_CanauxCollines` dans la table globale de l'état qui compile le script. Rend vrai si
/// elle y est : sans elle, le bloc Lua ne fait rien.
pub unsafe fn publier(l: *mut c_void) -> bool {
    let r = match hks() {
        Some(h) => {
            let r = (h.cpcall)(l, installer, std::ptr::null_mut());
            if r != 0 {
                (h.set_top)(l, -2);
            }
            r
        }
        None => -1,
    };
    if r != 0 {
        log(&format!("canaux : BGD_CanauxCollines non posée ({r}), canal de Panama sur terrain plat"));
    }
    r == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quartier_suit_pol_seul() {
        assert!(colline_comptee(true, false, false, true));
        assert!(!colline_comptee(true, true, false, false));
        assert!(!colline_comptee(true, true, false, true));
    }

    #[test]
    fn panama_demande_pol_et_option() {
        assert!(colline_comptee(true, false, true, true));
        assert!(colline_comptee(true, true, true, false));
        assert!(!colline_comptee(true, true, true, true));
    }

    #[test]
    fn case_plate_jamais_colline() {
        for (p, pa, o) in [(false, false, false), (true, true, true), (true, false, true)] {
            assert!(!colline_comptee(false, p, pa, o));
        }
    }

    /// Les octets vérifiés au chargement correspondent au GameCore installé, s'il est là.
    #[test]
    fn sites_du_jeu() {
        let path = r"C:\Program Files (x86)\Steam\steamapps\common\Sid Meier's Civilization VI\DLC\Expansion2\Binaries\Win64\GameCore_XP2_FinalRelease.dll";
        let Ok(b) = std::fs::read(path) else { return };
        let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
        let pe = u32_at(0x3c);
        let n = u16::from_le_bytes(b[pe + 6..pe + 8].try_into().unwrap()) as usize;
        let opt = u16::from_le_bytes(b[pe + 20..pe + 22].try_into().unwrap()) as usize;
        let fichier = |addr: u64| {
            let rva = (addr - IMAGE_BASE) as usize;
            (0..n).find_map(|i| {
                let s = pe + 24 + opt + i * 40;
                let (taille, va, brut) = (u32_at(s + 8), u32_at(s + 12), u32_at(s + 20));
                (va..va + taille).contains(&rva).then(|| brut + rva - va)
            })
        };
        for a in &APPELS {
            let o = fichier(a.site).unwrap();
            assert_eq!(&b[o - 3..o], &a.avant, "{}", a.nom);
            assert_eq!(b[o], 0xe8, "{}", a.nom);
            let rel = i32::from_le_bytes(b[o + 1..o + 5].try_into().unwrap());
            assert_eq!(a.site as i64 + 5 + rel as i64, EST_COLLINE as i64, "{}", a.nom);
            assert_eq!(&b[o + 5..o + 9], &APRES, "{}", a.nom);
        }
    }

    #[test]
    fn bloc_lua_lit_l_option() {
        assert!(BLOC_LUA.contains("GameConfiguration.GetValue(\"BBG_CANALS_ON_HILLS\")"));
        assert!(BLOC_LUA.contains("BGD_CanauxCollines"));
    }
}
