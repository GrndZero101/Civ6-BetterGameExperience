# Sommaire technique

Index du dépôt pour les développeurs : les proxys, leurs détours et les analyses qui les ont motivés.
Pour les joueurs : [README.md](README.md) (anglais) ou [README.fr.md](README.fr.md) (français).
Changements : [CHANGELOG.md](CHANGELOG.md) et ses traductions dans [changelog/](changelog/fr.md).

Modding de Civilization VI par DLL : un proxy de `GameCore_XP2_FinalRelease.dll` qui pose des
détours dans GameCore et dans l'exe, et les analyses qui l'ont motivé. Séparé de Civ6SimSolver
(le solveur hors partie) le 2026-10-30, avec l'historique des fichiers concernés.

Aucun fichier du jeu n'est remplacé, et Steam ne retire donc pas les proxys :
- le proxy de démarrage est posé à côté de l'exe sous le nom `winmm.dll`, une DLL de Windows que
  l'exe importe ;
- il fait charger le proxy de GameCore, `BGD_GameCore_XP2.dll`, à la place de l'original.

- [proxy](proxy/README.md) : la DLL proxy (Rust), son installation et ses réglages
  (`rollover_proxy.cfg`) ;
- [startup_proxy](startup_proxy/README.md) : le proxy de démarrage (`winmm.dll`) : chargement du
  proxy de GameCore, onglet d'options, salon multijoueur, notification de mise à jour, bouton
  « Signaler un bug » du menu Échap, garde des messages réseau contre le gel à la déconnexion
  d'un joueur ; [rapport_worker](rapport_worker/README.md) : le relais
  Cloudflare qui reçoit les rapports et les publie dans le forum Discord ;
- [updater](updater/README.md) : l'installeur `BetterGameExperience_V<version>.exe`, et [release](release/README.md) :
  le manifeste signé des releases ;
- [rollover](rollover/README.md) : mesure du passage de tour (capture WPR, lecteur ETL, analyse) et
  [cascade d'une déclaration de guerre](rollover/cascade_guerre.md) ;
- [nuke](nuke/README.md) : animation des WMD (trace Frida, mod artdef, accélération dans le proxy) ;
- [ui_strike](ui_strike/README.md) : boutons d'attaque des bannières, patchés depuis le proxy ;
- [production_turns](production_turns/README.md) : compteur de tours de production qui ignore les
  bonus plats de gouvernement (fascisme), corrigé dans le proxy ;
- [wonder_queue](wonder_queue/README.md) : merveille retirée de la file de production après deux
  échanges, rattrapée par l'interface du proxy sans désynchro ;
- production dans l'interface du proxy (voir [proxy/README.md](proxy/README.md)) : clic droit
  sur la production en cours (onglet Production) et sur l'icône du bandeau de ville pour la
  retirer, badge des quartiers en réduction, production en réserve (débordement) sur le bandeau ;
- [empire_yields](empire_yields/README.md) : rendements d'empire du bandeau figés après un gouverneur ou
  un personnage illustre, rafraîchis par le proxy sans désynchro ;
- [map_pins](map_pins/README.md) : lag des épingles de carte en multijoueur, reconstructions inutiles
  retirées par l'interface du proxy ;
- [minimap](minimap/README.md) : minimap et caméra bornées aux cases révélées (d'où les épingles aux
  bords de la carte), étendues à la carte entière dès le début par le proxy ;
- [citoyens](citoyens/README.md) : choix des cases travaillées qui préfère une 2/1 à une 2/2,
  corrigé dans le proxy (premier correctif POL, actif si tous les joueurs ont le proxy) ;
- [mods_ui](mods_ui/README.md) : mods d'interface du Workshop (Better Report Screen, Extended Policy
  Cards, TopPanel Extension Pro, NotificationClear, Better Builder Charges Tracking) repris dans le
  proxy, corrigés (RMA) et prioritaires sur le mod ; tours restants des routes commerciales
  et or de Poundmaker dans l'écran de choix des routes ;
- [pantheon](analyses/pantheon/README.md) : joueur désigné pour le panthéon, égalités et restriction d'un tour
  (analyse statique) ;
- [congres](congres/README.md) : congrès mondial en multijoueur, triplets divergents, tours passés et
  votes ignorés ; retour au vote après la fin de tour (correctif POL, pas encore essayé en
  partie) ;
- [espions](analyses/espions/README.md) : chances de succès annoncées sans le contre-espion du défenseur, et
  succès avec fuite comptés comme des succès (analyse statique) ;
- [lobby](analyses/lobby/README.md) : création du salon multijoueur, format de la config, presets MPH
  réappliqués par chaque machine, et swaps d'emplacement (réseau compris, courses pendant une arrivée) ;
  hook Frida et décodeur de config (analyse statique) ; créer et rejoindre un salon depuis un site
  de draft par des liens `civ6bge://` ([hook_externe](analyses/lobby/hook_externe.md), conception),
  et la sonde Steam qui doit en fournir le lien du salon ([sonde_steam](analyses/lobby/sonde_steam.md),
  conception) ;
- [personnages_illustres](analyses/personnages_illustres/README.md) : ville d'arrivée d'un personnage
  illustre (ville la plus peuplée avec le district de la classe, port pour l'amiral) (analyse statique) ;
- [teleportation](analyses/teleportation/README.md) : téléportation vers une ville au départ d'un district
  pour une unité navale (correctif POL) ;
- [canaux](analyses/canaux/README.md) : quartier canal refusé en dur sur une colline même quand la base
  de règles l'autorise, test rendu à `District_ValidTerrains`, et canal de Panama sur colline avec
  l'option de partie `BBG_CANALS_ON_HILLS` de Better Balanced Game (correctif POL) ;
- [littoral](analyses/littoral/README.md) : ville côtière qui ne peut plus produire ni acheter de
  navire jusqu'au rechargement, et correctif POL proposé (analyse statique) ;
- [garde_messages](analyses/garde_messages/README.md) : revue de la garde des messages réseau contre
  le gel à la déconnexion d'un joueur (branche `yodha-desync-issue`, corrigée et mergée), points
  Ghidra vérifiés (lecture des drapeaux juste), reste à essayer en partie ;
- défauts d'interface corrigés dans le proxy (émissaires, Entrée et zone de déplacement confirmés en
  partie ; raccourcis et colon encore à confirmer) :
  [raccourcis](analyses/raccourcis/README.md) coupés en cours de partie,
  [Échap sur l'écran des émissaires](analyses/emissaires_echap/README.md),
  [émissaires en attente](analyses/emissaires_reinit/README.md) effacés par un rafraîchissement,
  [lentille du colon](analyses/filtre_colon/README.md) vidée par une ville fondée,
  [Entrée](analyses/raccourci_entree/README.md) active une fois retirée des touches,
  [zone de déplacement](analyses/quartier_capture_mouvements/README.md) sur un quartier ennemi ;
- [unites_superposees](analyses/unites_superposees/README.md) : deux unités sur la même case après
  un déplacement (analyse statique, détour POL proposé, pas encore corrigé) ;
- [empilement_combat](empilement_combat/README.md) : unités empilées en excédent retirées du soutien
  et du contournement (correctif POL, essayé en partie avec des unités terrestres) ;
- [spectateurs](spectateurs/README.md) : spectateurs de BSM hors du tourisme et du vote du congrès,
  qu'ils peuvent toujours consulter (correctif POL) ;
- [depart_tour](depart_tour/README.md) : départ de tour simultané pour tous (anti first/last move),
  option de partie « No First/Last moves (BGE) » (départ simultané, fin de tour donnée par
  l'hôte, essayés en partie), synchro de tour de MPH coupée ;
- [textes](textes/README.md) : textes en jeu et de l'installeur, traduits dans les 11 langues de BBG
  (un fichier XML par langue, au format de Civ6), et messages du chat affichés dans la langue de
  chacun ;
- [netcode](netcode/README.md) : reconnexions et boucles de désynchro (hooks Frida, empreintes), analyse dans
  [docs/netcode](docs/netcode/README.md) ;
- [macos](analyses/macos/README.md) : portage macOS, binaires du jeu inspectés (Mach-O universels,
  hardened runtime, GameCore chargé par `dlopen`) et essai de patch du code sur les machines macOS
  de la CI (passe, mais SIP y est désactivé : à refaire sur un Mac ordinaire) ;
- [docs/gamecore_xp2](docs/gamecore_xp2/README.md) : carte des fichiers source de GameCore_XP2.

## Installation

Prérequis : Rust (`stable-x86_64-pc-windows-gnu` suffit ; l'installeur de la release est compilé
par `stable-x86_64-pc-windows-msvc`, voir [updater/README.md](updater/README.md)), Python 3.10+.

```bash
python -m venv .venv
.venv/Scripts/python.exe -m pip install frida capstone numpy
```

Les commandes des README se lancent depuis la racine du dépôt.

## Tests continus

[ci.yml](.github/workflows/ci.yml), à chaque push et pull request, sans secret : `cargo test` des
trois crates sur Windows (proxy et startup_proxy avec et sans la feature `diag`), les tests Lua
(`lua5.4 proxy/tests/test_*.lua`, `startup_proxy/tests/test_*.lua`, `textes/test_textes.lua`,
depuis la racine), ceux du [relais des rapports](rapport_worker/README.md#tests) et
`textes/gen_textes.py --check --strict`. Les tests Lua qui lisent des fichiers du jeu les cherchent
dans le dossier Steam par défaut (ou le chemin passé en argument) et sont sautés sans eux
(« test sauté ») : la CI ne les exécute pas, ils restent à lancer sur une machine qui a le jeu.
