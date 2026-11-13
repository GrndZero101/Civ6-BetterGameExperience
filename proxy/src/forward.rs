//! Relais des 157 exports de GameCore_XP2_FinalRelease.dll vers l'originale, posée sous son vrai
//! nom à côté de ce proxy (`BGD_GameCore_XP2.dll`). Le proxy de démarrage fait charger ce proxy à
//! la place de l'originale (son module `chargeur`) ; notre propre chargement de l'originale passe
//! par nos imports, pas par ce détour.
//!
//! Toutes les fonctions sont relayées par un relais générique à 8 `usize` : en convention x64, les
//! arguments en trop sont lus dans la zone de pile de l'appelant et ignorés par la vraie fonction.
//! Ce relais ne préserve pas les arguments et retours flottants (XMM) : seuls les 3 exports que l'exe
//! utilise (DllCreateGameContext, DllDestroyGameContext, EXP_GetTelemetrySessionHash) comptent, et
//! ils n'en ont pas. Les 154 autres (API Lua de Havok) ne sont importés par aucun binaire du jeu.

use crate::{coalesce, log, noms, win};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

static REAL: OnceLock<usize> = OnceLock::new();

/// L'originale, à côté du proxy. Un proxy posé sous le nom de l'original (installation de la
/// v0.3.0 ou d'avant) se chargerait lui-même : il ne charge rien.
fn original_path() -> Result<std::path::PathBuf, String> {
    let module = crate::self_path().ok_or("chemin du proxy introuvable")?;
    let name = module.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if name.eq_ignore_ascii_case(noms::GAMECORE) {
        return Err(format!("proxy posé sous le nom de l'original ({name}) : relance l'installeur"));
    }
    Ok(module.with_file_name(noms::GAMECORE))
}

/// Chargée au premier appel relayé (DllCreateGameContext, appelée par l'exe hors du verrou du
/// chargeur), puis détours posés sur l'originale avant qu'elle ne serve.
pub(crate) fn real_module() -> usize {
    *REAL.get_or_init(|| unsafe {
        let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
        log(&format!("proxy chargé dans {exe}"));
        crate::read_config();
        let path = match original_path() {
            Ok(p) => p,
            Err(e) => {
                log(&e);
                return 0;
            }
        };
        let h = win::LoadLibraryW(win::wide(&path.display().to_string()).as_ptr()) as usize;
        if h == 0 {
            log(&format!("chargement de {} impossible, erreur {}", path.display(), win::GetLastError()));
            return 0;
        }
        crate::STATUS.fetch_or(crate::ST_ORIGINAL, Ordering::Relaxed);
        // Interrupteur général : l'originale sert telle quelle, comme sans proxy.
        if !crate::ENABLED.load(Ordering::Relaxed) {
            log("proxy désactivé (actif=off) : aucun détour posé, GameCore d'origine");
            return h;
        }
        coalesce::on_gamecore_loaded(h);
        crate::improvements::on_gamecore_loaded(h);
        crate::rendements::on_gamecore_loaded(h);
        crate::rendements_empire::on_gamecore_loaded(h);
        crate::tours::on_gamecore_loaded(h);
        crate::debordement::on_gamecore_loaded(h);
        crate::avancement::on_gamecore_loaded(h);
        crate::wmd::on_gamecore_loaded(h);
        crate::mods_ui::on_gamecore_loaded(h);
        // Avant `ui` : l'accroche Lua n'est posée que si une fonctionnalité en a besoin.
        crate::spectateurs::on_gamecore_loaded(h);
        crate::congres::on_gamecore_loaded(h);
        crate::canaux::on_gamecore_loaded(h);
        crate::ui::on_gamecore_loaded();
        crate::citoyens::on_gamecore_loaded(h);
        crate::ports::on_gamecore_loaded(h);
        crate::fin_tour::on_gamecore_loaded(h);
        crate::empilement::on_gamecore_loaded(h);
        crate::depart::on_gamecore_loaded(h);
        crate::carte::on_gamecore_loaded();
        h
    })
}

fn resolve(cache: &AtomicUsize, name: &'static [u8]) -> usize {
    let p = cache.load(Ordering::Relaxed);
    if p != 0 {
        return p;
    }
    let module = real_module();
    let p = if module == 0 { 0 } else { unsafe { win::GetProcAddress(module as win::HMODULE, name.as_ptr()) as usize } };
    if p == 0 {
        log(&format!("export {} introuvable dans l'originale", String::from_utf8_lossy(&name[..name.len() - 1])));
    }
    cache.store(p, Ordering::Relaxed);
    p
}

type Fn8 = unsafe extern "system" fn(usize, usize, usize, usize, usize, usize, usize, usize) -> usize;

macro_rules! forward {
    ($($id:ident => $name:literal),* $(,)?) => {$(
        #[export_name = $name]
        pub unsafe extern "system" fn $id(
            a: usize, b: usize, c: usize, d: usize, e: usize, f: usize, g: usize, h: usize,
        ) -> usize {
            static CACHE: AtomicUsize = AtomicUsize::new(0);
            if $name == "DllCreateGameContext" {
                // Nouvelle partie : les mémoires de la précédente ne valent plus.
                crate::improvements::reset();
                crate::wmd::reset();
                crate::citoyens::reset();
                crate::spectateurs::reset();
                crate::congres::reset();
                crate::ports::reset();
                crate::fin_tour::reset();
                crate::empilement::reset();
                crate::depart::reset();
                crate::rendements_empire::reset();
            }
            let p = resolve(&CACHE, concat!($name, "\0").as_bytes());
            if p == 0 {
                // DllCreateGameContext rend alors un contexte nul : le jeu signale l'échec.
                return 0;
            }
            let real: Fn8 = std::mem::transmute(p);
            real(a, b, c, d, e, f, g, h)
        }
    )*};
}

include!("exports.rs");
