#!/usr/bin/env node
// Tâche 5.1 — génération de clé et signature du manifeste du moteur.
//
// ## Pourquoi Node, et pas un outil de plus
//
// Le binaire ne peut que **vérifier** : une clé privée dans le dépôt serait
// le défaut le plus grave possible ici. Il faut donc un outil de signature qui
// vit **hors** du dépôt. Node est déjà là (le projet en dépend), et son
// `crypto` fait Ed25519 nativement — mesuré le 2026-10-05 : clé publique brute
// de 32 octets (les 32 derniers du DER SPKI), signature de 64 octets, rejet d'un
// message falsifié, et une clé privée relue d'un PEM redonne **la même**
// signature. Aucune dépendance ajoutée.
//
// ## Le format signé
//
// La signature ne porte **pas** sur le JSON : elle porte sur un texte
// canonique, ligne par ligne. Le JSON a des espaces libres, un ordre de clés
// libre et des échappements — signer « le JSON » demanderait deux implémentations
// d'une canonicalisation, et elles divergeraient. Un format texte n'a aucune
// ambiguïté : c'est exactement le contenu du fichier.
//
// Rust reconstruit ces mêmes lignes à partir du JSON qu'il a parsé
// (`moteur::charge_signee`), donc les deux côtés ne peuvent pas diverger sans
// que la vérification échoue.
//
// ## Commandes
//
//   node scripts/moteur-sign.mjs cles      # génère la paire, une seule fois
//   node scripts/moteur-sign.mjs signer <manifeste.json>
//   node scripts/moteur-sign.mjs verifier <manifeste.json>
//
// La clé privée va dans `%USERPROFILE%\.tatami\cle-moteur.pem` — **hors du
// dépôt**. La clé publique va dans le dépôt, parce qu'elle doit être embarquée
// dans le binaire.

// Fichier `.mjs` : Node le charge en **module ES**, donc `require` et
// `__dirname` n'existent pas. Le reste du dépôt (`check-i18n.mjs`) est en ESM
// aussi ; le format est donc cohérent, pas un accident.
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const RACINE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const CLE_PUBLIQUE = path.join(RACINE, 'apps/desktop/src-tauri/keys/cle-moteur.pub');
const CLE_PRIVEE = path.join(os.homedir(), '.tatami', 'cle-moteur.pem');

/** La première ligne de garde : sans elle, un format pourrait changer un jour et
 *  les deux côtés signeraient des choses différentes sans s'en apercevoir. */
const DOMAINE = 'tatami:moteur:manifeste:1';

/** Les champs couverts par la signature, dans un ordre **fixe**.
 *
 *  L'ordre est écrit deux fois (ici et dans `moteur.rs`) : c'est le contrat.
 *  Ajouter un champ sans l'ajouter ici ne le protège pas — et un test de
 *  contrat dans `moteur.rs` compare les deux listes pour que l'oubli soit rouge.
 */
const CHAMPS_SIGNES = ['version', 'engine', 'parsers', 'archive', 'sha256'];

/** Le texte canonique signé. Doit être identique à `moteur::charge_signee`. */
function charge_signee(manifeste) {
  const lignes = [DOMAINE];
  for (const champ of CHAMPS_SIGNES) {
    const valeur = manifeste[champ];
    if (typeof valeur !== 'string' || valeur.trim() === '') {
      throw new Error(`champ manquant ou vide dans le manifeste : ${champ}`);
    }
    lignes.push(`${champ}=${valeur.trim()}`);
  }
  // Chaque ligne est terminée par \n, la dernière comprise : sans cela, deux
  // charges differing seulement par une ligne finale se confondraient.
  return lignes.join('\n') + '\n';
}

function lire_cle_privee() {
  if (!fs.existsSync(CLE_PRIVEE)) {
    throw new Error(
      `clé privée absente : ${CLE_PRIVEE}\n` +
        `Elle ne vit jamais dans le dépôt. Si vous l'avez perdue, générez une nouvelle\n` +
        `paire et reconstruisez l'application : les manifestes déjà signés avec\n` +
        `l'ancienne clé seront refusés, et c'est le comportement voulu.`,
    );
  }
  return crypto.createPrivateKey(fs.readFileSync(CLE_PRIVEE, 'utf8'));
}

/** `cles` : génère la paire. Refuse d'écraser une clé existante. */
function generer() {
  if (fs.existsSync(CLE_PRIVEE)) {
    console.log(`clé privée déjà présente : ${CLE_PRIVEE}`);
    console.log('rien n a été écrasé. Supprimez le fichier à la main si vous voulez repartir de zéro.');
    return;
  }
  const { publicKey, privateKey } = crypto.generateKeyPairSync('ed25519');

  const spki = publicKey.export({ type: 'spki', format: 'der' });
  const brut = spki.subarray(spki.length - 32);
  if (brut.length !== 32) {
    throw new Error(`clé publique inattendue : ${brut.length} octets au lieu de 32`);
  }

  fs.mkdirSync(path.dirname(CLE_PRIVEE), { recursive: true });
  // 0o600 : la clé privée n'est pas un secret partagé, c'est LA clé.
  fs.writeFileSync(CLE_PRIVEE, privateKey.export({ type: 'pkcs8', format: 'pem' }), {
    mode: 0o600,
  });

  fs.mkdirSync(path.dirname(CLE_PUBLIQUE), { recursive: true });
  fs.writeFileSync(CLE_PUBLIQUE, brut);

  console.log(`clé privée : ${CLE_PRIVEE}   (hors dépôt)`);
  console.log(`clé publique : ${CLE_PUBLIQUE}   (dans le dépôt, embarquée dans le binaire)`);
  console.log('');
  console.log('SAUVEGARDEZ LA CLÉ PRIVÉE. Sans elle, plus aucune mise à jour du moteur');
  console.log('ne pourra être signée. Elle n’est ni versionnée ni reproductible.');
}

/** `signer` : ajoute le champ `signature` au manifeste, sur place. */
function signer(chemin) {
  const manifeste = JSON.parse(fs.readFileSync(chemin, 'utf8'));
  const charge = Buffer.from(charge_signee(manifeste), 'utf8');
  const signature = crypto.sign(null, charge, lire_cle_privee());

  const signe = { ...manifeste, signature: signature.toString('base64') };
  fs.writeFileSync(chemin, JSON.stringify(signe, null, 2) + '\n', 'utf8');

  console.log(`manifeste signé : ${chemin}`);
  console.log('charge signée :');
  process.stdout.write(
    charge
      .toString('utf8')
      .split('\n')
      .filter(Boolean)
      .map((l) => `  ${l}`)
      .join('\n') + '\n',
  );
}

/** `verifier` : contrôle hors application, avec la clé publique embarquée. */
function verifier(chemin) {
  const brut = fs.readFileSync(CLE_PUBLIQUE);
  if (brut.length !== 32) {
    throw new Error(`clé publique illisible : ${brut.length} octets au lieu de 32`);
  }
  // Node veut un clé SPKI DER, et le dépôt ne contient que les 32 octets bruts
  // (ce qu'embarque `include_bytes!` côté Rust). On reconstruit donc l'en-tête
  // SPKI d'Ed25519, qui est **fixe** : `SEQUENCE { SEQUENCE { OID 1.3.101.112 },
  // BIT STRING 32 octets }`.
  const EN_TETE_SPKI_ED25519 = Buffer.from('302a300506032b6570032100', 'hex');
  const der = Buffer.concat([EN_TETE_SPKI_ED25519, brut]);
  const cle = crypto.createPublicKey({ key: der, format: 'der', type: 'spki' });

  const manifeste = JSON.parse(fs.readFileSync(chemin, 'utf8'));
  if (typeof manifeste.signature !== 'string' || manifeste.signature.trim() === '') {
    console.log('MANIFESTE NON SIGNE : champ `signature` absent');
    process.exit(1);
  }
  const signature = Buffer.from(manifeste.signature, 'base64');
  const charge = Buffer.from(charge_signee(manifeste), 'utf8');
  const ok = crypto.verify(null, charge, cle, signature);
  console.log(ok ? 'signature valide' : 'SIGNATURE INVALIDE');
  process.exit(ok ? 0 : 1);
}

const [commande, argument] = process.argv.slice(2);
try {
  if (commande === 'cles') generer();
  else if (commande === 'signer') {
    if (!argument) throw new Error('usage : moteur-sign.mjs signer <manifeste.json>');
    signer(argument);
  } else if (commande === 'verifier') {
    if (!argument) throw new Error('usage : moteur-sign.mjs verifier <manifeste.json>');
    verifier(argument);
  } else {
    console.log('commandes : cles | signer <manifeste.json> | verifier <manifeste.json>');
    process.exit(2);
  }
} catch (e) {
  console.error('ÉCHEC : ' + e.message);
  process.exit(1);
}
