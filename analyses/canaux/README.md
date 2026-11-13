# Canaux sur colline

Analyse statique (2026-11-13) de `GameCore_XP2_FinalRelease.dll`, en vue d'un **correctif POL** :
laisser la base de règles décider si un canal peut être bâti sur une colline. Le correctif est dans
le proxy ([proxy/src/canaux.rs](../../proxy/src/canaux.rs)).

Sites trouvés par Hotaru Snowy (« Allow Canals on Hills - dll version », Workshop `3663157965`,
[github.com/Hotaru-Snowy/CanalOnHills](https://github.com/Hotaru-Snowy/CanalOnHills), AGPL-3.0),
qui remplace GameCore par une DLL dont ces deux sauts sont neutralisés. Aucun code repris : les
adresses sont des faits, revérifiées ici sur la DLL installée.

## Le refus en double

Le placement du quartier canal est jugé deux fois :

1. par la base de règles : `District_ValidTerrains`. Le jeu de base (`Expansion2_Districts.xml`) n'y
   met que les cinq terrains plats (`TERRAIN_GRASS`, `_PLAINS`, `_DESERT`, `_TUNDRA`, `_SNOW`) ;
2. par GameCore, en dur, quelle que soit la base de règles.

Un mod qui ajoute les collines à `District_ValidTerrains` charge sans erreur, les lignes sont bien
dans la base en partie, et une colline reste refusée (essayé avec Better Balanced Game le
2026-08-31).

## Le test en dur

| site | contexte | case | suite |
|---|---|---|---|
| `180362eb6` | placement du quartier canal, juste après le refus des montagnes (`bt` sur le masque `0x4924`) | `mov rcx,rsi` | `test al,al ; jne 180363169` (refus) |
| `1803639c2` | canal de Panama, après un premier test `call 180060820` | `mov rcx,r12` | `test al,al ; jne 180363fe1` (refus) |

`180060220` est la colline d'une case : c'est elle qu'appelle le `Plot:IsHills()` du Lua
(`18001e0b0`, `Lua_IMapStartPositioner.cpp`). Elle lit le terrain de la case (`case+0x2c`) puis son
drapeau de colline dans la table des terrains.

## Le correctif

Le `call 180060220` de chaque site est redirigé vers un relais `jmp [rip+0]`. La case reste dans
`rcx`, la fonction du proxy appelle l'originale et ne change la réponse que pour une colline :

- **quartier canal** : « pas une colline » dès que les correctifs POL sont actifs. Le terrain est
  alors jugé par `District_ValidTerrains` seul. Sans mod, rien ne change : les collines n'y sont pas.
- **canal de Panama** : il n'a pas de table de terrains (`Building_ValidTerrains` est vide pour
  `BUILDING_PANAMA_CANAL`), ce test est donc son seul garde. La colline ne compte plus que si les
  correctifs POL sont actifs **et** que l'hôte a coché l'option de partie `BBG_CANALS_ON_HILLS` de
  Better Balanced Game (« Canals on Hills »). L'option est relue à chaque partie par un bloc Lua
  ajouté à `WorldViewPlotMessages.lua` (contexte toujours chargé, que ni le proxy ni les mods
  courants ne remplacent), qui appelle `BGD_CanauxCollines(0|1)`.

Ce qui reste au jeu : la géométrie du canal (deux étendues d'eau, ou une et un centre-ville ; une case
de terre entière de chaque côté ; virage de 60° au plus ; pas de jonction à trois), qui dépend du
drapeau `Districts_XP2.Canal` et non d'une table.

## Essais en partie (2026-11-13)

Partie solo (correctifs POL actifs), Better Balanced Game avec l'option « Canals on Hills ». Case
de test : une case plate où un canal est légal (vérifié d'abord), passée en `TERRAIN_PLAINS_HILLS`,
seule variable changée. Légalité lue par `CityManager`/`play can-build`.

| option `BBG_CANALS_ON_HILLS` | collines dans `District_ValidTerrains` | case plate | même case en colline |
|---|---|---|---|
| cochée | oui (10 terrains) | légale | **légale** |
| décochée | non (5 terrains plats) | légale | **refusée** |

La seconde ligne est l'essai qui compte : le test en dur est contourné (le journal compte les
collines acceptées), et la base de règles seule refuse la colline. Une liste de terrains non vide
reste donc une liste blanche.

Le journal montre aussi le test du canal de Panama consulté et gardé par l'option. Le placement d'un
canal de Panama sur colline n'a pas encore été essayé de bout en bout (plateau de test difficile à
construire à la main).

## Reste à vérifier

- Canal de Panama sur colline, de bout en bout : refusé sans l'option, accepté avec.
- L'IA place-t-elle des canaux sur colline ?
