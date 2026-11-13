# Proxy de GameCore : regroupement des instantanés de villes

Prototype de la piste 1 de [cascade_guerre.md](../rollover/cascade_guerre.md). Pendant une cascade de
déclarations de guerre, le jeu refait l'instantané de toutes les villes des belligérants à chaque
déclaration : environ 2 900 instantanés pour 61 villes au tour 79, soit 2,2 s. Le proxy n'en fait plus
qu'un par couple (événement, joueur), à la fin de la cascade.

## Principe

- **À côté de l'original.** Le proxy est posé dans `DLC\Expansion2\Binaries\Win64` sous le nom
  `BGD_GameCore_XP2.dll`, à côté de `GameCore_XP2_FinalRelease.dll`, qui n'est pas touché. Le nom
  de la DLL vient de `Expansion2.modinfo`, l'exe ne le contient pas. Quand l'exe la charge, par
  `LoadLibraryW` ou `LoadLibraryA`, le proxy de démarrage (`winmm.dll`, voir
  [startup_proxy](../startup_proxy/README.md#le-chargement-du-proxy-de-gamecore)) lui donne ce
  proxy à la place. Aucun fichier du dépôt Steam n'est remplacé : une vérification des fichiers ou
  une mise à jour du jeu ne retire pas le proxy. Les noms sont dans [src/noms.rs](src/noms.rs),
  communs aux proxys et à l'installeur.
- **Jusqu'à la v0.3.0**, le proxy prenait le nom de l'original, renommé
  `GameCore_XP2_FinalRelease_orig.dll`. Steam remettait l'original à chaque vérification des
  fichiers ou mise à jour, y compris celle qu'il traite avant de télécharger un mod du Workshop en
  rejoignant un salon. L'installeur et `install.ps1` remettent ces anciennes installations en place.
- **Relais.** Le proxy relaie les 157 exports de l'original sous les mêmes noms, noms C++ décorés
  compris. La table `src/exports.rs` est générée par `gen_exports.py`. L'exe n'utilise que
  `DllCreateGameContext`, `DllDestroyGameContext` et `EXP_GetTelemetrySessionHash`, qu'il récupère
  par nom ; les 154 autres (API Lua de Havok) ne sont importés par aucun binaire du jeu. Le relais
  générique à 8 entiers ne transmet pas les arguments flottants, ce qui est sans effet pour ces 3
  fonctions.
- **Chargement de l'original.** Au premier appel relayé (`DllCreateGameContext`, hors du verrou du
  chargeur), le proxy charge l'original sous son vrai nom, à côté de lui (son propre chargement
  ne passe pas par le détour de l'exe), et vérifie octet par octet les deux sites. S'ils
  correspondent, il pose ses détours ; sinon, il ne patche rien et le note dans le journal.
- **Les deux détours :**
  - `1802a1f90` (DeclareWar) : un compteur de profondeur repère la cascade, rattachée au thread qui
    l'a ouverte. Le test `cmp edx,-1 / jz` du prologue est refait dans le détour, puisqu'un saut
    relatif ne se recopie pas.
  - `180072f80(événement, joueur)` : pendant la cascade, l'appel est noté au lieu d'être exécuté.
    À la sortie de l'appel le plus externe, chaque couple est rejoué une fois, dans l'ordre de
    première apparition, donc sur l'état final.
- **Déchargement.** Si le jeu décharge le proxy alors que l'original reste en mémoire, le proxy y
  remet les octets d'origine. Pour la même raison, l'état de cascade est un statique et non un
  `thread_local!`, dont les destructeurs survivraient au déchargement.
- **Pourquoi c'est sans risque pour la synchronisation.** L'instantané ne fait que lire l'état de jeu
  (vérifié dans la note), et sa réserve n'est lue par aucun code de règles. Différer ces instantanés
  ne change que le moment où l'interface reçoit ses mises à jour de villes.

### Pistes écartées

Les deux premiers prototypes se plaçaient à côté de l'exe, sous le nom d'une DLL système qu'il
importe :
- **`version.dll`** n'a jamais été chargé. L'exe démarre avec le moteur de compatibilité de Windows
  (`apphelp.dll`, `AcGenral.DLL`), qui importe `VERSION.dll` depuis System32 avant que l'exe ne
  résolve ses imports. Le module déjà chargé sous ce nom est réutilisé.
- **`dxgi.dll`** a bien été chargé, mais n'a pas réussi à charger la vraie `dxgi.dll` de System32.
  Le jeu a alors affiché « aucune carte graphique compatible » (`Renderer.log` :
  `Failed to create DXGI factory. Error: 0x80004001`).

Le proxy de GameCore évite toute interaction avec Windows et le graphisme. Et un échec reste
limité au lancement d'une partie Gathering Storm : le menu principal utilise `GameCore_Base`. Le
point d'entrée à côté de l'exe est désormais `winmm.dll`, que le moteur de compatibilité ne
précharge pas, et qui ne touche pas au graphisme.

## Modes

**Interrupteur général** : `actif=on|off` (`install.ps1 -Actif`, on par défaut). Avec `off`, le
proxy reste installé, charge l'original et relaie ses exports, mais ne pose aucun détour : le jeu se
comporte comme sans proxy (journal : `proxy désactivé (actif=off) : aucun détour posé`). Les autres
réglages restent dans le fichier, pour la réactivation. C'est aussi le premier réglage de l'onglet
BetterGameExperience, qui grise alors les autres.

Le mode se règle dans `rollover_proxy.cfg`, à côté de la DLL, par une ligne `mode=…` :

| mode | effet |
|---|---|
| `coalesce` (défaut) | regroupe les instantanés |
| `count` | pose les détours, ne change rien, compte et chronomètre les cascades |
| `off` | aucun détour |

Les autres réglages (`portee`, `ameliorations`, `rendements`, `wmd_accel`, `ui_*`,
`tours_production`) sont décrits plus bas, avec leur détour, et ceux des mods d'interface repris dans
[Mods d'interface repris](#mods-dinterface-repris). Ils se changent aussi en jeu, dans
l'onglet **BetterGameExperience** de l'écran d'options, qu'ajoute le proxy de démarrage
([startup_proxy](../startup_proxy/README.md)). **Tout nouveau réglage doit y figurer** : le test
`cles_communes_avec_le_proxy` (startup_proxy) le vérifie.

**Journal** : `%LOCALAPPDATA%\Civ6RolloverProxy\RolloverProxy.log`, hors du dossier `Logs` du jeu.
On y trouve :
- `proxy chargé dans <exe>`, au premier appel relayé ;
- une ligne au chargement de l'original, qui dit si les détours sont posés ;
- une ligne par cascade :

```
cascade mode=Count declarewar=358 profondeur_max=… instantanes_demandes=… uniques=… cascade_ms=… rejeu_ms=… joueurs=[…]
```

## Compiler, tester, installer

```bash
cd proxy && cargo build --release
```

**Régénérer la table des exports**, après une mise à jour du jeu :

```bash
.venv/Scripts/python.exe -I proxy/gen_exports.py "<jeu>\DLC\Expansion2\Binaries\Win64\GameCore_XP2_FinalRelease.dll" proxy/src/exports.rs
```

**Test hors jeu.** Il vérifie :
- les exports, identiques à ceux de l'original, plus les quatre du proxy (dont `BgdVersion`) ;
- le chargement de l'original et la lecture de la configuration ;
- les sauts écrits aux deux sites ;
- la remise des octets d'origine au déchargement.

L'original refuse de s'initialiser hors du jeu : le test le projette d'abord sans initialisation,
et le proxy récupère ce module tel quel.

```bash
.venv/Scripts/python.exe -I proxy/tests/check_hooks.py <proxy compilé> <GameCore_XP2_FinalRelease.dll d'origine> <dossier temporaire>
```

**Installation**, jeu fermé. Le script :
- remet en place une installation de la v0.3.0 ou d'avant (l'original sous `_orig`), et retire un
  `_orig` périmé à côté d'un original remis par Steam ;
- copie le proxy sous le nom `BGD_GameCore_XP2.dll`, et `rollover_proxy.cfg` ;
- retire les anciens prototypes posés à côté de l'exe.

Le proxy n'est chargé que si le proxy de démarrage est installé aussi
(`startup_proxy/install.ps1`). `-Uninstall` retire le proxy et ses réglages. Une vérification des
fichiers ou une mise à jour du jeu par Steam ne touche à rien.

```bash
powershell -ExecutionPolicy Bypass -File proxy/install.ps1 -Mode count
```

```bash
powershell -ExecutionPolicy Bypass -File proxy/install.ps1 -Uninstall
```

## Protocole de mesure en jeu

On part de `AutoSave_0078`, dont le passage au tour 79 déclenche la cascade, et on répète ce qui
suit pour chaque mode, en relançant le jeu entre deux modes :

1. Installer le proxy dans le mode voulu.
2. Charger `AutoSave_0078` et finir le tour sans rien jouer (en option, avec `capture.ps1` pour une
   trace WPR).
3. Au tour 79, finir encore le tour sans rien jouer. Copier de côté `AutoSave_0080.Civ6Save`, qui
   contient l'état après tout le tour 79.
4. Relever la ligne `cascade` du journal du proxy, et la durée du passage dans
   `net_message_debug.log` (`analyze.py`).

Les comparaisons à faire :

| comparaison | ce qu'elle montre |
|---|---|
| `count` contre `coalesce` | le gain : `cascade_ms`, et le nombre d'instantanés demandés pour 586 déclarations, qui dit si une déclaration rafraîchit un joueur ou deux |
| deux passages en `off` | si le jeu est reproductible : les deux `AutoSave_0080` sont-elles identiques à l'octet près ? |
| `off` contre `coalesce` | si les sauvegardes sont identiques. Ce test n'a de sens que si la comparaison précédente l'est déjà, sans quoi il faut comparer le contenu décodé, pas les octets |

## Résultats (27 octobre 2026, `AutoSave_0078` → tour 80)

**Gain.** Au passage au tour 79, la cascade compte 358 appels à DeclareWar et 1 172 instantanés
demandés, pour 17 couples (événement, joueur) différents, tous sur le même événement
`0x7dddaff1`. Une déclaration rafraîchit donc environ 3,3 joueurs, pas seulement les deux
belligérants.

| partie | mode | cascade | dont rejeu | passage de tour (à la seconde) |
|---|---|---|---|---|
| solo | count | 2 946 ms | – | 8 s |
| solo | count | 2 830 ms | – | 8 s |
| en ligne | count | 2 878 ms | – | – |
| solo | coalesce | 528 ms | 48 ms | 6 s |
| solo | coalesce | 513 ms | 48 ms | 6 s |
| en ligne | coalesce | 560 ms | 49 ms | 7 s, contrôle de synchronisation compris |

La cascade perd environ 2,4 s, soit 82 %. Un instantané coûte environ 2,1 ms pendant la cascade,
et 2,8 ms au rejeu, sur l'état final. Les petites cascades du tour 80 (1 à 4 déclarations) pèsent
quelques ms.

**État de jeu inchangé, en solo.** On a comparé les `AutoSave_0080` décompressées (29,6 Mo) de
deux parties count et de deux parties coalesce :
- **Elles se regroupent en deux familles sans rapport avec le mode**, distantes d'environ
  38 600 octets : {count 1, coalesce 2} et {count 2, coalesce 1}. Ce qui les sépare, c'est le
  hasard des IA au rechargement : un bloc de 55 Ko, et l'ordre `UNIT_SETTLER` / `UNIT_BUILDER`
  dans 6 listes.
- **Environ 2 600 octets d'écart restent dans une même famille.** Ce sont surtout des identifiants
  qui augmentent de 9 à chaque partie, quel que soit le mode.
- **Le test décisif :** aucun octet n'est identique dans les deux parties d'un même mode et
  différent entre les deux modes.

**Synchronisation en ligne.** On a joué à deux, l'hôte avec le proxy, l'invité sans. À chaque fin
de tour, l'invité envoie les sommes de contrôle de ≈ 9 350 archives d'état (`NetGameSyncCheck`).
L'hôte répond `NetSyncSnapshot : numDeltaArchives=0`, y compris à la fin du tour 79, qui couvre
l'état d'après la cascade. Aucune désynchronisation, et le salon a accepté la DLL modifiée.
**Le proxy n'a pas besoin d'être installé chez tout le monde.** Mais chaque client calcule le
passage de tour de son côté, donc seuls ceux qui ont le proxy gagnent du temps.

## Deuxième détour : `180260720`, amélioration constructible

[src/improvements.rs](src/improvements.rs). La fonction répond à « ce joueur peut-il construire
cette amélioration ? ». Pour retrouver les traits qui interdisent une amélioration, elle parcourt une
liste de la base de règles et compare chaque nom par `strcmp`. Le détour refait le même calcul en
appelant les mêmes fonctions de l'original, dans le même ordre, mais remplace ce parcours par une
table « amélioration → traits » remplie au premier appel. La table est refaite si la liste change
d'adresse ou de taille.

Réglage : une ligne `ameliorations=off|count|verify|fast|quiet` dans `rollover_proxy.cfg`
(`install.ps1 -Ameliorations …`). Le mode `verify` calcule les deux versions, rend celle de
l'original et compte les écarts. `quiet` fait comme `fast`, mais sans compteur, sans chronomètre ni
journal : c'est le mode pour jouer, et le défaut. Journal : une ligne `ameliorations …` par rafale d'appels, écrite au
premier appel qui suit une seconde de pause.

Mesures sur `AutoSave_0078` → tour 80, avec `mode=coalesce`. Les durées additionnent tous les
threads :

| mode | appels | durée cumulée | par appel | écarts | passages 79 / 80 |
|---|---:|---:|---:|---:|---|
| `verify` | 4,45 M + 4,44 M + 4,27 M | 13,2 s | 0,99 µs | **0** | 6 s / 6 s |
| `count` | 13 166 324 | 11 009 ms | 0,84 µs | – | 6 s / 6 s |
| `fast`, première version | 13 166 078 | 4 380 ms | 0,33 µs | – | 5 s / 4 s |
| `verify`, deuxième version | 13 252 977 | 11,6 s | 0,89 µs | **0** | 6 s / 6 s |
| `fast`, deuxième version | 13 166 078 | **1 609 ms** | 0,12 µs | – | **5 s / 4 s** |

- **Le gain :** 9,4 s de CPU économisées en trois passages de tour, soit environ 3,1 s par passage,
  tous threads confondus. Les passages de tour, mesurés à la seconde près, gagnent 1 à 2 s.
- **La deuxième version.** Une capture WPR en mode `quiet` montrait 417 ms dans le proxy lui-même :
  le `RwLock` de la table était en contention, et un compteur de références (`Arc`) changeait à chaque
  appel, depuis 13 threads. S'y ajoutaient 253 ms de `1802209a0`, la recherche dichotomique de la
  civilisation et du dirigeant, que l'original refait à chaque trait trouvé. La table est maintenant
  lue sans verrou : entrées jamais libérées, publiées par pointeur atomique. La civilisation et le
  dirigeant sont mémorisés par joueur, revérifiés à chaque appel par l'adresse de la configuration du
  joueur. Les mémoires sont abandonnées à chaque `DllCreateGameContext`.
- **Ce qui reste dans les 0,12 µs :** les prérequis de technologie et de dogme, les deux lectures du
  compteur de performance du mode `fast`, et l'entrée dans le détour.
- **Le volume d'appels :** environ 4,4 millions par passage de tour. Les appelants (rendements de
  quartier, évaluation de l'IA) posent sans cesse la même question. Ce serait la cible suivante,
  mais une mémorisation par (joueur, amélioration) devrait être invalidée chaque fois qu'une
  technologie ou un dogme change.

## Troisième série : instantané diplomatique, portée « message », rendements des cases

Trois chantiers menés ensemble, chacun avec son réglage et sa boucle count → verify → remplacement.

**1. Instantané diplomatique** (`1800b7630(événement, joueur)`, `Cache_Player_Diplomacy`).
- Pour un joueur, il relit une quinzaine d'accesseurs diplomatiques envers chaque autre joueur.
  Il appelle aussi `1801d02d0`, qui construit à chaque fois les infobulles traduites (jusqu'à
  9 `Localization::Lookup`).
- Coût : 355 des 480 ms qui restaient dans la cascade du tour 79 (capture quiet).
- Les 13 accesseurs décompilés ne font que lire l'état de jeu. La réserve `DAT_180b8bc10` n'est
  référencée que par `Cache_Player_Diplomacy` et par l'initialisation statique.
- Il est regroupé avec l'instantané des villes, dans la même liste, au même moment.

**2. Portée `message`** (`portee=cascade|message`, `-Portee`, `message` par défaut).
- En plus de DeclareWar, une passe de `18004afc0` (Game_Processor, appelée par `18063e2e0` pour
  chaque message `GameUpdate`) ouvre une portée.
- Les instantanés demandés hors cascade (gouverneurs, ressources, accords, fin de tour : ≈ 430 ms
  de villes) sont alors regroupés aussi.
- La portée se ferme avant `18007bbe0`, qui expédie les caches à l'interface.
- Une cascade imbriquée dans une passe est journalisée avec la passe entière.
- Les autres passes forment une ligne `passes …` par rafale (une seconde sans passe).

**3. Rendements des cases** (`18037ec60`, Rules_Yield, `rendements=off|count|verify|fast|quiet`,
`-Rendements`, `quiet` par défaut) : [src/rendements.rs](src/rendements.rs).
- **Ce que fait la fonction.** Elle lit la case, ses 6 voisines et des tables de règles, en entiers.
  C'est la feuille la plus chaude de l'évaluation des IA : ≈ 150 ms par passage, surtout dans les
  tâches parallèles de CityBuild et Homeland.
- **Quand on mémorise.** Seulement pendant `1804a16e0`, l'attente des tâches de l'IA. Le thread
  GameCore n'y fait qu'exécuter des tâches, et le moteur interdit d'y modifier l'état
  (`IsProcessingAi`).
  - Chaque attente ouvre une génération neuve.
  - Seule une vraie case de la carte sert de clé : la case doit être celle que la carte rend pour
    ses coordonnées.
- **Le remplacement de l'attente.** Le prologue de `1804a16e0` contient un `lea` relatif à RIP. Le
  détour la remplace donc entièrement : il refait l'appel à `EXP_JobManager_Wait`, la remise à zéro
  de `+0x38ac` et la notification au moteur.
- **Le journal.** Une ligne `rendements …` par rafale : appels, dont pendant l'attente, sur une vraie
  case (`cases_carte`), trouvés en mémoire, et écarts (en `verify`).

**Protocole.** Même départ qu'avant : `AutoSave_0078` → tour 80, sans rien jouer.

| étape | installation | à relever |
|---|---|---|
| mesure | `-Mode count -Portee message -Ameliorations quiet -Rendements verify` | lignes `cascade`, `passes` (demandés contre uniques) et `rendements` (`trouves`, `ecarts` = 0 ?), `AutoSave_0080` |
| remplacement | `-Mode coalesce -Portee message -Ameliorations quiet -Rendements fast` | durées de passage, `AutoSave_0080` comparée à celle de la mesure (`compare_saves.py`), puis contrôle de synchronisation en ligne |

## Quatrième détour : accélération de l'animation des WMD

[src/wmd.rs](src/wmd.rs), réglage `wmd_accel=<facteur>|off`, 20 par défaut (`-WmdAccel`). Il ne
touche pas au passage de tour. Contexte et mesures : [nuke/README.md](../nuke/README.md).

- **Le but.** La visualisation d'un nuke (`CombatVisBegin` → `CombatVisEnd`) dure 11 à 12 s et
  retient toutes les autres animations. Pendant cette seule visualisation, le proxy multiplie
  l'échelle de temps de l'affichage (App+0x559c, celle de la commande de console `timescale`) :
  ×20 la ramène à environ 0,6 s, la durée d'un combat d'unités.
- **Sans effet sur la partie.** GameCore et le minuteur de tour comptent en
  QueryPerformanceCounter et ne voient pas cette échelle. Rien ne part sur le réseau : chaque
  joueur qui a le proxy accélère son propre affichage.
- **La condition.** Une frappe a été calculée par le thread GameCore pendant la vidange de sa file
  (`180158af0` ou `180156ae0`, sous `1800372f0`), il y a moins de 3 s. La visualisation n'a qu'un
  membre, le tireur. Le joueur a activé le combat rapide (Options → Jeu, lu comme
  `UserConfiguration.IsQuickCombat` : `140119880(140118ca0())`). Sans combat rapide, le nuke garde
  sa vitesse d'origine.
- **Les détours de l'exe.** Ce sont les premiers posés hors de GameCore : les fonctions de
  diffusion de `CombatVisBegin` et `CombatVisEnd`. Leur prologue lit un pointeur relatif à RIP,
  que le trampoline relit en adressage absolu (`patch::install_at`).
- **DX12 et DX11.** Les deux exe sont compilés des mêmes sources, avec les mêmes fonctions à
  d'autres adresses. La table `EXES` donne un agencement par exe :

  | build 15296837 | DX12 | DX11 |
  |---|---|---|
  | CombatVisBegin / End | `1400f0400` / `1400f0460` | `1400eff70` / `1400effd0` |
  | configuration utilisateur, IsQuickCombat | `140118ca0`, `140119880` | `1401182e0`, `140118ec0` |
  | objet application, échelle de temps | `[1412fa2c8]` + `0x559c` | `[1412c2c88]` + `0x557c` |

  | build 26407122 (exe du 2026-11-02) | DX12 | DX11 |
  |---|---|---|
  | CombatVisBegin / End | `1400f0390` / `1400f03f0` | `1400ef640` / `1400ef6a0` |
  | configuration utilisateur, IsQuickCombat | `140118a50`, `140119630` | `140117b60`, `140118740` |
  | objet application, échelle de temps | `[1412fb2c8]` + `0x559c` | `[1412c3c88]` + `0x557c` |

  Les adresses DX11 du premier build viennent du projet Ghidra `civ6exe11`. Elles ont été
  retrouvées par les chaînes `SerialEvent::CombatVis*` (fonctions d'enregistrement), `IsQuickCombat`
  (sa closure Lua) et `timescale` (commande de console). Celles du second build, par le même chemin
  dans [tests/wmd_sites.py](tests/wmd_sites.py), qui retrouve en plus l'objet application par son
  constructeur (celui qui pose 1.0 à l'échelle de temps). Le module prend l'agencement dont tous
  les octets vérifiés correspondent. Avec un autre build, il ne pose rien et le note dans le
  journal. Le test `wmd::tests::agencement_des_exe_installes` échoue alors : il faut relancer le
  script et ajouter un agencement à `EXES`.
- **Les garde-fous.** L'échelle d'origine revient à `CombatVisEnd`, ou au bout de 5 s. Elle revient
  aussi à chaque nouvelle partie et au déchargement du proxy.
- **Le journal.** Une ligne `wmd : visualisation accélérée` par frappe, puis
  `wmd : échelle rendue à 1 (fin de visualisation, … ms)`.

## Cinquième accroche : boutons d'attaque des bannières

[src/ui.rs](src/ui.rs). Analyse : [ui_strike/README.md](../ui_strike/README.md). Le proxy
modifie l'interface au moment où l'exe lit ses fichiers, sans mod ni rien à partager avec les autres
joueurs.

`ui_journal`, `ui_mesure_epingles` et `ui_diag_accords` sont des outils de diagnostic. Ils n'existent
que dans les builds locaux : la release est compilée sans la feature `diag`
(voir [release/README.md](../release/README.md#diagnostic)).

| réglage | défaut | effet |
|---|---|---|
| `ui_boutons=on\|off` (`-UiBoutons`) | on | boutons d'attaque (ville, campement) et de frappe nucléaire (silo) au-dessus des drapeaux d'unités ; `ConsumeMouse` sur les boutons nucléaires |
| `ui_recentrage=on\|off` (`-UiRecentrage`) | off | off : plus de recentrage de la caméra au clic sur ces boutons |
| `ui_journal=on\|off` (`-UiJournal`) | off | note chaque script Lua et chaque XML lus |
| `ui_epingles=on\|off` (`-UiEpingles`) | on | ajoute [src/ui_epingles_opti.lua](src/ui_epingles_opti.lua) à `MapPinManager.lua` et `MapPinListPanel.lua` : pas de reconstruction quand aucune épingle n'a changé, une lecture de `GetMapPins` par joueur et par reconstruction, liste fermée reconstruite à son ouverture (voir [map_pins](../map_pins/README.md)) ; test hors jeu : `lua proxy/tests/test_ui_epingles_opti.lua` |
| `ui_carte_entiere=on\|off` (`-UiCarteEntiere`) | on | [src/carte.rs](src/carte.rs) : minimap et caméra sur la carte entière dès le premier tour, sans épingles aux bords. Deux octets de l'exe (le `jnc` qui garde le rectangle des cases révélées devient deux `nop`), site trouvé par signature dans les exe DX12 et DX11, remis au déchargement du proxy. Purement local, sans effet en multijoueur (voir [minimap](../minimap/README.md)) ; test : `cargo test` (signature unique dans les deux exe) |
| `ui_merveilles=on\|off` (`-UiMerveilles`) | on | ajoute [src/ui_merveilles.lua](src/ui_merveilles.lua) à `ProductionHelper.lua` : une merveille retirée de la file par un échange est remise en tête par des commandes du jeu (rajout, puis échanges), rejouées à l'identique chez tous les joueurs, proxy ou non (voir [wonder_queue](../wonder_queue/README.md)) ; test hors jeu : `lua proxy/tests/test_ui_merveilles.lua` |
| `ui_banniere_annuler=on\|off` (`-UiBanniereAnnuler`) | on | ajoute [src/ui_banniere_annuler.lua](src/ui_banniere_annuler.lua) à `CityBannerManager.lua` (XP2 ou BSM) : un clic droit sur l'icône de production du bandeau d'une de ses villes retire l'élément en tête (`BUILD`, `VALUE_REMOVE_AT`, commande ordinaire du jeu) ; test hors jeu : `lua proxy/tests/test_ui_banniere.lua` |
| `ui_reduction_quartiers=on\|off` (`-UiReductionQuartiers`) | on | ajoute [src/ui_quartiers.lua](src/ui_quartiers.lua) à `ProductionPanel.lua` : badge « -40% » sur les quartiers dont le coût est réduit, modèle `COST_PROGRESSION_NUM_UNDER_AVG_PLUS_TECH`. Le badge suit le coût que le jeu a enregistré (`BuildQueue:GetDistrictCost`), comparé au coût plein recalculé comme `1803703c0` (escalade, avancement technos/dogmes, vitesse, `DistrictProductionRate` de l'ère de départ). GameCore ne recalcule ce coût (`1802c4f10`) que pour tous les types à une techno, un dogme ou une ville fondée, et pour le seul type concerné à la pose ou à l'achèvement d'un quartier : la règle recalculée à chaque instant annonçait des réductions que le jeu n'appliquait pas encore. Test hors jeu : `lua proxy/tests/test_ui_quartiers.lua` |
| `ui_avancement=on\|off` (`-UiAvancement`) | on | ajoute [src/ui_avancement.lua](src/ui_avancement.lua) à `WorldTracker.lua` (jeu ou BSM), et un libellé `BGD_Avancement` à `WorldTrackerResearch.xml` et `WorldTrackerCivic.xml` : « x/y (+z) » à droite du nom de la technologie et du dogme en cours, dans le panneau en haut à gauche : science ou culture placée / coût, puis l'excédent gardé par le jeu après une technologie ou un dogme terminé avec plus que son coût. L'excédent est lu dans GameCore par [src/avancement.rs](src/avancement.rs) (`m_xResearchOverflow` en `PlayerTechs + 0x108`, `m_xCulturalOverflow` en `PlayerCulture + 0x430`, 24.8), rendu au Lua par `BGD_Excedent(joueur, 0|1)`, montré dès 0,1 point. Rien pour une technologie ou un dogme terminé ; le nom est raccourci, avec infobulle, s'il chevaucherait le compteur. Rien non plus quand l'hôte a coché l'option de partie « Disable Advanced UI indicators (BGE) » (`BGD_INDICATEURS_OFF`, lue à chaque redessin : une partie suivante dans la même session du jeu suit sa propre valeur). Lecture seule ; test hors jeu : `lua proxy/tests/test_ui_avancement.lua` |
| `ui_debordement=on\|off` (`-UiDebordement`) | on | [src/debordement.rs](src/debordement.rs) relève la production en réserve de chaque ville (`file + 0x1f4`) à chaque mise à jour du cache de la file (appel `18006b8d1` redirigé), et la rend au Lua par `BGD_Debordement(joueur, ville)` ; [src/ui_debordement.lua](src/ui_debordement.lua), ajouté au bandeau de ville, au panneau de la ville (CityPanel) et au panneau de production, affiche la réserve dès 1 point : infobulle du bandeau (« +N » à la place des tours si la file est vide), ligne dans le panneau de la ville, « +N » après la progression du panneau de production. Rien quand l'hôte a coché l'option de partie « Disable Advanced UI indicators (BGE) » (`BGD_INDICATEURS_OFF`, lue à chaque redessin). Lecture seule ; test : `cargo test`, `lua proxy/tests/test_ui_banniere.lua` |
| `ui_mesure_epingles=on\|off` (`-UiMesureEpingles`) | off | ajoute [src/ui_epingles.lua](src/ui_epingles.lua) aux scripts reconstruits à chaque `PlayerInfoChanged` (épingles, liste des épingles, bandeau des dirigeants, DMT) : durées dans `Lua.log`, voir [map_pins](../map_pins/README.md) ; test hors jeu : `lua proxy/tests/test_ui_epingles.lua` |
| `ui_diag_accords=on\|off` (`-UiDiagAccords`) | off | ajoute [src/ui_diag_accords.lua](src/ui_diag_accords.lua) à `DiplomacyDealView.lua` : événements d'accord et de session, et état de l'écran d'accord, dans `Lua.log` (lignes `BGD accords`). Diagnostic des accords croisés, voir [docs/netcode](../docs/netcode/README.md) ; test hors jeu : `lua proxy/tests/test_ui_diag_accords.lua` |

- **Lua.** L'entrée `luaL_loadbuffer` de la table d'imports de l'exe est remplacée : il suffit de
  changer un pointeur, sans réécrire d'instruction, et cela marche en DX12 comme en DX11. Pour
  `CityBannerManager.lua`, quel que soit le mod qui le fournit (Better Spectator Mod dans nos
  parties), le proxy compile un texte modifié :
  - les trois fonctions de clic passent `false` en second argument de `UI.SelectCity` /
    `UI.SelectDistrict` ;
  - [src/ui_boutons.lua](src/ui_boutons.lua) est ajouté à la fin du fichier.
- **XML.** La lecture d'un XML entier (`140a2d280` en DX12, `140a1f3f0` en DX11) est retrouvée par
  une signature de 71 octets, unique dans les deux exe. Après la lecture de
  `CityBannerInstances.xml`, le tampon du jeu est réécrit avec son propre allocateur. On y ajoute les
  calques de [src/ui_boutons.xml](src/ui_boutons.xml) et `ConsumeMouse="1"` sur `NukeBombButton` et
  `ThermoNukeBombButton`. Le patch n'est appliqué que si les cadres reproduits par les calques sont
  présents tels quels.
- **Au-dessus des drapeaux.** Chaque calque reproduit le cadre de sa bannière : même `ZoomAnchor`,
  même ancrage, même taille. Le bloc Lua y déplace le bouton (`ChangeParent`), qui garde ainsi sa
  place, ses rappels et ses mises à jour. Le calque est ensuite placé dans `SelectedUnitContainer`,
  que `InGame.xml` dessine après `BannerAndFlags`. `MapPinManager.lua` fait de même avec l'épingle
  sélectionnée.
- **Suivi des bannières.** Le bloc enveloppe les méthodes de `CityBanner` qui positionnent
  (`UpdatePosition`), masquent (`UpdateVisibility`, `UpdateRangeStrike`, `UpdateWMDBanner`) ou
  libèrent (`Uninitialize`) la bannière. Il masque aussi les calques pendant la révélation d'une
  merveille, et remet chaque bouton dans sa bannière à l'arrêt du contexte.
- **Un seul clic pour attaquer depuis une ville non sélectionnée.** C'est une faute du jeu, présente
  aussi sans mod. Le clic sélectionne la ville puis passe en `CITY_RANGE_ATTACK`. Ensuite,
  `OnCitySelectionChanged` (`CityPanel.lua`) doit garder ce mode si la ville peut tirer. Mais il
  teste `CityManager.GetCity(owningPlayerID, cityID)`, alors que le paramètre s'appelle
  `ownerPlayerID`. Le test échoue donc toujours, et l'interface repasse en `SELECTION`. Dès que
  l'accroche Lua est posée, le proxy corrige ce nom au chargement de `CityPanel.lua`
  (`patch_city_panel`).
- **Tirer en cliquant sur le drapeau de la cible.** En visée d'une ville ou d'un quartier, un clic
  sur le drapeau d'une unité étrangère ne faisait rien. Le drapeau prend le clic avant `WorldInput`,
  et `OnUnitFlagClick` (`UnitFlagManager.lua`) ne sait faire attaquer que l'unité sélectionnée.
  - Le proxy branche les deux clics gauches des drapeaux sur `Civ6RolloverProxy_OnUnitFlagLClick`
    ([src/ui_drapeaux.lua](src/ui_drapeaux.lua)), inséré avant `OnUnitFlagClick` (`patch_unit_flags`).
  - Ce gestionnaire tire sur la case de l'unité, comme un clic sur la case (`CityRangeAttack`,
    `DistrictRangeAttack`). Si la cible n'est pas valide, il appelle `OnUnitFlagClick`.
  - Le clic droit et le lancer de WMD (`ICBM_STRIKE`) ne changent pas.
  - Le correctif travaille sur les octets : le fichier du jeu n'est pas en UTF-8.
- **Production en cours au changement de ville.** Faute du jeu, présente aussi sans mod. Dans
  `ProductionPanel.lua`, la production en cours (`CurrentProductionContainer`) et le message
  « aucune production » (`NoProductionContainer`) ne sont montrés ou cachés qu'au changement
  d'onglet et, file ouverte, à `CityProductionQueueChanged`. Changer de ville passe par `Refresh`
  puis `View`, qui remplit la production en cours sans toucher à sa visibilité. Après une ville sans
  production (par exemple, production choisie puis retirée), celle de la ville suivante restait donc
  cachée.
  - Le proxy ajoute [src/ui_production.lua](src/ui_production.lua) à la fin du fichier, pour lire
    ses variables locales (`patch_production_panel`). Il enveloppe `View` et applique ensuite la
    règle des onglets : file (`OnTabChangeQueue`) ou production (`OnTabChangeProduction`).
  - BBG remplace le panneau par un fichier qui inclut `ProductionPanel.lua` : le correctif s'y
    applique aussi.
  - Interface seule : rien ne part sur le réseau.
  - Test hors jeu, sur le vrai fichier du jeu : `lua proxy/tests/test_ui_production.lua`.
- **Bouton d'or de l'écran d'accord désactivé sans raison.** Faute du jeu, présente aussi sans mod.
  Dans `DiplomacyDealView.lua`, les icônes de la colonne « disponible » viennent de deux réserves
  partagées (`g_IconOnlyIM`, `g_IconAndTextIM`) que `ResetInstances` rend à chaque ouverture sans
  remettre leur état à zéro. Une ressource stratégique sans place sous le plafond, ou l'icône
  réduite d'un accord, d'une cité ou d'une œuvre non valide, désactive son bouton.
  `PopulateAvailableGold` reprend ensuite une de ces instances pour l'or ponctuel ou par tour sans
  appeler `SetDisabled(false)` (`PopulateAvailableFavor` le fait). Si la disposition change d'une
  ouverture à l'autre (autre joueur, autres ressources), on ne peut plus demander ni offrir d'or :
  le bouton paraît normal mais ne répond pas.
  - Le proxy ajoute [src/ui_accord_or.lua](src/ui_accord_or.lua) à la fin du fichier
    (`patch_accord_or`). Il enveloppe `PopulateAvailableGold` et réactive les boutons qu'elle vient
    de prendre, ajoutés à la fin de `m_AllocatedInstances`.
  - MPH (`diplomacydealview_MPH.lua`) remplace `PopulateAvailableGold` en rappelant celle du jeu ;
    il est inclus par le joker `include("DiplomacyDealView_")` avant le bloc, qui l'enveloppe donc
    aussi.
  - Toujours actif, sans réglage. Interface seule : rien ne part sur le réseau, il suffit que le
    joueur qui ouvre l'écran ait le proxy.
  - Test hors jeu, avec le vrai `InstanceManager.lua` : `lua proxy/tests/test_ui_accord_or.lua`.
- **Accord entre humains pendant le passage de tour.** Un accord proposé ou accepté dans la
  dernière seconde arrive chez l'hôte pendant le passage de tour ; l'hôte lance alors le tour
  suivant trop tôt et des joueurs voient leur tour passer en boucle (analyse :
  [depart_tour](../depart_tour/README.md#accord-pendant-le-passage-de-tour--tours-passés-en-boucle)).
  - Le proxy ajoute [src/ui_accord_fin.lua](src/ui_accord_fin.lua) à la fin du fichier, après le
    bouton d'or (`patch_accord_fin`). Il remplace la globale `DealManager` de ce contexte par une
    table dont seule `SendWorkingDeal` change, le reste venant du jeu. Vers un humain, en
    multijoueur en réseau, l'accord ne part pas minuteur local écoulé (fin forcée comprise), ou
    quand plus aucun humain n'a son tour actif ; une ligne locale le dit au joueur.
  - Toujours actif, sans réglage. Interface seule : rien ne part sur le réseau, il suffit que le
    joueur qui envoie l'accord ait le proxy.
  - Test hors jeu : `lua proxy/tests/test_ui_accord_fin.lua`.
- **Code commun avec le proxy de démarrage.** La table d'imports, la signature et le tampon de
  la lecture XML sont dans [src/exe.rs](src/exe.rs), que `startup_proxy` inclut aussi. Le proxy de
  démarrage détourne la lecture XML le premier : on y trouve alors son `jmp [rip+0]`, qu'on recopie
  dans notre trampoline (`ui : lecture XML déjà détournée (proxy de démarrage), détour enchaîné`).
  Au déchargement, on remet ce saut, pas le prologue d'origine.
- **L'accroche Lua.** L'exe importe les fonctions HavokScript sous leur nom C++ décoré
  (`?luaL_loadbuffer@@YAHPEAUlua_State@@PEBD_K1@Z`). La recherche dans la table d'imports accepte
  donc les deux formes.
- **Le journal.** `ui : accroches posées (…)` au chargement de la partie. Puis
  `ui : …CityBannerInstances.xml : calques ajoutés`, et
  `ui : …CityBannerManager.lua modifié (…), compilation réussie`, et
  `ui : …CityPanel.lua : owningPlayerID corrigé, compilation réussie`, et
  `ui : …UnitFlagManager.lua : clic sur un drapeau en visée d'une ville ou d'un quartier, compilation réussie`, et
  `ui : …ProductionPanel.lua : production en cours affichée au changement de ville, compilation réussie`, et
  `ui : …DiplomacyDealView.lua : bouton d'or disponible réactivé, pas d'accord vers un humain pendant le passage de tour, compilation réussie`.
  `Lua.log` doit contenir
  `Civ6RolloverProxy : boutons d'attaque au-dessus des drapeaux d'unités`.
- **Tests hors jeu** (`cargo test --release`) : les remplacements de texte sur les fichiers du jeu et
  sur ceux de BSM, le bon format du XML, l'adresse trouvée par la signature dans les deux exe, la
  lecture de la table d'imports. Le bloc Lua passe `luac -p`.

## Sixième accroche : compteur de tours de production

[src/tours.rs](src/tours.rs), réglage `tours_production=on|off` (`-ToursProduction`), on par défaut.
Analyse : [production_turns/README.md](../production_turns/README.md).

- **La faute du jeu.** Le nombre de tours affiché ignore les bonus plats de gouvernement, que la
  production réelle applique : fascisme (+50 % aux unités), autocratie (+10 % aux merveilles),
  république marchande (+15 % aux districts). Une unité affichée à 6 tours passe ainsi à 4 restants
  après un seul tour.
- **Interface seulement.** Les prédicteurs de tours servent aussi à l'IA, qu'on ne touche pas : cela
  désynchroniserait les joueurs sans proxy. Le proxy redirige seulement les quatre `call rel32` du
  cache de l'interface (`Cache_City_BuildQueue`) :
  - `18006b8dc` et `18006c8d3`, vers `1800fe9b0` ;
  - `18006c6ad` et `18006c7b5`, vers `1800fe300`.

  Les appels passent par un relais `jmp [rip+0]` alloué à moins de 2 Go de GameCore. Ils mènent à une
  réécriture de ces deux répartiteurs, qui passe aux prédicteurs d'origine un bonus ou un
  multiplicateur contenant le facteur `1 + 0,01 × bonus`.
- **DX11 et DX12.** Tout est dans GameCore, chargé à l'identique par les deux exe.
- **Garde-fou.** Chaque site doit être un `call` vers la fonction attendue, sinon rien n'est
  redirigé. Les appels d'origine reviennent au déchargement du proxy.
- **Le journal.** `tours : compteur de tours de production corrigé (4 appels du cache de
  l'interface)`.
- **Test hors jeu.** `check_hooks.py` vérifie la redirection des quatre appels, puis leur
  restauration. `cargo test` vérifie la conversion du facteur en virgule fixe.

## Septième détour : choix des cases travaillées (correctif POL)

[src/citoyens.rs](src/citoyens.rs). Analyse : [citoyens/README.md](../citoyens/README.md).

- **La faute du jeu.** Le sac à dos qui choisit les cases (`18010f050`) compare les combinaisons
  avec une règle qui n'est pas un ordre : avec le focus nourriture et production, une ville peut
  travailler une 2/1 à côté d'une 2/2 libre.
- **Le détour** refait le choix exactement : au plus `K` emplacements ; parmi ceux qui couvrent le
  déficit de nourriture s'il y en a, le meilleur score favori puis autre ; à égalité, le moins
  d'emplacements. Prologue recopié : 15 octets de `mov` sans adresse relative.
- **Correctif POL, sans réglage.** Il change la simulation : il ne s'applique que si l'export
  `BgdPolActifs` du proxy de démarrage rend 1 (tous les joueurs ont la même version, ou partie
  locale). Sinon, ou sans le proxy de démarrage, l'originale. Le détour posé lève le bit 16 de
  `RolloverProxyStatus` ; avec `actif=off`, il n'est pas posé, et le proxy de démarrage ne
  s'annonce pas dans le salon. Il ne s'annonce pas non plus si la version de ce proxy (export
  `BgdVersion`, ou marque `BgdVersion=<version>` du fichier tant qu'il n'est pas chargé) n'est pas
  la sienne : installation partielle, ou proxy plus ancien que cet export.
- **Le journal.** `citoyens : détour posé en 0x18010f050, BetterGameExperience 0.2.0`,
  `pol : correctifs POL actifs|coupés` (état lu par [src/pol.rs](src/pol.rs), commun aux
  correctifs POL), et les 30 premiers écarts de chaque partie avec le choix de l'originale
  (`citoyens : choix corrigé …`).
- **Tests.** `cargo test` compare le choix à une recherche exhaustive (20 000 tirages) ;
  `check_hooks.py` vérifie la pose et la restauration du détour.

## Spectateurs hors du tourisme et du vote du congrès (correctif POL)

[src/spectateurs.rs](src/spectateurs.rs), [src/ui_congres.lua](src/ui_congres.lua),
[src/ui_congres_cibles.lua](src/ui_congres_cibles.lua). Analyse :
[spectateurs/README.md](../spectateurs/README.md). Un spectateur de Better Spectator Mod est un majeur
humain vivant dont le leader est `LEADER_SPECTATOR`.

- **Tourisme.** `18027ce10` (touristes que A prend à P) et `18027e960` (demande totale sur P)
  divisent par le nombre de majeurs ayant vécu, spectateurs compris : réécrites à l'identique, ce
  nombre compté sans eux. Le prologue de `18027ce10` lit `GlobalParameters` relativement à RIP : le
  trampoline le refait en `movabs`.
- **Vote.** `1803c0ba0` et `1803c0d40` refusent le vote d'un spectateur. Au dépouillement
  (`1803c5f50`), sa voix d'office pour le choix A est retirée.
- **Scripts.** `WorldCongress.lua` (gameplay) : les cinq résolutions à cible joueur ne proposent plus
  les spectateurs. `WorldCongressPopup.lua` : pour un spectateur, fenêtre en lecture seule pendant
  toute la session, tour du congrès soumis d'office (la session ne l'attend plus), fin de tour qui
  ne ferme plus la fenêtre, bouton « Fermer ». Les deux blocs lisent `Civ6RolloverProxy_Pol()`,
  posée par le proxy dans l'état Lua qui les compile.
- **Correctif POL, sans réglage.** Même règle que le précédent (`BgdPolActifs`). Sans lui, le jeu
  d'origine partout, scripts compris.
- **Le journal.** `spectateurs : détours posés (tourisme, votes, dépouillement)`,
  `spectateurs : tourisme divisé par N' majeurs au lieu de N`, les 20 premiers votes refusés et
  dépouillements corrigés de chaque partie, et `ui : WorldCongressPopup.lua : fenêtre du congrès en
  lecture seule pour les spectateurs`, `ui : WorldCongress.lua : spectateurs retirés des cibles des
  résolutions`.
- **Tests.** `cargo test` compare la formule réécrite à celle de l'assembleur (100 000 tirages) et
  patche les vrais scripts ; `lua proxy/tests/test_congres.lua` exécute les deux blocs sur les vrais
  fichiers avec des bouchons ; `check_hooks.py` vérifie la pose et la restauration des cinq détours.

## Congrès mondial : tours passés, retour au vote

[src/congres.rs](src/congres.rs) et quatre blocs d'interface. Analyse et liste des causes :
[congres/README.md](../congres/README.md).

- **Retour au vote (correctif POL).** `18005b640` (le segment permet-il d'annuler sa fin de tour ?)
  est réécrite : elle répond « permis » pendant le vote (`TURNSEG_WORLDCONGRESS_1` et `_2`), le
  reste des conditions du jeu tenant toujours (minuteur non écoulé, au moins un autre joueur encore
  actif). Les détours de vote de `spectateurs` rendent au joueur le coût de ses voix déjà comptées
  avant un second vote sur le même sujet, et le reprennent si le jeu refuse ce vote. Posé seulement
  si les détours de `spectateurs` le sont.
- **Écran d'attente.** `WorldCongressBetweenTurns.xml` reçoit le bouton `BGD_Revoter` («
  Revenir au vote »), `WorldCongressBetweenTurns.lua` le montre quand le jeu acceptera l'annulation
  (`Civ6RolloverProxy_Revote()`, `CanUnreadyTurn`, minuteur) et remet en attente un joueur revenu
  au vote. `WorldCongressPopup.lua` se rouvre alors avec les votes envoyés, la faveur affichée
  comptant ce que GameCore rendra.
- **Retirés le 2026-11-08** : l'attente des résolutions sur un cache vide (`WorldCongressPopup.lua`),
  le blocage de la fin de tour automatique pendant le vote (`ActionPanel_Expansion2.lua`) et les 240 s
  de MPH pendant le vote (`TurnProcessing.lua` de MPH). Le minuteur et la fin de tour du jeu (et de
  MPH) reprennent la main pendant le vote.
- **Luxes (correctif POL).** `WorldCongress.lua` : la liste des luxes de l'interdiction d'un luxe est
  recalculée à chaque session, au lieu d'être gardée depuis le premier appel de l'état Lua.
- **Intro (local).** `WorldCongressIntro.lua` s'ouvrait à chaque `WorldCongressStage1` / `2` ;
  l'annulation de fin de tour d'un autre joueur relance l'événement. Elle ne s'ouvre plus qu'une
  fois par tour et par session (vérifié en partie le 2026-11-06).
- **Pas de réglage** : ce sont des corrections de bugs.
- **Le journal.** `congrès : détour posé (…)`, `congrès : annulation de fin de tour permise pendant le
  vote`, `congrès : joueur N revote sur … : X voix déjà comptées, Y de faveur rendue`, et dans
  `Lua.log` les lignes `BGE congrès : …` (retour au vote).
- **Tests.** `cargo test` (décision d'annulation, hachages, blocs ajoutés aux vrais scripts),
  `lua proxy/tests/test_congres.lua` (cache vide laissé au jeu, retour au vote et faveur, bouton,
  luxes, intro), `check_hooks.py` (pose et restauration de `18005b640`).

## Téléportation au départ d'un district (correctif POL)

[src/ports.rs](src/ports.rs). Analyse : [analyses/teleportation/README.md](../analyses/teleportation/README.md).

- **La règle du jeu.** `UNITOPERATION_TELEPORT_TO_CITY` (`18067d290`) exige que l'unité parte d'un
  centre-ville (`18005e770`), alors qu'une unité navale peut arriver dans un port (`18067d650`).
- **Le détour.** L'appel `call 18005e770` en `18067d315` est redirigé vers un relais
  (`mov rdx,r14 ; jmp [rip+0]`) : le départ est aussi accepté pour une unité navale posée sur un
  district, quel qu'il soit (port, parc aquatique, canal…), comme à l'arrivée. Une première version
  exigeait un district `DOMAIN_SEA` et refusait le parc aquatique (testé en jeu le 2026-11-05). Le
  contrôle de propriétaire, les destinations et l'interface ne changent pas.
- **Correctif POL, sans réglage.** Même règle que les précédents (`BgdPolActifs`).
- **Garde-fou.** Le site doit être `mov rcx,rax ; call 18005e770 ; test al,al`, sinon rien n'est
  redirigé. L'appel d'origine revient au déchargement du proxy.
- **Le journal.** `ports : départ de TELEPORT_TO_CITY accepté depuis un district (appel redirigé en
  0x18067d315)`, puis les 20 premiers départs acceptés de chaque partie.
- **Tests.** `check_hooks.py` vérifie la redirection et sa restauration.

## Unités empilées hors du soutien et du contournement (correctif POL)

[src/empilement.rs](src/empilement.rs). Analyse : [empilement_combat/README.md](../empilement_combat/README.md).

- **La règle du jeu.** Le contournement (`18036c2a0`) et le soutien (`18036d1d0`) donnent +2 par
  unité alliée voisine, sans regarder l'empilement : deux unités de même classe sur une case voisine
  (unité achetée dans une ville occupée, déplacement interrompu…) donnent +4.
- **Le détour.** Dans chacun, l'appel `call 1802c4360` qui retrouve l'unité de l'entrée courante
  (`18036c3e1`, `18036d301`) est redirigé vers un relais (`mov r8,rsi ; mov r9,r14 ; jmp [rip+0]`)
  qui ajoute le porteur du bonus (attaquant ou défenseur) et la liste d'unités de la case. Une unité
  en excédent est rendue nulle et ignorée : même propriétaire et même classe (`LAND_COMBAT` ou
  `NAVAL` seulement) qu'une unité vivante, avec une force, placée avant elle sur la case, ou que le
  porteur s'il est sur sa case. Le calcul, la prévisualisation et la garnison ne changent pas.
- **Correctif POL, sans réglage.** Même règle que les précédents (`BgdPolActifs`) ; `VERSION` à passer à 0.7.0
  à la prochaine release.
- **Garde-fou.** Chaque site doit être `mov edx,[rdi+4] ; mov rcx,[rax+0x6c8] ; call 1802c4360 ;
  mov rbx,rax ; test rax,rax`, sinon aucun des deux n'est redirigé. Les appels d'origine reviennent
  au déchargement du proxy.
- **Le journal.** `empilement : unités en excédent ignorées par le soutien et le contournement
  (appels redirigés en 0x18036c3e1 et 0x18036d301)`, puis les 20 premières unités ignorées de
  chaque partie (prévisualisations comprises).
- **Tests.** `cargo test` (règle d'excédent, hachages des classes) ; `check_hooks.py` vérifie les
  octets des deux sites, la redirection et sa restauration.

## Canaux sur colline (correctif POL)

[src/canaux.rs](src/canaux.rs). Analyse : [analyses/canaux/README.md](../analyses/canaux/README.md).

- **La règle du jeu.** Le placement du quartier canal refuse une colline en dur (`call 180060220`
  en `180362eb6`, la colline de la case), même quand `District_ValidTerrains` l'autorise.
- **Le détour.** L'appel est redirigé vers un relais (`jmp [rip+0]`) : la colline ne compte plus
  quand les correctifs POL sont actifs, et le terrain est jugé par la base de règles seule. Le jeu
  de base n'y met que les terrains plats : rien ne change sans un mod qui y ajoute les collines.
- **Correctif POL, sans réglage.** Même règle que les précédents (`BgdPolActifs`).
- **Garde-fou.** Le site doit être `mov rcx,rsi ; call 180060220 ; test al,al ; jne`, sinon rien
  n'est redirigé. L'appel d'origine revient au déchargement du proxy.
- **Le journal.** `canaux : test de colline du quartier canal redirigé (0x180362eb6)`, puis les 10
  premières collines acceptées.
- **Tests.** `cargo test` (règle, octets du site dans le GameCore installé).
- **Crédit.** Site trouvé par Hotaru Snowy (« Allow Canals on Hills - dll version ») ; aucun code repris.

## Diplomatie reçue pendant le passage de tour (correctif POL)

[src/fin_tour.rs](src/fin_tour.rs). Analyse : [depart_tour/README.md](../depart_tour/README.md),
« Diplomatie reçue pendant le passage de tour : la cause dans GameCore » (rapport BGE-FB5126).

- **Le défaut du jeu.** Après `EndTurn`, le prochain `Game::Processor::Update` démarre le tour
  suivant, quel qu'en soit l'appelant. `GameUpdate` attend le contrôle de synchro, mais
  `DiplomacyAction`, `DiplomacyDeal` et `DiplomacySession` lancent l'étape de mise à jour
  (`18063e2e0`) sans regarder le drapeau de fin de tour : reçus dans ce créneau, ils démarrent le
  tour chez l'hôte avant la synchro, et des joueurs commencent le tour « tour fini ».
- **Le détour.** Les trois appels (`jmp` terminal en `18003514f` et `1800353b8`, `call` en
  `1800358f2`) sont redirigés vers un relais (`mov edx,quel ; jmp [rip+0]`) qui saute l'étape quand
  le drapeau est levé (`18004ab90`), comme `GameUpdate::Execute`. L'action est appliquée ; le
  `GameUpdate` initial lance la mise à jour. Pendant le tour (joueur qui a passé son tour puis
  agit), rien ne change.
- **Correctif POL, sans réglage.** Même règle que les précédents (`BgdPolActifs`).
- **Garde-fou.** Les trois sites (instruction qui précède, opcode, cible `18063e2e0`) et les octets
  de `18004ab90` doivent correspondre, sinon rien n'est redirigé. Les appels d'origine reviennent au
  déchargement du proxy.
- **Le journal.** `fin de tour : mise à jour après DiplomacyAction/Deal/Session suspendue pendant le
  passage de tour (3 appels redirigés)`, puis les 20 premières mises à jour sautées de chaque partie.
- **Tests.** `cargo test` (octets des sites dans le GameCore installé, s'il est là),
  `check_hooks.py` (redirection et restauration).

## Départ et fin de tour simultanés (option de partie « No First/Last moves (BGE) »)

[src/depart.rs](src/depart.rs), [src/ui_depart.lua](src/ui_depart.lua),
[src/ui_depart_chat.lua](src/ui_depart_chat.lua). Conception et reverse :
[depart_tour/README.md](../depart_tour/README.md).

- **Deux détours, posés à chaque partie.** L'envoi des messages de GameCore (`1800366b0`) : pendant
  l'attente, les ordres du joueur ne partent pas. La pause du minuteur de l'exe (`1400f8590` en
  DX12, retrouvée par signature dans les deux exe) : pendant l'attente, ses deux chronos restent à
  zéro ; au départ, ils repartent de zéro. Ni l'un ni l'autre ne touche la simulation.
- **Le protocole, en Lua** (`ui_depart.lua`, à la fin de `InGame.lua`) : prêt (`.bgd_pret <tour>`
  à l'hôte) quand l'interface a rattrapé le début du tour, départ (`.bgd_depart <tour>`) quand
  tous les humains équipés et connectés sont prêts, ou après 60 s. Signaux en messages privés,
  cachés par `ui_depart_chat.lua` (à la fin de `ChatPanel.lua`). Actif seulement si l'hôte a coché
  l'option (`BGD_DEPART_SIMULTANE`, ajoutée au salon par le proxy de démarrage) et que les
  correctifs POL sont actifs.
- **Synchro de tour de MPH coupée.** Le même bloc met `CPL_SYNCTURN` à faux sur chaque machine,
  option cochée ou non : elle ne synchronise rien.
- **Garde-fous.** Sans départ reçu, le proxy lève l'attente au premier ordre tenté après 120 s.
  Un joueur qui se reconnecte en plein tour reçoit le départ aussitôt ; après une migration d'hôte,
  les signaux repartent vers le nouvel hôte.
- **Fin simultanée** (même option). Quand le minuteur de l'hôte arrive à zéro, il envoie
  `.bgd_fin <tour>` à chacun ; à réception, le proxy ajoute 10⁶ s au cumul des chronos de l'exe,
  qui termine le tour du joueur à l'image suivante, puis les retire au début du tour suivant.
  Mêmes conditions que le départ, plus le minuteur détourné.
- **Réglage** `depart_signaux=on|off` (off) : les signaux restent visibles dans le chat, avec une
  ligne locale quand la fin de l'hôte termine le tour.
- **Le journal.** `depart : détours posés (envoi des ordres, minuteur en exe+0x…)`, puis
  `depart : attente`, `… retenu`, `depart : parti (…)`, et les lignes du bloc Lua (`prêt pour le
  tour N`, `tour N lancé`, `tour N : minuteur de l'hôte écoulé, fin envoyée`, `depart : fin du tour
  forcée (minuteur de l'hôte écoulé)`).
- **Tests.** `cargo test` (signature unique dans les deux exe, blocs ajoutés), `check_hooks.py`
  (site de l'envoi), et le protocole sur plusieurs machines simulées :

  ```bash
  lua proxy/tests/test_ui_depart.lua
  ```

## Merveille retirée de la file après deux échanges

Analyse et trace : [wonder_queue/README.md](../wonder_queue/README.md). Quand une merveille déjà
posée revient en tête de file après un échange, `1801007a0` revérifie sa case avec `18025bad0`, qui
refuse : la merveille est retirée.

- **Le correctif actif : `ui_merveilles`** (voir le tableau des réglages de l'interface). Le proxy ne
  change aucune règle : l'interface du joueur renvoie le rajout de la merveille puis des échanges,
  des commandes du jeu que toutes les machines rejouent à l'identique. Pas de désynchro, même face
  à des joueurs sans proxy.
- **Mis de côté : le correctif dans GameCore**, [wonder_queue/merveilles_gamecore.rs](../wonder_queue/merveilles_gamecore.rs),
  qui faisait réussir le test de `180100a9a` quand la case porte encore le chantier de cette ville.
  Il change une règle chez un seul joueur et désynchronise la partie si les autres ne l'ont pas. Il
  n'est plus compilé ; son en-tête dit comment le rebrancher.

## Rendements d'empire après un gouverneur ou un personnage illustre

[src/rendements_empire.rs](src/rendements_empire.rs) et [src/ui_rendements.lua](src/ui_rendements.lua),
réglage `ui_rendements=on|off` (`-UiRendements`), on par défaut. Analyse et traces :
[empire_yields/README.md](../empire_yields/README.md).

- **Le défaut.** Après un titre de gouverneur, un déplacement de gouverneur ou un personnage
  illustre, le bandeau du haut garde ses anciens rendements jusqu'à un changement de citoyen.
  GameCore calcule pourtant les bons : ce sont les instantanés de l'interface qui ne sont pas
  refaits, ou refaits avant que l'action ne soit appliquée.
- **Côté GameCore.** Quatre détours notent le joueur local : promotion (`1802df340`), retrait
  (`1802df800`) et établissement (`180259b10`) d'un gouverneur, activation d'un personnage illustre
  (`180655560`). Une fois l'action appliquée, on appelle les gestionnaires de cache qu'un
  changement de citoyen déclenche (`0xc2f385c0`) : villes (`180072f80`), culture, foi, sciences,
  trésor. À la sortie de l'action, ou à la fin de la passe de `Game_Processor` (`18004afc0`) si
  elle s'y exécute : détour de `coalesce`, posé pour ce réglage même en mode Off ou en portée
  `cascade`.
- **Côté interface.** `ui_rendements.lua`, ajouté à `TopPanel.lua` : `OnRefreshYields` sur les
  événements de gouverneur et de personnage illustre, puis encore à `GameCoreEventPublishComplete`.
- **Sans désynchro.** Les gestionnaires ne font que lire l'état de jeu et poster un instantané à
  l'interface. Aucun événement Lua, aucune commande : face à des joueurs sans proxy aussi.
- **Journal :** `rendements d'empire : détours posés …`, puis, pour les 10 premiers de chaque
  partie, `rendements d'empire : <action> du joueur N, rafraîchi en fin de l'action|de passe` et
  `instantanés refaits pour le joueur N`, et
  `ui : …TopPanel.lua : rendements redessinés …`.
- **Tests.** `cargo test` (enregistrement d'événement, patch du vrai `TopPanel.lua`) ;
  `check_hooks.py` vérifie la pose et la restauration des quatre détours.

## Mods d'interface repris

[src/mods_ui.rs](src/mods_ui.rs) et [src/mods](src/mods). Analyse, choix et diagnostic de RMA :
[mods_ui/README.md](../mods_ui/README.md). Cinq mods du Workshop, repris avec leurs défauts corrigés,
plus les tours des routes commerciales.

| réglage | défaut | effet |
|---|---|---|
| `ui_charges=on\|off` (`-UiCharges`) | on | charges restantes sur le drapeau des unités qui en ont (Better Builder Charges Tracking, wltk) |
| `ui_routes=on\|off` (`-UiRoutes`) | on | tours avant la fin de la route sur le drapeau des négociants du joueur ; dans l'écran de choix des routes, l'or que reçoit la destination d'une route intérieure (Poundmaker), ajouté par [src/mods/routes_choix.lua](src/mods/routes_choix.lua) à `TradeRouteChooser.lua` ; test hors jeu : `lua proxy/tests/test_mods_routes_choix.lua` |
| `ui_notifications=on\|off` (`-UiNotifications`) | on | bouton « tout effacer » des notifications (NotificationClear, 336tarot) |
| `ui_bandeau=on\|off` (`-UiBandeau`) | on | nourriture, production, population, activités moyennes par ville et luxes dans le bandeau (TopPanel Extension Pro, 号码菌 / Azukii) |
| `ui_politiques=on\|off` (`-UiPolitiques`) | on | effet chiffré des cartes de politique (Extended Policy Cards, Aristos), avec RMA corrigé |
| `ui_rapports=on\|off` (`-UiRapports`) | on | écran des rapports de Better Report Screen (Infixo), avec RMA corrigé |

- **Le proxy prime.** Réglage actif, les fichiers du mod sont neutralisés quand l'exe les compile ou
  les lit, reconnus par leur nom et une marque de leur contenu. Réglage inactif, le mod reste tel quel.
- **Trois étapes par script.** L'accroche `luaL_loadbuffer` de `ui` remplace d'abord le texte entier
  (`mods_ui::remplacer`), applique ensuite ses correctifs, puis ajoute les blocs des reprises
  (`mods_ui::ajouter`). La compilation n'a lieu qu'une fois. Les XML passent par `mods_ui::xml`.
- **Fichiers embarqués.** Le proxy pose deux fonctions natives dans chaque contexte dont il change un
  script :
  - `Civ6Proxy_Charger(nom)` compile un fichier embarqué : les pages de BRS, RMA,
    [commun.lua](src/mods/commun.lua) et les textes ;
  - `Civ6Proxy_FinRoute` rend le tour de fin d'une route (voir plus bas).
- **RMA dans chaque contexte qui s'en sert**, l'écran du gouvernement et celui des rapports. Il tourne
  dans l'environnement du contexte hôte : HavokScript n'isole pas un chunk par `setfenv`. Toutes ses
  globales sont donc préfixées `RMA_`, et il travaille sur sa propre copie de `YieldTypes`.
  `proxy/tests/test_mods_rma.lua` vérifie qu'il ne change aucune globale de l'hôte.
- **Textes des mods** : [gen_textes.py](gen_textes.py) les lit dans les XML des mods et écrit
  `src/mods/textes.lua` et `textes.tsv`. Côté Lua, nos copies appellent `BGD_Lookup`. Côté XML,
  `mods_ui::traduire_xml` remplace les `"LOC_…"`, dans la langue rendue par
  `Localization::GetCurrentLanguage`.

  ```bash
  .venv/Scripts/python.exe -I proxy/gen_textes.py "C:/Program Files (x86)/Steam/steamapps/workshop/content/289070" proxy/src/mods
  ```

- **Le bandeau** est ajouté au script du contexte TopPanel, quel qu'il soit. C'est le premier compilé
  après la lecture de `TopPanel.xml` : toute la chaîne est alors chargée.
- **La fin d'une route commerciale** n'est pas exposée en Lua. `Civ6Proxy_FinRoute` la lit dans
  GameCore :
  - `18038f6c0` rend le gestionnaire de commerce ;
  - `180386480(gestionnaire + 0xc0, &origine, &destination)` rend la fiche de la route : le
    négociant en `+0x8` / `+0xc`, le tour de départ en `+0x4` ;
  - `18038f720(gestionnaire, départ)` rend la fin de la durée minimale.

  La route finit au premier retour du négociant chez lui après cette fin. Le bloc compte le trajet
  avec `TradeManager:GetTradeRoutePath`, à une case par tour.

  Les octets de ces trois fonctions sont vérifiés au chargement. La lecture se fait depuis le thread
  de l'interface, sans rien écrire.
- **Journal :**
  - `ui : mods repris : charges on, …`, avec les autres accroches ;
  - `ui : <script> : <ce qui est fait>, compilation réussie`, par exemple
    `TopPanel Extension Pro neutralisé ; bandeau de TopPanel Extension Pro` ;
  - `ui : <xml> : écran de Better Report Screen du proxy (fr_FR)` ;
  - `mods : fonctions natives posées` et
    `mods : fin des routes commerciales lue dans GameCore`.
- **Tests :**
  - `cargo test` : patchs des vrais fichiers du jeu, neutralisation des fichiers des mods, traduction
    du XML, octets des fonctions de commerce ;
  - `lua proxy/tests/test_mods_rma.lua` : corrections de RMA ;
  - `lua proxy/tests/test_mods_drapeaux.lua` : charges et routes ;
  - `lua proxy/tests/check_havok.lua <fichiers>` : syntaxe des scripts HavokScript.
- **Crédits** : section en bas de l'onglet BetterGameExperience (`PROXY_CREDITS` de
  `startup_proxy/src/options_proxy.lua`) :
  - Better Report Screen, Real Modifier Analysis et ReportsList Loader : Infixo ; onglets Unités et
    Accords d'origine : Greg Miller ; remerciements : nightfearer, Tyolan, copysoul, RohBang, Gorlod,
    Zergus, Rac, Aristos ;
  - Extended Policy Cards : Aristos ;
  - TopPanel Extension Pro : 号码菌 (Azukii) ; remerciements : JNNR ;
  - NotificationClear : 336tarot ;
  - Better Builder Charges Tracking : wltk ; remerciements : DeepLogic, 小小雨10101.

## Défauts d'interface corrigés (raccourcis, émissaires, colon, déplacements)

Interface seule, rien ne part sur le réseau, pas de réglage (bugs purs). Ce sont des entrées de
`FIXES` dans [src/ui.rs](src/ui.rs), appliquées dès que l'accroche Lua est posée. Échap des émissaires, Entrée et
zone de déplacement : confirmés en partie le 2026-11-06 ; émissaires en attente : le 2026-11-08 ;
raccourcis et lentille du colon : encore à confirmer.

| fichier patché | défaut | correctif | analyse |
|---|---|---|---|
| `InGame.lua` (XP2 ou MPH) | raccourcis coupés en cours de partie : un contexte d'entrée (`GameOptions` du menu Échap fermé par une diplomatie, `Popup` du congrès empilé deux fois) reste en haut de la pile, et les actions du contexte `World` ne partent plus | [src/ui_raccourcis.lua](src/ui_raccourcis.lua), ajouté après le bloc du départ simultané : retire ce contexte quand son écran est fermé (au repos, à chaque touche, après une diplomatie), avec une ligne `BGD raccourcis : contexte d'entrée N retiré` dans `Lua.log` | [raccourcis](../analyses/raccourcis/README.md) |
| `MPHOptions.lua`, `VotePanel.lua` (MPH) | la pause de MPH passe au-dessus du menu MULTIPLAYER HELPER (Échap puis le bouton) et du vote de remap : la pause est une popup (priorité `PausePanel`, Utmost + 2), les deux panneaux de simples contextes d'InGame affichés par `SetHide(false)`, et le cadre de la pause (`ConsumeMouse`) prend les clics de leurs boutons | chaque `ContextPtr:SetHide(` devient `BGD_PopupMPH(`, définie sur la première ligne : afficher met le panneau dans la file des popups à la priorité du menu Échap (`InGameTopOptionsMenu`, Utmost + 5), masquer l'en retire ; seulement si le texte est celui de MPH | — |
| `CityStates.lua` (copie de BSM) | Échap fait glisser l'écran des émissaires sans `Close()` : les émissaires en attente restent, le blocage de fin de tour aussi | `OnClose` remis dans le gestionnaire d'Échap, comme dans le fichier du jeu (laissé tel quel) | [emissaires_echap](../analyses/emissaires_echap/README.md) |
| `CityStates.lua` (jeu ou BSM) | émissaires en attente effacés à chaque rafraîchissement de l'écran : émissaire, quête, guerre ou paix de n'importe quel joueur | [src/ui_emissaires.lua](src/ui_emissaires.lua), ajouté à la fin : les choix en attente sont gardés, sauf pour une cité-État détruite ou à laquelle l'équipe du joueur déclare la guerre ; une cité-État en guerre par son suzerain garde les siens, qui ne partent pas à la confirmation | [emissaires_reinit](../analyses/emissaires_reinit/README.md) |
| `MinimapPanel.lua` | lentille du colon vide sur les cases visibles : `OnCityAddedToMap` vide la couche à chaque ville fondée, par n'importe quel joueur, sans la recolorier (les cases du brouillard gardent leur ancienne couleur) ; défaut secondaire : les cases à 2 ou 3 d'une ville fondée hors des terres du joueur restent coloriées comme fondables (bit du cache de GameCore périmé) | `OnCityAddedToMap` redemande le coloriage, `OnFlushChanges` recolorie si la couche est allumée, [src/ui_colon_fin.lua](src/ui_colon_fin.lua) recolorie à la sélection d'une unité ; [src/ui_colon.lua](src/ui_colon.lua) passe les cases trop proches en interdites (villes révélées seulement, sous `pcall`) | [filtre_colon](../analyses/filtre_colon/README.md) |
| `ActionPanel.lua` | Entrée fait « Action suivante » même retirée des touches : `OnInputHandler` la teste en dur | [src/ui_entree.lua](src/ui_entree.lua) : Entrée n'agit que si l'action `EndTurn` porte Entrée, Maj+Entrée (fin de tour forcée) reste toujours actif ; première lecture notée dans `Lua.log` (`Action suivante = […]`) | [raccourci_entree](../analyses/raccourci_entree/README.md) |
| `SelectedUnit.lua` | pas de zone de déplacement sur un quartier défendable ennemi : le parcours de GameCore donne 0 PM à la case de départ | [src/ui_mouvements.lua](src/ui_mouvements.lua) enveloppe `RealizeMoveRadius` et refait la zone avec les chemins du survol (`GetMoveToPathEx`), dans ce cas seulement (quartier terminé ou non) : exploration par front, chemins réutilisés, cases inaccessibles écartées d'après la vraie case (infranchissable, eau sans embarquement, océan non débloqué), zones de contrôle ennemies dessinées et non traversées, zone en cache, budget de 50 ms ; lignes `BGD mouvements:` dans `Lua.log` avec `ui_mesure_mouvements=on` (`-UiMesureMouvements`, off par défaut, option avancée de l'onglet) ; test hors jeu : `lua proxy/tests/test_ui_mouvements.lua` | [quartier_capture_mouvements](../analyses/quartier_capture_mouvements/README.md) |
| `GreatPeoplePopup.lua`, `CivilopediaPage_GreatPerson.lua` | effet d'Imhotep à 175 quelle que soit la vitesse : `LOC_GREAT_PERSON_INDIVIDUAL_IMHOTEP_PRODUCTION_ANCIENT_CLASSICAL` écrit le nombre en dur (les autres ingénieurs passent par `{Amount}`), alors que les deux modificateurs (`_OTHER` 175, `_ANCIENT_CLASSICAL` 350) sont `ScaleByGameSpeed` : 87 et 175 en vitesse en ligne | [src/ui_imhotep.lua](src/ui_imhotep.lua), ajouté à la fin des deux fichiers : enveloppe `PopulateData` (écran des personnages illustres) et `GetModifierText` (Civilopédie), remplace ce texte par `LOC_BGD_IMHOTEP_EFFET` rempli avec les montants de la base × `CostMultiplier` de la vitesse (arrondi à l'entier inférieur) ; texte d'un autre mod laissé tel quel s'il ne porte plus 175 ; test hors jeu : `lua proxy/tests/test_ui_imhotep.lua` | — |

- **Journal :** `ui : …<fichier> : <correctif>, compilation réussie`, ou `laissé tel quel : <raison>`
  si le texte attendu manque.
- **Tests :** `cargo test` patche les vrais fichiers du jeu, de BSM et de MPH ; les blocs passent
  `lua proxy/tests/check_havok.lua`.
- **À vérifier en jeu :** que `Input.GetGestureDisplayString` rend bien le texte de
  `LOC_OPTIONS_KEY_RETURN` (sinon Entrée serait coupée pour tous, la ligne du journal le dira) ; que
  `Plot:GetArea()` existe côté interface (sinon repli sur le continent).

## Limites du prototype

- **Un seul build du jeu.** Les adresses sont celles de GameCore_XP2 FinalRelease
  (TimeDateStamp `0x667c6f5b`). Une mise à jour désactive le proxy, qui le note dans le journal ;
  il faudrait alors retrouver les sites par motif d'octets.
- **Multijoueur.** Testé avec deux joueurs seulement, le proxy étant chez l'hôte. Le cas où seul
  l'invité a le proxy n'a pas été essayé.
- **Un seul thread suivi.** Si un instantané était demandé depuis un autre thread pendant la
  cascade, il serait exécuté normalement.
- **Interface momentanément en retard.** Pendant la cascade, les infobulles de villes ne sont plus
  rafraîchies au fil de l'eau, mais une fois à la fin.
- **Couverture partielle.** Seuls les instantanés de villes et de diplomatie sont regroupés. Les
  autres gestionnaires de cache, plus petits, ne le sont pas.
- **Portée `message` : interface en retard d'une passe.** Les écouteurs Lua d'un événement émis
  pendant la passe voient encore l'ancien instantané de la ville ou de la diplomatie, jusqu'à la fin
  de la passe.
