package org.tatami.engine

/**
 * Les octets reçus ressemblent-ils à une image ?
 *
 * Sert de **garde-fou** au téléchargement hors verrou de `fetchImage` : depuis le
 * 2026-10-18, le parser est posé sur le fil (`ParserActif`) sans que le verrou de
 * source soit tenu, et c'est la **pose** qui débrouille la réponse — pas le
 * verrou. Le moteur doit donc pouvoir reconnaître le cas où le débrouillage n'a
 * pas eu lieu, pour refaire le téléchargement comme avant.
 *
 * Mesure qui fonde la fonction, le 2026-10-05 sur ZENMANGA : la réponse
 * **embrouillée** commence par `AA A1 AC 8B 70 20 08 3C` — identique sur deux
 * pages différentes — et ce n'est ni un JPEG, ni un PNG, ni un WEBP, ni du HTML.
 * Les mêmes octets passés par le parser commencent par `FF D8 FF E0`.
 *
 * La liste est **volontairement large** : un faux positif ne coûte rien (on ne
 * refait pas le téléchargement), un faux négatif coûte une seconde requête.
 */
internal fun estImageProbable(octets: ByteArray): Boolean {
    if (octets.size < 8) return false
    return debutePar(octets, 0, 0xFF, 0xD8, 0xFF) || // JPEG
        debutePar(octets, 0, 0x89, 0x50, 0x4E, 0x47) || // PNG
        debutePar(octets, 0, 0x47, 0x49, 0x46, 0x38) || // GIF (« GIF8 »)
        (
            debutePar(octets, 0, 0x52, 0x49, 0x46, 0x46) &&
                debutePar(octets, 8, 0x57, 0x45, 0x42, 0x50)
        ) || // RIFF….WEBP
        debutePar(octets, 0, 0x42, 0x4D) || // BMP
        debutePar(octets, 0, 0x49, 0x49, 0x2A, 0x00) || // TIFF, boutisme petit
        debutePar(octets, 0, 0x4D, 0x4D, 0x00, 0x2A) || // TIFF, boutisme grand
        debutePar(octets, 0, 0x00, 0x00, 0x01, 0x00) || // ICO
        debutePar(octets, 0, 0xFF, 0x0A) || // JPEG XL (flux brut)
        debutePar(octets, 4, 0x66, 0x74, 0x79, 0x70) || // AVIF / HEIF (`ftyp`)
        estTexteSvg(octets)
}

/**
 * Un SVG servi en `image/svg+xml` est du **texte** : ni signature binaire, ni
 * hasard. Il commence par la déclaration XML, un commentaire ou la balise
 * elle-même — parfois après un BOM UTF-8.
 */
private fun estTexteSvg(octets: ByteArray): Boolean {
    val debut = String(octets, 0, minOf(64, octets.size), Charsets.UTF_8)
        .removePrefix("\uFEFF")
        .trimStart()
    return debut.startsWith("<?xml") || debut.startsWith("<svg") || debut.startsWith("<!--")
}

private fun debutePar(octets: ByteArray, decalage: Int, vararg attendus: Int): Boolean =
    octets.size >= decalage + attendus.size &&
        attendus.withIndex().all { (i, v) -> (octets[decalage + i].toInt() and 0xFF) == v }
