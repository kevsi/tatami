package org.tatami.engine

/**
 * Plafond de lecture d'une réponse d'image.
 *
 * Mesuré le 2026-10-09 (audit) : `fetch_octets` (`Server.kt:303`) et
 * `octetsImage` (`:712`) faisaient `body.bytes()` **sans plafond**, alors que
 * `HttpBridge` en impose un de 5 Mo (`HttpBridge.kt:25`, `:66-69`). La même
 * famille de code avait donc deux disciplines.
 *
 * Conséquence, chiffrée : `LimiteDeConcurrence` autorise 4 requêtes simultanées
 * (`LimiteDeConcurrence.kt:84`), donc une source qui sert un « JPEG » de
 * 500 Mo faisait monter **quatre lectures de 500 Mo en RAM** dans le sidecar.
 * Un JVM qui gonte puis meurt ne laisse aucune trace utile — et le moteur est un
 * sidecar : sa mort est lue comme « le moteur ne répond plus », jamais comme
 * « une réponse trop grosse ».
 *
 * La borne n'est PAS celle de `HttpBridge` : les deux ne protègent pas la même
 * chose. 5 Mo suffisent à une page HTML, une page de manga fait couramment
 * 1 à 2 Mo et certaines planches dépassent 4 Mo. On prend donc une valeur qui
 * couvre le cas réel avec de la marge.
 */
const val MAX_OCTETS_IMAGE: Long = 32L * 1024 * 1024 // 32 Mo

/**
 * Lit un corps d'image avec un plafond.
 *
 * Le plafond est un paramètre, et non une constante interne, pour une raison
 * précise : **prouver** la borne exige de la dépasser, et un test qui alloue
 * 33 Mo pour éprouver un plafond de 32 Mo est à la fois lent et fragile. Avec
 * un paramètre, le test passe un plafond de quelques octets.
 */
internal fun lireOctetsBornes(resp: okhttp3.Response, quoi: String, plafond: Long): ByteArray {
    val body = resp.body ?: throw IllegalStateException("empty $quoi")
    // Un octet de plus que le plafond : c'est ce surnombre qui décide, pas la
    // taille annoncée. Un `Content-Length` est une déclaration du serveur, pas
    // une garantie — s'y fier reviendrait à faire confiance à celui qu'on borne.
    val octets = body.byteStream().readNBytes(plafond.toInt() + 1)
    if (octets.size > plafond) {
        throw IllegalStateException("$quoi depasse $plafond octets")
    }
    return octets
}
