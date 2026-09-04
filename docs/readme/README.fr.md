<div align="center">

# IMDb Tech Manager

[简体中文](../../README.md) | [繁體中文](./README.zh-Hant.md) | [English](./README.en.md) | **Français** | [Русский](./README.ru.md) | [日本語](./README.ja.md) | [Español](./README.es.md) | [ไทย](./README.th.md)

[![Release](https://img.shields.io/github/v/release/Eric-Hou1997/IMDb-Tech-Manager?label=release)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)
[![Downloads](https://img.shields.io/github/downloads/Eric-Hou1997/IMDb-Tech-Manager/total?label=downloads)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)

<img src="../../macos/assets/ITM_logo.png" alt="IMDb Tech Manager" width="220">

Gestion des spécifications techniques et des métadonnées pour les médiathèques.

</div>

## Présentation

IMDb Tech Manager (ITM) récupère et structure les IMDb Technical Specifications. Les caméras, objectifs, formats de prise de vues, formats audio, rapports d’image et procédés de production peuvent être enregistrés de façon sûre dans les fichiers NFO. L’application propose aussi un inspecteur, une prévisualisation, l’annulation, des tâches par lot et la gestion des tags techniques par règles locales ou par IA.

ITM peut être associé à [Tech Card Manager (TCM)](https://github.com/Eric-Hou1997/Tech-Card-Manager) :

```text
IMDb → ITM → NFO / Technical Specifications → TCM → affichage dans Emby
```

## Version et plateforme

- Version stable : [`v4.1.0`](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases/tag/v4.1.0)
- Plateforme : macOS 12 ou version ultérieure, Apple Silicon (arm64)
- Archive : `ITM-v4.1.0-MacOS-AArch64-APP.zip`
- Nom stable dans l’archive : `IMDb Tech Manager.app`
- La Release comprend le SHA-256, la signature OTA Ed25519, les instructions et le changelog

## Fonctions principales

- Acquisition, cache et structuration des IMDb Technical Specifications
- Écriture NFO limitée de façon sûre à `<technicalspecs>`
- Inspecteur, prévisualisation, contrôle du hash source, sauvegarde et annulation
- Propriété distincte pour les tags techniques External, Generated et Manual
- Recherche, filtres, sélection et tâches séparés pour films et séries
- Fournisseurs IA OpenAI-compatible et Anthropic remplaçables
- Comptage réel des requêtes HTTP, tokens, caches et coûts
- Mise à jour OTA distinguant proxy, limitations GitHub, ressource absente, téléchargement et signature

## Langues

Le chinois simplifié, le chinois traditionnel et l’anglais (États-Unis) sont intégrés. Le français, le russe, le japonais, l’espagnol et le thaï sont distribués sous forme de packs séparés dans la Release `v4.1.0` et ne sont chargés qu’après téléchargement et vérification.

L’interface Web, le Core Go, le moteur Python et les menus macOS partagent la même langue. La langue des journaux et des explications de révision est figée au démarrage d’une tâche. Les anciens journaux, caches, NFO, données de propriété et prompts ne sont jamais réécrits lors d’un changement de langue.

## Limites de sécurité

- Le Spec Agent ne peut modifier que `<technicalspecs>` et jamais les `<tag>` racine.
- Les opérations automatiques ne touchent que les Generated Tech Tags dont la propriété est prouvée.
- Les tags External, Manual, TMM ou appartenant à une autre application sont conservés.
- XML, BOM UTF-8, fins de ligne, permissions, sauvegardes et remplacement atomique sont préservés ou vérifiés.
- Un chemin ambigu, une propriété conflictuelle ou un fichier modifié après prévisualisation provoque un arrêt sûr.

## Aperçu

![Gestion des données](../images/data-management.png)

![Gestion des tags](../images/tag-management.png)

## Feuille de route

Terminé : publication Apple Silicon, sécurité et propriété NFO, tags Local/IA, gestion des tâches et de l’usage, interface native trilingue, cinq packs téléchargeables, mise à jour liée à la version et contrôles de régression.

En cours : normalisation des Technical Specifications, davantage de tests sur de vraies médiathèques, compatibilité des fournisseurs IA, récupération après erreur, signature Developer ID et notarization, ainsi que l’intégration d’autres plateformes et serveurs multimédias.

## Développement et licence

Lisez [`AGENTS.md`](../../AGENTS.md) avant de contribuer. L’architecture des packs est décrite dans [`docs/language-packs.md`](../language-packs.md).

Projet sous [Apache License 2.0](../../LICENSE). IMDb, Emby et les autres marques appartiennent à leurs propriétaires respectifs. Ce projet n’est ni affilié, ni autorisé, ni approuvé par IMDb.com, Inc. ou Emby LLC.
