package org.tatami.engine

import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * La garde qui distingue une image d'une réponse **embrouillée**.
 *
 * Elle décide si `fetchImage` garde le résultat du chemin rapide (hors verrou)
 * ou s'il refait le téléchargement sous le verrou. Un faux négatif coûte une
 * requête, un faux positif laisse passer une page illisible : c'est donc le
 * refus des octets embrouillés qui compte, et l'acceptation des formats réellement
 * servis par les sources.
 */
class SignatureImageTest {

    private fun octets(vararg valeurs: Int) = ByteArray(valeurs.size) { valeurs[it].toByte() }

    /** La signature suivie de huit zéros : la garde ne doit pas dépendre de la suite. */
    private fun signe(vararg valeurs: Int) = ByteArray(valeurs.size + 8) { i ->
        if (i < valeurs.size) valeurs[i].toByte() else 0
    }

    @Test
    fun `les formats servis par les sources mesurees sont reconnus`() {
        assertTrue(
            estImageProbable(signe(0xFF, 0xD8, 0xFF, 0xE0)),
            "JPEG — HARMONYSCAN et les pages du balayage FR du 2026-10-17",
        )
        assertTrue(
            estImageProbable(signe(0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A)),
            "PNG — FURYOSOCIETY",
        )
        assertTrue(
            estImageProbable(signe(0x52, 0x49, 0x46, 0x46, 0x10, 0, 0, 0, 0x57, 0x45, 0x42, 0x50, 0x56, 0x50)),
            "WEBP — SUSHISCAN, LELSCANVF",
        )
        assertTrue(
            estImageProbable(signe(0x47, 0x49, 0x46, 0x38, 0x39, 0x61)),
            "GIF",
        )
    }

    @Test
    fun `l entete embrouille mesure sur ZENMANGA est refuse`() {
        // Mesuré le 2026-10-05 : 832 513 octets, `AA A1 AC 8B 70 20 08 3C 19 2E
        // 77 6D 77 50 32 57`, identiques sur deux pages différentes — donc ce
        // n'est pas une image, c'est du chiffrement.
        val embrouille = octets(0xAA, 0xA1, 0xAC, 0x8B, 0x70, 0x20, 0x08, 0x3C, 0x19, 0x2E, 0x77, 0x6D)
        assertFalse(estImageProbable(embrouille), "des octets chiffrés ne doivent pas passer pour une image")
    }

    @Test
    fun `un interstitiel HTML et un corps vide sont refuses`() {
        // Mesuré le 2026-10-14 : une source a servi `200 OK` avec la page HTML de
        // son interstitiel de vérification d'âge — 45 URL d'images « servies »
        // qui étaient ce même HTML.
        assertFalse(
            estImageProbable("<!DOCTYPE html><html><body>verification</body></html>".toByteArray()),
            "HTML",
        )
        assertFalse(estImageProbable(ByteArray(0)), "corps vide")
    }

    @Test
    fun `un SVG est reconnu malgre l absence de signature binaire`() {
        assertTrue(
            estImageProbable("<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\"/>".toByteArray()),
            "un SVG est du texte, pas un format à signature",
        )
    }
}
