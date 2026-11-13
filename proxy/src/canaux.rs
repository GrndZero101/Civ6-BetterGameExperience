//! Canaux sur colline : le jeu refuse en dur une colline au quartier canal, même quand la base de
//! règles l'autorise. Correctif POL : il change la simulation (placement, IA), il n'est appliqué que
//! si tous les joueurs ont le proxy (voir `pol`). Pas de réglage.
//!
//! Le test de placement du quartier canal appelle `180060220` (la colline d'une case, celle de
//! `Plot:IsHills()` en Lua : `18001e0b0`) en `180362eb6` (`mov rcx,rsi` : la case), juste après le
//! refus des montagnes, puis `test al,al ; jne <refus>`.
//!
//! Le `call` est redirigé vers un relais `jmp [rip+0]` qui mène à `colline_quartier` : elle rend
//! « pas une colline » dès que les correctifs POL sont actifs, sinon la réponse du jeu. Le terrain est
//! alors jugé par la base de règles seule (`District_ValidTerrains`) : le jeu de base n'y met que les
//! cinq terrains plats, donc rien ne change sans un mod qui y ajoute les collines (par exemple
//! Better Balanced Game, option « Canals on Hills »).
//!
//! Site trouvé par Hotaru Snowy (« Allow Canals on Hills - dll version », Workshop 3663157965,
//! github.com/Hotaru-Snowy/CanalOnHills) ; aucun code repris. Analyse : analyses/canaux/README.md.

use crate::patch::{alloc_near, rel32, write_rel32, IMAGE_BASE};
use crate::{log, pol};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// `(case) -> la case est une colline`.
const EST_COLLINE: u64 = 0x1_8006_0220;

struct Appel {
    site: u64,
    /// Le `mov rcx, <case>` qui précède l'appel.
    avant: [u8; 3],
    nom: &'static str,
}

const QUARTIER: usize = 0;
const APPELS: [Appel; 1] = [Appel { site: 0x1_8036_2eb6, avant: [0x48, 0x8b, 0xce], nom: "quartier canal" }];
/// `test al,al ; jne` après l'appel.
const APRES: [u8; 4] = [0x84, 0xc0, 0x0f, 0x85];

static BASE: AtomicUsize = AtomicUsize::new(0);
static PATCHED_BASE: AtomicUsize = AtomicUsize::new(0);
static ACCEPTEES: [AtomicU32; 1] = [AtomicU32::new(0)];
const MAX_LOGS: u32 = 10;

fn gc(addr: u64) -> usize {
    BASE.load(Ordering::Relaxed) + (addr - IMAGE_BASE) as usize
}

unsafe fn rd<T: Copy>(addr: usize) -> T {
    std::ptr::read_unaligned(addr as *const T)
}

/// La colline compte-t-elle encore pour le test du quartier canal ?
fn colline_comptee(colline: bool, pol: bool) -> bool {
    colline && !pol
}

unsafe fn tester(i: usize, case: usize) -> bool {
    let est_colline: unsafe extern "system" fn(usize) -> bool = std::mem::transmute(gc(EST_COLLINE));
    let colline = est_colline(case);
    if !colline {
        return false;
    }
    let garde = colline_comptee(true, pol::actifs());
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

unsafe fn site_attendu(a: &Appel, at: usize) -> bool {
    std::slice::from_raw_parts((at - a.avant.len()) as *const u8, a.avant.len()) == a.avant
        && rd::<u8>(at) == 0xe8
        && rel32(at, gc(EST_COLLINE)) == Some(rd::<i32>(at + 1))
        && std::slice::from_raw_parts((at + 5) as *const u8, APRES.len()) == APRES
}

/// Appelé quand l'originale vient d'être chargée.
pub unsafe fn on_gamecore_loaded(base: usize) {
    BASE.store(base, Ordering::Relaxed);
    if let Some(a) = APPELS.iter().find(|a| !site_attendu(a, gc(a.site))) {
        log(&format!("canaux : version inattendue en {:#x}, canaux sur colline non posés", a.site));
        return;
    }
    let Some(page) = alloc_near(base) else {
        log("canaux : pas de mémoire à moins de 2 Go de GameCore, canaux sur colline non posés");
        return;
    };
    let cibles = [colline_quartier as *const () as usize];
    let mut rels = [0i32; 1];
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
    log("canaux : test de colline du quartier canal redirigé (0x180362eb6)");
}

/// Le proxy est déchargé alors que l'originale reste en mémoire : on remet l'appel d'origine.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quartier_suit_pol() {
        assert!(colline_comptee(true, false));
        assert!(!colline_comptee(true, true));
        assert!(!colline_comptee(false, false));
        assert!(!colline_comptee(false, true));
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
}
