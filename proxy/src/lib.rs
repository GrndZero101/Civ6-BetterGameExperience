//! Proxy de GameCore_XP2_FinalRelease.dll pour Civilization VI.
//!
//! Posé à côté de l'originale sous le nom `BGD_GameCore_XP2.dll`, sans la remplacer : le proxy de
//! démarrage (winmm.dll) le fait charger à sa place par l'exe. Il relaie ses 157 exports
//! (`forward`) et, au premier appel, la charge sous son vrai nom puis pose ses détours.
//!
//! Réglage : fichier `rollover_proxy.cfg` à côté de la DLL, lignes `actif=on|off` (interrupteur
//! général, on par défaut), `mode=coalesce|count|off`
//! (coalesce par défaut), `portee=cascade|message` (message par défaut),
//! `ameliorations=off|count|verify|fast|quiet` et `rendements=off|count|verify|fast|quiet` (quiet par
//! défaut), `wmd_accel=<facteur>|off` (20 par défaut, voir `wmd`), `ui_boutons`, `ui_recentrage` et
//! `ui_journal`, `ui_mesure_epingles`, `ui_epingles`, `ui_merveilles`, `ui_diag_accords` (voir `ui`), `ui_carte_entiere=on|off`
//! (on par défaut, voir `carte`), `ui_rendements=on|off`
//! (on par défaut, voir `rendements_empire`), `tours_production=on|off` (on par défaut, voir `tours`), `ui_debordement=on|off` (on, voir
//! `debordement`), `ui_banniere_annuler`, `ui_reduction_quartiers` et `ui_avancement` (on, voir `ui`),
//! `ui_charges`, `ui_routes`, `ui_notifications`, `ui_bandeau`, `ui_politiques`, `ui_rapports` (on
//! par défaut, voir `mods_ui`), `depart_signaux=on|off` (off, voir `depart`), `ui_mesure_mouvements=on|off` (off, voir `ui`). Les réglages de
//! diagnostic `ui_journal`, `ui_mesure_epingles` et `ui_diag_accords` ne sont lus qu'avec la
//! feature `diag` (builds locaux) : la release l'omet, ils y restent sur off. Journal :
//! `%LOCALAPPDATA%\Civ6RolloverProxy\RolloverProxy.log`, hors du dossier Logs du jeu.

#![allow(non_snake_case)] // le nom de la DLL produite, BGD_GameCore_XP2.

mod avancement;
mod canaux;
mod carte;
mod citoyens;
mod coalesce;
mod congres;
mod debordement;
mod depart;
mod empilement;
mod exe;
mod fin_tour;
mod forward;
mod improvements;
mod mods_ui;
mod noms;
mod patch;
mod pings;
mod pol;
mod ports;
mod rendements;
mod rendements_empire;
mod spectateurs;
mod tours;
mod ui;
#[allow(dead_code)] // RELEASE ne sert qu'au proxy de démarrage.
mod version;
mod win;
mod wmd;

use std::ffi::c_void;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

pub(crate) static SELF_MODULE: AtomicUsize = AtomicUsize::new(0);

/// Interrupteur général (`actif=on|off`, on par défaut) : off, le proxy charge l'originale et relaie
/// ses exports sans poser aucun détour, sans qu'il faille le désinstaller.
pub(crate) static ENABLED: AtomicBool = AtomicBool::new(true);

/// Étapes franchies, pour les tests : voir `RolloverProxyStatus`.
pub(crate) static STATUS: AtomicU32 = AtomicU32::new(0);
pub(crate) const ST_DLLMAIN: u32 = 1;
pub(crate) const ST_ORIGINAL: u32 = 2;
pub(crate) const ST_HOOKED: u32 = 8;
/// Correctifs POL posés (`citoyens`, `spectateurs`) : le proxy de démarrage ne s'annonce dans le salon que si ce
/// bit est là, une fois ce proxy chargé.
pub(crate) const ST_POL: u32 = 16;

/// Export de diagnostic : DllMain appelée (1), originale chargée (2), détours posés (8), correctifs
/// POL posés (16).
#[no_mangle]
pub extern "system" fn RolloverProxyStatus() -> u32 {
    STATUS.load(Ordering::Relaxed)
}

/// `BgdVersion=<VERSION>` terminée par un NUL : la marque que le proxy de démarrage cherche dans ce
/// fichier tant qu'il n'est pas chargé (premier salon d'une session). `BgdVersion` en rend la fin.
const LONGUEUR_MARQUE: usize = version::MARQUE.len() + version::VERSION.len() + 1;
#[used]
static MARQUE_VERSION: [u8; LONGUEUR_MARQUE] = {
    let (m, v) = (version::MARQUE.as_bytes(), version::VERSION.as_bytes());
    let mut out = [0u8; LONGUEUR_MARQUE];
    let mut i = 0;
    while i < m.len() {
        out[i] = m[i];
        i += 1;
    }
    let mut j = 0;
    while j < v.len() {
        out[i + j] = v[j];
        j += 1;
    }
    out
};

/// Export lu par le proxy de démarrage avant de s'annoncer dans le salon : `version::VERSION`, en
/// chaîne terminée par un NUL. Les correctifs POL sont ceux de ce proxy-ci : une version différente
/// de celle de `winmm.dll` (installation partielle), ou un proxy plus ancien sans cet export, et la
/// machine ne s'annonce pas (startup_proxy/src/salon.rs).
#[no_mangle]
pub extern "system" fn BgdVersion() -> *const u8 {
    MARQUE_VERSION[version::MARQUE.len()..].as_ptr()
}

/// Export de test : charge l'originale et pose les détours sans appeler aucune fonction du jeu.
#[no_mangle]
pub extern "system" fn RolloverProxyLoadOriginal() -> usize {
    forward::real_module()
}

/// Chemin du journal, calculé une fois : le dossier n'est pas recréé à chaque ligne.
fn log_path() -> Option<&'static std::path::Path> {
    static CHEMIN: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    CHEMIN
        .get_or_init(|| {
            let dir = std::path::Path::new(&std::env::var_os("LOCALAPPDATA")?).join("Civ6RolloverProxy");
            std::fs::create_dir_all(&dir).ok()?;
            Some(dir.join("RolloverProxy.log"))
        })
        .as_deref()
}

pub(crate) fn log(msg: &str) {
    let Some(path) = log_path() else { return };
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[{secs}] {msg}");
    }
}

pub(crate) fn self_dir() -> Option<std::path::PathBuf> {
    self_path()?.parent().map(|p| p.to_path_buf())
}

/// Chemin complet du proxy.
pub(crate) fn self_path() -> Option<std::path::PathBuf> {
    let mut buf = [0u16; 1024];
    let n = unsafe {
        win::GetModuleFileNameW(SELF_MODULE.load(Ordering::Relaxed) as win::HMODULE, buf.as_mut_ptr(), buf.len() as u32)
    } as usize;
    (n > 0).then(|| std::path::PathBuf::from(String::from_utf16_lossy(&buf[..n])))
}

pub(crate) fn read_config() {
    let Some(text) = self_dir().and_then(|d| std::fs::read_to_string(d.join("rollover_proxy.cfg")).ok()) else {
        return;
    };
    for line in text.lines() {
        if let Some(v) = line.trim().strip_prefix("actif=") {
            ENABLED.store(v.trim() != "off", Ordering::Relaxed);
        }
        if let Some(v) = line.trim().strip_prefix("mode=") {
            coalesce::set_mode(match v.trim() {
                "off" => coalesce::Mode::Off,
                "count" => coalesce::Mode::Count,
                _ => coalesce::Mode::Coalesce,
            });
        }
        if let Some(v) = line.trim().strip_prefix("portee=") {
            coalesce::set_scope(match v.trim() {
                "cascade" => coalesce::Scope::Cascade,
                _ => coalesce::Scope::Message,
            });
        }
        if let Some(v) = line.trim().strip_prefix("ameliorations=") {
            improvements::set_mode(match v.trim() {
                "count" => improvements::Mode::Count,
                "verify" => improvements::Mode::Verify,
                "fast" => improvements::Mode::Fast,
                "off" => improvements::Mode::Off,
                _ => improvements::Mode::Quiet,
            });
        }
        if let Some(v) = line.trim().strip_prefix("rendements=") {
            rendements::set_mode(match v.trim() {
                "count" => rendements::Mode::Count,
                "verify" => rendements::Mode::Verify,
                "fast" => rendements::Mode::Fast,
                "off" => rendements::Mode::Off,
                _ => rendements::Mode::Quiet,
            });
        }
        if let Some(v) = line.trim().strip_prefix("wmd_accel=") {
            // `off`, 1 ou une valeur illisible : pas d'accélération.
            wmd::set_factor(v.trim().parse::<f32>().ok().filter(|f| f.is_finite() && *f >= 1.0).unwrap_or(1.0));
        }
        if let Some(v) = line.trim().strip_prefix("tours_production=") {
            tours::set_enabled(v.trim() != "off");
        }
        if let Some(v) = line.trim().strip_prefix("ui_boutons=") {
            ui::set_boutons(v.trim() != "off");
        }
        if let Some(v) = line.trim().strip_prefix("ui_recentrage=") {
            ui::set_recentrage(v.trim() == "on");
        }
        #[cfg(feature = "diag")]
        if let Some(v) = line.trim().strip_prefix("ui_journal=") {
            ui::set_journal(v.trim() == "on");
        }
        #[cfg(feature = "diag")]
        if let Some(v) = line.trim().strip_prefix("ui_mesure_epingles=") {
            ui::set_mesure_epingles(v.trim() == "on");
        }
        if let Some(v) = line.trim().strip_prefix("ui_mesure_mouvements=") {
            ui::set_mesure_mouvements(v.trim() == "on");
        }
        if let Some(v) = line.trim().strip_prefix("ui_epingles=") {
            ui::set_epingles(v.trim() != "off");
        }
        if let Some(v) = line.trim().strip_prefix("ui_carte_entiere=") {
            carte::set_enabled(v.trim() != "off");
        }
        if let Some(v) = line.trim().strip_prefix("ui_banniere_annuler=") {
            ui::set_banniere_annuler(v.trim() != "off");
        }
        if let Some(v) = line.trim().strip_prefix("ui_reduction_quartiers=") {
            ui::set_reduction_quartiers(v.trim() != "off");
        }
        if let Some(v) = line.trim().strip_prefix("ui_avancement=") {
            ui::set_avancement(v.trim() != "off");
        }
        if let Some(v) = line.trim().strip_prefix("ui_debordement=") {
            debordement::set_enabled(v.trim() != "off");
        }
        if let Some(v) = line.trim().strip_prefix("ui_merveilles=") {
            ui::set_merveilles(v.trim() != "off");
        }
        #[cfg(feature = "diag")]
        if let Some(v) = line.trim().strip_prefix("ui_diag_accords=") {
            ui::set_diag_accords(v.trim() == "on");
        }
        if let Some(v) = line.trim().strip_prefix("ui_rendements=") {
            rendements_empire::set_enabled(v.trim() != "off");
        }
        if let Some(v) = line.trim().strip_prefix("ui_charges=") {
            mods_ui::set_charges(v.trim() != "off");
        }
        if let Some(v) = line.trim().strip_prefix("ui_routes=") {
            mods_ui::set_routes(v.trim() != "off");
        }
        if let Some(v) = line.trim().strip_prefix("ui_notifications=") {
            mods_ui::set_notifications(v.trim() != "off");
        }
        if let Some(v) = line.trim().strip_prefix("ui_bandeau=") {
            mods_ui::set_bandeau(v.trim() != "off");
        }
        if let Some(v) = line.trim().strip_prefix("ui_politiques=") {
            mods_ui::set_politiques(v.trim() != "off");
        }
        if let Some(v) = line.trim().strip_prefix("ui_rapports=") {
            mods_ui::set_rapports(v.trim() != "off");
        }
        if let Some(v) = line.trim().strip_prefix("depart_signaux=") {
            depart::set_signaux(v.trim() == "on");
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn DllMain(module: win::HMODULE, reason: u32, reserved: *mut c_void) -> i32 {
    match reason {
        win::DLL_PROCESS_ATTACH => {
            SELF_MODULE.store(module as usize, Ordering::Relaxed);
            STATUS.fetch_or(ST_DLLMAIN, Ordering::Relaxed);
            win::DisableThreadLibraryCalls(module);
        }
        // Déchargement par FreeLibrary (reserved nul), pas fin du processus : l'originale reste
        // chargée, on y remet les octets d'origine pour qu'aucun saut ne vise ce proxy déchargé.
        win::DLL_PROCESS_DETACH if reserved.is_null() => {
            coalesce::on_proxy_unload();
            citoyens::on_proxy_unload();
            congres::on_proxy_unload();
            spectateurs::on_proxy_unload();
            ports::on_proxy_unload();
            fin_tour::on_proxy_unload();
            empilement::on_proxy_unload();
            canaux::on_proxy_unload();
            depart::on_proxy_unload();
            carte::on_proxy_unload();
            improvements::on_proxy_unload();
            rendements::on_proxy_unload();
            rendements_empire::on_proxy_unload();
            tours::on_proxy_unload();
            debordement::on_proxy_unload();
            avancement::on_proxy_unload();
            wmd::on_proxy_unload();
            ui::on_proxy_unload();
        }
        _ => {}
    }
    1
}

#[cfg(test)]
mod tests {
    /// Ce que lit le proxy de démarrage (startup_proxy/src/salon.rs) : l'export, et la marque du fichier.
    #[test]
    fn version_exportee() {
        let v = unsafe { std::ffi::CStr::from_ptr(super::BgdVersion() as *const std::ffi::c_char) };
        assert_eq!(v.to_str().unwrap(), super::version::VERSION);
        let marque = format!("{}{}\0", super::version::MARQUE, super::version::VERSION);
        assert_eq!(&super::MARQUE_VERSION[..], marque.as_bytes());
    }
}
